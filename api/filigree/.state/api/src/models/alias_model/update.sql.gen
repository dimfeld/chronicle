WITH permissions AS (
  SELECT
    COALESCE(bool_or(permission IN ('org_admin', 'AliasModel::owner')), FALSE) AS is_owner,
    COALESCE(bool_or(permission IN ('org_admin', 'AliasModel::owner', 'AliasModel::write')), FALSE) AS is_user
  FROM
    permissions
  WHERE
    organization_id = $2
    AND actor_id = ANY ($3)
    AND permission IN ('org_admin', 'AliasModel::owner', 'AliasModel::write'))
UPDATE
  alias_models
SET
  model = $4,
  provider = $5,
  api_key_name = $6,
  sort = $7,
  alias_id = $8,
  updated_at = now()
FROM
  permissions
WHERE
  id = $1
  AND organization_id = $2
  AND (permissions.is_owner
    OR permissions.is_user)
RETURNING
  permissions.is_owner AS "is_owner!"
