use axum::{routing, Router};

use super::ServerState;
use crate::auth::permissions::list_permissions;

pub fn create_routes() -> Router<ServerState> {
    Router::new().route("/permissions", routing::get(list_permissions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{start_app, BootstrappedData};

    #[sqlx::test]
    async fn list_permissions(db: sqlx::PgPool) {
        #[derive(Debug, serde::Deserialize)]
        struct Perm {
            name: String,
            description: String,
            key: String,
        }

        let (
            _app,
            BootstrappedData {
                organization,
                no_roles_user,
                ..
            },
        ) = start_app(db.clone()).await;

        let perms = no_roles_user
            .client
            .get("meta/permissions")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Vec<Perm>>()
            .await
            .unwrap();

        println!("{:#?}", perms);
        assert!(!perms.is_empty());
    }
}
