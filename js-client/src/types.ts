import type * as openai from 'openai';

/** A chat request. This is the same as the arguments to OpenAI's chat.completions.create function. */
export type ChronicleChatRequest = openai.OpenAI.Chat.ChatCompletionCreateParamsNonStreaming;

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
}

export interface ChronicleResponseMeta {
  /** A UUID linked to the logged information about the request. */
  id: string;
  /** Any provider-specific metadata returned from the provider that doesn't fit in with
   * the regular fields. */
  response_meta?: object;
  /** True if this request had to retry or fallback from the default model due to rate limiting. */
  was_rate_limited: boolean;
}

export interface ChronicleChatResponse extends openai.OpenAI.Chat.ChatCompletion {
  meta: ChronicleResponseMeta;
}
