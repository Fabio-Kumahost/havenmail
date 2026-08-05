pub mod aliases;
pub mod audit;
pub mod auth;
pub mod distribution_lists;
pub mod dns;
pub mod domains;
pub mod forwards;
pub mod mail_queue;
pub mod security_settings;
pub mod system;
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
            "/api/v1/domains",
            post(domains::create_domain).get(domains::list_domains),
        )
        .route(
            "/api/v1/domains/:domain_id",
            get(domains::get_domain)
                .patch(domains::update_domain)
                .delete(domains::delete_domain),
        )
        .route(
            "/api/v1/domains/:domain_id/users",
            post(users::create_user).get(users::list_users),
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
            "/api/v1/system/mail-queue",
            get(mail_queue::list_mail_queue).delete(mail_queue::delete_all_queue),
        )
        .route(
            "/api/v1/system/mail-queue/:queue_id",
            delete(mail_queue::delete_queue_entry),
        )
        .route("/api/v1/audit-log", get(audit::list_audit_log))
}
