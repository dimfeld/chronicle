CREATE TABLE chronicle_runs (
  id text PRIMARY KEY,
  name text NOT NULL,
  description text,
  application text NOT NULL,
  environment text NOT NULL,
  input textb NOT NULL,
  output textb NOT NULL,
  status text NOT NULL,
  trace_id blob,
  span_id blob,
  tags text[],
  info text,
  updated_at int NOT NULL,
  created_at int NOT NULL
);

CREATE INDEX ON chronicle_runs (name);

CREATE INDEX ON chronicle_runs USING gin (tags);

CREATE TABLE chronicle_steps (
  id text PRIMARY KEY,
  run_id text NOT NULL REFERENCES chronicle_runs (id) ON DELETE CASCADE,
  type text NOT NULL,
  parent_step text,
  name text NOT NULL,
  input textb NOT NULL,
  output textb,
  status text NOT NULL,
  tags text[],
  info text,
  span_id blob,
  start_time int NOT NULL,
  end_time int
);

CREATE INDEX ON chronicle_steps (run_id);

CREATE INDEX ON chronicle_steps USING gin (tags);
