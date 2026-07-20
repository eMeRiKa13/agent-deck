mod claude;
mod codex;
mod cursor;

use crate::domain::{DashboardSnapshot, ProviderDefinition, ProviderId, ProviderSnapshot};

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
        refreshed_at: "à l’instant".to_string(),
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
            providers.push(ProviderDefinition {
                id,
                name,
                short_name,
                accent,
                connected: true,
                error: snapshot.warning,
            });
            sessions.append(&mut snapshot.sessions);
            if let Some(quota) = snapshot.quota {
                quotas.push(quota);
            }
        }
        Err(error) => providers.push(ProviderDefinition {
            id,
            name,
            short_name,
            accent,
            connected: false,
            error: Some(error),
        }),
    }
}
