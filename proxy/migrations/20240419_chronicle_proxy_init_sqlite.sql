-- basic tables required for general proxy use
INSERT INTO chronicle_meta (
  key,
  value)
VALUES (
  'migration_version',
  '1');

CREATE TABLE chronicle_pricing_plans (
  id text PRIMARY KEY,
  provider text,
  start_date bigint,
  end_date bigint,
  per_input_token numeric,
  per_output_token numeric,
  per_request numeric
);

CREATE TABLE IF NOT EXISTS chronicle_events (
  id text PRIMARY KEY,
  event_type text,
  organization_id text,
  project_id text,
  user_id text,
  chat_request text,
  chat_response text,
  error text,
  provider text,
  model text,
  application text,
  environment text,
  request_organization_id text,
  request_project_id text,
  request_user_id text,
  workflow_id text,
  workflow_name text,
  run_id text,
  step text,
  step_index int,
  prompt_id text,
  prompt_version int,
  meta text,
  response_meta text,
  retries int,
  rate_limited bool,
  request_latency_ms int,
  total_latency_ms int,
  pricing_plan bigint REFERENCES chronicle_pricing_plans (id),
  created_at int NOT NULL
);

CREATE INDEX chronicle_events_workflow_id_idx ON chronicle_events (workflow_id);

CREATE INDEX chronicle_events_run_id_idx ON chronicle_events (run_id);
