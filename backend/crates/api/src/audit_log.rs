//! Dünner Wrapper um `havenmail_core::audit::record` für die REST-API:
//! ermittelt die Client-IP aus den Request-Headern und loggt Schreibfehler
//! statt sie an den Aufrufer durchzureichen — ein Audit-Log-Ausfall soll
//! keine sonst erfolgreiche administrative Aktion scheitern lassen (siehe
//! docs/architecture.md, Abwägung Verfügbarkeit vs. lückenlose Protokollierung).

use crate::client_ip;
use crate::state::AppState;
use axum::http::HeaderMap;
use havenmail_core::rbac::Actor;
use serde_json::Value;
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub async fn log(
    state: &AppState,
    actor: &Actor,
    headers: &HeaderMap,
    action: &str,
    target: &str,
    domain_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
) {
    let ip = client_ip::extract(headers).to_string();
    if let Err(err) = havenmail_core::audit::record(
        &state.db,
        Some(actor.user_id),
        action,
        target,
        domain_id,
        before,
        after,
        Some(&ip),
    )
    .await
    {
        tracing::error!(%err, action, target, "Audit-Log-Eintrag konnte nicht geschrieben werden");
    }
}
