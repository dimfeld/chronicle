pub mod endpoints;
pub mod queries;
#[cfg(test)]
pub mod testing;
pub mod types;

pub use types::*;

pub const READ_PERMISSION: &str = "Role::read";
pub const WRITE_PERMISSION: &str = "Role::write";
pub const OWNER_PERMISSION: &str = "Role::owner";

pub const CREATE_PERMISSION: &str = "Role::owner";

pub type RoleId = filigree::auth::RoleId;
