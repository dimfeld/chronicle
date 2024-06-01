//! The common format for requests. This mostly hews to the OpenAI format except
//! some response fields are made optional to accomodate different model providers.

use std::{borrow::Cow, collections::BTreeMap};

use error_stack::Report;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::providers::ProviderError;

/// A chat response, in non-chunked format
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatResponse<CHOICE> {
    // Omitted certain fields that aren't really useful
    // id: String,
    // object: String,
    pub created: u64,
    pub model: Option<String>,
    pub system_fingerprint: Option<String>,
    pub choices: Vec<CHOICE>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageResponse>,
}

pub type StreamingChatResponse = ChatResponse<ChatChoiceDelta>;
pub type SingleChatResponse = ChatResponse<ChatChoice>;

impl ChatResponse<ChatChoice> {
    ///Create a new, empty ChatResponse designed for collecting streaming chat responses.
    pub fn new_for_collection(num_choices: usize) -> Self {
        SingleChatResponse {
            created: 0,
            model: None,
            system_fingerprint: None,
            choices: Vec::with_capacity(num_choices),
            usage: Some(UsageResponse {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            }),
        }
    }

    pub fn merge_delta(&mut self, chunk: &ChatResponse<ChatChoiceDelta>) {
        if self.created == 0 {
            self.created = chunk.created;
        }

        if self.model.is_none() {
            self.model = chunk.model.clone();
        }

        if self.system_fingerprint.is_none() {
            self.system_fingerprint = chunk.system_fingerprint.clone();
        }

        if let Some(delta_usage) = chunk.usage.as_ref() {
            if let Some(usage) = self.usage.as_mut() {
                usage.merge(delta_usage);
            } else {
                self.usage = chunk.usage.clone();
            }
        }

        for choice in chunk.choices.iter() {
            if choice.index >= self.choices.len() {
                // Resize to either the index mentioned here, or the total number of choices in
                // this message. This way we only resize once.
                let new_size = std::cmp::max(chunk.choices.len(), choice.index + 1);
                self.choices.resize(new_size, ChatChoice::default());

                for i in 0..self.choices.len() {
                    self.choices[i].index = i;
                }
            }

            let c = &mut self.choices[choice.index];
            c.message.add_delta(&choice.delta);

            if let Some(finish) = choice.finish_reason.as_ref() {
                c.finish_reason = finish.clone();
            }
        }
    }
}

/// For when we need to make a non-streaming chat response appear like it was a streaming response
impl From<SingleChatResponse> for StreamingChatResponse {
    fn from(value: SingleChatResponse) -> Self {
        ChatResponse {
            created: value.created,
            model: value.model,
            system_fingerprint: value.system_fingerprint,
            choices: value
                .choices
                .into_iter()
                .map(|c| ChatChoiceDelta {
                    index: c.index,
                    delta: c.message,
                    finish_reason: Some(c.finish_reason),
                })
                .collect(),
            usage: value.usage,
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ChatChoiceDelta {
    pub index: usize,
    pub delta: ChatMessage,
    pub finish_reason: Option<String>,
}

/// A single message in a chat
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ChatMessage {
    pub role: Option<String>,
    /// Some providers support this natively. For those that don't, the name
    /// will be prepended to the message using the format "{name}: {content}".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl ChatMessage {
    /// Merge a chat delta into this message. This replaces most fields, but will concatenate
    /// message content.
    pub fn add_delta(&mut self, delta: &ChatMessage) {
        if self.role.is_none() {
            self.role = delta.role.clone();
        }
        if self.name.is_none() {
            self.name = delta.name.clone();
        }

        match (&mut self.content, &delta.content) {
            (Some(content), Some(new_content)) => content.push_str(new_content),
            (None, Some(new_content)) => {
                self.content = Some(new_content.clone());
            }
            _ => {}
        }

        for tool_call in &delta.tool_calls {
            let Some(index) = tool_call.index else {
                // Tool call chunks must always have an index
                continue;
            };
            if self.tool_calls.len() <= index {
                self.tool_calls.resize(
                    index + 1,
                    ToolCall {
                        index: None,
                        id: None,
                        typ: None,
                        function: ToolCallFunction {
                            name: None,
                            arguments: None,
                        },
                    },
                );
            }

            self.tool_calls[index].merge_delta(tool_call);
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UsageResponse {
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
}

impl UsageResponse {
    pub fn is_empty(&self) -> bool {
        self.prompt_tokens.is_none()
            && self.completion_tokens.is_none()
            && self.total_tokens.is_none()
    }

    /// Merge another UsageResponse into this one. Any fields present in `other` will overwrite
    /// the current values.
    pub fn merge(&mut self, other: &UsageResponse) {
        if other.prompt_tokens.is_some() {
            self.prompt_tokens = other.prompt_tokens;
        }

        if other.completion_tokens.is_some() {
            self.completion_tokens = other.completion_tokens;
        }

        if other.total_tokens.is_some() {
            self.total_tokens = other.total_tokens;
        }
    }
}

/// Metadata about the request, from the proxy.
#[derive(Debug, Clone, Serialize)]
pub struct RequestInfo {
    pub id: Uuid,
    /// Which provider was used for the successful request.
    pub provider: String,
    /// Which model was used for the request
    pub model: String,
    /// How many times we had to retry before we got a successful response.
    pub num_retries: u32,
    /// If we retried due to hitting a rate limit.
    pub was_rate_limited: bool,
}

/// Metadata about the response, from the provider.
#[derive(Debug, Clone, Serialize)]
pub struct ResponseInfo {
    /// Any other metadata from the provider that should be logged.
    pub meta: Option<serde_json::Value>,
    /// The model used for the request, as returned by the provider.
    pub model: String,
}

#[cfg_attr(test, derive(Serialize))]
#[derive(Debug, Clone)]
pub enum StreamingResponse {
    /// Metadata about the request, from the proxy. This will always be the first message in the
    /// stream.
    RequestInfo(RequestInfo),
    /// A chunk of a streaming response.
    Chunk(StreamingChatResponse),
    /// The chat response is completely in this one message. Used for non-streaming requests.
    Single(SingleChatResponse),
    /// Metadata about the response, from the provider. This chunk might not be sent.
    ResponseInfo(ResponseInfo),
}

pub type StreamingResponseSender = flume::Sender<Result<StreamingResponse, Report<ProviderError>>>;
pub type StreamingResponseReceiver =
    flume::Receiver<Result<StreamingResponse, Report<ProviderError>>>;

/// For providers that conform almost, but not quite, to the OpenAI spec, these transformations
/// apply small changes that can alter the request in place to the form needed for the provider.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ChatRequestTransformation<'a> {
    /// True if the model provider supports a `name` for each message. False if name
    /// should be merged into the main content of the message when it is provided.
    pub supports_message_name: bool,
    /// True if the system message is just another message with a system role.
    /// False if it is the special `system` field.
    pub system_in_messages: bool,
    /// If the model starts with this prefix, strip it off.
    pub strip_model_prefix: Option<Cow<'a, str>>,
}

impl<'a> Default for ChatRequestTransformation<'a> {
    /// The default values match OpenAI's behavior
    fn default() -> Self {
        Self {
            supports_message_name: true,
            system_in_messages: true,
            strip_model_prefix: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    /// A separate field for system message as an alternative to specifying it in
    /// `messages`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// The model to use. This can be omitted if the proxy options specify a model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<BTreeMap<usize, f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,
    /// max_tokens is optional for some providers but you should include it.
    /// We don't require it here for compatibility when wrapping other libraries that may not be aware they
    /// are using Chronicle.
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    // todo this should be a string or a vec
    pub stop: Vec<String>,
    // stream not supported yet
    // pub stream: Option<bool>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    /// The "user" to send to the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Send the response back as a stream of chunks.
    #[serde(default)]
    pub stream: bool,
    /// For OpenAI, this lets us enable usage when streaming. Chronicle will set this
    /// automatically when appropriate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct StreamOptions {
    pub include_usage: bool,
}

impl ChatRequest {
    pub fn transform(&mut self, options: &ChatRequestTransformation) {
        let stripped = options
            .strip_model_prefix
            .as_deref()
            .zip(self.model.as_deref())
            .and_then(|(prefix, model)| model.strip_prefix(prefix));
        if let Some(stripped) = stripped {
            self.model = Some(stripped.to_string());
        }

        if !options.supports_message_name {
            self.merge_message_names();
        }

        if options.system_in_messages {
            self.move_system_to_messages();
        } else {
            self.move_system_message_to_top_level();
        }
    }

    /// For providers that don't support a `name` field in their message,
    /// convert messages with names to the format "{name}: {content}
    pub fn merge_message_names(&mut self) {
        for message in self.messages.iter_mut() {
            if let Some(name) = message.name.take() {
                message.content = message.content.as_deref().map(|c| format!("{name}: {c}"));
            }
        }
    }

    /// Move the entry in the `system` field to the start of `messages`.
    pub fn move_system_to_messages(&mut self) {
        let system = self.system.take();
        if let Some(system) = system {
            self.messages = std::iter::once(ChatMessage {
                role: Some("system".to_string()),
                content: Some(system),
                tool_calls: Vec::new(),
                name: None,
            })
            .chain(self.messages.drain(..))
            .collect();
        }
    }

    /// Move a `system` role [ChatMessage] to the `system` field
    pub fn move_system_message_to_top_level(&mut self) {
        if self
            .messages
            .get(0)
            .map(|m| m.role.as_deref().unwrap_or_default() == "system")
            .unwrap_or(false)
        {
            let system = self.messages.remove(0);
            self.system = system.content;
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tool {
    #[serde(rename = "type")]
    pub typ: String,
    pub function: FunctionTool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionTool {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub typ: Option<String>,
    pub function: ToolCallFunction,
}

impl ToolCall {
    fn merge_delta(&mut self, delta: &ToolCall) {
        if self.index.is_none() {
            self.index = delta.index;
        }
        if self.id.is_none() {
            self.id = delta.id.clone();
        }
        if self.typ.is_none() {
            self.typ = delta.typ.clone();
        }
        if self.function.name.is_none() {
            self.function.name = delta.function.name.clone();
        }

        if self.function.arguments.is_none() {
            self.function.arguments = delta.function.arguments.clone();
        } else if delta.function.arguments.is_some() {
            self.function
                .arguments
                .as_mut()
                .unwrap()
                .push_str(&delta.function.arguments.as_ref().unwrap());
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCallFunction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}
