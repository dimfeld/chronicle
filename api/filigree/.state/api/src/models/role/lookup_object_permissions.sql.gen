SELECT
  CASE WHEN bool_or(permission IN ('org_admin', 'Role::owner')) THEN
    'owner'
  WHEN bool_or(permission = 'Role::write') THEN
    'write'
  WHEN bool_or(permission = 'Role::read') THEN
    'read'
  ELSE
    NULL
  END _permission
FROM
  permissions
WHERE
  organization_id = $1
  AND actor_id = ANY ($2)
  AND permission IN ('org_admin', 'Role::owner', 'Role::write', 'Role::read')
