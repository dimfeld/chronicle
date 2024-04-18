#![allow(unused_imports, unused_variables, dead_code)]
use super::{UserCreatePayload, UserId, UserUpdatePayload};

/// Generate a UserCreatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_create_payload(i: usize) -> UserCreatePayload {
    UserCreatePayload {
        id: None,
        name: format!("Test object {i}"),
        email: (i > 1).then(|| format!("Test object {i}")),
        avatar_url: (i > 1).then(|| format!("Test object {i}")),
    }
}

/// Generate a UserUpdatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_update_payload(i: usize) -> UserUpdatePayload {
    UserUpdatePayload {
        id: None,
        name: format!("Test object {i}"),
        email: Some(format!("Test object {i}")),
        avatar_url: Some(format!("Test object {i}")),
    }
}
