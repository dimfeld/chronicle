//! AWS Bedrock Runtime Support
//!
//! This is very similar to the Anthropic provider since Bedrock's API is close to the same. The
//! main difference is that we use their SDK for it.

use std::{borrow::Cow, collections::HashMap, time::Duration};

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
use aws_smithy_types::{event_stream::RawMessage, Document};
use error_stack::{Report, ResultExt};
use http::Response;
use serde_json::Value;

use super::{ChatModelProvider, ProviderError, ProviderErrorKind, SendRequestOptions};
use crate::{
    format::{
        ChatMessage, ChatRequestTransformation, ResponseInfo, StreamingResponse,
        StreamingResponseSender, Tool,
    },
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
        chunk_tx: StreamingResponseSender,
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
                .model_id(model.clone())
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

            let mut response = builder
                .send()
                .await
                .change_context_lazy(|| ProviderError::from_kind(ProviderErrorKind::Permanent))?;

            let mut processor = streaming::ChunkProcessor::new(chunk_tx);

            loop {
                let chunk = response.stream.recv().await;
                match chunk {
                    Ok(Some(response)) => {
                        processor.handle_data(response).await;
                    }
                    Err(e) => {
                        processor.handle_error(e).await;
                    }
                    // End of stream
                    Ok(None) => break,
                }
            }

            processor.finish(model).await;
        } else {
            let builder = self
                .client
                .converse()
                .model_id(model.clone())
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

            let response = builder.send().await;

            match response {
                Ok(result) => {
                    // TODO convert to SingleChatResponse and send it

                    chunk_tx
                        .send_async(Ok(StreamingResponse::ResponseInfo(ResponseInfo {
                            model,
                            meta: result
                                .additional_model_response_fields
                                .map(document_to_value),
                        })))
                        .await
                        .ok();
                }
                Err(e) => {
                    let res = e.raw_response();
                    let status_code = res
                        .map(|r| r.status().as_u16())
                        .and_then(|s| http::StatusCode::from_u16(s).ok());
                    let body = res.and_then(|r| r.body().bytes()).map(|r| {
                        let json: Option<serde_json::Value> = serde_json::from_slice(&r).ok();
                        json.unwrap_or_else(|| {
                            serde_json::Value::String(String::from_utf8_lossy(r).to_string())
                        })
                    });

                    let outer_err = ProviderError {
                        kind: status_code
                            .and_then(ProviderErrorKind::from_status_code)
                            .unwrap_or(ProviderErrorKind::Permanent),
                        status_code,
                        latency: Duration::ZERO,
                        body,
                    };

                    let err = Err(Report::new(e).change_context(outer_err));

                    chunk_tx.send_async(err).await.ok();
                }
            }
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

fn conversation_role_to_string(role: ConversationRole) -> &'static str {
    match role {
        ConversationRole::User => "user",
        ConversationRole::Assistant => "assistant",
        _ => {
            tracing::warn!(?role, "Unknown role");
            "user"
        }
    }
}

mod streaming {
    use std::time::Duration;

    use aws_sdk_bedrockruntime::{
        error::SdkError,
        operation::converse_stream::ConverseStreamError,
        types::{
            error::ConverseStreamOutputError, ContentBlockDelta, ContentBlockStart,
            ContentBlockStartEvent, ConverseStreamOutput,
        },
    };
    use aws_smithy_types::event_stream::RawMessage;
    use error_stack::Report;
    use http::StatusCode;
    use serde_json::json;

    use super::{conversation_role_to_string, document_to_value};
    use crate::{
        format::{
            ChatChoiceDelta, ChatMessage, ResponseInfo, StreamingChatResponse, StreamingResponse,
            StreamingResponseSender, ToolCall, ToolCallFunction, UsageResponse,
        },
        providers::{ProviderError, ProviderErrorKind},
    };

    pub struct ChunkProcessor {
        message: StreamingChatResponse,
        tool_call_index: Vec<u32>,
        chunk_tx: StreamingResponseSender,

        pub additional_model_response_fields: Option<serde_json::Value>,
    }

    impl ChunkProcessor {
        pub fn new(chunk_tx: StreamingResponseSender) -> Self {
            Self {
                message: StreamingChatResponse {
                    created: 0,
                    model: None,
                    system_fingerprint: None,
                    choices: Vec::with_capacity(1),
                    usage: None,
                },
                tool_call_index: Vec::new(),
                chunk_tx,
                additional_model_response_fields: None,
            }
        }

        fn transform_chunk(&mut self, data: ConverseStreamOutput) -> Option<StreamingChatResponse> {
            match data {
                ConverseStreamOutput::MessageStart(e) => {
                    // start of text with role
                    let mut ret = self.message.clone();
                    ret.choices.push(ChatChoiceDelta {
                        index: 0,
                        delta: ChatMessage {
                            role: Some(conversation_role_to_string(e.role).to_string()),
                            ..Default::default()
                        },
                        finish_reason: None,
                    });

                    Some(ret)
                }
                ConverseStreamOutput::MessageStop(e) => {
                    self.additional_model_response_fields =
                        e.additional_model_response_fields.map(document_to_value);
                    None
                }
                ConverseStreamOutput::Metadata(e) => {
                    let mut message = self.message.clone();

                    message.usage = e.usage.map(|u| UsageResponse {
                        prompt_tokens: Some(u.input_tokens as usize),
                        completion_tokens: Some(u.output_tokens as usize),
                        total_tokens: Some(u.total_tokens as usize),
                    });

                    Some(message)
                }
                ConverseStreamOutput::ContentBlockStart(e) => {
                    let mut message = self.message.clone();
                    let delta = match e.start {
                        Some(ContentBlockStart::ToolUse(tool)) => {
                            self.tool_call_index.push(e.content_block_index as u32);

                            ChatMessage {
                                tool_calls: vec![ToolCall {
                                    index: Some(self.tool_call_index.len() - 1),
                                    id: Some(tool.tool_use_id),
                                    typ: Some("function".to_string()),
                                    function: ToolCallFunction {
                                        name: Some(tool.name),
                                        arguments: None,
                                    },
                                }],
                                ..Default::default()
                            }
                        }
                        _ => return None,
                    };

                    message.choices.push(ChatChoiceDelta {
                        index: 0,
                        delta,
                        finish_reason: None,
                    });

                    Some(message)
                }
                ConverseStreamOutput::ContentBlockDelta(delta) => {
                    let mut message = self.message.clone();
                    let index = delta.content_block_index;
                    let delta = match delta.delta {
                        Some(ContentBlockDelta::Text(text)) => ChatMessage {
                            content: Some(text),
                            ..Default::default()
                        },
                        Some(ContentBlockDelta::ToolUse(delta)) => {
                            let tool_call_index =
                                self.tool_call_index.iter().position(|i| *i == index as u32);
                            let Some(tool_call_index) = tool_call_index else {
                                return None;
                            };

                            ChatMessage {
                                tool_calls: vec![ToolCall {
                                    index: Some(tool_call_index),
                                    id: None,
                                    typ: None,
                                    function: ToolCallFunction {
                                        name: None,
                                        arguments: Some(delta.input),
                                    },
                                }],
                                ..Default::default()
                            }
                        }
                        _ => return None,
                    };

                    message.choices.push(ChatChoiceDelta {
                        index: 0,
                        delta,
                        finish_reason: None,
                    });

                    Some(message)
                }
                ConverseStreamOutput::ContentBlockStop(_) => None,
                _ => None,
            }
        }

        pub async fn handle_data(&mut self, data: ConverseStreamOutput) {
            let chunk = self.transform_chunk(data);
            if let Some(message) = chunk {
                let message = Ok(StreamingResponse::Chunk(message));
                self.chunk_tx.send_async(message).await.ok();
            }
        }

        pub async fn handle_error(&self, error: SdkError<ConverseStreamOutputError, RawMessage>) {
            let e = error.into_service_error();
            let (status_code, kind) = if e.is_model_stream_error_exception() {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ProviderErrorKind::ProviderClosedConnection,
                )
            } else if e.is_throttling_exception() {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    ProviderErrorKind::RateLimit { retry_after: None },
                )
            } else if e.is_validation_exception() {
                (StatusCode::BAD_REQUEST, ProviderErrorKind::BadInput)
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, ProviderErrorKind::Server)
            };
            let meta = e.meta();
            let body = json!({
                "code": meta.code(),
                "message": meta.message(),
            });

            let err = Err(Report::new(e).change_context(ProviderError {
                kind,
                body: Some(body),
                status_code: Some(status_code),
                latency: Duration::ZERO,
            }));

            self.chunk_tx.send_async(err).await.ok();
        }

        pub async fn finish(self, model: String) {
            self.chunk_tx
                .send_async(Ok(StreamingResponse::ResponseInfo(ResponseInfo {
                    model,
                    meta: self.additional_model_response_fields,
                })))
                .await
                .ok();
        }
    }
}
