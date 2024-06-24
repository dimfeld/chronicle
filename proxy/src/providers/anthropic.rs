use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use itertools::Itertools;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};

use super::{ChatModelProvider, ProviderError, ProviderErrorKind, SendRequestOptions};
use crate::{
    format::{
        ChatChoice, ChatChoiceDelta, ChatMessage, ChatRequestTransformation, ChatResponse,
        ResponseInfo, SingleChatResponse, StreamingChatResponse, StreamingResponse,
        StreamingResponseSender, Tool, ToolCall, ToolCallFunction, UsageResponse,
    },
    request::{parse_response_json, response_is_sse, send_standard_request},
    streaming::stream_sse_to_channel,
};

#[derive(Debug)]
pub struct Anthropic {
    client: reqwest::Client,
    pub(crate) url: String,
    token: Option<String>,
}

impl Anthropic {
    pub fn new(client: reqwest::Client, token: Option<String>) -> Self {
        Self {
            client,
            url: "https://api.anthropic.com/v1/messages".to_string(),
            token: token.or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
        }
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for Anthropic {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn label(&self) -> &str {
        "Anthropic"
    }

    async fn send_request(
        &self,
        SendRequestOptions {
            timeout,
            api_key,
            mut body,
            ..
        }: SendRequestOptions,
        chunk_tx: StreamingResponseSender,
    ) -> Result<(), Report<ProviderError>> {
        body.transform(&ChatRequestTransformation {
            supports_message_name: false,
            system_in_messages: false,
            strip_model_prefix: Some("anthropic/".into()),
        });

        let body = AnthropicChatRequest {
            model: body.model.unwrap_or_default(),
            max_tokens: body.max_tokens.unwrap_or(4096),
            system: body.system,
            metadata: AnthropicMetadata { user_id: body.user },
            messages: body.messages,
            stop: body.stop,
            temperature: body.temperature,
            top_p: body.top_p,
            tools: body.tools.into_iter().map(From::from).collect::<Vec<_>>(),
            tool_choice: body.tool_choice.map(|c| c.into()),
            stream: body.stream,
        };

        let body = serde_json::to_vec(&body).change_context_lazy(|| {
            ProviderError::from_kind(ProviderErrorKind::TransformingRequest)
        })?;
        let body = Bytes::from(body);

        let api_token = api_key
            .as_deref()
            .or(self.token.as_deref())
            .ok_or_else(|| ProviderError::from_kind(ProviderErrorKind::AuthMissing))?;

        let (response, latency) = send_standard_request(
            timeout,
            || {
                self.client
                    .post(self.url.as_str())
                    .header("x-api-key", api_token)
                    .header("anthropic-version", "2023-06-01")
                    .header(CONTENT_TYPE, "application/json; charset=utf8")
            },
            handle_retry_after,
            body,
        )
        .await?;

        if response_is_sse(&response) {
            let processor = streaming::ChunkProcessor::new();
            stream_sse_to_channel(response, chunk_tx, processor);
        } else {
            let result = parse_response_json::<AnthropicChatResponse>(response, latency).await;
            match result {
                Ok(result) => {
                    let info = StreamingResponse::ResponseInfo(ResponseInfo {
                        model: result.model.clone(),
                        meta: None,
                    });

                    chunk_tx
                        .send_async(Ok(StreamingResponse::Single(result.into())))
                        .await
                        .ok();
                    chunk_tx.send_async(Ok(info)).await.ok();
                }

                Err(e) => {
                    chunk_tx.send_async(Err(e)).await.ok();
                }
            }
        }

        Ok(())
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("anthropic/") || model.starts_with("claude")
    }
}

fn handle_retry_after(res: &reqwest::Response) -> Option<Duration> {
    let headers = res.headers();
    let req_limit = headers
        .get("anthropic-ratelimit-requests-limit")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let req_remaining = headers
        .get("anthropic-ratelimit-requests-remaining")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let req_reset = headers
        .get("anthropic-ratelimit-requests-reset")
        .and_then(|s| s.to_str().ok());
    let token_limit = headers
        .get("anthropic-ratelimit-tokens-limit")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let token_remaining = headers
        .get("anthropic-ratelimit-tokens-remaining")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let token_reset = headers
        .get("anthropic-ratelimit-tokens-reset")
        .and_then(|s| s.to_str().ok());
    tracing::warn!(
        llm.rate_limiting.req_limit = req_limit,
        llm.rate_liting.req_remaining = req_remaining,
        llm.rate_limiting.req_reset = req_reset,
        llm.rate_limiting.token_limit = token_limit,
        llm.rate_liting.token_remaining = token_remaining,
        llm.rate_limiting.token_reset = token_reset,
        "Hit Anthropic rate limit"
    );

    let token_reset = token_remaining
        .zip(token_reset)
        .filter(|(remaining, _)| *remaining == 0)
        .and_then(|(_, reset_time)| chrono::DateTime::parse_from_rfc3339(reset_time).ok());

    let req_reset = req_remaining
        .zip(req_reset)
        .filter(|(remaining, _)| *remaining == 0)
        .and_then(|(_, reset_time)| chrono::DateTime::parse_from_rfc3339(reset_time).ok());

    let reset_time = match (token_reset, req_reset) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    let until_reset_time = reset_time
        .map(|time| time.to_utc() - chrono::Utc::now())
        .and_then(|d| d.to_std().ok());

    until_reset_time
}

#[derive(Serialize, Debug, Clone)]
struct AnthropicChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    metadata: AnthropicMetadata,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<AnthropicToolChoice>,
}

#[derive(Serialize, Debug, Clone)]
struct AnthropicMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
struct AnthropicTool {
    name: String,
    description: Option<String>,
    input_schema: Option<serde_json::Value>,
}

impl From<Tool> for AnthropicTool {
    fn from(tool: Tool) -> Self {
        AnthropicTool {
            name: tool.function.name,
            description: tool.function.description,
            input_schema: tool.function.parameters,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
enum AnthropicToolChoice {
    /// Let the model decide whether to use a tool or not
    Auto,
    /// Force the model to use a tool, but let it decide which one
    Any,
    /// Force a specific tool
    Tool {
        /// Which tool to use
        name: String,
    },
}

impl From<serde_json::Value> for AnthropicToolChoice {
    fn from(value: serde_json::Value) -> Self {
        match value.as_str().unwrap_or_default() {
            "none" => return AnthropicToolChoice::Auto,
            "auto" => return AnthropicToolChoice::Auto,
            "required" => return AnthropicToolChoice::Any,
            _ => {}
        };

        if value["type"] == "function" {
            if let Some(tool_name) = value["function"]["name"].as_str() {
                return AnthropicToolChoice::Tool {
                    name: tool_name.to_string(),
                };
            }
        }

        AnthropicToolChoice::Auto
    }
}

/// A chat response, in non-chunked format
#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnthropicChatResponse {
    // Omitted certain fields that aren't really useful
    // id: String,
    // type: String,
    pub role: String,
    pub content: Vec<AnthropicChatContent>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: Option<AnthropicUsageResponse>,
}

impl Into<SingleChatResponse> for AnthropicChatResponse {
    fn into(self) -> SingleChatResponse {
        let (text, tool_calls) = convert_content(self.content);
        ChatResponse {
            created: chrono::Utc::now().timestamp() as u64,
            model: Some(self.model),
            system_fingerprint: None,
            choices: vec![ChatChoice {
                index: 0,
                // TODO align this with OpenAI finish_reason
                finish_reason: self.stop_reason.unwrap_or_default(),
                message: ChatMessage {
                    role: Some(self.role),
                    name: None,
                    content: text,
                    tool_calls,
                },
            }],
            usage: Some(UsageResponse {
                prompt_tokens: self.usage.as_ref().and_then(|u| u.input_tokens),
                completion_tokens: self.usage.as_ref().and_then(|u| u.output_tokens),
                total_tokens: None,
            }),
        }
    }
}

impl AnthropicChatResponse {
    /// Do the conversion as required by the message_start streaming event
    fn as_new_streaming_message(self) -> StreamingChatResponse {
        let (text, tool_calls) = convert_content(self.content);
        ChatResponse {
            created: chrono::Utc::now().timestamp() as u64,
            model: Some(self.model),
            system_fingerprint: None,
            choices: vec![ChatChoiceDelta {
                index: 0,
                // TODO align this with OpenAI finish_reason
                finish_reason: self.stop_reason,
                delta: ChatMessage {
                    role: Some(self.role),
                    name: None,
                    content: text,
                    tool_calls,
                },
            }],
            usage: Some(UsageResponse {
                prompt_tokens: self.usage.as_ref().and_then(|u| u.input_tokens),
                completion_tokens: self.usage.as_ref().and_then(|u| u.output_tokens),
                total_tokens: None,
            }),
        }
    }
}

/// Extract the text and tool calls from the chat content
fn convert_content(mut content: Vec<AnthropicChatContent>) -> (Option<String>, Vec<ToolCall>) {
    if content.len() == 1 {
        match content.pop().unwrap() {
            AnthropicChatContent::Text { text } => (Some(text), Vec::new()),
            AnthropicChatContent::ToolUse(tool) => {
                let tools = vec![ToolCall::from(tool)];
                (None, tools)
            }
            _ => (None, Vec::new()),
        }
    } else {
        let text = content
            .iter()
            .filter_map(|c| match c {
                AnthropicChatContent::Text { text } => Some(text),
                _ => None,
            })
            .join("");

        let tools = content
            .into_iter()
            .filter_map(|c| {
                if let AnthropicChatContent::ToolUse(tool) = c {
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicChatContent {
    Text {
        text: String,
    },
    ToolUse(AnthropicToolUse),
    ToolResult {
        tool_use_id: String,
        content: Option<String>,
        is_error: bool,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnthropicToolUse {
    id: String,
    name: String,
    input: serde_json::Value,
}

impl From<AnthropicToolUse> for ToolCall {
    fn from(tool: AnthropicToolUse) -> ToolCall {
        ToolCall {
            index: None,
            id: Some(tool.id),
            typ: Some("function".to_string()),
            function: ToolCallFunction {
                name: Some(tool.name),
                arguments: Some(tool.input.to_string()),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnthropicUsageResponse {
    pub input_tokens: Option<usize>,
    pub output_tokens: Option<usize>,
}

mod streaming {
    use std::time::Duration;

    use error_stack::{Report, ResultExt};
    use http::StatusCode;
    use serde::{Deserialize, Serialize};

    use super::{AnthropicChatResponse, AnthropicToolUse, AnthropicUsageResponse};
    use crate::{
        format::{
            ChatChoiceDelta, ChatMessage, StreamingChatResponse, ToolCall, ToolCallFunction,
            UsageResponse,
        },
        providers::ProviderError,
        streaming::StreamingChunkMapper,
    };

    pub struct ChunkProcessor {
        message: StreamingChatResponse,
    }

    impl ChunkProcessor {
        pub fn new() -> Self {
            Self {
                message: StreamingChatResponse {
                    created: 0,
                    model: None,
                    system_fingerprint: None,
                    choices: Vec::new(),
                    usage: None,
                },
            }
        }

        fn handle_data(
            &mut self,
            data: StreamingMessage,
        ) -> Result<Option<StreamingChatResponse>, error_stack::Report<ProviderError>> {
            match data {
                StreamingMessage::MessageStart {
                    message: new_message,
                } => {
                    // transform and save the message, then return it
                    self.message = new_message.as_new_streaming_message();

                    let ret = Ok(Some(self.message.clone()));
                    // Clear the saved copy of `choices` so we can easily reuse it for later
                    // messages.
                    self.message.choices.clear();
                    ret
                }
                StreamingMessage::MessageDelta {
                    delta: message,
                    usage,
                } => {
                    if let Some(usage) = usage {
                        let usage = UsageResponse {
                            prompt_tokens: usage.input_tokens,
                            completion_tokens: usage.output_tokens,
                            total_tokens: None,
                        };

                        match self.message.usage.as_mut() {
                            Some(msg_usage) => {
                                msg_usage.merge(&usage);
                            }
                            None => {
                                self.message.usage = Some(usage);
                            }
                        }
                    }

                    let mut output = self.message.clone();
                    if message.stop_reason.is_some() {
                        let delta = ChatChoiceDelta {
                            index: 0,
                            delta: ChatMessage {
                                content: Some(String::new()),
                                ..Default::default()
                            },
                            finish_reason: message.stop_reason,
                        };

                        output.choices.push(delta);
                    }

                    Ok(Some(output))
                }
                StreamingMessage::ContentBlockStart {
                    index,
                    content_block,
                } => {
                    let mut message = self.message.clone();
                    let delta = match content_block {
                        ContentBlock::Text { text } => ChatMessage {
                            content: Some(text),
                            ..Default::default()
                        },
                        ContentBlock::ToolUse(tool) => ChatMessage {
                            tool_calls: vec![ToolCall {
                                index: Some(index),
                                id: Some(tool.id),
                                typ: Some("function".to_string()),
                                function: ToolCallFunction {
                                    name: Some(tool.name),
                                    arguments: None,
                                },
                            }],
                            ..Default::default()
                        },
                    };

                    message.choices.push(ChatChoiceDelta {
                        index,
                        delta,
                        finish_reason: None,
                    });

                    Ok(Some(message))
                }
                StreamingMessage::ContentBlockDelta { index, delta } => {
                    let mut message = self.message.clone();
                    let delta = match delta {
                        ContentBlockDelta::TextDelta { text } => ChatMessage {
                            content: Some(text),
                            ..Default::default()
                        },
                        ContentBlockDelta::InputJsonDelta { partial_json } => ChatMessage {
                            tool_calls: vec![ToolCall {
                                index: Some(index),
                                id: None,
                                typ: None,
                                function: ToolCallFunction {
                                    name: None,
                                    arguments: Some(partial_json),
                                },
                            }],
                            ..Default::default()
                        },
                    };

                    message.choices.push(ChatChoiceDelta {
                        index,
                        delta,
                        finish_reason: None,
                    });

                    Ok(Some(message))
                }
                StreamingMessage::ContentBlockStop { .. } => Ok(None),
                StreamingMessage::Error { error } => {
                    let status_code = match error.typ.as_str() {
                        "invalid_request_error" => Some(StatusCode::BAD_REQUEST),
                        "api_error" => Some(StatusCode::INTERNAL_SERVER_ERROR),
                        "overloaded_error" => Some(StatusCode::from_u16(529).unwrap()),
                        _ => None,
                    };

                    Err(Report::new(ProviderError {
                        kind: crate::providers::ProviderErrorKind::Server,
                        body: serde_json::to_value(error).ok(),
                        status_code,
                        latency: Duration::ZERO,
                    }))
                }
            }
        }
    }

    impl StreamingChunkMapper for ChunkProcessor {
        fn process_chunk(
            &mut self,
            event: &eventsource_stream::Event,
        ) -> Result<
            Option<crate::format::StreamingChatResponse>,
            error_stack::Report<crate::providers::ProviderError>,
        > {
            match event.event.as_str() {
                "message_start"
                | "message_delta"
                | "content_block_start"
                | "content_block_delta"
                | "content_block_stop"
                | "error" => {
                    let data = serde_json::from_str::<StreamingMessage>(&event.data)
                        .change_context_lazy(|| ProviderError {
                            kind: crate::providers::ProviderErrorKind::ParsingResponse,
                            body: serde_json::from_str(&event.data).ok(),
                            status_code: None,
                            latency: Duration::ZERO,
                        })?;
                    self.handle_data(data)
                }
                "message_stop" => Ok(None),
                _ => Ok(None),
            }
        }
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum StreamingMessage {
        MessageStart {
            message: AnthropicChatResponse,
        },
        MessageDelta {
            delta: MessageDelta,
            usage: Option<AnthropicUsageResponse>,
        },
        ContentBlockStart {
            index: usize,
            content_block: ContentBlock,
        },
        ContentBlockDelta {
            index: usize,
            delta: ContentBlockDelta,
        },
        ContentBlockStop {
            index: usize,
        },
        Error {
            error: StreamingError,
        },
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum ContentBlock {
        Text { text: String },
        ToolUse(AnthropicToolUse),
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum ContentBlockDelta {
        TextDelta { text: String },
        InputJsonDelta { partial_json: String },
    }

    #[derive(Deserialize, Debug)]
    struct MessageDelta {
        stop_reason: Option<String>,
        stop_sequence: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct StreamingError {
        #[serde(rename = "type")]
        typ: String,
        message: String,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wiremock::MockServer;

    use super::*;
    use crate::testing::test_fixture_response;

    async fn run_fixture_test(test_name: &str, stream: bool, response: &str) {
        let server = MockServer::start().await;
        let mut provider = super::Anthropic::new(reqwest::Client::new(), Some("token".to_string()));
        provider.url = format!("{}/v1/messages", server.uri());

        let provider = Arc::new(provider) as Arc<dyn ChatModelProvider>;
        test_fixture_response(
            test_name,
            server,
            "/v1/messages",
            provider,
            stream,
            response,
        )
        .await
    }

    #[tokio::test]
    async fn text_streaming() {
        run_fixture_test(
            "anthropic_text_streaming",
            true,
            include_str!("./fixtures/anthropic_text_response_streaming.txt"),
        )
        .await
    }

    #[tokio::test]
    async fn text_nonstreaming() {
        run_fixture_test(
            "anthropic_text_nonstreaming",
            false,
            include_str!("./fixtures/anthropic_text_response_nonstreaming.json"),
        )
        .await
    }

    #[tokio::test]
    async fn tool_calls_streaming() {
        run_fixture_test(
            "anthropic_tool_calls_streaming",
            true,
            include_str!("./fixtures/anthropic_tools_response_streaming.txt"),
        )
        .await
    }

    #[tokio::test]
    async fn tool_calls_nonstreaming() {
        run_fixture_test(
            "anthropic_tool_calls_nonstreaming",
            false,
            include_str!("./fixtures/anthropic_tools_response_nonstreaming.json"),
        )
        .await
    }
}
