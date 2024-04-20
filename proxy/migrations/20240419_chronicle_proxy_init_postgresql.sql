CREATE TABLE chronicle_meta (
  key text PRIMARY KEY,
  value jsonb,
);

INSERT INTO chronicle_meta (
  key,
  value)
VALUES (
  "migration_version",
  1::jsonb);

CREATE TABLE IF NOT EXISTS chronicle_events (
  id uuid PRIMARY KEY,
  chat_request jsonb,
  chat_response jsonb,
  error jsonb,
  application text,
  environment text,
  request_organization_id text,
  request_user_id text,
  workflow id text,
  workflow_name text,
  run_id text,
  step text,
  step_index int,
  extra_meta jsonb,
  response_meta jsonb,
  retries int,
  rate_limited bool,
  request_latency_ms int,
  total_latency_ms int,
  created_at timestamptz NOT NULL DEFAULT now()
);
