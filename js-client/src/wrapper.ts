/** Functions for wrapping the OpenAI SDK client */

import { ChronicleClientOptions } from './client.js';
import { defaultUrl, propagateSpan } from './internal.js';

/** Return a custom fetch function that calls Chronicle instead. This can be passed to
 * the OpenAI client's `fetch` parameter. */
export function fetchChronicle(options?: ChronicleClientOptions) {
  let thisFetch = options?.fetch ?? globalThis.fetch;
  const url = `${options?.url ?? defaultUrl()}/chat`;
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
    ['x-chronicle-application', defaults?.meta?.application],
    ['x-chronicle-environment', defaults?.meta?.environment],
    ['x-chronicle-organization-id', defaults?.meta?.organization_id],
    ['x-chronicle-project-id', defaults?.meta?.project_id],
    ['x-chronicle-user-id', defaults?.meta?.user_id],
    ['x-chronicle-workflow-id', defaults?.meta?.workflow_id],
    ['x-chronicle-workflow-name', defaults?.meta?.workflow_name],
    ['x-chronicle-run-id', defaults?.meta?.run_id],
    ['x-chronicle-step', defaults?.meta?.step],
    ['x-chronicle-step-index', defaults?.meta?.step_index],
    ['x-chronicle-prompt-id', defaults?.meta?.prompt_id],
    ['x-chronicle-prompt-version', defaults?.meta?.prompt_version],
    ['x-chronicle-extra-meta', JSON.stringify(defaults?.meta?.extra)],
  ]
    .filter(([_, v]) => v !== undefined)
    .map(([k, v]) => [k, v!.toString()]) as [string, string][];

  const setOverrideUrl = !defaults?.override_url;
  const setProvider = !defaults?.provider;

  return async function (requestInfo: RequestInfo, init?: RequestInit) {
    let req = new Request(url, init);
    propagateSpan(req);

    for (const [k, v] of headers) {
      req.headers.set(k, v);
    }

    if (setOverrideUrl) {
      req.headers.set(
        'x-chronicle-override-url',
        typeof requestInfo === 'string' ? requestInfo : requestInfo.url
      );
    }

    if (setProvider) {
      req.headers.set('x-chronicle-provider', 'openai');
    }

    if (token) {
      req.headers.set('Authorization', `Bearer ${token}`);
    }

    return thisFetch(req);
  };
}
