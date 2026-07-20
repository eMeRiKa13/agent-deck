mod claude;
mod codex;
mod cursor;

use serde_json::{Map, Value};

use crate::{
    domain::{DashboardSnapshot, ProviderDefinition, ProviderId, ProviderSnapshot, UiText},
    utils::now_ms,
};

pub async fn dashboard_snapshot() -> DashboardSnapshot {
    let (codex, claude, cursor) = tokio::join!(codex::load(), claude::load(), cursor::load());

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
            let error = snapshot.warning.map(|_| provider_error("errors.providerPartial", name));
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
        Err(_error) => providers.push(ProviderDefinition {
            id,
            name,
            short_name,
            accent,
            connected: false,
            error: Some(provider_error("errors.providerUnavailable", name)),
        }),
    }
}

fn provider_error(key: &str, provider: &str) -> UiText {
    let mut params = Map::new();
    params.insert("provider".to_string(), Value::String(provider.to_string()));
    UiText::message_with(key, params)
}
