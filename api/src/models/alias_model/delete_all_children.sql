DELETE FROM alias_models
WHERE organization_id = $1
  AND alias_id = $2