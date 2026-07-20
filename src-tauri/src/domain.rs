use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Codex,
    Claude,
    Cursor,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Working,
    Waiting,
    Completed,
    Attention,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDefinition {
    pub id: ProviderId,
    pub name: &'static str,
    pub short_name: &'static str,
    pub accent: &'static str,
    pub connected: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvent {
    pub id: String,
    pub kind: EventKind,
    pub title: String,
    pub body: String,
    pub time: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub provider: ProviderId,
    pub title: String,
    pub project: String,
    pub branch: String,
    pub model: String,
    pub status: SessionStatus,
    pub elapsed: String,
    pub updated_at: String,
    pub token_count: u64,
    pub summary: String,
    pub events: Vec<SessionEvent>,
    #[serde(skip)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub label: String,
    pub used_percent: u8,
    pub reset_label: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuota {
    pub provider: ProviderId,
    pub plan: String,
    pub account: String,
    pub windows: Vec<QuotaWindow>,
    pub cost_label: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub providers: Vec<ProviderDefinition>,
    pub sessions: Vec<AgentSession>,
    pub quotas: Vec<ProviderQuota>,
    pub refreshed_at: String,
}

#[derive(Debug, Default)]
pub struct ProviderSnapshot {
    pub sessions: Vec<AgentSession>,
    pub quota: Option<ProviderQuota>,
    pub warning: Option<String>,
}
