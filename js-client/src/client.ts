import { Attributes, trace } from '@opentelemetry/api';
import { proxyUrl, propagateSpan, handleError } from './internal.js';
import { Stream } from './streaming.js';
import type {
  ChronicleChatRequest,
  ChronicleChatRequestNonStreaming,
  ChronicleChatRequestStreaming,
  ChronicleChatResponseStream,
  ChronicleRequestOptions,
  ChronicleChatResponseNonStreaming,
  ChronicleChatResponseStreaming,
  ChronicleRequestMetadata,
} from './types.js';
import { ChronicleEvent, fillInEvents, isWorkflowEvent, getLoggingEnabled } from './events.js';
import { getEventContext } from './runs.js';
import { getLogger } from './logger.js';
import EventEmitter from 'node:events';

export interface ChronicleClientOptions {
  /** Replace the normal fetch function with this one */
  fetch?: typeof fetch;
  /** Set the url of the proxy. If omitted, the client will try to use the `CHRONICLE_PROXY_URL` environment variable,
   * or default to http://localhost:9782. */
  url?: string;
  /** If the Chronicle proxy is behind a system that requires authentication, a bearer token to use. */
  token?: string;

  /** Set default options for requests made by this client. */
  defaults?: Omit<Partial<ChronicleRequestOptions>, 'signal'>;
}

export type NonStreamingClientFn = (
  chat: ChronicleChatRequestNonStreaming & Partial<ChronicleRequestOptions>,
  options?: ChronicleRequestOptions
) => Promise<ChronicleChatResponseNonStreaming>;
export type StreamingClientFn = (
  chat: ChronicleChatRequestStreaming & Partial<ChronicleRequestOptions>,
  options?: ChronicleRequestOptions
) => Promise<ChronicleChatResponseStream>;

export type ChronicleEventFn = (event: ChronicleEvent | ChronicleEvent[]) => Promise<void>;
export type ChronicleClient = NonStreamingClientFn &
  StreamingClientFn & {
    /** Send an event to Chronicle. All events will also be re-emitted from the client using the EventEmitter interface.
     * where the event name is 'event' and the data is the event passed to this function. */
    event: ChronicleEventFn;
    /** Create a child client which shares the same settings and EventEmitter, but with new metadata merged
     * over the existing metadata values. */
    withMetadata: (newDefaults: ChronicleRequestMetadata) => ChronicleClient;
    /** The request options used by this client. */
    requestOptions: Partial<ChronicleRequestOptions>;
  } & EventEmitter<{ event: [ChronicleEvent] }>;

/** Create a Chronicle proxy client. This returns a function which will call the Chronicle proxy */
export function createChronicleClient(options?: ChronicleClientOptions): ChronicleClient {
  let { fetch = globalThis.fetch, token, defaults = {} } = options ?? {};
  let url = proxyUrl(options?.url);
  let eventUrl = new URL('/events', url);

  let emitter = new EventEmitter<{ event: [ChronicleEvent] }>();

  const client = async (
    chat: ChronicleChatRequest & Partial<ChronicleRequestOptions>,
    options?: ChronicleRequestOptions
  ) => {
    let { signal, ...reqOptions } = options ?? {};

    let body = {
      ...client.requestOptions,
      ...chat,
      ...reqOptions,
      metadata: {
        ...client.requestOptions?.metadata,
        ...chat.metadata,
        ...reqOptions.metadata,
      },
    };

    if (!body.metadata.run_id || !body.metadata.step_id) {
      const context = getEventContext();
      body.metadata.run_id ??= context?.runId;
      body.metadata.step_id ??= context?.stepId ?? undefined;
    }

    let req = new Request(url, {
      method: 'POST',
      headers: {
        'content-type': 'application/json; charset=utf-8',
        accept: body.stream
          ? 'text/event-stream; charset=utf-8'
          : 'application/json; charset=utf-8',
      },
      body: JSON.stringify(body),
      signal,
    });

    if (token) {
      req.headers.set('Authorization', `Bearer ${token}`);
    }

    propagateSpan(req);

    let res = await fetch(req);
    if (res.ok) {
      if (chat.stream) {
        return Stream.fromSSEResponse<ChronicleChatResponseStreaming>(res, options?.signal);
      } else {
        return (await res.json()) as ChronicleChatResponseNonStreaming;
      }
    } else {
      throw new Error(await handleError(res));
    }
  };

  client.requestOptions = defaults;

  // Wire through the event emitter
  client.on = emitter.on.bind(emitter);
  client.once = emitter.once.bind(emitter);
  client.emit = emitter.emit.bind(emitter);

  client.event = (event: ChronicleEvent | ChronicleEvent[]) => {
    return sendEvent(eventUrl, event, emitter);
  };

  function updateWithMetadata(
    client: ChronicleClient,
    newMetadata: ChronicleRequestMetadata
  ): ChronicleClient {
    return {
      ...client,
      requestOptions: {
        ...client.requestOptions,
        metadata: {
          ...client.requestOptions.metadata,
          ...newMetadata,
        },
      },
    } as ChronicleClient;
  }

  client.withMetadata = (defaults: ChronicleRequestMetadata): ChronicleClient => {
    let newClient = updateWithMetadata(client as ChronicleClient, defaults);
    newClient.withMetadata = (defaults: ChronicleRequestMetadata): ChronicleClient => {
      return updateWithMetadata(newClient, defaults);
    };

    return newClient;
  };

  // @ts-expect-error
  return client;
}

function sendEvent(
  url: string | URL | undefined,
  body: ChronicleEvent | ChronicleEvent[],
  emitter: EventEmitter<{ event: [ChronicleEvent] }>
) {
  const payload = Array.isArray(body) ? body : [body];

  const span = trace.getActiveSpan();
  if (span?.isRecording() && !Array.isArray(body) && !isWorkflowEvent(body)) {
    let eventAttributes: Attributes = {};

    if (body.data) {
      for (let k in body.data) {
        let value = body.data[k as keyof typeof body.data];

        let attrKey = `llm.meta.${k}`;
        if (value && typeof value === 'object' && !Array.isArray(value)) {
          eventAttributes[attrKey] = JSON.stringify(value);
        } else {
          eventAttributes[attrKey] = value;
        }
      }
    }

    if (body.error) {
      eventAttributes['error'] =
        typeof body.error === 'object' ? JSON.stringify(body.error) : body.error;
    }

    span.addEvent(body.type as string, eventAttributes);
  }

  let eventContext = getEventContext();
  let spanCtx = span?.isRecording() ? span.spanContext() : undefined;
  fillInEvents(payload, eventContext?.runId, eventContext?.stepId, spanCtx, new Date());

  for (let event of payload) {
    emitter.emit('event', event);
  }

  if (!getLoggingEnabled()) {
    return;
  }

  let logger = getLogger(proxyUrl(url, '/events'));
  logger.enqueue(payload);
}

let defaultClient: ChronicleClient | undefined;

/** Initialize the default client with custom options. */
export function createDefaultClient(options: ChronicleClientOptions) {
  defaultClient = createChronicleClient(options);
  return defaultClient;
}

/** Return the default client, or create one if it doesn't exist. This is primarily
 * used by the run auto-instrumentation functions. */
export function getDefaultClient() {
  if (!defaultClient) {
    defaultClient = createChronicleClient();
  }
  return defaultClient;
}
