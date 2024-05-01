# chronicle

Chronicle is a proxy for language model API calls which

- Provides retries and optionally falls back to other providers on a failed call
- Records each call in a database, and sends OpenTelemetry events
- Lets you switch model provider APIs without changing your request format.
- Comes with a drop-in fetch function that will redirect OpenAI SDK calls to Chronicle instead.
- Comes in two versions:
    - A full-fledged server for production use that uses PostgreSQL
    - A lightweight server that logs to an SQLite database and can read configuration from disk.

[See the roadmap](https://imfeld.dev/notes/projects_chronicle) for the current status and other notes.
