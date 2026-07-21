//! Shared serialization model for the Rust providers and the Vue dashboard IPC boundary.

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
    use super::*;
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

    #[test]
    fn serializes_the_complete_dashboard_contract() {
        let snapshot = DashboardSnapshot {
            providers: vec![ProviderDefinition {
                id: ProviderId::Codex,
                name: "Codex",
                short_name: "CX",
                accent: "#111111",
                connected: true,
                error: Some(UiText::message("errors.providerPartial")),
            }],
            sessions: vec![AgentSession {
                id: "codex:thread-1".to_string(),
                provider: ProviderId::Codex,
                title: UiText::from("Dashboard contract"),
                project: UiText::message("generated.localProject"),
                branch: "main".to_string(),
                model: "gpt-5".to_string(),
                status: SessionStatus::Working,
                started_at_ms: 1_000,
                updated_at_ms: 2_000,
                token_count: 42,
                summary: UiText::from("Contract fixture"),
                events: vec![SessionEvent {
                    id: "event-1".to_string(),
                    kind: EventKind::Assistant,
                    title: UiText::from("Codex"),
                    body: UiText::from("Ready"),
                    timestamp_ms: 2_000,
                }],
            }],
            quotas: vec![ProviderQuota {
                provider: ProviderId::Codex,
                plan: "Plus".to_string(),
                account: UiText::from("codex@example.com"),
                windows: vec![QuotaWindow {
                    id: "codex-global".to_string(),
                    label: UiText::message("quota.global"),
                    used_percent: 43,
                    reset_at_ms: Some(3_000),
                    reset_text: None,
                }],
                cost: Some(UsageCost::IncludedOnDemand {
                    included: 12.5,
                    on_demand: 1.25,
                }),
            }],
            refreshed_at_ms: 2_500,
        };

        let serialized =
            serde_json::to_value(snapshot).expect("dashboard snapshot should serialize");

        assert_eq!(
            serialized,
            json!({
                "providers": [{
                    "id": "codex",
                    "name": "Codex",
                    "shortName": "CX",
                    "accent": "#111111",
                    "connected": true,
                    "error": {"key": "errors.providerPartial"}
                }],
                "sessions": [{
                    "id": "codex:thread-1",
                    "provider": "codex",
                    "title": "Dashboard contract",
                    "project": {"key": "generated.localProject"},
                    "branch": "main",
                    "model": "gpt-5",
                    "status": "working",
                    "startedAtMs": 1_000,
                    "updatedAtMs": 2_000,
                    "tokenCount": 42,
                    "summary": "Contract fixture",
                    "events": [{
                        "id": "event-1",
                        "kind": "assistant",
                        "title": "Codex",
                        "body": "Ready",
                        "timestampMs": 2_000
                    }]
                }],
                "quotas": [{
                    "provider": "codex",
                    "plan": "Plus",
                    "account": "codex@example.com",
                    "windows": [{
                        "id": "codex-global",
                        "label": {"key": "quota.global"},
                        "usedPercent": 43,
                        "resetAtMs": 3_000,
                        "resetText": null
                    }],
                    "cost": {
                        "kind": "includedOnDemand",
                        "included": 12.5,
                        "onDemand": 1.25
                    }
                }],
                "refreshedAtMs": 2_500
            })
        );
    }
}
