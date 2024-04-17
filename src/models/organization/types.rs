#![allow(unused_imports, dead_code)]
use super::OrganizationId;
use filigree::auth::ObjectPermission;
use serde::{
    ser::{SerializeStruct, Serializer},
    Deserialize, Serialize,
};
use sqlx_transparent_json_decode::sqlx_json_decode;

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]

pub struct Organization {
    pub id: OrganizationId,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub name: String,
    pub owner: Option<crate::models::user::UserId>,
    pub default_role: Option<crate::models::role::RoleId>,
    pub active: bool,
    pub _permission: ObjectPermission,
}

pub type OrganizationListResult = Organization;

pub type OrganizationPopulatedGetResult = Organization;

pub type OrganizationPopulatedListResult = Organization;

pub type OrganizationCreateResult = Organization;

impl Organization {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> OrganizationId {
        <OrganizationId as Default>::default().into()
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

    pub fn default_owner() -> Option<crate::models::user::UserId> {
        None
    }

    pub fn default_default_role() -> Option<crate::models::role::RoleId> {
        None
    }

    pub fn default_active() -> bool {
        <bool as Default>::default().into()
    }
}

sqlx_json_decode!(Organization);

impl Default for Organization {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            updated_at: Self::default_updated_at(),
            created_at: Self::default_created_at(),
            name: Self::default_name(),
            owner: Self::default_owner(),
            default_role: Self::default_default_role(),
            active: Self::default_active(),
            _permission: ObjectPermission::Owner,
        }
    }
}

impl Serialize for Organization {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self._permission == ObjectPermission::Owner {
            let mut state = serializer.serialize_struct("Organization", 7)?;
            state.serialize_field("id", &self.id)?;
            state.serialize_field("updated_at", &self.updated_at)?;
            state.serialize_field("created_at", &self.created_at)?;
            state.serialize_field("name", &self.name)?;
            state.serialize_field("owner", &self.owner)?;
            state.serialize_field("default_role", &self.default_role)?;
            state.serialize_field("_permission", &self._permission)?;
            state.end()
        } else {
            let mut state = serializer.serialize_struct("Organization", 5)?;
            state.serialize_field("id", &self.id)?;
            state.serialize_field("updated_at", &self.updated_at)?;
            state.serialize_field("created_at", &self.created_at)?;
            state.serialize_field("name", &self.name)?;
            state.serialize_field("_permission", &self._permission)?;
            state.end()
        }
    }
}

#[derive(Deserialize, Debug, Clone, schemars::JsonSchema, sqlx::FromRow)]
#[cfg_attr(test, derive(Serialize))]
pub struct OrganizationCreatePayloadAndUpdatePayload {
    pub id: Option<OrganizationId>,
    pub name: String,
    pub owner: Option<crate::models::user::UserId>,
    pub default_role: Option<crate::models::role::RoleId>,
}

pub type OrganizationCreatePayload = OrganizationCreatePayloadAndUpdatePayload;

pub type OrganizationUpdatePayload = OrganizationCreatePayloadAndUpdatePayload;

impl OrganizationCreatePayloadAndUpdatePayload {
    // The <T as Default> syntax here is weird but lets us generate from the template without needing to
    // detect whether to add the extra :: in cases like DateTime::<Utc>::default

    pub fn default_id() -> Option<OrganizationId> {
        None
    }

    pub fn default_name() -> String {
        <String as Default>::default().into()
    }

    pub fn default_owner() -> Option<crate::models::user::UserId> {
        None
    }

    pub fn default_default_role() -> Option<crate::models::role::RoleId> {
        None
    }
}

impl Default for OrganizationCreatePayloadAndUpdatePayload {
    fn default() -> Self {
        Self {
            id: Self::default_id(),
            name: Self::default_name(),
            owner: Self::default_owner(),
            default_role: Self::default_default_role(),
        }
    }
}
