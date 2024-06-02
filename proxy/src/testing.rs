use std::{
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use error_stack::Report;
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

use crate::{
    collect_response,
    format::{
        ChatChoice, ChatMessage, ChatRequest, ChatResponse, ResponseInfo, StreamingChatResponse,
        StreamingResponse, StreamingResponseSender, UsageResponse,
    },
    providers::{ChatModelProvider, ProviderError, ProviderErrorKind, SendRequestOptions},
};

#[derive(Debug)]
pub enum TestFailure {
    Transient,
    Timeout,
    BadRequest,
    RateLimit,
    Auth,
    TransformingRequest,
    TransformingResponse,
}

#[derive(Debug)]
pub struct TestProvider {
    pub name: String,
    /// Fail requests
    pub fail: Option<TestFailure>,
    pub response: String,
    pub fail_times: usize,
    pub calls: AtomicUsize,
}

impl Default for TestProvider {
    fn default() -> Self {
        Self {
            name: "test".to_string(),
            fail: None,
            response: "A response".to_string(),
            fail_times: usize::MAX,
            calls: AtomicUsize::new(0),
        }
    }
}

impl Into<Arc<dyn ChatModelProvider>> for TestProvider {
    fn into(self) -> Arc<dyn ChatModelProvider> {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for TestProvider {
    fn name(&self) -> &str {
        "test"
    }

    fn label(&self) -> &str {
        "Test"
    }

    async fn send_request(
        &self,
        options: SendRequestOptions,
        chunk_tx: StreamingResponseSender,
    ) -> Result<(), Report<ProviderError>> {
        let current_call = self
            .calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if current_call < self.fail_times {
            match self.fail {
                Some(TestFailure::Transient) => Err(ProviderErrorKind::Server),
                Some(TestFailure::Timeout) => Err(ProviderErrorKind::Timeout),
                Some(TestFailure::BadRequest) => Err(ProviderErrorKind::BadInput),
                Some(TestFailure::RateLimit) => {
                    Err(ProviderErrorKind::RateLimit { retry_after: None })
                }
                Some(TestFailure::Auth) => Err(ProviderErrorKind::AuthRejected),
                Some(TestFailure::TransformingRequest) => {
                    return Err(ProviderError::from_kind(
                        ProviderErrorKind::TransformingRequest,
                    ))?
                }
                Some(TestFailure::TransformingResponse) => {
                    return Err(ProviderError::from_kind(
                        ProviderErrorKind::TransformingResponse,
                    ))?
                }
                None => Ok(()),
            }
            .map_err(|kind| ProviderError {
                kind,
                status_code: None,
                body: None,
                latency: Duration::from_millis(500),
            })?;
        }

        let body = ChatResponse {
            created: 1,
            model: options.body.model.clone(),
            system_fingerprint: None,
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: Some("assistant".to_string()),
                    content: Some(self.response.clone()),
                    tool_calls: Vec::new(),
                    name: None,
                },
                finish_reason: "stop".to_string(),
            }],
            usage: Some(UsageResponse {
                prompt_tokens: Some(1),
                completion_tokens: Some(2),
                total_tokens: Some(3),
            }),
        };

        let response_info = StreamingResponse::ResponseInfo(ResponseInfo {
            model: options.body.model.clone().unwrap_or_default(),
            meta: None,
        });

        if options.body.stream {
            let mut body1 = StreamingChatResponse::from(body.clone());
            let mut body2 = StreamingChatResponse::from(body);

            body1.choices[0].delta.content =
                Some(self.response[0..self.response.len() / 2].to_string());
            body1.choices[0].finish_reason = None;
            body1.usage = Some(UsageResponse::default());
            body2.choices[0].delta.content =
                Some(self.response[self.response.len() / 2..].to_string());
            body2.choices[0].delta.role = None;
            body2.created = 2;

            chunk_tx
                .send_async(Ok(StreamingResponse::Chunk(body1)))
                .await
                .ok();
            chunk_tx
                .send_async(Ok(StreamingResponse::Chunk(body2)))
                .await
                .ok();
        } else {
            chunk_tx
                .send_async(Ok(StreamingResponse::Single(body)))
                .await
                .ok();
        }
        chunk_tx.send_async(Ok(response_info)).await.ok();
        Ok(())
    }

    fn is_default_for_model(&self, _model: &str) -> bool {
        false
    }
}

pub async fn test_fixture_response(
    test_name: &str,
    mock_server: MockServer,
    path: &str,
    provider: Arc<dyn ChatModelProvider>,
    stream: bool,
    response: &str,
) {
    let mut insta_settings = insta::Settings::clone_current();
    insta_settings.set_snapshot_suffix(test_name);
    insta_settings
        .bind_async(async move {
            let mime = if stream {
                "text/event-stream"
            } else {
                "application/json"
            };

            let provider_name = provider.name().to_string();
            Mock::given(matchers::method("POST"))
                .and(matchers::path(path))
                .respond_with(ResponseTemplate::new(200).set_body_raw(response, mime))
                .mount(&mock_server)
                .await;

            let proxy = super::Proxy::builder()
                .without_default_providers()
                .with_provider(provider)
                .build()
                .await
                .expect("Building proxy");

            let chan = proxy
                .send(
                    crate::ProxyRequestOptions {
                        provider: Some(provider_name),
                        override_url: Some(format!("{}/{}", mock_server.uri(), path)),
                        ..Default::default()
                    },
                    ChatRequest {
                        model: Some("me/a-test-model".to_string()),
                        messages: vec![ChatMessage {
                            role: Some("user".to_string()),
                            content: Some("hello".to_string()),
                            tool_calls: Vec::new(),
                            name: None,
                        }],
                        stream,
                        ..Default::default()
                    },
                )
                .await
                .expect("should have succeeded");

            let response = collect_response(chan, 1).await;
            if let Err(e) = &response {
                let provider_error = e.frames().find_map(|f| f.downcast_ref::<ProviderError>());
                println!("{provider_error:?}");
            }

            let mut response = response.unwrap();

            // ID will be different every time, so zero it for the snapshot
            response.request_info.id = uuid::Uuid::nil();
            response.response.created = 0;
            insta::assert_json_snapshot!(response);
        })
        .await;
}
