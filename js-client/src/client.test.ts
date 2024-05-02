import { test, describe, expect } from 'bun:test';
import { createChronicleClient } from './client.js';
import { ChronicleChatRequest } from './types.js';

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

describe('tools', () => {
  const request: ChronicleChatRequest = {
    model: '',
    max_tokens: 1024,
    messages: [
      {
        role: 'user',
        content: `My name is Daniel, my hair is brown, and my favorite color is green.\n\nWhat are Daniel's characteristics? Respond only with JSON`,
      },
    ],
    tools: [
      {
        type: 'function',
        function: {
          name: 'get_characteristics',
          description: 'Use this tool to extract the physical characteristics of a person.',
          parameters: {
            type: 'object',
            properties: {
              name: { type: 'string' },
              hair_color: { type: 'string' },
              favorite_color: { type: 'string', description: `The person's favorite color.` },
            },
            required: ['name', 'hair', 'color'],
          },
        },
      },
    ],
  };

  test('OpenAI compatible providers', async () => {
    const testRequest = {
      ...request,
      model: 'gpt-3.5-turbo',
      // model: 'groq/llama3-70b-8192',
    };

    const client = createChronicleClient();
    const result = await client(testRequest);
    // expect(result.model).toBe('llama3-8b-8192');
    // expect(result.meta.provider).toBe('groq');

    console.dir(result, { depth: null });

    const toolCall = result.choices[0].message.tool_calls?.[0];
    expect(toolCall).toBeTruthy();
    expect(toolCall?.function?.name).toBe('get_characteristics');
    expect(JSON.parse(toolCall?.function.arguments)).toEqual({
      name: 'Daniel',
      hair_color: 'brown',
      favorite_color: 'green',
    });
  });

  test('Anthropic', async () => {
    const testRequest = {
      ...request,
      model: 'claude-3-haiku-20240307',
    };

    const client = createChronicleClient();
    const result = await client(testRequest);
    console.dir(result, { depth: null });
    expect(result.model).toBe('claude-3-haiku-20240307');
    expect(result.meta.provider).toBe('anthropic');

    const toolCall = result.choices[0].message.tool_calls?.[0];
    expect(toolCall).toBeTruthy();
    expect(toolCall?.function?.name).toBe('get_characteristics');
    expect(JSON.parse(toolCall?.function.arguments)).toEqual({
      name: 'Daniel',
      hair_color: 'brown',
      favorite_color: 'green',
    });
  });
});
