import { test, expect } from 'bun:test';
import { fetchChronicle } from './wrapper.js';
import OpenAI from 'openai';

test('wrap OpenAI', async () => {
  let client = new OpenAI({
    apiKey: '',
    fetch: fetchChronicle({
      defaults: {
        metadata: {
          application: 'chronicle-test',
          environment: 'test',
          workflow_id: 'wrap OpenAI',
        },
      },
    }),
  });

  let result = await client.chat.completions.create({
    model: 'groq/llama3-8b-8192',
    max_tokens: 128,
    temperature: 0,
    messages: [
      {
        role: 'user',
        content: 'Hello',
      },
    ],
  });

  console.dir(result, { depth: null });
});
