use error_stack::{Report, ResultExt};
use filigree::{
    auth::password::HashedPassword,
    users::{
        roles::{add_permissions_to_role, add_roles_to_user},
        users::add_user_email_login,
    },
};
use sqlx::{PgConnection, PgPool};

use crate::{
    models::{
        organization::OrganizationId,
        role::{self, RoleId},
        user::{UserCreatePayload, UserId},
    },
    users::{
        organization::create_new_organization, users::create_new_user_with_prehashed_password,
    },
    Error,
};

/// Run the database migrations, if needed
pub async fn run_migrations(db: &PgPool) -> Result<(), Report<Error>> {
    sqlx::migrate!().run(db).await.change_context(Error::Db)
}

/// Arguments to the [boostrap] function.
#[derive(Debug, Default)]
pub struct BootstrapData {
    /// If false, don't do anything if the database already contains at least one organization.
    /// If true, try to add the admin user and organization regardless of what's in the database
    /// right now.
    pub force: bool,
    /// The email for the admin user
    pub admin_email: String,
    /// The name of the admin user, or "Admin" if omitted
    pub admin_name: Option<String>,
    /// The hashed password for the admin user. Can be omitted if you only want to do passwordless
    /// login methods.
    pub admin_password: Option<HashedPassword>,
    /// The name of the administrator's organization.
    pub organization_name: Option<String>,
}

/// Bootstrap the database, adding an administrator user and organization.
/// This users gets the special superuser role, which has a "_global:admin" permission.
pub async fn bootstrap(db: PgPool, data: BootstrapData) -> Result<bool, Report<Error>> {
    let mut tx = db.begin().await.unwrap();

    if !data.force {
        let any_exists = sqlx::query_scalar!("SELECT true FROM organizations LIMIT 1")
            .fetch_optional(&mut *tx)
            .await
            .change_context(Error::Db)?
            .is_some();

        if any_exists {
            return Ok(false);
        }
    }

    let admin_user_id = UserId::new();

    let org = create_new_organization(
        &mut *tx,
        data.organization_name
            .unwrap_or_else(|| "Administration".to_string()),
        admin_user_id,
    )
    .await?;

    let user_details = UserCreatePayload {
        name: data.admin_name.unwrap_or_else(|| "Admin".to_string()),
        email: Some(data.admin_email.clone()),
        ..Default::default()
    };

    create_new_user_with_prehashed_password(
        &mut *tx,
        admin_user_id,
        org.organization.id,
        user_details,
        data.admin_password,
    )
    .await?;

    add_user_email_login(&mut *tx, admin_user_id, data.admin_email, true)
        .await
        .change_context(Error::Db)?;

    let superuser_role = create_superuser_role(&mut *tx, org.organization.id).await?;

    add_roles_to_user(
        &mut *tx,
        org.organization.id,
        admin_user_id,
        &[
            org.admin_role,
            org.read_role,
            org.write_role,
            superuser_role,
        ],
    )
    .await
    .change_context(Error::Db)?;

    tx.commit().await.change_context(Error::Db)?;

    Ok(true)
}

async fn create_superuser_role(
    tx: &mut PgConnection,
    org_id: OrganizationId,
) -> Result<RoleId, Error> {
    let superuser_role_id = RoleId::new();
    let superuser_role = role::RoleCreatePayload {
        id: None,
        name: "Superuser".to_string(),
        description: None,
    };

    add_permissions_to_role(
        &mut *tx,
        org_id,
        superuser_role_id,
        &["_global:admin".to_string()],
    )
    .await
    .change_context(Error::Db)?;

    role::queries::create_raw(tx, superuser_role_id, org_id, superuser_role).await?;

    Ok(superuser_role_id)
}
