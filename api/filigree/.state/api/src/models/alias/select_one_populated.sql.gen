SELECT
  id AS "id: AliasId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  random_order,
  (
    SELECT
      COALESCE(ARRAY_AGG(JSONB_BUILD_OBJECT('id', id, 'organization_id', organization_id,
	'updated_at', updated_at, 'created_at', created_at, 'model', model,
	'provider', provider, 'api_key_name', api_key_name, 'alias_id', alias_id,
	'_permission', _permission)), ARRAY[]::jsonb[])
    FROM
      alias_models
    WHERE
      alias_id = $1
      AND organization_id = $2) AS "models!: Vec<AliasModel>",
  _permission AS "_permission!: filigree::auth::ObjectPermission"
FROM
  aliases tb
  JOIN LATERAL (
    SELECT
      CASE WHEN bool_or(permission IN ('org_admin', 'Alias::owner')) THEN
        'owner'
      WHEN bool_or(permission = 'Alias::write') THEN
        'write'
      WHEN bool_or(permission = 'Alias::read') THEN
        'read'
      ELSE
        NULL
      END _permission
    FROM
      permissions
    WHERE
      organization_id = $2
      AND actor_id = ANY ($3)
      AND permission IN ('org_admin', 'Alias::owner', 'Alias::write', 'Alias::read'))
	_permission ON _permission IS NOT NULL
WHERE
  tb.id = $1
  AND tb.organization_id = $2
