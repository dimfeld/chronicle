pub mod endpoints;
pub mod queries;
#[cfg(test)]
pub mod testing;
pub mod types;

pub use types::*;

pub const READ_PERMISSION: &str = "CustomProvider::read";
pub const WRITE_PERMISSION: &str = "CustomProvider::write";
pub const OWNER_PERMISSION: &str = "CustomProvider::owner";

pub const CREATE_PERMISSION: &str = "CustomProvider::owner";

filigree::make_object_id!(CustomProviderId, cus);
