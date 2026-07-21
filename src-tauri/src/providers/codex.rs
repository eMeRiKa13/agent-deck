use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::UNIX_EPOCH,
};

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
        SessionEvent, SessionStatus, UiText, UsageCost,
    },
    providers::partial_snapshot,
    utils::{
        clamp_percent, event_timestamp_ms, home_dir, normalize_epoch_ms, project_name,
        reset_at_ms_from_epoch, tail_lines, truncate,
    },
};

pub async fn load(include_quotas: bool) -> Result<ProviderSnapshot, String> {
    let sessions = match tokio::task::spawn_blocking(load_sessions).await {
        Ok(result) => result,
        Err(error) => Err(format!("Lecture Codex interrompue : {error}")),
    };
    let quota = if include_quotas {
        Some(load_quota().await)
    } else {
        None
    };
    partial_snapshot(sessions, quota)
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

// Load the newest visible Codex threads from the most relevant local state database.
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

    let rows = rows
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(thread_to_session).collect()
}

// Prefer a readable database with the newest thread instead of relying on path order.
fn find_codex_state_db(home: &Path) -> Option<PathBuf> {
    [
        home.join(".codex/state_5.sqlite"),
        home.join(".codex/sqlite/state_5.sqlite"),
    ]
    .into_iter()
    .filter(|path| path.exists())
    .max_by_key(|path| codex_db_rank(path))
}

// Rank readable schemas first, then use latest thread activity and file modification as tie-breakers.
fn codex_db_rank(path: &Path) -> (bool, i64, u128) {
    let latest_thread_ms = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
    .and_then(|connection| {
        connection
            .query_row(
                "SELECT MAX(COALESCE(updated_at_ms, updated_at * 1000, 0)) FROM threads",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .ok()
            .flatten()
    });
    let modified_ns = path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    (
        latest_thread_ms.is_some(),
        latest_thread_ms.unwrap_or_default(),
        modified_ns,
    )
}

fn thread_to_session(row: ThreadRow) -> Result<AgentSession, String> {
    let path = PathBuf::from(&row.rollout_path);
    let events = if path.exists() {
        parse_events(&path, row.updated_at)?
    } else {
        Vec::new()
    };
    let summary_source = [&row.preview, &row.first_user_message, &row.title]
        .into_iter()
        .find(|value| !value.trim().is_empty())
        .map(|value| value.as_str());
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

    Ok(AgentSession {
        id: format!("codex:{}", row.id),
        provider: ProviderId::Codex,
        title: {
            let source = if row.title.trim().is_empty() {
                &row.first_user_message
            } else {
                &row.title
            };
            if source.trim().is_empty() {
                UiText::message("generated.codexConversation")
            } else {
                UiText::from(truncate(source, 74))
            }
        },
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
        started_at_ms: normalize_epoch_ms(row.created_at),
        updated_at_ms: normalize_epoch_ms(row.updated_at),
        token_count: row.token_count,
        summary: summary_source
            .map(|value| UiText::from(truncate(value, 180)))
            .unwrap_or_else(|| UiText::message("generated.codexConversation")),
        events,
    })
}

// Parse only recent rollout records while surfacing I/O and encoding failures to the provider.
fn parse_events(path: &Path, fallback_ms: i64) -> Result<Vec<SessionEvent>, String> {
    let lines = tail_lines(path, 512 * 1024)
        .map_err(|error| format!("Historique Codex illisible ({}): {error}", path.display()))?;
    let mut events = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = &value["payload"];
        let payload_type = payload["type"].as_str().unwrap_or_default();
        let timestamp = value["timestamp"].as_str();
        let event = match payload_type {
            "user_message" => payload["message"].as_str().map(|body| {
                (
                    EventKind::User,
                    UiText::message("generated.you"),
                    UiText::from(truncate(body, 240)),
                )
            }),
            "agent_message" => payload["message"].as_str().map(|body| {
                (
                    EventKind::Assistant,
                    UiText::from("Codex"),
                    UiText::from(truncate(body, 240)),
                )
            }),
            "custom_tool_call" => Some((
                EventKind::Tool,
                payload["name"]
                    .as_str()
                    .map(UiText::from)
                    .unwrap_or_else(|| UiText::message("generated.tool")),
                UiText::message("generated.localToolExecution"),
            )),
            "task_started" => Some((
                EventKind::System,
                UiText::message("generated.taskStarted"),
                UiText::message("generated.taskStartedBody"),
            )),
            "task_complete" | "turn_complete" => Some((
                EventKind::System,
                UiText::message("generated.taskCompleted"),
                UiText::message("generated.taskCompletedBody"),
            )),
            _ => None,
        };
        if let Some((kind, title, body)) = event {
            events.push(SessionEvent {
                id: format!("codex-event-{index}"),
                kind,
                title,
                body,
                timestamp_ms: event_timestamp_ms(timestamp, fallback_ms),
            });
        }
    }
    if events.len() > 6 {
        events.drain(..events.len() - 6);
    }
    Ok(events)
}

// Exchange the minimum RPC messages required to read the authenticated account and its limits.
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
        complete_quota_exchange(account, rate_limits)
    };

    let result = timeout(Duration::from_secs(12), exchange)
        .await
        .map_err(|_| "Délai dépassé pour codex app-server".to_string())??;
    child.kill().await.ok();
    parse_quota(&result.0, &result.1)
}

// Turn premature app-server EOF into an explicit provider warning instead of empty quota data.
fn complete_quota_exchange(
    account: Option<Value>,
    rate_limits: Option<Value>,
) -> Result<(Value, Value), String> {
    let account = account.ok_or_else(|| {
        "codex app-server a fermé la connexion avant la réponse du compte".to_string()
    })?;
    let rate_limits = rate_limits.ok_or_else(|| {
        "codex app-server a fermé la connexion avant la réponse des quotas".to_string()
    })?;
    Ok((account, rate_limits))
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

// Validate the private RPC response before translating it into the stable dashboard model.
fn parse_quota(account: &Value, response: &Value) -> Result<ProviderQuota, String> {
    let account = account
        .get("account")
        .filter(|value| value.is_object())
        .ok_or_else(|| "Réponse compte Codex incomplète".to_string())?;
    let snapshot = response
        .get("rateLimits")
        .filter(|value| value.is_object())
        .ok_or_else(|| "Réponse quotas Codex incomplète".to_string())?;
    let source_windows = ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| {
            snapshot
                .get(key)
                .filter(|window| window.is_object())
                .map(|window| (key, window))
        })
        .collect::<Vec<_>>();
    if source_windows.is_empty() {
        return Err("Réponse quotas Codex sans fenêtre d’utilisation".to_string());
    }
    if source_windows
        .iter()
        .any(|(_, window)| !window["usedPercent"].is_number())
    {
        return Err("Fenêtre de quota Codex sans pourcentage utilisé".to_string());
    }
    let has_single_window = source_windows.len() == 1;
    let mut windows = Vec::new();
    for (key, window) in source_windows {
        let duration_minutes = window["windowDurationMins"]
            .as_i64()
            .or_else(|| window["window_minutes"].as_i64());
        let (id, label) = codex_window_identity(key, duration_minutes, has_single_window);
        windows.push(QuotaWindow {
            id: id.to_string(),
            label: UiText::message(label),
            used_percent: clamp_percent(window["usedPercent"].as_f64().unwrap_or_default()),
            reset_at_ms: reset_at_ms_from_epoch(window["resetsAt"].as_i64()),
            reset_text: None,
        });
    }
    let credits = snapshot["credits"]["balance"]
        .as_str()
        .and_then(|balance| balance.parse::<f64>().ok())
        .map(|value| UsageCost::Credits { value });
    Ok(ProviderQuota {
        provider: ProviderId::Codex,
        plan: title_case(account["planType"].as_str().unwrap_or("Codex")),
        account: account["email"]
            .as_str()
            .map(UiText::from)
            .unwrap_or_else(|| UiText::message("generated.codexAccount")),
        windows,
        cost: credits,
    })
}

fn codex_window_identity(
    key: &str,
    duration_minutes: Option<i64>,
    has_single_window: bool,
) -> (&'static str, &'static str) {
    match duration_minutes {
        Some(300) => ("codex-session", "quota.fiveHours"),
        Some(minutes) if minutes >= 24 * 60 => ("codex-global", "quota.global"),
        _ if has_single_window => ("codex-global", "quota.global"),
        _ if key == "primary" => ("codex-session", "quota.session"),
        _ => ("codex-global", "quota.global"),
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
    use std::{fs, time::SystemTime};

    use super::*;

    struct TemporaryHome(PathBuf);

    impl TemporaryHome {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "agent-deck-codex-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(path.join(".codex/sqlite")).expect("create temporary Codex home");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn create_state_db(&self, nested: bool, updated_at_ms: i64) -> PathBuf {
            let path = if nested {
                self.0.join(".codex/sqlite/state_5.sqlite")
            } else {
                self.0.join(".codex/state_5.sqlite")
            };
            let connection = Connection::open(&path).expect("create state database");
            connection
                .execute(
                    "CREATE TABLE threads (updated_at_ms INTEGER, updated_at INTEGER)",
                    [],
                )
                .expect("create threads table");
            connection
                .execute(
                    "INSERT INTO threads (updated_at_ms, updated_at) VALUES (?1, 0)",
                    [updated_at_ms],
                )
                .expect("insert thread timestamp");
            path
        }
    }

    impl Drop for TemporaryHome {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

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
        let quota = parse_quota(&account, &limits).expect("valid current quota");

        assert_eq!(quota.windows.len(), 1);
        assert_eq!(quota.windows[0].id, "codex-global");
        assert!(matches!(
            &quota.windows[0].label,
            UiText::Message { key, .. } if key == "quota.global"
        ));
        assert_eq!(quota.windows[0].used_percent, 39);
        assert_eq!(quota.plan, "Pro");
        assert!(matches!(
            quota.cost,
            Some(UsageCost::Credits { value }) if value == 12.5
        ));
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
        let quota = parse_quota(&account, &limits).expect("valid legacy quota");

        assert_eq!(quota.windows.len(), 2);
        assert!(matches!(
            &quota.windows[0].label,
            UiText::Message { key, .. } if key == "quota.fiveHours"
        ));
        assert!(matches!(
            &quota.windows[1].label,
            UiText::Message { key, .. } if key == "quota.global"
        ));
    }

    #[test]
    fn selects_database_with_the_newest_thread() {
        let home = TemporaryHome::new("database-recency");
        home.create_state_db(false, 1_000);
        let expected = home.create_state_db(true, 2_000);

        assert_eq!(find_codex_state_db(home.path()), Some(expected));
    }

    #[test]
    fn prefers_a_readable_state_database_over_an_invalid_candidate() {
        let home = TemporaryHome::new("database-validity");
        let expected = home.create_state_db(false, 1_000);
        fs::write(
            home.path().join(".codex/sqlite/state_5.sqlite"),
            b"not a sqlite database",
        )
        .expect("write invalid database");

        assert_eq!(find_codex_state_db(home.path()), Some(expected));
    }

    #[test]
    fn incomplete_quota_exchange_reports_the_missing_response() {
        let account = Some(json!({"account": {"planType": "pro"}}));

        let error = complete_quota_exchange(account, None).expect_err("missing rate limits");

        assert!(error.contains("réponse des quotas"));
    }

    #[test]
    fn rejects_quota_without_required_structure() {
        let account = json!({"account": {"planType": "pro"}});

        let missing_limits = parse_quota(&account, &json!({})).expect_err("missing rateLimits");
        let missing_windows =
            parse_quota(&account, &json!({"rateLimits": {}})).expect_err("missing quota window");
        let missing_percent = parse_quota(
            &account,
            &json!({"rateLimits": {"primary": {"windowDurationMins": 10_080}}}),
        )
        .expect_err("missing used percentage");

        assert!(missing_limits.contains("incomplète"));
        assert!(missing_windows.contains("sans fenêtre"));
        assert!(missing_percent.contains("pourcentage"));
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
