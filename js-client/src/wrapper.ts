/** Functions for wrapping the OpenAI SDK client */

import { ChronicleClientOptions } from './client.js';
import { proxyUrl, propagateSpan } from './internal.js';

/** Return a custom fetch function that calls Chronicle instead. This can be passed to
 * the OpenAI client's `fetch` parameter. */
export function fetchChronicle(options?: ChronicleClientOptions) {
  let thisFetch = options?.fetch ?? globalThis.fetch;
  const url = proxyUrl(options?.url);
  const { token, defaults } = options ?? {};

  const headers = [
    ['x-chronicle-provider-api-key', defaults?.api_key],
    ['x-chronicle-provider', defaults?.provider],
    ['x-chronicle-model', defaults?.model],
    ['x-chronicle-override-url', defaults?.override_url],
    ['x-chronicle-api-key', defaults?.api_key],
    ['x-chronicle-models', JSON.stringify(defaults?.models)],
    ['x-chronicle-random-choice', defaults?.random_choice],
    ['x-chronicle-retry', JSON.stringify(defaults?.retry)],
    ['x-chronicle-timeout', defaults?.timeout],
    ['x-chronicle-application', defaults?.metadata?.application],
    ['x-chronicle-environment', defaults?.metadata?.environment],
    ['x-chronicle-organization-id', defaults?.metadata?.organization_id],
    ['x-chronicle-project-id', defaults?.metadata?.project_id],
    ['x-chronicle-user-id', defaults?.metadata?.user_id],
    ['x-chronicle-workflow-id', defaults?.metadata?.workflow_id],
    ['x-chronicle-workflow-name', defaults?.metadata?.workflow_name],
    ['x-chronicle-run-id', defaults?.metadata?.run_id],
    ['x-chronicle-step', defaults?.metadata?.step],
    ['x-chronicle-step-index', defaults?.metadata?.step_index],
    ['x-chronicle-prompt-id', defaults?.metadata?.prompt_id],
    ['x-chronicle-prompt-version', defaults?.metadata?.prompt_version],
    ['x-chronicle-extra-meta', JSON.stringify(defaults?.metadata?.extra)],
  ]
    .filter(([_, v]) => v !== undefined)
    .map(([k, v]) => [k, v!.toString()]) as [string, string][];

  return async function (requestInfo: RequestInfo, init?: RequestInit) {
    // First just coalesce requestInfo and init into a single request
    let req = new Request(requestInfo, init);
    // If Chronicle updates to support other types of endpoints then we should look at the URL to decide
    // which endpoint it's trying to call.
    req = new Request(url, req);

    propagateSpan(req);

    for (const [k, v] of headers) {
      req.headers.set(k, v);
    }

    if (token) {
      req.headers.set('Authorization', `Bearer ${token}`);
    }

    return thisFetch(req);
  };
}
