#![allow(unused_imports, unused_variables, dead_code)]
use super::{AliasModelCreatePayload, AliasModelId, AliasModelUpdatePayload};
use crate::models::alias::AliasId;

/// Generate a AliasModelCreatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_create_payload(i: usize) -> AliasModelCreatePayload {
    AliasModelCreatePayload {
        id: None,
        model: format!("Test object {i}"),
        provider: format!("Test object {i}"),
        api_key_name: (i > 1).then(|| format!("Test object {i}")),
        alias_id: <AliasId as Default>::default(),
    }
}

/// Generate a AliasModelUpdatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_update_payload(i: usize) -> AliasModelUpdatePayload {
    AliasModelUpdatePayload {
        id: None,
        model: format!("Test object {i}"),
        provider: format!("Test object {i}"),
        api_key_name: Some(format!("Test object {i}")),
        alias_id: <AliasId as Default>::default(),
    }
}
