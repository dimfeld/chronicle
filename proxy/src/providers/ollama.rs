use std::{borrow::Cow, time::Duration};

use bytes::Bytes;
use chrono::Utc;
use error_stack::{Report, ResultExt};
use futures::TryStreamExt;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncBufReadExt;

use super::{ChatModelProvider, ProviderError, ProviderErrorKind, SendRequestOptions};
use crate::{
    format::{
        ChatChoice, ChatChoiceDelta, ChatMessage, ChatRequestTransformation, ChatResponse,
        ResponseInfo, StreamingChatResponse, StreamingResponse, StreamingResponseSender,
        UsageResponse,
    },
    request::{parse_response_json, send_standard_request},
};

#[derive(Debug)]
pub struct Ollama {
    pub url: String,
    client: reqwest::Client,
}

impl Ollama {
    pub fn new(client: reqwest::Client, url: Option<String>) -> Self {
        let url = url.as_deref().unwrap_or("http://localhost:11434");
        let url = format!("{url}/api/chat");

        Self { url, client }
    }

    fn handle_streaming_line(
        now: i64,
        line: Result<String, std::io::Error>,
    ) -> Result<(StreamingChatResponse, Option<ResponseInfo>), Report<ProviderError>> {
        let line =
            line.change_context_lazy(|| ProviderError::from_kind(ProviderErrorKind::Server))?;

        let result: OllamaResponse =
            serde_json::from_str(&line).change_context_lazy(|| ProviderError {
                kind: ProviderErrorKind::ParsingResponse,
                status_code: None,
                body: serde_json::from_str(&line).ok(),
                latency: Duration::ZERO,
            })?;

        let response_info = result.done.filter(|x| *x).map(|_| result.response_info());

        let response = StreamingChatResponse {
            created: now as u64,
            model: Some(result.model.clone()),
            system_fingerprint: None,
            choices: vec![ChatChoiceDelta {
                index: 0,
                finish_reason: result.done_reason,
                delta: result.message,
            }],
            usage: Some(UsageResponse {
                prompt_tokens: result.prompt_eval_count.map(|c| c as usize),
                completion_tokens: result.eval_count.map(|c| c as usize),
                total_tokens: None,
            }),
        };

        Ok((response, response_info))
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for Ollama {
    fn name(&self) -> &str {
        "ollama"
    }

    fn label(&self) -> &str {
        "Ollama"
    }

    async fn send_request(
        &self,
        SendRequestOptions {
            timeout,
            override_url,
            mut body,
            ..
        }: SendRequestOptions,
        chunk_tx: StreamingResponseSender,
    ) -> Result<(), Report<ProviderError>> {
        body.transform(&ChatRequestTransformation {
            supports_message_name: false,
            system_in_messages: true,
            strip_model_prefix: Some(Cow::Borrowed("ollama/")),
        });

        let stream = body.stream;
        let model = body
            .model
            .ok_or_else(|| ProviderError::from_kind(ProviderErrorKind::TransformingRequest))
            .attach_printable("Model not specified ")?;

        let request = OllamaChatRequest {
            model,
            messages: body.messages,
            options: OllamaModelOptions {
                temperature: body.temperature,
                top_p: body.top_p,
                stop: body.stop,
                num_predict: body.max_tokens,
                frequency_penalty: body.frequency_penalty,
                presence_penalty: body.presence_penalty,
                seed: body.seed,
            },
            stream,
            keep_alive: None,
        };

        let body = serde_json::to_vec(&request).change_context_lazy(|| {
            ProviderError::from_kind(ProviderErrorKind::TransformingRequest)
        })?;
        let body = Bytes::from(body);

        let now = Utc::now().timestamp();
        let (response, latency) = send_standard_request(
            timeout,
            || {
                self.client
                    .post(override_url.as_deref().unwrap_or(&self.url.as_str()))
                    .timeout(timeout)
                    .header(CONTENT_TYPE, "application/json; charset=utf8")
            },
            // Ollama never returns a 429
            |_| None,
            body,
        )
        .await?;

        if stream {
            tokio::task::spawn(async move {
                let stream = response
                    .bytes_stream()
                    .map_err(|e| std::io::Error::other(e));
                let mut stream = tokio_util::io::StreamReader::new(stream).lines();
                while let Some(line) = stream.next_line().await.transpose() {
                    let chunk = Self::handle_streaming_line(now, line);
                    match chunk {
                        Ok((chunk, info)) => {
                            chunk_tx
                                .send_async(Ok(StreamingResponse::Chunk(chunk)))
                                .await
                                .ok();
                            if let Some(info) = info {
                                chunk_tx
                                    .send_async(Ok(StreamingResponse::ResponseInfo(info)))
                                    .await
                                    .ok();
                            }
                        }
                        Err(e) => {
                            chunk_tx.send_async(Err(e)).await.ok();
                        }
                    }
                }
            });
        } else {
            let result: OllamaResponse = parse_response_json(response, latency).await?;

            let info = StreamingResponse::ResponseInfo(result.response_info());
            let response = ChatResponse {
                created: now as u64,
                model: Some(result.model),
                system_fingerprint: None,
                choices: vec![ChatChoice {
                    index: 0,
                    finish_reason: result.done_reason.unwrap_or_else(|| "stop".to_string()),
                    message: result.message,
                }],
                usage: Some(UsageResponse {
                    prompt_tokens: result.prompt_eval_count.map(|c| c as usize),
                    completion_tokens: result.eval_count.map(|c| c as usize),
                    total_tokens: None,
                }),
            };

            chunk_tx
                .send_async(Ok(StreamingResponse::Single(response)))
                .await
                .ok();
            chunk_tx.send_async(Ok(info)).await.ok();
        }

        Ok(())
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("ollama/")
    }
}

#[derive(Serialize, Debug)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    options: OllamaModelOptions,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    keep_alive: Option<String>,
}

#[derive(Serialize, Debug)]
struct OllamaModelOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct OllamaResponse {
    // created_at: String,
    model: String,
    message: ChatMessage,
    done_reason: Option<String>,
    // total_duration: u64,
    load_duration: Option<u64>,
    prompt_eval_count: Option<u64>,
    prompt_eval_duration: Option<u64>,
    eval_count: Option<u64>,
    eval_duration: Option<u64>,
    done: Option<bool>,
}

impl OllamaResponse {
    fn response_info(&self) -> ResponseInfo {
        let meta = json!({
            "load_duration": self.load_duration,
            "prompt_eval_duration": self.prompt_eval_duration,
            "eval_duration": self.eval_duration,
        });
        ResponseInfo {
            meta: Some(meta),
            model: self.model.clone(),
        }
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
        let provider = super::Ollama::new(reqwest::Client::new(), None);

        let provider = Arc::new(provider) as Arc<dyn ChatModelProvider>;
        test_fixture_response(test_name, server, "api/chat", provider, stream, response).await
    }

    #[tokio::test]
    async fn text_streaming() {
        run_fixture_test(
            "ollama_text_streaming",
            true,
            include_str!("./fixtures/ollama_text_response_streaming.txt"),
        )
        .await
    }

    #[tokio::test]
    async fn text_nonstreaming() {
        run_fixture_test(
            "ollama_text_nonstreaming",
            false,
            include_str!("./fixtures/ollama_text_response_nonstreaming.json"),
        )
        .await
    }
}
