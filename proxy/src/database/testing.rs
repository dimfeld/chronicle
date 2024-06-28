use serde_json::json;
use uuid::Uuid;

use crate::{
    database::logging::ProxyLogEntry,
    workflow_events::{
        ErrorData, RunEndEvent, RunStartEvent, StepEndData, StepEvent, StepEventData, StepStartData,
    },
};

pub fn test_events() -> Vec<ProxyLogEntry> {
    let step1_id = Uuid::from_u128(1);
    let step2_id = Uuid::from_u128(2);
    let run_id = Uuid::from_u128(100);

    vec![
        ProxyLogEntry::RunStart(RunStartEvent {
            id: run_id,
            name: "test run".to_string(),
            description: Some("test description".to_string()),
            application: Some("test application".to_string()),
            environment: Some("test environment".to_string()),
            input: None,
            trace_id: Some("12345678".to_string()),
            span_id: Some("0123456789abcdef".to_string()),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            info: Some(json!({
                "info1": "value1",
                "info2": "value2"
            })),
            time: None,
        }),
        ProxyLogEntry::StepEvent(StepEvent {
            step_id: step1_id,
            run_id,
            time: None,
            data: StepEventData::Start(StepStartData {
                name: Some("source_node1".to_string()),
                typ: "step_type".to_string(),
                parent_step: None,
                span_id: Some("11111111".to_string()),
                info: Some(json!({ "model": "a_model" })),
                tags: vec!["dag".to_string(), "node".to_string()],
                input: json!({ "task_param": "value" }),
            }),
        }),
        ProxyLogEntry::StepEvent(StepEvent {
            step_id: step2_id,
            run_id,
            time: None,
            data: StepEventData::Start(StepStartData {
                name: Some("source_node2".to_string()),
                typ: "llm".to_string(),
                parent_step: Some(step1_id),
                span_id: Some("22222222".to_string()),
                info: Some(json!({ "model": "a_model" })),
                tags: vec![],
                input: json!({ "task_param": "value" }),
            }),
        }),
        ProxyLogEntry::StepEvent(StepEvent {
            step_id: step2_id,
            run_id,
            time: None,
            data: StepEventData::Error(ErrorData {
                error: json!({"message": "an error"}),
            }),
        }),
        ProxyLogEntry::StepEvent(StepEvent {
            step_id: step1_id,
            run_id,
            time: None,
            data: StepEventData::End(StepEndData {
                output: json!({ "result": "success" }),
                info: Some(json!({ "info2": "new value", "info3": "value3" })),
            }),
        }),
        ProxyLogEntry::RunEnd(RunEndEvent {
            id: run_id,
            status: Some("finished".to_string()),
            output: Some(json!({ "result": "success" })),
            info: Some(json!({ "info3": "value3"})),
            time: None,
        }),
    ]
}
