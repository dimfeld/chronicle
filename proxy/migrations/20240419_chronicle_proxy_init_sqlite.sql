-- basic tables required for general proxy use
CREATE TABLE chronicle_meta (
  key text PRIMARY KEY,
  value text
);

INSERT INTO chronicle_meta (
  key,
  value)
VALUES (
  "migration_version",
  1);

CREATE TABLE IF NOT EXISTS chronicle_events (
  id text PRIMARY KEY,
  organization_id text,
  project_id text,
  user_id text,
  chat_request text NOT NULL,
  chat_response text,
  error text,
  provider text,
  model text,
  application text,
  environment text,
  request_organization_id text,
  request_project_id text,
  request_user_id text,
  workflow id text,
  workflow_name text,
  run_id text,
  step text,
  step_index int,
  extra_meta text,
  response_meta text,
  retries int,
  rate_limited bool,
  request_latency_ms int,
  total_latency_ms int,
  pricing_plan bigint REFERENCES chronicle_pricing_plans (id),
  created_at int NOT NULL DEFAULT unix_epoch ()
);

CREATE TABLE chronicle_pricing_plans (
  id text PRIMARY KEY,
  provider text,
  start_date bigint,
  end_date bigint,
  per_input_token numeric,
  per_output_token numeric,
  per_request numeric
);
