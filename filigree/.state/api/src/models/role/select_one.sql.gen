SELECT
  id AS "id: RoleId",
  organization_id AS "organization_id: crate::models::organization::OrganizationId",
  updated_at,
  created_at,
  name,
  description,
  _permission AS "_permission!: filigree::auth::ObjectPermission"
FROM
  roles tb
  JOIN LATERAL (
    SELECT
      CASE WHEN bool_or(permission IN ('org_admin', 'Role::owner')) THEN
        'owner'
      WHEN bool_or(permission = 'Role::write') THEN
        'write'
      WHEN bool_or(permission = 'Role::read') THEN
        'read'
      ELSE
        NULL
      END _permission
    FROM
      permissions
    WHERE
      organization_id = $2
      AND actor_id = ANY ($3)
      AND permission IN ('org_admin', 'Role::owner', 'Role::write', 'Role::read'))
	_permission ON _permission IS NOT NULL
WHERE
  tb.id = $1
  AND tb.organization_id = $2
