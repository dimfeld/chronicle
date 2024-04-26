CREATE TABLE provider_api_keys (
  id uuid NOT NULL PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
  updated_at timestamptz NOT NULL DEFAULT now(),
  created_at timestamptz NOT NULL DEFAULT now(),
  name text NOT NULL,
  source text NOT NULL,
  value text NOT NULL
);

CREATE INDEX provider_api_keys_organization_id ON provider_api_keys (organization_id);

CREATE INDEX provider_api_keys_updated_at ON provider_api_keys (organization_id, updated_at DESC);

CREATE INDEX provider_api_keys_created_at ON provider_api_keys (organization_id, created_at DESC);

CREATE TABLE custom_providers (
  id uuid NOT NULL PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
  updated_at timestamptz NOT NULL DEFAULT now(),
  created_at timestamptz NOT NULL DEFAULT now(),
  name text NOT NULL,
  label text,
  url text NOT NULL,
  token text,
  api_key text,
  api_key_source text NOT NULL,
  format jsonb NOT NULL,
  headers jsonb,
  prefix text
);

CREATE INDEX custom_providers_organization_id ON custom_providers (organization_id);

CREATE INDEX custom_providers_updated_at ON custom_providers (organization_id, updated_at DESC);

CREATE INDEX custom_providers_created_at ON custom_providers (organization_id, created_at DESC);

CREATE TABLE aliases (
  id uuid NOT NULL PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
  updated_at timestamptz NOT NULL DEFAULT now(),
  created_at timestamptz NOT NULL DEFAULT now(),
  name text NOT NULL,
  random_order boolean NOT NULL
);

CREATE INDEX aliases_organization_id ON aliases (organization_id);

CREATE INDEX aliases_updated_at ON aliases (organization_id, updated_at DESC);

CREATE INDEX aliases_created_at ON aliases (organization_id, created_at DESC);

CREATE TABLE alias_models (
  id uuid NOT NULL PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
  updated_at timestamptz NOT NULL DEFAULT now(),
  created_at timestamptz NOT NULL DEFAULT now(),
  model text NOT NULL,
  provider text NOT NULL,
  api_key_name text,
  alias_id uuid NOT NULL REFERENCES aliases (id) ON DELETE CASCADE
);

CREATE INDEX alias_models_organization_id ON alias_models (organization_id);

CREATE INDEX alias_models_alias_id ON alias_models (organization_id, alias_id);

CREATE INDEX alias_models_updated_at ON alias_models (organization_id, updated_at DESC);

CREATE INDEX alias_models_created_at ON alias_models (organization_id, created_at DESC);
