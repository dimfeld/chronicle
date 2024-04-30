/** Functions for wrapping the OpenAI SDK client */

import OpenAI from 'openai';
import { propagateSpan, type ClientOptions } from './client.js';

/** Return a custom fetch function that calls Chronicle and can be used with the OpenAI SDK */
export function openaiFetch(options: ClientOptions) {
  let client = new OpenAI();
  client.chat.completions.create;
  return async function (url: RequestInfo, init?: RequestInit) {
    let req = new Request(url, init);
    propagateSpan(req);

    // TODO update body to add the extra fields

    return fetch(req);
  };
}

export class ChronicleProxy extends OpenAI {
  // TODO
}
