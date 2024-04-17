INSERT INTO users (
  id,
  organization_id,
  password_hash,
  name,
  email,
  avatar_url)
VALUES (
  $1,
  $2,
  $3,
  $4,
  $5,
  $6)
RETURNING
  id AS "id: UserId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  email,
  avatar_url,
  'owner' AS "_permission!: filigree::auth::ObjectPermission"
