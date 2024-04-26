#![allow(unused_imports, unused_variables, dead_code)]
use error_stack::ResultExt;
use filigree::{
    auth::ObjectPermission,
    errors::OrderByError,
    sql::{BindingOperator, FilterBuilder, ValuesBuilder},
};
use serde::Deserialize;
use sqlx::{
    postgres::PgRow, query_file, query_file_as, query_file_scalar, PgConnection, PgExecutor,
};
use tracing::{event, instrument, Level};

use std::str::FromStr;

use super::CustomProviderId;

use crate::models::organization::OrganizationId;

use super::types::*;

use crate::auth::AuthInfo;
use crate::Error;

type QueryAs<'q, T> = sqlx::query::QueryAs<
    'q,
    sqlx::Postgres,
    T,
    <sqlx::Postgres as sqlx::database::HasArguments<'q>>::Arguments,
>;

/// Get a CustomProvider from the database
#[instrument(skip(db))]
pub async fn get(
    db: impl PgExecutor<'_>,
    auth: &AuthInfo,
    id: CustomProviderId,
) -> Result<CustomProvider, error_stack::Report<Error>> {
    let actor_ids = auth.actor_ids();
    let object = query_file_as!(
        CustomProvider,
        "src/models/custom_provider/select_one.sql",
        id.as_uuid(),
        auth.organization_id.as_uuid(),
        &actor_ids
    )
    .fetch_optional(db)
    .await
    .change_context(Error::Db)?
    .ok_or(Error::NotFound("CustomProvider"))?;

    Ok(object)
}

#[derive(Debug, Default)]
enum OrderByField {
    #[default]
    UpdatedAt,
    CreatedAt,
}

impl OrderByField {
    fn as_str(&self) -> &str {
        match self {
            Self::UpdatedAt => "updated_at",
            Self::CreatedAt => "created_at",
        }
    }

    fn allowed_direction(&self, descending: bool) -> bool {
        match self {
            _ => true,
        }
    }
}

impl std::str::FromStr for OrderByField {
    type Err = OrderByError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = match s {
            "updated_at" => OrderByField::UpdatedAt,
            "created_at" => OrderByField::CreatedAt,
            _ => return Err(OrderByError::InvalidField),
        };

        Ok(value)
    }
}

fn parse_order_by(field: &str) -> Result<(bool, OrderByField), OrderByError> {
    let descending = field.starts_with('-');
    let field = if descending { &field[1..] } else { field };

    let value = OrderByField::from_str(field)?;
    if !value.allowed_direction(descending) {
        return Err(OrderByError::InvalidDirection);
    }
    Ok((descending, value))
}

#[derive(Deserialize, Debug, Default)]
pub struct ListQueryFilters {
    pub page: Option<u32>,
    pub per_page: Option<u32>,

    pub order_by: Option<String>,
    #[serde(default)]
    pub id: Vec<CustomProviderId>,
    pub updated_at_lte: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at_gte: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at_lte: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at_gte: Option<chrono::DateTime<chrono::Utc>>,
}

impl ListQueryFilters {
    fn build_where_clause(&self) -> String {
        let mut bindings = FilterBuilder::new(5);

        if !self.id.is_empty() {
            bindings.add_vec("id", &self.id);
        }

        if self.updated_at_lte.is_some() {
            bindings.add_option("updated_at", &self.updated_at_lte, BindingOperator::Lte);
        }

        if self.updated_at_gte.is_some() {
            bindings.add_option("updated_at", &self.updated_at_gte, BindingOperator::Gte);
        }

        if self.created_at_lte.is_some() {
            bindings.add_option("created_at", &self.created_at_lte, BindingOperator::Lte);
        }

        if self.created_at_gte.is_some() {
            bindings.add_option("created_at", &self.created_at_gte, BindingOperator::Gte);
        }

        let query = bindings.to_string();
        event!(Level::DEBUG, %query);
        query
    }

    fn bind_to_query<'a, T>(&'a self, mut query: QueryAs<'a, T>) -> QueryAs<'a, T> {
        const MAX_PER_PAGE: u32 = 200;
        const DEFAULT_PER_PAGE: u32 = 50;
        let per_page = self
            .per_page
            .unwrap_or(DEFAULT_PER_PAGE)
            .min(MAX_PER_PAGE)
            .max(1);
        let offset = self.page.unwrap_or(0) * per_page;
        event!(Level::DEBUG, per_page, offset);
        query = query.bind(per_page as i32).bind(offset as i32);

        if !self.id.is_empty() {
            event!(Level::DEBUG, id = ?self.id);
            query = query.bind(&self.id);
        }

        if self.updated_at_lte.is_some() {
            event!(Level::DEBUG, updated_at_lte = ?self.updated_at_lte);
            query = query.bind(&self.updated_at_lte);
        }

        if self.updated_at_gte.is_some() {
            event!(Level::DEBUG, updated_at_gte = ?self.updated_at_gte);
            query = query.bind(&self.updated_at_gte);
        }

        if self.created_at_lte.is_some() {
            event!(Level::DEBUG, created_at_lte = ?self.created_at_lte);
            query = query.bind(&self.created_at_lte);
        }

        if self.created_at_gte.is_some() {
            event!(Level::DEBUG, created_at_gte = ?self.created_at_gte);
            query = query.bind(&self.created_at_gte);
        }

        query
    }
}

#[instrument(skip(db))]
pub async fn list(
    db: impl PgExecutor<'_>,
    auth: &AuthInfo,
    filters: &ListQueryFilters,
) -> Result<Vec<CustomProviderListResult>, error_stack::Report<Error>> {
    let q = include_str!("list.sql");
    list_internal(q, db, auth, filters).await
}

async fn list_internal<T>(
    query_template: &str,
    db: impl PgExecutor<'_>,
    auth: &AuthInfo,
    filters: &ListQueryFilters,
) -> Result<Vec<T>, error_stack::Report<Error>>
where
    T: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
{
    let (descending, order_by_field) =
        parse_order_by(filters.order_by.as_deref().unwrap_or("-updated_at"))
            .change_context(Error::Filter)?;
    let order_direction = if descending { "DESC" } else { "ASC" };

    let q = query_template.replace(
        "__insertion_point_order_by",
        &format!("{} {}", order_by_field.as_str(), order_direction),
    );

    let q = q.replace("__insertion_point_filters", &filters.build_where_clause());

    let mut query = sqlx::query_as::<_, T>(q.as_str());

    let actor_ids = auth.actor_ids();
    event!(Level::DEBUG, organization_id=%auth.organization_id, actor_ids=?actor_ids);
    query = query.bind(&auth.organization_id).bind(&actor_ids);

    query = filters.bind_to_query(query);

    let results = query.fetch_all(db).await.change_context(Error::Db)?;

    Ok(results)
}

/// Create a new CustomProvider in the database.
pub async fn create(
    db: &mut PgConnection,
    auth: &AuthInfo,
    payload: CustomProviderCreatePayload,
) -> Result<CustomProviderCreateResult, error_stack::Report<Error>> {
    // TODO create permissions auth check

    let id = CustomProviderId::new();

    create_raw(&mut *db, id, auth.organization_id, payload).await
}

/// Create a new CustomProvider in the database, allowing the ID to be explicitly specified
/// regardless of whether it would normally be allowed.
#[instrument(skip(db))]
pub async fn create_raw(
    db: &mut PgConnection,
    id: CustomProviderId,
    organization_id: OrganizationId,
    payload: CustomProviderCreatePayload,
) -> Result<CustomProviderCreateResult, error_stack::Report<Error>> {
    let result = query_file_as!(
        CustomProvider,
        "src/models/custom_provider/insert.sql",
        id.as_uuid(),
        organization_id.as_uuid(),
        &payload.name,
        payload.label.as_ref(),
        &payload.url,
        payload.token.as_ref(),
        payload.api_key.as_ref(),
        &payload.api_key_source,
        sqlx::types::Json(&payload.format) as _,
        sqlx::types::Json(payload.headers.as_ref()) as _,
        payload.prefix.as_ref(),
    )
    .fetch_one(&mut *db)
    .await
    .change_context(Error::Db)?;

    Ok(result)
}

#[instrument(skip(db))]
pub async fn update(
    db: &mut PgConnection,
    auth: &AuthInfo,
    id: CustomProviderId,
    payload: CustomProviderUpdatePayload,
) -> Result<bool, error_stack::Report<Error>> {
    let actor_ids = auth.actor_ids();
    let result = query_file_scalar!(
        "src/models/custom_provider/update.sql",
        id.as_uuid(),
        auth.organization_id.as_uuid(),
        &actor_ids,
        &payload.name as _,
        payload.label.as_ref() as _,
        &payload.url as _,
        payload.token.as_ref() as _,
        payload.api_key.as_ref() as _,
        &payload.api_key_source as _,
        sqlx::types::Json(&payload.format) as _,
        sqlx::types::Json(payload.headers.as_ref()) as _,
        payload.prefix.as_ref() as _,
    )
    .fetch_optional(&mut *db)
    .await
    .change_context(Error::Db)?;

    let Some(is_owner) = result else {
        return Ok(false);
    };

    Ok(true)
}

#[instrument(skip(db))]
pub async fn delete(
    db: impl PgExecutor<'_>,
    auth: &AuthInfo,
    id: CustomProviderId,
) -> Result<bool, error_stack::Report<Error>> {
    let actor_ids = auth.actor_ids();
    let result = query_file!(
        "src/models/custom_provider/delete.sql",
        id.as_uuid(),
        auth.organization_id.as_uuid(),
        &actor_ids
    )
    .execute(db)
    .await
    .change_context(Error::Db)?;
    Ok(result.rows_affected() > 0)
}

#[instrument(skip(db))]
pub async fn lookup_object_permissions(
    db: impl PgExecutor<'_>,
    auth: &AuthInfo,
    #[allow(unused_variables)] id: CustomProviderId,
) -> Result<Option<ObjectPermission>, error_stack::Report<Error>> {
    let actor_ids = auth.actor_ids();
    let result = query_file_scalar!(
        "src/models/custom_provider/lookup_object_permissions.sql",
        auth.organization_id.as_uuid(),
        &actor_ids,
    )
    .fetch_one(db)
    .await
    .change_context(Error::Db)?;

    let perm = result.and_then(|r| ObjectPermission::from_str_infallible(&r));
    Ok(perm)
}
