-- basic tables required for general proxy use
CREATE TABLE IF NOT EXISTS chronicle_meta (
  key text PRIMARY KEY,
  value jsonb
);

INSERT INTO chronicle_meta (
  key,
  value)
VALUES (
  'migration_version',
  '1' ::jsonb)
ON CONFLICT DO NOTHING;

CREATE TABLE IF NOT EXISTS chronicle_pricing_plans (
  id uuid PRIMARY KEY,
  provider uuid,
  start_date date,
  end_date date,
  per_input_token numeric,
  per_output_token numeric,
  per_request numeric
);

CREATE TABLE IF NOT EXISTS chronicle_events (
  id uuid PRIMARY KEY,
  organization_id text,
  project_id text,
  user_id text,
  chat_request jsonb NOT NULL,
  chat_response jsonb,
  error jsonb,
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
  extra_meta jsonb,
  response_meta jsonb,
  retries int,
  rate_limited bool,
  request_latency_ms int,
  total_latency_ms int,
  pricing_plan uuid REFERENCES chronicle_pricing_plans (id),
  created_at timestamptz NOT NULL DEFAULT now()
);
