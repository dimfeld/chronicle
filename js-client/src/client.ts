import { context, propagation, trace } from '@opentelemetry/api';
import { W3CTraceContextPropagator } from '@opentelemetry/core';

export interface ClientOptions {
  /** Replace the normal fetch function with this one */
  fetch?: typeof fetch;
  url: string;
}

export interface RequestOptions {}

export function propagateSpan(req: Request) {
  let propagator = new W3CTraceContextPropagator();

  const setter = {
    set: (req: Request, headerName: string, headerValue: string) => {
      req.headers.set(headerName, headerValue);
    },
  };

  propagator.inject(context.active(), req, setter);
}
