CREATE TABLE chronicle_runs (
  id uuid PRIMARY KEY,
  name text NOT NULL,
  description text,
  application text NOT NULL,
  environment text NOT NULL,
  input jsonb NOT NULL,
  output jsonb NOT NULL,
  status text NOT NULL,
  trace_id text,
  span_id text,
  tags text[],
  info jsonb,
  updated_at timestamp with time zone NOT NULL DEFAULT now(),
  created_at timestamp with time zone NOT NULL DEFAULT now()
);

CREATE INDEX chronicle_runs_name_created_at_idx ON chronicle_runs (name, created_at DESC);

CREATE INDEX chronicle_runs_name_updated_at_idx ON chronicle_runs (name, updated_at DESC);

CREATE INDEX chronicle_runs_app_env_created_at_idx ON chronicle_runs (application, environment,
  created_at DESC);

CREATE INDEX chronicle_runs_app_env_updated_at_idx ON chronicle_runs (application, environment,
  updated_at DESC);

CREATE INDEX chronicle_runs_updated_at_idx ON chronicle_runs (updated_at DESC);

CREATE INDEX chronicle_runs_created_at_idx ON chronicle_runs (created_at DESC);

CREATE INDEX chronicle_runs_tags_idx ON chronicle_runs USING gin (tags);

CREATE TABLE chronicle_steps (
  id uuid PRIMARY KEY,
  run_id uuid NOT NULL REFERENCES chronicle_runs (id) ON DELETE CASCADE,
  type text NOT NULL,
  parent_step uuid,
  name text NOT NULL,
  input jsonb NOT NULL,
  output jsonb,
  status text NOT NULL,
  tags text[],
  info jsonb,
  span_id text,
  start_time timestamp with time zone NOT NULL DEFAULT now(),
  end_time timestamp with time zone
);

CREATE INDEX ON chronicle_steps (run_id);

CREATE INDEX ON chronicle_steps USING gin (tags);

ALTER TABLE chronicle_events
  ALTER COLUMN run_id TYPE uuid
  USING run_id::uuid;

ALTER TABLE chronicle_events
  ALTER COLUMN step TYPE uuid
  USING step::uuid;

CREATE INDEX chronicle_events_run_id_created_at_idx ON chronicle_events (run_id, created_at DESC);

DROP INDEX chronicle_events_run_id_idx;
