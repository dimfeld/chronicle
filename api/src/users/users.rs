use async_trait::async_trait;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing};
use axum_jsonschema::Json;
use error_stack::{Report, ResultExt};
use filigree::{
    auth::password::{new_hash, HashedPassword},
    extract::FormOrJson,
    users::{
        organization::add_user_to_organization,
        roles::add_default_role_to_user,
        users::{add_user_email_login, CreateUserDetails, UserCreatorError},
    },
};
use schemars::JsonSchema;
use serde::Serialize;
use sqlx::{PgConnection, PgExecutor};

use crate::{
    auth::Authed,
    models::{
        organization::OrganizationId,
        user::{User, UserCreatePayload, UserId},
    },
    server::ServerState,
    Error,
};

/// Create a new user, with the given password. This only creates the user object, and
/// does not add any entries to other related tables such as email_logins or organization_members.
pub async fn create_new_user_with_plaintext_password(
    db: impl PgExecutor<'_>,
    user_id: UserId,
    organization_id: OrganizationId,
    payload: UserCreatePayload,
    password_plaintext: String,
) -> Result<User, Report<Error>> {
    let password_hash = if password_plaintext.is_empty() {
        let hash = filigree::auth::password::new_hash(password_plaintext)
            .await
            .change_context(Error::AuthSubsystem)?;
        Some(hash)
    } else {
        None
    };

    create_new_user_with_prehashed_password(db, user_id, organization_id, payload, password_hash)
        .await
}

/// Create a new user, optionally with the given password. This only creates the user object, and
/// does not add any entries to other related tables such as email_logins or organization_members.
pub async fn create_new_user_with_prehashed_password(
    db: impl PgExecutor<'_>,
    user_id: UserId,
    organization_id: OrganizationId,
    payload: UserCreatePayload,
    password_hash: Option<HashedPassword>,
) -> Result<User, Report<Error>> {
    let user = sqlx::query_file_as!(
        User,
        "src/users/create_user.sql",
        user_id.as_uuid(),
        organization_id.as_uuid(),
        password_hash.map(|h| h.0),
        &payload.name,
        payload.email.as_ref(),
        payload.avatar_url.as_ref(),
    )
    .fetch_one(db)
    .await
    .change_context(Error::Db)?;

    Ok(user)
}

pub struct UserCreator;

impl UserCreator {
    pub async fn create_user(
        tx: &mut PgConnection,
        add_to_organization: Option<OrganizationId>,
        details: CreateUserDetails,
    ) -> Result<(UserId, OrganizationId), Report<UserCreatorError>> {
        let user_id = UserId::new();
        let organization_fut = async {
            match add_to_organization {
                Some(organization_id) => {
                    sqlx::query!("SET CONSTRAINTS ALL DEFERRED")
                        .execute(&mut *tx)
                        .await
                        .change_context(UserCreatorError)?;
                    add_user_to_organization(&mut *tx, organization_id, user_id)
                        .await
                        .change_context(UserCreatorError)?;
                    add_default_role_to_user(&mut *tx, organization_id, user_id)
                        .await
                        .change_context(UserCreatorError)?;

                    Ok(organization_id)
                }
                None => {
                    let org_name = details
                        .name
                        .as_deref()
                        .unwrap_or("My Organization")
                        .to_string();

                    // create_new_organization does everything except actually creating the user
                    // object.
                    let org =
                        super::organization::create_new_organization(&mut *tx, org_name, user_id)
                            .await
                            .change_context(UserCreatorError)?;

                    Ok(org.organization.id)
                }
            }
        };

        let password_fut = async {
            match details.password_plaintext {
                Some(password) => new_hash(password)
                    .await
                    .map(Some)
                    .change_context(UserCreatorError),
                None => Ok(None),
            }
        };

        let (organization_id, password_hash) = tokio::try_join!(organization_fut, password_fut)?;

        let create_payload = UserCreatePayload {
            name: details.name.clone().unwrap_or_default(),
            email: details.email.clone(),
            avatar_url: details.avatar_url.map(|u| u.to_string()),
            ..Default::default()
        };

        create_new_user_with_prehashed_password(
            &mut *tx,
            user_id,
            organization_id,
            create_payload,
            password_hash,
        )
        .await
        .change_context(UserCreatorError)?;

        if let Some(email) = details.email {
            add_user_email_login(&mut *tx, user_id, email, true)
                .await
                .change_context(UserCreatorError)?;
        }

        Ok((user_id, organization_id))
    }
}

#[async_trait]
impl filigree::users::users::UserCreator for UserCreator {
    async fn create_user(
        &self,
        tx: &mut PgConnection,
        add_to_organization: Option<OrganizationId>,
        details: CreateUserDetails,
    ) -> Result<UserId, Report<UserCreatorError>> {
        Self::create_user(tx, add_to_organization, details)
            .await
            .map(|(user_id, _)| user_id)
    }
}

/// The current user and other information to return to the client.
#[derive(Serialize, Debug, JsonSchema)]
pub struct SelfUser {
    user: crate::models::user::User,
    roles: Vec<crate::models::role::RoleId>,
    permissions: Vec<String>,
}

async fn get_current_user_endpoint(
    State(state): State<ServerState>,
    authed: Authed,
) -> Result<impl IntoResponse, Error> {
    // TODO This should be a more custom query, include organization info and permissions
    // and such, and work even if the user doesn't have the User:read permission.
    let user = crate::models::user::queries::get(&state.db, &authed, authed.user_id).await?;

    let user = SelfUser {
        user,
        roles: authed.roles.clone(),
        permissions: authed.permissions.clone(),
    };

    Ok(Json(user))
}

async fn update_current_user_endpoint(
    State(state): State<ServerState>,
    authed: Authed,
    FormOrJson(body): FormOrJson<crate::models::user::UserUpdatePayload>,
) -> Result<impl IntoResponse, Error> {
    // TODO Need a query specifically for updating self
    let mut tx = state.db.begin().await.change_context(Error::Db)?;
    let updated =
        crate::models::user::queries::update(&mut *tx, &authed, authed.user_id, body).await?;
    tx.commit().await.change_context(Error::Db)?;

    let status = if updated {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    Ok(status)
}

pub fn create_routes() -> axum::Router<ServerState> {
    axum::Router::new()
        .route("/self", routing::get(get_current_user_endpoint))
        .route("/self", routing::put(update_current_user_endpoint))
}

#[cfg(test)]
mod test {
    use crate::tests::{start_app, BootstrappedData};

    #[sqlx::test]
    async fn get_current_user(db: sqlx::PgPool) {
        let (app, BootstrappedData { admin_user, .. }) = start_app(db).await;

        let user_info: serde_json::Value = admin_user
            .client
            .get("self")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(user_info["user"]["name"], "Admin");
    }

    #[sqlx::test]
    async fn update_current_user(db: sqlx::PgPool) {
        let (app, BootstrappedData { admin_user, .. }) = start_app(db).await;

        let payload = crate::models::user::UserUpdatePayload {
            name: "Not Admin".into(),
            email: Some("another-email@example.com".into()),
            ..Default::default()
        };

        admin_user
            .client
            .put("self")
            .json(&payload)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        let user_info: serde_json::Value = admin_user
            .client
            .get("self")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(user_info["user"]["name"], "Not Admin");
        assert_eq!(user_info["user"]["email"], "another-email@example.com");
    }
}
