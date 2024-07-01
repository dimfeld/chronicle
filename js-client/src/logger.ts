import EventEmitter from 'node:events';
import { ChronicleEvent } from './events.js';
import { handleError } from './internal.js';

/** Map of event URL to logging queue. */
const loggerMap = new Map<string, Logger>();

type QueueState = 'idle' | 'waiting' | 'writing';

const QUEUE_THRESHOLD = 500;
const DEBOUNCE_TIME = 50;

/** Collect logs in order and send them out in batches periodically */
export class Logger {
  url: string;
  queue_state: QueueState = 'idle';
  event_queue: ChronicleEvent[] = [];

  flushed = new EventEmitter<{ flush: [] }>();

  constructor(url: string) {
    this.url = url;
  }

  enqueue(event: ChronicleEvent | ChronicleEvent[]) {
    if (Array.isArray(event)) {
      this.event_queue.push(...event);
    } else {
      this.event_queue.push(event);
    }

    if (this.event_queue.length > QUEUE_THRESHOLD && this.queue_state !== 'writing') {
      this.writeEvents();
    } else if (this.queue_state === 'idle') {
      this.queue_state = 'waiting';
      setTimeout(() => this.writeEvents(), DEBOUNCE_TIME);
    }
  }

  async writeEvents() {
    let thisBatch = this.event_queue;
    this.event_queue = [];

    this.queue_state = 'writing';

    try {
      let req = new Request(this.url, {
        method: 'POST',
        headers: {
          'content-type': 'application/json; charset=utf-8',
        },
        body: JSON.stringify({ events: thisBatch }),
      });
      let res = await fetch(req);
      if (!res.ok) {
        throw new Error(await handleError(res));
      }
    } catch (e) {
      // TODO log error to somewhere real
      console.error(e);
    } finally {
      if (this.event_queue.length) {
        const overThreshold = this.event_queue.length > QUEUE_THRESHOLD;
        let nextTime = overThreshold ? 0 : DEBOUNCE_TIME;
        this.queue_state = overThreshold ? 'writing' : 'waiting';
        setTimeout(() => this.writeEvents(), nextTime);
      } else {
        this.flushed.emit('flush');
        this.queue_state = 'idle';
      }
    }
  }

  /** Wait for all existing events to be flushed. */
  flushEvents() {
    if (!this.event_queue.length) {
      return;
    }

    return new Promise<void>((resolve) => {
      this.flushed.once('flush', () => resolve());
    });
  }
}

export function getLogger(url: string | URL): Logger {
  url = url.toString();
  let existing = loggerMap.get(url);
  if (existing) {
    return existing;
  }

  let logger = new Logger(url);
  loggerMap.set(url, logger);
  return logger;
}

/** Wait for all loggers to flush their events. This can be used to ensure that the
 * process stays alive while events are still being sent, in some environments that
 * may not stay alive such as Bun tests. */
export async function flushEvents() {
  const flushes = Array.from(loggerMap.values()).map((l) => l.flushEvents());
  await Promise.all(flushes);
}
