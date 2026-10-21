use std::{collections::HashMap, path::Path};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde_json::Value;

use crate::{
    domain::{
        ActivityGroup, ActivityItem, ActivityKind, ActivityStatus, AgentSession, ProviderId,
        ProviderQuota, ProviderSessionDetails, ProviderSnapshot, QuotaWindow, SessionMetrics,
        SessionStatus, TokenUsage, TokenUsageQuality, UiText, UsageCost,
    },
    providers::partial_snapshot,
    utils::{
        clamp_percent, event_timestamp_ms, home_dir, normalize_epoch_ms, project_name,
        reset_at_ms_from_iso, truncate,
    },
};

// Load local history off the async runtime and treat remote quota access as optional data.
pub async fn load(include_quotas: bool) -> Result<ProviderSnapshot, String> {
    let (sessions, session_warning) = match tokio::task::spawn_blocking(load_sessions).await {
        Ok(Ok((sessions, warning))) => (Ok(sessions), warning),
        Ok(Err(error)) => (Err(error), None),
        Err(error) => (Err(format!("Lecture Cursor interrompue : {error}")), None),
    };
    let quota = if include_quotas {
        Some(load_quota().await)
    } else {
        None
    };
    let mut snapshot = partial_snapshot(sessions, quota)?;
    if let Some(warning) = session_warning {
        snapshot.warning = Some(match snapshot.warning {
            Some(existing) => format!("{existing} · {warning}"),
            None => warning,
        });
    }
    Ok(snapshot)
}

fn db_path() -> Result<std::path::PathBuf, String> {
    Ok(home_dir()?.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb"))
}

fn open_db(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Base Cursor illisible : {error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_millis(500))
        .ok();
    Ok(connection)
}

struct HeaderRow {
    composer_id: String,
    workspace_id: String,
    created_at: i64,
    updated_at: i64,
    value: String,
}

// Read recent Composer headers and attach their complete local usage history.
fn load_sessions() -> Result<(Vec<AgentSession>, Option<String>), String> {
    let path = db_path()?;
    if !path.exists() {
        return Err("Aucun état local Cursor trouvé".to_string());
    }
    let connection = open_db(&path)?;
    let mut statement = connection
        .prepare(
            "SELECT composerId, COALESCE(workspaceId, ''), COALESCE(createdAt, 0),
                    COALESCE(lastUpdatedAt, createdAt, 0), COALESCE(value, '{}')
             FROM composerHeaders
             WHERE COALESCE(isArchived, 0) = 0 AND COALESCE(isSubagent, 0) = 0
             ORDER BY COALESCE(lastUpdatedAt, createdAt, 0) DESC
             LIMIT 10",
        )
        .map_err(|error| format!("Schéma Cursor incompatible : {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(HeaderRow {
                composer_id: row.get(0)?,
                workspace_id: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                value: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut headers = Vec::new();
    let mut warnings = Vec::new();
    for row in rows {
        match row {
            Ok(header) => headers.push(header),
            Err(error) => warnings.push(format!("En-tête Cursor illisible : {error}")),
        }
    }
    drop(statement);

    let (sessions, conversion_warning) = sessions_from_headers(&connection, headers)?;
    if let Some(warning) = conversion_warning {
        warnings.push(warning);
    }
    if sessions.is_empty() {
        Err(if warnings.is_empty() {
            "Aucune conversation Cursor trouvée".to_string()
        } else {
            warnings.join(" · ")
        })
    } else {
        Ok((
            sessions,
            (!warnings.is_empty()).then(|| warnings.join(" · ")),
        ))
    }
}

// Preserve valid conversations while reporting malformed headers or bubble histories.
fn sessions_from_headers(
    connection: &Connection,
    headers: Vec<HeaderRow>,
) -> Result<(Vec<AgentSession>, Option<String>), String> {
    let mut sessions = Vec::new();
    let mut warnings = Vec::new();
    for header in headers {
        match header_to_session(connection, header) {
            Ok((session, skipped_bubbles)) => {
                sessions.push(session);
                if skipped_bubbles > 0 {
                    warnings.push(format!(
                        "{skipped_bubbles} bulles Cursor invalides ignorées"
                    ));
                }
            }
            Err(error) => warnings.push(error),
        }
    }
    if sessions.is_empty() && !warnings.is_empty() {
        return Err(warnings.join(" · "));
    }
    Ok((
        sessions,
        (!warnings.is_empty()).then(|| warnings.join(" · ")),
    ))
}

fn header_to_session(
    connection: &Connection,
    row: HeaderRow,
) -> Result<(AgentSession, usize), String> {
    let header: Value = serde_json::from_str(&row.value)
        .map_err(|error| format!("En-tête Cursor {} invalide : {error}", row.composer_id))?;
    let history = load_bubbles(connection, &row.composer_id)?;
    let name = header["name"]
        .as_str()
        .or_else(|| header["subtitle"].as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Conversation Cursor");
    let summary = history
        .latest_assistant
        .as_deref()
        .or_else(|| header["subtitle"].as_str())
        .unwrap_or(name);
    let workspace = header["workspaceIdentifier"]["uri"]
        .as_str()
        .or_else(|| header["workspaceIdentifier"]["id"].as_str())
        .map(|value| value.strip_prefix("file://").unwrap_or(value))
        .filter(|value| !value.is_empty())
        .unwrap_or(&row.workspace_id);
    let branch = header["trackedGitRepos"]
        .as_array()
        .and_then(|repos| repos.first())
        .and_then(|repo| repo["branches"].as_array())
        .and_then(|branches| branches.first())
        .and_then(|branch| branch["branchName"].as_str())
        .unwrap_or("—")
        .to_string();
    let age_seconds = (crate::utils::now_ms() - normalize_epoch_ms(row.updated_at)).max(0) / 1_000;
    let blocked = header["hasBlockingPendingActions"]
        .as_bool()
        .unwrap_or(false)
        || header["hasPendingPlan"].as_bool().unwrap_or(false);
    let status = if blocked {
        SessionStatus::Attention
    } else if age_seconds < 100 {
        SessionStatus::Working
    } else if history.last_type == Some(2) && age_seconds < 900 {
        SessionStatus::Waiting
    } else {
        SessionStatus::Completed
    };

    Ok((
        AgentSession {
            id: format!("cursor:{}", row.composer_id),
            provider: ProviderId::Cursor,
            title: if name == "Conversation Cursor" {
                UiText::message("generated.cursorConversation")
            } else {
                UiText::from(truncate(name, 74))
            },
            project: project_name(workspace),
            branch,
            model: history.model,
            status,
            started_at_ms: normalize_epoch_ms(row.created_at),
            updated_at_ms: normalize_epoch_ms(row.updated_at),
            token_usage: history.token_usage,
            metrics: SessionMetrics {
                turn_count: Some(history.turn_count),
                tool_call_count: Some(history.tool_call_count),
                tool_error_count: Some(history.tool_error_count),
                context_usage_percent: header["contextUsagePercent"].as_f64(),
                last_turn_duration_ms: history.last_turn_duration_ms,
                thinking_duration_ms: history.thinking_duration_ms,
                files_changed: header["filesChangedCount"]
                    .as_u64()
                    .map(|value| value as u32),
                lines_added: header["totalLinesAdded"].as_u64().map(|value| value as u32),
                lines_removed: header["totalLinesRemoved"]
                    .as_u64()
                    .map(|value| value as u32),
                ..SessionMetrics::default()
            },
            provider_details: ProviderSessionDetails::Cursor {
                force_mode: optional_string(header["forceMode"].as_str()),
                unified_mode: optional_string(header["unifiedMode"].as_str()),
            },
            summary: if summary == "Conversation Cursor" {
                UiText::message("generated.cursorConversation")
            } else {
                UiText::from(truncate(summary, 180))
            },
            activity_groups: history.activity_groups,
            activity_truncated: history.activity_truncated,
        },
        history.skipped_bubbles,
    ))
}

struct CursorHistory {
    activity_groups: Vec<ActivityGroup>,
    activity_truncated: bool,
    token_usage: Option<TokenUsage>,
    model: Option<String>,
    latest_assistant: Option<String>,
    last_type: Option<i64>,
    turn_count: u32,
    tool_call_count: u32,
    tool_error_count: u32,
    last_turn_duration_ms: Option<u64>,
    thinking_duration_ms: Option<u64>,
    skipped_bubbles: usize,
}

fn optional_string(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn cursor_item(
    id: String,
    kind: ActivityKind,
    title: UiText,
    body: UiText,
    timestamp_ms: i64,
) -> ActivityItem {
    ActivityItem {
        id,
        kind,
        title,
        body,
        timestamp_ms,
        status: None,
        duration_ms: None,
        count: None,
    }
}

fn cursor_group(id: String, timestamp_ms: i64) -> ActivityGroup {
    ActivityGroup {
        id,
        started_at_ms: timestamp_ms,
        completed_at_ms: None,
        status: None,
        items: Vec::new(),
    }
}

fn cursor_tool_status(value: Option<&str>) -> Option<ActivityStatus> {
    match value {
        Some("completed") => Some(ActivityStatus::Completed),
        Some("error") => Some(ActivityStatus::Error),
        Some("cancelled") => Some(ActivityStatus::Cancelled),
        Some(_) => Some(ActivityStatus::Running),
        None => None,
    }
}

// Normalize fragmented Cursor bubbles into recent user turns and provider-aware activity.
fn load_bubbles(connection: &Connection, composer_id: &str) -> Result<CursorHistory, String> {
    let mut statement = connection
        .prepare(
            "SELECT value FROM cursorDiskKV
         WHERE key >= ?1 AND key < ?2
         ORDER BY rowid ASC",
        )
        .map_err(|error| format!("Bulles Cursor indisponibles : {error}"))?;
    let key_prefix = format!("bubbleId:{composer_id}:");
    let key_upper_bound = format!("bubbleId:{composer_id};");
    let values = statement
        .query_map([key_prefix, key_upper_bound], |row| {
            let value = row.get_ref(0)?;
            match value {
                ValueRef::Text(value) | ValueRef::Blob(value) => Ok(value.to_vec()),
                _ => Err(rusqlite::Error::InvalidColumnType(
                    0,
                    "value".to_string(),
                    value.data_type(),
                )),
            }
        })
        .map_err(|error| format!("Requête de bulles Cursor impossible : {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Bulle Cursor illisible : {error}"))?;

    let mut skipped_bubbles = 0;
    let mut parsed = values
        .into_iter()
        .filter_map(|value| {
            let text = match String::from_utf8(value) {
                Ok(value) => value,
                Err(_) => {
                    skipped_bubbles += 1;
                    return None;
                }
            };
            match serde_json::from_str::<Value>(&text) {
                Ok(value) => Some(value),
                Err(_) => {
                    skipped_bubbles += 1;
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    parsed.sort_by_key(|value| value["createdAt"].as_i64().unwrap_or_default());
    let input_tokens = parsed
        .iter()
        .map(|value| {
            value["tokenCount"]["inputTokens"]
                .as_u64()
                .unwrap_or_default()
        })
        .sum::<u64>();
    let output_tokens = parsed
        .iter()
        .map(|value| {
            value["tokenCount"]["outputTokens"]
                .as_u64()
                .unwrap_or_default()
        })
        .sum::<u64>();
    let token_usage = (input_tokens + output_tokens > 0).then_some(TokenUsage {
        quality: TokenUsageQuality::Detailed,
        total: input_tokens + output_tokens,
        input: Some(input_tokens),
        output: Some(output_tokens),
        cached_input: None,
        cache_write_input: None,
        reasoning_output: None,
    });
    let last_type = parsed.last().and_then(|value| value["type"].as_i64());
    let mut groups: Vec<ActivityGroup> = Vec::new();
    let mut assistant_positions: HashMap<String, (usize, usize)> = HashMap::new();
    let mut tool_positions: HashMap<String, (usize, usize)> = HashMap::new();
    let mut model = None;
    let mut latest_assistant = None;
    let mut turn_count = 0;
    let mut last_turn_duration_ms = None;
    let mut thinking_duration_ms = None;
    for (index, value) in parsed.into_iter().enumerate() {
        let bubble_type = value["type"].as_i64();
        let timestamp = event_timestamp_ms(None, value["createdAt"].as_i64().unwrap_or_default());
        let bubble_id = value["bubbleId"].as_str().unwrap_or("bubble");
        if bubble_type == Some(1) {
            let Some(body) = value["text"]
                .as_str()
                .filter(|value| !value.trim().is_empty())
            else {
                continue;
            };
            groups.push(cursor_group(format!("cursor-turn-{bubble_id}"), timestamp));
            groups.last_mut().unwrap().items.push(cursor_item(
                format!("cursor-user-{bubble_id}"),
                ActivityKind::User,
                UiText::message("generated.you"),
                UiText::from(truncate(body, 360)),
                timestamp,
            ));
            turn_count += 1;
            continue;
        }
        if bubble_type != Some(2) {
            continue;
        }
        if groups.is_empty() {
            groups.push(cursor_group(format!("cursor-turn-{index}"), timestamp));
        }
        let group_index = groups.len() - 1;
        model = optional_string(value["modelInfo"]["modelName"].as_str()).or(model);
        last_turn_duration_ms = value["turnDurationMs"].as_u64().or(last_turn_duration_ms);
        thinking_duration_ms = value["thinkingDurationMs"]
            .as_u64()
            .or(thinking_duration_ms);
        if let Some(body) = value["text"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
        {
            latest_assistant = Some(body.to_string());
            let key = value["requestId"].as_str().unwrap_or(bubble_id).to_string();
            if let Some(&(existing_group, item_index)) = assistant_positions.get(&key) {
                let item = &mut groups[existing_group].items[item_index];
                item.body = UiText::from(truncate(body, 360));
                item.timestamp_ms = timestamp;
            } else {
                let item_index = groups[group_index].items.len();
                groups[group_index].items.push(cursor_item(
                    format!("cursor-assistant-{bubble_id}"),
                    ActivityKind::Assistant,
                    UiText::from("Cursor"),
                    UiText::from(truncate(body, 360)),
                    timestamp,
                ));
                assistant_positions.insert(key, (group_index, item_index));
            }
        }
        if value["toolFormerData"].is_object() {
            let tool = &value["toolFormerData"];
            let call_id = tool["toolCallId"].as_str().unwrap_or(bubble_id).to_string();
            let name = tool["name"]
                .as_str()
                .or_else(|| tool["tool"].as_str())
                .unwrap_or("Outil Cursor");
            let status = cursor_tool_status(tool["status"].as_str());
            if let Some(&(existing_group, item_index)) = tool_positions.get(&call_id) {
                groups[existing_group].items[item_index].status = status;
            } else {
                let item_index = groups[group_index].items.len();
                let mut item = cursor_item(
                    call_id.clone(),
                    ActivityKind::Tool,
                    UiText::from(name),
                    UiText::message("generated.localToolExecution"),
                    timestamp,
                );
                item.status = status;
                groups[group_index].items.push(item);
                tool_positions.insert(call_id, (group_index, item_index));
            }
        }
    }
    for group in &mut groups {
        group.completed_at_ms = group.items.last().map(|item| item.timestamp_ms);
        group.status = Some(
            if group
                .items
                .iter()
                .any(|item| item.status == Some(ActivityStatus::Error))
            {
                ActivityStatus::Error
            } else {
                ActivityStatus::Completed
            },
        );
    }
    let tool_call_count = groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|item| item.kind == ActivityKind::Tool)
        .count() as u32;
    let tool_error_count = groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|item| item.status == Some(ActivityStatus::Error))
        .count() as u32;
    let mut activity_truncated = false;
    if groups.len() > 20 {
        groups.drain(..groups.len() - 20);
        activity_truncated = true;
    }
    Ok(CursorHistory {
        activity_groups: groups,
        activity_truncated,
        token_usage,
        model,
        latest_assistant,
        last_type,
        turn_count,
        tool_call_count,
        tool_error_count,
        last_turn_duration_ms,
        thinking_duration_ms,
        skipped_bubbles,
    })
}

// Fetch usage and account metadata concurrently from Cursor's private authenticated endpoints.
async fn load_quota() -> Result<ProviderQuota, String> {
    let token = tokio::task::spawn_blocking(load_access_token)
        .await
        .map_err(|error| error.to_string())??;
    let payload = jwt_payload(&token)?;
    let subject = payload["sub"]
        .as_str()
        .ok_or("Identité Cursor absente du jeton")?;
    let user_id = subject
        .split('|')
        .next_back()
        .filter(|value| !value.is_empty())
        .ok_or("Identité Cursor invalide")?;
    let cookie = format!("WorkosCursorSessionToken={user_id}%3A%3A{token}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;

    let summary_request = client
        .get("https://cursor.com/api/usage-summary")
        .header("accept", "application/json")
        .header("cookie", &cookie)
        .send();
    let account_request = client
        .get("https://cursor.com/api/auth/me")
        .header("accept", "application/json")
        .header("cookie", &cookie)
        .send();
    let (summary_response, account_response) = tokio::join!(summary_request, account_request);
    let summary_response =
        summary_response.map_err(|error| format!("Quotas Cursor injoignables : {error}"))?;
    if !summary_response.status().is_success() {
        return Err(format!(
            "Quotas Cursor : HTTP {}",
            summary_response.status()
        ));
    }
    let summary: Value = summary_response
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let account = match account_response {
        Ok(response) if response.status().is_success() => {
            response.json::<Value>().await.unwrap_or_default()
        }
        _ => Value::Null,
    };
    parse_quota(&summary, &account)
}

// Read Cursor's session token from its local state database without sending it to Vue.
fn load_access_token() -> Result<String, String> {
    let path = db_path()?;
    let connection = open_db(&path)?;
    let mut statement = connection
        .prepare("SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken' LIMIT 1")
        .map_err(|error| error.to_string())?;
    let token = statement
        .query_row([], |row| {
            let value = row.get_ref(0)?;
            let bytes = match value {
                ValueRef::Text(value) | ValueRef::Blob(value) => value,
                _ => &[],
            };
            Ok(String::from_utf8_lossy(bytes).to_string())
        })
        .map_err(|_| "Jeton local Cursor introuvable".to_string())?;
    if token.trim().is_empty() {
        Err("Jeton local Cursor vide".to_string())
    } else {
        Ok(token)
    }
}

// Decode only the JWT payload needed to derive Cursor's authenticated user identifier.
fn jwt_payload(token: &str) -> Result<Value, String> {
    let encoded = token.split('.').nth(1).ok_or("Jeton Cursor invalide")?;
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "Jeton Cursor illisible")?;
    serde_json::from_slice(&decoded).map_err(|_| "Payload Cursor invalide".to_string())
}

// Validate the private API shape before converting it into UI quota values.
fn parse_quota(summary: &Value, account: &Value) -> Result<ProviderQuota, String> {
    if !summary.is_object() {
        return Err("Réponse de quota Cursor invalide".to_string());
    }
    let plan = &summary["individualUsage"]["plan"];
    let overall = &summary["individualUsage"]["overall"];
    let pooled = &summary["teamUsage"]["pooled"];
    let ratio = |value: &Value| {
        let used = value["used"].as_f64().unwrap_or_default();
        let limit = value["limit"].as_f64().unwrap_or_default();
        (limit > 0.0).then_some((used / limit) * 100.0)
    };
    if !plan.is_object() && !overall.is_object() && !pooled.is_object() {
        return Err("Réponse de quota Cursor sans données d’utilisation".to_string());
    }
    let plan_percent = plan["totalPercentUsed"]
        .as_f64()
        .or_else(|| {
            let auto = plan["autoPercentUsed"].as_f64();
            let api = plan["apiPercentUsed"].as_f64();
            match (auto, api) {
                (Some(left), Some(right)) => Some((left + right) / 2.0),
                (Some(value), None) | (None, Some(value)) => Some(value),
                _ => None,
            }
        })
        .or_else(|| ratio(plan))
        .or_else(|| ratio(overall))
        .or_else(|| ratio(pooled))
        .filter(|value| value.is_finite())
        .ok_or("Réponse de quota Cursor sans pourcentage exploitable")?;

    let reset_at_ms = match summary.get("billingCycleEnd") {
        Some(Value::String(value)) => reset_at_ms_from_iso(Some(value))
            .ok_or("Date de cycle Cursor invalide")
            .map(Some)?,
        Some(Value::Null) | None => None,
        Some(_) => return Err("Date de cycle Cursor invalide".to_string()),
    };

    let mut windows = vec![QuotaWindow {
        id: "cursor-plan".to_string(),
        label: UiText::message("quota.included"),
        used_percent: clamp_percent(plan_percent),
        reset_at_ms,
        reset_text: Some(UiText::message("quota.resetUnknown")),
    }];
    if plan["autoPercentUsed"].is_number() {
        windows.push(QuotaWindow {
            id: "cursor-auto".to_string(),
            label: UiText::message("quota.autoModel"),
            used_percent: clamp_percent(plan["autoPercentUsed"].as_f64().unwrap_or_default()),
            reset_at_ms,
            reset_text: Some(UiText::message("quota.billingCycle")),
        });
    }
    if plan["apiPercentUsed"].is_number() {
        windows.push(QuotaWindow {
            id: "cursor-api".to_string(),
            label: UiText::message("quota.apiModels"),
            used_percent: clamp_percent(plan["apiPercentUsed"].as_f64().unwrap_or_default()),
            reset_at_ms,
            reset_text: Some(UiText::message("quota.billingCycle")),
        });
    }
    let used_cents = plan["used"]
        .as_f64()
        .or_else(|| overall["used"].as_f64())
        .or_else(|| pooled["used"].as_f64());
    let on_demand_cents = summary["individualUsage"]["onDemand"]["used"].as_f64();
    let cost = used_cents
        .or(on_demand_cents)
        .map(|_| UsageCost::IncludedOnDemand {
            included: used_cents.unwrap_or_default() / 100.0,
            on_demand: on_demand_cents.unwrap_or_default() / 100.0,
        });
    Ok(ProviderQuota {
        provider: ProviderId::Cursor,
        plan: summary["membershipType"]
            .as_str()
            .map(format_membership)
            .unwrap_or_else(|| "Cursor".to_string()),
        account: account["email"]
            .as_str()
            .map(UiText::from)
            .unwrap_or_else(|| UiText::message("generated.cursorAccount")),
        windows,
        cost,
        reset_offers: Vec::new(),
    })
}

fn format_membership(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_else(|| "Cursor".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_usage_summary() {
        let quota = parse_quota(
            &json!({
                "membershipType": "pro",
                "billingCycleEnd": "2030-01-31T00:00:00Z",
                "individualUsage": {"plan": {"used": 1250, "limit": 2000, "totalPercentUsed": 62.5}}
            }),
            &json!({"email": "cursor@example.com"}),
        )
        .expect("valid Cursor quota");
        assert_eq!(quota.windows[0].used_percent, 63);
        assert_eq!(quota.plan, "Pro");
    }

    #[test]
    fn rejects_empty_usage_summary() {
        assert!(parse_quota(&json!({}), &Value::Null).is_err());
    }

    #[test]
    fn groups_all_bubbles_into_user_turns() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        connection
            .execute(
                "CREATE TABLE cursorDiskKV (key TEXT UNIQUE, value BLOB)",
                [],
            )
            .expect("Cursor bubble table");
        for index in 0..8 {
            let value = json!({
                "bubbleId": format!("bubble-{index}"),
                "type": if index % 2 == 0 { 1 } else { 2 },
                "text": format!("message-{index}"),
                "createdAt": 1_000 + index,
                "tokenCount": {"inputTokens": 10, "outputTokens": 2}
            });
            connection
                .execute(
                    "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
                    rusqlite::params![format!("bubbleId:composer:{index}"), value.to_string()],
                )
                .expect("bubble row");
        }

        let history = load_bubbles(&connection, "composer").expect("bubble history");
        assert_eq!(history.token_usage.expect("token usage").total, 8 * 12);
        assert_eq!(history.activity_groups.len(), 4);
        assert_eq!(
            history.activity_groups[1].items[0].body.as_raw(),
            Some("message-2")
        );
        assert_eq!(history.last_type, Some(2));
    }

    #[test]
    fn reports_invalid_bubble_json() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        connection
            .execute(
                "CREATE TABLE cursorDiskKV (key TEXT UNIQUE, value BLOB)",
                [],
            )
            .expect("Cursor bubble table");
        connection
            .execute(
                "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
                rusqlite::params!["bubbleId:composer:broken", "not-json"],
            )
            .expect("invalid bubble row");

        let history = load_bubbles(&connection, "composer").expect("partial bubble history");
        assert_eq!(history.skipped_bubbles, 1);
    }

    #[test]
    fn preserves_session_headers_when_one_conversation_is_corrupt() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        connection
            .execute(
                "CREATE TABLE cursorDiskKV (key TEXT UNIQUE, value BLOB)",
                [],
            )
            .expect("Cursor bubble table");
        connection
            .execute(
                "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
                rusqlite::params![
                    "bubbleId:good:one",
                    json!({
                        "bubbleId": "one",
                        "type": 1,
                        "text": "valid session",
                        "createdAt": 1_000
                    })
                    .to_string()
                ],
            )
            .expect("valid bubble row");
        connection
            .execute(
                "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
                rusqlite::params!["bubbleId:broken:one", "not-json"],
            )
            .expect("invalid bubble row");
        let headers = ["good", "broken"]
            .into_iter()
            .map(|composer_id| HeaderRow {
                composer_id: composer_id.to_string(),
                workspace_id: "workspace".to_string(),
                created_at: 1_000,
                updated_at: 1_000,
                value: json!({"name": composer_id}).to_string(),
            })
            .collect();

        let (sessions, warning) =
            sessions_from_headers(&connection, headers).expect("partial Cursor sessions");

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "cursor:good");
        assert_eq!(sessions[1].id, "cursor:broken");
        assert!(sessions[1].activity_groups.is_empty());
        assert!(warning.is_some());
    }

    #[test]
    #[ignore = "dépend des données Cursor installées sur la machine"]
    fn reads_local_composers() {
        assert!(!load_sessions()
            .expect("historique Cursor local")
            .0
            .is_empty());
    }
}
