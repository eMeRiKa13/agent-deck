use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum UiText {
    Raw(String),
    Message {
        key: String,
        #[serde(skip_serializing_if = "Map::is_empty")]
        params: Map<String, Value>,
    },
}

impl UiText {
    pub fn message(key: impl Into<String>) -> Self {
        Self::Message {
            key: key.into(),
            params: Map::new(),
        }
    }

    pub fn message_with(key: impl Into<String>, params: Map<String, Value>) -> Self {
        Self::Message {
            key: key.into(),
            params,
        }
    }

    pub fn as_raw(&self) -> Option<&str> {
        match self {
            Self::Raw(value) => Some(value),
            Self::Message { .. } => None,
        }
    }
}

impl From<String> for UiText {
    fn from(value: String) -> Self {
        Self::Raw(value)
    }
}

impl From<&str> for UiText {
    fn from(value: &str) -> Self {
        Self::Raw(value.to_string())
    }
}

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
    pub error: Option<UiText>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvent {
    pub id: String,
    pub kind: EventKind,
    pub title: UiText,
    pub body: UiText,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub provider: ProviderId,
    pub title: UiText,
    pub project: UiText,
    pub branch: String,
    pub model: String,
    pub status: SessionStatus,
    pub started_at_ms: i64,
    pub updated_at_ms: i64,
    pub token_count: u64,
    pub summary: UiText,
    pub events: Vec<SessionEvent>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub label: UiText,
    pub used_percent: u8,
    pub reset_at_ms: Option<i64>,
    pub reset_text: Option<UiText>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UsageCost {
    Credits { value: f64 },
    ExtraUsage { value: f64, currency: String },
    IncludedOnDemand { included: f64, on_demand: f64 },
    LocalMeasurement { measured_at_ms: i64 },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuota {
    pub provider: ProviderId,
    pub plan: String,
    pub account: UiText,
    pub windows: Vec<QuotaWindow>,
    pub cost: Option<UsageCost>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub providers: Vec<ProviderDefinition>,
    pub sessions: Vec<AgentSession>,
    pub quotas: Vec<ProviderQuota>,
    pub refreshed_at_ms: i64,
}

#[derive(Debug, Default)]
pub struct ProviderSnapshot {
    pub sessions: Vec<AgentSession>,
    pub quota: Option<ProviderQuota>,
    pub warning: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::UsageCost;
    use serde_json::json;

    #[test]
    fn serializes_usage_cost_fields_as_camel_case() {
        let local_measurement = serde_json::to_value(UsageCost::LocalMeasurement {
            measured_at_ms: 1_234,
        })
        .expect("local measurement should serialize");
        assert_eq!(
            local_measurement,
            json!({ "kind": "localMeasurement", "measuredAtMs": 1_234 })
        );

        let cursor_usage = serde_json::to_value(UsageCost::IncludedOnDemand {
            included: 111.73,
            on_demand: 2.5,
        })
        .expect("Cursor usage should serialize");
        assert_eq!(
            cursor_usage,
            json!({
                "kind": "includedOnDemand",
                "included": 111.73,
                "onDemand": 2.5
            })
        );
    }
}
