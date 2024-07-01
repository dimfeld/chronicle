import { expect, test } from 'bun:test';
import { asStep, recordStepInfo, runStep, startRun } from './runs.js';
import { uuidv7 } from 'uuidv7';
import { flushEvents } from './logger.js';

const autoStep = asStep(async function autoStep(n: number) {
  recordStepInfo({ addend: 1 });
  return await namedStep(n + 1);
});

const namedStep = asStep(
  async (n: number) => {
    recordStepInfo({ more_info: true });
    return n + 2;
  },
  {
    type: 'adder',
    name: 'addTwo',
    info: {
      addend: 2,
    },
    tags: ['adder', 'math'],
  }
);

const errorStep = asStep(async function errorStep() {
  throw new Error('test error');
});

test('runs and steps ', async () => {
  const runId = uuidv7();
  let retVal = await startRun(
    {
      name: 'Test Run',
      application: 'chronicle-test',
      environment: 'test',
      description: 'This is a test run',
      runId,
      info: {
        testing: true,
      },
      input: {
        value: 1,
      },
      skipFinishEvent: false,
      tags: ['test'],
    },
    async () => {
      return await runStep(
        {
          name: 'outer step',
          type: 'outer',
        },
        async (ctx) => {
          expect(ctx.runId).toEqual(runId);
          try {
            await errorStep();
            throw new Error(`Failed to propagate error from step`);
          } catch (e) {
            expect(e.message).toEqual('test error');
          }

          ctx.recordRunInfo({ addedRunInfo: true });
          return await autoStep(1);
        }
      );
    }
  );

  expect(retVal).toEqual({
    id: runId,
    info: {
      addedRunInfo: true,
    },
    output: 4,
  });

  await flushEvents();
});

test('run with skipFinishEvent', async () => {
  const runId = uuidv7();
  await startRun(
    {
      name: 'Test with skipFinishEvent',
      runId,
      info: {
        testing: true,
      },
      input: {
        value: 1,
      },
      skipFinishEvent: true,
      tags: ['test'],
    },
    async () => {
      return 1;
    }
  );

  await flushEvents();
});

test('run error', async () => {
  const runId = uuidv7();
  try {
    await startRun(
      {
        runId,
        name: 'error test run',
      },
      async () => {
        throw new Error('test error');
      }
    );

    throw new Error(`Failed to propagate error from run`);
  } catch (e) {
    expect(e.message).toEqual('test error');
  } finally {
    await flushEvents();
  }
});
