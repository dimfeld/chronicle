INSERT INTO alias_models (
  id,
  organization_id,
  model,
  provider,
  api_key_name,
  sort,
  alias_id)
VALUES (
  $1,
  $2,
  $3,
  $4,
  $5,
  $6,
  $7)
ON CONFLICT (
  id)
  DO UPDATE SET
    model = EXCLUDED.model,
    provider = EXCLUDED.provider,
    api_key_name = EXCLUDED.api_key_name,
    sort = EXCLUDED.sort,
    alias_id = EXCLUDED.alias_id,
    updated_at = now()
  WHERE
    alias_models.organization_id = $2
    AND alias_models.alias_id = EXCLUDED.alias_id
  RETURNING
    id AS "id: AliasModelId",
    organization_id AS "organization_id: crate::models::organization::OrganizationId",
    updated_at,
    created_at,
    model,
    provider,
    api_key_name,
    sort,
    alias_id AS "alias_id: AliasId",
    'owner' AS "_permission!: filigree::auth::ObjectPermission"
