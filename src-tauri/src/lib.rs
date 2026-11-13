mod config;
mod conversation_log;
mod db;
mod model;
mod provider_stats;
mod proxy;

use db::Database;
use model::*;
use proxy::ProxyManager;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use serde::Serialize;
use std::sync::Arc;
use tauri::{Manager, State};
use uuid::Uuid;

struct AppState {
    db: Arc<Database>,
    proxy: Arc<ProxyManager>,
    config_path: std::path::PathBuf,
    config: config::Config,
}

type Result<T> = std::result::Result<T, String>;

fn safe_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn has_secret(id: &str) -> bool {
    keyring::Entry::new("agenthub", id)
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .is_some()
}

#[tauri::command]
fn list_providers(state: State<AppState>) -> Result<Vec<Provider>> {
    let mut providers = state.db.providers().map_err(safe_error)?;
    for provider in &mut providers {
        provider.has_secret = has_secret(&provider.id);
    }
    Ok(providers)
}

#[tauri::command]
fn save_provider(state: State<AppState>, provider: ProviderInput) -> Result<Provider> {
    validate_provider(&provider)?;
    let id = if provider.id.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        provider.id
    };
    if let Some(key) = provider.api_key.filter(|key| !key.is_empty()) {
        keyring::Entry::new("agenthub", &id)
            .map_err(safe_error)?
            .set_password(&key)
            .map_err(safe_error)?;
    }
    let row = Provider {
        id: id.clone(),
        name: provider.name.trim().to_string(),
        kind: provider.kind,
        base_url: provider.base_url.trim_end_matches('/').to_string(),
        default_model: provider.default_model.trim().to_string(),
        is_default: provider.is_default,
        has_secret: has_secret(&id),
    };
    state.db.save_provider(&row).map_err(safe_error)?;
    let mut saved = state
        .db
        .provider(&id)
        .map_err(safe_error)?
        .ok_or_else(|| "Provider was not saved".to_string())?;
    saved.has_secret = has_secret(&id);
    Ok(saved)
}

#[tauri::command]
async fn list_provider_models(provider: ProviderInput) -> Result<Vec<String>> {
    validate_provider_connection(&provider)?;
    let key = provider
        .api_key
        .as_deref()
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            (!provider.id.is_empty())
                .then(|| provider_secret(&provider.id))
                .flatten()
        })
        .unwrap_or_default();
    if key.is_empty() && !matches!(provider.kind.as_str(), "ollama" | "compatible") {
        return Err("Enter an API key before loading models".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| "Could not initialize the provider connection".to_string())?;
    proxy::discover_models(&client, &provider.kind, &provider.base_url, &key).await
}

fn provider_secret(id: &str) -> Option<String> {
    keyring::Entry::new("agenthub", id)
        .ok()
        .and_then(|entry| entry.get_password().ok())
}

#[tauri::command]
fn delete_provider(state: State<AppState>, id: String) -> Result<()> {
    state.db.delete_provider(&id).map_err(safe_error)?;
    if let Ok(entry) = keyring::Entry::new("agenthub", &id) {
        let _ = entry.delete_credential();
    }
    Ok(())
}

#[tauri::command]
fn set_default_provider(state: State<AppState>, id: String) -> Result<()> {
    state.db.set_default(&id).map_err(safe_error)
}

#[tauri::command]
fn list_applications(state: State<AppState>) -> Result<Vec<Application>> {
    state.db.applications().map_err(safe_error)
}

#[tauri::command]
async fn save_application(
    state: State<'_, AppState>,
    application: ApplicationInput,
) -> Result<Application> {
    if application.name.trim().is_empty() {
        return Err("Application name is required".into());
    }
    if let Some(provider_id) = application.provider_id.as_deref() {
        if state
            .db
            .provider(provider_id)
            .map_err(safe_error)?
            .is_none()
        {
            return Err("The selected provider no longer exists".into());
        }
    }
    let existing = match application.id.as_deref() {
        Some(id) => state.db.application(id).map_err(safe_error)?,
        None => None,
    };
    let mut row = if let Some(mut current) = existing.clone() {
        current.name = application.name.trim().to_string();
        current.provider_id = application.provider_id;
        current.enabled = application.enabled.unwrap_or(current.enabled);
        current
    } else {
        Application {
            id: Uuid::new_v4().to_string(),
            name: application.name.trim().to_string(),
            port: state.proxy.free_port().await.map_err(safe_error)?,
            api_key: random_key(),
            provider_id: application.provider_id,
            enabled: application.enabled.unwrap_or(true),
            model_access: ModelAccess::DefaultOnly,
            allowed_models: vec![],
        }
    };
    if let Some(access) = application.model_access {
        row.model_access = access;
    }
    if let Some(models) = application.allowed_models {
        row.allowed_models = models;
    }
    row.allowed_models = normalize_model_selection(row.allowed_models)?;
    state.db.save_application(&row).map_err(safe_error)?;
    if row.enabled {
        if let Err(error) = state.proxy.start(row.clone()).await {
            if let Some(previous) = existing {
                let _ = state.db.save_application(&previous);
                let _ = state.proxy.start(previous).await;
            } else {
                let _ = state.db.delete_application(&row.id);
            }
            return Err(format!("Could not start the local endpoint: {error}"));
        }
    } else {
        state.proxy.stop(&row.id).await;
    }
    Ok(row)
}

fn normalize_model_selection(mut models: Vec<String>) -> Result<Vec<String>> {
    if models.iter().any(|model| model.trim().is_empty()) {
        return Err("Enabled model IDs cannot be empty".into());
    }
    models.sort();
    models.dedup();
    Ok(models)
}

#[tauri::command]
async fn delete_application(state: State<'_, AppState>, id: String) -> Result<()> {
    state.proxy.stop(&id).await;
    state.db.delete_application(&id).map_err(safe_error)
}

#[tauri::command]
async fn regenerate_application_key(state: State<'_, AppState>, id: String) -> Result<Application> {
    let mut application = state
        .db
        .application(&id)
        .map_err(safe_error)?
        .ok_or_else(|| "Application not found".to_string())?;
    let previous = application.clone();
    application.api_key = random_key();
    state
        .db
        .save_application(&application)
        .map_err(safe_error)?;
    if application.enabled {
        if let Err(error) = state.proxy.start(application.clone()).await {
            let _ = state.db.save_application(&previous);
            let _ = state.proxy.start(previous).await;
            return Err(format!("Could not restart the local endpoint: {error}"));
        }
    }
    Ok(application)
}

#[tauri::command]
fn dashboard_stats(state: State<AppState>) -> Result<DashboardStats> {
    state.db.stats().map_err(safe_error)
}

#[tauri::command]
fn list_provider_usage(state: State<AppState>) -> Result<Vec<ProviderUsage>> {
    state.db.provider_usage().map_err(safe_error)
}

#[tauri::command]
fn provider_stats(state: State<AppState>, id: String) -> Result<ProviderStats> {
    state.db.provider_stats(&id).map_err(safe_error)
}

#[tauri::command]
fn set_provider_billing_start_day(state: State<AppState>, id: String, day: u32) -> Result<()> {
    state.db.set_billing_start_day(&id, day).map_err(safe_error)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    log_conversations: bool,
    config_path: String,
    conversation_log_path: String,
    conversation_retention_days: u64,
    request_metadata_days: u64,
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    let config_directory = state
        .config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let log_path = state
        .config
        .privacy
        .conversation_log_directory
        .clone()
        .unwrap_or_else(|| config_directory.join("conversations"));
    Settings {
        log_conversations: state.config.privacy.log_conversations,
        config_path: state.config_path.display().to_string(),
        conversation_log_path: log_path.display().to_string(),
        conversation_retention_days: state.config.privacy.conversation_retention_days,
        request_metadata_days: state.config.retention.request_metadata_days,
    }
}

fn validate_provider(provider: &ProviderInput) -> Result<()> {
    validate_provider_connection(provider)?;
    if provider.default_model.trim().is_empty() {
        return Err("Default model is required".into());
    }
    Ok(())
}

fn validate_provider_connection(provider: &ProviderInput) -> Result<()> {
    const KINDS: &[&str] = &[
        "openai",
        "anthropic",
        "google",
        "azure",
        "aws",
        "openrouter",
        "deepseek",
        "ollama",
        "compatible",
    ];
    if provider.name.trim().is_empty() {
        return Err("Provider name is required".into());
    }
    if !KINDS.contains(&provider.kind.as_str()) {
        return Err("Unsupported provider type".into());
    }
    if !provider.base_url.starts_with("https://")
        && !provider.base_url.starts_with("http://127.0.0.1")
        && !provider.base_url.starts_with("http://localhost")
        && !provider.base_url.starts_with("http://[::1]")
    {
        return Err("Provider URL must use HTTPS or a loopback HTTP address".into());
    }
    Ok(())
}

fn random_key() -> String {
    format!(
        "ah_{}",
        OsRng
            .sample_iter(&Alphanumeric)
            .take(40)
            .map(char::from)
            .collect::<String>()
    )
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = config::config_dir();
            std::fs::create_dir_all(&directory)?;
            let config_path = directory.join("config.toml");
            let config = config::load(&config_path)?;
            let database = Arc::new(Database::open(&directory.join("agenthub.db"))?);
            let proxy = Arc::new(ProxyManager::new(
                database.clone(),
                config.clone(),
                &directory,
            ));
            let startup = proxy.clone();
            tauri::async_runtime::spawn(async move {
                startup.start_all().await;
            });
            app.manage(AppState {
                db: database,
                proxy,
                config_path,
                config,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_providers,
            save_provider,
            list_provider_models,
            delete_provider,
            set_default_provider,
            list_applications,
            save_application,
            delete_application,
            regenerate_application_key,
            dashboard_stats,
            list_provider_usage,
            provider_stats,
            set_provider_billing_start_day,
            get_settings
        ])
        .run(tauri::generate_context!())
        .expect("failed to run agenthub");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_model_selection_and_validates_access_inputs() {
        assert!(normalize_model_selection(vec![]).unwrap().is_empty());
        assert_eq!(
            normalize_model_selection(vec!["two".into(), "one".into(), "two".into()]).unwrap(),
            vec!["one", "two"]
        );
        for model in ["", " ", "\n"] {
            assert!(normalize_model_selection(vec![model.into()]).is_err());
        }
        for mode in ["defaultOnly", "selected", "all"] {
            let input: ApplicationInput = serde_json::from_value(serde_json::json!({
                "name":"Test", "modelAccess":mode, "allowedModels":["one"]
            }))
            .unwrap();
            assert_eq!(
                serde_json::to_value(input.model_access.unwrap()).unwrap(),
                mode
            );
            assert_eq!(input.allowed_models.unwrap(), vec!["one"]);
        }
        let input: ApplicationInput =
            serde_json::from_value(serde_json::json!({"name":"Test"})).unwrap();
        assert!(input.model_access.is_none());
        assert!(input.allowed_models.is_none());
        assert!(
            serde_json::from_value::<ApplicationInput>(serde_json::json!({
                "name":"Test","modelAccess":"unknown"
            }))
            .is_err()
        );
    }

    #[test]
    fn keys_are_unique_and_well_formed() {
        let first = random_key();
        let second = random_key();
        assert!(first.starts_with("ah_"));
        assert_eq!(first.len(), 43);
        assert_ne!(first, second);
    }

    #[test]
    fn validates_provider_inputs() {
        let mut provider = ProviderInput {
            id: String::new(),
            name: "Local".into(),
            kind: "compatible".into(),
            base_url: "http://localhost:11434/v1".into(),
            default_model: "model".into(),
            is_default: false,
            api_key: None,
        };
        assert!(validate_provider(&provider).is_ok());
        provider.kind = "ollama".into();
        assert!(validate_provider(&provider).is_ok());
        provider.base_url = "http://example.com".into();
        assert!(validate_provider(&provider).is_err());
        provider.base_url = "https://example.com".into();
        provider.kind = "unknown".into();
        assert!(validate_provider(&provider).is_err());
    }

    #[test]
    fn parses_provider_model_lists() {
        assert_eq!(
            proxy::parse_provider_models(&serde_json::json!({
                "data": [{"id":"z-model"}, {"id":"a-model"}, {"id":"a-model"}]
            })),
            vec!["a-model", "z-model"]
        );
        assert_eq!(
            proxy::parse_provider_models(&serde_json::json!({
                "models": [{"name":"models/gemini-flash"}]
            })),
            vec!["gemini-flash"]
        );
        assert!(proxy::parse_provider_models(&serde_json::json!({})).is_empty());
    }
}
