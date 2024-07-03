import { AsyncLocalStorage } from 'node:async_hooks';
import opentelemetry, {
  AttributeValue,
  Span,
  SpanOptions,
  SpanStatusCode,
} from '@opentelemetry/api';
import { uuidv7 } from 'uuidv7';
import {
  RunStartEvent,
  RunUpdateEvent,
  StepEndEvent,
  StepErrorEvent,
  isWorkflowEvent,
  type ChronicleEvent,
  type StepStartEvent,
} from './events.js';
import { ChronicleClient, getDefaultClient } from './client.js';

export const tracer = opentelemetry.trace.getTracer('ramus');

export interface StepOptions {
  /** The name of this step */
  name: string;
  /** The type of this step, such as 'dag:node', 'model query', or similar */
  type: string;
  tags?: string[];
  info?: object;
  /** Record this as the input for the step. */
  input?: unknown;

  /** Options for the OpenTelemetry span that will wrap this step. */
  spanOptions?: SpanOptions;

  /** Override the parent OpenTelemetry span */
  parentSpan?: opentelemetry.Context;

  /** Use this context as the parent for the step. This can be useful if the step's execution was delayed in
   * some way that makes it difficult to link to the parent run context, such as when running the step in a DAG,
   * based on some EventEmitter. */
  parentRunContext?: RunContext;
}

/** Run a step of a workflow. This both adds a tracing span and starts a new step in the
 * workflow's event tracking. */
export function runStep<T>(options: StepOptions, f: (ctx: RunContext, span: Span) => Promise<T>) {
  let stepId = uuidv7();
  let spanOptions: SpanOptions = options.spanOptions ?? {};
  spanOptions.attributes = {
    'workflow.step.name': options.name,
    'workflow.step.type': options.type,
    'workflow.step.tags': options.tags,
    'workflow.step.id': stepId,
    ...objectToSpanAttributeValues(options.info, 'workflow.step.info.'),
    ...spanOptions.attributes,
  };

  return runInSpanWithParent(options.name, spanOptions, options.parentSpan, (span) => {
    return runNewStepInternal(options, stepId, span, (ctx) => f(ctx, span));
  });
}

/** Run a function in a span, and record errors if they occur */
export async function runInSpanWithParent<T>(
  spanName: string,
  options: SpanOptions,
  parent: opentelemetry.Context | undefined,
  f: (span: Span) => Promise<T>
): Promise<T> {
  parent ??= opentelemetry.context.active();
  return tracer.startActiveSpan(spanName, options, parent, async (span) => {
    try {
      let value = await f(span);
      span.end();
      return value;
    } catch (e) {
      span.recordException(e as Error);
      span.setStatus({ code: SpanStatusCode.ERROR });
      throw e;
    } finally {
      span.end();
    }
  });
}

/** Run a function in a span, and record errors if they occur */
export async function runInSpan<T>(
  spanName: string,
  options: SpanOptions,
  f: (span: Span) => Promise<T>
): Promise<T> {
  return runInSpanWithParent(spanName, options, undefined, f);
}

export function addSpanEvent(span: Span, e: ChronicleEvent) {
  if (!isWorkflowEvent(e) && span.isRecording()) {
    const spanData = Object.fromEntries(
      Object.entries(e.data ?? {}).map(([k, v]) => [k, toSpanAttributeValue(v)])
    );

    span.addEvent(e.type as string, spanData);
  }
}

export function toSpanAttributeValue(v: AttributeValue | object): AttributeValue {
  if (v && typeof v === 'object' && !Array.isArray(v)) {
    return JSON.stringify(v);
  } else {
    return v;
  }
}

export function objectToSpanAttributeValues(
  o: object | null | undefined,
  prefix = ''
): Record<string, AttributeValue> | undefined {
  if (!o) {
    return undefined;
  }

  return Object.fromEntries(
    Object.entries(o).map(([k, v]) => [prefix + k, toSpanAttributeValue(v)])
  );
}

export const asyncEventStorage = new AsyncLocalStorage<RunContext>();

export interface RunContext {
  /** The ID of this run */
  runId: string;
  /** The ID of the current step */
  stepId: string | undefined;
  /** The Chronicle client, which can be used for logging events and making LLM requests */
  chronicle: ChronicleClient;
  /** Record additional information about the step that is only known after starting it.
   * Each call to `recordStepInfo will merge the argument with the arguments to previous calls.`*/
  recordStepInfo: (o: object) => void;
  /** Retrieve any information recorded by `recordStepInfo` */
  getRecordedStepInfo: () => object | undefined;
  /** Record additional information about the run that is only known after starting it.
   * Each call to `recordStepInfo will merge the argument with the arguments to previous calls.`*/
  recordRunInfo: (o: object) => void;
  /** Record any information recorded by `recordRunInfo`. */
  getRecordedRunInfo: () => object | undefined;
  /** Set the final status for the run to something other than the default of 'finished'.
   * This does not update the status right away; rather it customizes the status passed
   * to the `run:update` event after the run's function finishes.
   *
   * If you want to update the status of a run while it is running, you can send
   * your own `run:update` event to `chronicle.event()`.
   * */
  setRunFinishStatus: (status: string) => void;
}

export function getEventContext(): RunContext | undefined {
  return asyncEventStorage.getStore();
}

/** Options for starting a new run. Many of these options are designed for use when
 * restarting a previous run, as when a state machine was dormant and has received an
 * event. */
export interface RunOptions {
  /** Restore context with this existing run ID */
  runId?: string;

  /** A Chronicle client. If omitted, the default client will be used */
  chronicle?: ChronicleClient;

  /** A name for this run */
  name?: string;

  /** Description for this run */
  description?: string;

  /** Tags for this run */
  tags?: string[];

  /** Input for this run. */
  input?: unknown;

  /** Additional information about this run. */
  info?: object;

  /** Customize the initial status of the run. The default value is 'started'. */
  status?: string;

  /** The application name linked to the run. If ommited, the value from the Chronicle client
   * can be used instead. */
  application?: string;
  /** The environment name linked to the run. If ommited, the value from the Chronicle client
   * can be used instead. */
  environment?: string;

  /** Force the span for this run to have this parent. */
  parentSpanContext?: opentelemetry.Context;
}

/** Run a workflow and initialize an event context, if one does not already exist. */
export function startRun<T>(
  options: RunOptions,
  fn: (ctx: RunContext) => Promise<T>
): Promise<{ id: string; output: T; info: object | undefined }> {
  const chronicle = options.chronicle ?? getDefaultClient();
  let runInfo: object | undefined;
  let runFinishStatus = 'finished';
  let context: RunContext = {
    runId: options.runId ?? uuidv7(),
    stepId: undefined,
    chronicle,
    // We're not in a step yet, so these don't do anything yet.
    recordStepInfo: () => {},
    getRecordedStepInfo: () => undefined,
    recordRunInfo: (o: object) => {
      runInfo = {
        ...runInfo,
        ...o,
      };
    },
    getRecordedRunInfo: () => runInfo,
    setRunFinishStatus: (status: string) => {
      runFinishStatus = status;
    },
  };

  return runInSpanWithParent(
    options.name ?? 'run',
    {
      attributes: {
        'workflow.run.application': options.application ?? chronicle.metadata.metadata?.application,
        'workflow.run.environment': options.environment ?? chronicle.metadata.metadata?.environment,
        'workflow.run.id': context.runId,
        'workflow.run.tags': options.tags,
        'workflow.run.input':
          options.input == undefined ? undefined : JSON.stringify(options.input),
        ...objectToSpanAttributeValues(options.info, 'workflow.run.info.'),
      },
    },
    options.parentSpanContext,
    () => {
      chronicle.event({
        type: 'run:start',
        id: context.runId,
        name: options.name ?? '',
        application: options.application ?? chronicle.metadata.metadata?.application,
        environment: options.environment ?? chronicle.metadata.metadata?.environment,
        input: options.input,
        info: options.info,
        status: options.status,
        description: options.description,
        tags: options.tags,
        time: new Date(),
      } satisfies RunStartEvent);

      return asyncEventStorage.run(context, async () => {
        try {
          const retVal = await fn(context);
          chronicle.event({
            type: 'run:update',
            id: context.runId,
            output: retVal,
            info: runInfo,
            status: runFinishStatus,
            time: new Date(),
          } satisfies RunUpdateEvent);

          return { id: context.runId, info: runInfo, output: retVal };
        } catch (e) {
          chronicle.event({
            type: 'run:update',
            id: context.runId,
            status: 'error',
            time: new Date(),
            output: e as Error,
            info: runInfo,
          } satisfies RunUpdateEvent);
          throw e;
        }
      });
    }
  );
}

/** Run a new step, recording the current step as the step's parent. */
async function runNewStepInternal<T>(
  options: StepOptions,
  stepId: string | undefined,
  span: Span,
  fn: (ctx: RunContext) => Promise<T>
): Promise<T> {
  const { name, tags, info, input } = options;
  let additionalInfo: object | undefined;
  function recordStepInfo(o: object) {
    additionalInfo = {
      ...additionalInfo,
      ...o,
    };
  }

  function getRecordedStepInfo() {
    return additionalInfo;
  }

  let oldContext = options.parentRunContext ?? getEventContext();
  let currentStep = stepId ?? uuidv7();
  let runId = oldContext?.runId;

  if (!runId) {
    // Wrap this step in a run
    const { output } = await startRun(
      {
        name: options.name,
        chronicle: oldContext?.chronicle ?? getDefaultClient(),
        input: options.input,
        info: options.info,
        tags: options.tags,
      },
      () => runNewStepInternal(options, stepId, span, fn)
    );

    return output;
  }

  let newContext: RunContext = {
    chronicle: oldContext?.chronicle ?? getDefaultClient(),
    runId,
    stepId: currentStep,
    recordRunInfo: oldContext?.recordRunInfo ?? (() => {}),
    getRecordedRunInfo: oldContext?.getRecordedRunInfo ?? (() => undefined),
    setRunFinishStatus: oldContext?.setRunFinishStatus ?? (() => {}),
    recordStepInfo,
    getRecordedStepInfo,
  };

  return asyncEventStorage.run(newContext, async () => {
    let startTime = new Date();
    newContext.chronicle.event({
      type: 'step:start',
      step_id: currentStep,
      run_id: newContext.runId,
      time: startTime,
      data: {
        name,
        type: options.type,
        input,
        tags,
        info: info,
        parent_step: oldContext?.stepId,
        span_id: stepSpanId(span),
      },
    } satisfies StepStartEvent);

    try {
      const retVal = await fn(newContext);

      newContext.chronicle.event({
        type: 'step:end',
        run_id: newContext.runId,
        step_id: newContext.stepId,
        time: new Date(),
        data: {
          info: additionalInfo,
          output: retVal,
        },
      } satisfies StepEndEvent);

      return retVal;
    } catch (e) {
      newContext.chronicle.event({
        type: 'step:error',
        run_id: newContext.runId,
        step_id: newContext.stepId ?? undefined,
        time: new Date(),
        data: {
          error: {
            message: (e as Error).message,
            stack: (e as Error).stack,
          },
        },
      } satisfies StepErrorEvent);
      throw e;
    }
  });
}

export interface AsStepOptions {
  name?: string;
  type: string;
  tags?: string[];
  info?: object;
}

/** Wrap a function so that it runs as a step.
 *
 *  export const doIt = asStep(async doIt(input) => {
 *    await callModel(input)
 *  })
 * */
export function asStep<P extends unknown[] = unknown[], RET = unknown>(
  fn: (...args: P) => Promise<RET>,
  options?: AsStepOptions
): (...args: P) => Promise<RET> {
  const name = options?.name ?? fn.name;

  if (!name) {
    throw new Error(
      `Step has no name. You may need to declare your function differently or explicitly provide a name`
    );
  }

  const { tags, info, type } = options ?? {};
  return (...args: P) =>
    runStep(
      {
        name,
        type: type ?? 'task',
        tags,
        info,
        input: args.length > 1 ? args : args[0],
      },
      () => fn(...args)
    );
}

/** Explicitly provide an existing `RunContext`, for cases where the function is not causally linked to the run closely
 * enough for it to be retireved through the normal AsyncLocalStorage mechanism. */
export function runWithContext<T>(ctx: RunContext, fn: () => T): T {
  return asyncEventStorage.run(ctx, fn);
}

/** Get the current span ID, but only if recording */
function stepSpanId(span: Span | undefined) {
  return span?.isRecording() ? span.spanContext().spanId : undefined;
}

/** Record additional information about a step that is only known after starting it.
 * This data will be shallowly merged with existing step information. */
export function recordStepInfo(info: object) {
  getEventContext()?.recordStepInfo?.(info);
}

/** Record additional information about a run that is only known after starting it.
 * This data will be shallowly merged with existing run information. */
export function recordRunInfo(info: object) {
  getEventContext()?.recordRunInfo?.(info);
}

/** Set the status that will be written to the run when it finishes. */
export function setRunFinishStatus(state: string) {
  getEventContext()?.setRunFinishStatus(state);
}
