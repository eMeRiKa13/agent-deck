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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Working,
    Waiting,
    Completed,
    Attention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    User,
    Assistant,
    Tool,
    System,
    Subagent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityStatus {
    Running,
    Completed,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenUsageQuality {
    Detailed,
    TotalOnly,
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
pub struct TokenUsage {
    pub quality: TokenUsageQuality,
    pub total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_input: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_output: Option<u64>,
}

impl TokenUsage {
    pub fn total_only(total: u64) -> Option<Self> {
        (total > 0).then_some(Self {
            quality: TokenUsageQuality::TotalOnly,
            total,
            input: None,
            output: None,
            cached_input: None,
            cache_write_input: None,
            reasoning_output: None,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_error_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_usage_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_turn_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_changed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines_added: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines_removed: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentSummary {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    pub status: SessionStatus,
    pub updated_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "provider",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ProviderSessionDetails {
    Codex {
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_effort: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        collaboration_mode: Option<String>,
        subagents: Vec<SubagentSummary>,
    },
    Claude {
        #[serde(skip_serializing_if = "Option::is_none")]
        effort: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        completed_turns: Option<u32>,
    },
    Cursor {
        #[serde(skip_serializing_if = "Option::is_none")]
        force_mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unified_mode: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: String,
    pub kind: ActivityKind,
    pub title: UiText,
    pub body: UiText,
    pub timestamp_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ActivityStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityGroup {
    pub id: String,
    pub started_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ActivityStatus>,
    pub items: Vec<ActivityItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub provider: ProviderId,
    pub title: UiText,
    pub project: UiText,
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub status: SessionStatus,
    pub started_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    pub metrics: SessionMetrics,
    pub provider_details: ProviderSessionDetails,
    pub summary: UiText,
    pub activity_groups: Vec<ActivityGroup>,
    pub activity_truncated: bool,
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
    Credits {
        value: f64,
    },
    ExtraUsage {
        value: f64,
        currency: String,
    },
    IncludedOnDemand {
        included: f64,
        on_demand: f64,
    },
    // Retain the legacy wire representation for older dashboard consumers.
    #[allow(dead_code)]
    LocalMeasurement {
        measured_at_ms: i64,
    },
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
                model: Some("gpt-5".to_string()),
                status: SessionStatus::Working,
                started_at_ms: 1_000,
                updated_at_ms: 2_000,
                token_usage: TokenUsage::total_only(42),
                metrics: SessionMetrics {
                    turn_count: Some(1),
                    tool_call_count: Some(0),
                    ..SessionMetrics::default()
                },
                provider_details: ProviderSessionDetails::Codex {
                    reasoning_effort: Some("high".to_string()),
                    collaboration_mode: Some("default".to_string()),
                    subagents: Vec::new(),
                },
                summary: UiText::from("Contract fixture"),
                activity_groups: vec![ActivityGroup {
                    id: "turn-1".to_string(),
                    started_at_ms: 1_000,
                    completed_at_ms: Some(2_000),
                    status: Some(ActivityStatus::Completed),
                    items: vec![ActivityItem {
                        id: "event-1".to_string(),
                        kind: ActivityKind::Assistant,
                        title: UiText::from("Codex"),
                        body: UiText::from("Ready"),
                        timestamp_ms: 2_000,
                        status: None,
                        duration_ms: None,
                        count: None,
                    }],
                }],
                activity_truncated: false,
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
                    "tokenUsage": {"quality": "totalOnly", "total": 42},
                    "metrics": {"turnCount": 1, "toolCallCount": 0},
                    "providerDetails": {
                        "provider": "codex",
                        "reasoningEffort": "high",
                        "collaborationMode": "default",
                        "subagents": []
                    },
                    "summary": "Contract fixture",
                    "activityGroups": [{
                        "id": "turn-1",
                        "startedAtMs": 1_000,
                        "completedAtMs": 2_000,
                        "status": "completed",
                        "items": [{
                            "id": "event-1",
                            "kind": "assistant",
                            "title": "Codex",
                            "body": "Ready",
                            "timestampMs": 2_000
                        }]
                    }],
                    "activityTruncated": false
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
