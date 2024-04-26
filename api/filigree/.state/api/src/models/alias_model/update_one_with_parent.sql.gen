WITH permissions AS (
  SELECT
    COALESCE(bool_or(permission IN ('org_admin', 'AliasModel::owner')), FALSE) AS is_owner,
    COALESCE(bool_or(permission IN ('org_admin', 'AliasModel::owner', 'AliasModel::write')), FALSE) AS is_user
  FROM
    permissions
  WHERE
    organization_id = $3
    AND actor_id = ANY ($4)
    AND permission IN ('org_admin', 'AliasModel::owner', 'AliasModel::write'))
UPDATE
  alias_models
SET
  model = $5,
  provider = $6,
  api_key_name = $7,
  updated_at = now()
FROM
  permissions
WHERE
  id = $1
  AND alias_id = $2
  AND organization_id = $3
  AND (permissions.is_owner
    OR permissions.is_user)
