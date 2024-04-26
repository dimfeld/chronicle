INSERT INTO provider_api_keys (
  id,
  organization_id,
  name,
  source,
  value)
VALUES (
  $1,
  $2,
  $3,
  $4,
  $5)
RETURNING
  id AS "id: ProviderApiKeyId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  source,
  value,
  'owner' AS "_permission!: filigree::auth::ObjectPermission"
