#![allow(unused_imports, unused_variables, dead_code)]
use super::{AliasCreatePayload, AliasId, AliasUpdatePayload};
use crate::models::alias_model::AliasModel;
use crate::models::alias_model::AliasModelCreatePayload;
use crate::models::alias_model::AliasModelId;
use crate::models::alias_model::AliasModelUpdatePayload;

/// Generate a AliasCreatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_create_payload(i: usize) -> AliasCreatePayload {
    AliasCreatePayload {
        id: None,
        name: format!("Test object {i}"),
        random_order: i % 2 == 0,
    }
}

/// Generate a AliasUpdatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_update_payload(i: usize) -> AliasUpdatePayload {
    AliasUpdatePayload {
        id: None,
        name: format!("Test object {i}"),
        random_order: i % 2 == 0,
    }
}
