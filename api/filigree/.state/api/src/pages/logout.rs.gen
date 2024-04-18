use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing,
};
use maud::html;
use tower_cookies::Cookies;

use crate::{pages::layout::root_layout_page, server::ServerState};

async fn logout_page(State(state): State<ServerState>, cookies: Cookies) -> Response {
    state.session_backend.delete_session(&cookies).await.ok();
    let body = root_layout_page(None, "Logout", html! { p { "You have logged out" } });
    body.into_response()
}

pub fn create_routes() -> axum::Router<ServerState> {
    axum::Router::new().route("/logout", routing::get(logout_page))
}
