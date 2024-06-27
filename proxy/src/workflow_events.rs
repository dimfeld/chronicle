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
    #[serde(rename = "type")]
    pub typ: String,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_workflow_event_dag_start_deserialization() {
        let json_data = json!({
            "type": "dag:start",
            "data": {
                "parent_step": null,
                "span_id": "span-123",
                "tags": ["tag1", "tag2"],
                "info": {"key": "value"},
                "input": {"param": "value"}
            },
            "run_id": "01234567-89ab-cdef-0123-456789abcdef",
            "source": "main_dag",
            "source_node": "start_node",
            "step": "fedcba98-7654-3210-fedc-ba9876543210",
            "meta": null,
            "start_time": "2023-06-27T12:34:56Z",
            "end_time": null
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(event.source, "main_dag");
        assert_eq!(event.source_node, "start_node");
        assert_eq!(
            event.step.unwrap().to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert!(event.meta.is_none());
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T12:34:56+00:00"
        );
        assert!(event.end_time.is_none());

        if let WorkflowEventData::DagStart(data) = event.data {
            assert!(data.parent_step.is_none());
            assert_eq!(data.span_id.unwrap(), "span-123");
            assert_eq!(data.tags, vec!["tag1", "tag2"]);
            assert_eq!(data.info.unwrap(), json!({"key": "value"}));
            assert_eq!(data.input, json!({"param": "value"}));
        } else {
            panic!("Expected DagStart event");
        }
    }

    #[test]
    fn test_workflow_event_dag_node_error_deserialization() {
        let json_data = json!({
            "type": "dag:node_error",
            "data": {
                "error": "An error occurred"
            },
            "run_id": "01234567-89ab-cdef-0123-456789abcdef",
            "source": "main_dag",
            "source_node": "error_node",
            "step": null,
            "start_time": "2023-06-27T12:34:56Z",
            "end_time": "2023-06-27T12:35:00Z"
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(event.source, "main_dag");
        assert_eq!(event.source_node, "error_node");
        assert!(event.step.is_none());
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T12:34:56+00:00"
        );
        assert_eq!(
            event.end_time.unwrap().to_rfc3339(),
            "2023-06-27T12:35:00+00:00"
        );

        if let WorkflowEventData::DagNodeError(data) = event.data {
            assert_eq!(data.error, "An error occurred");
        } else {
            panic!("Expected DagNodeError event");
        }
    }

    #[test]
    fn test_workflow_event_state_machine_transition_deserialization() {
        let json_data = json!({
            "type": "state_machine:transition",
            "data": {
                "event": {
                    "type": "user_input",
                    "data": {"user_choice": "option_a"}
                },
                "input": {"state": "initial"},
                "output": {"result": "processed"},
                "from": "state_a",
                "to": "state_b",
                "final_state": false
            },
            "run_id": "01234567-89ab-cdef-0123-456789abcdef",
            "source": "main_state_machine",
            "source_node": "transition_node",
            "step": "fedcba98-7654-3210-fedc-ba9876543210",
            "meta": null,
            "start_time": null,
            "end_time": null
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(event.source, "main_state_machine");
        assert_eq!(event.source_node, "transition_node");
        assert_eq!(
            event.step.unwrap().to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert!(event.meta.is_none());
        assert!(event.start_time.is_none());
        assert!(event.end_time.is_none());

        if let WorkflowEventData::StateMachineTransition(data) = event.data {
            assert_eq!(data.event.as_ref().unwrap().typ, "user_input");
            assert_eq!(
                data.event.as_ref().unwrap().data,
                json!({"user_choice": "option_a"})
            );
            assert_eq!(data.input, json!({"state": "initial"}));
            assert_eq!(data.output, json!({"result": "processed"}));
            assert_eq!(data.from, "state_a");
            assert_eq!(data.to, "state_b");
            assert!(!data.final_state);
        } else {
            panic!("Expected StateMachineTransition event");
        }
    }

    #[test]
    fn test_workflow_event_step_end_deserialization() {
        let json_data = json!({
            "type": "step:end",
            "data": {
                "output": {"result": "success"},
                "info": {"duration": 1000}
            },
            "run_id": "01234567-89ab-cdef-0123-456789abcdef",
            "source": "main_workflow",
            "source_node": "end_step",
            "step": "fedcba98-7654-3210-fedc-ba9876543210",
            "meta": null,
            "start_time": "2023-06-27T12:34:56Z",
            "end_time": "2023-06-27T12:35:56Z"
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(event.source, "main_workflow");
        assert_eq!(event.source_node, "end_step");
        assert_eq!(
            event.step.unwrap().to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert!(event.meta.is_none());
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T12:34:56+00:00"
        );
        assert_eq!(
            event.end_time.unwrap().to_rfc3339(),
            "2023-06-27T12:35:56+00:00"
        );

        if let WorkflowEventData::StepEnd(data) = event.data {
            assert_eq!(data.output, json!({"result": "success"}));
            assert_eq!(data.info.unwrap(), json!({"duration": 1000}));
        } else {
            panic!("Expected StepEnd event");
        }
    }

    #[test]
    fn test_workflow_event_dag_node_start_deserialization() {
        let json_data = json!({
            "type": "dag:node_start",
            "data": {
                "parent_step": "01234567-89ab-cdef-0123-456789abcdef",
                "span_id": "span-456",
                "tags": ["dag", "node"],
                "info": {"node_type": "task"},
                "input": {"task_param": "value"},
                "context": {"dag_context": "some_context"}
            },
            "run_id": "fedcba98-7654-3210-fedc-ba9876543210",
            "source": "main_dag",
            "source_node": "task_node",
            "step": "abcdef01-2345-6789-abcd-ef0123456789",
            "meta": null,
            "start_time": "2023-06-27T13:00:00Z",
            "end_time": null
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert_eq!(event.source, "main_dag");
        assert_eq!(event.source_node, "task_node");
        assert_eq!(
            event.step.unwrap().to_string(),
            "abcdef01-2345-6789-abcd-ef0123456789"
        );
        assert!(event.meta.is_none());
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T13:00:00+00:00"
        );
        assert!(event.end_time.is_none());

        if let WorkflowEventData::DagNodeStart(data) = event.data {
            assert_eq!(
                data.step_start_data.parent_step.unwrap().to_string(),
                "01234567-89ab-cdef-0123-456789abcdef"
            );
            assert_eq!(data.step_start_data.span_id.unwrap(), "span-456");
            assert_eq!(data.step_start_data.tags, vec!["dag", "node"]);
            assert_eq!(
                data.step_start_data.info.unwrap(),
                json!({"node_type": "task"})
            );
            assert_eq!(data.step_start_data.input, json!({"task_param": "value"}));
            assert_eq!(data.context, json!({"dag_context": "some_context"}));
        } else {
            panic!("Expected DagNodeStart event");
        }
    }

    #[test]
    fn test_workflow_event_state_machine_node_start_deserialization() {
        let json_data = json!({
            "type": "state_machine:node_start",
            "data": {
                "parent_step": null,
                "span_id": "span-789",
                "tags": ["state_machine", "node"],
                "info": {"state": "initial"},
                "input": {"init_param": "value"},
                "context": {"machine_context": "some_context"},
                "event": {
                    "type": "init",
                    "data": {"init_data": "start"}
                }
            },
            "run_id": "12345678-90ab-cdef-1234-567890abcdef",
            "source": "main_state_machine",
            "source_node": "initial_state",
            "step": "98765432-10fe-dcba-9876-543210fedcba",
            "meta": {"project_id": "req-456"},
            "start_time": "2023-06-27T14:00:00Z",
            "end_time": null
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "12345678-90ab-cdef-1234-567890abcdef"
        );
        assert_eq!(event.source, "main_state_machine");
        assert_eq!(event.source_node, "initial_state");
        assert_eq!(
            event.step.unwrap().to_string(),
            "98765432-10fe-dcba-9876-543210fedcba"
        );
        assert_eq!(
            event.meta.as_ref().unwrap().project_id.as_ref().unwrap(),
            "req-456"
        );
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T14:00:00+00:00"
        );
        assert!(event.end_time.is_none());

        if let WorkflowEventData::StateMachineNodeStart(data) = event.data {
            assert!(data.step_start_data.parent_step.is_none());
            assert_eq!(data.step_start_data.span_id.unwrap(), "span-789");
            assert_eq!(data.step_start_data.tags, vec!["state_machine", "node"]);
            assert_eq!(
                data.step_start_data.info.unwrap(),
                json!({"state": "initial"})
            );
            assert_eq!(data.step_start_data.input, json!({"init_param": "value"}));
            assert_eq!(data.context, json!({"machine_context": "some_context"}));
            assert_eq!(data.event.as_ref().unwrap().typ, "init");
            assert_eq!(
                data.event.as_ref().unwrap().data,
                json!({"init_data": "start"})
            );
        } else {
            panic!("Expected StateMachineNodeStart event");
        }
    }

    #[test]
    fn test_workflow_event_state_machine_status_deserialization() {
        let json_data = json!({
            "type": "state_machine:status",
            "data": {
                "status": "running"
            },
            "run_id": "abcdef01-2345-6789-abcd-ef0123456789",
            "source": "main_state_machine",
            "source_node": "status_update",
            "step": null,
            "meta": null,
            "start_time": "2023-06-27T15:00:00Z",
            "end_time": "2023-06-27T15:00:01Z"
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "abcdef01-2345-6789-abcd-ef0123456789"
        );
        assert_eq!(event.source, "main_state_machine");
        assert_eq!(event.source_node, "status_update");
        assert!(event.step.is_none());
        assert!(event.meta.is_none());
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T15:00:00+00:00"
        );
        assert_eq!(
            event.end_time.unwrap().to_rfc3339(),
            "2023-06-27T15:00:01+00:00"
        );

        if let WorkflowEventData::StateMachineStatus(data) = event.data {
            assert_eq!(data.status, "running");
        } else {
            panic!("Expected StateMachineStatus event");
        }
    }

    #[test]
    fn test_workflow_event_dag_node_state_deserialization() {
        let json_data = json!({
            "type": "dag:node_state",
            "data": {
                "state": "processing"
            },
            "run_id": "fedcba98-7654-3210-fedc-ba9876543210",
            "source": "main_dag",
            "source_node": "processing_node",
            "step": "01234567-89ab-cdef-0123-456789abcdef",
            "start_time": "2023-06-27T16:00:00Z",
            "end_time": null
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert_eq!(event.source, "main_dag");
        assert_eq!(event.source_node, "processing_node");
        assert_eq!(
            event.step.unwrap().to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T16:00:00+00:00"
        );
        assert!(event.end_time.is_none());

        if let WorkflowEventData::DagNodeState(data) = event.data {
            assert_eq!(data.state, "processing");
        } else {
            panic!("Expected DagNodeState event");
        }
    }

    #[test]
    fn test_workflow_event_step_error_deserialization() {
        let json_data = json!({
            "type": "step:error",
            "data": {
                "error": "Step execution failed"
            },
            "run_id": "12345678-90ab-cdef-1234-567890abcdef",
            "source": "main_workflow",
            "source_node": "error_step",
            "step": "abcdef01-2345-6789-abcd-ef0123456789",
            "meta": null,
            "start_time": "2023-06-27T17:00:00Z",
            "end_time": "2023-06-27T17:00:05Z"
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "12345678-90ab-cdef-1234-567890abcdef"
        );
        assert_eq!(event.source, "main_workflow");
        assert_eq!(event.source_node, "error_step");
        assert_eq!(
            event.step.unwrap().to_string(),
            "abcdef01-2345-6789-abcd-ef0123456789"
        );
        assert!(event.meta.is_none());
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T17:00:00+00:00"
        );
        assert_eq!(
            event.end_time.unwrap().to_rfc3339(),
            "2023-06-27T17:00:05+00:00"
        );

        if let WorkflowEventData::StepError(data) = event.data {
            assert_eq!(data.error, "Step execution failed");
        } else {
            panic!("Expected StepError event");
        }
    }

    #[test]
    fn test_workflow_event_state_machine_transition_final_state_deserialization() {
        let json_data = json!({
            "type": "state_machine:transition",
            "data": {
                "event": null,
                "input": {"final_input": "value"},
                "output": {"final_result": "success"},
                "from": "processing",
                "to": "completed",
                "final_state": true
            },
            "run_id": "98765432-10fe-dcba-9876-543210fedcba",
            "source": "main_state_machine",
            "source_node": "final_transition",
            "step": "fedcba98-7654-3210-fedc-ba9876543210",
            "start_time": "2023-06-27T18:00:00Z",
            "end_time": "2023-06-27T18:00:10Z"
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "98765432-10fe-dcba-9876-543210fedcba"
        );
        assert_eq!(event.source, "main_state_machine");
        assert_eq!(event.source_node, "final_transition");
        assert_eq!(
            event.step.unwrap().to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert_eq!(
            event.start_time.unwrap().to_rfc3339(),
            "2023-06-27T18:00:00+00:00"
        );
        assert_eq!(
            event.end_time.unwrap().to_rfc3339(),
            "2023-06-27T18:00:10+00:00"
        );

        if let WorkflowEventData::StateMachineTransition(data) = event.data {
            assert!(data.event.is_none());
            assert_eq!(data.input, json!({"final_input": "value"}));
            assert_eq!(data.output, json!({"final_result": "success"}));
            assert_eq!(data.from, "processing");
            assert_eq!(data.to, "completed");
            assert!(data.final_state);
        } else {
            panic!("Expected StateMachineTransition event");
        }
    }
}
