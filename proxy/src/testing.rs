use std::{
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use error_stack::Report;

use crate::{
    format::{
        ChatChoice, ChatMessage, ChatResponse, ResponseInfo, StreamingChatResponse,
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
