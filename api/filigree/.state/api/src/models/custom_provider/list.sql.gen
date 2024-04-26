SELECT
  id,
  organization_id,
  updated_at,
  created_at,
  name,
  label,
  url,
  token,
  api_key,
  api_key_source,
  format,
  headers,
  prefix,
  perm._permission
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
      organization_id = $1
      AND actor_id = ANY ($2)
      AND permission IN ('org_admin', 'CustomProvider::owner', 'CustomProvider::write', 'CustomProvider::read')) perm ON
	perm._permission IS NOT NULL
WHERE
  organization_id = $1
  AND __insertion_point_filters
ORDER BY
  __insertion_point_order_by
LIMIT $3 OFFSET $4
