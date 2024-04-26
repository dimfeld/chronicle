DELETE FROM provider_api_keys
WHERE id = $1
  AND organization_id = $2
  AND EXISTS (
    SELECT
      1
    FROM
      permissions
    WHERE
      organization_id = $2
      AND actor_id = ANY ($3)
      AND permission IN ('org_admin', 'ProviderApiKey::owner'))
