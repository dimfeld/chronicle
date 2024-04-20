CREATE TABLE chronicle_meta (
  key text PRIMARY KEY,
  value json
);

INSERT INTO chronicle_meta (
  key,
  value)
VALUES (
  "migration_version",
  1::json);

CREATE TABLE IF NOT EXISTS chronicle_events (
  id text PRIMARY KEY,
  chat_request json,
  chat_response json,
  error json,
  application text,
  environment text,
  request_organization_id text,
  request_user_id text,
  workflow id text,
  workflow_name text,
  run_id text,
  step text,
  step_index int,
  extra_meta json,
  response_meta json,
  retries int,
  rate_limited bool,
  request_latency_ms int,
  total_latency_ms int,
  created_at int NOT NULL DEFAULT now()
);
