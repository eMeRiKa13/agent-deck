use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use chrono::DateTime;
use serde_json::{Map, Value};
use walkdir::WalkDir;

use crate::{
    domain::{
        AgentSession, EventKind, ProviderId, ProviderQuota, ProviderSnapshot, QuotaWindow,
        SessionEvent, SessionStatus, UiText, UsageCost,
    },
    utils::{
        clamp_percent, event_timestamp_ms, home_dir, normalize_epoch_ms, project_name,
        reset_at_ms_from_iso, truncate,
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

type SessionCandidate = (i64, PathBuf, Option<DesktopSessionMetadata>);

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

// Discover desktop and local-agent transcripts, then keep ten valid sessions.
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

    let sessions = parse_candidate_sessions(candidates, 10);
    if sessions.is_empty() {
        Err("Aucune conversation locale Claude trouvée".to_string())
    } else {
        Ok(sessions)
    }
}

// Skip unreadable candidates without reducing the requested result count.
fn parse_candidate_sessions(candidates: Vec<SessionCandidate>, limit: usize) -> Vec<AgentSession> {
    candidates
        .into_iter()
        .filter_map(|(updated, path, metadata)| {
            parse_session(&path, updated, metadata.as_ref()).ok()
        })
        .take(limit)
        .collect()
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

// Convert a Claude JSONL transcript into the compact dashboard representation.
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
                Some((
                    EventKind::User,
                    UiText::message("generated.you"),
                    UiText::from(text),
                ))
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
                        let mut params = Map::new();
                        params.insert("name".to_string(), Value::String(name));
                        (
                            EventKind::Tool,
                            UiText::message("generated.claudeTool"),
                            UiText::message_with("generated.toolUsage", params),
                        )
                    })
                } else {
                    Some((
                        EventKind::Assistant,
                        UiText::from("Claude"),
                        UiText::from(text),
                    ))
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
                title,
                body: match body {
                    UiText::Raw(value) => UiText::from(truncate(&value, 240)),
                    message => message,
                },
                timestamp_ms: event_timestamp_ms(timestamp, latest_timestamp),
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
        title: if title_source.is_empty() {
            UiText::message("generated.claudeConversation")
        } else {
            UiText::from(truncate(title_source, 74))
        },
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
        started_at_ms: normalize_epoch_ms(first_timestamp),
        updated_at_ms: normalize_epoch_ms(modified_ms),
        token_count: usages.values().sum(),
        summary: if summary_source.is_empty() {
            UiText::message("generated.claudeConversation")
        } else {
            UiText::from(truncate(summary_source, 180))
        },
        events: events.into_iter().collect(),
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
    parse_quota(&value, credentials.plan)
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
            label: UiText::message("quota.fiveHours"),
            used_percent: clamp_percent(percent),
            reset_at_ms: None,
            reset_text: Some(UiText::message("quota.resetHourNotStored")),
        });
    }
    if let Some(percent) = usage["sd"].as_f64() {
        windows.push(QuotaWindow {
            id: "claude-week-local".to_string(),
            label: UiText::message("quota.weekly"),
            used_percent: clamp_percent(percent),
            reset_at_ms: None,
            reset_text: Some(UiText::message("quota.resetDayNotStored")),
        });
    }
    if windows.is_empty() {
        return Err("mesure d’usage locale vide".to_string());
    }
    let measured_at = sample["t"].as_i64().unwrap_or_default();
    Ok(ProviderQuota {
        provider: ProviderId::Claude,
        plan: "Claude App".to_string(),
        account: UiText::from("Claude Desktop"),
        windows,
        cost: (measured_at > 0).then_some(UsageCost::LocalMeasurement {
            measured_at_ms: normalize_epoch_ms(measured_at),
        }),
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

// Validate OAuth quota windows before exposing percentages to the dashboard.
fn parse_quota(value: &Value, plan: String) -> Result<ProviderQuota, String> {
    if !value.is_object() {
        return Err("Réponse de quota Claude invalide".to_string());
    }
    let mut windows = Vec::new();
    for (id, label, key) in [
        ("claude-session", "quota.fiveHours", "five_hour"),
        ("claude-week", "quota.weekly", "seven_day"),
        ("claude-sonnet", "quota.sonnetWeekly", "seven_day_sonnet"),
        ("claude-opus", "quota.opusWeekly", "seven_day_opus"),
    ] {
        let window = &value[key];
        if window.is_object() {
            let utilization = window["utilization"]
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("Quota Claude {key} sans utilisation valide"))?;
            let reset_at_ms = match window.get("resets_at") {
                Some(Value::String(value)) => reset_at_ms_from_iso(Some(value))
                    .ok_or_else(|| format!("Date de réinitialisation Claude {key} invalide"))
                    .map(Some)?,
                Some(Value::Null) | None => None,
                Some(_) => return Err(format!("Date de réinitialisation Claude {key} invalide")),
            };
            windows.push(QuotaWindow {
                id: id.to_string(),
                label: UiText::message(label),
                used_percent: clamp_percent(utilization),
                reset_at_ms,
                reset_text: Some(UiText::message("quota.resetUnknown")),
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
            let percent = limit["percent"]
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("Quota Claude limité sans pourcentage valide")?;
            let reset_at_ms = match limit.get("resets_at") {
                Some(Value::String(value)) => reset_at_ms_from_iso(Some(value))
                    .ok_or("Date de quota Claude limité invalide")
                    .map(Some)?,
                Some(Value::Null) | None => None,
                Some(_) => return Err("Date de quota Claude limité invalide".to_string()),
            };
            let name = limit["scope"]["model"]["display_name"]
                .as_str()
                .unwrap_or("Model");
            let mut params = Map::new();
            params.insert("model".to_string(), Value::String(name.to_string()));
            windows.push(QuotaWindow {
                id: format!("claude-scoped-{index}"),
                label: UiText::message_with("quota.modelWeekly", params),
                used_percent: clamp_percent(percent),
                reset_at_ms,
                reset_text: Some(UiText::message("quota.resetUnknown")),
            });
        }
    }
    if windows.is_empty() {
        return Err("Réponse de quota Claude sans fenêtre d’utilisation".to_string());
    }
    let extra = &value["extra_usage"];
    let cost = match extra.get("used_credits") {
        Some(Value::Number(value)) => Some(UsageCost::ExtraUsage {
            value: value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("Crédits Claude invalides")?,
            currency: extra["currency"].as_str().unwrap_or("USD").to_string(),
        }),
        Some(Value::Null) | None => None,
        Some(_) => return Err("Crédits Claude invalides".to_string()),
    };
    Ok(ProviderQuota {
        provider: ProviderId::Claude,
        plan,
        account: UiText::message("generated.claudeAccount"),
        windows,
        cost,
    })
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
        )
        .expect("valid Claude quota");
        assert_eq!(quota.windows.len(), 2);
        assert_eq!(quota.windows[0].used_percent, 72);
    }

    #[test]
    fn rejects_incomplete_oauth_quota() {
        assert!(parse_quota(&json!({}), "Pro".to_string()).is_err());
        assert!(parse_quota(
            &json!({"five_hour": {"resets_at": "2030-01-01T10:00:00Z"}}),
            "Pro".to_string(),
        )
        .is_err());
    }

    #[test]
    fn skips_invalid_candidates_until_limit_is_reached() {
        let root = std::env::temp_dir().join(format!(
            "agent-deck-claude-candidates-{}-{}",
            std::process::id(),
            crate::utils::now_ms()
        ));
        std::fs::create_dir_all(&root).expect("temporary candidate directory");
        let mut candidates = vec![(1, root.join("missing.jsonl"), None)];
        for index in 0..11 {
            let path = root.join(format!("session-{index}.jsonl"));
            std::fs::write(
                &path,
                format!(
                    "{{\"type\":\"user\",\"sessionId\":\"session-{index}\",\"timestamp\":\"2030-01-01T10:00:00Z\",\"message\":{{\"content\":\"hello {index}\"}}}}\n"
                ),
            )
            .expect("Claude transcript fixture");
            candidates.push((1_000 + index, path, None));
        }

        let sessions = parse_candidate_sessions(candidates, 10);
        std::fs::remove_dir_all(root).expect("remove candidate directory");

        assert_eq!(sessions.len(), 10);
        assert!(sessions
            .iter()
            .all(|session| session.id != "claude:session-10"));
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
