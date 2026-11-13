use crate::{
    config::Config,
    conversation_log::ConversationLogger,
    db::Database,
    model::{Application, ModelAccess, Provider, RequestRecord},
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

pub(crate) fn parse_provider_models(body: &Value) -> Vec<String> {
    let mut models = body
        .get("data")
        .or_else(|| body.get("models"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            model
                .get("id")
                .or_else(|| model.get("name"))
                .and_then(Value::as_str)
        })
        .filter(|name| !name.is_empty())
        .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    models
}

pub(crate) async fn discover_models(
    client: &reqwest::Client,
    kind: &str,
    base_url: &str,
    key: &str,
) -> Result<Vec<String>, String> {
    let base = base_url.trim_end_matches('/');
    let url = match kind {
        "azure" => format!("{base}/openai/models?api-version=2024-10-21"),
        _ => format!("{base}/models"),
    };
    let mut models = Vec::new();
    let mut cursor = None;
    for _ in 0..100 {
        let mut request = client.get(&url).timeout(std::time::Duration::from_secs(15));
        request = match kind {
            "azure" => request.header("api-key", key),
            "anthropic" => request
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01"),
            "google" => request.query(&[("key", key)]),
            _ if !key.is_empty() => request.bearer_auth(key),
            _ => request,
        };
        if let Some(cursor) = &cursor {
            request = request.query(&[(
                if kind == "google" {
                    "pageToken"
                } else {
                    "after_id"
                },
                cursor,
            )]);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "Could not connect to the provider model endpoint".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "The provider model endpoint returned status {}",
                response.status().as_u16()
            ));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "The provider returned an invalid model list".to_string())?;
        models.extend(parse_provider_models(&body));
        cursor = match kind {
            "google" => body
                .get("nextPageToken")
                .and_then(Value::as_str)
                .map(str::to_owned),
            "anthropic" if body.get("has_more").and_then(Value::as_bool) == Some(true) => Some(
                body.get("last_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "The provider returned an invalid model page".to_string())?
                    .to_owned(),
            ),
            _ => None,
        };
        if cursor.is_none() {
            models.sort();
            models.dedup();
            return if models.is_empty() {
                Err("The provider returned no models".into())
            } else {
                Ok(models)
            };
        }
    }
    Err("The provider returned too many model pages".into())
}

async fn enabled_models(
    state: &HandlerState,
    application: &Application,
    provider: &Provider,
    key: &str,
) -> Result<Vec<String>, ProxyError> {
    match application.model_access {
        ModelAccess::DefaultOnly => Ok(vec![provider.default_model.clone()]),
        ModelAccess::Selected => Ok(application.allowed_models.clone()),
        ModelAccess::All => discover_models(&state.client, &provider.kind, &provider.base_url, key)
            .await
            .map_err(|message| {
                ProxyError::new(StatusCode::BAD_GATEWAY, message, "model_discovery_failed")
            }),
    }
}

async fn request_model(
    headers: &HeaderMap,
    body: &[u8],
    default_model: &str,
) -> Result<(Value, String, Vec<u8>), ProxyError> {
    let invalid = || {
        ProxyError::new(
            StatusCode::BAD_REQUEST,
            "Send a JSON object or multipart form with a valid model ID",
            "invalid_request_error",
        )
    };
    let content_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if content_type.starts_with("multipart/form-data") {
        let boundary = multer::parse_boundary(content_type).map_err(|_| invalid())?;
        let bytes = Bytes::copy_from_slice(body);
        let stream = futures_util::stream::once(async move { Ok::<_, std::io::Error>(bytes) });
        let mut multipart = multer::Multipart::new(stream, boundary);
        let mut model = None;
        while let Some(field) = multipart.next_field().await.map_err(|_| invalid())? {
            if field.name() == Some("model") {
                if model.is_some() {
                    return Err(invalid());
                }
                let bytes = field.bytes().await.map_err(|_| invalid())?;
                model = Some(String::from_utf8(bytes.to_vec()).map_err(|_| invalid())?);
            }
        }
        let model = model
            .filter(|model| !model.trim().is_empty())
            .ok_or_else(invalid)?;
        return Ok((Value::Null, model, body.to_vec()));
    }
    let mut json: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let object = json.as_object_mut().ok_or_else(invalid)?;
    let model = match object.get("model") {
        None => default_model.to_owned(),
        Some(Value::String(model)) if !model.trim().is_empty() => model.clone(),
        _ => return Err(invalid()),
    };
    object.insert("model".into(), json!(model));
    let encoded = serde_json::to_vec(&json).map_err(internal)?;
    Ok((json, model, encoded))
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
    let models = enabled_models(state, &application, &provider, &key)
        .await
        .inspect_err(|_| {
            record(
                state,
                &application,
                Some(&provider),
                "",
                endpoint,
                modality,
                StatusCode::BAD_GATEWAY,
                started,
                body.len(),
                0,
                Some("model_discovery_failed"),
                0,
                0,
            );
        })?;
    if endpoint == "/models" && method == Method::GET {
        let response_body = serde_json::to_vec(&json!({
            "object":"list",
            "data":models.iter().map(|model| json!({"id":model,"object":"model","owned_by":provider.name})).collect::<Vec<_>>()
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

    let (original_json, model, request_body) =
        request_model(&headers, body, &provider.default_model).await?;
    if !models.contains(&model) {
        record(
            state,
            &application,
            Some(&provider),
            &model,
            endpoint,
            modality,
            StatusCode::FORBIDDEN,
            started,
            body.len(),
            0,
            Some("model_not_allowed"),
            0,
            0,
        );
        return Err(ProxyError::new(
            StatusCode::FORBIDDEN,
            "This model is not enabled for this application",
            "model_not_allowed",
        ));
    }
    let wants_stream = original_json.get("stream").and_then(Value::as_bool) == Some(true);
    let adapted = adapt_request(&provider, path, &model, &key, &original_json, &request_body)?;
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
            body: passthrough_body(path, json_body, raw),
            translated: false,
        }),
        _ => Ok(AdaptedRequest {
            url: format!("{}{}", provider.base_url.trim_end_matches('/'), suffix),
            body: passthrough_body(path, json_body, raw),
            translated: false,
        }),
    }
}

fn passthrough_body(path: &str, json_body: &Value, raw: &[u8]) -> Vec<u8> {
    let path = path.split('?').next().unwrap_or(path);
    let endpoint = path.strip_prefix("/v1").unwrap_or(path);
    if endpoint != "/chat/completions"
        || json_body.get("stream").and_then(Value::as_bool) != Some(true)
    {
        return raw.to_vec();
    }

    let Some(mut body) = json_body.as_object().cloned() else {
        return raw.to_vec();
    };
    if let Some(Value::Object(options)) = body.get_mut("stream_options") {
        options.insert("include_usage".into(), json!(true));
    } else {
        body.insert("stream_options".into(), json!({"include_usage": true}));
    }
    serde_json::to_vec(&Value::Object(body)).unwrap_or_else(|_| raw.to_vec())
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
        let mut stream_usage = StreamingUsage::default();
        while let Some(item) = upstream_stream.next().await {
            let chunk = match item {
                Ok(chunk) => chunk,
                Err(error) => {
                    let (tokens_in, tokens_out) = stream_usage.finish();
                    let _ = db.record(&RequestRecord {
                        application_id: &application.id,
                        provider_id: Some(&provider.id),
                        model: &model,
                        endpoint: &endpoint,
                        modality: &modality,
                        status: StatusCode::BAD_GATEWAY.as_u16(),
                        tokens_in,
                        tokens_out,
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
            stream_usage.feed(&chunk);
            if capture_response {
                captured.extend_from_slice(&chunk);
            }
            yield Ok::<Bytes, std::io::Error>(chunk);
        }
        let (tokens_in, tokens_out) = stream_usage.finish();
        let _ = db.record(&RequestRecord {
            application_id: &application.id,
            provider_id: Some(&provider.id),
            model: &model,
            endpoint: &endpoint,
            modality: &modality,
            status: status.as_u16(),
            tokens_in,
            tokens_out,
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
    let mut counts = (None, None);
    if let Ok(parsed) = serde_json::from_slice::<Value>(body) {
        update_usage(&mut counts, token_counts(&parsed));
    } else {
        if body.starts_with(b"data:") || body.starts_with(b"event:") || body.starts_with(b":") {
            let mut stream = StreamingUsage::default();
            stream.feed(body);
            let streamed = stream.finish();
            if streamed != (0, 0) {
                return streamed;
            }
        }
        for line in body.split(|byte| *byte == b'\n') {
            if let Ok(parsed) = serde_json::from_slice::<Value>(line) {
                update_usage(&mut counts, token_counts(&parsed));
            }
        }
    }
    (counts.0.unwrap_or(0), counts.1.unwrap_or(0))
}

#[derive(Default)]
struct StreamingUsage {
    buffer: Vec<u8>,
    tokens_in: Option<u64>,
    tokens_out: Option<u64>,
}

impl StreamingUsage {
    fn feed(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        while let Some(end) = sse_event_end(&self.buffer) {
            let event = self.buffer.drain(..end).collect::<Vec<_>>();
            self.update(&event);
        }
    }

    fn finish(&mut self) -> (u64, u64) {
        if !self.buffer.is_empty() {
            let event = std::mem::take(&mut self.buffer);
            self.update(&event);
        }
        (self.tokens_in.unwrap_or(0), self.tokens_out.unwrap_or(0))
    }

    fn update(&mut self, event: &[u8]) {
        let mut data = Vec::new();
        for line in event.split(|byte| *byte == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if let Some(value) = line.strip_prefix(b"data:") {
                if !data.is_empty() {
                    data.push(b'\n');
                }
                data.extend_from_slice(value.strip_prefix(b" ").unwrap_or(value));
            }
        }
        if let Ok(value) = serde_json::from_slice::<Value>(&data) {
            let mut counts = (self.tokens_in, self.tokens_out);
            update_usage(&mut counts, token_counts(&value));
            (self.tokens_in, self.tokens_out) = counts;
        }
    }
}

fn sse_event_end(buffer: &[u8]) -> Option<usize> {
    let lf_end = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| position + 2);
    let crlf_end = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4);
    lf_end.into_iter().chain(crlf_end).min()
}

fn token_counts(value: &Value) -> Option<(Option<u64>, Option<u64>)> {
    let usage_objects = [
        value.get("usage"),
        value.get("usageMetadata"),
        value.pointer("/response/usage"),
        value.pointer("/data/usage"),
    ];
    for usage in usage_objects.into_iter().flatten() {
        let input = usage
            .get("prompt_tokens")
            .or_else(|| usage.get("input_tokens"))
            .or_else(|| usage.get("promptTokenCount"))
            .and_then(Value::as_u64);
        let output = usage
            .get("completion_tokens")
            .or_else(|| usage.get("output_tokens"))
            .or_else(|| usage.get("candidatesTokenCount"))
            .and_then(Value::as_u64);
        if input.is_some() || output.is_some() {
            return Some((input, output));
        }
    }

    let input = value.get("prompt_eval_count").and_then(Value::as_u64);
    let output = value.get("eval_count").and_then(Value::as_u64);
    (input.is_some() || output.is_some()).then_some((input, output))
}

fn update_usage(
    current: &mut (Option<u64>, Option<u64>),
    next: Option<(Option<u64>, Option<u64>)>,
) {
    if let Some((tokens_in, tokens_out)) = next {
        if tokens_in.is_some() {
            current.0 = tokens_in;
        }
        if tokens_out.is_some() {
            current.1 = tokens_out;
        }
    }
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

    #[tokio::test]
    async fn parses_and_validates_request_models() {
        let headers = HeaderMap::new();
        let (_, model, encoded) = request_model(&headers, br#"{"messages":[]}"#, "default")
            .await
            .unwrap();
        assert_eq!(model, "default");
        assert_eq!(
            serde_json::from_slice::<Value>(&encoded).unwrap()["model"],
            "default"
        );
        for body in [
            b"invalid".as_slice(),
            b"[]",
            br#"{"model":null}"#,
            br#"{"model":1}"#,
            br#"{"model":""}"#,
            br#"{"model":" "}"#,
        ] {
            assert_eq!(
                request_model(&headers, body, "default")
                    .await
                    .unwrap_err()
                    .status,
                StatusCode::BAD_REQUEST
            );
        }
        let (_, model, encoded) =
            request_model(&headers, br#"{"model":"first","model":"last"}"#, "default")
                .await
                .unwrap();
        assert_eq!(model, "last");
        assert_eq!(
            serde_json::from_slice::<Value>(&encoded).unwrap()["model"],
            "last"
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            "multipart/form-data; boundary=test".parse().unwrap(),
        );
        let body = b"--test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n\r\nbinary\r\n--test\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nspeech\r\n--test--\r\n";
        let (json, model, encoded) = request_model(&headers, body, "default").await.unwrap();
        assert_eq!(json, Value::Null);
        assert_eq!(model, "speech");
        assert_eq!(encoded, body);
        for body in [
            b"--test--\r\n".as_slice(),
            b"--test\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n\r\n--test--\r\n",
            b"--test\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\na\r\n--test\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nb\r\n--test--\r\n",
            b"malformed",
            b"--test\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n\xff\r\n--test--\r\n",
        ] {
            assert!(request_model(&headers, body, "default").await.is_err());
        }
        headers.insert("content-type", "multipart/form-data".parse().unwrap());
        assert!(request_model(&headers, body, "default").await.is_err());
    }

    #[tokio::test]
    async fn discovers_provider_models_with_authentication_pagination_and_errors() {
        let router = Router::new().fallback(axum::routing::get(
            |OriginalUri(uri): OriginalUri, headers: HeaderMap| async move {
                let path = uri.path();
                let query = uri.query().unwrap_or("");
                let response = if path.starts_with("/status/") {
                    return (StatusCode::UNAUTHORIZED, "denied".to_string());
                } else if path.starts_with("/invalid/") {
                    return (StatusCode::OK, "not json".to_string());
                } else if path.starts_with("/empty/") {
                    json!({"data":[]})
                } else if path.starts_with("/missing-cursor/") {
                    json!({"data":[{"id":"one"}],"has_more":true})
                } else if path.starts_with("/infinite/") {
                    json!({"models":[{"name":"models/one"}],"nextPageToken":"again"})
                } else if path.starts_with("/google/") {
                    assert!(query.contains("key=secret"));
                    assert!(headers.get("authorization").is_none());
                    if query.contains("pageToken=next") {
                        json!({"models":[{"name":"models/two"}]})
                    } else {
                        json!({"models":[{"name":"models/one"}],"nextPageToken":"next"})
                    }
                } else if path.starts_with("/anthropic/") {
                    assert_eq!(headers["x-api-key"], "secret");
                    assert_eq!(headers["anthropic-version"], "2023-06-01");
                    if query.contains("after_id=one") {
                        json!({"data":[{"id":"two"}],"has_more":false})
                    } else {
                        json!({"data":[{"id":"one"}],"has_more":true,"last_id":"one"})
                    }
                } else if path.starts_with("/azure/") {
                    assert_eq!(headers["api-key"], "secret");
                    assert_eq!(path, "/azure/openai/models");
                    assert_eq!(query, "api-version=2024-10-21");
                    json!({"data":[{"id":"one"},{"id":"two"}]})
                } else {
                    if path.starts_with("/openai/") {
                        assert_eq!(headers["authorization"], "Bearer secret");
                    } else {
                        assert!(headers.get("authorization").is_none());
                    }
                    json!({"data":[{"id":"two"},{"id":"one"},{"id":"one"},{},{"id":""},{"id":12}]})
                };
                (StatusCode::OK, response.to_string())
            },
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        for kind in ["openai", "anthropic", "google", "azure", "compatible"] {
            let models = discover_models(
                &client,
                kind,
                &format!("http://{address}/{kind}/"),
                if kind == "compatible" { "" } else { "secret" },
            )
            .await
            .unwrap();
            assert_eq!(models, vec!["one", "two"]);
        }
        for (path, kind, message) in [
            ("status", "compatible", "status 401"),
            ("invalid", "compatible", "invalid model list"),
            ("empty", "compatible", "no models"),
            ("missing-cursor", "anthropic", "invalid model page"),
            ("infinite", "google", "too many model pages"),
        ] {
            let error = discover_models(&client, kind, &format!("http://{address}/{path}"), "")
                .await
                .unwrap_err();
            assert!(error.contains(message), "{error}");
            assert!(!error.contains("secret"));
        }
        assert!(discover_models(&client, "compatible", "not a URL", "")
            .await
            .unwrap_err()
            .contains("Could not connect"));
        task.abort();
        let _ = task.await;
    }

    #[tokio::test]
    async fn enforces_model_access_on_every_endpoint() {
        let upstream = Router::new()
            .route(
                "/v1/models",
                axum::routing::get(|| async {
                    axum::Json(json!({"data":[{"id":"model"},{"id":"other"}]}))
                }),
            )
            .route(
                "/v1/chat/completions",
                axum::routing::post(|axum::Json(body): axum::Json<Value>| async {
                    axum::Json(body)
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let directory = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(&directory.path().join("test.db")).unwrap());
        let mut provider = provider("compatible");
        provider.base_url = format!("http://{address}/v1");
        db.save_provider(&provider).unwrap();
        let mut application = Application {
            id: "test".into(),
            name: "Test".into(),
            port: 43100,
            api_key: "secret".into(),
            provider_id: None,
            enabled: true,
            model_access: ModelAccess::DefaultOnly,
            allowed_models: vec![],
        };
        db.save_application(&application).unwrap();
        let state = HandlerState {
            app_id: application.id.clone(),
            db: db.clone(),
            client: reqwest::Client::new(),
            conversations: ConversationLogger::new(&Config::default().privacy, directory.path()),
        };
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer secret".parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());
        let list = |response: Response| async {
            serde_json::from_slice::<Value>(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap()
        };
        let response = forward_inner(&state, "/v1/models", Method::GET, headers.clone(), b"")
            .await
            .unwrap();
        assert_eq!(list(response).await["data"][0]["id"], "model");
        for endpoint in [
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
        ] {
            let error = forward_inner(
                &state,
                endpoint,
                Method::POST,
                headers.clone(),
                br#"{"model":"other","stream":true}"#,
            )
            .await
            .unwrap_err();
            assert_eq!(error.status, StatusCode::FORBIDDEN);
            assert_eq!(error.kind, "model_not_allowed");
        }
        let response = forward_inner(
            &state,
            "/v1/chat/completions",
            Method::POST,
            headers.clone(),
            b"{}",
        )
        .await
        .unwrap();
        assert_eq!(list(response).await["model"], "model");
        application.model_access = ModelAccess::Selected;
        application.allowed_models = vec!["other".into()];
        db.save_application(&application).unwrap();
        let response = forward_inner(&state, "/models", Method::GET, headers.clone(), b"")
            .await
            .unwrap();
        assert_eq!(
            list(response).await["data"],
            json!([{"id":"other","object":"model","owned_by":provider.name}])
        );
        assert_eq!(
            forward_inner(
                &state,
                "/chat/completions",
                Method::POST,
                headers.clone(),
                b"{}"
            )
            .await
            .unwrap_err()
            .status,
            StatusCode::FORBIDDEN
        );
        let response = forward_inner(
            &state,
            "/v1/chat/completions",
            Method::POST,
            headers.clone(),
            br#"{"model":"other"}"#,
        )
        .await
        .unwrap();
        assert_eq!(list(response).await["model"], "other");
        let mut multipart_headers = headers.clone();
        multipart_headers.insert(
            "content-type",
            "multipart/form-data; boundary=test".parse().unwrap(),
        );
        let denied = forward_inner(
            &state,
            "/v1/audio/transcriptions",
            Method::POST,
            multipart_headers,
            b"--test\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nmodel\r\n--test--\r\n",
        )
        .await
        .unwrap_err();
        assert_eq!(denied.status, StatusCode::FORBIDDEN);
        application.allowed_models.clear();
        db.save_application(&application).unwrap();
        let response = forward_inner(&state, "/models", Method::GET, headers.clone(), b"")
            .await
            .unwrap();
        assert_eq!(list(response).await["data"], json!([]));
        assert!(forward_inner(
            &state,
            "/chat/completions",
            Method::POST,
            headers.clone(),
            br#"{"model":"other"}"#
        )
        .await
        .is_err());
        application.model_access = ModelAccess::All;
        db.save_application(&application).unwrap();
        let response = forward_inner(&state, "/models", Method::GET, headers.clone(), b"")
            .await
            .unwrap();
        assert_eq!(list(response).await["data"].as_array().unwrap().len(), 2);
        assert_eq!(
            forward_inner(
                &state,
                "/chat/completions",
                Method::POST,
                headers.clone(),
                br#"{"model":"unknown"}"#
            )
            .await
            .unwrap_err()
            .status,
            StatusCode::FORBIDDEN
        );
        let response = forward_inner(
            &state,
            "/v1/chat/completions",
            Method::POST,
            headers.clone(),
            br#"{"model":"other"}"#,
        )
        .await
        .unwrap();
        assert_eq!(list(response).await["model"], "other");
        task.abort();
        let _ = task.await;
        assert_eq!(
            forward_inner(&state, "/models", Method::GET, headers, b"")
                .await
                .unwrap_err()
                .kind,
            "model_discovery_failed"
        );
    }

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
    fn requests_usage_for_streaming_chat_completions() {
        let request = adapt_request(
            &provider("openai"),
            "/v1/chat/completions?trace=true",
            "model",
            "key",
            &json!({
                "model":"model",
                "stream":true,
                "stream_options":{"custom":true}
            }),
            br#"{"model":"model","stream":true,"stream_options":{"custom":true}}"#,
        )
        .unwrap();
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["stream_options"]["custom"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);

        let non_streaming = adapt_request(
            &provider("openai"),
            "/v1/chat/completions",
            "model",
            "key",
            &json!({"stream":false}),
            b"{\"stream\":false}",
        )
        .unwrap();
        assert_eq!(non_streaming.body, b"{\"stream\":false}");
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

    #[test]
    fn reads_streaming_and_provider_specific_usage() {
        let mut stream = StreamingUsage::default();
        stream.feed(b"data: {\"choices\":[]}\n\n");
        stream.feed(b"data: {\"usage\":{\"prompt_tokens\":12");
        stream.feed(b",\"completion_tokens\":7}}\n\n");
        stream.feed(b"data: [DONE]\n\n");
        assert_eq!(stream.finish(), (12, 7));

        let mut response_stream = StreamingUsage::default();
        response_stream.feed(
            b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":4,\"output_tokens\":9}}}\r\n\r\n",
        );
        assert_eq!(response_stream.finish(), (4, 9));
        assert_eq!(
            usage(b"data: {\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":3}}\n\n"),
            (2, 3)
        );

        assert_eq!(usage(b"{\"prompt_eval_count\":3,\"eval_count\":5}"), (3, 5));
        assert_eq!(
            usage(b"{\"response\":{\"usage\":{\"input_tokens\":6,\"output_tokens\":8}}}"),
            (6, 8)
        );
        assert_eq!(usage(b"not json"), (0, 0));
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
            model_access: ModelAccess::DefaultOnly,
            allowed_models: vec![],
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
