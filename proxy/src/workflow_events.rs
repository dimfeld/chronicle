use chrono::DateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ProxyRequestMetadata;

/// An event that updates a run or step in a workflow.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowEvent {
    /// The event's type and data
    #[serde(flatten)]
    pub data: WorkflowEventData,
    /// A UUIDv7 for the entire run
    pub run_id: Uuid,
    /// The DAG or state machine that this event belongs to
    pub source: String,
    /// The node within the workflow that this event belongs to
    pub source_node: String,
    /// A UUIDv7 identifying the step the event belongs to
    pub step: Option<Uuid>,
    pub meta: Option<ProxyRequestMetadata>,
    pub start_time: Option<DateTime<chrono::Utc>>,
    pub end_time: Option<DateTime<chrono::Utc>>,
}

/// Type-specific data for an event.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WorkflowEventData {
    /// Event data for the start of a DAG.
    #[serde(rename = "dag:start")]
    DagStart(StepStartData),
    /// Event data for a DAG error.
    #[serde(rename = "dag:error")]
    DagError(ErrorData),
    /// Event data for the finish of a DAG.
    #[serde(rename = "dag:finish")]
    DagFinish(StepEndData),
    /// Event data for the start of a DAG node.
    #[serde(rename = "dag:node_start")]
    DagNodeStart(DagNodeStartData),
    /// Event data for the finish of a DAG node.
    #[serde(rename = "dag:node_finish")]
    DagNodeFinish(StepEndData),
    /// Event data for a DAG node error.
    #[serde(rename = "dag:node_error")]
    DagNodeError(ErrorData),
    /// Event data for a DAG node state change.
    #[serde(rename = "dag:node_state")]
    DagNodeState(DagNodeStateData),
    /// Event data for the start of a state machine.
    #[serde(rename = "state_machine:start")]
    StateMachineStart(StepStartData),
    /// Event data for a state machine status update.
    #[serde(rename = "state_machine:status")]
    StateMachineStatus(StateMachineStatusData),
    /// Event data for the start of a state machine node.
    #[serde(rename = "state_machine:node_start")]
    StateMachineNodeStart(StateMachineNodeStartData),
    /// Event data for the finish of a state machine node.
    #[serde(rename = "state_machine:node_finish")]
    StateMachineNodeFinish(StepEndData),
    /// Event data for a state machine transition.
    #[serde(rename = "state_machine:transition")]
    StateMachineTransition(StateMachineTransitionData),
    /// Event data for the start of a step.
    #[serde(rename = "step:start")]
    StepStart(StepStartData),
    /// Event data for the end of a step.
    #[serde(rename = "step:end")]
    StepEnd(StepEndData),
    /// Event data for a step error.
    #[serde(rename = "step:error")]
    StepError(ErrorData),
}

/// Data structure for the start of a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepStartData {
    /// UUID of the parent step, if any.
    pub parent_step: Option<Uuid>,
    /// Span ID for tracing purposes.
    pub span_id: Option<String>,
    /// Tags associated with the step.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Additional information about the step.
    pub info: Option<serde_json::Value>,
    /// Input data for the step.
    pub input: serde_json::Value,
}

/// Data structure for the end of a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEndData {
    /// Output data from the step.
    pub output: serde_json::Value,
    /// Additional information about the step completion.
    pub info: Option<serde_json::Value>,
}

/// Data structure for error information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    /// Error message or description.
    pub error: String,
}

/// Data structure for the start of a DAG node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNodeStartData {
    /// Embedded step start data.
    #[serde(flatten)]
    pub step_start_data: StepStartData,
    /// Input data specific to the DAG node.
    pub input: serde_json::Value,
    /// Context information for the DAG node.
    pub context: serde_json::Value,
}

/// Data structure for DAG node state information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNodeStateData {
    /// Current state of the DAG node.
    pub state: String,
}

/// Data structure for state machine status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineStatusData {
    /// Current status of the state machine.
    pub status: String,
}

/// Data structure for the start of a state machine node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineNodeStartData {
    /// Embedded step start data.
    #[serde(flatten)]
    pub step_start_data: StepStartData,
    /// Context information for the state machine node when this state started.
    pub context: serde_json::Value,
    /// Optional event data associated with the state starting.
    pub event: Option<StateMachineEventData>,
}

/// An event that was sent to a state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineEventData {
    /// Type of the event.
    pub r#type: String,
    /// Data associated with the event.
    pub data: serde_json::Value,
}

/// Data structure for state machine transition information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineTransitionData {
    /// Optional event data associated with the transition.
    pub event: Option<StateMachineEventData>,
    /// Input data for the transition.
    pub input: serde_json::Value,
    /// Output data from the transition.
    pub output: serde_json::Value,
    /// Source state of the transition.
    pub from: String,
    /// Destination state of the transition.
    pub to: String,
    /// Indicates if this is a final state of the state machine.
    pub final_state: bool,
}
