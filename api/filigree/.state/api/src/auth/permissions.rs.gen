use axum::{response::IntoResponse, Json};
use serde::Serialize;

use crate::auth::Authed;

#[derive(Debug, Serialize)]
pub struct PermissionInfo {
    name: &'static str,
    description: &'static str,
    key: &'static str,
}

pub const PERMISSIONS: &[PermissionInfo] = &[
    PermissionInfo {
        name: "Read AliasModels",
        description: "List and read AliasModel objects",
        key: "AliasModel::read",
    },
    PermissionInfo {
        name: "Write AliasModels",
        description: "Write AliasModel objects",
        key: "AliasModel::write",
    },
    PermissionInfo {
        name: "Administer AliasModels",
        description: "Create and delete AliasModel objects",
        key: "AliasModel::owner",
    },
    PermissionInfo {
        name: "Read Aliases",
        description: "List and read Alias objects",
        key: "Alias::read",
    },
    PermissionInfo {
        name: "Write Aliases",
        description: "Write Alias objects",
        key: "Alias::write",
    },
    PermissionInfo {
        name: "Administer Aliases",
        description: "Create and delete Alias objects",
        key: "Alias::owner",
    },
    PermissionInfo {
        name: "Read CustomProviders",
        description: "List and read CustomProvider objects",
        key: "CustomProvider::read",
    },
    PermissionInfo {
        name: "Write CustomProviders",
        description: "Write CustomProvider objects",
        key: "CustomProvider::write",
    },
    PermissionInfo {
        name: "Administer CustomProviders",
        description: "Create and delete CustomProvider objects",
        key: "CustomProvider::owner",
    },
    PermissionInfo {
        name: "Read Users",
        description: "List and read User objects",
        key: "User::read",
    },
    PermissionInfo {
        name: "Write Users",
        description: "Write User objects",
        key: "User::write",
    },
    PermissionInfo {
        name: "Administer Users",
        description: "Create and delete User objects",
        key: "User::owner",
    },
    PermissionInfo {
        name: "Read Organizations",
        description: "List and read Organization objects",
        key: "Organization::read",
    },
    PermissionInfo {
        name: "Write Organizations",
        description: "Write Organization objects",
        key: "Organization::write",
    },
    PermissionInfo {
        name: "Administer Organizations",
        description: "Create and delete Organization objects",
        key: "Organization::owner",
    },
    PermissionInfo {
        name: "Read ProviderApiKeys",
        description: "List and read ProviderApiKey objects",
        key: "ProviderApiKey::read",
    },
    PermissionInfo {
        name: "Write ProviderApiKeys",
        description: "Write ProviderApiKey objects",
        key: "ProviderApiKey::write",
    },
    PermissionInfo {
        name: "Administer ProviderApiKeys",
        description: "Create and delete ProviderApiKey objects",
        key: "ProviderApiKey::owner",
    },
    PermissionInfo {
        name: "Read Roles",
        description: "List and read Role objects",
        key: "Role::read",
    },
    PermissionInfo {
        name: "Write Roles",
        description: "Write Role objects",
        key: "Role::write",
    },
    PermissionInfo {
        name: "Administer Roles",
        description: "Create and delete Role objects",
        key: "Role::owner",
    },
];

pub async fn list_permissions(_authed: Authed) -> impl IntoResponse {
    Json(PERMISSIONS)
}
