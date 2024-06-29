use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::{
    database::logging::{ProxyLogEntry, ProxyLogEvent},
    workflow_events::{
        ErrorData, RunStartEvent, RunUpdateEvent, StepEndData, StepEventData, StepStartData,
        WorkflowEvent,
    },
    EventPayload,
};

pub const TEST_STEP1_ID: Uuid = Uuid::from_u128(1);
pub const TEST_STEP2_ID: Uuid = Uuid::from_u128(2);
pub const TEST_RUN_ID: Uuid = Uuid::from_u128(100);
pub const TEST_EVENT1_ID: Uuid = Uuid::from_u128(5);

pub fn test_events() -> Vec<ProxyLogEntry> {
    vec![
        ProxyLogEntry::Workflow(WorkflowEvent::RunStart(RunStartEvent {
            id: TEST_RUN_ID,
            name: "test run".to_string(),
            description: Some("test description".to_string()),
            application: Some("test application".to_string()),
            environment: Some("test environment".to_string()),
            input: Some(json!({"query":"abc"})),
            trace_id: Some("0123456789abcdef".to_string()),
            span_id: Some("12345678".to_string()),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            info: Some(json!({
                "info1": "value1",
                "info2": "value2"
            })),
            time: Some(Utc.timestamp_opt(1, 0).unwrap()),
        })),
        ProxyLogEntry::Workflow(WorkflowEvent::StepStart(StepEventData {
            step_id: TEST_STEP1_ID,
            run_id: TEST_RUN_ID,
            time: Some(Utc.timestamp_opt(2, 0).unwrap()),
            data: StepStartData {
                name: Some("source_node1".to_string()),
                typ: "step_type".to_string(),
                parent_step: None,
                span_id: Some("11111111".to_string()),
                info: Some(json!({ "model": "a_model" })),
                tags: vec!["dag".to_string(), "node".to_string()],
                input: json!({ "task_param": "value" }),
            },
        })),
        ProxyLogEntry::Workflow(WorkflowEvent::StepStart(StepEventData {
            step_id: TEST_STEP2_ID,
            run_id: TEST_RUN_ID,
            time: Some(Utc.timestamp_opt(3, 0).unwrap()),
            data: StepStartData {
                name: Some("source_node2".to_string()),
                typ: "llm".to_string(),
                parent_step: Some(TEST_STEP1_ID),
                span_id: Some("22222222".to_string()),
                info: Some(json!({ "model": "a_model" })),
                tags: vec![],
                input: json!({ "task_param2": "value" }),
            },
        })),
        ProxyLogEntry::Proxied(Box::new(ProxyLogEvent {
            id: TEST_EVENT1_ID,
            event_type: std::borrow::Cow::Borrowed("query"),
            timestamp: Utc.timestamp_opt(4, 0).unwrap(),
            request: None,
            response: None,
            latency: None,
            total_latency: None,
            was_rate_limited: Some(false),
            num_retries: Some(0),
            error: None,
            options: crate::ProxyRequestOptions {
                metadata: crate::ProxyRequestMetadata {
                    step: Some(TEST_STEP2_ID),
                    run_id: Some(TEST_RUN_ID),
                    extra: Some(
                        json!({
                            "some_key": "some_value",
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    ),
                    ..Default::default()
                },
                ..Default::default()
            },
        })),
        ProxyLogEntry::Workflow(WorkflowEvent::Event(EventPayload {
            typ: "an_event".to_string(),
            data: Some(json!({
                "key": "value",
            })),
            error: Some(json!({
                "message": "something went wrong"
            })),
            step_id: TEST_STEP2_ID,
            run_id: TEST_RUN_ID,
            time: Some(Utc.timestamp_opt(5, 0).unwrap()),
            internal_metadata: None,
        })),
        ProxyLogEntry::Workflow(WorkflowEvent::StepError(StepEventData {
            step_id: TEST_STEP2_ID,
            run_id: TEST_RUN_ID,
            time: Some(Utc.timestamp_opt(5, 0).unwrap()),
            data: ErrorData {
                error: json!({"message": "an error"}),
            },
        })),
        ProxyLogEntry::Workflow(WorkflowEvent::StepEnd(StepEventData {
            step_id: TEST_STEP1_ID,
            run_id: TEST_RUN_ID,
            time: Some(Utc.timestamp_opt(5, 0).unwrap()),
            data: StepEndData {
                output: json!({ "result": "success" }),
                info: Some(json!({ "info3": "value3" })),
            },
        })),
        ProxyLogEntry::Workflow(WorkflowEvent::RunUpdate(RunUpdateEvent {
            id: TEST_RUN_ID,
            status: Some("finished".to_string()),
            output: Some(json!({ "result": "success" })),
            info: Some(json!({ "info2": "new_value", "info3": "value3"})),
            time: Some(Utc.timestamp_opt(5, 0).unwrap()),
        })),
    ]
}
