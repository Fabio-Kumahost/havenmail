pub mod aliases;
pub mod api_tokens;
pub mod audit;
pub mod auth;
pub mod backup;
pub mod branding;
pub mod distribution_lists;
pub mod dns;
pub mod domains;
pub mod fail2ban;
pub mod forwards;
pub mod mail_queue;
pub mod search;
pub mod security_settings;
pub mod sessions;
pub mod system;
pub mod totp;
pub mod users;

use crate::state::AppState;
use axum::{
    routing::{delete, get, patch, post},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/refresh", post(auth::refresh))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route(
            "/api/v1/system/branding",
            get(branding::get_branding).patch(branding::update_branding),
        )
        .route(
            "/api/v1/domains",
            post(domains::create_domain).get(domains::list_domains),
        )
        .route("/api/v1/domains/overview", get(domains::domains_overview))
        .route("/api/v1/search", get(search::search))
        .route(
            "/api/v1/domains/:domain_id",
            get(domains::get_domain)
                .patch(domains::update_domain)
                .delete(domains::delete_domain),
        )
        .route(
            "/api/v1/domains/:domain_id/ratelimit-override",
            patch(domains::update_ratelimit_override),
        )
        .route(
            "/api/v1/domains/:domain_id/users",
            post(users::create_user).get(users::list_users),
        )
        .route(
            "/api/v1/domains/:domain_id/users/storage",
            get(users::get_users_storage),
        )
        .route(
            "/api/v1/domains/:domain_id/users/import",
            post(users::import_users),
        )
        .route(
            "/api/v1/domains/:domain_id/users/export",
            get(users::export_users),
        )
        .route(
            "/api/v1/users/:user_id",
            get(users::get_user)
                .patch(users::update_user)
                .delete(users::delete_user),
        )
        .route(
            "/api/v1/users/me/password",
            patch(users::change_own_password),
        )
        .route("/api/v1/users/me/totp", get(totp::get_status))
        .route("/api/v1/users/me/totp/enroll", post(totp::enroll))
        .route("/api/v1/users/me/totp/confirm", post(totp::confirm))
        .route("/api/v1/users/me/totp/disable", post(totp::disable))
        .route("/api/v1/users/me/sessions", get(sessions::list_sessions))
        .route(
            "/api/v1/users/me/sessions/:session_id",
            delete(sessions::revoke_session),
        )
        .route(
            "/api/v1/users/me/api-tokens",
            get(api_tokens::list_api_tokens).post(api_tokens::create_api_token),
        )
        .route(
            "/api/v1/users/me/api-tokens/:token_id",
            delete(api_tokens::revoke_api_token),
        )
        .route(
            "/api/v1/domains/:domain_id/aliases",
            post(aliases::create_alias).get(aliases::list_aliases),
        )
        .route("/api/v1/aliases/:alias_id", delete(aliases::delete_alias))
        .route(
            "/api/v1/domains/:domain_id/distribution-lists",
            post(distribution_lists::create_distribution_list)
                .get(distribution_lists::list_distribution_lists),
        )
        .route(
            "/api/v1/distribution-lists/:list_id",
            delete(distribution_lists::delete_distribution_list),
        )
        .route(
            "/api/v1/users/:user_id/forwards",
            post(forwards::create_forward).get(forwards::list_forwards),
        )
        .route(
            "/api/v1/forwards/:forward_id",
            delete(forwards::delete_forward),
        )
        .route(
            "/api/v1/domains/:domain_id/dkim",
            post(dns::generate_dkim_key),
        )
        .route(
            "/api/v1/domains/:domain_id/dns-recommendations",
            get(dns::dns_recommendations),
        )
        .route(
            "/api/v1/domains/:domain_id/dns-check",
            post(dns::run_dns_check),
        )
        .route("/api/v1/system/status", get(system::system_status))
        .route("/api/v1/system/metrics", get(system::system_metrics))
        .route(
            "/api/v1/system/security-settings",
            get(security_settings::get_settings),
        )
        .route(
            "/api/v1/system/spam-settings",
            patch(security_settings::update_spam_settings),
        )
        .route(
            "/api/v1/system/virus-settings",
            patch(security_settings::update_virus_settings),
        )
        .route(
            "/api/v1/system/password-policy",
            get(security_settings::get_password_policy)
                .patch(security_settings::update_password_policy),
        )
        .route(
            "/api/v1/system/mail-queue",
            get(mail_queue::list_mail_queue).delete(mail_queue::delete_all_queue),
        )
        .route(
            "/api/v1/system/mail-queue/:queue_id",
            delete(mail_queue::delete_queue_entry),
        )
        .route("/api/v1/system/fail2ban", get(fail2ban::get_status))
        .route("/api/v1/system/fail2ban/unban", post(fail2ban::unban))
        .route("/api/v1/system/backup", get(backup::get_status))
        .route("/api/v1/system/backup/trigger", post(backup::trigger))
        .route("/api/v1/audit-log", get(audit::list_audit_log))
        .route("/api/v1/audit-log/actions", get(audit::list_audit_actions))
}
