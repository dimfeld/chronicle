# chronicle

Chronicle is a proxy for language model API calls which

- Provides retries and optionally falls back to other providers on a failed call
- Records each call in a database, and sends OpenTelemetry events
- Lets you switch model provider APIs without changing your request format.
- Supports both SQLite and PostgreSQL databases
- Comes with a drop-in fetch function that will redirect OpenAI SDK calls to Chronicle instead.
- Supports logging "runs" and "steps" for multi-step workflows

[See the roadmap](https://imfeld.dev/notes/projects_chronicle) for the current status and other notes.

The project contains both a Rust crate named [chronicle-proxy](https://crates.io/crates/chronicle-proxy) in the `proxy` directory for embedding in applications, and a [turnkey server](https://crates.io/crates/chronicle-api) in the `api` directory which can be run directly.

See the [CHANGELOG](./api/CHANGELOG.md) for latest changes.

## Supported Providers

- OpenAI
- Anthropic
- AWS Bedrock
- Groq
- Ollama
- AnyScale
- DeepInfra
- Fireworks
- Together
