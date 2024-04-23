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
  pricing_plan bigint REFERENCES chronicle_pricing_plans (id),
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS chronicle_custom_providers (
  name text PRIMARY KEY,
  label text,
  url text NOT NULL,
  token text,
  token_env text,
  format jsonb NOT NULL,
  headers jsonb,
  prefix text,
  default_for jsonb,
);

CREATE TABLE IF NOT EXISTS chronicle_webhooks (
  id uuid PRIMARY KEY,
  name text NOT NULL,
  url text NOT NULL,
  method text NOT NULL,
  id_field text
);

CREATE TABLE IF NOT EXISTS chronicle_aliases (
  name text PRIMARY KEY,
  model text NOT NULL,
  provider text NOT NULL,
  api_key_name text
);

CREATE TABLE IF NOT EXISTS chronicle_api_keys (
  name text PRIMARY KEY,
  source text,
  value text
);

CREATE TABLE chronicle_pricing_plans (
  id integer PRIMARY KEY,
  provider text,
  start_date date,
  end_date date,
  per_input_token numeric,
  per_output_token numeric,
  per_request numeric
);
