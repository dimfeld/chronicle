pub mod endpoints;
pub mod queries;
#[cfg(test)]
pub mod testing;
pub mod types;

pub use types::*;

pub const READ_PERMISSION: &str = "ProviderApiKey::read";
pub const WRITE_PERMISSION: &str = "ProviderApiKey::write";
pub const OWNER_PERMISSION: &str = "ProviderApiKey::owner";

pub const CREATE_PERMISSION: &str = "ProviderApiKey::owner";

filigree::make_object_id!(ProviderApiKeyId, pro);
