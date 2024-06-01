import type * as openai from 'openai';

/** A chat request. This is the same as the arguments to OpenAI's chat.completions.create function. */
export type ChronicleChatRequest<STREAMING extends boolean> = (STREAMING extends true
  ? openai.OpenAI.Chat.ChatCompletionCreateParamsStreaming
  : openai.OpenAI.Chat.ChatCompletionCreateParamsNonStreaming) & {
  max_tokens: number;
};

export interface ChronicleModelAndProvider {
  /** The model to use */
  model: string;
  /** The name of the provider to use. */
  provider: string;
  /** An API key to pass to the provider. */
  api_key?: string;
  /** A reference to an API key object that was configured in the proxy. */
  api_key_name?: string;
}

export type ChronicleRepeatBackoffBehavior =
  | {
      /** Use the initial backoff duration for additional retries as well. */
      type: 'constant';
    }
  | {
      /** Increase backoff time exponentially. */
      type: 'exponential';
      /** Multiply the previous backoff time by this number on every retry. */
      multiplier: number;
    }
  | {
      /** Add a fixed amount to the backoff time, in milliseconds, on every retry. */
      type: 'additive';
      /** The number of milliseconds to add. */
      amount: number;
    };

export interface ChronicleRetryOptions {
  /** The amount of time to wait on the first backoff, in milliseconds. Defaults to 500ms */
  initial_backoff?: number;
  /** How to increase the backoff time on multiple retries. The default is to multiply the previous backoff time by 2. */
  increase?: ChronicleRepeatBackoffBehavior;
  /** The number of times to try the request, including the first try. Defaults to 4. */
  max_tries?: number;
  /** The maximum amount of jitter to add to the backoff time, in milliseconds. Defaults to 100ms. */
  jitter?: number;
  /** The maximum amount of time to wait between tries, in milliseconds. Defaults to 5000ms. */
  max_backoff?: number;
  /** If a rate limit response asks us to wait longer than `max_backoff`, just fail instead of waiting
   * if we don't have any other models to fall back to.
   * Defaults to true. */
  fail_if_rate_limit_exceeds_max_backoff?: boolean;
}

export interface ChronicleRequestMetadata {
  /** application making this call. This can also be set by passing the
    chronicle-application HTTP header. */
  application?: string;
  /** The environment the application is running in. This can also be set by passing the
     x-chronicle-environment HTTP header. */
  environment?: string;
  /** The organization related to the request. This can also be set by passing the
     x-chronicle-organization-id HTTP header. */
  organization_id?: string;
  /** The project related to the request. This can also be set by passing the
     x-chronicle-project-id HTTP header. */
  project_id?: string;
  /** The id of the user that triggered the request. This can also be set by passing the
     x-chronicle-user-id HTTP header. */
  user_id?: string;
  /** The id of the workflow that this request belongs to. This can also be set by passing the
     x-chronicle-workflow-id HTTP header. */
  workflow_id?: string;
  /** A readable name of the workflow that this request belongs to. This can also be set by
     passing the x-chronicle-workflow-name HTTP header. */
  workflow_name?: string;
  /** The id of of the specific run that this request belongs to. This can also be set by
     passing the x-chronicle-run-id HTTP header. */
  run_id?: string;
  /** The name of the workflow step. This can also be set by passing the
     x-chronicle-step HTTP header. */
  step?: string;
  /** The index of the step within the workflow. This can also be set by passing the
     x-chronicle-step-index HTTP header. */
  step_index?: number;
  /** A unique ID for this prompt. This can also be set by passing the
     x-chronicle-prompt-id HTTP header. */
  prompt_id?: string;
  /** The version of this prompt. This can also be set by passing the
     x-chronicle-prompt-version HTTP header. */
  prompt_version?: number;

  /** Any other metadata to include. When passing this in the request body, any unknown fields
     are collected here. This can also be set by passing a JSON object to the
     x-chronicle-extra-meta HTTP header. */
  extra?: Record<string, number | string | boolean>;
}

export interface ChronicleRequestOptions {
  /** Override the model from the request body or select an alias.
     This can also be set by passing the x-chronicle-model HTTP header. */
  model?: string;
  /** Choose a specific provider to use. This can also be set by passing the
     x-chronicle-provider HTTP header. */
  provider?: string;
  /** Force the provider to use a specific URL instead of its default. This can also be set
     by passing the x-chronicle-override-url HTTP header. */
  override_url?: string;
  /** An API key to pass to the provider. This can also be set by passing the
     x-chronicle-provider-api-key HTTP header. */
  api_key?: string;
  /** Supply multiple provider/model choices, which will be tried in order.
   If this is provided, the `model`, `provider`, and `api_key` fields are ignored in favor of those given here.
   This field can not reference model aliases.
   This can also be set by passing the x-chronicle-models HTTP header using JSON syntax. */
  models?: Array<ChronicleModelAndProvider>;
  /** When using `models` to supply multiple choices, start at a random choice instead of the
     first one. This can also be set by passing the x-chronicle-random-choice HTTP header. */
  random_choice?: boolean;
  /** The timeout, in milliseconds.
     This can also be set by passing the x-chronicle-timeout HTTP header. */
  timeout?: number;
  /** Customize the retry behavior. This can also be set by passing the
     x-chronicle-retry HTTP header. */
  retry?: ChronicleRetryOptions;

  metadata?: ChronicleRequestMetadata;

  /** An AbortSignal for this request */
  signal?: AbortSignal;
}

export interface ChronicleResponseMeta {
  /** A UUID assigned by Chronicle to the request, which is linked to the logged information.
     This is different from the `id` returned at the top level of the `ChronicleChatResponse`, which
     comes from the provider itself. */
  id: string;
  /** Which provider was useed for the request. */
  provider: string;
  /** Any provider-specific metadata returned from the provider that doesn't fit in with
   * the regular fields. */
  response_meta?: object;
  /** True if this request had to retry or fallback from the default model due to rate limiting. */
  was_rate_limited: boolean;
}

export interface SingleChronicleChatResponse extends openai.OpenAI.Chat.ChatCompletion {
  meta: ChronicleResponseMeta;
}

// TODO this isn't quite right, need to account for delta types
export type StreamingChronicleChatResponse = SingleChronicleChatResponse;

export type ChronicleChatResponseStream = AsyncIterable<SingleChronicleChatResponse>;

export type ChronicleChatResponse<STREAMING extends boolean> = STREAMING extends true
  ? ChronicleChatResponseStream
  : SingleChronicleChatResponse;
