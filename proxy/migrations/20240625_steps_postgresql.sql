CREATE TABLE chronicle_runs (
  id uuid PRIMARY KEY,
  name text NOT NULL,
  description text,
  application text NOT NULL,
  environment text NOT NULL,
  input jsonb NOT NULL,
  output jsonb NOT NULL,
  status text NOT NULL,
  trace_id bytea,
  span_id bytea,
  tags text[],
  info jsonb,
  updated_at timestamp with time zone NOT NULL DEFAULT now(),
  created_at timestamp with time zone NOT NULL DEFAULT now()
);

CREATE INDEX ON chronicle_runs (name);

CREATE INDEX ON chronicle_runs USING gin (tags);

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
  span_id bytea,
  start_time timestamp with time zone NOT NULL DEFAULT now(),
  end_time timestamp with time zone
);

CREATE INDEX ON chronicle_steps (run_id);

CREATE INDEX ON chronicle_steps USING gin (tags);
