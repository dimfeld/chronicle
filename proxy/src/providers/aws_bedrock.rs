//! AWS Bedrock Runtime Support
//!
//! This is very similar to the Anthropic provider since Bedrock's API is close to the same. The
//! main difference is that we use their SDK for it.

mod conversions;

use std::{borrow::Cow, time::Duration};

use aws_sdk_bedrockruntime::types::{InferenceConfiguration, ToolConfiguration};
use conversions::{
    convert_from_single_aws_output, convert_message_to_aws_bedrock,
    convert_tool_choice_to_aws_bedrock, convert_tool_to_aws_bedrock, document_to_value,
};
use error_stack::{Report, ResultExt};

use super::{ChatModelProvider, ProviderError, ProviderErrorKind, SendRequestOptions};
use crate::format::{
    ChatRequestTransformation, ResponseInfo, StreamingResponse, StreamingResponseSender,
};

pub struct AwsBedrock {
    client: aws_sdk_bedrockruntime::Client,
}

impl std::fmt::Debug for AwsBedrock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsBedrock").finish_non_exhaustive()
    }
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
        SendRequestOptions { mut body, .. }: SendRequestOptions,
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

            tokio::task::spawn(async move {
                let mut processor = streaming::ChunkProcessor::new(chunk_tx);

                loop {
                    let chunk = response.stream.recv().await;
                    tracing::debug!(chunk = ?chunk, "Received chunk");
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
            });
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
                    let meta = result
                        .additional_model_response_fields
                        .clone()
                        .map(document_to_value);

                    let message = convert_from_single_aws_output(model.clone(), result)
                        .map(StreamingResponse::Single);
                    chunk_tx.send_async(message).await.ok();

                    chunk_tx
                        .send_async(Ok(StreamingResponse::ResponseInfo(ResponseInfo {
                            model,
                            meta,
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

mod streaming {
    use std::time::Duration;

    use aws_sdk_bedrockruntime::{
        error::SdkError,
        types::{
            error::ConverseStreamOutputError, ContentBlockDelta, ContentBlockStart,
            ConverseStreamOutput,
        },
    };
    use aws_smithy_types::event_stream::RawMessage;
    use error_stack::Report;
    use http::StatusCode;
    use serde_json::json;

    use super::conversions::{conversation_role_to_string, convert_usage, document_to_value};
    use crate::{
        format::{
            ChatChoiceDelta, ChatMessage, ResponseInfo, StreamingChatResponse, StreamingResponse,
            StreamingResponseSender, ToolCall, ToolCallFunction,
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

                    message.usage = e.usage.map(convert_usage);

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
            let meta = e.meta();
            let code = meta.code();

            let (status_code, kind, message) = match &e {
                ConverseStreamOutputError::ValidationException(e) => (
                    StatusCode::BAD_REQUEST,
                    ProviderErrorKind::BadInput,
                    e.message(),
                ),
                ConverseStreamOutputError::InternalServerException(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ProviderErrorKind::Server,
                    e.message(),
                ),
                ConverseStreamOutputError::ModelStreamErrorException(e) => {
                    let status_code = e
                        .original_status_code()
                        .and_then(|code| StatusCode::from_u16(code as u16).ok())
                        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

                    let message = e.original_message().or_else(|| e.message());

                    (
                        status_code,
                        ProviderErrorKind::ProviderClosedConnection,
                        message,
                    )
                }
                ConverseStreamOutputError::ThrottlingException(e) => (
                    StatusCode::TOO_MANY_REQUESTS,
                    ProviderErrorKind::RateLimit { retry_after: None },
                    e.message(),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ProviderErrorKind::Server,
                    meta.message(),
                ),
            };

            let body = json!({
                "code": code,
                "message": message,
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

#[cfg(test)]
mod tests {
    use crate::testing::{test_tool_use, test_tool_use_response};

    #[cfg_attr(
        not(feature = "live-test-aws-bedrock"),
        ignore = "live-test-aws-bedrock disabled"
    )]
    #[tokio::test]
    async fn live_test_tool_use_nonstreaming() {
        dotenvy::dotenv().ok();
        let model = "aws-bedrock/anthropic.claude-3-haiku-20240307-v1:0";
        test_tool_use(model, false).await;
    }

    #[cfg_attr(
        not(feature = "live-test-aws-bedrock"),
        ignore = "live-test-aws-bedrock disabled"
    )]
    #[tokio::test]
    async fn live_test_tool_use_streaming() {
        dotenvy::dotenv().ok();
        let model = "aws-bedrock/anthropic.claude-3-haiku-20240307-v1:0";
        test_tool_use(model, true).await;
    }

    #[cfg_attr(
        not(feature = "live-test-aws-bedrock"),
        ignore = "live-test-aws-bedrock disabled"
    )]
    #[tokio::test]
    async fn live_test_tool_use_response() {
        dotenvy::dotenv().ok();
        let model = "aws-bedrock/anthropic.claude-3-haiku-20240307-v1:0";
        test_tool_use_response(model, true).await;
    }
}
