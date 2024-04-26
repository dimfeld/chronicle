SELECT
  id,
  organization_id,
  updated_at,
  created_at,
  model,
  provider,
  api_key_name,
  alias_id,
  perm._permission
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
      organization_id = $1
      AND actor_id = ANY ($2)
      AND permission IN ('org_admin', 'AliasModel::owner', 'AliasModel::write', 'AliasModel::read')) perm ON
	perm._permission IS NOT NULL
WHERE
  organization_id = $1
  AND __insertion_point_filters
ORDER BY
  __insertion_point_order_by
LIMIT $3 OFFSET $4
