use chrono::DateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ProxyRequestMetadata;

pub struct RunStartEvent {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub application: Option<String>,
    pub environment: Option<String>,
    pub input: Option<serde_json::Value>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub tags: Vec<String>,
    pub info: Option<serde_json::Value>,
    pub time: Option<DateTime<chrono::Utc>>,
}

pub struct RunEndEvent {
    pub id: Uuid,
    pub status: Option<String>,
    pub output: Option<serde_json::Value>,
    pub info: Option<serde_json::Value>,
    pub time: Option<DateTime<chrono::Utc>>,
}

/// An event that updates a run or step in a workflow.
#[derive(Debug, Serialize, Deserialize)]
pub struct StepEvent {
    /// A UUIDv7 identifying the step the event belongs to
    pub step_id: Uuid,
    /// A UUIDv7 for the entire run
    pub run_id: Uuid,
    /// The event's type and data
    #[serde(flatten)]
    pub data: StepEventData,
    /// The DAG or state machine that this event belongs to
    pub source: Option<String>,
    /// The node within the workflow that this event belongs to
    pub source_node: Option<String>,
    pub meta: Option<ProxyRequestMetadata>,
    pub time: Option<DateTime<chrono::Utc>>,
}

/// Type-specific data for an event.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum StepEventData {
    /// Event data for the start of a step.
    Start(StepStartData),
    /// Event data for the end of a step.
    #[serde(rename = "step:end")]
    End(StepEndData),
    /// Event data for a step error.
    #[serde(rename = "step:error")]
    Error(ErrorData),
    /// Event data for a DAG node state change.
    #[serde(rename = "step:state")]
    State(StepStateData),
}

/// Data structure for the start of a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepStartData {
    pub step_type: String,
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
    pub error: serde_json::Value,
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
pub struct StepStateData {
    /// Current state of the DAG node.
    pub state: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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

        let event: StepEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(event.source.unwrap(), "main_state_machine");
        assert_eq!(event.source_node.unwrap(), "transition_node");
        assert_eq!(
            event.step_id.to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert!(event.meta.is_none());
        assert!(event.time.is_none());
    }

    #[test]
    fn test_workflow_event_step_start_deserialization() {
        let json_data = json!({
            "type": "step:start",
            "data": {
                "parent_step": "01234567-89ab-cdef-0123-456789abcdef",
                "span_id": "span-456",
                "tags": ["dag", "node"],
                "info": {"node_type": "task"},
                "input": {"task_param": "value"},
                "context": {"dag_context": "some_context"}
            },
            "run_id": "01234567-89ab-cdef-0123-456789abcdef",
            "source": "main_workflow",
            "source_node": "end_step",
            "step": "fedcba98-7654-3210-fedc-ba9876543210",
            "meta": null,
            "time": "2023-06-27T12:34:56Z"
        });

        let event: StepEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(event.source.unwrap(), "main_workflow");
        assert_eq!(event.source_node.unwrap(), "end_step");
        assert_eq!(
            event.step_id.to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert!(event.meta.is_none());
        assert_eq!(
            event.time.unwrap().to_rfc3339(),
            "2023-06-27T12:34:56+00:00"
        );

        if let StepEventData::Start(data) = event.data {
            assert_eq!(
                data.parent_step.unwrap().to_string(),
                "01234567-89ab-cdef-0123-456789abcdef"
            );
            assert_eq!(data.span_id.unwrap(), "span-456");
            assert_eq!(data.tags, vec!["dag", "node"]);
            assert_eq!(data.info.unwrap(), json!({"node_type": "task"}));
            assert_eq!(data.input, json!({"task_param": "value"}));
        } else {
            panic!("Expected StepEnd event");
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
            "time": "2023-06-27T12:34:56Z"
        });

        let event: StepEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(event.source.unwrap(), "main_workflow");
        assert_eq!(event.source_node.unwrap(), "end_step");
        assert_eq!(
            event.step_id.to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert!(event.meta.is_none());
        assert_eq!(
            event.time.unwrap().to_rfc3339(),
            "2023-06-27T12:34:56+00:00"
        );

        if let StepEventData::End(data) = event.data {
            assert_eq!(data.output, json!({"result": "success"}));
            assert_eq!(data.info.unwrap(), json!({"duration": 1000}));
        } else {
            panic!("Expected StepEnd event");
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
            "time": "2023-06-27T17:00:00Z"
        });

        let event: StepEvent = serde_json::from_value(json_data).unwrap();

        assert_eq!(
            event.run_id.to_string(),
            "12345678-90ab-cdef-1234-567890abcdef"
        );
        assert_eq!(event.source.unwrap(), "main_workflow");
        assert_eq!(event.source_node.unwrap(), "error_step");
        assert_eq!(
            event.step_id.to_string(),
            "abcdef01-2345-6789-abcd-ef0123456789"
        );
        assert!(event.meta.is_none());
        assert_eq!(
            event.time.unwrap().to_rfc3339(),
            "2023-06-27T17:00:00+00:00"
        );

        if let StepEventData::Error(data) = event.data {
            assert_eq!(data.error, "Step execution failed");
        } else {
            panic!("Expected StepError event");
        }
    }
}
