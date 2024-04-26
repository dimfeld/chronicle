INSERT INTO alias_models (
  id,
  organization_id,
  model,
  provider,
  api_key_name,
  alias_id)
VALUES (
  $1,
  $2,
  $3,
  $4,
  $5,
  $6)
RETURNING
  id AS "id: AliasModelId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  model,
  provider,
  api_key_name,
  alias_id AS "alias_id: AliasId",
  'owner' AS "_permission!: filigree::auth::ObjectPermission"
