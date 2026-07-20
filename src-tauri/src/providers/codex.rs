use std::{path::PathBuf, process::Stdio};

use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::{timeout, Duration},
};

use crate::{
    domain::{
        AgentSession, EventKind, ProviderId, ProviderQuota, ProviderSnapshot, QuotaWindow,
        SessionEvent, SessionStatus,
    },
    utils::{
        clamp_percent, elapsed_label, event_time, home_dir, normalize_epoch_ms, project_name,
        relative_time, reset_label_from_epoch, tail_lines, truncate,
    },
};

pub async fn load() -> Result<ProviderSnapshot, String> {
    let sessions = tokio::task::spawn_blocking(load_sessions)
        .await
        .map_err(|error| format!("Lecture Codex interrompue : {error}"))??;
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

#[derive(Debug)]
struct ThreadRow {
    id: String,
    rollout_path: String,
    created_at: i64,
    updated_at: i64,
    cwd: String,
    title: String,
    branch: String,
    model: String,
    token_count: u64,
    preview: String,
    first_user_message: String,
}

fn load_sessions() -> Result<Vec<AgentSession>, String> {
    let home = home_dir()?;
    let db_path = find_codex_state_db(&home).ok_or("Aucun état local Codex trouvé")?;

    let connection = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Base Codex illisible : {error}"))?;
    connection.busy_timeout(Duration::from_millis(400)).ok();

    let mut statement = connection
        .prepare(
            "SELECT id, COALESCE(rollout_path, ''),
                    COALESCE(created_at_ms, created_at * 1000, 0),
                    COALESCE(updated_at_ms, updated_at * 1000, 0),
                    COALESCE(cwd, ''), COALESCE(title, ''),
                    COALESCE(git_branch, ''), COALESCE(model, ''),
                    COALESCE(tokens_used, 0), COALESCE(preview, ''),
                    COALESCE(first_user_message, '')
             FROM threads
             WHERE COALESCE(archived, 0) = 0
               AND (COALESCE(title, '') <> '' OR COALESCE(first_user_message, '') <> '')
             ORDER BY COALESCE(updated_at_ms, updated_at * 1000, 0) DESC
             LIMIT 10",
        )
        .map_err(|error| format!("Schéma Codex incompatible : {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(ThreadRow {
                id: row.get(0)?,
                rollout_path: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                cwd: row.get(4)?,
                title: row.get(5)?,
                branch: row.get(6)?,
                model: row.get(7)?,
                token_count: row.get::<_, i64>(8)?.max(0) as u64,
                preview: row.get(9)?,
                first_user_message: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string()))
        .map(|row| row.map(thread_to_session))
        .collect()
}

fn find_codex_state_db(home: &std::path::Path) -> Option<PathBuf> {
    [
        home.join(".codex/state_5.sqlite"),
        home.join(".codex/sqlite/state_5.sqlite"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn thread_to_session(row: ThreadRow) -> AgentSession {
    let path = PathBuf::from(&row.rollout_path);
    let events = if path.exists() {
        parse_events(&path, row.updated_at)
    } else {
        Vec::new()
    };
    let summary_source = [&row.preview, &row.first_user_message, &row.title]
        .into_iter()
        .find(|value| !value.trim().is_empty())
        .map(|value| value.as_str())
        .unwrap_or("Conversation Codex");
    let age_seconds = (crate::utils::now_ms() - normalize_epoch_ms(row.updated_at)).max(0) / 1_000;
    let last_kind = events.last().map(|event| event.kind);
    let status = if age_seconds < 100 {
        SessionStatus::Working
    } else if matches!(last_kind, Some(EventKind::User)) && age_seconds < 900 {
        SessionStatus::Attention
    } else if matches!(last_kind, Some(EventKind::Assistant)) && age_seconds < 900 {
        SessionStatus::Waiting
    } else {
        SessionStatus::Completed
    };

    AgentSession {
        id: format!("codex:{}", row.id),
        provider: ProviderId::Codex,
        title: truncate(
            if row.title.trim().is_empty() {
                &row.first_user_message
            } else {
                &row.title
            },
            74,
        ),
        project: project_name(&row.cwd),
        branch: if row.branch.is_empty() {
            "—".to_string()
        } else {
            row.branch
        },
        model: if row.model.is_empty() {
            "Codex".to_string()
        } else {
            row.model
        },
        status,
        elapsed: elapsed_label(row.created_at, row.updated_at),
        updated_at: relative_time(row.updated_at),
        token_count: row.token_count,
        summary: truncate(summary_source, 180),
        events,
        updated_at_ms: normalize_epoch_ms(row.updated_at),
    }
}

fn parse_events(path: &std::path::Path, fallback_ms: i64) -> Vec<SessionEvent> {
    let lines = tail_lines(path, 512 * 1024).unwrap_or_default();
    let mut events = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = &value["payload"];
        let payload_type = payload["type"].as_str().unwrap_or_default();
        let timestamp = value["timestamp"].as_str();
        let event = match payload_type {
            "user_message" => payload["message"]
                .as_str()
                .map(|body| (EventKind::User, "Vous".to_string(), truncate(body, 240))),
            "agent_message" => payload["message"].as_str().map(|body| {
                (
                    EventKind::Assistant,
                    "Codex".to_string(),
                    truncate(body, 240),
                )
            }),
            "custom_tool_call" => Some((
                EventKind::Tool,
                payload["name"].as_str().unwrap_or("Outil").to_string(),
                "Exécution d’un outil local".to_string(),
            )),
            "task_started" => Some((
                EventKind::System,
                "Travail démarré".to_string(),
                "Codex traite la demande.".to_string(),
            )),
            "task_complete" | "turn_complete" => Some((
                EventKind::System,
                "Travail terminé".to_string(),
                "La dernière demande est terminée.".to_string(),
            )),
            _ => None,
        };
        if let Some((kind, title, body)) = event {
            events.push(SessionEvent {
                id: format!("codex-event-{index}"),
                kind,
                title,
                body,
                time: event_time(timestamp, fallback_ms),
            });
        }
    }
    if events.len() > 6 {
        events.drain(..events.len() - 6);
    }
    events
}

async fn load_quota() -> Result<ProviderQuota, String> {
    let binary = find_codex_binary();
    let mut child = Command::new(&binary)
        .arg("app-server")
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Impossible de lancer codex app-server : {error}"))?;

    let mut stdin = child.stdin.take().ok_or("Entrée app-server indisponible")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Sortie app-server indisponible")?;
    let mut lines = BufReader::new(stdout).lines();

    async fn send(stdin: &mut tokio::process::ChildStdin, value: Value) -> Result<(), String> {
        stdin
            .write_all(format!("{value}\n").as_bytes())
            .await
            .map_err(|error| error.to_string())
    }

    let exchange = async {
        send(
            &mut stdin,
            json!({
                "method": "initialize",
                "id": 0,
                "params": {"clientInfo": {"name": "agent-deck", "title": "Agent Deck", "version": "0.1.0"}}
            }),
        )
        .await?;

        let mut initialized = false;
        let mut account: Option<Value> = None;
        let mut rate_limits: Option<Value> = None;
        while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
            let value: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
            if value["id"] == 0 && !initialized {
                initialized = true;
                send(&mut stdin, json!({"method": "initialized", "params": {}})).await?;
                send(
                    &mut stdin,
                    json!({"method": "account/read", "id": 1, "params": {"refreshToken": false}}),
                )
                .await?;
                send(
                    &mut stdin,
                    json!({"method": "account/rateLimits/read", "id": 2, "params": {}}),
                )
                .await?;
            } else if value["id"] == 1 {
                if let Some(error) = value.get("error") {
                    return Err(format!(
                        "Compte Codex indisponible : {}",
                        error["message"].as_str().unwrap_or("erreur RPC")
                    ));
                }
                account = value.get("result").cloned();
            } else if value["id"] == 2 {
                if let Some(error) = value.get("error") {
                    return Err(format!(
                        "Quotas Codex indisponibles : {}",
                        error["message"].as_str().unwrap_or("erreur RPC")
                    ));
                }
                rate_limits = value.get("result").cloned();
            }
            if account.is_some() && rate_limits.is_some() {
                break;
            }
        }
        Ok::<_, String>((account.unwrap_or_default(), rate_limits.unwrap_or_default()))
    };

    let result = timeout(Duration::from_secs(12), exchange)
        .await
        .map_err(|_| "Délai dépassé pour codex app-server".to_string())??;
    child.kill().await.ok();
    Ok(parse_quota(&result.0, &result.1))
}

fn find_codex_binary() -> PathBuf {
    std::env::var_os("CODEX_PATH")
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .or_else(|| {
            [
                "/Applications/ChatGPT.app/Contents/Resources/codex",
                "/Applications/Codex.app/Contents/Resources/codex",
            ]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.exists())
        })
        .unwrap_or_else(|| PathBuf::from("codex"))
}

fn parse_quota(account: &Value, response: &Value) -> ProviderQuota {
    let account = &account["account"];
    let snapshot = &response["rateLimits"];
    let source_windows = ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| {
            snapshot
                .get(key)
                .filter(|window| window.is_object())
                .map(|window| (key, window))
        })
        .collect::<Vec<_>>();
    let has_single_window = source_windows.len() == 1;
    let mut windows = Vec::new();
    for (key, window) in source_windows {
        let duration_minutes = window["windowDurationMins"]
            .as_i64()
            .or_else(|| window["window_minutes"].as_i64());
        let (id, label) = codex_window_identity(key, duration_minutes, has_single_window);
        windows.push(QuotaWindow {
            id: id.to_string(),
            label: label.to_string(),
            used_percent: clamp_percent(window["usedPercent"].as_f64().unwrap_or_default()),
            reset_label: reset_label_from_epoch(window["resetsAt"].as_i64()),
        });
    }
    let credits = snapshot["credits"]["balance"]
        .as_str()
        .map(|balance| {
            balance
                .parse::<f64>()
                .map(|value| format!("{value:.2} crédits"))
                .unwrap_or_else(|_| format!("{balance} crédits"))
        });
    ProviderQuota {
        provider: ProviderId::Codex,
        plan: title_case(account["planType"].as_str().unwrap_or("Codex")),
        account: account["email"]
            .as_str()
            .unwrap_or("Compte Codex")
            .to_string(),
        windows,
        cost_label: credits,
    }
}

fn codex_window_identity(
    key: &str,
    duration_minutes: Option<i64>,
    has_single_window: bool,
) -> (&'static str, &'static str) {
    match duration_minutes {
        Some(300) => ("codex-session", "5 heures"),
        Some(minutes) if minutes >= 24 * 60 => ("codex-global", "Limite globale"),
        _ if has_single_window => ("codex-global", "Limite globale"),
        _ if key == "primary" => ("codex-session", "Session"),
        _ => ("codex-global", "Limite globale"),
    }
}

fn title_case(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_current_single_global_rate_limit_response() {
        let account =
            json!({"account": {"type": "chatgpt", "email": "a@example.com", "planType": "pro"}});
        let limits = json!({"rateLimits": {
            "primary": {
                "usedPercent": 39,
                "resetsAt": 2_000_000_000,
                "windowDurationMins": 10_080
            },
            "secondary": null,
            "credits": {"balance": "12.50"}
        }});
        let quota = parse_quota(&account, &limits);

        assert_eq!(quota.windows.len(), 1);
        assert_eq!(quota.windows[0].id, "codex-global");
        assert_eq!(quota.windows[0].label, "Limite globale");
        assert_eq!(quota.windows[0].used_percent, 39);
        assert_eq!(quota.plan, "Pro");
        assert_eq!(quota.cost_label.as_deref(), Some("12.50 crédits"));
    }

    #[test]
    fn keeps_legacy_dual_rate_limit_response_compatible() {
        let account =
            json!({"account": {"type": "chatgpt", "email": "a@example.com", "planType": "pro"}});
        let limits = json!({"rateLimits": {
            "primary": {
                "usedPercent": 41,
                "resetsAt": 2_000_000_000,
                "windowDurationMins": 300
            },
            "secondary": {
                "usedPercent": 68,
                "resetsAt": 2_000_086_400,
                "windowDurationMins": 10_080
            }
        }});
        let quota = parse_quota(&account, &limits);

        assert_eq!(quota.windows.len(), 2);
        assert_eq!(quota.windows[0].label, "5 heures");
        assert_eq!(quota.windows[1].label, "Limite globale");
    }

    #[test]
    #[ignore = "dépend des données Codex installées sur la machine"]
    fn reads_local_threads() {
        let sessions = load_sessions().expect("état Codex local");
        let latest = sessions.first().expect("au moins une tâche Codex");
        assert!(
            crate::utils::now_ms() - latest.updated_at_ms < 24 * 60 * 60 * 1_000,
            "la tâche Codex la plus récente devrait dater de moins de 24 h"
        );
    }
}
