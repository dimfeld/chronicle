#![allow(unused_imports, dead_code)]
use super::AliasId;
use crate::models::alias_model::AliasModel;
use crate::models::alias_model::AliasModelCreatePayload;
use crate::models::alias_model::AliasModelId;
use crate::models::alias_model::AliasModelUpdatePayload;
use crate::models::organization::OrganizationId;
use filigree::auth::ObjectPermission;
use serde::{
    ser::{SerializeStruct, Serializer},
    Deserialize, Serialize,
};
use sqlx_transparent_json_decode::sqlx_json_decode;

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]

pub struct Alias {
    pub id: AliasId,
    pub organization_id: crate::models::organization::OrganizationId,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub name: String,
    pub random_order: bool,
    pub _permission: ObjectPermission,
}

pub type AliasListResult = Alias;

pub type AliasPopulatedListResult = Alias;

pub type AliasCreateResult = Alias;

impl Alias {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> AliasId {
        <AliasId as Default>::default().into()
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

    pub fn default_name() -> String {
        <String as Default>::default().into()
    }

    pub fn default_random_order() -> bool {
        <bool as Default>::default().into()
    }
}

sqlx_json_decode!(Alias);

impl Default for Alias {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            organization_id: Self::default_organization_id(),
            updated_at: Self::default_updated_at(),
            created_at: Self::default_created_at(),
            name: Self::default_name(),
            random_order: Self::default_random_order(),
            _permission: ObjectPermission::Owner,
        }
    }
}

impl Serialize for Alias {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Alias", 7)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("organization_id", &self.organization_id)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("random_order", &self.random_order)?;
        state.serialize_field("_permission", &self._permission)?;
        state.end()
    }
}

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]
#[cfg_attr(test, derive(Serialize))]
pub struct AliasCreatePayloadAndUpdatePayload {
    pub id: Option<AliasId>,
    pub name: String,
    pub random_order: bool,
}

pub type AliasCreatePayload = AliasCreatePayloadAndUpdatePayload;

pub type AliasUpdatePayload = AliasCreatePayloadAndUpdatePayload;

impl AliasCreatePayloadAndUpdatePayload {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> Option<AliasId> {
        None
    }

    pub fn default_name() -> String {
        <String as Default>::default().into()
    }

    pub fn default_random_order() -> bool {
        <bool as Default>::default().into()
    }
}

impl Default for AliasCreatePayloadAndUpdatePayload {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            name: Self::default_name(),
            random_order: Self::default_random_order(),
        }
    }
}

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]

pub struct AliasPopulatedGetResult {
    pub id: AliasId,
    pub organization_id: crate::models::organization::OrganizationId,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub name: String,
    pub random_order: bool,
    pub models: Vec<AliasModel>,
    pub _permission: ObjectPermission,
}

impl AliasPopulatedGetResult {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> AliasId {
        <AliasId as Default>::default().into()
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

    pub fn default_name() -> String {
        <String as Default>::default().into()
    }

    pub fn default_random_order() -> bool {
        <bool as Default>::default().into()
    }

    pub fn default_models() -> Vec<AliasModel> {
        <Vec<AliasModel> as Default>::default().into()
    }
}

sqlx_json_decode!(AliasPopulatedGetResult);

impl Default for AliasPopulatedGetResult {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            organization_id: Self::default_organization_id(),
            updated_at: Self::default_updated_at(),
            created_at: Self::default_created_at(),
            name: Self::default_name(),
            random_order: Self::default_random_order(),
            models: Self::default_models(),
            _permission: ObjectPermission::Owner,
        }
    }
}

impl Serialize for AliasPopulatedGetResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AliasPopulatedGetResult", 8)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("organization_id", &self.organization_id)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("random_order", &self.random_order)?;
        state.serialize_field("models", &self.models)?;
        state.serialize_field("_permission", &self._permission)?;
        state.end()
    }
}
