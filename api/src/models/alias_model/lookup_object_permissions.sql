SELECT
  CASE WHEN bool_or(permission IN ('org_admin', 'AliasModel::owner')) THEN
    'owner'
  WHEN bool_or(permission = 'AliasModel::write') THEN
    'write'
  WHEN bool_or(permission = 'AliasModel::read') THEN
    'read'
  ELSE
    NULL
  END _permission
FROM
  permissions
WHERE
  organization_id = $1
  AND actor_id = ANY ($2)
  AND permission IN ('org_admin', 'AliasModel::owner', 'AliasModel::write', 'AliasModel::read')
