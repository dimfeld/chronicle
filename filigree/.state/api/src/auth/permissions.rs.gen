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
