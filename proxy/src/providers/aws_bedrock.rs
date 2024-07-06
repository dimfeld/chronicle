use std::borrow::Cow;

use async_trait::async_trait;
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ConversationRole, DocumentBlock, Message, ToolResultBlock, ToolUseBlock,
};
use error_stack::{Report, ResultExt};

use super::{ChatModelProvider, ProviderError, ProviderErrorKind, SendRequestOptions};
use crate::{
    format::{ChatMessage, ChatRequestTransformation, StreamingResponseSender},
    Error,
};

#[derive(Debug)]
pub struct AwsBedrock {
    client: aws_sdk_bedrockruntime::Client,
}

impl AwsBedrock {
    pub async fn new() -> Self {
        let config = aws_config::load_from_env().await;
        let client = aws_sdk_bedrockruntime::Client::new(&config);

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

        if body.stream {
            let builder = self.client.converse_stream().model_id(model);

            let builder = if let Some(system) = body.system {
                builder.system(aws_sdk_bedrockruntime::types::SystemContentBlock::Text(
                    system,
                ))
            } else {
                builder
            };
        } else {
            let builder = self.client.converse().model_id(model);
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

impl TryFrom<ChatMessage> for Message {
    type Error = Report<ProviderError>;

    fn try_from(msg: ChatMessage) -> Result<Self, Self::Error> {
        if !msg.tool_calls.is_empty() {
            let calls = msg
                .tool_calls
                .into_iter()
                .map(|call| {
                    let block = ToolUseBlock::builder()
                        .tool_use_id(call.id.unwrap_or_default())
                        .name(call.function.name.unwrap_or_default())
                        .input(call.function.arguments.unwrap_or_default().into())
                        .build()
                        .change_context_lazy(|| {
                            ProviderError::from_kind(ProviderErrorKind::TransformingRequest)
                        })?;

                    Ok::<_, Report<ProviderError>>(ContentBlock::ToolUse(block))
                })
                .collect::<Result<Vec<_>, _>>()?;

            return Message::builder()
                .role(ConversationRole::Assistant)
                .set_content(Some(calls))
                .build()
                .change_context_lazy(|| {
                    ProviderError::from_kind(ProviderErrorKind::TransformingRequest)
                });
        }

        let (role, content) = match msg.role.as_deref() {
            None | Some("user") => (
                ConversationRole::User,
                ContentBlock::Text(msg.content.unwrap_or_default()),
            ),
            Some("assistant") => (
                ConversationRole::Assistant,
                ContentBlock::Text(msg.content.unwrap_or_default()),
            ),
            Some("tool") => (
                ConversationRole::User,
                ContentBlock::ToolResult(
                    ToolResultBlock::builder()
                        .tool_use_id(msg.tool_call_id.unwrap_or_default())
                        .content(aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                            msg.content.unwrap_or_default(),
                        ))
                        .build()
                        .change_context_lazy(|| {
                            ProviderError::from_kind(ProviderErrorKind::TransformingRequest)
                        })?,
                ),
            ),

            Some(other) => {
                return Err(Report::from(ProviderError::from_kind(
                    ProviderErrorKind::TransformingRequest,
                ))
                .attach_printable(format!("Unknown message role: {other}")));
            }
        };

        Message::builder()
            .role(role)
            .content(content)
            .build()
            .change_context_lazy(|| {
                ProviderError::from_kind(ProviderErrorKind::TransformingRequest)
            })
    }
}
