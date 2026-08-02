use super::error::{SyncError, SyncErrorKind};
#[cfg(test)]
use super::protocol::{manifest_json, validate_manifest, Manifest};
use super::protocol::{
    manifest_v2_bytes, marker_bytes, parse_manifest_document, parse_marker, parse_revision_object,
    revision_object_bytes, revision_object_hash, ManifestDocument, ProtocolV2Marker, RemoteSnippet,
    RevisionObjectV2, WebDavBase, MAX_ERROR_BODY_BYTES, MAX_MANIFEST_BYTES, MAX_MARKER_BYTES,
    MAX_REVISION_BYTES, MAX_SNIPPET_BYTES,
};
use crate::db::{self, Snippet};
use crate::revision::sha256_hex;
use reqwest::header::{
    HeaderMap, HeaderValue, ETAG, IF_MATCH, IF_NONE_MATCH, RETRY_AFTER, WWW_AUTHENTICATE,
};
use std::borrow::Cow;
use std::io::Read;
use std::time::{Duration, Instant};

pub(crate) const MAX_HTTP_ATTEMPTS: usize = 3;
pub(crate) const MAX_RETRY_AFTER: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebDavAuthMode {
    Auto,
    Basic,
    Digest,
    Bearer,
    None,
}

impl WebDavAuthMode {
    pub(crate) fn from_settings(value: &str) -> Result<Self, SyncError> {
        match value.trim().to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "basic" => Ok(Self::Basic),
            "digest" => Ok(Self::Digest),
            "bearer" => Ok(Self::Bearer),
            "none" => Ok(Self::None),
            _ => Err(SyncError::configuration(
                "WebDAV authentication mode is unsupported",
            )),
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Basic => "basic",
            Self::Digest => "digest",
            Self::Bearer => "bearer",
            Self::None => "none",
        }
    }
}

#[derive(Clone)]
pub(crate) struct WebDavAuth {
    pub(crate) mode: WebDavAuthMode,
    username: String,
    password: String,
}

impl WebDavAuth {
    pub(crate) fn from_settings(
        mode: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, SyncError> {
        Ok(Self {
            mode: WebDavAuthMode::from_settings(mode)?,
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    fn bearer_token(&self) -> Option<String> {
        let token = if !self.password.trim().is_empty() {
            self.password.trim()
        } else {
            self.username.trim()
        };
        (!token.is_empty()).then(|| token.to_string())
    }
}

pub(crate) trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, duration: Duration);
}

#[derive(Debug, Default)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryPolicy {
    pub(crate) max_attempts: usize,
    pub(crate) initial_backoff: Duration,
    pub(crate) retry_after_cap: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: MAX_HTTP_ATTEMPTS,
            initial_backoff: Duration::from_millis(200),
            retry_after_cap: MAX_RETRY_AFTER,
        }
    }
}

impl RetryPolicy {
    fn delay(self, attempt: usize, headers: Option<&HeaderMap>) -> Duration {
        if let Some(retry_after) = headers
            .and_then(|headers| headers.get(RETRY_AFTER))
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
        {
            return Duration::from_secs(retry_after).min(self.retry_after_cap);
        }
        let exponent = attempt.saturating_sub(1).min(16) as u32;
        self.initial_backoff
            .checked_mul(1_u32 << exponent)
            .unwrap_or(MAX_BACKOFF)
            .min(MAX_BACKOFF)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedResource<T> {
    pub(crate) value: T,
    pub(crate) etag: Option<String>,
    pub(crate) body_hash: String,
    pub(crate) body_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResourceState<T> {
    Missing,
    Present(ParsedResource<T>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Precondition {
    Create,
    Match(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CasOutcome {
    Published { etag: Option<String> },
    PreconditionFailed,
    PreconditionRequired,
}

pub(crate) trait RemoteTransport {
    fn ensure_collection(&self, deadline: Instant) -> Result<(), SyncError>;
    fn ensure_objects_collection(&self, _deadline: Instant) -> Result<(), SyncError> {
        Err(SyncError::configuration(
            "WebDAV protocol v2 objects are unsupported by this transport",
        ))
    }
    fn get_manifest_document(
        &self,
        _deadline: Instant,
    ) -> Result<ResourceState<ManifestDocument>, SyncError> {
        Err(SyncError::configuration(
            "WebDAV protocol v2 manifests are unsupported by this transport",
        ))
    }
    fn get_marker(&self, _deadline: Instant) -> Result<ResourceState<ProtocolV2Marker>, SyncError> {
        Err(SyncError::configuration(
            "WebDAV protocol v2 markers are unsupported by this transport",
        ))
    }
    fn put_marker_conditional(
        &self,
        _marker: &ProtocolV2Marker,
        _precondition: &Precondition,
        _deadline: Instant,
    ) -> Result<CasOutcome, SyncError> {
        Err(SyncError::configuration(
            "WebDAV protocol v2 markers are unsupported by this transport",
        ))
    }
    fn get_revision(
        &self,
        _revision_id: &str,
        _deadline: Instant,
    ) -> Result<ResourceState<RevisionObjectV2>, SyncError> {
        Err(SyncError::configuration(
            "WebDAV protocol v2 objects are unsupported by this transport",
        ))
    }
    fn put_revision_immutable(
        &self,
        _revision: &RevisionObjectV2,
        _deadline: Instant,
    ) -> Result<(), SyncError> {
        Err(SyncError::configuration(
            "WebDAV protocol v2 objects are unsupported by this transport",
        ))
    }
    fn put_manifest_v2_conditional(
        &self,
        _manifest: &super::protocol::ManifestV2,
        _precondition: &Precondition,
        _deadline: Instant,
    ) -> Result<CasOutcome, SyncError> {
        Err(SyncError::configuration(
            "WebDAV protocol v2 manifests are unsupported by this transport",
        ))
    }

    #[cfg(test)]
    fn get_manifest(&self, deadline: Instant) -> Result<Option<Manifest>, SyncError>;
    #[cfg(test)]
    fn put_manifest(&self, manifest: &Manifest, deadline: Instant) -> Result<(), SyncError>;
    fn get_snippet(&self, id: &str, deadline: Instant) -> Result<Option<Snippet>, SyncError>;
    #[cfg(test)]
    fn put_snippet(&self, snippet: &Snippet, deadline: Instant) -> Result<(), SyncError>;
    #[cfg(test)]
    fn snippet_exists(&self, id: &str, deadline: Instant) -> Result<bool, SyncError>;
}

pub(crate) struct ReqwestTransport<C: Clock = SystemClock> {
    client: reqwest::blocking::Client,
    base: WebDavBase,
    auth: WebDavAuth,
    retry: RetryPolicy,
    operation_timeout: Duration,
    clock: C,
}

impl ReqwestTransport<SystemClock> {
    pub(crate) fn new(
        base: WebDavBase,
        auth: WebDavAuth,
        timeout: Duration,
    ) -> Result<Self, SyncError> {
        Self::with_clock(base, auth, timeout, RetryPolicy::default(), SystemClock)
    }
}

impl<C: Clock> ReqwestTransport<C> {
    pub(crate) fn with_clock(
        base: WebDavBase,
        auth: WebDavAuth,
        timeout: Duration,
        retry: RetryPolicy,
        clock: C,
    ) -> Result<Self, SyncError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|_| SyncError::network("HTTP client creation failed"))?;
        Ok(Self {
            client,
            base,
            auth,
            retry,
            operation_timeout: timeout,
            clock,
        })
    }

    fn check_deadline(&self, deadline: Instant) -> Result<(), SyncError> {
        if self.clock.now() >= deadline {
            Err(SyncError::deadline())
        } else {
            Ok(())
        }
    }

    fn build_request(
        &self,
        method: reqwest::Method,
        url: &reqwest::Url,
        body: Option<&[u8]>,
        content_type: Option<&str>,
        conditional: Option<&Precondition>,
        deadline: Instant,
    ) -> Result<reqwest::blocking::RequestBuilder, SyncError> {
        let remaining = deadline
            .checked_duration_since(self.clock.now())
            .ok_or_else(SyncError::deadline)?;
        let request_timeout = remaining.min(self.operation_timeout);
        let mut request = self
            .client
            .request(method, url.clone())
            .timeout(request_timeout);
        if let Some(content_type) = content_type {
            request = request.header("Content-Type", content_type);
        }
        if let Some(payload) = body {
            request = request.body(payload.to_vec());
        }
        if let Some(conditional) = conditional {
            request = match conditional {
                Precondition::Create => request.header(IF_NONE_MATCH, "*"),
                Precondition::Match(etag) => {
                    let value = HeaderValue::from_str(etag)
                        .map_err(|_| SyncError::validation("Remote resource ETag is invalid"))?;
                    request.header(IF_MATCH, value)
                }
            };
        }
        Ok(request)
    }

    fn send_once(
        &self,
        method: reqwest::Method,
        url: &reqwest::Url,
        body: Option<&[u8]>,
        content_type: Option<&str>,
        conditional: Option<&Precondition>,
        deadline: Instant,
    ) -> Result<reqwest::blocking::Response, SyncError> {
        let build = || {
            self.build_request(
                method.clone(),
                url,
                body,
                content_type,
                conditional,
                deadline,
            )
        };
        match self.auth.mode {
            WebDavAuthMode::None => build()?.send().map_err(request_error),
            WebDavAuthMode::Basic => {
                let request = if self.auth.username.trim().is_empty() {
                    build()?
                } else {
                    build()?.basic_auth(&self.auth.username, Some(&self.auth.password))
                };
                request.send().map_err(request_error)
            }
            WebDavAuthMode::Bearer => {
                let request = if let Some(token) = self.auth.bearer_token() {
                    build()?.bearer_auth(token)
                } else {
                    build()?
                };
                request.send().map_err(request_error)
            }
            WebDavAuthMode::Digest => self.send_with_digest(
                method,
                url,
                body,
                content_type,
                conditional,
                false,
                deadline,
            ),
            WebDavAuthMode::Auto => {
                self.send_with_digest(method, url, body, content_type, conditional, true, deadline)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn send_with_digest(
        &self,
        method: reqwest::Method,
        url: &reqwest::Url,
        body: Option<&[u8]>,
        content_type: Option<&str>,
        conditional: Option<&Precondition>,
        fallback_to_basic: bool,
        deadline: Instant,
    ) -> Result<reqwest::blocking::Response, SyncError> {
        let build = || {
            self.build_request(
                method.clone(),
                url,
                body,
                content_type,
                conditional,
                deadline,
            )
        };
        let send_basic = || {
            let request = if self.auth.username.trim().is_empty() {
                build()?
            } else {
                build()?.basic_auth(&self.auth.username, Some(&self.auth.password))
            };
            request.send().map_err(request_error)
        };

        let first = build()?.send().map_err(request_error)?;
        if first.status() != reqwest::StatusCode::UNAUTHORIZED
            || self.auth.username.trim().is_empty()
        {
            return Ok(first);
        }
        let challenge = find_digest_challenge(first.headers());
        discard_error_body(first);
        let Some(challenge) = challenge else {
            return if fallback_to_basic {
                send_basic()
            } else {
                Err(SyncError::authentication())
            };
        };
        let mut challenge_header = match digest_auth::parse(&challenge) {
            Ok(parsed) => parsed,
            Err(_) if fallback_to_basic => return send_basic(),
            Err(_) => return Err(SyncError::authentication()),
        };
        let context = digest_auth::AuthContext::new_with_method(
            self.auth.username.clone(),
            self.auth.password.clone(),
            digest_request_uri(url),
            body.map(ToOwned::to_owned),
            digest_auth::HttpMethod(Cow::Owned(method.as_str().to_string())),
        );
        let digest_header = match challenge_header.respond(&context) {
            Ok(header) => header.to_header_string(),
            Err(_) if fallback_to_basic => return send_basic(),
            Err(_) => return Err(SyncError::authentication()),
        };
        let response = build()?
            .header(reqwest::header::AUTHORIZATION, digest_header)
            .send()
            .map_err(request_error)?;
        if fallback_to_basic && response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return send_basic();
        }
        Ok(response)
    }

    fn wait_before_retry(&self, delay: Duration, deadline: Instant) -> Result<(), SyncError> {
        let remaining = deadline
            .checked_duration_since(self.clock.now())
            .ok_or_else(SyncError::deadline)?;
        if !delay.is_zero() && delay >= remaining {
            return Err(SyncError::deadline());
        }
        self.clock.sleep(delay);
        self.check_deadline(deadline)
    }

    fn send_idempotent(
        &self,
        method: reqwest::Method,
        url: &reqwest::Url,
        body: Option<&[u8]>,
        content_type: Option<&str>,
        conditional: Option<&Precondition>,
        deadline: Instant,
    ) -> Result<reqwest::blocking::Response, SyncError> {
        for attempt in 1..=self.retry.max_attempts {
            self.check_deadline(deadline)?;
            match self.send_once(
                method.clone(),
                url,
                body,
                content_type,
                conditional,
                deadline,
            ) {
                Ok(response)
                    if retryable_status(response.status()) && attempt < self.retry.max_attempts =>
                {
                    let delay = self.retry.delay(attempt, Some(response.headers()));
                    discard_error_body(response);
                    self.wait_before_retry(delay, deadline)?;
                }
                Ok(response) if retryable_status(response.status()) => {
                    discard_error_body(response);
                    return Err(SyncError::retry_limit());
                }
                Ok(response) => return Ok(response),
                Err(error)
                    if error.kind == SyncErrorKind::Network
                        && attempt < self.retry.max_attempts =>
                {
                    self.wait_before_retry(self.retry.delay(attempt, None), deadline)?;
                }
                Err(error) => return Err(error),
            }
        }
        Err(SyncError::retry_limit())
    }

    fn send_non_retrying(
        &self,
        method: reqwest::Method,
        url: &reqwest::Url,
        body: Option<&[u8]>,
        content_type: Option<&str>,
        conditional: Option<&Precondition>,
        deadline: Instant,
    ) -> Result<reqwest::blocking::Response, SyncError> {
        self.check_deadline(deadline)?;
        self.send_once(method, url, body, content_type, conditional, deadline)
    }

    fn put_conditional(
        &self,
        url: &reqwest::Url,
        bytes: &[u8],
        precondition: &Precondition,
        deadline: Instant,
    ) -> Result<CasOutcome, SyncError> {
        let response = self.send_non_retrying(
            reqwest::Method::PUT,
            url,
            Some(bytes),
            Some("application/json"),
            Some(precondition),
            deadline,
        )?;
        let status = response.status();
        if status == reqwest::StatusCode::PRECONDITION_FAILED {
            discard_error_body(response);
            return Ok(CasOutcome::PreconditionFailed);
        }
        if status.as_u16() == 428 {
            discard_error_body(response);
            return Ok(CasOutcome::PreconditionRequired);
        }
        if status.is_success() {
            let etag = response_etag(response.headers())?;
            discard_error_body(response);
            return Ok(CasOutcome::Published { etag });
        }
        discard_error_body(response);
        Err(http_error(status))
    }

    fn ensure_collection_url(
        &self,
        url: &reqwest::Url,
        deadline: Instant,
    ) -> Result<(), SyncError> {
        let method = reqwest::Method::from_bytes(b"MKCOL").expect("valid MKCOL method");
        let response = self.send_non_retrying(method, url, None, None, None, deadline)?;
        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            return Ok(());
        }
        discard_error_body(response);
        Err(http_error(status))
    }
}

impl<C: Clock> RemoteTransport for ReqwestTransport<C> {
    fn ensure_collection(&self, deadline: Instant) -> Result<(), SyncError> {
        let url = self.base.collection_url().map_err(SyncError::from)?;
        self.ensure_collection_url(&url, deadline)
    }

    fn ensure_objects_collection(&self, deadline: Instant) -> Result<(), SyncError> {
        let url = self
            .base
            .objects_collection_url()
            .map_err(SyncError::from)?;
        self.ensure_collection_url(&url, deadline)
    }

    fn get_manifest_document(
        &self,
        deadline: Instant,
    ) -> Result<ResourceState<ManifestDocument>, SyncError> {
        let url = self.base.manifest_url().map_err(SyncError::from)?;
        let response =
            self.send_idempotent(reqwest::Method::GET, &url, None, None, None, deadline)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(ResourceState::Missing);
        }
        if !status.is_success() {
            discard_error_body(response);
            return Err(http_error(status));
        }
        let etag = response_etag(response.headers())?;
        let bytes = read_limited_response_bytes(response, MAX_MANIFEST_BYTES, "Remote manifest")?;
        let value = parse_manifest_document(&bytes).map_err(SyncError::from)?;
        Ok(ResourceState::Present(ParsedResource {
            value,
            etag,
            body_hash: sha256_hex(&bytes),
            body_bytes: bytes.len(),
        }))
    }

    fn get_marker(&self, deadline: Instant) -> Result<ResourceState<ProtocolV2Marker>, SyncError> {
        let url = self.base.marker_url().map_err(SyncError::from)?;
        let response =
            self.send_idempotent(reqwest::Method::GET, &url, None, None, None, deadline)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(ResourceState::Missing);
        }
        if !status.is_success() {
            discard_error_body(response);
            return Err(http_error(status));
        }
        let etag = response_etag(response.headers())?;
        let bytes = read_limited_response_bytes(response, MAX_MARKER_BYTES, "Remote marker")?;
        let value = parse_marker(&bytes).map_err(SyncError::from)?;
        Ok(ResourceState::Present(ParsedResource {
            value,
            etag,
            body_hash: sha256_hex(&bytes),
            body_bytes: bytes.len(),
        }))
    }

    fn put_marker_conditional(
        &self,
        marker: &ProtocolV2Marker,
        precondition: &Precondition,
        deadline: Instant,
    ) -> Result<CasOutcome, SyncError> {
        let url = self.base.marker_url().map_err(SyncError::from)?;
        let bytes = marker_bytes(marker).map_err(SyncError::from)?;
        self.put_conditional(&url, &bytes, precondition, deadline)
    }

    fn get_revision(
        &self,
        revision_id: &str,
        deadline: Instant,
    ) -> Result<ResourceState<RevisionObjectV2>, SyncError> {
        let url = self
            .base
            .revision_url(revision_id)
            .map_err(SyncError::from)?;
        let response =
            self.send_idempotent(reqwest::Method::GET, &url, None, None, None, deadline)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(ResourceState::Missing);
        }
        if !status.is_success() {
            discard_error_body(response);
            return Err(http_error(status));
        }
        let etag = response_etag(response.headers())?;
        let bytes = read_limited_response_bytes(response, MAX_REVISION_BYTES, "Remote revision")?;
        let value = parse_revision_object(revision_id, &bytes).map_err(SyncError::from)?;
        Ok(ResourceState::Present(ParsedResource {
            value,
            etag,
            body_hash: sha256_hex(&bytes),
            body_bytes: bytes.len(),
        }))
    }

    fn put_revision_immutable(
        &self,
        revision: &RevisionObjectV2,
        deadline: Instant,
    ) -> Result<(), SyncError> {
        let url = self
            .base
            .revision_url(&revision.revision_id)
            .map_err(SyncError::from)?;
        let bytes = revision_object_bytes(revision).map_err(SyncError::from)?;
        match self.put_conditional(&url, &bytes, &Precondition::Create, deadline)? {
            CasOutcome::Published { .. } => Ok(()),
            CasOutcome::PreconditionFailed | CasOutcome::PreconditionRequired => {
                match self.get_revision(&revision.revision_id, deadline)? {
                    ResourceState::Present(observed)
                        if observed.body_hash == sha256_hex(&bytes)
                            && revision_object_hash(&observed.value)
                                .map_err(SyncError::from)?
                                == revision_object_hash(revision).map_err(SyncError::from)? =>
                    {
                        Ok(())
                    }
                    ResourceState::Present(_) => Err(SyncError::validation(
                        "Remote immutable revision identifier collision detected",
                    )),
                    ResourceState::Missing => Err(SyncError::cas_conflict()),
                }
            }
        }
    }

    fn put_manifest_v2_conditional(
        &self,
        manifest: &super::protocol::ManifestV2,
        precondition: &Precondition,
        deadline: Instant,
    ) -> Result<CasOutcome, SyncError> {
        let url = self.base.manifest_url().map_err(SyncError::from)?;
        let bytes = manifest_v2_bytes(manifest).map_err(SyncError::from)?;
        self.put_conditional(&url, &bytes, precondition, deadline)
    }

    #[cfg(test)]
    fn get_manifest(&self, deadline: Instant) -> Result<Option<Manifest>, SyncError> {
        let url = self.base.manifest_url().map_err(SyncError::from)?;
        let response =
            self.send_idempotent(reqwest::Method::GET, &url, None, None, None, deadline)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            discard_error_body(response);
            return Err(http_error(status));
        }
        let text = read_limited_response(response, MAX_MANIFEST_BYTES, "Remote manifest")?;
        let manifest = serde_json::from_str::<Manifest>(&text)
            .map_err(|_| SyncError::validation("Remote manifest JSON is invalid"))?;
        validate_manifest(&manifest)
            .map_err(|_| SyncError::validation("Remote manifest validation failed"))?;
        Ok(Some(manifest))
    }

    #[cfg(test)]
    fn put_manifest(&self, manifest: &Manifest, deadline: Instant) -> Result<(), SyncError> {
        let json = manifest_json(manifest).map_err(|_| {
            SyncError::validation("Remote manifest serialization or validation failed")
        })?;
        let url = self.base.manifest_url().map_err(SyncError::from)?;
        let response = self.send_idempotent(
            reqwest::Method::PUT,
            &url,
            Some(json.as_bytes()),
            Some("application/json"),
            None,
            deadline,
        )?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            discard_error_body(response);
            Err(http_error(status))
        }
    }

    fn get_snippet(&self, id: &str, deadline: Instant) -> Result<Option<Snippet>, SyncError> {
        let url = self.base.snippet_url(id).map_err(SyncError::from)?;
        let response =
            self.send_idempotent(reqwest::Method::GET, &url, None, None, None, deadline)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            discard_error_body(response);
            return Err(http_error(status));
        }
        let text = read_limited_response(response, MAX_SNIPPET_BYTES, "Remote snippet")?;
        let remote = serde_json::from_str::<RemoteSnippet>(&text)
            .map_err(|_| SyncError::validation("Remote snippet JSON is invalid"))?;
        if remote.id != id {
            return Err(SyncError::validation(
                "Remote snippet identifier does not match its manifest entry",
            ));
        }
        let snippet: Snippet = remote.into();
        db::validate_snippet(&snippet)
            .map_err(|_| SyncError::validation("Remote snippet validation failed"))?;
        Ok(Some(snippet))
    }

    #[cfg(test)]
    fn put_snippet(&self, snippet: &Snippet, deadline: Instant) -> Result<(), SyncError> {
        let remote = RemoteSnippet::from(snippet);
        let json = serde_json::to_string_pretty(&remote)
            .map_err(|_| SyncError::validation("Snippet serialization failed"))?;
        if json.len() > MAX_SNIPPET_BYTES {
            return Err(SyncError::validation(
                "Snippet exceeds the synchronization size limit",
            ));
        }
        let url = self
            .base
            .snippet_url(&snippet.id)
            .map_err(SyncError::from)?;
        let response = self.send_idempotent(
            reqwest::Method::PUT,
            &url,
            Some(json.as_bytes()),
            Some("application/json"),
            None,
            deadline,
        )?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            discard_error_body(response);
            Err(http_error(status))
        }
    }

    #[cfg(test)]
    fn snippet_exists(&self, id: &str, deadline: Instant) -> Result<bool, SyncError> {
        let url = self.base.snippet_url(id).map_err(SyncError::from)?;
        let response =
            self.send_non_retrying(reqwest::Method::HEAD, &url, None, None, None, deadline)?;
        let status = response.status();
        if status.is_success() {
            return Ok(true);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if is_explicit_policy_or_auth(status) {
            discard_error_body(response);
            return Err(http_error(status));
        }

        // Reconciliation still follows with bounded GET and content validation.
        discard_error_body(response);
        self.get_snippet(id, deadline)
            .map(|snippet| snippet.is_some())
    }
}

fn response_etag(headers: &HeaderMap) -> Result<Option<String>, SyncError> {
    let values = headers.get_all(ETAG);
    let mut values = values.iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(SyncError::validation(
            "Remote resource returned multiple ETag values",
        ));
    }
    let value = value
        .to_str()
        .map_err(|_| SyncError::validation("Remote resource ETag is invalid"))?
        .trim();
    if value.is_empty() || value.contains(',') || value.chars().any(char::is_control) {
        return Err(SyncError::validation("Remote resource ETag is invalid"));
    }
    Ok(Some(value.to_string()))
}

pub(crate) fn require_strong_etag(value: Option<&str>) -> Result<String, SyncError> {
    let value = value.ok_or_else(|| {
        SyncError::configuration("WebDAV server did not return a required strong ETag")
    })?;
    let bytes = value.as_bytes();
    if value.starts_with("W/")
        || bytes.len() < 2
        || bytes.first() != Some(&b'"')
        || bytes.last() != Some(&b'"')
        || value[1..value.len() - 1]
            .chars()
            .any(|character| character == '"' || character.is_control())
    {
        return Err(SyncError::configuration(
            "WebDAV server did not return a valid strong ETag",
        ));
    }
    Ok(value.to_string())
}

fn request_error(error: reqwest::Error) -> SyncError {
    if error.is_timeout() {
        SyncError::network("WebDAV request timed out")
    } else if error.is_connect() {
        SyncError::network("WebDAV server could not be reached")
    } else {
        SyncError::network("WebDAV request failed")
    }
}

fn digest_request_uri(url: &reqwest::Url) -> String {
    let mut uri = url.path().to_string();
    if uri.is_empty() {
        uri.push('/');
    }
    if let Some(query) = url.query() {
        uri.push('?');
        uri.push_str(query);
    }
    uri
}

fn find_digest_challenge(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(WWW_AUTHENTICATE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.to_ascii_lowercase().contains("digest"))
        .map(str::to_string)
}

fn retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::TOO_MANY_REQUESTS
            | reqwest::StatusCode::INTERNAL_SERVER_ERROR
            | reqwest::StatusCode::BAD_GATEWAY
            | reqwest::StatusCode::SERVICE_UNAVAILABLE
            | reqwest::StatusCode::GATEWAY_TIMEOUT
    )
}

#[cfg(test)]
fn is_explicit_policy_or_auth(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED
            | reqwest::StatusCode::FORBIDDEN
            | reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED
            | reqwest::StatusCode::UPGRADE_REQUIRED
            | reqwest::StatusCode::MISDIRECTED_REQUEST
    )
}

fn http_error(status: reqwest::StatusCode) -> SyncError {
    match status {
        reqwest::StatusCode::UNAUTHORIZED => SyncError::authentication(),
        reqwest::StatusCode::FORBIDDEN => SyncError::authorization(),
        status if status.is_client_error() => {
            SyncError::validation("WebDAV server rejected the synchronization request")
        }
        _ => SyncError::network("WebDAV server operation failed"),
    }
}

fn read_limited_response_bytes(
    response: reqwest::blocking::Response,
    limit: usize,
    label: &'static str,
) -> Result<Vec<u8>, SyncError> {
    if response
        .content_length()
        .map(|length| length > limit as u64)
        .unwrap_or(false)
    {
        return Err(SyncError::validation(
            "WebDAV response exceeds its size limit",
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SyncError::network("Reading the WebDAV response failed"))?;
    if bytes.len() > limit {
        return Err(SyncError::validation(
            "WebDAV response exceeds its size limit",
        ));
    }
    log::trace!("{label} response passed bounded read");
    Ok(bytes)
}

fn read_limited_response(
    response: reqwest::blocking::Response,
    limit: usize,
    label: &'static str,
) -> Result<String, SyncError> {
    String::from_utf8(read_limited_response_bytes(response, limit, label)?)
        .map_err(|_| SyncError::validation("WebDAV response is not valid UTF-8"))
        .inspect_err(|_| {
            log::debug!("{label} response failed bounded decoding");
        })
}

fn discard_error_body(response: reqwest::blocking::Response) {
    let _ = read_limited_response(response, MAX_ERROR_BODY_BYTES, "WebDAV error");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_statuses_are_transient_only() {
        for status in [408, 429, 500, 502, 503, 504] {
            assert!(retryable_status(
                reqwest::StatusCode::from_u16(status).unwrap()
            ));
        }
        for status in [400, 401, 403, 404, 405, 409, 422] {
            assert!(!retryable_status(
                reqwest::StatusCode::from_u16(status).unwrap()
            ));
        }
    }

    #[test]
    fn retry_policy_is_bounded_and_exponential() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.delay(1, None), Duration::from_millis(200));
        assert_eq!(policy.delay(2, None), Duration::from_millis(400));
        assert_eq!(policy.delay(20, None), MAX_BACKOFF);
        assert_eq!(policy.max_attempts, MAX_HTTP_ATTEMPTS);
    }

    #[test]
    fn retry_after_is_capped() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, "999".parse().unwrap());
        assert_eq!(
            RetryPolicy::default().delay(1, Some(&headers)),
            MAX_RETRY_AFTER
        );
    }

    #[test]
    fn retry_wait_does_not_sleep_past_the_deadline() {
        #[derive(Debug)]
        struct FakeClock {
            now: Instant,
            sleeps: std::sync::Mutex<Vec<Duration>>,
        }

        impl Clock for FakeClock {
            fn now(&self) -> Instant {
                self.now
            }

            fn sleep(&self, duration: Duration) {
                self.sleeps.lock().unwrap().push(duration);
            }
        }

        let now = Instant::now();
        let transport = ReqwestTransport::with_clock(
            WebDavBase::parse("http://127.0.0.1/dedicated-test-root/").unwrap(),
            WebDavAuth::from_settings("none", "", "").unwrap(),
            Duration::from_secs(5),
            RetryPolicy::default(),
            FakeClock {
                now,
                sleeps: std::sync::Mutex::new(Vec::new()),
            },
        )
        .unwrap();

        let error = transport
            .wait_before_retry(Duration::from_secs(2), now + Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(error.kind, SyncErrorKind::Deadline);
        assert!(transport.clock.sleeps.lock().unwrap().is_empty());
    }

    #[test]
    fn strong_etag_validation_rejects_missing_and_weak_values() {
        assert_eq!(
            require_strong_etag(Some("\"manifest-1\"")).unwrap(),
            "\"manifest-1\""
        );
        for value in [None, Some("W/\"manifest-1\""), Some("manifest-1"), Some("")] {
            let error = require_strong_etag(value).unwrap_err();
            assert_eq!(error.kind, SyncErrorKind::Configuration);
            assert!(!error.retryable);
        }
    }

    #[test]
    fn auth_modes_parse_all_supported_values() {
        for value in ["auto", "basic", "digest", "bearer", "none"] {
            assert!(WebDavAuthMode::from_settings(value).is_ok());
        }
        assert!(WebDavAuthMode::from_settings("unknown").is_err());
    }
}
