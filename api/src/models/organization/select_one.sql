SELECT
  id AS "id: OrganizationId",
  updated_at,
  created_at,
  name,
  OWNER AS "owner: crate::models::user::UserId",
  default_role AS "default_role: crate::models::role::RoleId",
  active,
  _permission AS "_permission!: filigree::auth::ObjectPermission"
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
      organization_id = $2
      AND actor_id = ANY ($3)
      AND permission IN ('org_admin', 'Organization::owner', 'Organization::write', 'Organization::read'))
	_permission ON _permission IS NOT NULL
WHERE
  tb.id = $1
