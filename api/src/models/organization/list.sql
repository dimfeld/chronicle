SELECT
  id,
  updated_at,
  created_at,
  name,
  OWNER,
  default_role,
  active,
  perm._permission
FROM
  organizations tb
  JOIN LATERAL (
    SELECT
      CASE WHEN bool_or(permission IN ('org_admin', 'Organization::owner')) THEN
        'owner'
      WHEN bool_or(permission = 'Organization::write') THEN
        'write'
      WHEN bool_or(permission = 'Organization::read') THEN
        'read'
      ELSE
        NULL
      END _permission
    FROM
      permissions
    WHERE
      organization_id = $1
      AND actor_id = ANY ($2)
      AND permission IN ('org_admin', 'Organization::owner', 'Organization::write', 'Organization::read')) perm ON
	perm._permission IS NOT NULL
WHERE
  AND __insertion_point_filters
ORDER BY
  __insertion_point_order_by
LIMIT $3 OFFSET $4
