#![allow(unused_imports, unused_variables, dead_code)]
use super::{ProviderApiKeyCreatePayload, ProviderApiKeyId, ProviderApiKeyUpdatePayload};

/// Generate a ProviderApiKeyCreatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_create_payload(i: usize) -> ProviderApiKeyCreatePayload {
    ProviderApiKeyCreatePayload {
        id: None,
        name: format!("Test object {i}"),
        source: format!("Test object {i}"),
        value: format!("Test object {i}"),
    }
}

/// Generate a ProviderApiKeyUpdatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_update_payload(i: usize) -> ProviderApiKeyUpdatePayload {
    ProviderApiKeyUpdatePayload {
        id: None,
        name: format!("Test object {i}"),
        source: format!("Test object {i}"),
        value: format!("Test object {i}"),
    }
}
