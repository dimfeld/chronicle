#![allow(unused_imports, dead_code)]
use super::UserId;
use crate::models::organization::OrganizationId;

use filigree::auth::ObjectPermission;
use serde::{
    ser::{SerializeStruct, Serializer},
    Deserialize, Serialize,
};
use sqlx_transparent_json_decode::sqlx_json_decode;

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]

pub struct User {
    pub id: UserId,
    pub organization_id: Option<crate::models::organization::OrganizationId>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub _permission: ObjectPermission,
}

pub type UserListResult = User;

pub type UserPopulatedGetResult = User;

pub type UserPopulatedListResult = User;

pub type UserCreateResult = User;

impl User {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> UserId {
        <UserId as Default>::default().into()
    }

    pub fn default_organization_id() -> Option<crate::models::organization::OrganizationId> {
        None
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

    pub fn default_email() -> Option<String> {
        None
    }

    pub fn default_avatar_url() -> Option<String> {
        None
    }
}

sqlx_json_decode!(User);

impl Default for User {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            organization_id: Self::default_organization_id(),
            updated_at: Self::default_updated_at(),
            created_at: Self::default_created_at(),
            name: Self::default_name(),
            email: Self::default_email(),
            avatar_url: Self::default_avatar_url(),
            _permission: ObjectPermission::Owner,
        }
    }
}

impl Serialize for User {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("User", 8)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("organization_id", &self.organization_id)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("email", &self.email)?;
        state.serialize_field("avatar_url", &self.avatar_url)?;
        state.serialize_field("_permission", &self._permission)?;
        state.end()
    }
}

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]
#[cfg_attr(test, derive(Serialize))]
pub struct UserCreatePayloadAndUpdatePayload {
    pub id: Option<UserId>,
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

pub type UserCreatePayload = UserCreatePayloadAndUpdatePayload;

pub type UserUpdatePayload = UserCreatePayloadAndUpdatePayload;

impl UserCreatePayloadAndUpdatePayload {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> Option<UserId> {
        None
    }

    pub fn default_name() -> String {
        <String as Default>::default().into()
    }

    pub fn default_email() -> Option<String> {
        None
    }

    pub fn default_avatar_url() -> Option<String> {
        None
    }
}

impl Default for UserCreatePayloadAndUpdatePayload {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            name: Self::default_name(),
            email: Self::default_email(),
            avatar_url: Self::default_avatar_url(),
        }
    }
}
