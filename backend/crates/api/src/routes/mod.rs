pub mod aliases;
pub mod auth;
pub mod distribution_lists;
pub mod domains;
pub mod forwards;
pub mod users;

use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
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
}
