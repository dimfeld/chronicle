# chronicle-proxy changelog

## Unreleased

- Support Ollama 0.3.0 tool calls
- Support `strict` field in function calls with GPT4o

## 0.4.3

- Allow customizing the initial status in a `run:start` event.
- When receiving a `run:start` event for an already-existing run, update the status instead of ignoring it.
- Fix Anthropic tool use results and streaming
- Add support for AWS Bedrock
- Change `finish_reason` to be an enum instead of a string. This also standardizes the field across providers.

## 0.4.2

- When using a PostgreSQL database, send notifications for each run that has an update

## 0.4.1

- Don't require `tags` on "run:start" event
- Don't require `input` on "step:start" event
- Remove foreign key constraint of step run_id, in case events arrive out of order.
- Don't require `application` or `environment` in run SQLite table schema

## 0.4.0

- Add "runs" and "steps", with events to manage them

## 0.3.2

- Anthropic provider was omitting the system message

## 0.3.1

- Provide a `max_token` value to Anthropic when the request omits it.
- Add Mistral provider

## 0.3.0

- Streaming support for OpenAI-compatible providers, Anthropic, and Groq

## 0.2.0

- Support Anthropic `tool_choice` field
- Recover from Groq error when it fails to parse an actually-valid tool call response
- Support both SQLite and PostgreSQL without recompiling.

## 0.1.5

- Add function for recording non-LLM events

## 0.1.4

- Support tool calling

## 0.1.3

- Added support for Anyscale, DeepInfra, Fireworks, and Together.ai
