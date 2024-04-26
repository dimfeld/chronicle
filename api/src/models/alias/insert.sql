INSERT INTO aliases (
  id,
  organization_id,
  name,
  random_order)
VALUES (
  $1,
  $2,
  $3,
  $4)
RETURNING
  id AS "id: AliasId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  random_order,
  'owner' AS "_permission!: filigree::auth::ObjectPermission"
