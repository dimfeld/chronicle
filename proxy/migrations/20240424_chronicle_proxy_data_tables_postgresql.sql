-- Data tables. These are optional and only needed if you want to store and load configuration in the database.
CREATE TABLE IF NOT EXISTS chronicle_custom_providers (
  id uuid PRIMARY KEY,
  name text,
  label text,
  url text NOT NULL,
  token text,
  token_env text,
  format jsonb NOT NULL,
  headers jsonb,
  prefix text,
  default_for jsonb,
);

CREATE TABLE IF NOT EXISTS chronicle_aliases (
  id uuid PRIMARY KEY,
  name text NOT NULL UNIQUE,
  random_order bool NOT NULL DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS chronicle_alias_providers (
  id uuid PRIMARY KEY,
  alias_id bigint REFERENCES chronicle_aliases (id),
  order int NOT NULL DEFAULT 0,
  model text NOT NULL,
  provider text NOT NULL,
  api_key_name text
);

CREATE TABLE IF NOT EXISTS chronicle_api_keys (
  id uuid PRIMARY KEY,
  name text,
  source text,
  value text
);
