SELECT
  id AS "id: ProviderApiKeyId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  source,
  value,
  _permission AS "_permission!: filigree::auth::ObjectPermission"
FROM
  provider_api_keys tb
  JOIN LATERAL (
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
      organization_id = $2
      AND actor_id = ANY ($3)
      AND permission IN ('org_admin', 'ProviderApiKey::owner', 'ProviderApiKey::write', 'ProviderApiKey::read'))
	_permission ON _permission IS NOT NULL
WHERE
  tb.id = $1
  AND tb.organization_id = $2
