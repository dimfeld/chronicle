SELECT
  CASE WHEN bool_or(permission IN ('org_admin', 'CustomProvider::owner')) THEN
    'owner'
  WHEN bool_or(permission = 'CustomProvider::write') THEN
    'write'
  WHEN bool_or(permission = 'CustomProvider::read') THEN
    'read'
  ELSE
    NULL
  END _permission
FROM
  permissions
WHERE
  organization_id = $1
  AND actor_id = ANY ($2)
  AND permission IN ('org_admin', 'CustomProvider::owner', 'CustomProvider::write', 'CustomProvider::read')
