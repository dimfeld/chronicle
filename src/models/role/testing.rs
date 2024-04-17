#![allow(unused_imports, unused_variables, dead_code)]
use super::{RoleCreatePayload, RoleId, RoleUpdatePayload};

/// Generate a RoleCreatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_create_payload(i: usize) -> RoleCreatePayload {
    RoleCreatePayload {
        id: None,
        name: format!("Test object {i}"),
        description: (i > 1).then(|| format!("Test object {i}")),
    }
}

/// Generate a RoleUpdatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_update_payload(i: usize) -> RoleUpdatePayload {
    RoleUpdatePayload {
        id: None,
        name: format!("Test object {i}"),
        description: Some(format!("Test object {i}")),
    }
}
