#![allow(unused_imports, unused_variables, dead_code)]
use chronicle_proxy::providers::custom::{OpenAiRequestFormatOptions, ProviderRequestFormat};

use super::{CustomProviderCreatePayload, CustomProviderId, CustomProviderUpdatePayload};

/// Generate a CustomProviderCreatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_create_payload(i: usize) -> CustomProviderCreatePayload {
    CustomProviderCreatePayload {
        id: None,
        name: format!("Test object {i}"),
        label: (i > 1).then(|| format!("Test object {i}")),
        url: format!("Test object {i}"),
        token: (i > 1).then(|| format!("Test object {i}")),
        api_key: (i > 1).then(|| format!("Test object {i}")),
        api_key_source: format!("Test object {i}"),
        format: ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions::default()),
        headers: (i > 1).then(|| serde_json::json!({ "key": i })),
        prefix: (i > 1).then(|| format!("Test object {i}")),
    }
}

/// Generate a CustomProviderUpdatePayload for testing.
/// Parameter `i` controls the value of some of the fields, just to make sure that the objects
/// don't all look identical.
pub fn make_update_payload(i: usize) -> CustomProviderUpdatePayload {
    CustomProviderUpdatePayload {
        id: None,
        name: format!("Test object {i}"),
        label: Some(format!("Test object {i}")),
        url: format!("Test object {i}"),
        token: Some(format!("Test object {i}")),
        api_key: Some(format!("Test object {i}")),
        api_key_source: format!("Test object {i}"),
        format: ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions::default()),
        headers: Some(serde_json::json!({ "key": i })),
        prefix: Some(format!("Test object {i}")),
    }
}
