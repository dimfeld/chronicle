# chronicle-api changelog

## 0.4.2

- When using a PostgreSQL database, send notifications for each run that has an update

## 0.4.1

- Better validation of step and run events with invalid payloads
- Don't require `tags` on "run:start" event
- Don't require `input` on "step:start" event
- Remove foreign key constraint of step run_id, in case events arrive out of order.
- Don't require `application` or `environment` in run SQLite table schema

## 0.4.0

- Add "runs" and "steps", with events to manage them

## 0.3.2

- Anthropic provider was omitting the system message

## 0.3.1

- Fix reading `.env` files associated with global configs.
- Provide a `max_token` value to Anthropic when the request omits it.
- Add Mistral provider
- Handle the `/` route. This just returns `{ status: 'ok' }` without doing anything.

## 0.3.0

- Streaming support for OpenAI-compatible providers, Anthropic, and Groq

## 0.2.0

- Removed the version of the API which was designed to eventually have a full web app. The API-only binary is the only one available for now. A web app will probably return at some point in some other form.
- Support configuring SQLite or PostgreSQL at runtime.

## 0.1.1

- Allow sending arbitrary events to the Chronicle proxy

## 0.1.0

- Initial release
