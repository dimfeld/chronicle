SELECT
  id AS "id: UserId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  email,
  avatar_url,
  _permission AS "_permission!: filigree::auth::ObjectPermission"
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
      organization_id = $2
      AND actor_id = ANY ($3)
      AND permission IN ('org_admin', 'User::owner', 'User::write', 'User::read'))
	_permission ON _permission IS NOT NULL
WHERE
  tb.id = $1
  AND tb.organization_id = $2
