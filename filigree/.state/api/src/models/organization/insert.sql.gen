INSERT INTO organizations (
  id,
  name,
  OWNER,
  default_role)
VALUES (
  $1,
  $2,
  $3,
  $4)
RETURNING
  id AS "id: OrganizationId",
  updated_at,
  created_at,
  name,
  OWNER AS "owner: crate::models::user::UserId",
  default_role AS "default_role: crate::models::role::RoleId",
  active,
  'owner' AS "_permission!: filigree::auth::ObjectPermission"
