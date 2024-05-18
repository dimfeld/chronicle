use bytes::Bytes;
use chrono::Utc;
use error_stack::{Report, ResultExt};
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;

use super::{
    openai::handle_rate_limit_headers, ChatModelProvider, ProviderErrorKind, ProviderResponse,
    SendRequestOptions,
};
use crate::{
    format::{
        ChatChoice, ChatMessage, ChatRequestTransformation, ChatResponse, ToolCall,
        ToolCallFunction, UsageResponse,
    },
    request::send_standard_request,
    Error,
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
    ) -> Result<ProviderResponse, Report<Error>> {
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

        let bytes = serde_json::to_vec(&body).change_context(Error::TransformingRequest)?;
        let bytes = Bytes::from(bytes);

        let api_token = api_key
            .as_deref()
            .or(self.token.as_deref())
            .ok_or(Error::MissingApiKey)?;

        let result = send_standard_request::<ChatResponse>(
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

        let result = match result {
            Err(e) if matches!(e.current_context().kind, ProviderErrorKind::BadInput) => {
                let err = e.current_context();
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
                                role: "assistant".to_string(),
                                tool_calls: tool_calls.tool_calls,
                                content: None,
                                name: None,
                            },
                            finish_reason: "tool_calls".to_string(),
                        }],
                        usage: UsageResponse {
                            // TODO This should be better
                            prompt_tokens: None,
                            completion_tokens: None,
                            total_tokens: None,
                        },
                    });

                if let Some(recovered_tool_use) = recovered_tool_use {
                    Ok((recovered_tool_use, err.latency))
                } else {
                    Err(e)
                }
            }
            _ => result,
        };

        let result = result.change_context(Error::ModelError)?;

        Ok(ProviderResponse {
            model: result.0.model.clone().or(body.model).unwrap_or_default(),
            body: result.0,
            latency: result.1,
            meta: None,
        })
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
                            id: uuid::Uuid::new_v4().to_string(),
                            typ: tc.typ,
                            function: ToolCallFunction {
                                name: tc.function.name,
                                arguments: tc
                                    .parameters
                                    .and_then(|p| serde_json::to_string(&p).ok())
                                    .unwrap_or_else(|| "{}".to_string()),
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
