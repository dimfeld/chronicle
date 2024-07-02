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
} from './types.js';
import { ChronicleEvent, fillInEvents, isGenericEvent, getLoggingEnabled } from './events.js';
import { getEventContext } from './runs.js';
import { getLogger } from './logger.js';

export interface ChronicleClientOptions {
  /** Replace the normal fetch function with this one */
  fetch?: typeof fetch;
  /** Set the url of the proxy. If omitted, the client will try to use the `CHRONICLE_PROXY_URL` environment variable,
   * or default to http://localhost:9782. */
  url?: string;
  /** If the Chronicle proxy is behind a system that requires authentication, a bearer token to use. */
  token?: string;

  /** Set default options for requests made by this client. */
  defaults?: Partial<ChronicleRequestOptions>;
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
  StreamingClientFn & { event: ChronicleEventFn; metadata: Partial<ChronicleRequestOptions> };

/** Create a Chronicle proxy client. This returns a function which will call the Chronicle proxy */
export function createChronicleClient(options?: ChronicleClientOptions): ChronicleClient {
  let { fetch = globalThis.fetch, token, defaults = {} } = options ?? {};
  let url = proxyUrl(options?.url);
  let eventUrl = new URL('/events', url);

  const client = async (
    chat: ChronicleChatRequest & Partial<ChronicleRequestOptions>,
    options?: ChronicleRequestOptions
  ) => {
    let { signal, ...reqOptions } = options ?? {};

    let body = {
      ...client.metadata,
      ...chat,
      ...reqOptions,
      metadata: {
        ...client.metadata,
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

  client.metadata = defaults;

  client.event = (event: ChronicleEvent | ChronicleEvent[]) => {
    return sendEvent(eventUrl, event);
  };

  // @ts-expect-error
  return client;
}

export function sendEvent(url: string | URL | undefined, body: ChronicleEvent | ChronicleEvent[]) {
  if (!getLoggingEnabled()) {
    return;
  }

  const payload = Array.isArray(body) ? body : [body];

  const span = trace.getActiveSpan();
  if (span?.isRecording() && !Array.isArray(body) && isGenericEvent(body)) {
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

  let logger = getLogger(proxyUrl(url, '/events'));
  logger.enqueue(payload);
}

let defaultClient: ChronicleClient | undefined;

/** Initialize the default client. */
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
