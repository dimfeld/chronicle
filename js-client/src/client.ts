import { defaultUrl, propagateSpan } from './internal.js';
import type {
  ChronicleChatRequest,
  ChronicleChatResponse,
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

export type ChronicleClient = (
  chat: ChronicleChatRequest & Partial<ChronicleRequestOptions>,
  options?: ChronicleRequestOptions
) => Promise<ChronicleChatResponse>;

/** Create a Chronicle proxy client. This returns a function which will call the Chronicle proxy */
export function createChronicleClient(options: ChronicleClientOptions): ChronicleClient {
  let { fetch = globalThis.fetch, url, token, defaults = {} } = options;
  if (!url) {
    url = defaultUrl();
  }

  return async (
    chat: ChronicleChatRequest & Partial<ChronicleRequestOptions>,
    options?: ChronicleRequestOptions
  ) => {
    let { signal, ...reqOptions } = options ?? {};

    let body = {
      ...defaults,
      ...chat,
      ...reqOptions,
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
    let responseBody = await res.json();
    if (res.ok) {
      return responseBody as ChronicleChatResponse;
    } else {
      // TODO The api returns a bunch of other error details, so integrate them here.
      throw new Error(responseBody?.error?.message || 'An error occurred');
    }
  };
}
