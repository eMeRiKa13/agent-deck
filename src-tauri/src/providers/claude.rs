use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    time::UNIX_EPOCH,
};

use chrono::DateTime;
use serde_json::Value;
use walkdir::WalkDir;

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

#[derive(Clone, Debug)]
struct DesktopSessionMetadata {
    session_id: String,
    title: String,
    cwd: String,
    model: String,
    created_at: i64,
    last_activity_at: i64,
    last_focused_at: i64,
    is_archived: bool,
}

impl DesktopSessionMetadata {
    fn updated_at(&self) -> i64 {
        self.last_activity_at.max(self.last_focused_at)
    }
}

pub async fn load() -> Result<ProviderSnapshot, String> {
    let sessions = tokio::task::spawn_blocking(load_sessions)
        .await
        .map_err(|error| format!("Lecture Claude interrompue : {error}"))??;
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

fn load_sessions() -> Result<Vec<AgentSession>, String> {
    let home = home_dir()?;
    let desktop_sessions_root =
        home.join("Library/Application Support/Claude/claude-code-sessions");
    let desktop_metadata = load_desktop_session_metadata(&desktop_sessions_root);
    let mut roots = vec![home.join(".claude/projects")];
    for container in [
        home.join("Library/Application Support/Claude/local-agent-mode-sessions"),
        home.join("Library/Application Support/Claude/claude-code-sessions"),
    ] {
        if !container.exists() {
            continue;
        }
        for entry in WalkDir::new(container)
            .max_depth(8)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_dir()
                && entry.file_name() == "projects"
                && entry.path().parent().and_then(Path::file_name)
                    == Some(std::ffi::OsStr::new(".claude"))
            {
                roots.push(entry.into_path());
            }
        }
    }

    let mut candidates = Vec::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !entry.file_type().is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("jsonl")
                || path
                    .components()
                    .any(|part| part.as_os_str() == "subagents")
            {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or_default();
            let metadata = path
                .file_stem()
                .and_then(|value| value.to_str())
                .and_then(|session_id| desktop_metadata.get(session_id))
                .cloned();
            if metadata
                .as_ref()
                .is_some_and(|metadata| metadata.is_archived)
            {
                continue;
            }
            let updated = metadata
                .as_ref()
                .map(DesktopSessionMetadata::updated_at)
                .filter(|updated| *updated > 0)
                .unwrap_or(modified);
            candidates.push((updated, path.to_path_buf(), metadata));
        }
    }
    candidates.sort_by_key(|(updated, _, _)| std::cmp::Reverse(*updated));
    candidates.dedup_by(|left, right| left.1 == right.1);

    let sessions = candidates
        .into_iter()
        .take(10)
        .filter_map(|(updated, path, metadata)| {
            parse_session(&path, updated, metadata.as_ref()).ok()
        })
        .collect::<Vec<_>>();
    if sessions.is_empty() {
        Err("Aucune conversation locale Claude trouvée".to_string())
    } else {
        Ok(sessions)
    }
}

fn load_desktop_session_metadata(root: &Path) -> HashMap<String, DesktopSessionMetadata> {
    if !root.exists() {
        return HashMap::new();
    }
    WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("json")
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("local_"))
        })
        .filter_map(|entry| {
            let value: Value = serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok()?;
            let cli_session_id = value["cliSessionId"].as_str()?.to_string();
            let session_id = value["sessionId"]
                .as_str()
                .unwrap_or(&cli_session_id)
                .to_string();
            Some((
                cli_session_id,
                DesktopSessionMetadata {
                    session_id,
                    title: value["title"].as_str().unwrap_or_default().to_string(),
                    cwd: value["cwd"].as_str().unwrap_or_default().to_string(),
                    model: value["model"].as_str().unwrap_or_default().to_string(),
                    created_at: value["createdAt"].as_i64().unwrap_or_default(),
                    last_activity_at: value["lastActivityAt"].as_i64().unwrap_or_default(),
                    last_focused_at: value["lastFocusedAt"].as_i64().unwrap_or_default(),
                    is_archived: value["isArchived"].as_bool().unwrap_or(false),
                },
            ))
        })
        .collect()
}

fn parse_session(
    path: &Path,
    modified_ms: i64,
    metadata: Option<&DesktopSessionMetadata>,
) -> Result<AgentSession, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut ai_title = String::new();
    let mut first_user = String::new();
    let mut latest_assistant = String::new();
    let mut cwd = metadata
        .map(|metadata| metadata.cwd.clone())
        .unwrap_or_default();
    let mut branch = String::new();
    let mut model = metadata
        .map(|metadata| metadata.model.clone())
        .unwrap_or_default();
    let mut session_id = metadata
        .map(|metadata| metadata.session_id.clone())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("session")
                .to_string()
        });
    let mut first_timestamp = metadata
        .map(|metadata| metadata.created_at)
        .filter(|created| *created > 0)
        .unwrap_or(modified_ms);
    let mut latest_timestamp = modified_ms;
    let mut last_type = String::new();
    let mut events = VecDeque::with_capacity(6);
    let mut usages: HashMap<String, u64> = HashMap::new();

    for (index, line) in BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .enumerate()
    {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let row_type = value["type"].as_str().unwrap_or_default();
        if row_type == "ai-title" {
            ai_title = value["aiTitle"].as_str().unwrap_or_default().to_string();
            continue;
        }
        if let Some(value) = value["sessionId"].as_str() {
            session_id = value.to_string();
        }
        if let Some(value) = value["cwd"].as_str() {
            cwd = value.to_string();
        }
        if let Some(value) = value["gitBranch"].as_str() {
            branch = value.to_string();
        }
        let timestamp = value["timestamp"].as_str();
        if let Some(timestamp_ms) = timestamp.and_then(parse_iso_ms) {
            if first_timestamp == modified_ms {
                first_timestamp = timestamp_ms;
            }
            latest_timestamp = latest_timestamp.max(timestamp_ms);
        }

        let message = &value["message"];
        let text = message_text(&message["content"]);
        let kind = match row_type {
            "user" if !text.is_empty() => {
                if first_user.is_empty() {
                    first_user = text.clone();
                }
                Some((EventKind::User, "Vous", text))
            }
            "assistant" => {
                if let Some(value) = message["model"].as_str() {
                    model = value.to_string();
                }
                if !text.is_empty() {
                    latest_assistant = text.clone();
                }
                let usage = &message["usage"];
                if usage.is_object() {
                    let key = format!(
                        "{}:{}",
                        message["id"].as_str().unwrap_or("message"),
                        value["requestId"].as_str().unwrap_or("request")
                    );
                    let total = [
                        "input_tokens",
                        "output_tokens",
                        "cache_creation_input_tokens",
                        "cache_read_input_tokens",
                    ]
                    .into_iter()
                    .map(|key| usage[key].as_u64().unwrap_or_default())
                    .sum();
                    usages.insert(key, total);
                }
                if text.is_empty() {
                    tool_name(&message["content"]).map(|name| {
                        (
                            EventKind::Tool,
                            "Outil Claude",
                            format!("Utilisation de {name}"),
                        )
                    })
                } else {
                    Some((EventKind::Assistant, "Claude", text))
                }
            }
            _ => None,
        };
        if matches!(row_type, "user" | "assistant") {
            last_type = row_type.to_string();
        }
        if let Some((kind, title, body)) = kind {
            if events.len() == 6 {
                events.pop_front();
            }
            events.push_back(SessionEvent {
                id: format!("claude-event-{index}"),
                kind,
                title: title.to_string(),
                body: truncate(&body, 240),
                time: event_time(timestamp, latest_timestamp),
            });
        }
    }

    let age_seconds = (crate::utils::now_ms() - normalize_epoch_ms(modified_ms)).max(0) / 1_000;
    let status = if age_seconds < 100 {
        SessionStatus::Working
    } else if last_type == "user" && age_seconds < 900 {
        SessionStatus::Attention
    } else if last_type == "assistant" && age_seconds < 900 {
        SessionStatus::Waiting
    } else {
        SessionStatus::Completed
    };
    let desktop_title = metadata
        .map(|metadata| metadata.title.as_str())
        .unwrap_or_default();
    let title_source = if !desktop_title.is_empty() {
        desktop_title
    } else if !ai_title.is_empty() {
        &ai_title
    } else {
        &first_user
    };
    let summary_source = if !latest_assistant.is_empty() {
        &latest_assistant
    } else {
        title_source
    };

    Ok(AgentSession {
        id: format!("claude:{session_id}"),
        provider: ProviderId::Claude,
        title: truncate(
            if title_source.is_empty() {
                "Conversation Claude"
            } else {
                title_source
            },
            74,
        ),
        project: project_name(&cwd),
        branch: if branch.is_empty() {
            "—".to_string()
        } else {
            branch
        },
        model: if model.is_empty() {
            "Claude".to_string()
        } else {
            model
        },
        status,
        elapsed: elapsed_label(first_timestamp, latest_timestamp),
        updated_at: relative_time(modified_ms),
        token_count: usages.values().sum(),
        summary: truncate(summary_source, 180),
        events: events.into_iter().collect(),
        updated_at_ms: normalize_epoch_ms(modified_ms),
    })
}

fn parse_iso_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.timestamp_millis())
}

fn message_text(content: &Value) -> String {
    if let Some(value) = content.as_str() {
        return value.to_string();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["type"] == "text")
        .filter_map(|entry| entry["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_name(content: &Value) -> Option<String> {
    content
        .as_array()?
        .iter()
        .find(|entry| entry["type"] == "tool_use")?["name"]
        .as_str()
        .map(str::to_string)
}

struct Credentials {
    access_token: String,
    plan: String,
}

async fn load_quota() -> Result<ProviderQuota, String> {
    match load_credentials().await {
        Ok(credentials) => match load_oauth_quota(credentials).await {
            Ok(quota) => Ok(quota),
            Err(remote_error) => load_desktop_quota().map_err(|local_error| {
                format!("{remote_error} · secours Claude Desktop : {local_error}")
            }),
        },
        Err(credentials_error) => load_desktop_quota().map_err(|local_error| {
            format!("{credentials_error} · secours Claude Desktop : {local_error}")
        }),
    }
}

async fn load_oauth_quota(credentials: Credentials) -> Result<ProviderQuota, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(&credentials.access_token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("accept", "application/json")
        .header("user-agent", "claude-code/2.1.0")
        .send()
        .await
        .map_err(|error| format!("Quotas Claude injoignables : {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Quotas Claude : HTTP {}", response.status()));
    }
    let value: Value = response.json().await.map_err(|error| error.to_string())?;
    Ok(parse_quota(&value, credentials.plan))
}

fn load_desktop_quota() -> Result<ProviderQuota, String> {
    let path = home_dir()?.join("Library/Application Support/Claude/plan-usage-history.json");
    let value: Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|error| format!("historique d’usage absent : {error}"))?,
    )
    .map_err(|error| format!("historique d’usage invalide : {error}"))?;
    let sample = value["samples"]
        .as_array()
        .and_then(|samples| samples.last())
        .ok_or("aucune mesure d’usage locale")?;
    let usage = &sample["u"];
    let mut windows = Vec::new();
    if let Some(percent) = usage["fh"].as_f64() {
        windows.push(QuotaWindow {
            id: "claude-session-local".to_string(),
            label: "5 heures".to_string(),
            used_percent: clamp_percent(percent),
            reset_label: "heure de réinitialisation non stockée localement".to_string(),
        });
    }
    if let Some(percent) = usage["sd"].as_f64() {
        windows.push(QuotaWindow {
            id: "claude-week-local".to_string(),
            label: "Hebdomadaire".to_string(),
            used_percent: clamp_percent(percent),
            reset_label: "jour de réinitialisation non stocké localement".to_string(),
        });
    }
    if windows.is_empty() {
        return Err("mesure d’usage locale vide".to_string());
    }
    let measured_at = sample["t"].as_i64().unwrap_or_default();
    Ok(ProviderQuota {
        provider: ProviderId::Claude,
        plan: "Claude App".to_string(),
        account: "Claude Desktop".to_string(),
        windows,
        cost_label: (measured_at > 0)
            .then(|| format!("mesure locale, {}", relative_time(measured_at))),
    })
}

async fn load_credentials() -> Result<Credentials, String> {
    let file = home_dir()?.join(".claude/.credentials.json");
    let data = if file.exists() {
        std::fs::read(&file).map_err(|error| error.to_string())?
    } else if cfg!(target_os = "macos") {
        let mut command = tokio::process::Command::new("/usr/bin/security");
        command
            .args([
                "find-generic-password",
                "-s",
                "Claude Code-credentials",
                "-w",
            ])
            .kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(15), command.output())
            .await
            .map_err(|_| "Délai dépassé pour le Trousseau Claude".to_string())?
            .map_err(|error| format!("Trousseau Claude inaccessible : {error}"))?;
        if !output.status.success() {
            return Err("Identifiants OAuth Claude introuvables".to_string());
        }
        output.stdout
    } else {
        return Err("Identifiants OAuth Claude introuvables".to_string());
    };
    let value: Value =
        serde_json::from_slice(&data).map_err(|_| "Identifiants Claude invalides".to_string())?;
    let oauth = &value["claudeAiOauth"];
    let access_token = oauth["accessToken"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or("Jeton OAuth Claude absent")?
        .to_string();
    let plan = oauth["subscriptionType"]
        .as_str()
        .or_else(|| oauth["rateLimitTier"].as_str())
        .map(format_plan)
        .unwrap_or_else(|| "Claude".to_string());
    Ok(Credentials { access_token, plan })
}

fn parse_quota(value: &Value, plan: String) -> ProviderQuota {
    let mut windows = Vec::new();
    for (id, label, key) in [
        ("claude-session", "5 heures", "five_hour"),
        ("claude-week", "Hebdomadaire", "seven_day"),
        ("claude-sonnet", "Sonnet hebdomadaire", "seven_day_sonnet"),
        ("claude-opus", "Opus hebdomadaire", "seven_day_opus"),
    ] {
        let window = &value[key];
        if window.is_object() {
            windows.push(QuotaWindow {
                id: id.to_string(),
                label: label.to_string(),
                used_percent: clamp_percent(window["utilization"].as_f64().unwrap_or_default()),
                reset_label: reset_label_from_iso(window["resets_at"].as_str()),
            });
        }
    }
    if let Some(limits) = value["limits"].as_array() {
        for (index, limit) in limits
            .iter()
            .filter(|limit| limit["kind"] == "weekly_scoped")
            .take(3)
            .enumerate()
        {
            let name = limit["scope"]["model"]["display_name"]
                .as_str()
                .unwrap_or("Modèle");
            windows.push(QuotaWindow {
                id: format!("claude-scoped-{index}"),
                label: format!("{name} hebdomadaire"),
                used_percent: clamp_percent(limit["percent"].as_f64().unwrap_or_default()),
                reset_label: reset_label_from_iso(limit["resets_at"].as_str()),
            });
        }
    }
    let extra = &value["extra_usage"];
    let cost_label = extra["used_credits"].as_f64().map(|used| {
        let currency = extra["currency"].as_str().unwrap_or("USD");
        format!("{used:.2} {currency} d’usage supplémentaire")
    });
    ProviderQuota {
        provider: ProviderId::Claude,
        plan,
        account: "Compte Claude".to_string(),
        windows,
        cost_label,
    }
}

fn format_plan(value: &str) -> String {
    let lower = value.to_lowercase();
    if lower.contains("max_20x") || lower.contains("max20") {
        "Max 20×".to_string()
    } else if lower.contains("max_5x") || lower.contains("max5") {
        "Max 5×".to_string()
    } else if lower.contains("max") {
        "Max".to_string()
    } else if lower.contains("pro") {
        "Pro".to_string()
    } else if lower.contains("team") {
        "Team".to_string()
    } else {
        value.replace('_', " ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_oauth_windows() {
        let quota = parse_quota(
            &json!({
                "five_hour": {"utilization": 72.4, "resets_at": "2030-01-01T10:00:00Z"},
                "seven_day": {"utilization": 31.0, "resets_at": "2030-01-07T10:00:00Z"}
            }),
            "Max 20×".to_string(),
        );
        assert_eq!(quota.windows.len(), 2);
        assert_eq!(quota.windows[0].used_percent, 72);
    }

    #[test]
    fn parses_desktop_usage_history() {
        let sample = serde_json::json!({"t": 2_000_000_000_000_i64, "u": {"fh": 31, "sd": 12}});
        let usage = &sample["u"];
        assert_eq!(clamp_percent(usage["fh"].as_f64().unwrap()), 31);
        assert_eq!(clamp_percent(usage["sd"].as_f64().unwrap()), 12);
    }

    #[test]
    fn extracts_text_blocks() {
        let content =
            json!([{"type": "text", "text": "Bonjour"}, {"type": "tool_use", "name": "Read"}]);
        assert_eq!(message_text(&content), "Bonjour");
        assert_eq!(tool_name(&content).as_deref(), Some("Read"));
    }

    #[test]
    fn desktop_metadata_uses_the_most_recent_activity_or_focus() {
        let metadata = DesktopSessionMetadata {
            session_id: "local-session".to_string(),
            title: "Session active".to_string(),
            cwd: "/tmp/project".to_string(),
            model: "claude-test".to_string(),
            created_at: 100,
            last_activity_at: 200,
            last_focused_at: 300,
            is_archived: false,
        };

        assert_eq!(metadata.updated_at(), 300);
    }

    #[test]
    #[ignore = "dépend des données Claude installées sur la machine"]
    fn reads_local_sessions() {
        let sessions = load_sessions().expect("historique Claude local");
        let latest = sessions.first().expect("au moins une session Claude");
        assert!(
            crate::utils::now_ms() - latest.updated_at_ms < 24 * 60 * 60 * 1_000,
            "la session Claude la plus récente devrait dater de moins de 24 h"
        );
    }

    #[test]
    #[ignore = "dépend de l’historique d’usage de Claude Desktop"]
    fn reads_local_desktop_quota() {
        assert!(!load_desktop_quota()
            .expect("quota Claude Desktop")
            .windows
            .is_empty());
    }
}
