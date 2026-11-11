use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub default_model: String,
    pub is_default: bool,
    pub has_secret: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub default_model: String,
    pub is_default: bool,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    pub id: String,
    pub name: String,
    pub port: u16,
    pub api_key: String,
    pub provider_id: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationInput {
    pub id: Option<String>,
    pub name: String,
    pub provider_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestStat {
    pub id: i64,
    pub application: String,
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub modality: String,
    pub status: u16,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub accumulated_tokens_in: u64,
    pub accumulated_tokens_out: u64,
    pub duration_ms: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub error_type: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyStat {
    pub day: String,
    pub requests: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTokenStat {
    pub provider: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub average_tokens_per_second: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSummary {
    pub requests: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub total_tokens: u64,
    pub average_tokens: f64,
    pub average_tokens_per_second: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBucket {
    pub date: String,
    #[serde(flatten)]
    pub usage: TokenSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    pub provider_id: String,
    pub total_tokens: u64,
    pub month_tokens: u64,
    pub period_start: String,
    pub period_end: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStats {
    pub provider_id: String,
    pub provider_name: String,
    pub billing_start_day: u32,
    pub period_start: String,
    pub period_end: String,
    pub current: TokenSummary,
    pub historical: TokenSummary,
    pub daily: Vec<TokenBucket>,
    pub monthly: Vec<TokenBucket>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub requests: u64,
    pub success_rate: f64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub average_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub provider_tokens: Vec<ProviderTokenStat>,
    pub daily: Vec<DailyStat>,
    pub recent: Vec<RequestStat>,
}

#[derive(Clone, Debug)]
pub struct RequestRecord<'a> {
    pub application_id: &'a str,
    pub provider_id: Option<&'a str>,
    pub model: &'a str,
    pub endpoint: &'a str,
    pub modality: &'a str,
    pub status: u16,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub duration_ms: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub error_type: Option<&'a str>,
}
