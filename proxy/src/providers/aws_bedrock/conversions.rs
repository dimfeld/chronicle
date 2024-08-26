use std::borrow::Cow;

use aws_sdk_bedrockruntime::types::{
    AnyToolChoice, AutoToolChoice, ContentBlock, ConversationRole, Message, SpecificToolChoice,
    StopReason, TokenUsage, ToolChoice, ToolInputSchema, ToolResultBlock, ToolSpecification,
    ToolUseBlock,
};
use aws_smithy_types::Document;
use chrono::Utc;
use error_stack::{Report, ResultExt};
use itertools::Itertools;
use serde_json::Value;

use crate::{
    format::{
        ChatChoice, ChatMessage, FinishReason, SingleChatResponse, Tool, ToolCall,
        ToolCallFunction, UsageResponse,
    },
    providers::ProviderError,
};

pub fn convert_tool_choice_to_aws_bedrock(
    tool_choice: Option<serde_json::Value>,
) -> Option<aws_sdk_bedrockruntime::types::ToolChoice> {
    let Some(value) = tool_choice else {
        return None;
    };

    match value.as_str().unwrap_or_default() {
        // "none" is not supported so we don't set a value, which isn't the same but is
        // as close as we can get
        "none" => return None,
        "auto" => return Some(ToolChoice::Auto(AutoToolChoice::builder().build())),
        "required" => return Some(ToolChoice::Any(AnyToolChoice::builder().build())),
        _ => {}
    };

    if value["type"] == "function" {
        if let Some(tool_name) = value["function"]["name"].as_str() {
            return Some(ToolChoice::Tool(
                SpecificToolChoice::builder()
                    .name(tool_name)
                    .build()
                    .unwrap(),
            ));
        }
    }

    None
}

pub fn convert_tool_to_aws_bedrock(
    tool: Tool,
) -> Result<aws_sdk_bedrockruntime::types::Tool, Report<ProviderError>> {
    let schema = tool
        .function
        .parameters
        .map(|p| ToolInputSchema::Json(value_to_document(p)));
    let tool_spec = ToolSpecification::builder()
        .name(tool.function.name)
        .set_description(tool.function.description)
        .set_input_schema(schema)
        .build()
        .unwrap();

    Ok(aws_sdk_bedrockruntime::types::Tool::ToolSpec(tool_spec))
}

/// Convert a serde_json::Value to a Document
pub fn value_to_document(value: Value) -> Document {
    match value {
        Value::Null => Document::Null,
        Value::Bool(b) => Document::Bool(b),
        Value::Number(n) => {
            if let Some(n) = n.as_f64() {
                aws_smithy_types::Number::Float(n).into()
            } else if let Some(n) = n.as_u64() {
                aws_smithy_types::Number::PosInt(n).into()
            } else if let Some(n) = n.as_i64() {
                aws_smithy_types::Number::NegInt(n).into()
            } else {
                // This shouldn't ever happen
                Document::Null
            }
        }
        Value::String(s) => Document::String(s),
        Value::Array(arr) => Document::Array(arr.into_iter().map(value_to_document).collect()),
        Value::Object(obj) => Document::Object(
            obj.into_iter()
                .map(|(k, v)| (k, value_to_document(v)))
                .collect(),
        ),
    }
}

/// Convert a Document to a serde_json::Value
pub fn document_to_value(document: Document) -> serde_json::Value {
    match document {
        Document::Null => Value::Null,
        Document::Bool(b) => Value::Bool(b),
        Document::Number(n) => {
            let n = match n {
                aws_smithy_types::Number::Float(f) => {
                    serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0))
                }
                aws_smithy_types::Number::PosInt(p) => serde_json::Number::from(p),
                aws_smithy_types::Number::NegInt(p) => serde_json::Number::from(p),
            };

            Value::Number(n)
        }
        Document::String(s) => Value::String(s),
        Document::Array(arr) => Value::Array(arr.into_iter().map(document_to_value).collect()),
        Document::Object(obj) => Value::Object(
            obj.into_iter()
                .map(|(k, v)| (k, document_to_value(v)))
                .collect::<serde_json::Map<String, Value>>()
                .into(),
        ),
    }
}

pub fn convert_message_to_aws_bedrock(
    message: ChatMessage,
) -> Result<aws_sdk_bedrockruntime::types::Message, Report<ProviderError>> {
    let role = message.role.as_deref().unwrap_or("user");
    if role == "tool" {
        // Tool result
        let block = ToolResultBlock::builder()
            .set_tool_use_id(message.tool_call_id)
            .content(aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                message.content.unwrap_or_default(),
            ))
            .build()
            .change_context_lazy(ProviderError::transforming_request)?;

        let tool_result = ContentBlock::ToolResult(block);
        return Message::builder()
            .role(ConversationRole::User)
            .content(tool_result)
            .build()
            .change_context_lazy(ProviderError::transforming_request);
    }

    let role = match role {
        "assistant" => ConversationRole::Assistant,
        _ => ConversationRole::User,
    };

    let content = message
        .content
        .into_iter()
        .map(|text| Ok(ContentBlock::Text(text)))
        .chain(message.tool_calls.into_iter().map(|c| {
            let input = c
                .function
                .arguments
                .map(|a| serde_json::from_str(&a))
                .transpose()
                .change_context_lazy(ProviderError::transforming_request)?
                .map(value_to_document)
                .unwrap_or_default();

            let block = ToolUseBlock::builder()
                .set_tool_use_id(c.id)
                .set_name(c.function.name)
                .input(input)
                .build()
                .change_context_lazy(ProviderError::transforming_request)?;
            Ok::<_, Report<ProviderError>>(ContentBlock::ToolUse(block))
        }))
        .collect::<Result<_, _>>()?;

    Message::builder()
        .role(role)
        .set_content(Some(content))
        .build()
        .change_context_lazy(ProviderError::transforming_request)
}

pub fn conversation_role_to_string(role: ConversationRole) -> &'static str {
    match role {
        ConversationRole::User => "user",
        ConversationRole::Assistant => "assistant",
        _ => {
            tracing::warn!(?role, "Unknown role");
            "user"
        }
    }
}

pub fn convert_usage(u: TokenUsage) -> UsageResponse {
    UsageResponse {
        prompt_tokens: Some(u.input_tokens as usize),
        completion_tokens: Some(u.output_tokens as usize),
        total_tokens: Some(u.total_tokens as usize),
    }
}

/// Extract the text and tool calls from the chat content
fn convert_content_blocks(mut content: Vec<ContentBlock>) -> (Option<String>, Vec<ToolCall>) {
    if content.len() == 1 {
        match content.pop().unwrap() {
            ContentBlock::Text(text) => (Some(text), Vec::new()),
            ContentBlock::ToolUse(tool) => {
                let tools = vec![ToolCall::from(tool)];
                (None, tools)
            }
            _ => (None, Vec::new()),
        }
    } else {
        let text = content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text(text) => Some(text),
                _ => None,
            })
            .join("");

        let tools = content
            .into_iter()
            .filter_map(|c| {
                if let ContentBlock::ToolUse(tool) = c {
                    Some(ToolCall::from(tool))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let text = if text.is_empty() { None } else { Some(text) };

        (text, tools)
    }
}

impl From<ToolUseBlock> for ToolCall {
    fn from(value: ToolUseBlock) -> Self {
        Self {
            index: None,
            id: Some(value.tool_use_id),
            typ: Some("function".to_string()),
            function: ToolCallFunction {
                name: Some(value.name),
                arguments: serde_json::to_string(&document_to_value(value.input)).ok(),
            },
        }
    }
}

fn convert_aws_message(message: Message) -> ChatMessage {
    let role = conversation_role_to_string(message.role);

    let (message, tools) = convert_content_blocks(message.content);

    ChatMessage {
        role: Some(role.to_string()),
        name: None,
        content: message,
        tool_calls: tools,
        tool_call_id: None,
        cache_control: None,
    }
}

pub fn convert_from_single_aws_output(
    model: String,
    output: aws_sdk_bedrockruntime::operation::converse::ConverseOutput,
) -> Result<SingleChatResponse, Report<ProviderError>> {
    let finish_reason = match output.stop_reason {
        StopReason::ContentFiltered => FinishReason::ContentFilter,
        StopReason::EndTurn => FinishReason::Stop,
        StopReason::GuardrailIntervened => FinishReason::Other(Cow::from("guardrail_intervened")),
        StopReason::MaxTokens => FinishReason::Length,
        StopReason::StopSequence => FinishReason::Stop,
        StopReason::ToolUse => FinishReason::ToolCalls,
        e @ _ => FinishReason::Other(Cow::from(e.to_string())),
    };

    let message = output
        .output
        .and_then(|o| match o {
            aws_sdk_bedrockruntime::types::ConverseOutput::Message(m) => Some(m),
            _ => None,
        })
        .ok_or_else(|| ProviderError::from_kind(crate::providers::ProviderErrorKind::Server))
        .attach_printable("Model returned no output")?;

    let choice = ChatChoice {
        index: 0,
        message: convert_aws_message(message),
        finish_reason,
    };

    Ok(SingleChatResponse {
        model: Some(model),
        created: Utc::now().timestamp() as u64,
        system_fingerprint: None,
        choices: vec![choice],
        usage: output.usage.map(convert_usage),
    })
}
