#![allow(unused_imports, dead_code)]
use chronicle_proxy::providers::custom::ProviderRequestFormat;
use filigree::auth::ObjectPermission;
use serde::{
    ser::{SerializeStruct, Serializer},
    Deserialize, Serialize,
};
use sqlx_transparent_json_decode::sqlx_json_decode;

use super::CustomProviderId;
use crate::models::organization::OrganizationId;

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]

pub struct CustomProvider {
    pub id: CustomProviderId,
    pub organization_id: crate::models::organization::OrganizationId,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub name: String,
    pub label: Option<String>,
    pub url: String,
    pub token: Option<String>,
    pub api_key: Option<String>,
    pub api_key_source: String,
    pub format: ProviderRequestFormat,
    pub headers: Option<serde_json::Value>,
    pub prefix: Option<String>,
    pub _permission: ObjectPermission,
}

pub type CustomProviderListResult = CustomProvider;

pub type CustomProviderPopulatedGetResult = CustomProvider;

pub type CustomProviderPopulatedListResult = CustomProvider;

pub type CustomProviderCreateResult = CustomProvider;

impl CustomProvider {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> CustomProviderId {
        <CustomProviderId as Default>::default().into()
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

    pub fn default_label() -> Option<String> {
        None
    }

    pub fn default_url() -> String {
        <String as Default>::default().into()
    }

    pub fn default_token() -> Option<String> {
        None
    }

    pub fn default_api_key() -> Option<String> {
        None
    }

    pub fn default_api_key_source() -> String {
        <String as Default>::default().into()
    }

    pub fn default_format() -> ProviderRequestFormat {
        <ProviderRequestFormat as Default>::default().into()
    }

    pub fn default_headers() -> Option<serde_json::Value> {
        None
    }

    pub fn default_prefix() -> Option<String> {
        None
    }
}

sqlx_json_decode!(CustomProvider);

impl Default for CustomProvider {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            organization_id: Self::default_organization_id(),
            updated_at: Self::default_updated_at(),
            created_at: Self::default_created_at(),
            name: Self::default_name(),
            label: Self::default_label(),
            url: Self::default_url(),
            token: Self::default_token(),
            api_key: Self::default_api_key(),
            api_key_source: Self::default_api_key_source(),
            format: Self::default_format(),
            headers: Self::default_headers(),
            prefix: Self::default_prefix(),
            _permission: ObjectPermission::Owner,
        }
    }
}

impl Serialize for CustomProvider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("CustomProvider", 14)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("organization_id", &self.organization_id)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("label", &self.label)?;
        state.serialize_field("url", &self.url)?;
        state.serialize_field("token", &self.token)?;
        state.serialize_field("api_key", &self.api_key)?;
        state.serialize_field("api_key_source", &self.api_key_source)?;
        state.serialize_field("format", &self.format)?;
        state.serialize_field("headers", &self.headers)?;
        state.serialize_field("prefix", &self.prefix)?;
        state.serialize_field("_permission", &self._permission)?;
        state.end()
    }
}

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]
#[cfg_attr(test, derive(Serialize))]
pub struct CustomProviderCreatePayloadAndUpdatePayload {
    pub id: Option<CustomProviderId>,
    pub name: String,
    pub label: Option<String>,
    pub url: String,
    pub token: Option<String>,
    pub api_key: Option<String>,
    pub api_key_source: String,
    pub format: ProviderRequestFormat,
    pub headers: Option<serde_json::Value>,
    pub prefix: Option<String>,
}

pub type CustomProviderCreatePayload = CustomProviderCreatePayloadAndUpdatePayload;

pub type CustomProviderUpdatePayload = CustomProviderCreatePayloadAndUpdatePayload;

impl CustomProviderCreatePayloadAndUpdatePayload {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> Option<CustomProviderId> {
        None
    }

    pub fn default_name() -> String {
        <String as Default>::default().into()
    }

    pub fn default_label() -> Option<String> {
        None
    }

    pub fn default_url() -> String {
        <String as Default>::default().into()
    }

    pub fn default_token() -> Option<String> {
        None
    }

    pub fn default_api_key() -> Option<String> {
        None
    }

    pub fn default_api_key_source() -> String {
        <String as Default>::default().into()
    }

    pub fn default_format() -> ProviderRequestFormat {
        <ProviderRequestFormat as Default>::default().into()
    }

    pub fn default_headers() -> Option<serde_json::Value> {
        None
    }

    pub fn default_prefix() -> Option<String> {
        None
    }
}

impl Default for CustomProviderCreatePayloadAndUpdatePayload {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            name: Self::default_name(),
            label: Self::default_label(),
            url: Self::default_url(),
            token: Self::default_token(),
            api_key: Self::default_api_key(),
            api_key_source: Self::default_api_key_source(),
            format: Self::default_format(),
            headers: Self::default_headers(),
            prefix: Self::default_prefix(),
        }
    }
}
