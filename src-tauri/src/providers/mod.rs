mod claude;
mod codex;
mod cursor;

use serde_json::{Map, Value};

use crate::{
    domain::{
        AgentSession, DashboardSnapshot, ProviderDefinition, ProviderId, ProviderQuota,
        ProviderSnapshot, UiText,
    },
    utils::now_ms,
};

pub async fn dashboard_snapshot(include_quotas: bool) -> DashboardSnapshot {
    let (codex, claude, cursor) = tokio::join!(
        codex::load(include_quotas),
        claude::load(include_quotas),
        cursor::load(include_quotas)
    );

    let mut providers = Vec::with_capacity(3);
    let mut sessions = Vec::new();
    let mut quotas = Vec::new();

    merge_provider(
        &mut providers,
        &mut sessions,
        &mut quotas,
        ProviderId::Codex,
        "Codex",
        "CX",
        "#111111",
        codex,
    );
    merge_provider(
        &mut providers,
        &mut sessions,
        &mut quotas,
        ProviderId::Claude,
        "Claude",
        "CL",
        "#d97757",
        claude,
    );
    merge_provider(
        &mut providers,
        &mut sessions,
        &mut quotas,
        ProviderId::Cursor,
        "Cursor",
        "CU",
        "#6d5dfc",
        cursor,
    );

    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at_ms));

    DashboardSnapshot {
        providers,
        sessions,
        quotas,
        refreshed_at_ms: now_ms(),
    }
}

// Keep whichever part of a provider loaded successfully and surface the rest as a warning.
pub(crate) fn partial_snapshot(
    sessions: Result<Vec<AgentSession>, String>,
    quota: Option<Result<ProviderQuota, String>>,
) -> Result<ProviderSnapshot, String> {
    let mut warnings = Vec::new();
    let sessions_loaded = sessions.is_ok();
    let sessions = match sessions {
        Ok(sessions) => sessions,
        Err(error) => {
            warnings.push(error);
            Vec::new()
        }
    };
    let quota_requested = quota.is_some();
    let quota = match quota {
        Some(Ok(quota)) => Some(quota),
        Some(Err(error)) => {
            warnings.push(error);
            None
        }
        None => None,
    };

    let has_requested_data = sessions_loaded || (quota_requested && quota.is_some());
    if !has_requested_data {
        return Err(warnings.join(" · "));
    }

    Ok(ProviderSnapshot {
        sessions,
        quota,
        warning: (!warnings.is_empty()).then(|| warnings.join(" · ")),
    })
}

fn merge_provider(
    providers: &mut Vec<ProviderDefinition>,
    sessions: &mut Vec<crate::domain::AgentSession>,
    quotas: &mut Vec<crate::domain::ProviderQuota>,
    id: ProviderId,
    name: &'static str,
    short_name: &'static str,
    accent: &'static str,
    result: Result<ProviderSnapshot, String>,
) {
    match result {
        Ok(mut snapshot) => {
            let error = snapshot.warning.map(|warning| {
                eprintln!("Agent Deck: partial {name} data: {warning}");
                provider_error("errors.providerPartial", name)
            });
            providers.push(ProviderDefinition {
                id,
                name,
                short_name,
                accent,
                connected: true,
                error,
            });
            sessions.append(&mut snapshot.sessions);
            if let Some(quota) = snapshot.quota {
                quotas.push(quota);
            }
        }
        Err(error) => {
            eprintln!("Agent Deck: {name} unavailable: {error}");
            providers.push(ProviderDefinition {
                id,
                name,
                short_name,
                accent,
                connected: false,
                error: Some(provider_error("errors.providerUnavailable", name)),
            });
        }
    }
}

fn provider_error(key: &str, provider: &str) -> UiText {
    let mut params = Map::new();
    params.insert("provider".to_string(), Value::String(provider.to_string()));
    UiText::message_with(key, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota() -> ProviderQuota {
        ProviderQuota {
            provider: ProviderId::Codex,
            plan: "Plus".to_string(),
            account: UiText::from("codex@example.com"),
            windows: Vec::new(),
            cost: None,
        }
    }

    #[test]
    fn keeps_quota_when_session_loading_fails() {
        let snapshot = partial_snapshot(Err("sessions unavailable".to_string()), Some(Ok(quota())))
            .expect("partial provider snapshot");

        assert!(snapshot.sessions.is_empty());
        assert!(snapshot.quota.is_some());
        assert!(snapshot.warning.is_some());
    }

    #[test]
    fn rejects_provider_when_every_requested_source_fails() {
        let error = partial_snapshot(
            Err("sessions unavailable".to_string()),
            Some(Err("quota unavailable".to_string())),
        )
        .expect_err("provider should be unavailable");

        assert!(error.contains("sessions unavailable"));
        assert!(error.contains("quota unavailable"));
    }

    #[test]
    fn accepts_an_empty_successful_session_result_without_quotas() {
        let snapshot = partial_snapshot(Ok(Vec::new()), None).expect("connected empty provider");

        assert!(snapshot.sessions.is_empty());
        assert!(snapshot.quota.is_none());
        assert!(snapshot.warning.is_none());
    }
}
