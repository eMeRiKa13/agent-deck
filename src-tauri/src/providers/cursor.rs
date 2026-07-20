use std::path::Path;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde_json::Value;

use crate::{
    domain::{
        AgentSession, EventKind, ProviderId, ProviderQuota, ProviderSnapshot, QuotaWindow,
        SessionEvent, SessionStatus,
    },
    utils::{
        clamp_percent, elapsed_label, event_time, home_dir, normalize_epoch_ms, project_name,
        relative_time, reset_label_from_iso, truncate,
    },
};

pub async fn load() -> Result<ProviderSnapshot, String> {
    let sessions = tokio::task::spawn_blocking(load_sessions)
        .await
        .map_err(|error| format!("Lecture Cursor interrompue : {error}"))??;
    let (quota, warning) = match load_quota().await {
        Ok(quota) => (Some(quota), None),
        Err(error) => (None, Some(error)),
    };
    Ok(ProviderSnapshot {
        sessions,
        quota,
        warning,
    })
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

fn load_sessions() -> Result<Vec<AgentSession>, String> {
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
    let headers = statement
        .query_map([], |row| {
            Ok(HeaderRow {
                composer_id: row.get(0)?,
                workspace_id: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                value: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    drop(statement);

    let sessions = headers
        .into_iter()
        .map(|header| header_to_session(&connection, header))
        .collect::<Vec<_>>();
    if sessions.is_empty() {
        Err("Aucune conversation Cursor trouvée".to_string())
    } else {
        Ok(sessions)
    }
}

fn header_to_session(connection: &Connection, row: HeaderRow) -> AgentSession {
    let header: Value = serde_json::from_str(&row.value).unwrap_or_default();
    let (events, token_count, last_type) = load_bubbles(connection, &row.composer_id);
    let name = header["name"]
        .as_str()
        .or_else(|| header["subtitle"].as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Conversation Cursor");
    let summary = events
        .iter()
        .rev()
        .find(|event| matches!(event.kind, EventKind::Assistant))
        .map(|event| event.body.as_str())
        .or_else(|| header["subtitle"].as_str())
        .unwrap_or(name);
    let workspace = header["workspaceIdentifier"]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or(&row.workspace_id);
    let branch = header["trackedGitRepos"]
        .as_array()
        .and_then(|repos| repos.first())
        .and_then(|repo| repo["branch"].as_str().or_else(|| repo["name"].as_str()))
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
    } else if last_type == Some(2) && age_seconds < 900 {
        SessionStatus::Waiting
    } else {
        SessionStatus::Completed
    };

    AgentSession {
        id: format!("cursor:{}", row.composer_id),
        provider: ProviderId::Cursor,
        title: truncate(name, 74),
        project: project_name(workspace),
        branch,
        model: header["forceMode"]
            .as_str()
            .or_else(|| header["unifiedMode"].as_str())
            .unwrap_or("Cursor Agent")
            .to_string(),
        status,
        elapsed: elapsed_label(row.created_at, row.updated_at),
        updated_at: relative_time(row.updated_at),
        token_count,
        summary: truncate(summary, 180),
        events,
        updated_at_ms: normalize_epoch_ms(row.updated_at),
    }
}

fn load_bubbles(
    connection: &Connection,
    composer_id: &str,
) -> (Vec<SessionEvent>, u64, Option<i64>) {
    let mut statement = match connection.prepare(
        "SELECT value FROM cursorDiskKV
         WHERE rowid > (SELECT COALESCE(MAX(rowid), 0) - 10000 FROM cursorDiskKV)
           AND key LIKE ?1
         ORDER BY rowid DESC LIMIT 40",
    ) {
        Ok(statement) => statement,
        Err(_) => return (Vec::new(), 0, None),
    };
    let pattern = format!("bubbleId:{composer_id}:%");
    let values = match statement.query_map([pattern], |row| {
        let value = row.get_ref(0)?;
        let bytes = match value {
            ValueRef::Text(value) | ValueRef::Blob(value) => value,
            _ => return Ok(None),
        };
        Ok(String::from_utf8(bytes.to_vec()).ok())
    }) {
        Ok(values) => values.filter_map(Result::ok).flatten().collect::<Vec<_>>(),
        Err(_) => return (Vec::new(), 0, None),
    };

    let mut parsed = values
        .into_iter()
        .filter_map(|value| serde_json::from_str::<Value>(&value).ok())
        .collect::<Vec<_>>();
    parsed.sort_by_key(|value| value["createdAt"].as_i64().unwrap_or_default());
    let token_count = parsed
        .iter()
        .map(|value| {
            value["tokenCount"]["inputTokens"]
                .as_u64()
                .unwrap_or_default()
                + value["tokenCount"]["outputTokens"]
                    .as_u64()
                    .unwrap_or_default()
        })
        .sum();
    let last_type = parsed.last().and_then(|value| value["type"].as_i64());
    let mut events = parsed
        .into_iter()
        .filter_map(|value| {
            let bubble_type = value["type"].as_i64()?;
            let body = value["text"]
                .as_str()
                .filter(|value| !value.trim().is_empty())?;
            let (kind, title) = if bubble_type == 1 {
                (EventKind::User, "Vous")
            } else if bubble_type == 2 {
                (EventKind::Assistant, "Cursor")
            } else {
                return None;
            };
            let id = value["bubbleId"].as_str().unwrap_or("bubble");
            let timestamp = value["createdAt"].as_i64().unwrap_or_default();
            Some(SessionEvent {
                id: format!("cursor-event-{id}"),
                kind,
                title: title.to_string(),
                body: truncate(body, 240),
                time: event_time(None, timestamp),
            })
        })
        .collect::<Vec<_>>();
    if events.len() > 6 {
        events.drain(..events.len() - 6);
    }
    (events, token_count, last_type)
}

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
    Ok(parse_quota(&summary, &account))
}

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

fn jwt_payload(token: &str) -> Result<Value, String> {
    let encoded = token.split('.').nth(1).ok_or("Jeton Cursor invalide")?;
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "Jeton Cursor illisible")?;
    serde_json::from_slice(&decoded).map_err(|_| "Payload Cursor invalide".to_string())
}

fn parse_quota(summary: &Value, account: &Value) -> ProviderQuota {
    let plan = &summary["individualUsage"]["plan"];
    let overall = &summary["individualUsage"]["overall"];
    let pooled = &summary["teamUsage"]["pooled"];
    let ratio = |value: &Value| {
        let used = value["used"].as_f64().unwrap_or_default();
        let limit = value["limit"].as_f64().unwrap_or_default();
        (limit > 0.0).then_some((used / limit) * 100.0)
    };
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
        .unwrap_or_default();

    let mut windows = vec![QuotaWindow {
        id: "cursor-plan".to_string(),
        label: "Inclus au forfait".to_string(),
        used_percent: clamp_percent(plan_percent),
        reset_label: reset_label_from_iso(summary["billingCycleEnd"].as_str()),
    }];
    if plan["autoPercentUsed"].is_number() {
        windows.push(QuotaWindow {
            id: "cursor-auto".to_string(),
            label: "Modèle Auto".to_string(),
            used_percent: clamp_percent(plan["autoPercentUsed"].as_f64().unwrap_or_default()),
            reset_label: "cycle de facturation en cours".to_string(),
        });
    }
    if plan["apiPercentUsed"].is_number() {
        windows.push(QuotaWindow {
            id: "cursor-api".to_string(),
            label: "Modèles API".to_string(),
            used_percent: clamp_percent(plan["apiPercentUsed"].as_f64().unwrap_or_default()),
            reset_label: "cycle de facturation en cours".to_string(),
        });
    }
    let used_cents = plan["used"]
        .as_f64()
        .or_else(|| overall["used"].as_f64())
        .or_else(|| pooled["used"].as_f64())
        .unwrap_or_default();
    let on_demand_cents = summary["individualUsage"]["onDemand"]["used"]
        .as_f64()
        .unwrap_or_default();
    ProviderQuota {
        provider: ProviderId::Cursor,
        plan: summary["membershipType"]
            .as_str()
            .map(format_membership)
            .unwrap_or_else(|| "Cursor".to_string()),
        account: account["email"]
            .as_str()
            .unwrap_or("Compte Cursor")
            .to_string(),
        windows,
        cost_label: Some(format!(
            "{:.2} $ inclus · {:.2} $ à la demande",
            used_cents / 100.0,
            on_demand_cents / 100.0
        )),
    }
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
        );
        assert_eq!(quota.windows[0].used_percent, 63);
        assert_eq!(quota.plan, "Pro");
    }

    #[test]
    #[ignore = "dépend des données Cursor installées sur la machine"]
    fn reads_local_composers() {
        assert!(!load_sessions().expect("historique Cursor local").is_empty());
    }
}
