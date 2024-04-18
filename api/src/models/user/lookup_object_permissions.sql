SELECT
  CASE WHEN bool_or(permission IN ('org_admin', 'User::owner')) THEN
    'owner'
  WHEN bool_or(permission = 'User::write') THEN
    'write'
  WHEN bool_or(permission = 'User::read') THEN
    'read'
  ELSE
    NULL
  END _permission
FROM
  permissions
WHERE
  organization_id = $1
  AND actor_id = ANY ($2)
  AND permission IN ('org_admin', 'User::owner', 'User::write', 'User::read')
