import { Attributes, trace } from '@opentelemetry/api';
import { proxyUrl, propagateSpan } from './internal.js';
import { Stream } from './streaming.js';
import type {
  ChronicleChatRequest,
  ChronicleChatRequestNonStreaming,
  ChronicleChatRequestStreaming,
  ChronicleChatResponse,
  ChronicleChatResponseStream,
  ChronicleRequestMetadata,
  ChronicleRequestOptions,
  ChronicleChatResponseNonStreaming,
  ChronicleChatResponseStreaming,
} from './types.js';
import { ChronicleEvent, isGenericEvent } from './events.js';

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
  StreamingClientFn & { event: ChronicleEventFn };

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
      ...defaults,
      ...chat,
      ...reqOptions,
      metadata: {
        ...defaults.metadata,
        ...chat.metadata,
        ...reqOptions.metadata,
      },
    };

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

  client.event = (event: ChronicleEvent | ChronicleEvent[]) => {
    return sendEvent(eventUrl, event);
  };

  // @ts-expect-error
  return client;
}

export async function sendEvent(
  url: string | URL | undefined,
  body: ChronicleEvent | ChronicleEvent[]
): Promise<void> {
  const path = Array.isArray(body) ? '/events' : '/event';
  const payload = Array.isArray(body) ? body : [body];
  let req = new Request(proxyUrl(url, path), {
    method: 'POST',
    headers: {
      'content-type': 'application/json; charset=utf-8',
    },
    body: JSON.stringify({ events: payload }),
  });

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

  let res = await fetch(req);
  if (!res.ok) {
    throw new Error(await handleError(res));
  }
}

async function handleError(res: Response) {
  let message = '';
  const err = await res.text();
  try {
    const { error } = JSON.parse(err);

    let errorBody = error?.details.body;
    if (errorBody?.error) {
      errorBody = errorBody.error;
    }

    if (errorBody) {
      message = typeof errorBody === 'string' ? errorBody : JSON.stringify(errorBody);
    }
  } catch (e) {
    message = err;
  }

  // TODO The api returns a bunch of other error details, so integrate them here.
  return message || 'An error occurred';
}
