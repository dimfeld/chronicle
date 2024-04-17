INSERT INTO roles (
  id,
  organization_id,
  name,
  description)
VALUES (
  $1,
  $2,
  $3,
  $4)
RETURNING
  id AS "id: RoleId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  description,
  'owner' AS "_permission!: filigree::auth::ObjectPermission"
