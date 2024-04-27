SELECT
  id AS "id: CustomProviderId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  label,
  url,
  token,
  api_key,
  api_key_source,
  format AS "format: ProviderRequestFormat",
  headers,
  prefix,
  _permission AS "_permission!: filigree::auth::ObjectPermission"
FROM
  custom_providers tb
  JOIN LATERAL (
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
      organization_id = $2
      AND actor_id = ANY ($3)
      AND permission IN ('org_admin', 'CustomProvider::owner', 'CustomProvider::write', 'CustomProvider::read'))
	_permission ON _permission IS NOT NULL
WHERE
  tb.id = $1
  AND tb.organization_id = $2
