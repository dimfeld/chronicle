use serde_json::json;

use crate::tests::{start_app, start_app_with_options, BootstrappedData, TestAppOptions};

pub fn extract_token_from_email(email: &filigree::email::Email) -> &str {
    email
        .text
        .split_once("token=")
        .unwrap()
        .1
        .split_once('&')
        .unwrap()
        .0
}

#[sqlx::test]
#[cfg_attr(not(feature = "test_password"), ignore = "slow password test")]
async fn login_with_password_and_logout(db: sqlx::PgPool) {
    let (app, BootstrappedData { admin_user, .. }) = start_app(db.clone()).await;

    let client = &app.client;
    let response: serde_json::Value = client
        .post("auth/login")
        .json(&json!({ "email": admin_user.email, "password": admin_user.password }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(response["message"], "Logged in");

    let expires = sqlx::query_scalar!(
        "UPDATE user_sessions
        SET expires_at = now() + '1 minute'::interval
        WHERE user_id = $1
        RETURNING expires_at",
        admin_user.user_id.as_uuid()
    )
    .fetch_one(&db)
    .await
    .unwrap();

    let user: serde_json::Value = client
        .get(&format!("users/{}", admin_user.user_id))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(user["name"], "Admin");

    let new_expires = sqlx::query_scalar!(
        "SELECT expires_at
        FROM user_sessions
        WHERE user_id = $1",
        admin_user.user_id.as_uuid()
    )
    .fetch_one(&db)
    .await
    .unwrap();

    assert!(
        new_expires > expires,
        "session expiration should have been updated"
    );

    let response: serde_json::Value = client
        .post("auth/logout")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["message"], "Logged out");

    let anon_response = client
        .get(&format!("users/{}", admin_user.user_id))
        .send()
        .await
        .unwrap();

    assert_eq!(
        anon_response.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "Authed requests should not work after logout"
    );

    // TODO check explicitly that the session cookie is gone
    // TODO check that adding the session cookie back to the request after logout doesn't work
}

#[sqlx::test]
#[cfg_attr(not(feature = "test_password"), ignore = "slow password test")]
async fn login_with_nonexistent_email(db: sqlx::PgPool) {
    let (app, BootstrappedData { admin_user, .. }) = start_app_with_options(
        db,
        TestAppOptions {
            obfuscate_errors: Some(true),
        },
    )
    .await;

    let client = &app.client;
    let response = client
        .post("auth/login")
        .json(&json!({ "email": "nobody@example.com", "password": admin_user.password }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    let data: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        data,
        json!({
            "error": {
                "message": "Unauthenticated",
                "kind": "unauthenticated",
                "details": null,
            },
            "form": { "email": "nobody@example.com" },
        })
    );
}

#[sqlx::test]
#[cfg_attr(not(feature = "test_password"), ignore = "slow password test")]
async fn login_with_wrong_password(db: sqlx::PgPool) {
    let (app, BootstrappedData { admin_user, .. }) = start_app_with_options(
        db,
        TestAppOptions {
            obfuscate_errors: Some(true),
        },
    )
    .await;

    let client = &app.client;
    let response = client
        .post("auth/login")
        .json(&json!({ "email": admin_user.email, "password": "wrong" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    let data: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        data,
        json!({
            "error": {
                "message": "Unauthenticated",
                "kind": "unauthenticated",
                "details": null,
            },
            "form": { "email": admin_user.email },
        })
    );
}

#[sqlx::test]
#[cfg_attr(not(feature = "test_password"), ignore = "slow password test")]
async fn login_with_no_roles_user(db: sqlx::PgPool) {
    let (app, BootstrappedData { no_roles_user, .. }) = start_app(db).await;

    let client = &app.client;
    let response: serde_json::Value = client
        .post("auth/login")
        .json(&json!({ "email": no_roles_user.email, "password": no_roles_user.password }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(response["message"], "Logged in");

    let response = client
        .get(&format!("users/{}", no_roles_user.user_id))
        .send()
        .await
        .unwrap();
    // Should see 403 because user has no roles and hence no permissions, but not
    // 401 which would indicate some other problem in the auth system.
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
}
