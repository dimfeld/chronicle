import { test, expect } from 'bun:test';
import { createChronicleClient } from './client.js';

test('basic client', async () => {
  let client = createChronicleClient();

  let result = await client({
    model: 'groq/llama3-8b-8192',
    metadata: {
      application: 'chronicle-test',
      environment: 'test',
      workflow_id: 'basic client',
    },
    max_tokens: 128,
    messages: [
      {
        role: 'user',
        content: 'Hello',
      },
    ],
  });

  console.log(result);

  expect(result.choices[0].message.content).toBeTruthy();
  expect(result.model).toBe('llama3-8b-8192');
  expect(result.meta.provider).toBe('groq');
});

test('with defaults', async () => {
  let client = createChronicleClient({
    defaults: {
      provider: 'groq',
      model: 'llama3-8b-8192',
      metadata: {
        application: 'chronicle-test',
        environment: 'test',
        workflow_id: 'dont use this',
      },
    },
  });

  let result = await client({
    provider: 'groq',
    model: 'llama3-70b-8192',
    max_tokens: 128,
    metadata: {
      workflow_id: 'with defaults',
    },
    messages: [
      {
        role: 'user',
        content: 'Hello',
      },
    ],
  });

  expect(result.choices[0].message.content).toBeTruthy();
  expect(result.model).toBe('llama3-70b-8192');
  expect(result.meta.provider).toBe('groq');
});
