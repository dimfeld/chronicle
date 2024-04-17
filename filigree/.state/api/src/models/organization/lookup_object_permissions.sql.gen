SELECT
  CASE WHEN bool_or(permission IN ('org_admin', 'Organization::owner')) THEN
    'owner'
  WHEN bool_or(permission = 'Organization::write') THEN
    'write'
  WHEN bool_or(permission = 'Organization::read') THEN
    'read'
  ELSE
    NULL
  END _permission
FROM
  permissions
WHERE
  organization_id = $1
  AND actor_id = ANY ($2)
  AND permission IN ('org_admin', 'Organization::owner', 'Organization::write', 'Organization::read')
