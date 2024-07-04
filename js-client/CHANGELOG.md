# chronicle-proxy JavaScript client changelog

## 0.4.1 

- Make `run_id` and `step_id` optional in `GenericEvent` since they will be filled in automatically when possible.

## 0.4.0

- When running a step outside of a run, automatically wrap it in a run.
- Allow disabling all event logging in an application.
- Allow explicitly passing a run context to `runStep`, in case it can not be retrieved from the normal `AsyncLocalStorage` context.
- The client is now also an `EventEmitter`, and will emit any events passed to `client.event()`.
- Add a `withMetadata` function to the client, which returns a child client with updated default metadata values in the requests. This client shares the same `EventEmitter` and other settings with its parent.
- Allow passing a custom Chronicle client instance to `runStep`.
- Allow passing metadata to `startRun` or `runStep`, which will automatically create a child Chronicle client using `withMetadata`.

## 0.3.0

- Support submitting runs and step trace data to Chronicle
- Add an event queue to ensure that events are submitted in the order they occur.

## 0.2.0

- Support streaming

## 0.1.1

- Allow sending arbitrary events to the Chronicle proxy

## 0.1.0

- Initial release



