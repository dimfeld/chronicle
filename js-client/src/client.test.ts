import { test, describe, expect, afterAll, beforeAll } from 'bun:test';
import { createChronicleClient } from './client.js';
import { ChronicleChatRequest, ChronicleChatResponseStreaming } from './types.js';
import { HoneycombSDK } from '@honeycombio/opentelemetry-node';
import { trace } from '@opentelemetry/api';

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

test('streaming', async () => {
  let client = createChronicleClient();

  let result = await client({
    model: 'groq/llama3-8b-8192',
    metadata: {
      application: 'chronicle-test',
      environment: 'test',
      workflow_id: 'basic client',
    },
    max_tokens: 128,
    stream: true,
    messages: [
      {
        role: 'user',
        content: 'Hello',
      },
    ],
  });

  let chunks: ChronicleChatResponseStreaming[] = [];
  for await (const chunk of result) {
    chunks.push(chunk);
  }

  expect(chunks.length).toBeGreaterThan(1);

  let first = chunks[0];
  expect(first.choices[0].delta).toBeTruthy();
  expect(first.model).toBe('llama3-8b-8192');
  expect(first.meta?.provider).toBe('groq');

  const text = chunks
    .map((chunk) => chunk.choices[0].delta.content)
    .join('')
    .trim();
  console.log(text);

  expect(text).toBeTruthy();
  expect(text.length).toBeGreaterThan(chunks[0].choices[0].delta.content?.length ?? 0);
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
      // model: 'gpt-3.5-turbo',
      model: 'groq/llama3-8b-8192',
    };

    const client = createChronicleClient();
    const result = await client(testRequest);
    // expect(result.model).toBe('llama3-8b-8192');
    // expect(result.meta.provider).toBe('groq');

    console.dir(result, { depth: null });

    const toolCall = result.choices[0].message.tool_calls?.[0]!;
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

    const toolCall = result.choices[0].message.tool_calls?.[0]!;
    expect(toolCall).toBeTruthy();
    expect(toolCall?.function?.name).toBe('get_characteristics');
    expect(JSON.parse(toolCall?.function.arguments)).toEqual({
      name: 'Daniel',
      hair_color: 'brown',
      favorite_color: 'green',
    });
  });
});

describe('events', () => {
  test('normal event', async () => {
    const client = createChronicleClient();
    await client.event({
      type: 'test_event',
      data: {
        some_data: true,
      },
      // TODO need to use UUIDs here
      step_id: 'test-step',
      run_id: 'test-run',
    });
  });

  test('error event', async () => {
    const client = createChronicleClient();
    await client.event({
      type: 'step:error',
      error: { message: 'failed to do the thing' },
      step_id: 'test-step',
      run_id: 'test-run',
    });
  });
});

describe('tracing', () => {
  const sdk = new HoneycombSDK();
  const tracer = trace.getTracer('chronicle-test');

  beforeAll(() => sdk.start());
  afterAll(() => sdk.shutdown());

  test('LLM call', async () => {
    await tracer.startActiveSpan('chronicle-js call test', async (span) => {
      try {
        const client = createChronicleClient();
        await client({
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
      } finally {
        span.end();
      }
    });
  });

  test('event', async () => {
    await tracer.startActiveSpan('chronicle-js event test', async (span) => {
      try {
        const client = createChronicleClient({
          defaults: {
            metadata: {
              workflow_name: 'event test',
            },
          },
        });
        await client.event({
          type: 'test_event',
          data: {
            some_data: true,
          },
          step_id: 'test-step',
          run_id: 'test-run',
        });
      } finally {
        span.end();
      }
    });
  });
});
