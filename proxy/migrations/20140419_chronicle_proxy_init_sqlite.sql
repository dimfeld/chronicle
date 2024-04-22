CREATE TABLE chronicle_meta (
  key text PRIMARY KEY,
  value json
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
  chat_request json NOT NULL,
  chat_response json,
  error json,
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
  extra_meta json,
  response_meta json,
  retries int,
  rate_limited bool,
  request_latency_ms int,
  total_latency_ms int,
  created_at int NOT NULL DEFAULT unix_epoch ()
);

CREATE TABLE IF NOT EXISTS chronicle_custom_providers (
  name text PRIMARY KEY,
  headers json,
  format text NOT NULL,
  url text NOT NULL,
);

CREATE TABLE IF NOT EXISTS chronicle_webhooks (
  id text PRIMARY KEY,
  name text NOT NULL,
  url text NOT NULL,
  method text NOT NULL,
  id_field text
);
