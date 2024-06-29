use std::fmt::Debug;

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{EventPayload, ProxyRequestMetadata};

/// Type-specific data for an event.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowEvent {
    #[serde(rename = "run:start")]
    RunStart(RunStartEvent),
    #[serde(rename = "run:update")]
    RunUpdate(RunUpdateEvent),
    /// Event data for the start of a step.
    #[serde(rename = "step:start")]
    StepStart(StepEventData<StepStartData>),
    /// Event data for the end of a step.
    #[serde(rename = "step:end")]
    StepEnd(StepEventData<StepEndData>),
    /// Event data for a step error.
    #[serde(rename = "step:error")]
    StepError(StepEventData<ErrorData>),
    /// Event data for a DAG node state change.
    #[serde(rename = "step:state")]
    StepState(StepEventData<StepStateData>),
    #[serde(untagged)]
    Event(EventPayload),
}

/// An event that starts a run in a workflow.
#[derive(Debug, Serialize, Deserialize)]
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

impl RunStartEvent {
    /// Merge metadata into the event.
    pub fn merge_metadata(&mut self, other: &ProxyRequestMetadata) {
        if self.application.is_none() {
            self.application = other.application.clone();
        }
        if self.environment.is_none() {
            self.environment = other.environment.clone();
        }

        // Create info if it doesn't exist
        if self.info.is_none() {
            self.info = Some(serde_json::Value::Object(serde_json::Map::new()));
        }

        // Get a mutable reference to the info object
        let info = self.info.as_mut().unwrap().as_object_mut().unwrap();

        // Add other fields to info
        if let Some(org_id) = &other.organization_id {
            info.insert(
                "organization_id".to_string(),
                serde_json::Value::String(org_id.clone()),
            );
        }
        if let Some(project_id) = &other.project_id {
            info.insert(
                "project_id".to_string(),
                serde_json::Value::String(project_id.clone()),
            );
        }
        if let Some(user_id) = &other.user_id {
            info.insert(
                "user_id".to_string(),
                serde_json::Value::String(user_id.clone()),
            );
        }
        if let Some(workflow_id) = &other.workflow_id {
            info.insert(
                "workflow_id".to_string(),
                serde_json::Value::String(workflow_id.clone()),
            );
        }
        if let Some(workflow_name) = &other.workflow_name {
            info.insert(
                "workflow_name".to_string(),
                serde_json::Value::String(workflow_name.clone()),
            );
        }
        if let Some(step_index) = &other.step_index {
            info.insert(
                "step_index".to_string(),
                serde_json::Value::Number((*step_index).into()),
            );
        }
        if let Some(prompt_id) = &other.prompt_id {
            info.insert(
                "prompt_id".to_string(),
                serde_json::Value::String(prompt_id.clone()),
            );
        }
        if let Some(prompt_version) = &other.prompt_version {
            info.insert(
                "prompt_version".to_string(),
                serde_json::Value::Number((*prompt_version).into()),
            );
        }

        // Merge extra fields
        if let Some(extra) = &other.extra {
            for (key, value) in extra {
                info.insert(key.clone(), value.clone());
            }
        }
    }
}

/// An event that updates a run in a workflow.
#[derive(Debug, Serialize, Deserialize)]
pub struct RunUpdateEvent {
    /// The run ID
    pub id: Uuid,
    /// The new status value for the run.
    pub status: Option<String>,
    pub output: Option<serde_json::Value>,
    /// Extra info for the run. This is merged with any existing info.
    pub info: Option<serde_json::Value>,
    pub time: Option<DateTime<chrono::Utc>>,
}

/// An event that updates a run or step in a workflow.
#[derive(Debug, Serialize, Deserialize)]
pub struct StepEventData<DATA> {
    /// A UUIDv7 identifying the step the event belongs to
    pub step_id: Uuid,
    /// A UUIDv7 for the entire run
    pub run_id: Uuid,
    /// The event's type and data
    pub data: DATA,
    pub time: Option<DateTime<chrono::Utc>>,
}

/// Data structure for the start of a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepStartData {
    #[serde(rename = "type")]
    pub typ: String,
    /// A human-readable name for this step
    pub name: Option<String>,
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
    fn test_workflow_event_step_start_deserialization() {
        let json_data = json!({
            "type": "step:start",
            "data": {
                "parent_step": "01234567-89ab-cdef-0123-456789abcdef",
                "type": "a_step",
                "span_id": "span-456",
                "tags": ["dag", "node"],
                "info": {"node_type": "task"},
                "input": {"task_param": "value"},
                "name": "main_workflow",
                "context": {"dag_context": "some_context"}
            },
            "run_id": "01234567-89ab-cdef-0123-456789abcdef",
            "step_id": "fedcba98-7654-3210-fedc-ba9876543210",
            "time": "2023-06-27T12:34:56Z"
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();

        let WorkflowEvent::StepStart(event) = event else {
            panic!("Expected StepStart event");
        };

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(
            event.step_id.to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert_eq!(
            event.time.unwrap().to_rfc3339(),
            "2023-06-27T12:34:56+00:00"
        );

        assert_eq!(
            event.data.parent_step.unwrap().to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(event.data.typ, "a_step");
        assert_eq!(event.data.name.unwrap(), "main_workflow");
        assert_eq!(event.data.span_id.unwrap(), "span-456");
        assert_eq!(event.data.tags, vec!["dag", "node"]);
        assert_eq!(event.data.info.unwrap(), json!({"node_type": "task"}));
        assert_eq!(event.data.input, json!({"task_param": "value"}));
    }

    #[test]
    fn test_workflow_event_step_end_deserialization() {
        let json_data = json!({
            "type": "end",
            "data": {
                "output": {"result": "success"},
                "info": {"duration": 1000}
            },
            "run_id": "01234567-89ab-cdef-0123-456789abcdef",
            "step_id": "fedcba98-7654-3210-fedc-ba9876543210",
            "time": "2023-06-27T12:34:56Z"
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();
        let WorkflowEvent::StepEnd(event) = event else {
            panic!("Expected StepEnd event");
        };

        assert_eq!(
            event.run_id.to_string(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(
            event.step_id.to_string(),
            "fedcba98-7654-3210-fedc-ba9876543210"
        );
        assert_eq!(
            event.time.unwrap().to_rfc3339(),
            "2023-06-27T12:34:56+00:00"
        );

        assert_eq!(event.data.output, json!({"result": "success"}));
        assert_eq!(event.data.info.unwrap(), json!({"duration": 1000}));
    }

    #[test]
    fn test_workflow_event_step_error_deserialization() {
        let json_data = json!({
            "type": "error",
            "data": {
                "error": "Step execution failed"
            },
            "run_id": "12345678-90ab-cdef-1234-567890abcdef",
            "step_id": "abcdef01-2345-6789-abcd-ef0123456789",
            "time": "2023-06-27T17:00:00Z"
        });

        let event: WorkflowEvent = serde_json::from_value(json_data).unwrap();
        let WorkflowEvent::StepError(event) = event else {
            panic!("Expected StepEnd event");
        };

        assert_eq!(
            event.run_id.to_string(),
            "12345678-90ab-cdef-1234-567890abcdef"
        );
        assert_eq!(
            event.step_id.to_string(),
            "abcdef01-2345-6789-abcd-ef0123456789"
        );
        assert_eq!(
            event.time.unwrap().to_rfc3339(),
            "2023-06-27T17:00:00+00:00"
        );

        assert_eq!(event.data.error, "Step execution failed");
    }

    // TODO add tests for remaining event types
}
