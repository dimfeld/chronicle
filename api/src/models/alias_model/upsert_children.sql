-- $1 = has owner permission on the model
-- $2 = organization_id
-- $3 = parent_id
INSERT INTO alias_models (
  id,
  organization_id,
  model,
  provider,
  api_key_name,
  sort,
  alias_id)
VALUES
  __insertion_point_insert_values
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
    AND alias_models.alias_id = $3
  RETURNING
    id,
    organization_id,
    updated_at,
    created_at,
    model,
    provider,
    api_key_name,
    sort,
    alias_id,
    'owner' AS "_permission"
