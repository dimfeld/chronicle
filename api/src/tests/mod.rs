use std::sync::{Arc, Mutex};

use error_stack::Report;
use filigree::{
    auth::{api_key::ApiKeyData, password::HashedPassword, ExpiryStyle, SessionCookieBuilder},
    testing::{self, TestClient},
};
use futures::future::FutureExt;
use sqlx::{PgConnection, PgPool};
use tracing::{event, instrument, Level};

use crate::{
    models::{
        organization::{Organization, OrganizationId},
        role::RoleId,
        user::{self, UserId},
    },
    users::organization::CreatedOrganization,
    Error,
};

pub struct TestApp {
    /// Hold on to the shutdown signal so the server stays alive
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
    pub client: TestClient,
    pub base_url: String,
    pub pg_pool: PgPool,
    pub server_task: tokio::task::JoinHandle<Result<(), Report<Error>>>,
    pub sent_emails: Arc<Mutex<Vec<filigree::email::Email>>>,
}

#[derive(Clone, Debug)]
pub struct TestUser {
    pub user_id: UserId,
    pub organization_id: OrganizationId,
    pub email: String,
    pub password: String,
    pub api_key: String,
    pub client: TestClient,
}

pub struct BootstrappedData {
    pub organization: Organization,
    pub admin_role: RoleId,
    pub user_role: RoleId,
    pub admin_user: TestUser,
    pub user: TestUser,
    pub no_roles_user: TestUser,
}

pub struct TestAppOptions {
    pub obfuscate_errors: Option<bool>,
}

impl Default for TestAppOptions {
    fn default() -> Self {
        Self {
            obfuscate_errors: Some(false),
        }
    }
}

pub async fn start_app(pg_pool: PgPool) -> (TestApp, BootstrappedData) {
    start_app_with_options(pg_pool, TestAppOptions::default()).await
}

pub async fn start_app_with_options(
    pg_pool: PgPool,
    options: TestAppOptions,
) -> (TestApp, BootstrappedData) {
    error_stack::Report::set_color_mode(error_stack::fmt::ColorMode::None);
    filigree::tracing_config::test::init();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    // Make the shutdown future resolve to () so the type matches what Axum expects.
    let shutdown_rx = shutdown_rx.map(|_| ());

    let email_service = filigree::email::services::test_service::TestEmailService::new();
    let sent_emails = email_service.emails.clone();

    let listener = crate::server::create_tcp_listener("127.0.0.1", 0)
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://127.0.0.1:{port}");

    let config = crate::server::Config {
        env: "test".into(),
        bind: crate::server::ServerBind::Listener(listener),
        serve_frontend: crate::server::ServeFrontend {
            port: None,
            path: None,
            vite_manifest: None,
            watch_vite_manifest: false,
            livereload: false,
        },
        insecure: true,
        request_timeout: std::time::Duration::from_secs(30),
        pg_pool: pg_pool.clone(),
        api_cors: filigree::auth::CorsSetting::default(),
        hosts: vec![],
        cookie_configuration: SessionCookieBuilder::new(
            false,
            tower_cookies::cookie::SameSite::Strict,
        ),
        obfuscate_errors: options.obfuscate_errors,
        secrets: crate::server::Secrets::empty(),
        session_expiry: ExpiryStyle::AfterIdle(std::time::Duration::from_secs(24 * 60 * 60)),
        oauth_redirect_url_base: base_url.clone(),
        oauth_providers: Some(vec![]),
        new_user_flags: filigree::server::NewUserFlags {
            allow_public_signup: true,
            allow_invite_to_same_org: true,
            allow_invite_to_new_org: true,
            same_org_invites_require_email_verification: true,
        },
        email_sender: filigree::email::services::EmailSender::new(
            "support@example.com".to_string(),
            crate::emails::create_tera(),
            Box::new(email_service),
        ),
    };

    let server = crate::server::create_server(config)
        .await
        .expect("creating server");

    let test_client = TestClient::new(format!("{base_url}/api"));

    let bootstrapped_data = bootstrap_data(&pg_pool, &test_client).await;

    let server_task = tokio::task::spawn(server.run_with_shutdown_signal(shutdown_rx));

    event!(Level::INFO, "finished bootstrapping test");

    let app = TestApp {
        shutdown_tx,
        client: test_client,
        base_url,
        server_task,
        sent_emails,
        pg_pool,
    };

    (app, bootstrapped_data)
}

#[instrument(skip(db, base_client))]
async fn add_test_user(
    db: &mut PgConnection,
    base_client: &TestClient,
    user_id: UserId,
    organization_id: OrganizationId,
    name: &str,
) -> TestUser {
    let key_data = ApiKeyData::new();

    let test_client = base_client.with_api_key(&key_data.key);

    let email = format!("{name}@example.com");
    let user_payload = user::UserCreatePayload {
        email: Some(email.clone()),
        name: name.to_string(),
        ..Default::default()
    };

    crate::users::users::create_new_user_with_prehashed_password(
        &mut *db,
        user_id,
        organization_id,
        user_payload,
        Some(HashedPassword(testing::TEST_PASSWORD_HASH.to_string())),
    )
    .await
    .expect("Creating user");

    crate::users::organization::add_user_to_organization(&mut *db, organization_id, user_id)
        .await
        .expect("Adding user to organization");

    let key = filigree::auth::api_key::ApiKey {
        api_key_id: key_data.api_key_id,
        organization_id,
        user_id: Some(user_id),
        inherits_user_permissions: true,
        description: String::new(),
        active: true,
        expires_at: chrono::Utc::now() + chrono::Duration::days(365),
    };
    filigree::auth::api_key::add_api_key(&mut *db, &key, &key_data.hash)
        .await
        .expect("Adding api key");

    filigree::users::users::add_user_email_login(&mut *db, user_id, email.clone(), true)
        .await
        .expect("Adding email login");

    TestUser {
        user_id,
        organization_id,
        email,
        password: testing::TEST_PASSWORD.to_string(),
        client: test_client,
        api_key: key_data.key,
    }
}

async fn bootstrap_data(pg_pool: &sqlx::PgPool, base_client: &TestClient) -> BootstrappedData {
    let mut tx = pg_pool.begin().await.unwrap();
    let admin_user_id = testing::ADMIN_USER_ID;
    let CreatedOrganization {
        organization,
        write_role,
        admin_role,
        ..
    } = crate::users::organization::create_new_organization(
        &mut *tx,
        "Test Org".into(),
        admin_user_id,
    )
    .await
    .expect("Creating test org");

    let admin_user = add_test_user(
        &mut *tx,
        base_client,
        admin_user_id,
        organization.id,
        "Admin",
    )
    .await;
    let regular_user = add_test_user(
        &mut *tx,
        base_client,
        UserId::new(),
        organization.id,
        "User",
    )
    .await;
    filigree::users::roles::add_roles_to_user(
        &mut *tx,
        organization.id,
        regular_user.user_id,
        &[write_role],
    )
    .await
    .expect("Adding user role to regular user");

    let no_roles_user = add_test_user(
        &mut *tx,
        base_client,
        UserId::new(),
        organization.id,
        "No Roles User",
    )
    .await;

    tx.commit().await.unwrap();

    BootstrappedData {
        organization,
        user_role: write_role,
        admin_role,
        admin_user,
        user: regular_user,
        no_roles_user,
    }
}
