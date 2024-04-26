#![allow(unused_imports, dead_code)]
use super::ProviderApiKeyId;
use crate::models::organization::OrganizationId;

use filigree::auth::ObjectPermission;
use serde::{
    ser::{SerializeStruct, Serializer},
    Deserialize, Serialize,
};
use sqlx_transparent_json_decode::sqlx_json_decode;

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]

pub struct ProviderApiKey {
    pub id: ProviderApiKeyId,
    pub organization_id: crate::models::organization::OrganizationId,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub name: String,
    pub source: String,
    pub value: String,
    pub _permission: ObjectPermission,
}

pub type ProviderApiKeyListResult = ProviderApiKey;

pub type ProviderApiKeyPopulatedGetResult = ProviderApiKey;

pub type ProviderApiKeyPopulatedListResult = ProviderApiKey;

pub type ProviderApiKeyCreateResult = ProviderApiKey;

impl ProviderApiKey {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> ProviderApiKeyId {
        <ProviderApiKeyId as Default>::default().into()
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

    pub fn default_source() -> String {
        <String as Default>::default().into()
    }

    pub fn default_value() -> String {
        <String as Default>::default().into()
    }
}

sqlx_json_decode!(ProviderApiKey);

impl Default for ProviderApiKey {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            organization_id: Self::default_organization_id(),
            updated_at: Self::default_updated_at(),
            created_at: Self::default_created_at(),
            name: Self::default_name(),
            source: Self::default_source(),
            value: Self::default_value(),
            _permission: ObjectPermission::Owner,
        }
    }
}

impl Serialize for ProviderApiKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ProviderApiKey", 8)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("organization_id", &self.organization_id)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("source", &self.source)?;
        state.serialize_field("value", &self.value)?;
        state.serialize_field("_permission", &self._permission)?;
        state.end()
    }
}

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]
#[cfg_attr(test, derive(Serialize))]
pub struct ProviderApiKeyCreatePayloadAndUpdatePayload {
    pub id: Option<ProviderApiKeyId>,
    pub name: String,
    pub source: String,
    pub value: String,
}

pub type ProviderApiKeyCreatePayload = ProviderApiKeyCreatePayloadAndUpdatePayload;

pub type ProviderApiKeyUpdatePayload = ProviderApiKeyCreatePayloadAndUpdatePayload;

impl ProviderApiKeyCreatePayloadAndUpdatePayload {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> Option<ProviderApiKeyId> {
        None
    }

    pub fn default_name() -> String {
        <String as Default>::default().into()
    }

    pub fn default_source() -> String {
        <String as Default>::default().into()
    }

    pub fn default_value() -> String {
        <String as Default>::default().into()
    }
}

impl Default for ProviderApiKeyCreatePayloadAndUpdatePayload {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            name: Self::default_name(),
            source: Self::default_source(),
            value: Self::default_value(),
        }
    }
}
