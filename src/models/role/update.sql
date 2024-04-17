WITH permissions AS (
  SELECT
    COALESCE(bool_or(permission IN ('org_admin', 'Role::owner')), FALSE) AS is_owner,
    COALESCE(bool_or(permission IN ('org_admin', 'Role::owner', 'Role::write')), FALSE) AS is_user
  FROM
    permissions
  WHERE
    organization_id = $2
    AND actor_id = ANY ($3)
    AND permission IN ('org_admin', 'Role::owner', 'Role::write'))
UPDATE
  roles
SET
  name = CASE WHEN permissions.is_owner THEN
    $4
  ELSE
    roles.name
  END,
  description = CASE WHEN permissions.is_owner THEN
    $5
  ELSE
    roles.description
  END,
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
