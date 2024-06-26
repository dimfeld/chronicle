# chronicle

Chronicle is a proxy for language model API calls which

- Provides retries and optionally falls back to other providers on a failed call
- Records each call in a database, and sends OpenTelemetry events
- Lets you switch model provider APIs without changing your request format.
- Supports both SQLite and PostgreSQL databases
- Comes with a drop-in fetch function that will redirect OpenAI SDK calls to Chronicle instead.
- Supports logging "runs" and "steps" for multi-step workflows

[See the roadmap](https://imfeld.dev/notes/projects_chronicle) for the current status and other notes.
