//! AWS Bedrock Runtime Support
//!
//! This is very similar to the Anthropic provider since Bedrock's API is close to the same. The
//! main difference is that we use their SDK for it.

use std::{borrow::Cow, collections::HashMap};

use async_trait::async_trait;
use aws_config::retry::RetryConfig;
use aws_sdk_bedrockruntime::{
    error::BuildError,
    types::{
        AnyToolChoice, AutoToolChoice, ContentBlock, ConversationRole, DocumentBlock,
        InferenceConfiguration, Message, SpecificToolChoice, ToolChoice, ToolConfiguration,
        ToolInputSchema, ToolResultBlock, ToolSpecification, ToolUseBlock,
    },
};
use aws_smithy_types::Document;
use error_stack::{Report, ResultExt};
use serde_json::Value;

use super::{ChatModelProvider, ProviderError, ProviderErrorKind, SendRequestOptions};
use crate::{
    format::{ChatMessage, ChatRequestTransformation, StreamingResponseSender, Tool},
    Error,
};

#[derive(Debug)]
pub struct AwsBedrock {
    client: aws_sdk_bedrockruntime::Client,
}

impl AwsBedrock {
    pub async fn new(client: Option<aws_sdk_bedrockruntime::Client>) -> Self {
        let client = match client {
            Some(client) => client,
            None => {
                let config = aws_config::load_from_env().await;
                aws_sdk_bedrockruntime::Client::new(&config)
            }
        };

        Self { client }
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for AwsBedrock {
    fn name(&self) -> &str {
        "aws-bedrock"
    }

    fn label(&self) -> &str {
        "AWS Bedrock"
    }

    async fn send_request(
        &self,
        SendRequestOptions {
            timeout, mut body, ..
        }: SendRequestOptions,
        _chunk_tx: StreamingResponseSender,
    ) -> Result<(), Report<ProviderError>> {
        body.transform(&ChatRequestTransformation {
            supports_message_name: false,
            system_in_messages: false,
            strip_model_prefix: Some(Cow::Borrowed("aws-bedrock/")),
        });

        let model = body
            .model
            .ok_or_else(|| ProviderError::from_kind(ProviderErrorKind::TransformingRequest))
            .attach_printable("Model not specified")?;

        let stop = if body.stop.is_empty() {
            None
        } else {
            Some(body.stop)
        };

        let inference_config = InferenceConfiguration::builder()
            .set_max_tokens(body.max_tokens.map(|t| t as i32))
            .set_temperature(body.temperature)
            .set_top_p(body.top_p)
            .set_stop_sequences(stop)
            .build();

        let messages = body
            .messages
            .into_iter()
            .map(convert_message_to_aws_bedrock)
            .collect::<Result<Vec<_>, _>>()
            .change_context_lazy(|| {
                ProviderError::from_kind(ProviderErrorKind::TransformingRequest)
            })
            .attach_printable("Failed to convert messages to AWS Bedrock format")?;

        let tool_config = if body.tools.is_empty() {
            None
        } else {
            let tools = body
                .tools
                .into_iter()
                .map(convert_tool_to_aws_bedrock)
                .collect::<Result<Vec<_>, _>>()?;
            let tool_choice = convert_tool_choice_to_aws_bedrock(body.tool_choice);
            Some(
                ToolConfiguration::builder()
                    .set_tool_choice(tool_choice)
                    .set_tools(Some(tools))
                    .build()
                    .change_context_lazy(ProviderError::transforming_request)?,
            )
        };

        if body.stream {
            let builder = self
                .client
                .converse_stream()
                .model_id(model)
                .inference_config(inference_config)
                .set_messages(Some(messages))
                .set_tool_config(tool_config);

            let builder = if let Some(system) = body.system {
                builder.system(aws_sdk_bedrockruntime::types::SystemContentBlock::Text(
                    system,
                ))
            } else {
                builder
            };
        } else {
            let builder = self
                .client
                .converse()
                .model_id(model)
                .inference_config(inference_config)
                .set_messages(Some(messages))
                .set_tool_config(tool_config);
            let builder = if let Some(system) = body.system {
                builder.system(aws_sdk_bedrockruntime::types::SystemContentBlock::Text(
                    system,
                ))
            } else {
                builder
            };
        }

        Ok(())
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("aws-bedrock/")
    }
}

fn convert_tool_choice_to_aws_bedrock(
    tool_choice: Option<serde_json::Value>,
) -> Option<aws_sdk_bedrockruntime::types::ToolChoice> {
    let Some(value) = tool_choice else {
        return None;
    };

    match value.as_str().unwrap_or_default() {
        // "none" is not supported so we don't set a value
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

fn convert_tool_to_aws_bedrock(
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
fn value_to_document(value: Value) -> Document {
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
fn document_to_value(document: Document) -> serde_json::Value {
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

fn convert_message_to_aws_bedrock(
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
