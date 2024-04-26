#![allow(unused_imports, unused_variables, dead_code)]
use std::{borrow::Cow, str::FromStr};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing,
};
use axum_extra::extract::Query;
use axum_jsonschema::Json;
use error_stack::ResultExt;
use filigree::{
    auth::{AuthError, ObjectPermission},
    extract::FormOrJson,
};
use tracing::{event, Level};

use crate::{
    auth::{has_any_permission, Authed},
    server::ServerState,
    Error,
};

use super::{
    queries, types::*, CustomProviderId, CREATE_PERMISSION, OWNER_PERMISSION, READ_PERMISSION,
    WRITE_PERMISSION,
};

async fn get(
    State(state): State<ServerState>,
    auth: Authed,
    Path(id): Path<CustomProviderId>,
) -> Result<impl IntoResponse, Error> {
    let object = queries::get(&state.db, &auth, id).await?;

    Ok(Json(object))
}

async fn list(
    State(state): State<ServerState>,
    auth: Authed,
    Query(qs): Query<queries::ListQueryFilters>,
) -> Result<impl IntoResponse, Error> {
    let results = queries::list(&state.db, &auth, &qs).await?;

    Ok(Json(results))
}

async fn create(
    State(state): State<ServerState>,
    auth: Authed,
    FormOrJson(payload): FormOrJson<CustomProviderCreatePayload>,
) -> Result<impl IntoResponse, Error> {
    let mut tx = state.db.begin().await.change_context(Error::Db)?;
    let result = queries::create(&mut *tx, &auth, payload).await?;
    tx.commit().await.change_context(Error::Db)?;

    Ok((StatusCode::CREATED, Json(result)))
}

async fn update(
    State(state): State<ServerState>,
    auth: Authed,
    Path(id): Path<CustomProviderId>,
    FormOrJson(payload): FormOrJson<CustomProviderUpdatePayload>,
) -> Result<impl IntoResponse, Error> {
    let mut tx = state.db.begin().await.change_context(Error::Db)?;

    let result = queries::update(&mut *tx, &auth, id, payload).await?;

    tx.commit().await.change_context(Error::Db)?;

    if result {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

async fn delete(
    State(state): State<ServerState>,
    auth: Authed,
    Path(id): Path<CustomProviderId>,
) -> Result<impl IntoResponse, Error> {
    let mut tx = state.db.begin().await.change_context(Error::Db)?;

    let deleted = queries::delete(&mut *tx, &auth, id).await?;

    if !deleted {
        return Ok(StatusCode::NOT_FOUND);
    }

    tx.commit().await.change_context(Error::Db)?;

    Ok(StatusCode::OK)
}

pub fn create_routes() -> axum::Router<ServerState> {
    axum::Router::new()
        .route(
            "/custom_providers",
            routing::get(list).route_layer(has_any_permission(vec![READ_PERMISSION, "org_admin"])),
        )
        .route(
            "/custom_providers/:id",
            routing::get(get).route_layer(has_any_permission(vec![READ_PERMISSION, "org_admin"])),
        )
        .route(
            "/custom_providers",
            routing::post(create)
                .route_layer(has_any_permission(vec![CREATE_PERMISSION, "org_admin"])),
        )
        .route(
            "/custom_providers/:id",
            routing::put(update).route_layer(has_any_permission(vec![
                WRITE_PERMISSION,
                OWNER_PERMISSION,
                "org_admin",
            ])),
        )
        .route(
            "/custom_providers/:id",
            routing::delete(delete)
                .route_layer(has_any_permission(vec![CREATE_PERMISSION, "org_admin"])),
        )
}

#[cfg(test)]
mod test {
    use filigree::testing::ResponseExt;
    use futures::{StreamExt, TryStreamExt};
    use tracing::{event, Level};

    use super::super::testing::{make_create_payload, make_update_payload};
    use super::*;
    use crate::{
        models::organization::OrganizationId,
        tests::{start_app, BootstrappedData},
    };

    async fn setup_test_objects(
        db: &sqlx::PgPool,
        organization_id: OrganizationId,
        count: usize,
    ) -> Vec<(CustomProviderCreatePayload, CustomProviderCreateResult)> {
        let mut tx = db.begin().await.unwrap();
        let mut objects = Vec::with_capacity(count);
        for i in 0..count {
            let id = CustomProviderId::new();
            event!(Level::INFO, %id, "Creating test object {}", i);
            let payload = make_create_payload(i);
            let result = super::queries::create_raw(&mut *tx, id, organization_id, payload.clone())
                .await
                .expect("Creating test object failed");

            objects.push((payload, result));
        }

        tx.commit().await.unwrap();
        objects
    }

    #[sqlx::test]
    async fn list_objects(pool: sqlx::PgPool) {
        let (
            _app,
            BootstrappedData {
                organization,
                admin_user,
                no_roles_user,
                user,
                ..
            },
        ) = start_app(pool.clone()).await;

        let added_objects = setup_test_objects(&pool, organization.id, 3).await;

        let results = admin_user
            .client
            .get("custom_providers")
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<Vec<serde_json::Value>>()
            .await
            .unwrap();

        assert_eq!(results.len(), added_objects.len());

        for result in results {
            let (payload, added) = added_objects
                .iter()
                .find(|i| i.1.id.to_string() == result["id"].as_str().unwrap())
                .expect("Returned object did not match any of the added objects");
            assert_eq!(
                result["id"],
                serde_json::to_value(&added.id).unwrap(),
                "field id"
            );
            assert_eq!(
                result["organization_id"],
                serde_json::to_value(&added.organization_id).unwrap(),
                "field organization_id"
            );
            assert_eq!(
                result["updated_at"],
                serde_json::to_value(&added.updated_at).unwrap(),
                "field updated_at"
            );
            assert_eq!(
                result["created_at"],
                serde_json::to_value(&added.created_at).unwrap(),
                "field created_at"
            );
            assert_eq!(
                result["name"],
                serde_json::to_value(&added.name).unwrap(),
                "field name"
            );

            assert_eq!(payload.name, added.name, "create result field name");
            assert_eq!(
                result["label"],
                serde_json::to_value(&added.label).unwrap(),
                "field label"
            );

            assert_eq!(payload.label, added.label, "create result field label");
            assert_eq!(
                result["url"],
                serde_json::to_value(&added.url).unwrap(),
                "field url"
            );

            assert_eq!(payload.url, added.url, "create result field url");
            assert_eq!(
                result["token"],
                serde_json::to_value(&added.token).unwrap(),
                "field token"
            );

            assert_eq!(payload.token, added.token, "create result field token");
            assert_eq!(
                result["api_key"],
                serde_json::to_value(&added.api_key).unwrap(),
                "field api_key"
            );

            assert_eq!(
                payload.api_key, added.api_key,
                "create result field api_key"
            );
            assert_eq!(
                result["api_key_source"],
                serde_json::to_value(&added.api_key_source).unwrap(),
                "field api_key_source"
            );

            assert_eq!(
                payload.api_key_source, added.api_key_source,
                "create result field api_key_source"
            );
            assert_eq!(
                result["format"],
                serde_json::to_value(&added.format).unwrap(),
                "field format"
            );

            assert_eq!(payload.format, added.format, "create result field format");
            assert_eq!(
                result["headers"],
                serde_json::to_value(&added.headers).unwrap(),
                "field headers"
            );

            assert_eq!(
                payload.headers, added.headers,
                "create result field headers"
            );
            assert_eq!(
                result["prefix"],
                serde_json::to_value(&added.prefix).unwrap(),
                "field prefix"
            );

            assert_eq!(payload.prefix, added.prefix, "create result field prefix");

            assert_eq!(result["_permission"], "owner");
        }

        let results = user
            .client
            .get("custom_providers")
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<Vec<serde_json::Value>>()
            .await
            .unwrap();

        for result in results {
            let (payload, added) = added_objects
                .iter()
                .find(|i| i.1.id.to_string() == result["id"].as_str().unwrap())
                .expect("Returned object did not match any of the added objects");
            assert_eq!(
                result["id"],
                serde_json::to_value(&added.id).unwrap(),
                "list result field id"
            );
            assert_eq!(
                result["organization_id"],
                serde_json::to_value(&added.organization_id).unwrap(),
                "list result field organization_id"
            );
            assert_eq!(
                result["updated_at"],
                serde_json::to_value(&added.updated_at).unwrap(),
                "list result field updated_at"
            );
            assert_eq!(
                result["created_at"],
                serde_json::to_value(&added.created_at).unwrap(),
                "list result field created_at"
            );
            assert_eq!(
                result["name"],
                serde_json::to_value(&added.name).unwrap(),
                "list result field name"
            );
            assert_eq!(
                result["label"],
                serde_json::to_value(&added.label).unwrap(),
                "list result field label"
            );
            assert_eq!(
                result["url"],
                serde_json::to_value(&added.url).unwrap(),
                "list result field url"
            );
            assert_eq!(
                result["token"],
                serde_json::to_value(&added.token).unwrap(),
                "list result field token"
            );
            assert_eq!(
                result["api_key"],
                serde_json::to_value(&added.api_key).unwrap(),
                "list result field api_key"
            );
            assert_eq!(
                result["api_key_source"],
                serde_json::to_value(&added.api_key_source).unwrap(),
                "list result field api_key_source"
            );
            assert_eq!(
                result["format"],
                serde_json::to_value(&added.format).unwrap(),
                "list result field format"
            );
            assert_eq!(
                result["headers"],
                serde_json::to_value(&added.headers).unwrap(),
                "list result field headers"
            );
            assert_eq!(
                result["prefix"],
                serde_json::to_value(&added.prefix).unwrap(),
                "list result field prefix"
            );
            assert_eq!(result["_permission"], "write");
        }

        let response = no_roles_user
            .client
            .get("custom_providers")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    }

    #[sqlx::test]
    async fn list_fetch_specific_ids(pool: sqlx::PgPool) {
        let (
            _app,
            BootstrappedData {
                organization, user, ..
            },
        ) = start_app(pool.clone()).await;

        let added_objects = setup_test_objects(&pool, organization.id, 3).await;

        let results = user
            .client
            .get("custom_providers")
            .query(&[("id", added_objects[0].1.id), ("id", added_objects[2].1.id)])
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<Vec<serde_json::Value>>()
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|o| o["id"] == added_objects[0].1.id.to_string()));
        assert!(results
            .iter()
            .any(|o| o["id"] == added_objects[2].1.id.to_string()));
    }

    #[sqlx::test]
    #[ignore = "todo"]
    async fn list_order_by(_pool: sqlx::PgPool) {}

    #[sqlx::test]
    #[ignore = "todo"]
    async fn list_paginated(_pool: sqlx::PgPool) {}

    #[sqlx::test]
    #[ignore = "todo"]
    async fn list_filters(_pool: sqlx::PgPool) {}

    #[sqlx::test]
    async fn get_object(pool: sqlx::PgPool) {
        let (
            _app,
            BootstrappedData {
                organization,
                admin_user,
                user,
                no_roles_user,
                ..
            },
        ) = start_app(pool.clone()).await;

        let added_objects = setup_test_objects(&pool, organization.id, 2).await;

        let result = admin_user
            .client
            .get(&format!("custom_providers/{}", added_objects[1].1.id))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        let (payload, added) = &added_objects[1];
        assert_eq!(
            result["id"],
            serde_json::to_value(&added.id).unwrap(),
            "get result field id"
        );
        assert_eq!(
            result["organization_id"],
            serde_json::to_value(&added.organization_id).unwrap(),
            "get result field organization_id"
        );
        assert_eq!(
            result["updated_at"],
            serde_json::to_value(&added.updated_at).unwrap(),
            "get result field updated_at"
        );
        assert_eq!(
            result["created_at"],
            serde_json::to_value(&added.created_at).unwrap(),
            "get result field created_at"
        );
        assert_eq!(
            result["name"],
            serde_json::to_value(&added.name).unwrap(),
            "get result field name"
        );

        assert_eq!(added.name, payload.name, "create result field name");
        assert_eq!(
            result["label"],
            serde_json::to_value(&added.label).unwrap(),
            "get result field label"
        );

        assert_eq!(added.label, payload.label, "create result field label");
        assert_eq!(
            result["url"],
            serde_json::to_value(&added.url).unwrap(),
            "get result field url"
        );

        assert_eq!(added.url, payload.url, "create result field url");
        assert_eq!(
            result["token"],
            serde_json::to_value(&added.token).unwrap(),
            "get result field token"
        );

        assert_eq!(added.token, payload.token, "create result field token");
        assert_eq!(
            result["api_key"],
            serde_json::to_value(&added.api_key).unwrap(),
            "get result field api_key"
        );

        assert_eq!(
            added.api_key, payload.api_key,
            "create result field api_key"
        );
        assert_eq!(
            result["api_key_source"],
            serde_json::to_value(&added.api_key_source).unwrap(),
            "get result field api_key_source"
        );

        assert_eq!(
            added.api_key_source, payload.api_key_source,
            "create result field api_key_source"
        );
        assert_eq!(
            result["format"],
            serde_json::to_value(&added.format).unwrap(),
            "get result field format"
        );

        assert_eq!(added.format, payload.format, "create result field format");
        assert_eq!(
            result["headers"],
            serde_json::to_value(&added.headers).unwrap(),
            "get result field headers"
        );

        assert_eq!(
            added.headers, payload.headers,
            "create result field headers"
        );
        assert_eq!(
            result["prefix"],
            serde_json::to_value(&added.prefix).unwrap(),
            "get result field prefix"
        );

        assert_eq!(added.prefix, payload.prefix, "create result field prefix");

        assert_eq!(result["_permission"], "owner");

        let result = user
            .client
            .get(&format!("custom_providers/{}", added_objects[1].1.id))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        let (payload, added) = &added_objects[1];
        assert_eq!(
            result["id"],
            serde_json::to_value(&added.id).unwrap(),
            "get result field id"
        );
        assert_eq!(
            result["organization_id"],
            serde_json::to_value(&added.organization_id).unwrap(),
            "get result field organization_id"
        );
        assert_eq!(
            result["updated_at"],
            serde_json::to_value(&added.updated_at).unwrap(),
            "get result field updated_at"
        );
        assert_eq!(
            result["created_at"],
            serde_json::to_value(&added.created_at).unwrap(),
            "get result field created_at"
        );
        assert_eq!(
            result["name"],
            serde_json::to_value(&added.name).unwrap(),
            "get result field name"
        );
        assert_eq!(
            result["label"],
            serde_json::to_value(&added.label).unwrap(),
            "get result field label"
        );
        assert_eq!(
            result["url"],
            serde_json::to_value(&added.url).unwrap(),
            "get result field url"
        );
        assert_eq!(
            result["token"],
            serde_json::to_value(&added.token).unwrap(),
            "get result field token"
        );
        assert_eq!(
            result["api_key"],
            serde_json::to_value(&added.api_key).unwrap(),
            "get result field api_key"
        );
        assert_eq!(
            result["api_key_source"],
            serde_json::to_value(&added.api_key_source).unwrap(),
            "get result field api_key_source"
        );
        assert_eq!(
            result["format"],
            serde_json::to_value(&added.format).unwrap(),
            "get result field format"
        );
        assert_eq!(
            result["headers"],
            serde_json::to_value(&added.headers).unwrap(),
            "get result field headers"
        );
        assert_eq!(
            result["prefix"],
            serde_json::to_value(&added.prefix).unwrap(),
            "get result field prefix"
        );
        assert_eq!(result["_permission"], "write");

        let response = no_roles_user
            .client
            .get(&format!("custom_providers/{}", added_objects[1].1.id))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    }

    #[sqlx::test]
    async fn update_object(pool: sqlx::PgPool) {
        let (
            _app,
            BootstrappedData {
                organization,
                admin_user,
                no_roles_user,
                ..
            },
        ) = start_app(pool.clone()).await;

        let added_objects = setup_test_objects(&pool, organization.id, 2).await;

        let update_payload = make_update_payload(20);
        admin_user
            .client
            .put(&format!("custom_providers/{}", added_objects[1].1.id))
            .json(&update_payload)
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap();

        let updated: serde_json::Value = admin_user
            .client
            .get(&format!("custom_providers/{}", added_objects[1].1.id))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(
            updated["name"],
            serde_json::to_value(&update_payload.name).unwrap(),
            "field name"
        );
        assert_eq!(
            updated["label"],
            serde_json::to_value(&update_payload.label).unwrap(),
            "field label"
        );
        assert_eq!(
            updated["url"],
            serde_json::to_value(&update_payload.url).unwrap(),
            "field url"
        );
        assert_eq!(
            updated["token"],
            serde_json::to_value(&update_payload.token).unwrap(),
            "field token"
        );
        assert_eq!(
            updated["api_key"],
            serde_json::to_value(&update_payload.api_key).unwrap(),
            "field api_key"
        );
        assert_eq!(
            updated["api_key_source"],
            serde_json::to_value(&update_payload.api_key_source).unwrap(),
            "field api_key_source"
        );
        assert_eq!(
            updated["format"],
            serde_json::to_value(&update_payload.format).unwrap(),
            "field format"
        );
        assert_eq!(
            updated["headers"],
            serde_json::to_value(&update_payload.headers).unwrap(),
            "field headers"
        );
        assert_eq!(
            updated["prefix"],
            serde_json::to_value(&update_payload.prefix).unwrap(),
            "field prefix"
        );
        assert_eq!(updated["_permission"], "owner");

        // TODO Test that owner can not write fields which are not writable by anyone.
        // TODO Test that user can not update fields which are writable by owner but not user

        // Make sure that no other objects were updated
        let non_updated: serde_json::Value = admin_user
            .client
            .get(&format!("custom_providers/{}", added_objects[0].1.id))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(
            non_updated["id"],
            serde_json::to_value(&added_objects[0].1.id).unwrap(),
            "field id"
        );
        assert_eq!(
            non_updated["organization_id"],
            serde_json::to_value(&added_objects[0].1.organization_id).unwrap(),
            "field organization_id"
        );
        assert_eq!(
            non_updated["updated_at"],
            serde_json::to_value(&added_objects[0].1.updated_at).unwrap(),
            "field updated_at"
        );
        assert_eq!(
            non_updated["created_at"],
            serde_json::to_value(&added_objects[0].1.created_at).unwrap(),
            "field created_at"
        );
        assert_eq!(
            non_updated["name"],
            serde_json::to_value(&added_objects[0].1.name).unwrap(),
            "field name"
        );
        assert_eq!(
            non_updated["label"],
            serde_json::to_value(&added_objects[0].1.label).unwrap(),
            "field label"
        );
        assert_eq!(
            non_updated["url"],
            serde_json::to_value(&added_objects[0].1.url).unwrap(),
            "field url"
        );
        assert_eq!(
            non_updated["token"],
            serde_json::to_value(&added_objects[0].1.token).unwrap(),
            "field token"
        );
        assert_eq!(
            non_updated["api_key"],
            serde_json::to_value(&added_objects[0].1.api_key).unwrap(),
            "field api_key"
        );
        assert_eq!(
            non_updated["api_key_source"],
            serde_json::to_value(&added_objects[0].1.api_key_source).unwrap(),
            "field api_key_source"
        );
        assert_eq!(
            non_updated["format"],
            serde_json::to_value(&added_objects[0].1.format).unwrap(),
            "field format"
        );
        assert_eq!(
            non_updated["headers"],
            serde_json::to_value(&added_objects[0].1.headers).unwrap(),
            "field headers"
        );
        assert_eq!(
            non_updated["prefix"],
            serde_json::to_value(&added_objects[0].1.prefix).unwrap(),
            "field prefix"
        );
        assert_eq!(non_updated["_permission"], "owner");

        let response = no_roles_user
            .client
            .put(&format!("custom_providers/{}", added_objects[1].1.id))
            .json(&update_payload)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    }

    #[sqlx::test]
    async fn create_object(pool: sqlx::PgPool) {
        let (
            _app,
            BootstrappedData {
                admin_user,
                no_roles_user,
                ..
            },
        ) = start_app(pool.clone()).await;

        let create_payload = make_create_payload(10);
        let created_result: serde_json::Value = admin_user
            .client
            .post("custom_providers")
            .json(&create_payload)
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(
            created_result["name"],
            serde_json::to_value(&create_payload.name).unwrap(),
            "field name from create response"
        );
        assert_eq!(
            created_result["label"],
            serde_json::to_value(&create_payload.label).unwrap(),
            "field label from create response"
        );
        assert_eq!(
            created_result["url"],
            serde_json::to_value(&create_payload.url).unwrap(),
            "field url from create response"
        );
        assert_eq!(
            created_result["token"],
            serde_json::to_value(&create_payload.token).unwrap(),
            "field token from create response"
        );
        assert_eq!(
            created_result["api_key"],
            serde_json::to_value(&create_payload.api_key).unwrap(),
            "field api_key from create response"
        );
        assert_eq!(
            created_result["api_key_source"],
            serde_json::to_value(&create_payload.api_key_source).unwrap(),
            "field api_key_source from create response"
        );
        assert_eq!(
            created_result["format"],
            serde_json::to_value(&create_payload.format).unwrap(),
            "field format from create response"
        );
        assert_eq!(
            created_result["headers"],
            serde_json::to_value(&create_payload.headers).unwrap(),
            "field headers from create response"
        );
        assert_eq!(
            created_result["prefix"],
            serde_json::to_value(&create_payload.prefix).unwrap(),
            "field prefix from create response"
        );
        assert_eq!(created_result["_permission"], "owner");

        let created_id = created_result["id"].as_str().unwrap();
        let get_result = admin_user
            .client
            .get(&format!("custom_providers/{}", created_id))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        assert_eq!(
            get_result["id"], created_result["id"],
            "field id from get response"
        );
        assert_eq!(
            get_result["organization_id"], created_result["organization_id"],
            "field organization_id from get response"
        );
        assert_eq!(
            get_result["updated_at"], created_result["updated_at"],
            "field updated_at from get response"
        );
        assert_eq!(
            get_result["created_at"], created_result["created_at"],
            "field created_at from get response"
        );
        assert_eq!(
            get_result["name"], created_result["name"],
            "field name from get response"
        );
        assert_eq!(
            get_result["label"], created_result["label"],
            "field label from get response"
        );
        assert_eq!(
            get_result["url"], created_result["url"],
            "field url from get response"
        );
        assert_eq!(
            get_result["token"], created_result["token"],
            "field token from get response"
        );
        assert_eq!(
            get_result["api_key"], created_result["api_key"],
            "field api_key from get response"
        );
        assert_eq!(
            get_result["api_key_source"], created_result["api_key_source"],
            "field api_key_source from get response"
        );
        assert_eq!(
            get_result["format"], created_result["format"],
            "field format from get response"
        );
        assert_eq!(
            get_result["headers"], created_result["headers"],
            "field headers from get response"
        );
        assert_eq!(
            get_result["prefix"], created_result["prefix"],
            "field prefix from get response"
        );
        assert_eq!(get_result["_permission"], "owner");

        let response = no_roles_user
            .client
            .post("custom_providers")
            .json(&create_payload)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    }

    #[sqlx::test]
    async fn delete_object(pool: sqlx::PgPool) {
        let (
            _app,
            BootstrappedData {
                organization,
                admin_user,
                no_roles_user,
                ..
            },
        ) = start_app(pool.clone()).await;

        let added_objects = setup_test_objects(&pool, organization.id, 2).await;

        admin_user
            .client
            .delete(&format!("custom_providers/{}", added_objects[1].1.id))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap();

        let response = admin_user
            .client
            .get(&format!("custom_providers/{}", added_objects[1].1.id))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

        // Delete should not happen without permissions
        let response = no_roles_user
            .client
            .delete(&format!("custom_providers/{}", added_objects[0].1.id))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

        // Make sure other objects still exist
        let response = admin_user
            .client
            .get(&format!("custom_providers/{}", added_objects[0].1.id))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
}
