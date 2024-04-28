#![allow(unused_imports, dead_code)]
use super::AliasModelId;
use crate::models::alias::AliasId;
use crate::models::organization::OrganizationId;
use filigree::auth::ObjectPermission;
use serde::{
    ser::{SerializeStruct, Serializer},
    Deserialize, Serialize,
};
use sqlx_transparent_json_decode::sqlx_json_decode;

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]

pub struct AliasModel {
    pub id: AliasModelId,
    pub organization_id: crate::models::organization::OrganizationId,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub model: String,
    pub provider: String,
    pub api_key_name: Option<String>,
    pub sort: i32,
    pub alias_id: AliasId,
    pub _permission: ObjectPermission,
}

pub type AliasModelListResult = AliasModel;

pub type AliasModelPopulatedGetResult = AliasModel;

pub type AliasModelPopulatedListResult = AliasModel;

pub type AliasModelCreateResult = AliasModel;

impl AliasModel {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> AliasModelId {
        <AliasModelId as Default>::default().into()
    }

    pub fn default_organization_id() -> crate::models::organization::OrganizationId {
        <crate::models::organization::OrganizationId as Default>::default().into()
    }

    pub fn default_updated_at() -> chrono::DateTime<chrono::Utc> {
        <chrono::DateTime<chrono::Utc> as Default>::default().into()
    }

    pub fn default_created_at() -> chrono::DateTime<chrono::Utc> {
        <chrono::DateTime<chrono::Utc> as Default>::default().into()
    }

    pub fn default_model() -> String {
        <String as Default>::default().into()
    }

    pub fn default_provider() -> String {
        <String as Default>::default().into()
    }

    pub fn default_api_key_name() -> Option<String> {
        None
    }

    pub fn default_sort() -> i32 {
        <i32 as Default>::default().into()
    }

    pub fn default_alias_id() -> AliasId {
        <AliasId as Default>::default().into()
    }
}

sqlx_json_decode!(AliasModel);

impl Default for AliasModel {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            organization_id: Self::default_organization_id(),
            updated_at: Self::default_updated_at(),
            created_at: Self::default_created_at(),
            model: Self::default_model(),
            provider: Self::default_provider(),
            api_key_name: Self::default_api_key_name(),
            sort: Self::default_sort(),
            alias_id: Self::default_alias_id(),
            _permission: ObjectPermission::Owner,
        }
    }
}

impl Serialize for AliasModel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AliasModel", 10)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("organization_id", &self.organization_id)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("model", &self.model)?;
        state.serialize_field("provider", &self.provider)?;
        state.serialize_field("api_key_name", &self.api_key_name)?;
        state.serialize_field("sort", &self.sort)?;
        state.serialize_field("alias_id", &self.alias_id)?;
        state.serialize_field("_permission", &self._permission)?;
        state.end()
    }
}

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]
#[cfg_attr(test, derive(Serialize))]
pub struct AliasModelCreatePayloadAndUpdatePayload {
    pub id: Option<AliasModelId>,
    pub model: String,
    pub provider: String,
    pub api_key_name: Option<String>,
    pub sort: i32,
    pub alias_id: AliasId,
}

pub type AliasModelCreatePayload = AliasModelCreatePayloadAndUpdatePayload;

pub type AliasModelUpdatePayload = AliasModelCreatePayloadAndUpdatePayload;

impl AliasModelCreatePayloadAndUpdatePayload {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> Option<AliasModelId> {
        None
    }

    pub fn default_model() -> String {
        <String as Default>::default().into()
    }

    pub fn default_provider() -> String {
        <String as Default>::default().into()
    }

    pub fn default_api_key_name() -> Option<String> {
        None
    }

    pub fn default_sort() -> i32 {
        <i32 as Default>::default().into()
    }

    pub fn default_alias_id() -> AliasId {
        <AliasId as Default>::default().into()
    }
}

impl Default for AliasModelCreatePayloadAndUpdatePayload {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            model: Self::default_model(),
            provider: Self::default_provider(),
            api_key_name: Self::default_api_key_name(),
            sort: Self::default_sort(),
            alias_id: Self::default_alias_id(),
        }
    }
}
