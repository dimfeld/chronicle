pub mod organization;
pub mod role;
pub mod user;

use axum::Router;

use crate::server::ServerState;

pub fn create_routes() -> Router<ServerState> {
    Router::new()
        .merge(role::endpoints::create_routes())
        .merge(user::endpoints::create_routes())
}
