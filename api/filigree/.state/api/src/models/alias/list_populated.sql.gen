SELECT
  id,
  organization_id,
  updated_at,
  created_at,
  name,
  random_order,
  perm._permission
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
      organization_id = $1
      AND actor_id = ANY ($2)
      AND permission IN ('org_admin', 'Alias::owner', 'Alias::write', 'Alias::read')) perm ON
	perm._permission IS NOT NULL
WHERE
  organization_id = $1
  AND __insertion_point_filters
ORDER BY
  __insertion_point_order_by
LIMIT $3 OFFSET $4
