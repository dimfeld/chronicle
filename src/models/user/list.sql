SELECT
  id,
  organization_id,
  updated_at,
  created_at,
  name,
  email,
  avatar_url,
  perm._permission
FROM
  users tb
  JOIN LATERAL (
    SELECT
      CASE WHEN bool_or(permission IN ('org_admin', 'User::owner')) THEN
        'owner'
      WHEN bool_or(permission = 'User::write') THEN
        'write'
      WHEN bool_or(permission = 'User::read') THEN
        'read'
      ELSE
        NULL
      END _permission
    FROM
      permissions
    WHERE
      organization_id = $1
      AND actor_id = ANY ($2)
      AND permission IN ('org_admin', 'User::owner', 'User::write', 'User::read')) perm ON
	perm._permission IS NOT NULL
WHERE
  organization_id = $1
  AND __insertion_point_filters
ORDER BY
  __insertion_point_order_by
LIMIT $3 OFFSET $4
