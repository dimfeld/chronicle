use error_stack::ResultExt;
pub use filigree::users::organization::*;
use filigree::users::roles::{add_permissions_to_role, add_roles_to_user};
use sqlx::PgConnection;

use crate::{
    models::{
        organization::{self, Organization, OrganizationCreatePayload, OrganizationId},
        role::{self, RoleId},
        user::UserId,
    },
    Error,
};

const ADMIN_DEFAULT_PERMISSIONS: &[&str] = &["org_admin"];
const USER_DEFAULT_PERMISSIONS: &[&str] = &[
    "User::read",
    "User::write",
    "Organization::read",
    "Organization::write",
    "Role::read",
    "Role::write",
];

pub struct CreatedOrganization {
    pub organization: Organization,
    pub admin_role: RoleId,
    pub user_role: RoleId,
}

/// Creates a new organization containing the specified user. The user doesn't
/// actually have to exist yet, but it is assumed that the user will be created within
/// the current transaction if it hasn't yet been created.
pub async fn create_new_organization(
    db: &mut PgConnection,
    name: String,
    owner: UserId,
) -> Result<CreatedOrganization, error_stack::Report<Error>> {
    // The user might not be created yet, so defer foreign key enforcement until the
    // transaction is committed.
    sqlx::query!("SET CONSTRAINTS ALL DEFERRED")
        .execute(&mut *db)
        .await
        .change_context(Error::Db)?;

    let admin_role_id = role::RoleId::new();
    let user_role_id = role::RoleId::new();

    let org_id = OrganizationId::new();
    let new_org = OrganizationCreatePayload {
        name,
        owner: Some(owner),
        default_role: Some(user_role_id),
        ..Default::default()
    };

    let new_org = organization::queries::create_raw(&mut *db, org_id, org_id, new_org).await?;

    add_user_to_organization(&mut *db, org_id, owner)
        .await
        .change_context(Error::Db)?;

    let admin_role = role::RoleCreatePayload {
        id: None,
        name: "Admin".to_string(),
        description: None,
    };

    let user_role = role::RoleCreatePayload {
        id: None,
        name: "User".to_string(),
        description: None,
    };

    role::queries::create_raw(&mut *db, admin_role_id, org_id, admin_role).await?;
    role::queries::create_raw(&mut *db, user_role_id, org_id, user_role).await?;
    add_roles_to_user(&mut *db, org_id, owner, &[admin_role_id, user_role_id])
        .await
        .change_context(Error::Db)?;

    let admin_permissions = ADMIN_DEFAULT_PERMISSIONS
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>();
    let user_permissions = USER_DEFAULT_PERMISSIONS
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>();

    add_permissions_to_role(&mut *db, org_id, admin_role_id, &admin_permissions)
        .await
        .change_context(Error::Db)?;
    add_permissions_to_role(&mut *db, org_id, user_role_id, &user_permissions)
        .await
        .change_context(Error::Db)?;

    Ok(CreatedOrganization {
        organization: new_org,
        admin_role: admin_role_id,
        user_role: user_role_id,
    })
}
