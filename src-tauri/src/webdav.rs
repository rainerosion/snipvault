use crate::db::{self, Snippet};
use crate::settings;
use reqwest::header::WWW_AUTHENTICATE;
use std::borrow::Cow;

#[derive(Debug, serde::Serialize)]
pub struct SyncResult {
    pub success: bool,
    pub message: String,
    pub uploaded: bool,
    pub uploaded_count: usize,
    pub downloaded_count: usize,
    pub total_count: usize,
}

impl Default for SyncResult {
    fn default() -> Self {
        Self {
            success: false,
            message: String::new(),
            uploaded: false,
            uploaded_count: 0,
            downloaded_count: 0,
            total_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebDavAuthMode {
    Auto,
    Basic,
    Digest,
    Bearer,
    None,
}

impl WebDavAuthMode {
    fn from_settings(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "auto" => Self::Auto,
            "digest" => Self::Digest,
            "bearer" => Self::Bearer,
            "none" => Self::None,
            _ => Self::Basic,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Basic => "basic",
            Self::Digest => "digest",
            Self::Bearer => "bearer",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone)]
struct WebDavAuth {
    mode: WebDavAuthMode,
    username: String,
    password: String,
}

impl WebDavAuth {
    fn from_settings(mode: &str, username: &str, password: &str) -> Self {
        Self {
            mode: WebDavAuthMode::from_settings(mode),
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    fn bearer_token(&self) -> Option<String> {
        let token = if !self.password.trim().is_empty() {
            self.password.trim()
        } else {
            self.username.trim()
        };

        if token.is_empty() {
            None
        } else {
            Some(token.to_string())
        }
    }
}

fn make_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败: {e}"))
}

fn base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

fn manifest_path() -> &'static str {
    "snipvault/manifest.json"
}

fn snippet_path(id: &str) -> String {
    format!("snipvault/{}.json", id)
}

fn snippets_collection_path() -> &'static str {
    "snipvault"
}

fn digest_request_uri(url: &str) -> String {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        let mut uri = parsed.path().to_string();
        if uri.is_empty() {
            uri.push('/');
        }
        if let Some(query) = parsed.query() {
            uri.push('?');
            uri.push_str(query);
        }
        uri
    } else {
        "/".to_string()
    }
}

fn find_digest_challenge(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all(WWW_AUTHENTICATE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.to_ascii_lowercase().contains("digest"))
        .map(|value| value.to_string())
}

fn send_with_digest(
    client: &reqwest::blocking::Client,
    method: reqwest::Method,
    url: &str,
    body: Option<String>,
    content_type: Option<&str>,
    username: &str,
    password: &str,
    fallback_to_basic: bool,
) -> Result<reqwest::blocking::Response, String> {
    let build = || {
        let mut req = client.request(method.clone(), url);
        if let Some(content_type) = content_type {
            req = req.header("Content-Type", content_type);
        }
        if let Some(payload) = &body {
            req = req.body(payload.clone());
        }
        req
    };

    let send_basic = || {
        let req = if username.trim().is_empty() {
            build()
        } else {
            build().basic_auth(username, Some(password))
        };
        req.send().map_err(|e| format!("请求失败: {e}"))
    };

    let first_response = build().send().map_err(|e| format!("请求失败: {e}"))?;
    if first_response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(first_response);
    }

    if username.trim().is_empty() {
        return Ok(first_response);
    }

    let Some(challenge) = find_digest_challenge(first_response.headers()) else {
        if fallback_to_basic {
            return send_basic();
        }
        return Ok(first_response);
    };

    let mut challenge_header = match digest_auth::parse(&challenge) {
        Ok(parsed) => parsed,
        Err(e) => {
            if fallback_to_basic {
                return send_basic();
            }
            return Err(format!("Digest 认证解析失败: {e}"));
        }
    };

    let method_for_digest = digest_auth::HttpMethod(Cow::Owned(method.as_str().to_string()));
    let context = digest_auth::AuthContext::new_with_method(
        username.to_string(),
        password.to_string(),
        digest_request_uri(url),
        body.as_ref().map(|payload| payload.as_bytes().to_vec()),
        method_for_digest,
    );

    let digest_header = match challenge_header.respond(&context) {
        Ok(header) => header.to_header_string(),
        Err(e) => {
            if fallback_to_basic {
                return send_basic();
            }
            return Err(format!("Digest 认证握手失败: {e}"));
        }
    };

    let digest_response = build()
        .header(reqwest::header::AUTHORIZATION, digest_header)
        .send()
        .map_err(|e| format!("请求失败: {e}"))?;

    if fallback_to_basic && digest_response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return send_basic();
    }

    Ok(digest_response)
}

fn send_authed(
    client: &reqwest::blocking::Client,
    method: reqwest::Method,
    url: &str,
    body: Option<String>,
    content_type: Option<&str>,
    auth: &WebDavAuth,
) -> Result<reqwest::blocking::Response, String> {
    let build = || {
        let mut req = client.request(method.clone(), url);
        if let Some(content_type) = content_type {
            req = req.header("Content-Type", content_type);
        }
        if let Some(payload) = &body {
            req = req.body(payload.clone());
        }
        req
    };

    match auth.mode {
        WebDavAuthMode::None => build().send().map_err(|e| format!("请求失败: {e}")),
        WebDavAuthMode::Basic => {
            if auth.username.trim().is_empty() {
                build().send().map_err(|e| format!("请求失败: {e}"))
            } else {
                build()
                    .basic_auth(&auth.username, Some(&auth.password))
                    .send()
                    .map_err(|e| format!("请求失败: {e}"))
            }
        }
        WebDavAuthMode::Digest => send_with_digest(
            client,
            method,
            url,
            body,
            content_type,
            &auth.username,
            &auth.password,
            false,
        ),
        WebDavAuthMode::Auto => send_with_digest(
            client,
            method,
            url,
            body,
            content_type,
            &auth.username,
            &auth.password,
            true,
        ),
        WebDavAuthMode::Bearer => {
            if let Some(token) = auth.bearer_token() {
                build()
                    .bearer_auth(token)
                    .send()
                    .map_err(|e| format!("请求失败: {e}"))
            } else {
                build().send().map_err(|e| format!("请求失败: {e}"))
            }
        }
    }
}

fn format_http_error(op: &str, status: reqwest::StatusCode, url: &str, body: String) -> String {
    let mut message = format!("{}失败 (HTTP {}): {}", op, status, body);

    if matches!(status.as_u16(), 401 | 403) {
        message.push_str(&format!(" | URL: {}", url));
        message.push_str(" | 可能原因: WebDAV 地址不是可写目录，或当前账号对该目录无写权限");
    }

    message
}

fn ensure_snippets_collection(
    client: &reqwest::blocking::Client,
    base: &str,
    auth: &WebDavAuth,
) -> Result<(), String> {
    let collection_url = format!("{}/{}/", base, snippets_collection_path());

    let resp = send_authed(
        client,
        reqwest::Method::from_bytes(b"MKCOL").expect("valid MKCOL"),
        &collection_url,
        None,
        None,
        auth,
    )
    .map_err(|e| format!("创建远端目录失败: {e}"))?;

    let status = resp.status();
    if status.is_success() || matches!(status.as_u16(), 405 | 409) {
        return Ok(());
    }

    Err(format_http_error(
        "创建远端目录",
        status,
        &collection_url,
        resp.text().unwrap_or_default(),
    ))
}

/// Per-snippet manifest entry
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SnippetMeta {
    pub id: String,
    pub updated_at: String,
}

/// Remote manifest
#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct Manifest {
    pub version: u64,
    pub snippets: Vec<SnippetMeta>,
}

/// Upload a single snippet JSON to WebDAV
fn upload_snippet(
    client: &reqwest::blocking::Client,
    base: &str,
    snippet: &Snippet,
    auth: &WebDavAuth,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(snippet).map_err(|e| format!("序列化失败: {e}"))?;
    let path = format!("{}/{}", base, snippet_path(&snippet.id));

    let resp = send_authed(
        client,
        reqwest::Method::PUT,
        &path,
        Some(json),
        Some("application/json"),
        auth,
    )
    .map_err(|e| format!("上传失败: {e}"))?;

    let status = resp.status();
    if !status.is_success() && status.as_u16() != 201 && status.as_u16() != 204 {
        return Err(format_http_error(
            "上传",
            status,
            &path,
            resp.text().unwrap_or_default(),
        ));
    }
    Ok(())
}

/// Download a single snippet from WebDAV
fn download_snippet(
    client: &reqwest::blocking::Client,
    base: &str,
    id: &str,
    auth: &WebDavAuth,
) -> Result<Option<Snippet>, String> {
    let path = format!("{}/{}", base, snippet_path(id));
    let resp = send_authed(client, reqwest::Method::GET, &path, None, None, auth)
        .map_err(|e| format!("下载失败: {e}"))?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!(
            "下载失败 (HTTP {}): {}",
            resp.status(),
            resp.text().unwrap_or_default()
        ));
    }
    let text = resp.text().map_err(|e| format!("读取响应失败: {e}"))?;
    let snippet: Snippet = serde_json::from_str(&text).map_err(|e| format!("JSON 解析失败: {e}"))?;
    Ok(Some(snippet))
}

/// Upload manifest to WebDAV
fn upload_manifest(
    client: &reqwest::blocking::Client,
    base: &str,
    manifest: &Manifest,
    auth: &WebDavAuth,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest).map_err(|e| format!("序列化失败: {e}"))?;
    let path = format!("{}/{}", base, manifest_path());

    let resp = send_authed(
        client,
        reqwest::Method::PUT,
        &path,
        Some(json),
        Some("application/json"),
        auth,
    )
    .map_err(|e| format!("上传清单失败: {e}"))?;

    let status = resp.status();
    if !status.is_success() && status.as_u16() != 201 && status.as_u16() != 204 {
        return Err(format_http_error(
            "上传清单",
            status,
            &path,
            resp.text().unwrap_or_default(),
        ));
    }
    Ok(())
}

/// Download manifest from WebDAV
fn download_manifest(
    client: &reqwest::blocking::Client,
    base: &str,
    auth: &WebDavAuth,
) -> Result<Option<Manifest>, String> {
    let path = format!("{}/{}", base, manifest_path());
    let resp = send_authed(client, reqwest::Method::GET, &path, None, None, auth)
        .map_err(|e| format!("下载清单失败: {e}"))?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!(
            "下载清单失败 (HTTP {}): {}",
            resp.status(),
            resp.text().unwrap_or_default()
        ));
    }
    let text = resp.text().map_err(|e| format!("读取响应失败: {e}"))?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|e| format!("清单 JSON 解析失败: {e}"))?;
    Ok(Some(manifest))
}

/// Delete a snippet file from WebDAV
fn delete_snippet(
    client: &reqwest::blocking::Client,
    base: &str,
    id: &str,
    auth: &WebDavAuth,
) -> Result<(), String> {
    let path = format!("{}/{}", base, snippet_path(id));
    let resp = send_authed(client, reqwest::Method::DELETE, &path, None, None, auth)
        .map_err(|e| format!("删除失败: {e}"))?;

    let status = resp.status();
    if !status.is_success() && status.as_u16() != 204 && status.as_u16() != 404 {
        return Err(format!(
            "删除失败 (HTTP {}): {}",
            status,
            resp.text().unwrap_or_default()
        ));
    }
    Ok(())
}

/// Per-snippet two-way merge sync.
/// Each snippet lives in snipvault/{id}.json, tracked by snipvault/manifest.json.
pub fn sync_merge() -> Result<SyncResult, String> {
    let settings = settings::get_settings();
    if settings.webdav_url.is_empty() {
        return Ok(SyncResult {
            success: false,
            message: "WebDAV 地址未配置，请在设置中填写".into(),
            ..Default::default()
        });
    }

    let client = make_client(settings.webdav_timeout_secs)?;
    let base = base_url(&settings.webdav_url);
    let auth = WebDavAuth::from_settings(
        &settings.webdav_auth_mode,
        &settings.webdav_username,
        &settings.webdav_password,
    );

    log::info!(
        "sync_merge: base_url = {} | auth_mode = {}",
        base,
        auth.mode.as_str()
    );

    ensure_snippets_collection(&client, &base, &auth)
        .map_err(|e| format!("创建 snipvault 目录失败: {e}"))?;

    // Step 1: Get all local snippets
    let local_snippets = db::get_all_for_upload().map_err(|e| format!("读取本地数据失败: {e}"))?;
    let local_map: std::collections::HashMap<String, &Snippet> =
        local_snippets.iter().map(|s| (s.id.clone(), s)).collect();

    // Step 2: Download remote manifest (if exists)
    let remote_manifest =
        download_manifest(&client, &base, &auth).map_err(|e| format!("下载清单失败: {e}"))?;
    let remote_map: std::collections::HashMap<String, String> = remote_manifest
        .as_ref()
        .map(|m| {
            m.snippets
                .iter()
                .map(|s| (s.id.clone(), s.updated_at.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut uploaded = 0usize;
    let mut downloaded = 0usize;
    let mut deleted_remote = 0usize;

    // Step 3: Upload local snippets newer than remote (or not exist remotely)
    for snippet in &local_snippets {
        let remote_updated = remote_map.get(&snippet.id);
        let should_upload = remote_updated
            .map(|r| snippet.updated_at.as_str() > r.as_str())
            .unwrap_or(true);

        if should_upload {
            upload_snippet(&client, &base, snippet, &auth)
                .map_err(|e| format!("上传 {} 失败: {}", snippet.id, e))?;
            uploaded += 1;
        }
    }

    // Step 4: Download remote snippets newer than local (or not exist locally)
    if let Some(manifest) = &remote_manifest {
        for meta in &manifest.snippets {
            if let Some(local) = local_map.get(&meta.id) {
                // Both exist: download if remote is newer
                if meta.updated_at.as_str() > local.updated_at.as_str() {
                    if let Some(remote_snippet) =
                        download_snippet(&client, &base, &meta.id, &auth)?
                    {
                        let merge = db::merge_snippets(vec![remote_snippet])
                            .map_err(|e| format!("合并失败: {e}"))?;
                        downloaded += merge.downloaded;
                    }
                }
            } else {
                // Remote-only: download and insert
                if let Some(remote_snippet) = download_snippet(&client, &base, &meta.id, &auth)? {
                    let merge = db::merge_snippets(vec![remote_snippet])
                        .map_err(|e| format!("合并失败: {e}"))?;
                    downloaded += merge.downloaded;
                }
            }
        }

        // Step 5: Delete remote snippets that no longer exist locally
        for meta in &manifest.snippets {
            if !local_map.contains_key(&meta.id) {
                delete_snippet(&client, &base, &meta.id, &auth)?;
                deleted_remote += 1;
            }
        }
    }

    // Step 6: Upload new manifest
    let new_manifest = Manifest {
        version: 1,
        snippets: local_snippets
            .iter()
            .map(|s| SnippetMeta {
                id: s.id.clone(),
                updated_at: s.updated_at.clone(),
            })
            .collect(),
    };
    upload_manifest(&client, &base, &new_manifest, &auth)
        .map_err(|e| format!("上传清单失败: {e}"))?;

    let total = local_snippets.len();
    let now = chrono::Utc::now().to_rfc3339();

    let summary_message = if uploaded == 0 && downloaded == 0 && deleted_remote == 0 {
        format!("同步完成：本地与远程数据一致（当前共 {} 条）", total)
    } else {
        format!(
            "同步完成：上传 {} 条，下载 {} 条，远程删除 {} 条（当前共 {} 条）",
            uploaded, downloaded, deleted_remote, total
        )
    };

    // Record sync history
    db::record_sync_version("merge", total, uploaded, downloaded, &summary_message).ok();

    // Update last_sync_at
    settings::update_settings(|s| {
        s.last_sync_at = now.clone();
    })
    .ok();

    log::info!(
        "sync_merge: done uploaded={} downloaded={} deleted_remote={}",
        uploaded,
        downloaded,
        deleted_remote
    );

    Ok(SyncResult {
        success: true,
        message: summary_message,
        uploaded: true,
        uploaded_count: uploaded,
        downloaded_count: downloaded,
        total_count: total,
    })
}

pub fn sync_to_webdav() -> Result<SyncResult, String> {
    sync_merge()
}

pub fn sync_from_webdav() -> Result<SyncResult, String> {
    sync_merge()
}
