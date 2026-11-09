use crate::{
    config::Config,
    conversation_log::ConversationLogger,
    db::Database,
    model::{Application, Provider, RequestRecord},
};
use axum::{
    body::{Body, Bytes},
    extract::{OriginalUri, State},
    http::{HeaderMap, Method, StatusCode},
    response::Response,
    Router,
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct HandlerState {
    app_id: String,
    db: Arc<Database>,
    client: reqwest::Client,
    conversations: ConversationLogger,
}

pub struct ProxyManager {
    db: Arc<Database>,
    config: Config,
    config_directory: std::path::PathBuf,
    servers: Mutex<HashMap<String, ServerHandle>>,
}

struct ServerHandle {
    token: CancellationToken,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl ProxyManager {
    pub fn new(db: Arc<Database>, config: Config, config_directory: &Path) -> Self {
        Self {
            db,
            config,
            config_directory: config_directory.to_path_buf(),
            servers: Mutex::new(HashMap::new()),
        }
    }

    pub async fn start_all(&self) {
        let _ = self
            .db
            .prune_requests(self.config.retention.request_metadata_days);
        for application in self
            .db
            .applications()
            .unwrap_or_default()
            .into_iter()
            .filter(|application| application.enabled)
        {
            let _ = self.start(application).await;
        }
    }

    pub async fn start(&self, application: Application) -> anyhow::Result<()> {
        self.stop(&application.id).await;
        let listener = tokio::net::TcpListener::bind(format!(
            "{}:{}",
            self.config.server.bind, application.port
        ))
        .await?;
        let token = CancellationToken::new();
        let state = HandlerState {
            app_id: application.id.clone(),
            db: self.db.clone(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(
                    self.config.server.request_timeout_seconds,
                ))
                .build()?,
            conversations: ConversationLogger::new(&self.config.privacy, &self.config_directory),
        };
        let router = Router::new().fallback(forward).with_state(state);
        let shutdown = token.clone();
        let task = tauri::async_runtime::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await;
        });
        self.servers
            .lock()
            .unwrap()
            .insert(application.id, ServerHandle { token, task });
        Ok(())
    }

    pub async fn stop(&self, id: &str) {
        let server = self.servers.lock().unwrap().remove(id);
        if let Some(server) = server {
            server.token.cancel();
            let _ = server.task.await;
        }
    }

    pub async fn free_port(&self) -> anyhow::Result<u16> {
        let used = self
            .db
            .applications()?
            .into_iter()
            .map(|application| application.port)
            .collect::<Vec<_>>();
        for port in self.config.server.port_start..=self.config.server.port_end {
            if !used.contains(&port)
                && tokio::net::TcpListener::bind((self.config.server.bind.as_str(), port))
                    .await
                    .is_ok()
            {
                return Ok(port);
            }
        }
        anyhow::bail!("No free ports in the configured range")
    }
}

async fn forward(
    State(state): State<HandlerState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let path = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    match forward_inner(&state, path, method, headers, &body).await {
        Ok(response) => response,
        Err(error) => json_response(
            error.status,
            json!({"error":{"message":error.message,"type":error.kind}}),
        ),
    }
}

#[derive(Debug)]
struct ProxyError {
    status: StatusCode,
    message: String,
    kind: &'static str,
}

impl ProxyError {
    fn new(status: StatusCode, message: impl Into<String>, kind: &'static str) -> Self {
        Self {
            status,
            message: message.into(),
            kind,
        }
    }
}

async fn forward_inner(
    state: &HandlerState,
    path: &str,
    method: Method,
    headers: HeaderMap,
    body: &[u8],
) -> Result<Response, ProxyError> {
    let started = Instant::now();
    let endpoint = endpoint(path).ok_or_else(|| {
        ProxyError::new(
            StatusCode::NOT_FOUND,
            "Unsupported OpenAI-compatible endpoint",
            "unsupported_endpoint",
        )
    })?;
    let modality = modality(endpoint);
    let application = state
        .db
        .application(&state.app_id)
        .map_err(internal)?
        .ok_or_else(|| {
            ProxyError::new(
                StatusCode::UNAUTHORIZED,
                "Unknown application",
                "authentication_error",
            )
        })?;
    let supplied = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
        });
    if supplied != Some(application.api_key.as_str()) {
        record(
            state,
            &application,
            None,
            "",
            endpoint,
            modality,
            StatusCode::UNAUTHORIZED,
            started,
            body.len(),
            0,
            Some("authentication_error"),
            0,
            0,
        );
        return Err(ProxyError::new(
            StatusCode::UNAUTHORIZED,
            "Invalid agenthub API key",
            "authentication_error",
        ));
    }
    let provider = match application.provider_id.as_deref() {
        Some(id) => state.db.provider(id).map_err(internal)?,
        None => state.db.default_provider().map_err(internal)?,
    }
    .ok_or_else(|| {
        record(
            state,
            &application,
            None,
            "",
            endpoint,
            modality,
            StatusCode::SERVICE_UNAVAILABLE,
            started,
            body.len(),
            0,
            Some("provider_unavailable"),
            0,
            0,
        );
        ProxyError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "No provider configured",
            "provider_unavailable",
        )
    })?;
    let key = provider_secret(&provider.id).unwrap_or_default();
    if key.is_empty() && !matches!(provider.kind.as_str(), "ollama" | "compatible") {
        record(
            state,
            &application,
            Some(&provider),
            &provider.default_model,
            endpoint,
            modality,
            StatusCode::SERVICE_UNAVAILABLE,
            started,
            body.len(),
            0,
            Some("provider_unavailable"),
            0,
            0,
        );
        return Err(ProxyError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Provider has no API key",
            "provider_unavailable",
        ));
    }
    if !supports(&provider.kind, endpoint) {
        record(
            state,
            &application,
            Some(&provider),
            &provider.default_model,
            endpoint,
            modality,
            StatusCode::NOT_IMPLEMENTED,
            started,
            body.len(),
            0,
            Some("unsupported_provider_capability"),
            0,
            0,
        );
        return Err(ProxyError::new(
            StatusCode::NOT_IMPLEMENTED,
            format!("{} does not support this endpoint", provider.name),
            "unsupported_provider_capability",
        ));
    }
    if endpoint == "/models" && method == Method::GET {
        let response_body = serde_json::to_vec(&json!({
            "object":"list",
            "data":[{"id":provider.default_model,"object":"model","owned_by":provider.name}]
        }))
        .map_err(internal)?;
        record(
            state,
            &application,
            Some(&provider),
            &provider.default_model,
            endpoint,
            modality,
            StatusCode::OK,
            started,
            body.len(),
            response_body.len(),
            None,
            0,
            0,
        );
        return Ok(json_bytes(StatusCode::OK, response_body));
    }

    let original_json: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let model = original_json
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(&provider.default_model)
        .to_owned();
    let wants_stream = original_json.get("stream").and_then(Value::as_bool) == Some(true);
    let adapted = adapt_request(&provider, path, &model, &key, &original_json, body)?;
    if wants_stream && adapted.translated {
        return Err(ProxyError::new(
            StatusCode::NOT_IMPLEMENTED,
            "Streaming translation is not supported for this provider",
            "unsupported_provider_capability",
        ));
    }
    let mut request = state.client.request(method, adapted.url);
    for (name, value) in &headers {
        if !matches!(
            name.as_str(),
            "host" | "authorization" | "x-api-key" | "content-length" | "accept-encoding"
        ) {
            request = request.header(name, value);
        }
    }
    request = match provider.kind.as_str() {
        "azure" => request.header("api-key", &key),
        "anthropic" => request
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01"),
        "google" => request,
        _ if !key.is_empty() => request.bearer_auth(&key),
        _ => request,
    };
    let upstream = request.body(adapted.body).send().await.map_err(|_| {
        record(
            state,
            &application,
            Some(&provider),
            &model,
            endpoint,
            modality,
            StatusCode::BAD_GATEWAY,
            started,
            body.len(),
            0,
            Some("upstream_connection_error"),
            0,
            0,
        );
        ProxyError::new(
            StatusCode::BAD_GATEWAY,
            "The provider request failed",
            "upstream_connection_error",
        )
    })?;
    let status = upstream.status();
    let response_headers = upstream.headers().clone();
    let is_stream = response_headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"));
    if is_stream && !adapted.translated {
        return streaming_response(
            state,
            upstream,
            response_headers,
            application,
            provider,
            model,
            endpoint.to_string(),
            modality.to_string(),
            started,
            body.to_vec(),
        );
    }
    let bytes = upstream.bytes().await.map_err(|_| {
        ProxyError::new(
            StatusCode::BAD_GATEWAY,
            "The provider response could not be read",
            "upstream_response_error",
        )
    })?;
    let response_body = adapt_response(&provider, path, status.as_u16(), &bytes);
    let (tokens_in, tokens_out) = usage(&response_body);
    let error_type = (status.as_u16() >= 400).then_some("upstream_error");
    record(
        state,
        &application,
        Some(&provider),
        &model,
        endpoint,
        modality,
        status,
        started,
        body.len(),
        response_body.len(),
        error_type,
        tokens_in,
        tokens_out,
    );
    let _ = state.conversations.log(
        &application.id,
        &provider.id,
        endpoint,
        body,
        &response_body,
    );
    response_with_headers(status, &response_headers, Body::from(response_body))
}

struct AdaptedRequest {
    url: String,
    body: Vec<u8>,
    translated: bool,
}

fn adapt_request(
    provider: &Provider,
    path: &str,
    model: &str,
    key: &str,
    json_body: &Value,
    raw: &[u8],
) -> Result<AdaptedRequest, ProxyError> {
    let suffix = path.strip_prefix("/v1").unwrap_or(path);
    match provider.kind.as_str() {
        "anthropic" if suffix.starts_with("/chat/completions") => {
            let messages = json_body
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let system = messages
                .iter()
                .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
                .filter_map(|message| message.get("content").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            let messages = messages
                .into_iter()
                .filter(|message| message.get("role").and_then(Value::as_str) != Some("system"))
                .collect::<Vec<_>>();
            let body = json!({
                "model":model,
                "messages":messages,
                "system":system,
                "max_tokens":json_body.get("max_tokens").and_then(Value::as_u64).unwrap_or(4096),
                "temperature":json_body.get("temperature").cloned().unwrap_or(json!(1))
            });
            Ok(AdaptedRequest {
                url: format!("{}/messages", provider.base_url.trim_end_matches('/')),
                body: serde_json::to_vec(&body).map_err(internal)?,
                translated: true,
            })
        }
        "google" if suffix.starts_with("/chat/completions") => {
            let contents = json_body
                .get("messages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|message| message.get("role").and_then(Value::as_str) != Some("system"))
                .map(|message| {
                    json!({
                        "role":if message.get("role").and_then(Value::as_str)==Some("assistant") {"model"} else {"user"},
                        "parts":[{"text":message.get("content").and_then(Value::as_str).unwrap_or("")}]
                    })
                })
                .collect::<Vec<_>>();
            let body = json!({
                "contents":contents,
                "generationConfig":{
                    "temperature":json_body.get("temperature").cloned().unwrap_or(json!(1)),
                    "maxOutputTokens":json_body.get("max_tokens").cloned().unwrap_or(json!(4096))
                }
            });
            Ok(AdaptedRequest {
                url: format!(
                    "{}/models/{}:generateContent?key={}",
                    provider.base_url.trim_end_matches('/'),
                    model,
                    key
                ),
                body: serde_json::to_vec(&body).map_err(internal)?,
                translated: true,
            })
        }
        "azure" => Ok(AdaptedRequest {
            url: format!(
                "{}/openai/deployments/{}{}{}",
                provider.base_url.trim_end_matches('/'),
                model,
                suffix,
                if suffix.contains('?') {
                    "&api-version=2024-10-21"
                } else {
                    "?api-version=2024-10-21"
                }
            ),
            body: raw.to_vec(),
            translated: false,
        }),
        _ => Ok(AdaptedRequest {
            url: format!("{}{}", provider.base_url.trim_end_matches('/'), suffix),
            body: raw.to_vec(),
            translated: false,
        }),
    }
}

fn adapt_response(provider: &Provider, path: &str, status: u16, raw: &[u8]) -> Vec<u8> {
    if status >= 400 || !path.contains("chat/completions") {
        return raw.to_vec();
    }
    let value: Value = match serde_json::from_slice(raw) {
        Ok(value) => value,
        Err(_) => return raw.to_vec(),
    };
    let converted = match provider.kind.as_str() {
        "anthropic" => {
            let text = value
                .get("content")
                .and_then(Value::as_array)
                .and_then(|content| content.first())
                .and_then(|content| content.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let usage = value.get("usage").cloned().unwrap_or(Value::Null);
            Some(json!({
                "id":value.get("id").cloned().unwrap_or(json!("agenthub")),
                "object":"chat.completion",
                "model":value.get("model").cloned().unwrap_or(json!(provider.default_model)),
                "choices":[{"index":0,"message":{"role":"assistant","content":text},"finish_reason":"stop"}],
                "usage":{
                    "prompt_tokens":usage.get("input_tokens").cloned().unwrap_or(json!(0)),
                    "completion_tokens":usage.get("output_tokens").cloned().unwrap_or(json!(0)),
                    "total_tokens":usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0)
                        + usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0)
                }
            }))
        }
        "google" => {
            let text = value
                .pointer("/candidates/0/content/parts/0/text")
                .and_then(Value::as_str)
                .unwrap_or("");
            let usage = value.get("usageMetadata").cloned().unwrap_or(Value::Null);
            Some(json!({
                "id":"agenthub",
                "object":"chat.completion",
                "model":provider.default_model,
                "choices":[{"index":0,"message":{"role":"assistant","content":text},"finish_reason":"stop"}],
                "usage":{
                    "prompt_tokens":usage.get("promptTokenCount").cloned().unwrap_or(json!(0)),
                    "completion_tokens":usage.get("candidatesTokenCount").cloned().unwrap_or(json!(0)),
                    "total_tokens":usage.get("totalTokenCount").cloned().unwrap_or(json!(0))
                }
            }))
        }
        _ => None,
    };
    converted
        .and_then(|value| serde_json::to_vec(&value).ok())
        .unwrap_or_else(|| raw.to_vec())
}

#[allow(clippy::too_many_arguments)]
fn streaming_response(
    state: &HandlerState,
    upstream: reqwest::Response,
    response_headers: HeaderMap,
    application: Application,
    provider: Provider,
    model: String,
    endpoint: String,
    modality: String,
    started: Instant,
    request_body: Vec<u8>,
) -> Result<Response, ProxyError> {
    let status = upstream.status();
    let db = state.db.clone();
    let conversations = state.conversations.clone();
    let capture_response = conversations.enabled();
    let stream = async_stream::stream! {
        let mut upstream_stream = upstream.bytes_stream();
        let mut response_bytes = 0_u64;
        let mut captured = Vec::new();
        while let Some(item) = upstream_stream.next().await {
            let chunk = match item {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = db.record(&RequestRecord {
                        application_id: &application.id,
                        provider_id: Some(&provider.id),
                        model: &model,
                        endpoint: &endpoint,
                        modality: &modality,
                        status: StatusCode::BAD_GATEWAY.as_u16(),
                        tokens_in: 0,
                        tokens_out: 0,
                        duration_ms: started.elapsed().as_millis() as u64,
                        request_bytes: request_body.len() as u64,
                        response_bytes,
                        error_type: Some("upstream_response_error"),
                    });
                    yield Err::<Bytes, std::io::Error>(std::io::Error::other(error));
                    return;
                }
            };
            response_bytes += chunk.len() as u64;
            if capture_response {
                captured.extend_from_slice(&chunk);
            }
            yield Ok::<Bytes, std::io::Error>(chunk);
        }
        let _ = db.record(&RequestRecord {
            application_id: &application.id,
            provider_id: Some(&provider.id),
            model: &model,
            endpoint: &endpoint,
            modality: &modality,
            status: status.as_u16(),
            tokens_in: 0,
            tokens_out: 0,
            duration_ms: started.elapsed().as_millis() as u64,
            request_bytes: request_body.len() as u64,
            response_bytes,
            error_type: (status.as_u16() >= 400).then_some("upstream_error"),
        });
        if capture_response {
            let _ = conversations.log(
                &application.id,
                &provider.id,
                &endpoint,
                &request_body,
                &captured,
            );
        }
    };
    response_with_headers(status, &response_headers, Body::from_stream(stream))
}

#[allow(clippy::too_many_arguments)]
fn record(
    state: &HandlerState,
    application: &Application,
    provider: Option<&Provider>,
    model: &str,
    endpoint: &str,
    modality: &str,
    status: StatusCode,
    started: Instant,
    request_bytes: usize,
    response_bytes: usize,
    error_type: Option<&str>,
    tokens_in: u64,
    tokens_out: u64,
) {
    let _ = state.db.record(&RequestRecord {
        application_id: &application.id,
        provider_id: provider.map(|value| value.id.as_str()),
        model,
        endpoint,
        modality,
        status: status.as_u16(),
        tokens_in,
        tokens_out,
        duration_ms: started.elapsed().as_millis() as u64,
        request_bytes: request_bytes as u64,
        response_bytes: response_bytes as u64,
        error_type,
    });
}

fn endpoint(path: &str) -> Option<&'static str> {
    let path = path.split('?').next().unwrap_or(path);
    [
        "/models",
        "/chat/completions",
        "/completions",
        "/responses",
        "/embeddings",
        "/images/generations",
        "/images/edits",
        "/images/variations",
        "/audio/speech",
        "/audio/transcriptions",
        "/audio/translations",
    ]
    .into_iter()
    .find(|candidate| path == *candidate || path.strip_prefix("/v1") == Some(*candidate))
}

fn modality(endpoint: &str) -> &'static str {
    if endpoint.starts_with("/audio") {
        "audio"
    } else if endpoint.starts_with("/images") {
        "image"
    } else if endpoint == "/embeddings" {
        "embedding"
    } else if endpoint == "/models" {
        "model"
    } else {
        "text"
    }
}

fn supports(kind: &str, endpoint: &str) -> bool {
    match kind {
        "anthropic" => matches!(endpoint, "/models" | "/chat/completions"),
        "google" => matches!(endpoint, "/models" | "/chat/completions"),
        _ => true,
    }
}

fn usage(body: &[u8]) -> (u64, u64) {
    let parsed: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let usage = parsed.get("usage").cloned().unwrap_or(Value::Null);
    (
        usage
            .get("prompt_tokens")
            .or_else(|| usage.get("input_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        usage
            .get("completion_tokens")
            .or_else(|| usage.get("output_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    )
}

fn provider_secret(id: &str) -> Option<String> {
    keyring::Entry::new("agenthub", id)
        .ok()?
        .get_password()
        .ok()
}

fn internal(_error: impl std::fmt::Display) -> ProxyError {
    ProxyError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Agenthub encountered an internal error",
        "internal_error",
    )
}

fn response_with_headers(
    status: StatusCode,
    headers: &HeaderMap,
    body: Body,
) -> Result<Response, ProxyError> {
    let mut response = Response::builder().status(status);
    for (name, value) in headers {
        if !matches!(
            name.as_str(),
            "content-length" | "content-encoding" | "transfer-encoding" | "connection"
        ) {
            response = response.header(name, value);
        }
    }
    response.body(body).map_err(internal)
}

fn json_bytes(status: StatusCode, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("valid static response")
}

fn json_response(status: StatusCode, value: Value) -> Response {
    json_bytes(status, value.to_string().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(kind: &str) -> Provider {
        Provider {
            id: "provider".into(),
            name: "Provider".into(),
            kind: kind.into(),
            base_url: "https://example.test/v1".into(),
            default_model: "model".into(),
            is_default: true,
            has_secret: true,
        }
    }

    #[test]
    fn recognizes_supported_routes_and_modalities() {
        assert_eq!(
            endpoint("/v1/chat/completions?beta=true"),
            Some("/chat/completions")
        );
        assert_eq!(endpoint("/unknown"), None);
        assert_eq!(modality("/audio/speech"), "audio");
        assert_eq!(modality("/images/generations"), "image");
        assert_eq!(modality("/embeddings"), "embedding");
        assert!(!supports("anthropic", "/images/generations"));
    }

    #[test]
    fn builds_passthrough_and_azure_urls() {
        let request = adapt_request(
            &provider("openai"),
            "/v1/responses",
            "model",
            "key",
            &json!({}),
            b"{}",
        )
        .unwrap();
        assert_eq!(request.url, "https://example.test/v1/responses");
        let request = adapt_request(
            &provider("azure"),
            "/v1/chat/completions",
            "deployment",
            "key",
            &json!({}),
            b"{}",
        )
        .unwrap();
        assert!(request
            .url
            .contains("deployments/deployment/chat/completions"));
        assert!(request.url.contains("api-version="));
    }

    #[test]
    fn translates_anthropic_and_google_chat() {
        let anthropic = provider("anthropic");
        let request = adapt_request(
            &anthropic,
            "/v1/chat/completions",
            "model",
            "key",
            &json!({"messages":[{"role":"system","content":"safe"},{"role":"user","content":"hi"}]}),
            b"",
        )
        .unwrap();
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["system"], "safe");
        assert_eq!(body["messages"][0]["content"], "hi");
        let response = adapt_response(
            &anthropic,
            "/v1/chat/completions",
            200,
            br#"{"content":[{"text":"ok"}],"usage":{"input_tokens":1,"output_tokens":2}}"#,
        );
        assert_eq!(usage(&response), (1, 2));

        let google = provider("google");
        let request = adapt_request(
            &google,
            "/v1/chat/completions",
            "model",
            "key",
            &json!({"messages":[{"role":"assistant","content":"hello"}]}),
            b"",
        )
        .unwrap();
        assert!(request.url.contains("generateContent?key=key"));
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).unwrap()["contents"][0]["role"],
            "model"
        );
    }

    #[tokio::test]
    async fn starts_authenticates_and_restarts_application_listener() {
        let directory = tempfile::tempdir().unwrap();
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let database = Arc::new(Database::open(&directory.path().join("gateway.db")).unwrap());
        let application = Application {
            id: "application".into(),
            name: "Test".into(),
            port,
            api_key: "secret".into(),
            provider_id: None,
            enabled: true,
        };
        database.save_application(&application).unwrap();
        let mut config = Config::default();
        config.server.port_start = port;
        config.server.port_end = port;
        let manager = ProxyManager::new(database, config, directory.path());
        assert_eq!(
            manager.free_port().await.unwrap_err().to_string(),
            "No free ports in the configured range"
        );

        manager.start(application.clone()).await.unwrap();
        let url = format!("http://127.0.0.1:{port}/v1/models");
        let unauthorized = reqwest::Client::new().get(&url).send().await.unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let unavailable = reqwest::Client::new()
            .get(&url)
            .bearer_auth("secret")
            .send()
            .await
            .unwrap();
        assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);

        manager.start(application).await.unwrap();
        let restarted = reqwest::Client::new()
            .get(url)
            .bearer_auth("secret")
            .send()
            .await
            .unwrap();
        assert_eq!(restarted.status(), StatusCode::SERVICE_UNAVAILABLE);
        manager.stop("application").await;
        manager.stop("missing").await;
    }
}
