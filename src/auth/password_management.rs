use axum::{
    extract::{Host, State},
    response::IntoResponse,
};
use error_stack::{Report, ResultExt};
use filigree::{auth::password::create_reset_token, extract::FormOrJson, EmailBody};

use crate::{server::ServerState, Error};

pub async fn start_password_reset(
    State(state): State<ServerState>,
    Host(host): Host,
    FormOrJson(body): FormOrJson<EmailBody>,
) -> Result<impl IntoResponse, Error> {
    if state.host_is_allowed(&host).is_err() {
        // Bail due to some kind of hijinks
        return Err(Error::InvalidHostHeader);
    }

    let token = create_reset_token(&state.db, &body.email).await;

    let token = match token {
        Ok(token) => token,
        Err(e) => {
            if e.is_unauthenticated() {
                // Don't do anything if the email was not found, but also don't tell the user that
                // the email doesn't exist.
                return Ok(());
            } else {
                return Err(Report::new(e).change_context(Error::AuthSubsystem).into());
            }
        }
    };

    let template = crate::emails::PasswordResetRequestTemplate {
        user_name: None,
        url_scheme: state.site_scheme(),
        host,
        email: body.email.clone(),
        token,
    };

    state
        .filigree
        .email
        .send_template(body.email, template)
        .await
        .change_context(Error::AuthSubsystem)?;

    Ok(())
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use filigree::{auth::endpoints::UpdatePasswordRequest, testing};
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::{
        auth::tests::extract_token_from_email,
        tests::{start_app, BootstrappedData},
    };

    #[sqlx::test]
    #[cfg_attr(not(feature = "test_password"), ignore = "slow password test")]
    async fn change_password(db: sqlx::PgPool) {
        let (app, BootstrappedData { user, .. }) = start_app(db).await;

        app.client
            .post("auth/request_password_reset")
            .json(&EmailBody {
                email: user.email.clone(),
            })
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        let email = app.sent_emails.lock().unwrap().pop().unwrap();
        let token = extract_token_from_email(&email);

        app.client
            .post("auth/update_password")
            .json(&UpdatePasswordRequest {
                email: user.email.clone(),
                token: Uuid::from_str(&token).unwrap(),
                password: "a_new_password".to_string(),
                confirm: "a_new_password".to_string(),
            })
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        // Try to log in with the new password
        app.client
            .post("auth/login")
            .json(&json!({ "email": user.email, "password": "a_new_password"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        // Should not be able to reuse password token
        let response = app
            .client
            .post("auth/update_password")
            .json(&UpdatePasswordRequest {
                email: user.email.clone(),
                token: Uuid::from_str(&token).unwrap(),
                password: "other_password".to_string(),
                confirm: "other_password".to_string(),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    #[cfg_attr(not(feature = "test_password"), ignore = "slow password test")]
    async fn invalid_reset_token(db: sqlx::PgPool) {
        let (app, BootstrappedData { user, .. }) = start_app(db).await;

        app.client
            .post("auth/request_password_reset")
            .json(&EmailBody {
                email: user.email.clone(),
            })
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        let email = app.sent_emails.lock().unwrap().pop().unwrap();
        let token = extract_token_from_email(&email);

        let response = app
            .client
            .post("auth/update_password")
            .json(&UpdatePasswordRequest {
                email: user.email.clone(),
                token: Uuid::new_v4(),
                password: "a_new_password".to_string(),
                confirm: "a_new_password".to_string(),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

        // Old password should still be in effect
        app.client
            .post("auth/login")
            .json(&json!({ "email": user.email, "password": testing::TEST_PASSWORD }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    #[sqlx::test]
    #[cfg_attr(not(feature = "test_password"), ignore = "slow password test")]
    async fn expired_reset_token(db: sqlx::PgPool) {
        let (app, BootstrappedData { user, .. }) = start_app(db.clone()).await;

        app.client
            .post("auth/request_password_reset")
            .json(&EmailBody {
                email: user.email.clone(),
            })
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        let email = app.sent_emails.lock().unwrap().pop().unwrap();
        let token = extract_token_from_email(&email);

        // Force it to expire
        sqlx::query!(
            "UPDATE email_logins
            SET reset_expires_at = now() - '1 second'::interval
            WHERE email=$1",
            user.email
        )
        .execute(&db)
        .await
        .unwrap();

        let response = app
            .client
            .post("auth/update_password")
            .json(&UpdatePasswordRequest {
                email: user.email.clone(),
                token: Uuid::from_str(&token).unwrap(),
                password: "a_new_password".to_string(),
                confirm: "a_new_password".to_string(),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

        // Old password should still be in effect
        app.client
            .post("auth/login")
            .json(&json!({ "email": user.email, "password": testing::TEST_PASSWORD }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    #[sqlx::test]
    #[cfg_attr(not(feature = "test_password"), ignore = "slow password test")]
    async fn password_mismatch(db: sqlx::PgPool) {
        let (app, BootstrappedData { user, .. }) = start_app(db).await;

        app.client
            .post("auth/request_password_reset")
            .json(&EmailBody {
                email: user.email.clone(),
            })
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        let email = app.sent_emails.lock().unwrap().pop().unwrap();
        let token = extract_token_from_email(&email);

        let response = app
            .client
            .post("auth/update_password")
            .json(&UpdatePasswordRequest {
                email: user.email.clone(),
                token: Uuid::from_str(&token).unwrap(),
                password: "a_new_password".to_string(),
                confirm: "not-the-same".to_string(),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

        // Reset token should still be valid on a confirm mismatch
        app.client
            .post("auth/update_password")
            .json(&UpdatePasswordRequest {
                email: user.email.clone(),
                token: Uuid::from_str(&token).unwrap(),
                password: "a_new_password".to_string(),
                confirm: "a_new_password".to_string(),
            })
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        // Try to log in with the new password
        app.client
            .post("auth/login")
            .json(&json!({ "email": user.email, "password": "a_new_password"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }

    #[sqlx::test]
    #[ignore = "todo"]
    async fn bad_host_header() {}
}
