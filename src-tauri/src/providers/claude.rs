use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::UNIX_EPOCH,
};

use chrono::DateTime;
use serde_json::{Map, Value};
use walkdir::WalkDir;

#[cfg(target_os = "macos")]
mod desktop;

use crate::{
    domain::{
        ActivityGroup, ActivityItem, ActivityKind, ActivityStatus, AgentSession, ProviderId,
        ProviderQuota, ProviderSessionDetails, ProviderSnapshot, QuotaWindow, ResetOffer,
        SessionMetrics, SessionStatus, TokenUsage, TokenUsageQuality, UiText, UsageCost,
    },
    providers::partial_snapshot,
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
    effort: String,
    completed_turns: Option<u32>,
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

struct SessionCandidate {
    updated_at_ms: i64,
    path: PathBuf,
    metadata: Option<DesktopSessionMetadata>,
}

#[derive(Clone)]
struct ParsedSession {
    session: AgentSession,
    idle_status: SessionStatus,
}

#[derive(Clone)]
struct CachedSession {
    file_len: u64,
    file_modified_ns: u128,
    modified_ms: i64,
    metadata_updated_ms: i64,
    parsed: ParsedSession,
}

static SESSION_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedSession>>> = OnceLock::new();

// Load local history off the async runtime and treat remote quota access as optional data.
pub async fn load(include_quotas: bool) -> Result<ProviderSnapshot, String> {
    let sessions = match tokio::task::spawn_blocking(load_sessions).await {
        Ok(result) => result,
        Err(error) => Err(format!("Lecture Claude interrompue : {error}")),
    };
    let quota = if include_quotas {
        Some(load_quota().await)
    } else {
        None
    };
    partial_snapshot(sessions, quota)
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
            candidates.push(SessionCandidate {
                updated_at_ms: updated,
                path: path.to_path_buf(),
                metadata,
            });
        }
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.updated_at_ms));
    candidates.dedup_by(|left, right| left.path == right.path);

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
        .filter_map(|candidate| {
            cached_session(
                &candidate.path,
                candidate.updated_at_ms,
                candidate.metadata.as_ref(),
            )
            .ok()
        })
        .take(limit)
        .collect()
}

// Reuse parsed transcripts until either the JSONL file or its desktop metadata changes.
fn cached_session(
    path: &Path,
    modified_ms: i64,
    metadata: Option<&DesktopSessionMetadata>,
) -> Result<AgentSession, String> {
    let file_metadata = path.metadata().map_err(|error| error.to_string())?;
    let file_len = file_metadata.len();
    let file_modified_ns = file_metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let metadata_updated_ms = metadata
        .map(DesktopSessionMetadata::updated_at)
        .unwrap_or_default();
    let cache = SESSION_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .map_err(|_| "Cache Claude indisponible".to_string())?
        .get(path)
        .filter(|cached| {
            cached.file_len == file_len
                && cached.file_modified_ns == file_modified_ns
                && cached.modified_ms == modified_ms
                && cached.metadata_updated_ms == metadata_updated_ms
        })
        .cloned()
    {
        return Ok(refresh_session_status(cached.parsed, modified_ms));
    }

    let parsed = parse_session(path, modified_ms, metadata)?;
    let session = refresh_session_status(parsed.clone(), modified_ms);
    let mut cache = cache
        .lock()
        .map_err(|_| "Cache Claude indisponible".to_string())?;
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(
        path.to_path_buf(),
        CachedSession {
            file_len,
            file_modified_ns,
            modified_ms,
            metadata_updated_ms,
            parsed,
        },
    );
    Ok(session)
}

// Recompute time-sensitive status without reparsing an unchanged transcript.
fn refresh_session_status(mut parsed: ParsedSession, modified_ms: i64) -> AgentSession {
    let age_seconds = (crate::utils::now_ms() - normalize_epoch_ms(modified_ms)).max(0) / 1_000;
    parsed.session.status = if age_seconds < 100 {
        SessionStatus::Working
    } else if age_seconds < 900 {
        parsed.idle_status
    } else {
        SessionStatus::Completed
    };
    parsed.session
}

// Index Claude Desktop metadata by CLI session ID so JSONL transcripts receive current labels.
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
                    effort: value["effort"].as_str().unwrap_or_default().to_string(),
                    completed_turns: value["completedTurns"].as_u64().map(|value| value as u32),
                    created_at: value["createdAt"].as_i64().unwrap_or_default(),
                    last_activity_at: value["lastActivityAt"].as_i64().unwrap_or_default(),
                    last_focused_at: value["lastFocusedAt"].as_i64().unwrap_or_default(),
                    is_archived: value["isArchived"].as_bool().unwrap_or(false),
                },
            ))
        })
        .collect()
}

#[derive(Clone, Copy, Default)]
struct ClaudeUsage {
    input: u64,
    output: u64,
    cached_input: u64,
    cache_write_input: u64,
}

impl ClaudeUsage {
    fn from_value(value: &Value) -> Self {
        Self {
            input: value["input_tokens"].as_u64().unwrap_or_default(),
            output: value["output_tokens"].as_u64().unwrap_or_default(),
            cached_input: value["cache_read_input_tokens"]
                .as_u64()
                .unwrap_or_default(),
            cache_write_input: value["cache_creation_input_tokens"]
                .as_u64()
                .unwrap_or_default(),
        }
    }

    fn total(self) -> u64 {
        self.input + self.output + self.cached_input + self.cache_write_input
    }
}

fn optional_string(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

fn claude_item(
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

fn claude_group(id: String, timestamp_ms: i64) -> ActivityGroup {
    ActivityGroup {
        id,
        started_at_ms: timestamp_ms,
        completed_at_ms: None,
        status: None,
        items: Vec::new(),
    }
}

// Convert a Claude JSONL transcript into normalized turns without exposing thinking content.
fn parse_session(
    path: &Path,
    modified_ms: i64,
    metadata: Option<&DesktopSessionMetadata>,
) -> Result<ParsedSession, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut ai_title = String::new();
    let mut custom_title = String::new();
    let mut first_user = String::new();
    let mut latest_assistant = String::new();
    let mut mode = String::new();
    let mut effort = metadata
        .map(|value| value.effort.clone())
        .unwrap_or_default();
    let mut cwd = metadata.map(|value| value.cwd.clone()).unwrap_or_default();
    let mut branch = String::new();
    let mut model = metadata
        .map(|value| value.model.clone())
        .unwrap_or_default();
    let mut session_id = metadata
        .map(|value| value.session_id.clone())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("session")
                .to_string()
        });
    let mut first_timestamp = metadata
        .map(|value| value.created_at)
        .filter(|value| *value > 0)
        .unwrap_or(modified_ms);
    let mut latest_timestamp = modified_ms;
    let mut last_type = String::new();
    let mut activity_groups: Vec<ActivityGroup> = Vec::new();
    let mut assistant_positions: HashMap<String, (usize, usize)> = HashMap::new();
    let mut tool_positions: HashMap<String, (usize, usize)> = HashMap::new();
    let mut usages: HashMap<String, ClaudeUsage> = HashMap::new();

    for (index, line) in BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .enumerate()
    {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let row_type = value["type"].as_str().unwrap_or_default();
        match row_type {
            "ai-title" => {
                ai_title = value["aiTitle"].as_str().unwrap_or_default().to_string();
                continue;
            }
            "custom-title" => {
                custom_title = value["customTitle"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                continue;
            }
            "mode" => {
                mode = value["mode"].as_str().unwrap_or_default().to_string();
                continue;
            }
            _ => {}
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
        let event_ms = event_timestamp_ms(timestamp, latest_timestamp);
        let message = &value["message"];
        let content = &message["content"];
        let text = message_text(content);

        if row_type == "user" {
            let tool_results = content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|entry| entry["type"] == "tool_result")
                .collect::<Vec<_>>();
            for result in &tool_results {
                if let Some(tool_id) = result["tool_use_id"].as_str() {
                    if let Some(&(group_index, item_index)) = tool_positions.get(tool_id) {
                        activity_groups[group_index].items[item_index].status =
                            Some(if result["is_error"].as_bool().unwrap_or(false) {
                                ActivityStatus::Error
                            } else {
                                ActivityStatus::Completed
                            });
                    }
                }
            }
            if !text.is_empty() && tool_results.is_empty() {
                if first_user.is_empty() {
                    first_user = text.clone();
                }
                let id = value["promptId"]
                    .as_str()
                    .or_else(|| value["uuid"].as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("claude-turn-{index}"));
                activity_groups.push(claude_group(id, event_ms));
                activity_groups.last_mut().unwrap().items.push(claude_item(
                    format!("claude-user-{index}"),
                    ActivityKind::User,
                    UiText::message("generated.you"),
                    UiText::from(truncate(&text, 360)),
                    event_ms,
                ));
                last_type = "user".to_string();
            }
            continue;
        }
        if row_type != "assistant" {
            continue;
        }

        if let Some(value) = message["model"].as_str() {
            model = value.to_string();
        }
        if let Some(value) = value["effort"].as_str() {
            effort = value.to_string();
        }
        if activity_groups.is_empty() {
            activity_groups.push(claude_group(format!("claude-turn-{index}"), event_ms));
        }
        let group_index = activity_groups.len() - 1;
        let response_key = format!(
            "{}:{}",
            message["id"].as_str().unwrap_or("message"),
            value["requestId"].as_str().unwrap_or("request")
        );
        if message["usage"].is_object() {
            usages.insert(
                response_key.clone(),
                ClaudeUsage::from_value(&message["usage"]),
            );
        }
        if !text.is_empty() {
            latest_assistant = text.clone();
            if let Some(&(existing_group, item_index)) = assistant_positions.get(&response_key) {
                let item = &mut activity_groups[existing_group].items[item_index];
                item.body = UiText::from(truncate(&text, 360));
                item.timestamp_ms = event_ms;
            } else {
                let item_index = activity_groups[group_index].items.len();
                activity_groups[group_index].items.push(claude_item(
                    format!("claude-assistant-{index}"),
                    ActivityKind::Assistant,
                    UiText::from("Claude"),
                    UiText::from(truncate(&text, 360)),
                    event_ms,
                ));
                assistant_positions.insert(response_key, (group_index, item_index));
            }
        }
        let mut has_tool = false;
        for block in content
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["type"] == "tool_use")
        {
            has_tool = true;
            let tool_id = block["id"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("claude-tool-{index}"));
            if tool_positions.contains_key(&tool_id) {
                continue;
            }
            let item_index = activity_groups[group_index].items.len();
            let mut item = claude_item(
                tool_id.clone(),
                ActivityKind::Tool,
                UiText::from(block["name"].as_str().unwrap_or("Outil Claude")),
                UiText::message("generated.localToolExecution"),
                event_ms,
            );
            item.status = Some(ActivityStatus::Running);
            activity_groups[group_index].items.push(item);
            tool_positions.insert(tool_id, (group_index, item_index));
        }
        if !text.is_empty() || has_tool {
            last_type = "assistant".to_string();
        }
    }

    for group in &mut activity_groups {
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
    let tool_call_count = activity_groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|item| item.kind == ActivityKind::Tool)
        .count() as u32;
    let tool_error_count = activity_groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|item| item.status == Some(ActivityStatus::Error))
        .count() as u32;
    let total_turn_count = activity_groups.len() as u32;
    let mut activity_truncated = false;
    if activity_groups.len() > 20 {
        activity_groups.drain(..activity_groups.len() - 20);
        activity_truncated = true;
    }
    let usage = usages
        .values()
        .copied()
        .fold(ClaudeUsage::default(), |total, value| ClaudeUsage {
            input: total.input + value.input,
            output: total.output + value.output,
            cached_input: total.cached_input + value.cached_input,
            cache_write_input: total.cache_write_input + value.cache_write_input,
        });
    let token_usage = (usage.total() > 0).then_some(TokenUsage {
        quality: TokenUsageQuality::Detailed,
        total: usage.total(),
        input: Some(usage.input),
        output: Some(usage.output),
        cached_input: Some(usage.cached_input),
        cache_write_input: Some(usage.cache_write_input),
        reasoning_output: None,
    });
    let idle_status = match last_type.as_str() {
        "user" => SessionStatus::Attention,
        "assistant" => SessionStatus::Waiting,
        _ => SessionStatus::Completed,
    };
    let desktop_title = metadata
        .map(|value| value.title.as_str())
        .unwrap_or_default();
    let title_source = if !desktop_title.is_empty() {
        desktop_title
    } else if !custom_title.is_empty() {
        &custom_title
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

    Ok(ParsedSession {
        idle_status,
        session: AgentSession {
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
            model: optional_string(&model),
            status: SessionStatus::Completed,
            started_at_ms: normalize_epoch_ms(first_timestamp),
            updated_at_ms: normalize_epoch_ms(modified_ms),
            token_usage,
            metrics: SessionMetrics {
                turn_count: metadata
                    .and_then(|value| value.completed_turns)
                    .or(Some(total_turn_count)),
                tool_call_count: Some(tool_call_count),
                tool_error_count: Some(tool_error_count),
                ..SessionMetrics::default()
            },
            provider_details: ProviderSessionDetails::Claude {
                effort: optional_string(&effort),
                mode: optional_string(&mode),
                completed_turns: metadata.and_then(|value| value.completed_turns),
            },
            summary: if summary_source.is_empty() {
                UiText::message("generated.claudeConversation")
            } else {
                UiText::from(truncate(summary_source, 180))
            },
            activity_groups,
            activity_truncated,
        },
    })
}

fn parse_iso_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.timestamp_millis())
}

// Flatten Claude's string-or-block message representation into searchable display text.
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

struct Credentials {
    access_token: String,
    plan: String,
}

// Read live Desktop usage first; historical samples no longer track the current limits API.
async fn load_quota() -> Result<ProviderQuota, String> {
    #[cfg(target_os = "macos")]
    let desktop_error = match desktop::load_quota().await {
        Ok(quota) => return Ok(quota),
        Err(error) => Some(error),
    };
    #[cfg(not(target_os = "macos"))]
    let desktop_error: Option<String> = None;

    let oauth = match load_credentials().await {
        Ok(credentials) => load_oauth_quota(credentials).await,
        Err(error) => Err(error),
    };
    oauth.map_err(|error| match desktop_error {
        Some(desktop_error) => format!("{desktop_error} · secours OAuth : {error}"),
        None => error,
    })
}

// Fetch the private OAuth usage payload with a bounded request timeout.
async fn load_oauth_quota(credentials: Credentials) -> Result<ProviderQuota, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get("https://api.anthropic.com/api/oauth/usage?cedar_ember=1")
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

// Read OAuth credentials from Claude's local configuration without exposing the token to Vue.
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
    // An explicit limits array is authoritative, including an empty array.
    // Never let legacy zero placeholders overwrite the current measurements.
    if let Some(limits) = value.get("limits").filter(|limits| !limits.is_null()) {
        let limits = limits
            .as_array()
            .ok_or("Format des limites Claude invalide")?;
        windows.extend(
            limits
                .iter()
                .enumerate()
                .filter_map(|(index, limit)| parse_current_limit(limit, index)),
        );
    } else {
        for (id, label, key) in [
            ("claude-session", "quota.session", "five_hour"),
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
                    Some(_) => {
                        return Err(format!("Date de réinitialisation Claude {key} invalide"))
                    }
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
        reset_offers: parse_reset_offers(value, crate::utils::now_ms()),
    })
}

fn parse_reset_offers(value: &Value, now: i64) -> Vec<ResetOffer> {
    let reset = &value["cedar_ember"];
    if reset["eligible"] != true {
        return Vec::new();
    }
    reset["grants"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|grant| {
            if grant["type"] != "full"
                || grant["paused"] == true
                || !grant["unavailable_reason"].is_null()
            {
                return None;
            }
            let count = grant["resets_left"]
                .as_u64()
                .and_then(|count| u32::try_from(count).ok())
                .filter(|count| *count > 0)?;
            if let Some(starts_at) = grant["starts_at"].as_str() {
                if reset_at_ms_from_iso(Some(starts_at)).is_none_or(|starts| starts > now) {
                    return None;
                }
            }
            let expires_at_ms = match grant.get("ends_at") {
                Some(Value::String(value)) => {
                    Some(reset_at_ms_from_iso(Some(value)).filter(|expiry| *expiry > now)?)
                }
                Some(Value::Null) | None => None,
                _ => return None,
            };
            Some(ResetOffer {
                count,
                expires_at_ms,
            })
        })
        .collect()
}

// Parse siblings independently so one malformed optional window cannot hide valid usage.
fn parse_current_limit(limit: &Value, index: usize) -> Option<QuotaWindow> {
    let percent = limit["percent"]
        .as_f64()
        .filter(|percent| percent.is_finite())?;
    let reset_at_ms = match limit.get("resets_at") {
        Some(Value::String(value)) => Some(reset_at_ms_from_iso(Some(value))?),
        Some(Value::Null) | None => None,
        Some(_) => return None,
    };
    let (id, label) = match limit["kind"].as_str()? {
        "session" => (
            "claude-session".to_string(),
            UiText::message("quota.session"),
        ),
        "weekly_all" => ("claude-week".to_string(), UiText::message("quota.weekly")),
        "weekly_scoped" => {
            let name = limit["scope"]["model"]["display_name"]
                .as_str()
                .or_else(|| limit["scope"]["surface"]["display_name"].as_str())?
                .trim();
            if name.is_empty() {
                return None;
            }
            let mut params = Map::new();
            params.insert("model".to_string(), Value::String(name.to_string()));
            (
                format!("claude-scoped-{index}"),
                UiText::message_with("quota.modelWeekly", params),
            )
        }
        _ => return None,
    };
    Some(QuotaWindow {
        id,
        label,
        used_percent: clamp_percent(percent),
        reset_at_ms,
        reset_text: Some(UiText::message("quota.resetUnknown")),
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
    fn current_limits_override_legacy_zero_placeholders() {
        let quota = parse_quota(&json!({
            "five_hour": {"utilization":0}, "seven_day":{"utilization":0},
            "limits": [
                {"kind":"session","percent":42.0,"resets_at":"2030-01-01T10:00:00Z"},
                {"kind":"weekly_all","percent":19},
                {"kind":"weekly_scoped","percent":76,"scope":{"model":{"display_name":"Fable"}}},
                {"kind":"weekly_scoped","percent":8,"scope":{"surface":{"display_name":"Cowork"}}}
            ]
        }), "Claude App".into()).unwrap();
        assert_eq!(
            quota
                .windows
                .iter()
                .map(|window| window.used_percent)
                .collect::<Vec<_>>(),
            vec![42, 19, 76, 8]
        );
        assert!(quota.windows[0].reset_at_ms.is_some());
        assert_eq!(quota.windows[0].id, "claude-session");
        assert_eq!(quota.windows[1].id, "claude-week");
    }

    #[test]
    fn malformed_siblings_do_not_hide_real_limits_or_invent_zero_usage() {
        let quota = parse_quota(
            &json!({"limits":[
                {"kind":"session","percent":"0"},
                {"kind":"weekly_scoped","percent":true},
                {"kind":"weekly_all","percent":37},
                {"kind":"session","percent":8,"resets_at":"invalid"}
            ]}),
            "Pro".into(),
        )
        .unwrap();
        assert_eq!(quota.windows.len(), 1);
        assert_eq!(quota.windows[0].used_percent, 37);
        assert!(parse_quota(
            &json!({"limits":[],"five_hour":{"utilization":0}}),
            "Pro".into()
        )
        .is_err());
        assert!(parse_quota(&json!({"limits":[{"kind":"session"}]}), "Pro".into()).is_err());
        let zero = parse_quota(
            &json!({"limits":[{"kind":"session","percent":0}]}),
            "Pro".into(),
        )
        .unwrap();
        assert_eq!(zero.windows[0].used_percent, 0);
    }

    #[test]
    fn projects_real_full_reset_grants_without_redemption_identifiers() {
        let value = json!({"cedar_ember":{"eligible":true,"grants":[
            {"id":"private-redemption-id","type":"full","resets_left":1,"ends_at":"2030-01-01T00:00:00Z","paused":false,"unavailable_reason":null},
            {"type":"full","resets_left":1,"ends_at":"2020-01-01T00:00:00Z"},
            {"type":"full","resets_left":0},
            {"type":"partial","resets_left":1},
            {"type":"full","resets_left":1,"paused":true},
            {"type":"full","resets_left":1,"ends_at":"invalid"}
        ]}});
        let offers = parse_reset_offers(&value, 1_800_000_000_000);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].count, 1);
        assert!(offers[0].expires_at_ms.is_some());
        assert!(!serde_json::to_string(&offers)
            .unwrap()
            .contains("private-redemption-id"));
        assert!(parse_reset_offers(&json!({"cedar_ember":null}), 1_800_000_000_000).is_empty());
    }

    #[test]
    fn skips_invalid_candidates_until_limit_is_reached() {
        let root = std::env::temp_dir().join(format!(
            "agent-deck-claude-candidates-{}-{}",
            std::process::id(),
            crate::utils::now_ms()
        ));
        std::fs::create_dir_all(&root).expect("temporary candidate directory");
        let mut candidates = vec![SessionCandidate {
            updated_at_ms: 1,
            path: root.join("missing.jsonl"),
            metadata: None,
        }];
        for index in 0..11 {
            let path = root.join(format!("session-{index}.jsonl"));
            std::fs::write(
                &path,
                format!(
                    "{{\"type\":\"user\",\"sessionId\":\"session-{index}\",\"timestamp\":\"2030-01-01T10:00:00Z\",\"message\":{{\"content\":\"hello {index}\"}}}}\n"
                ),
            )
            .expect("Claude transcript fixture");
            candidates.push(SessionCandidate {
                updated_at_ms: 1_000 + index,
                path,
                metadata: None,
            });
        }

        let sessions = parse_candidate_sessions(candidates, 10);
        std::fs::remove_dir_all(root).expect("remove candidate directory");

        assert_eq!(sessions.len(), 10);
        assert!(sessions
            .iter()
            .all(|session| session.id != "claude:session-10"));
    }

    #[test]
    fn invalidates_cached_session_when_transcript_grows() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "agent-deck-claude-cache-{}-{}",
            std::process::id(),
            crate::utils::now_ms()
        ));
        std::fs::create_dir_all(&root).expect("temporary cache directory");
        let path = root.join("session.jsonl");
        std::fs::write(
            &path,
            "{\"type\":\"user\",\"sessionId\":\"cached\",\"message\":{\"content\":\"hello\"}}\n",
        )
        .expect("initial transcript");

        let first = cached_session(&path, 1_000, None).expect("first cached session");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("append transcript");
        writeln!(
            file,
            "{{\"type\":\"assistant\",\"sessionId\":\"cached\",\"message\":{{\"content\":\"updated\"}}}}"
        )
        .expect("updated transcript");
        let second = cached_session(&path, 2_000, None).expect("refreshed cached session");
        std::fs::remove_dir_all(root).expect("remove cache directory");

        assert_eq!(first.activity_groups.len(), 1);
        assert_eq!(second.activity_groups[0].items.len(), 2);
    }

    #[test]
    fn extracts_text_blocks() {
        let content =
            json!([{"type": "text", "text": "Bonjour"}, {"type": "tool_use", "name": "Read"}]);
        assert_eq!(message_text(&content), "Bonjour");
    }

    #[test]
    fn desktop_metadata_uses_the_most_recent_activity_or_focus() {
        let metadata = DesktopSessionMetadata {
            session_id: "local-session".to_string(),
            title: "Session active".to_string(),
            cwd: "/tmp/project".to_string(),
            model: "claude-test".to_string(),
            effort: "high".to_string(),
            completed_turns: Some(1),
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

    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore = "requiert la session Claude Desktop et son accès réseau authentifié"]
    async fn reads_live_desktop_quota() {
        let quota = desktop::load_quota().await.expect("quota Claude Desktop");
        assert!(!quota.windows.is_empty());
        println!(
            "Claude live usage: {:?}",
            quota
                .windows
                .iter()
                .map(|window| (&window.id, window.used_percent))
                .collect::<Vec<_>>()
        );
        println!("Claude reset offers: {:?}", quota.reset_offers);
    }
}
