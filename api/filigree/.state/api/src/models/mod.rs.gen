pub mod alias;
pub mod alias_model;
pub mod custom_provider;
pub mod organization;
pub mod provider_api_key;
pub mod role;
pub mod user;

use axum::Router;

use crate::server::ServerState;

pub fn create_routes() -> Router<ServerState> {
    Router::new()
        .merge(alias::endpoints::create_routes())
        .merge(custom_provider::endpoints::create_routes())
        .merge(provider_api_key::endpoints::create_routes())
        .merge(role::endpoints::create_routes())
        .merge(user::endpoints::create_routes())
}
