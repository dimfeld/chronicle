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

export function proxyUrl(configured?: string | URL, path = '/chat') {
  let url = new URL(configured || process.env.CHRONICLE_PROXY_URL || 'http://localhost:9782');
  if (url.pathname.length <= 1) {
    url.pathname = path;
  }

  return url;
}

export async function handleError(res: Response) {
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
