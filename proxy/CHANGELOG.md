# chronicle-proxy changelog

## Unreleased

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
