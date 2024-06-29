/** Represents a UUID as a string */
export type Uuid = string;

/** Starts a run in a workflow */
export interface RunStartEvent {
  /** The type of the event */
  type: 'run:start';
  /** The unique identifier for the run. UUIDv7 recommended */
  id: Uuid;
  /** The name of the run */
  name: string;
  /** Optional description of the run */
  description?: string;
  /** Optional application associated with the run */
  application?: string;
  /** Optional environment in which the run is executed */
  environment?: string;
  /** Optional input data for the run */
  input?: object;
  /** OpenTelemetry trace ID for distributed tracing */
  trace_id?: string;
  /** OpenTelemetry span ID for distributed tracing */
  span_id?: string;
  /** Array of tags associated with the run */
  tags?: string[];
  /** Optional additional information about the run */
  info?: object;
  /** Optional timestamp of when the event occurred */
  time?: Date;
}

/** Updates a run in a workflow */
export interface RunUpdateEvent {
  /** The type of the event */
  type: 'run:update';
  /** The unique identifier for the run */
  id: Uuid;
  /** Optional new status of the run */
  status?: string;
  /** Optional output data from the run */
  output?: object;
  /** Optional additional information about the run */
  info?: object;
  /** Optional timestamp of when the event occurred */
  time?: Date;
}

/** Step event in a workflow */
export interface StepEventData<TYPE, T> {
  /** The type of the step event */
  type: TYPE;
  /** The unique identifier for the step */
  step_id: Uuid;
  /** The unique identifier for the run containing this step */
  run_id: Uuid;
  /** Optional timestamp of when the event occurred */
  time?: Date;
  /** The data associated with this step event */
  data: T;
}

/** Data for the start of a step */
export interface StepStartData {
  /** The type of the step */
  type: string;
  /** Optional name of the step */
  name?: string;
  /** Optional unique identifier of the parent step */
  parent_step?: Uuid;
  /** Optional span ID for distributed tracing */
  span_id?: string;
  /** Array of tags associated with the step */
  tags?: string[];
  /** Optional additional information about the step */
  info?: object;
  /** Input data for the step */
  input: object;
}

/** Data for the end of a step */
export interface StepEndData {
  /** Output data from the step */
  output: object;
  /** Optional additional information about the step completion. This will be merged with the info from the step start event */
  info?: object;
}

/** Data for an error in a step */
export interface ErrorData {
  /** Error information */
  error: object;
}

/** Data for updating the state of a step */
export interface StepStateData {
  /** The current state of the step */
  state: string;
}

/** Represents a step start event */
type StepStartEvent = StepEventData<'step:start', StepStartData>;

/** Represents a step end event */
type StepEndEvent = StepEventData<'step:end', StepEndData>;

/** Represents a step error event */
type StepErrorEvent = StepEventData<'step:error', ErrorData>;

/** Represents a step state change event */
type StepStateEvent = StepEventData<'step:state', StepStateData>;

export type WorkflowEventTypes =
  | 'run:start'
  | 'run:update'
  | 'step:start'
  | 'step:end'
  | 'step:error'
  | 'step:state';

/** Represents a generic event in the system */
export interface GenericEvent {
  /** The type of the event */
  type: Omit<string, WorkflowEventTypes>;
  /** Data associated with the event */
  data?: object;
  /** Optional error information */
  error?: object;
  /** The ID for the run associated with this event */
  run_id: Uuid;
  /** The ID for the step associated with this event */
  step_id: Uuid;
  /** Timestamp of when the event occurred */
  time?: Date;
}

/** Represents any type of event that can be submitted to Chronicle */
export type ChronicleEvent =
  | RunStartEvent
  | RunUpdateEvent
  | StepStartEvent
  | StepEndEvent
  | StepErrorEvent
  | StepStateEvent
  | GenericEvent;

export function isGenericEvent(event: ChronicleEvent): event is GenericEvent {
  // If the event type is not any of the known types, it's generic.
  return !(
    event.type === 'run:start' ||
    event.type === 'run:update' ||
    event.type === 'step:start' ||
    event.type === 'step:end' ||
    event.type === 'step:error' ||
    event.type === 'step:state'
  );
}
