SELECT
  id AS "id: AliasModelId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  model,
  provider,
  api_key_name,
  sort,
  alias_id AS "alias_id: AliasId",
  _permission AS "_permission!: filigree::auth::ObjectPermission"
FROM
  alias_models tb
  JOIN LATERAL (
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
      organization_id = $2
      AND actor_id = ANY ($3)
      AND permission IN ('org_admin', 'AliasModel::owner', 'AliasModel::write', 'AliasModel::read'))
	_permission ON _permission IS NOT NULL
WHERE
  tb.id = $1
  AND tb.organization_id = $2
