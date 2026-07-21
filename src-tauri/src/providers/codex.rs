use std::{
    collections::HashMap,
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
        ActivityGroup, ActivityItem, ActivityKind, ActivityStatus, AgentSession, ProviderId,
        ProviderQuota, ProviderSessionDetails, ProviderSnapshot, QuotaWindow, SessionMetrics,
        SessionStatus, SubagentSummary, TokenUsage, TokenUsageQuality, UiText, UsageCost,
    },
    providers::partial_snapshot,
    utils::{
        clamp_percent, event_timestamp_ms, home_dir, normalize_epoch_ms, project_name,
        reset_at_ms_from_epoch, tail_lines, truncate,
    },
};

// Load local history off the async runtime and treat app-server quota access as optional data.
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

#[derive(Clone, Debug)]
struct ThreadRow {
    id: String,
    rollout_path: String,
    created_at: i64,
    updated_at: i64,
    cwd: String,
    title: String,
    branch: String,
    model: String,
    reasoning_effort: String,
    token_count: u64,
    preview: String,
    first_user_message: String,
    parent_thread_id: Option<String>,
    agent_nickname: Option<String>,
    agent_role: Option<String>,
    agent_path: Option<String>,
    is_subagent: bool,
}

struct CodexHistory {
    activity_groups: Vec<ActivityGroup>,
    activity_truncated: bool,
    token_usage: Option<TokenUsage>,
    context_window_tokens: Option<u64>,
    last_turn_duration_ms: Option<u64>,
    collaboration_mode: Option<String>,
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
                    COALESCE(first_user_message, ''), COALESCE(reasoning_effort, ''),
                    COALESCE(source, ''), agent_nickname, agent_role, agent_path,
                    COALESCE(thread_source, '')
             FROM threads
             WHERE COALESCE(archived, 0) = 0
               AND (COALESCE(title, '') <> '' OR COALESCE(first_user_message, '') <> '')
             ORDER BY COALESCE(updated_at_ms, updated_at * 1000, 0) DESC
             LIMIT 80",
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
                reasoning_effort: row.get(11)?,
                parent_thread_id: parent_thread_id(&row.get::<_, String>(12)?),
                agent_nickname: row.get(13)?,
                agent_role: row.get(14)?,
                agent_path: row.get(15)?,
                is_subagent: row.get::<_, String>(16)? == "subagent",
            })
        })
        .map_err(|error| error.to_string())?;

    let rows = rows
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let roots = rows
        .iter()
        .filter(|row| !row.is_subagent)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();
    roots
        .into_iter()
        .map(|row| {
            let subagents = rows
                .iter()
                .filter(|candidate| candidate.parent_thread_id.as_deref() == Some(row.id.as_str()))
                .map(subagent_summary)
                .collect();
            thread_to_session(row, subagents)
        })
        .collect()
}

fn parent_thread_id(source: &str) -> Option<String> {
    serde_json::from_str::<Value>(source).ok()?["subagent"]["thread_spawn"]["parent_thread_id"]
        .as_str()
        .map(str::to_string)
}

fn status_from_updated_at(updated_at: i64) -> SessionStatus {
    let age_seconds = (crate::utils::now_ms() - normalize_epoch_ms(updated_at)).max(0) / 1_000;
    if age_seconds < 100 {
        SessionStatus::Working
    } else {
        SessionStatus::Completed
    }
}

fn subagent_summary(row: &ThreadRow) -> SubagentSummary {
    SubagentSummary {
        id: row.id.clone(),
        name: row
            .agent_nickname
            .clone()
            .or_else(|| {
                row.agent_path
                    .as_deref()
                    .and_then(|path| path.rsplit('/').next())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Sous-agent".to_string()),
        path: row.agent_path.clone(),
        role: row.agent_role.clone(),
        model: non_empty(&row.model),
        reasoning_effort: non_empty(&row.reasoning_effort),
        status: status_from_updated_at(row.updated_at),
        updated_at_ms: normalize_epoch_ms(row.updated_at),
        token_usage: TokenUsage::total_only(row.token_count),
    }
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

// Convert one database thread and its recent rollout events into the shared session model.
fn thread_to_session(
    row: ThreadRow,
    subagents: Vec<SubagentSummary>,
) -> Result<AgentSession, String> {
    let path = PathBuf::from(&row.rollout_path);
    let history = if path.exists() {
        parse_history(&path, row.updated_at)?
    } else {
        CodexHistory {
            activity_groups: Vec::new(),
            activity_truncated: false,
            token_usage: TokenUsage::total_only(row.token_count),
            context_window_tokens: None,
            last_turn_duration_ms: None,
            collaboration_mode: None,
        }
    };
    let summary_source = [&row.preview, &row.first_user_message, &row.title]
        .into_iter()
        .find(|value| !value.trim().is_empty())
        .map(|value| value.as_str());
    let age_seconds = (crate::utils::now_ms() - normalize_epoch_ms(row.updated_at)).max(0) / 1_000;
    let last_kind = history
        .activity_groups
        .last()
        .and_then(|group| group.items.last())
        .map(|item| item.kind);
    let status = if age_seconds < 100 {
        SessionStatus::Working
    } else if matches!(last_kind, Some(ActivityKind::User)) && age_seconds < 900 {
        SessionStatus::Attention
    } else if matches!(last_kind, Some(ActivityKind::Assistant)) && age_seconds < 900 {
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
        model: non_empty(&row.model),
        status,
        started_at_ms: normalize_epoch_ms(row.created_at),
        updated_at_ms: normalize_epoch_ms(row.updated_at),
        token_usage: history
            .token_usage
            .or_else(|| TokenUsage::total_only(row.token_count)),
        metrics: SessionMetrics {
            turn_count: Some(history.activity_groups.len() as u32),
            tool_call_count: Some(tool_call_count(&history.activity_groups)),
            context_window_tokens: history.context_window_tokens,
            last_turn_duration_ms: history.last_turn_duration_ms,
            ..SessionMetrics::default()
        },
        provider_details: ProviderSessionDetails::Codex {
            reasoning_effort: non_empty(&row.reasoning_effort),
            collaboration_mode: history.collaboration_mode,
            subagents,
        },
        summary: summary_source
            .map(|value| UiText::from(truncate(value, 180)))
            .unwrap_or_else(|| UiText::message("generated.codexConversation")),
        activity_groups: history.activity_groups,
        activity_truncated: history.activity_truncated,
    })
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

fn tool_call_count(groups: &[ActivityGroup]) -> u32 {
    groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|item| item.kind == ActivityKind::Tool)
        .map(|item| item.count.unwrap_or(1))
        .sum()
}

fn timestamp_ms(value: &Value, fallback_ms: i64) -> i64 {
    event_timestamp_ms(value["timestamp"].as_str(), fallback_ms)
}

fn activity_item(
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

fn ensure_group(groups: &mut Vec<ActivityGroup>, fallback_ms: i64, index: usize) -> usize {
    if groups.last().is_none_or(|group| {
        matches!(
            group.status,
            Some(ActivityStatus::Completed | ActivityStatus::Error | ActivityStatus::Cancelled)
        )
    }) {
        groups.push(ActivityGroup {
            id: format!("codex-turn-{index}"),
            started_at_ms: fallback_ms,
            completed_at_ms: None,
            status: None,
            items: Vec::new(),
        });
    }
    groups.len() - 1
}

fn parse_token_usage(info: &Value) -> Option<TokenUsage> {
    let usage = &info["total_token_usage"];
    let total = usage["total_tokens"].as_u64()?;
    (total > 0).then_some(TokenUsage {
        quality: TokenUsageQuality::Detailed,
        total,
        input: usage["input_tokens"].as_u64(),
        output: usage["output_tokens"].as_u64(),
        cached_input: usage["cached_input_tokens"].as_u64(),
        cache_write_input: usage["cache_write_input_tokens"].as_u64(),
        reasoning_output: usage["reasoning_output_tokens"].as_u64(),
    })
}

// Normalize the recent rollout stream into provider-agnostic activity groups.
fn parse_history(path: &Path, fallback_ms: i64) -> Result<CodexHistory, String> {
    let lines = tail_lines(path, 512 * 1024)
        .map_err(|error| format!("Historique Codex illisible ({}): {error}", path.display()))?;
    let mut groups = Vec::new();
    let mut pending_tools: HashMap<String, String> = HashMap::new();
    let mut token_usage = None;
    let mut context_window_tokens = None;
    let mut last_turn_duration_ms = None;
    let mut collaboration_mode = None;
    for (index, line) in lines.iter().enumerate() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = &value["payload"];
        let payload_type = payload["type"].as_str().unwrap_or_default();
        let event_ms = timestamp_ms(&value, fallback_ms);
        match payload_type {
            "task_started" => {
                groups.push(ActivityGroup {
                    id: payload["turn_id"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("codex-turn-{index}")),
                    started_at_ms: event_ms,
                    completed_at_ms: None,
                    status: Some(ActivityStatus::Running),
                    items: Vec::new(),
                });
            }
            "user_message" if value["type"] == "event_msg" => {
                if let Some(body) = payload["message"].as_str() {
                    let group = ensure_group(&mut groups, event_ms, index);
                    groups[group].status = None;
                    groups[group].items.push(activity_item(
                        format!("codex-user-{index}"),
                        ActivityKind::User,
                        UiText::message("generated.you"),
                        UiText::from(truncate(body, 360)),
                        event_ms,
                    ));
                }
            }
            "agent_message" if value["type"] == "event_msg" => {
                if let Some(body) = payload["message"].as_str() {
                    let group = ensure_group(&mut groups, event_ms, index);
                    groups[group].status = None;
                    groups[group].items.push(activity_item(
                        format!("codex-assistant-{index}"),
                        ActivityKind::Assistant,
                        UiText::from("Codex"),
                        UiText::from(truncate(body, 360)),
                        event_ms,
                    ));
                }
            }
            "custom_tool_call" | "function_call" => {
                let name = payload["name"].as_str().unwrap_or("Outil");
                let namespace = payload["namespace"].as_str();
                let label =
                    namespace.map_or_else(|| name.to_string(), |ns| format!("{ns} · {name}"));
                let call_id = payload["call_id"].as_str().unwrap_or(name).to_string();
                let group = ensure_group(&mut groups, event_ms, index);
                groups[group].status = None;
                if let Some(existing) = groups[group].items.iter_mut().find(|item| {
                    item.kind == ActivityKind::Tool && item.title.as_raw() == Some(label.as_str())
                }) {
                    existing.count = Some(existing.count.unwrap_or(1) + 1);
                    pending_tools.insert(call_id, existing.id.clone());
                } else {
                    let item_id = format!("codex-tool-{index}");
                    let mut item = activity_item(
                        item_id.clone(),
                        ActivityKind::Tool,
                        UiText::from(label),
                        UiText::message("generated.localToolExecution"),
                        event_ms,
                    );
                    item.status = Some(ActivityStatus::Running);
                    groups[group].items.push(item);
                    pending_tools.insert(call_id, item_id);
                }
            }
            "custom_tool_call_output" | "function_call_output" => {
                if let Some(call_id) = payload["call_id"].as_str() {
                    if let Some(item_id) = pending_tools.remove(call_id) {
                        if let Some(item) = groups
                            .iter_mut()
                            .flat_map(|group| &mut group.items)
                            .find(|item| item.id == item_id)
                        {
                            item.status = Some(ActivityStatus::Completed);
                        }
                    }
                }
            }
            "sub_agent_activity" => {
                let group = ensure_group(&mut groups, event_ms, index);
                groups[group].status = None;
                groups[group].items.push(activity_item(
                    format!("codex-subagent-{index}"),
                    ActivityKind::Subagent,
                    UiText::from(
                        payload["agent_path"]
                            .as_str()
                            .and_then(|path| path.rsplit('/').next())
                            .unwrap_or("Sous-agent"),
                    ),
                    UiText::message("generated.subagentActivity"),
                    event_ms,
                ));
            }
            "task_complete" | "turn_complete" => {
                let group = ensure_group(&mut groups, event_ms, index);
                groups[group].completed_at_ms = Some(event_ms);
                groups[group].status = Some(ActivityStatus::Completed);
                let duration = payload["duration_ms"].as_u64();
                last_turn_duration_ms = duration.or(last_turn_duration_ms);
                if let Some(duration) = duration {
                    if let Some(item) = groups[group].items.last_mut() {
                        item.duration_ms = Some(duration);
                    }
                }
            }
            "turn_aborted" => {
                let group = ensure_group(&mut groups, event_ms, index);
                groups[group].items.push(activity_item(
                    format!("codex-system-{index}"),
                    ActivityKind::System,
                    UiText::message("generated.taskCancelled"),
                    UiText::message("generated.taskCancelledBody"),
                    event_ms,
                ));
                groups[group].completed_at_ms = Some(event_ms);
                groups[group].status = Some(ActivityStatus::Cancelled);
            }
            "token_count" => {
                token_usage = parse_token_usage(&payload["info"]).or(token_usage);
                context_window_tokens = payload["info"]["model_context_window"]
                    .as_u64()
                    .or(context_window_tokens);
            }
            _ => {}
        }
        if value["type"] == "turn_context" {
            collaboration_mode = payload["collaboration_mode"]["mode"]
                .as_str()
                .map(str::to_string)
                .or(collaboration_mode);
        }
    }
    groups.retain(|group| !group.items.is_empty());
    let mut activity_truncated = path
        .metadata()
        .is_ok_and(|metadata| metadata.len() > 512 * 1024);
    if groups.len() > 20 {
        groups.drain(..groups.len() - 20);
        activity_truncated = true;
    }
    Ok(CodexHistory {
        activity_groups: groups,
        activity_truncated,
        token_usage,
        context_window_tokens,
        last_turn_duration_ms,
        collaboration_mode,
    })
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

// Locate the bundled desktop binary first while keeping CODEX_PATH available for development.
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

// Preserve stable window IDs while supporting both current and legacy rate-limit payloads.
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
    fn normalizes_rollout_activity_and_detailed_token_usage() {
        let home = TemporaryHome::new("rollout-history");
        let path = home.path().join("rollout.jsonl");
        let rows = [
            json!({"type":"event_msg","timestamp":"2030-01-01T10:00:00Z","payload":{"type":"user_message","message":"Inspect the project"}}),
            json!({"type":"response_item","timestamp":"2030-01-01T10:00:01Z","payload":{"type":"function_call","name":"read_file","call_id":"call-1"}}),
            json!({"type":"response_item","timestamp":"2030-01-01T10:00:02Z","payload":{"type":"function_call_output","call_id":"call-1"}}),
            json!({"type":"event_msg","timestamp":"2030-01-01T10:00:03Z","payload":{"type":"agent_message","message":"Inspection complete"}}),
            json!({"type":"event_msg","timestamp":"2030-01-01T10:00:04Z","payload":{"type":"token_count","info":{"model_context_window":200000,"total_token_usage":{"total_tokens":120,"input_tokens":80,"output_tokens":40,"cached_input_tokens":20}}}}),
            json!({"type":"turn_context","payload":{"collaboration_mode":{"mode":"default"}}}),
            json!({"type":"event_msg","timestamp":"2030-01-01T10:00:05Z","payload":{"type":"turn_complete","duration_ms":5000}}),
        ];
        fs::write(
            &path,
            rows.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .expect("write rollout fixture");

        let history = parse_history(&path, 0).expect("normalized rollout");
        let group = &history.activity_groups[0];
        let tool = group
            .items
            .iter()
            .find(|item| item.kind == ActivityKind::Tool)
            .expect("tool activity");

        assert_eq!(history.activity_groups.len(), 1);
        assert_eq!(tool.status, Some(ActivityStatus::Completed));
        assert_eq!(history.token_usage.expect("token usage").total, 120);
        assert_eq!(history.context_window_tokens, Some(200_000));
        assert_eq!(history.last_turn_duration_ms, Some(5_000));
        assert_eq!(history.collaboration_mode.as_deref(), Some("default"));
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
