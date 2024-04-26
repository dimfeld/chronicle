WITH permissions AS (
  SELECT
    COALESCE(bool_or(permission IN ('org_admin', 'CustomProvider::owner')), FALSE) AS is_owner,
    COALESCE(bool_or(permission IN ('org_admin', 'CustomProvider::owner', 'CustomProvider::write')), FALSE) AS is_user
  FROM
    permissions
  WHERE
    organization_id = $2
    AND actor_id = ANY ($3)
    AND permission IN ('org_admin', 'CustomProvider::owner', 'CustomProvider::write'))
UPDATE
  custom_providers
SET
  name = $4,
  label = $5,
  url = $6,
  token = $7,
  api_key = $8,
  api_key_source = $9,
  format = $10,
  headers = $11,
  prefix = $12,
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
