import { context } from '@opentelemetry/api';
import { W3CTraceContextPropagator } from '@opentelemetry/core';

export function propagateSpan(req: Request) {
  let propagator = new W3CTraceContextPropagator();

  const setter = {
    set: (req: Request, headerName: string, headerValue: string) => {
      req.headers.set(headerName, headerValue);
    },
  };

  propagator.inject(context.active(), req, setter);
}

export function proxyUrl(configured?: string) {
  let url = new URL(configured || process.env.CHRONICLE_PROXY_URL || 'http://localhost:9782');
  if (url.pathname.length <= 1) {
    url.pathname = '/chat';
  }

  return url;
}
