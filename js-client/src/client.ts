import { Attributes, trace } from '@opentelemetry/api';
import { proxyUrl, propagateSpan } from './internal.js';
import type {
  ChronicleChatRequest,
  ChronicleChatResponse,
  ChronicleRequestMetadata,
  ChronicleRequestOptions,
} from './types.js';

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

export type ChronicleEventFn = (event: ChronicleEvent) => Promise<{ id: string }>;
export type ChronicleClient = ((
  chat: ChronicleChatRequest & Partial<ChronicleRequestOptions>,
  options?: ChronicleRequestOptions
) => Promise<ChronicleChatResponse>) & { event: ChronicleEventFn };

/** Create a Chronicle proxy client. This returns a function which will call the Chronicle proxy */
export function createChronicleClient(options?: ChronicleClientOptions): ChronicleClient {
  let { fetch = globalThis.fetch, token, defaults = {} } = options ?? {};
  let url = proxyUrl(options?.url);
  let eventUrl = new URL('/event', url);

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
      return (await res.json()) as ChronicleChatResponse;
    } else {
      throw new Error(await handleError(res));
    }
  };

  client.event = (event: ChronicleEvent) => {
    const thisEvent = {
      ...event,
      metadata: {
        ...defaults.metadata,
        ...event.metadata,
      },
      url: eventUrl,
    };

    return sendEvent(thisEvent);
  };

  return client;
}

export interface ChronicleEvent {
  /** The type of event */
  type: string;
  /** Data associated with the event */
  data?: object;
  /** Error data for the event, if it represents an error. */
  error?: any;
  /** Additional metadata for the event */
  metadata?: Omit<ChronicleRequestMetadata, 'extra'>;
}

export interface ChronicleSendEventOptions extends ChronicleEvent {
  /** The URL to send the event to */
  url?: string | URL;
}

export async function sendEvent(event: ChronicleSendEventOptions): Promise<{ id: string }> {
  const { url, ...body } = event;
  let req = new Request(proxyUrl(event.url, '/event'), {
    method: 'POST',
    headers: {
      'content-type': 'application/json; charset=utf-8',
    },
    body: JSON.stringify(body),
  });

  const span = trace.getActiveSpan();
  if (span?.isRecording) {
    let eventAttributes: Attributes = {};

    if (body.metadata) {
      for (let k in body.metadata) {
        eventAttributes[`llm.meta.${k}`] = body.metadata[k as keyof typeof body.metadata];
      }
    }

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

    console.log(eventAttributes);

    span.addEvent(body.type, eventAttributes);
  }

  let res = await fetch(req);
  if (res.ok) {
    const result = (await res.json()) as { id: string };
    return result;
  } else {
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

  console.error(err);
  // TODO The api returns a bunch of other error details, so integrate them here.
  return message || 'An error occurred';
}
