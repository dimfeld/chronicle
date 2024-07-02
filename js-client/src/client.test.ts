import { test, describe, expect, afterAll, beforeAll } from 'bun:test';
import { createChronicleClient } from './client.js';
import { ChronicleChatRequest, ChronicleChatResponseStreaming } from './types.js';
import { HoneycombSDK } from '@honeycombio/opentelemetry-node';
import { trace } from '@opentelemetry/api';
import { uuidv7 } from 'uuidv7';

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
            required: ['name', 'hair_color', 'favorite_color'],
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

test('events', async () => {
  const client = createChronicleClient();
  const runId = uuidv7();
  const step1Id = uuidv7();
  const step2Id = uuidv7();

  console.log('runId', runId);

  // Start a run
  await client.event({
    type: 'run:start',
    id: runId,
    name: 'Test Run',
    application: 'chronicle-test',
    environment: 'test',
    tags: ['test', 'example'],
    info: {
      startValue: 1,
      midValue: 2,
    },
  });

  // Start a step
  await client.event({
    type: 'step:start',
    step_id: step1Id,
    run_id: runId,
    data: {
      type: 'process-step1',
      name: 'Test Step',
      input: { some_input: 'value' },
    },
  });

  // Normal event
  await client.event({
    type: 'test_event',
    data: {
      some_data: true,
    },
    step_id: step1Id,
    run_id: runId,
  });

  // Array of events
  await client.event([
    {
      type: 'step:state',
      step_id: step1Id,
      run_id: runId,
      data: {
        state: 'in progress',
      },
    },
    {
      type: 'step:start',
      step_id: step2Id,
      run_id: runId,
      data: {
        type: 'process',
        parent_step: step1Id,
        name: 'Test Inner Step',
        input: { some_input: 'value' },
      },
    },
    {
      type: 'custom_event',
      step_id: step1Id,
      run_id: runId,
      data: {
        custom_field: 'custom value',
      },
    },
    {
      type: 'step:error',
      step_id: step2Id,
      run_id: runId,
      data: {
        error: { message: 'failed to do the thing' },
      },
    },
    {
      type: 'step:end',
      step_id: step1Id,
      run_id: runId,
      data: {
        output: { result: 3 },
      },
    },
  ]);

  // Update run
  await client.event({
    type: 'run:update',
    id: runId,
    status: 'completed',
    output: { final_result: 'a long message' },
    info: {
      midValue: 10,
      endValue: 3,
    },
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
          step_id: uuidv7(),
          run_id: uuidv7(),
        });
      } finally {
        span.end();
      }
    });
  });
});
