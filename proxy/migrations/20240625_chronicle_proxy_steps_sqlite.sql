CREATE TABLE chronicle_runs (
  id text PRIMARY KEY,
  name text NOT NULL,
  description text,
  application text,
  environment text,
  input text,
  output text,
  status text NOT NULL,
  trace_id text,
  span_id text,
  tags text,
  info text,
  updated_at int NOT NULL,
  created_at int NOT NULL
);

CREATE INDEX chronicle_runs_name_created_at_idx ON chronicle_runs (name, created_at DESC);

CREATE INDEX chronicle_runs_name_updated_at_idx ON chronicle_runs (name, updated_at DESC);

CREATE INDEX chronicle_runs_env_app_created_at_idx ON chronicle_runs (environment, application,
  created_at DESC);

CREATE INDEX chronicle_runs_env_app_updated_at_idx ON chronicle_runs (environment, application,
  updated_at DESC);

CREATE INDEX chronicle_runs_updated_at_idx ON chronicle_runs (updated_at DESC);

CREATE INDEX chronicle_runs_created_at_idx ON chronicle_runs (created_at DESC);

CREATE TABLE chronicle_steps (
  id text PRIMARY KEY,
  run_id text NOT NULL,
  type text NOT NULL,
  parent_step text,
  name text,
  input text,
  output text,
  status text NOT NULL,
  span_id text,
  tags text,
  info text,
  start_time int NOT NULL,
  end_time int
);

CREATE INDEX chronicle_steps_run_id_idx ON chronicle_steps (run_id);

CREATE INDEX chronicle_events_run_id_created_at_idx ON chronicle_events (run_id, created_at DESC);

DROP INDEX chronicle_events_run_id_idx;

ALTER TABLE chronicle_events RENAME COLUMN step TO step_id;
