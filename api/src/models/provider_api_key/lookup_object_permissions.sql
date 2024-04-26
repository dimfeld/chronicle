SELECT
  CASE WHEN bool_or(permission IN ('org_admin', 'ProviderApiKey::owner')) THEN
    'owner'
  WHEN bool_or(permission = 'ProviderApiKey::write') THEN
    'write'
  WHEN bool_or(permission = 'ProviderApiKey::read') THEN
    'read'
  ELSE
    NULL
  END _permission
FROM
  permissions
WHERE
  organization_id = $1
  AND actor_id = ANY ($2)
  AND permission IN ('org_admin', 'ProviderApiKey::owner', 'ProviderApiKey::write', 'ProviderApiKey::read')
