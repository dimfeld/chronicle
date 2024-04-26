pub mod endpoints;
pub mod queries;
#[cfg(test)]
pub mod testing;
pub mod types;

pub use types::*;

pub const READ_PERMISSION: &str = "Alias::read";
pub const WRITE_PERMISSION: &str = "Alias::write";
pub const OWNER_PERMISSION: &str = "Alias::owner";

pub const CREATE_PERMISSION: &str = "Alias::owner";

filigree::make_object_id!(AliasId, ali);
