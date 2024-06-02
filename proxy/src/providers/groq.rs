use bytes::Bytes;
use chrono::Utc;
use error_stack::{Report, ResultExt};
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;

use super::{
    openai::handle_rate_limit_headers, ChatModelProvider, ProviderError, ProviderErrorKind,
    SendRequestOptions,
};
use crate::{
    format::{
        ChatChoice, ChatMessage, ChatRequestTransformation, ChatResponse, ResponseInfo,
        SingleChatResponse, StreamingResponse, StreamingResponseSender, ToolCall, ToolCallFunction,
        UsageResponse,
    },
    request::{parse_response_json, send_standard_request},
};

#[derive(Debug)]
pub struct Groq {
    client: reqwest::Client,
    token: Option<String>,
}

impl Groq {
    pub fn new(client: reqwest::Client, token: Option<String>) -> Self {
        Self {
            client,
            token: token.or_else(|| std::env::var("GROQ_API_KEY").ok()),
        }
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for Groq {
    fn name(&self) -> &str {
        "groq"
    }

    fn label(&self) -> &str {
        "Groq"
    }

    async fn send_request(
        &self,
        SendRequestOptions {
            override_url,
            timeout,
            api_key,
            mut body,
        }: SendRequestOptions,
        chunk_tx: StreamingResponseSender,
    ) -> Result<(), Report<ProviderError>> {
        body.transform(&ChatRequestTransformation {
            supports_message_name: true,
            system_in_messages: true,
            strip_model_prefix: Some("groq/".into()),
        });

        // Groq prohibits sending these fields
        body.logprobs = None;
        body.logit_bias = None;
        body.top_logprobs = None;
        body.n = None;
        // TODO enable streaming
        body.stream = false;

        let bytes = serde_json::to_vec(&body).change_context_lazy(|| {
            ProviderError::from_kind(ProviderErrorKind::TransformingRequest)
        })?;
        let bytes = Bytes::from(bytes);

        let api_token = api_key
            .as_deref()
            .or(self.token.as_deref())
            .ok_or(ProviderError::from_kind(ProviderErrorKind::AuthMissing))?;

        let response = send_standard_request(
            timeout,
            || {
                self.client
                    .post(
                        override_url
                            .as_deref()
                            .unwrap_or("https://api.groq.com/openai/v1/chat/completions"),
                    )
                    .bearer_auth(api_token)
                    .header(CONTENT_TYPE, "application/json; charset=utf8")
            },
            handle_rate_limit_headers,
            bytes,
        )
        .await;

        let response = match response {
            Err(e) if matches!(e.current_context().kind, ProviderErrorKind::BadInput) => {
                let err = e.current_context();
                // 2024-05 Groq's models sometimes incorrectly fail on tool calls, when the model
                // accurately generated the tool call but wrapped it in markdown triple ticks or
                // XML or something similar. In this case, attempt to extract the tool call and
                // proceed.
                let recovered_tool_use = err
                    .body
                    .as_ref()
                    .map(|b| &b["error"])
                    .filter(|b| b["code"] == "tool_use_failed")
                    .and_then(|b| b["failed_generation"].as_str())
                    .and_then(RecoveredToolCalls::recover)
                    .map(|tool_calls| ChatResponse {
                        created: Utc::now().timestamp() as u64,
                        model: body.model.clone(),
                        system_fingerprint: None,
                        choices: vec![ChatChoice {
                            index: 0,
                            message: ChatMessage {
                                role: Some("assistant".to_string()),
                                tool_calls: tool_calls.tool_calls,
                                content: None,
                                name: None,
                            },
                            finish_reason: "tool_calls".to_string(),
                        }],
                        usage: Some(UsageResponse {
                            // TODO This should be better
                            prompt_tokens: None,
                            completion_tokens: None,
                            total_tokens: None,
                        }),
                    });

                recovered_tool_use.ok_or(e)
            }
            Err(e) => Err(e),
            Ok((response, latency)) => {
                let result = parse_response_json::<SingleChatResponse>(response, latency).await?;

                Ok(result)
            }
        };

        let result = response?;

        // TODO Actually support streaming
        let info = StreamingResponse::ResponseInfo(ResponseInfo {
            model: result.model.clone().unwrap_or_default(),
            meta: None,
        });

        chunk_tx
            .send_async(Ok(StreamingResponse::Single(result.into())))
            .await
            .ok();
        chunk_tx.send_async(Ok(info)).await.ok();
        Ok(())
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("groq/")
    }
}

#[derive(Debug, Deserialize)]
struct RecoveredToolCalls {
    tool_calls: Vec<ToolCall>,
}

impl RecoveredToolCalls {
    fn recover(body: &str) -> Option<Self> {
        let first_brace = body.find('{').unwrap_or_default();
        let last_brace = body.rfind('}').unwrap_or_default();
        if last_brace <= first_brace {
            return None;
        }

        let parsed: Option<RecoveredToolCalls> =
            serde_json::from_str::<InternalToolCallResponse>(&body[first_brace..=last_brace])
                .ok()
                .map(|tc| {
                    let tool_calls = tc
                        .tool_calls
                        .into_iter()
                        .map(|tc| ToolCall {
                            index: None,
                            id: Some(uuid::Uuid::new_v4().to_string()),
                            typ: Some(tc.typ),
                            function: ToolCallFunction {
                                name: Some(tc.function.name),
                                arguments: Some(
                                    tc.parameters
                                        .and_then(|p| serde_json::to_string(&p).ok())
                                        .unwrap_or_else(|| "{}".to_string()),
                                ),
                            },
                        })
                        .collect::<Vec<_>>();

                    tracing::warn!("Recovered from Groq false error on invalid tool response");
                    RecoveredToolCalls { tool_calls }
                });

        parsed
    }
}

#[derive(Deserialize, Debug)]
struct InternalToolCallResponse {
    tool_calls: Vec<InternalToolCall>,
}

#[derive(Deserialize, Debug)]
struct InternalToolCall {
    #[serde(rename = "type")]
    typ: String,
    function: InternalToolCallFunction,
    parameters: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
struct InternalToolCallFunction {
    name: String,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wiremock::MockServer;

    use super::*;
    use crate::testing::test_fixture_response;

    async fn run_fixture_test(test_name: &str, stream: bool, response: &str) {
        let server = MockServer::start().await;
        let provider = super::Groq::new(reqwest::Client::new(), Some("token".to_string()));
        let provider = Arc::new(provider) as Arc<dyn ChatModelProvider>;
        test_fixture_response(
            test_name,
            server,
            "openai/v1/chat_completions",
            provider,
            stream,
            response,
        )
        .await
    }

    #[tokio::test]
    #[ignore = "streaming not implemented yet"]
    async fn text_streaming() {
        run_fixture_test(
            "groq_text_streaming",
            true,
            include_str!("./fixtures/groq_text_response_streaming.txt"),
        )
        .await
    }

    #[tokio::test]
    async fn text_nonstreaming() {
        run_fixture_test(
            "groq_text_nonstreaming",
            false,
            include_str!("./fixtures/groq_text_response_nonstreaming.json"),
        )
        .await
    }

    #[tokio::test]
    #[ignore = "streaming not implemented yet"]
    async fn tool_calls_streaming() {
        run_fixture_test(
            "groq_tool_calls_streaming",
            true,
            include_str!("./fixtures/groq_tools_response_streaming.txt"),
        )
        .await
    }

    #[tokio::test]
    async fn tool_calls_nonstreaming() {
        run_fixture_test(
            "groq_tool_calls_nonstreaming",
            false,
            include_str!("./fixtures/groq_tools_response_nonstreaming.json"),
        )
        .await
    }
}
