use crate::db::{self, Snippet};
use crate::settings;

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
    "snippets/manifest.json"
}

fn snippet_path(id: &str) -> String {
    format!("snippets/{}.json", id)
}

fn snippets_collection_path() -> &'static str {
    "snippets"
}

fn with_basic_auth(
    req: reqwest::blocking::RequestBuilder,
    user: &str,
    pass: &str,
) -> reqwest::blocking::RequestBuilder {
    if user.is_empty() {
        req
    } else {
        req.basic_auth(user, Some(pass))
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
    user: &str,
    pass: &str,
) -> Result<(), String> {
    let collection_url = format!("{}/{}/", base, snippets_collection_path());

    let req = client.request(reqwest::Method::from_bytes(b"MKCOL").expect("valid MKCOL"), &collection_url);
    let resp = with_basic_auth(req, user, pass)
        .send()
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
fn upload_snippet(client: &reqwest::blocking::Client, base: &str, snippet: &Snippet, user: &str, pass: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(snippet).map_err(|e| format!("序列化失败: {e}"))?;
    let path = format!("{}/{}", base, snippet_path(&snippet.id));
    let req = client
        .put(&path)
        .body(json)
        .header("Content-Type", "application/json");
    let resp = with_basic_auth(req, user, pass)
        .send()
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
fn download_snippet(client: &reqwest::blocking::Client, base: &str, id: &str, user: &str, pass: &str) -> Result<Option<Snippet>, String> {
    let path = format!("{}/{}", base, snippet_path(id));
    let mut req = client.get(&path);
    if !user.is_empty() {
        req = req.basic_auth(user, Some(pass));
    }
    let resp = req.send().map_err(|e| format!("下载失败: {e}"))?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("下载失败 (HTTP {}): {}", resp.status(), resp.text().unwrap_or_default()));
    }
    let text = resp.text().map_err(|e| format!("读取响应失败: {e}"))?;
    let snippet: Snippet = serde_json::from_str(&text).map_err(|e| format!("JSON 解析失败: {e}"))?;
    Ok(Some(snippet))
}

/// Upload manifest to WebDAV
fn upload_manifest(client: &reqwest::blocking::Client, base: &str, manifest: &Manifest, user: &str, pass: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest).map_err(|e| format!("序列化失败: {e}"))?;
    let path = format!("{}/{}", base, manifest_path());
    let req = client
        .put(&path)
        .body(json)
        .header("Content-Type", "application/json");
    let resp = with_basic_auth(req, user, pass)
        .send()
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
fn download_manifest(client: &reqwest::blocking::Client, base: &str, user: &str, pass: &str) -> Result<Option<Manifest>, String> {
    let path = format!("{}/{}", base, manifest_path());
    let mut req = client.get(&path);
    if !user.is_empty() {
        req = req.basic_auth(user, Some(pass));
    }
    let resp = req.send().map_err(|e| format!("下载清单失败: {e}"))?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("下载清单失败 (HTTP {}): {}", resp.status(), resp.text().unwrap_or_default()));
    }
    let text = resp.text().map_err(|e| format!("读取响应失败: {e}"))?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| format!("清单 JSON 解析失败: {e}"))?;
    Ok(Some(manifest))
}

/// Delete a snippet file from WebDAV
fn delete_snippet(client: &reqwest::blocking::Client, base: &str, id: &str, user: &str, pass: &str) -> Result<(), String> {
    let path = format!("{}/{}", base, snippet_path(id));
    let mut req = client.delete(&path);
    if !user.is_empty() {
        req = req.basic_auth(user, Some(pass));
    }
    let resp = req.send().map_err(|e| format!("删除失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() && status.as_u16() != 204 && status.as_u16() != 404 {
        return Err(format!("删除失败 (HTTP {}): {}", status, resp.text().unwrap_or_default()));
    }
    Ok(())
}

/// Per-snippet two-way merge sync.
/// Each snippet lives in snippets/{id}.json, tracked by snippets/manifest.json.
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
    let user = &settings.webdav_username;
    let pass = &settings.webdav_password;

    log::info!("sync_merge: base_url = {}", base);

    ensure_snippets_collection(&client, &base, user, pass)
        .map_err(|e| format!("创建 snippets 目录失败: {e}"))?;

    // Step 1: Get all local snippets
    let local_snippets = db::get_all_for_upload().map_err(|e| format!("读取本地数据失败: {e}"))?;
    let local_map: std::collections::HashMap<String, &Snippet> =
        local_snippets.iter().map(|s| (s.id.clone(), s)).collect();

    // Step 2: Download remote manifest (if exists)
    let remote_manifest = download_manifest(&client, &base, user, pass)
        .map_err(|e| format!("下载清单失败: {e}"))?;
    let remote_map: std::collections::HashMap<String, String> = remote_manifest
        .as_ref()
        .map(|m| m.snippets.iter().map(|s| (s.id.clone(), s.updated_at.clone())).collect())
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
            upload_snippet(&client, &base, snippet, user, pass)
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
                    if let Some(remote_snippet) = download_snippet(&client, &base, &meta.id, user, pass)? {
                        let merge = db::merge_snippets(vec![remote_snippet])
                            .map_err(|e| format!("合并失败: {e}"))?;
                        downloaded += merge.downloaded;
                    }
                }
            } else {
                // Remote-only: download and insert
                if let Some(remote_snippet) = download_snippet(&client, &base, &meta.id, user, pass)? {
                    let merge = db::merge_snippets(vec![remote_snippet])
                        .map_err(|e| format!("合并失败: {e}"))?;
                    downloaded += merge.downloaded;
                }
            }
        }

        // Step 5: Delete remote snippets that no longer exist locally
        for meta in &manifest.snippets {
            if !local_map.contains_key(&meta.id) {
                delete_snippet(&client, &base, &meta.id, user, pass)?;
                deleted_remote += 1;
            }
        }
    }

    // Step 6: Upload new manifest
    let new_manifest = Manifest {
        version: 1,
        snippets: local_snippets.iter().map(|s| SnippetMeta {
            id: s.id.clone(),
            updated_at: s.updated_at.clone(),
        }).collect(),
    };
    upload_manifest(&client, &base, &new_manifest, user, pass)
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
    db::record_sync_version(
        "merge",
        total,
        uploaded,
        downloaded,
        &summary_message,
    )
    .ok();

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
