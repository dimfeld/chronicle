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

use crate::models::alias_model::AliasModel;
use crate::models::alias_model::AliasModelCreatePayload;
use crate::models::alias_model::AliasModelId;
use crate::models::alias_model::AliasModelUpdatePayload;
use crate::{
    auth::{has_any_permission, Authed},
    server::ServerState,
    Error,
};

use super::{
    queries, types::*, AliasId, CREATE_PERMISSION, OWNER_PERMISSION, READ_PERMISSION,
    WRITE_PERMISSION,
};

async fn get(
    State(state): State<ServerState>,
    auth: Authed,
    Path(id): Path<AliasId>,
) -> Result<impl IntoResponse, Error> {
    let object = queries::get_populated(&state.db, &auth, id).await?;

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
    FormOrJson(payload): FormOrJson<AliasCreatePayload>,
) -> Result<impl IntoResponse, Error> {
    let mut tx = state.db.begin().await.change_context(Error::Db)?;
    let result = queries::create(&mut *tx, &auth, payload).await?;
    tx.commit().await.change_context(Error::Db)?;

    Ok((StatusCode::CREATED, Json(result)))
}

async fn update(
    State(state): State<ServerState>,
    auth: Authed,
    Path(id): Path<AliasId>,
    FormOrJson(payload): FormOrJson<AliasUpdatePayload>,
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
    Path(id): Path<AliasId>,
) -> Result<impl IntoResponse, Error> {
    let mut tx = state.db.begin().await.change_context(Error::Db)?;

    let deleted = queries::delete(&mut *tx, &auth, id).await?;

    if !deleted {
        return Ok(StatusCode::NOT_FOUND);
    }

    tx.commit().await.change_context(Error::Db)?;

    Ok(StatusCode::OK)
}

async fn list_child_alias_model(
    State(state): State<ServerState>,
    auth: Authed,
    Path(parent_id): Path<AliasId>,
    Query(mut qs): Query<crate::models::alias_model::queries::ListQueryFilters>,
) -> Result<impl IntoResponse, Error> {
    qs.alias_id = vec![parent_id];

    let object = crate::models::alias_model::queries::list(&state.db, &auth, &qs).await?;

    Ok(Json(object))
}

async fn get_child_alias_model(
    State(state): State<ServerState>,
    auth: Authed,
    Path((parent_id, child_id)): Path<(AliasId, AliasModelId)>,
) -> Result<impl IntoResponse, Error> {
    let object = crate::models::alias_model::queries::get(&state.db, &auth, child_id).await?;
    if object.alias_id != parent_id {
        return Err(Error::NotFound("Parent Alias"));
    }

    Ok(Json(object))
}

async fn create_child_alias_model(
    State(state): State<ServerState>,
    auth: Authed,
    Path(parent_id): Path<AliasId>,
    FormOrJson(mut payload): FormOrJson<AliasModelCreatePayload>,
) -> Result<impl IntoResponse, Error> {
    let mut tx = state.db.begin().await.change_context(Error::Db)?;

    payload.alias_id = parent_id;

    let result = crate::models::alias_model::queries::create(&mut *tx, &auth, payload).await?;

    tx.commit().await.change_context(Error::Db)?;

    Ok(Json(result))
}

async fn update_child_alias_model(
    State(state): State<ServerState>,
    auth: Authed,
    Path((parent_id, child_id)): Path<(AliasId, AliasModelId)>,
    FormOrJson(mut payload): FormOrJson<AliasModelUpdatePayload>,
) -> Result<impl IntoResponse, Error> {
    payload.id = Some(child_id);
    payload.alias_id = parent_id;

    let object_perm = queries::lookup_object_permissions(&state.db, &auth, parent_id)
        .await?
        .unwrap_or(ObjectPermission::Read);

    let is_owner = match object_perm {
        ObjectPermission::Owner => true,
        ObjectPermission::Write => false,
        ObjectPermission::Read => {
            return Err(Error::AuthError(AuthError::MissingPermission(
                Cow::Borrowed(super::WRITE_PERMISSION),
            )));
        }
    };

    let result = crate::models::alias_model::queries::update_one_with_parent(
        &state.db, &auth, is_owner, parent_id, child_id, payload,
    )
    .await?;

    Ok(Json(result))
}

async fn delete_child_alias_model(
    State(state): State<ServerState>,
    auth: Authed,
    Path((parent_id, child_id)): Path<(AliasId, AliasModelId)>,
) -> Result<impl IntoResponse, Error> {
    let deleted = crate::models::alias_model::queries::delete_with_parent(
        &state.db, &auth, parent_id, child_id,
    )
    .await?;

    if deleted {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

pub fn create_routes() -> axum::Router<ServerState> {
    axum::Router::new()
        .route(
            "/aliases",
            routing::get(list).route_layer(has_any_permission(vec![READ_PERMISSION, "org_admin"])),
        )
        .route(
            "/aliases/:id",
            routing::get(get).route_layer(has_any_permission(vec![READ_PERMISSION, "org_admin"])),
        )
        .route(
            "/aliases",
            routing::post(create)
                .route_layer(has_any_permission(vec![CREATE_PERMISSION, "org_admin"])),
        )
        .route(
            "/aliases/:id",
            routing::put(update).route_layer(has_any_permission(vec![
                WRITE_PERMISSION,
                OWNER_PERMISSION,
                "org_admin",
            ])),
        )
        .route(
            "/aliases/:id",
            routing::delete(delete)
                .route_layer(has_any_permission(vec![CREATE_PERMISSION, "org_admin"])),
        )
        .route(
            "/aliases/:id/alias_models",
            routing::get(list_child_alias_model)
                .route_layer(has_any_permission(vec![READ_PERMISSION, "org_admin"])),
        )
        .route(
            "/aliases/:id/alias_models",
            routing::post(create_child_alias_model)
                .route_layer(has_any_permission(vec![CREATE_PERMISSION, "org_admin"])),
        )
        .route(
            "/aliases/:id/alias_models/:child_id",
            routing::get(get_child_alias_model)
                .route_layer(has_any_permission(vec![READ_PERMISSION, "org_admin"])),
        )
        .route(
            "/aliases/:id/alias_models/:child_id",
            routing::put(update_child_alias_model).route_layer(has_any_permission(vec![
                WRITE_PERMISSION,
                OWNER_PERMISSION,
                "org_admin",
            ])),
        )
        .route(
            "/aliases/:id/alias_models/:child_id",
            routing::delete(delete_child_alias_model)
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
    ) -> Vec<(AliasCreatePayload, AliasCreateResult)> {
        let mut tx = db.begin().await.unwrap();
        let mut objects = Vec::with_capacity(count);
        for i in 0..count {
            let id = AliasId::new();
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
            .get("aliases")
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
                result["random_order"],
                serde_json::to_value(&added.random_order).unwrap(),
                "field random_order"
            );

            assert_eq!(
                payload.random_order, added.random_order,
                "create result field random_order"
            );

            assert_eq!(result["_permission"], "owner");
        }

        let results = user
            .client
            .get("aliases")
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
                result["random_order"],
                serde_json::to_value(&added.random_order).unwrap(),
                "list result field random_order"
            );
            assert_eq!(result["_permission"], "write");
        }

        let response = no_roles_user.client.get("aliases").send().await.unwrap();

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
            .get("aliases")
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
            .get(&format!("aliases/{}", added_objects[1].1.id))
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
            result["random_order"],
            serde_json::to_value(&added.random_order).unwrap(),
            "get result field random_order"
        );

        assert_eq!(
            added.random_order, payload.random_order,
            "create result field random_order"
        );

        assert_eq!(result["_permission"], "owner");

        assert_eq!(result["models"], serde_json::json!([]), "field models");

        let result = user
            .client
            .get(&format!("aliases/{}", added_objects[1].1.id))
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
            result["random_order"],
            serde_json::to_value(&added.random_order).unwrap(),
            "get result field random_order"
        );
        assert_eq!(result["_permission"], "write");

        let response = no_roles_user
            .client
            .get(&format!("aliases/{}", added_objects[1].1.id))
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
            .put(&format!("aliases/{}", added_objects[1].1.id))
            .json(&update_payload)
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap();

        let updated: serde_json::Value = admin_user
            .client
            .get(&format!("aliases/{}", added_objects[1].1.id))
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
            updated["random_order"],
            serde_json::to_value(&update_payload.random_order).unwrap(),
            "field random_order"
        );
        assert_eq!(updated["_permission"], "owner");

        // TODO Test that owner can not write fields which are not writable by anyone.
        // TODO Test that user can not update fields which are writable by owner but not user

        // Make sure that no other objects were updated
        let non_updated: serde_json::Value = admin_user
            .client
            .get(&format!("aliases/{}", added_objects[0].1.id))
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
            non_updated["random_order"],
            serde_json::to_value(&added_objects[0].1.random_order).unwrap(),
            "field random_order"
        );
        assert_eq!(non_updated["_permission"], "owner");

        let response = no_roles_user
            .client
            .put(&format!("aliases/{}", added_objects[1].1.id))
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
            .post("aliases")
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
            created_result["random_order"],
            serde_json::to_value(&create_payload.random_order).unwrap(),
            "field random_order from create response"
        );
        assert_eq!(created_result["_permission"], "owner");

        let created_id = created_result["id"].as_str().unwrap();
        let get_result = admin_user
            .client
            .get(&format!("aliases/{}", created_id))
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
            get_result["random_order"], created_result["random_order"],
            "field random_order from get response"
        );
        assert_eq!(get_result["_permission"], "owner");

        let response = no_roles_user
            .client
            .post("aliases")
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
            .delete(&format!("aliases/{}", added_objects[1].1.id))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap();

        let response = admin_user
            .client
            .get(&format!("aliases/{}", added_objects[1].1.id))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

        // Delete should not happen without permissions
        let response = no_roles_user
            .client
            .delete(&format!("aliases/{}", added_objects[0].1.id))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

        // Make sure other objects still exist
        let response = admin_user
            .client
            .get(&format!("aliases/{}", added_objects[0].1.id))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    #[sqlx::test]
    async fn child_alias_model(pool: sqlx::PgPool) {
        // Create a test object
        let (
            _app,
            BootstrappedData {
                organization,
                admin_user,
                no_roles_user,
                ..
            },
        ) = start_app(pool.clone()).await;

        let (parent_payload, parent_result) = setup_test_objects(&pool, organization.id, 2)
            .await
            .into_iter()
            .next()
            .unwrap();

        // Create a test object
        let payload_one = crate::models::alias_model::testing::make_create_payload(1);
        let create_result_one = admin_user
            .client
            .post(&format!("aliases/{}/alias_models", parent_result.id))
            .json(&payload_one)
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let id_one = AliasModelId::from_str(create_result_one["id"].as_str().unwrap()).unwrap();

        // Try to create a test object with a bad parent id
        let bad_parent_id = AliasId::new();
        let payload_two = crate::models::alias_model::testing::make_create_payload(1);
        let response = admin_user
            .client
            .post(&format!("aliases/{}/alias_models", bad_parent_id))
            .json(&payload_two)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

        // Create another test object
        let payload_two = crate::models::alias_model::testing::make_create_payload(2);
        let create_result_two = admin_user
            .client
            .post(&format!("aliases/{}/alias_models", parent_result.id))
            .json(&payload_two)
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let id_two = AliasModelId::from_str(create_result_two["id"].as_str().unwrap()).unwrap();

        // Check create without permissions
        let bad_create_payload = crate::models::alias_model::testing::make_create_payload(9);
        let res = no_roles_user
            .client
            .post(&format!("aliases/{}/alias_models", parent_result.id))
            .json(&bad_create_payload)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

        // Check get without permissions
        let res = no_roles_user
            .client
            .get(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id, id_one
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

        // Check update without permissions
        let bad_update_payload = crate::models::alias_model::testing::make_update_payload(8);
        let res = no_roles_user
            .client
            .put(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id, id_one
            ))
            .json(&bad_update_payload)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

        // Check delete without permissions
        let res = no_roles_user
            .client
            .delete(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id, id_one
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);

        // Check get of test object
        let get_result_one = admin_user
            .client
            .get(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id,
                create_result_one["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(create_result_one, get_result_one);

        // Check get of test object with a different parent ID
        let get_result_bad_parent = admin_user
            .client
            .get(&format!(
                "aliases/{}/alias_models/{}",
                bad_parent_id,
                create_result_one["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(
            get_result_bad_parent.status(),
            reqwest::StatusCode::NOT_FOUND
        );

        // Check update of test object
        let update_payload_one = crate::models::alias_model::testing::make_update_payload(5);
        let update_result_one = admin_user
            .client
            .put(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id,
                create_result_one["id"].as_str().unwrap()
            ))
            .json(&update_payload_one)
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        // Check update of test object with a different parent ID
        let bad_update_payload = crate::models::alias_model::testing::make_update_payload(5);
        let bad_update_response = admin_user
            .client
            .put(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id,
                create_result_one["id"].as_str().unwrap()
            ))
            .json(&bad_update_payload)
            .send()
            .await
            .unwrap();
        // TODO this is broken right now
        // assert_eq!(bad_update_response.status(), reqwest::StatusCode::NOT_FOUND);

        // Check that the data reflects the first update
        let updated_get_result_one = admin_user
            .client
            .get(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id,
                create_result_one["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        // TODO generate comparison for updated_get_result_one and update_payload_one

        // Check delete of test object with a different parent ID
        let delete_result = admin_user
            .client
            .delete(&format!(
                "aliases/{}/alias_models/{}",
                bad_parent_id,
                create_result_one["id"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(delete_result.status(), reqwest::StatusCode::NOT_FOUND);

        // Check list of test object
        let list_result = admin_user
            .client
            .get(&format!("aliases/{}/alias_models", parent_result.id))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap()
            .json::<Vec<AliasModel>>()
            .await
            .unwrap();

        assert!(list_result[0].id == id_one || list_result[0].id == id_two);
        assert!(list_result[1].id == id_one || list_result[1].id == id_two);
        // Just make sure that we didn't get the same object twice
        assert_ne!(list_result[0].id, list_result[1].id);
        assert_eq!(list_result.len(), 2);

        // Check delete of test object
        admin_user
            .client
            .delete(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id, id_one
            ))
            .send()
            .await
            .unwrap()
            .log_error()
            .await
            .unwrap();

        let res = admin_user
            .client
            .get(&format!(
                "aliases/{}/alias_models/{}",
                parent_result.id, id_one
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
    }
}
