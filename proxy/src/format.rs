//! The common format for requests. This mostly hews to the OpenAI format except
//! some response fields are made optional to accomodate different model providers.

use std::{borrow::Cow, collections::BTreeMap};

use error_stack::Report;
use serde::{Deserialize, Serialize};
use serde_with::{formats::PreferMany, serde_as, OneOrMany};
use uuid::Uuid;

use crate::providers::ProviderError;

/// A chat response, in non-chunked format
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatResponse<CHOICE> {
    // Omitted certain fields that aren't really useful
    // id: String,
    // object: String,
    /// Unix timestamp in seconds
    pub created: u64,
    /// The model that was used
    pub model: Option<String>,
    /// A fingerprint for the system prompt
    pub system_fingerprint: Option<String>,
    /// The chat choices
    pub choices: Vec<CHOICE>,
    /// Token usage information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageResponse>,
}

/// A chunk of streaming response
pub type StreamingChatResponse = ChatResponse<ChatChoiceDelta>;
/// A non-streaming chat response
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

    /// Merge a streaming delta into this response.
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

/// A single choice in a chat
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ChatChoice {
    /// Which choice this is
    pub index: usize,
    /// The message
    pub message: ChatMessage,
    /// The reason the chat terminated
    pub finish_reason: FinishReason,
}

/// A delta in a streaming chat choice
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ChatChoiceDelta {
    /// Which choice this is
    pub index: usize,
    /// The message
    pub delta: ChatMessage,
    /// The reason the chat terminated, if this is the final delta in the choice
    pub finish_reason: Option<FinishReason>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    #[default]
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    #[serde(untagged)]
    Other(Cow<'static, str>),
}

impl FinishReason {
    pub fn as_str(&self) -> &str {
        match self {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
            FinishReason::ContentFilter => "content_filter",
            FinishReason::ToolCalls => "tool_calls",
            FinishReason::Other(reason) => reason.as_ref(),
        }
    }
}

impl std::fmt::Display for FinishReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single message in a chat
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ChatMessage {
    /// The role of the message, such as "user" or "assistant".
    pub role: Option<String>,
    /// Some providers support this natively. For those that don't, the name
    /// will be prepended to the message using the format "{name}: {content}".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Text content of the message
    pub content: Option<String>,
    /// A tool call to be invoked
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// A tool call ID when responding to the tool call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
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

        if self.tool_call_id.is_none() {
            self.tool_call_id = delta.tool_call_id.clone();
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

/// Counts of prompt, completion, and total tokens
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UsageResponse {
    /// The number of input tokens
    pub prompt_tokens: Option<usize>,
    /// The number of output tokens
    pub completion_tokens: Option<usize>,
    /// The sum of the input and output tokens
    pub total_tokens: Option<usize>,
}

impl UsageResponse {
    /// Return true if there is no usage info recorded in this response
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
    /// A UUID assigned by Chronicle to the request, which is linked to the logged information.
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

/// Part of a streaming response, returned from the proxy
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

/// A channel on which streaming responses can be sent
pub type StreamingResponseSender = flume::Sender<Result<StreamingResponse, Report<ProviderError>>>;
/// A channel that can receive streaming responses
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

/// The request that can be submitted to the proxy, for transformation and submission to a
/// provider.
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ChatRequest {
    /// The messages in the chat so far.
    pub messages: Vec<ChatMessage>,
    /// A separate field for system message as an alternative to specifying it in
    /// `messages`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// The model to use. This can be omitted if the proxy options specify a model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// How to penalize tokens based on their frequency in the text so far
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Specific control of certain token probabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<BTreeMap<usize, f32>>,
    /// Return the logprobs of the generated tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// If `logprobs` is true, how many logprobs to return per token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,
    /// max_tokens is optional for some providers but you should include it.
    /// We don't require it here for compatibility when wrapping other libraries that may not be aware they
    /// are using Chronicle.
    pub max_tokens: Option<u32>,
    /// Generate multiple chat completions concurrently. Not every model provider supports this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// How to penalize tokens based on their existing presence in the text so far
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Force JSON output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
    /// A random seed to use when generating the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Tell the model to stop when it encounters these token sequences
    #[serde_as(as = "OneOrMany<_, PreferMany>")]
    pub stop: Vec<String>,
    /// Temperature to use when generating the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Customize the top-P probability of tokens to consider when generating the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Tools available for the model to use
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    /// Customize how the model chooses tools
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

/// Stream options for OpenAI. This is automatically set by the proxy when streaming. You can omit
/// it in your requests.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct StreamOptions {
    /// If true, include token usage in the response.
    pub include_usage: bool,
}

impl ChatRequest {
    /// Transform a chat request to fit different variations on the OpenAI format.
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
                tool_call_id: None,
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

/// Represents a tool that can be used by the OpenAI model
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tool {
    /// The type of the tool, typically "function"
    #[serde(rename = "type")]
    pub typ: String,
    /// The function details of the tool
    pub function: FunctionTool,
}

/// Represents the function details of a tool
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionTool {
    /// The name of the function
    pub name: String,
    /// An optional description of the function
    pub description: Option<String>,
    /// Optional parameters for the function, represented as a JSON value
    pub parameters: Option<serde_json::Value>,
}

/// Represents a call to a tool by the OpenAI model
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCall {
    /// The optional index of the tool call
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// The optional ID of the tool call
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The optional type of the tool call, typically "function"
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub typ: Option<String>,
    /// The function details of the tool call
    pub function: ToolCallFunction,
}

impl ToolCall {
    /// Merges a delta ToolCall into this ToolCall, updating fields if they are None
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

/// Represents the function details of a tool call
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCallFunction {
    /// The optional name of the function being called
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The optional arguments passed to the function, as a JSON string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::FinishReason;

    #[test]
    fn finish_reason_serialization() {
        let cases = vec![
            (FinishReason::Stop, "stop"),
            (FinishReason::Length, "length"),
            (FinishReason::ContentFilter, "content_filter"),
            (FinishReason::ToolCalls, "tool_calls"),
            (FinishReason::Other("custom_reason".into()), "custom_reason"),
        ];

        for (finish_reason, expected_str) in cases {
            let serialized = serde_json::to_value(&finish_reason).unwrap();
            assert_eq!(serialized, serde_json::json!(expected_str));
        }
    }

    #[test]
    fn finish_reason_deserialization() {
        let cases = vec![
            ("stop", FinishReason::Stop),
            ("length", FinishReason::Length),
            ("content_filter", FinishReason::ContentFilter),
            ("tool_calls", FinishReason::ToolCalls),
            ("custom_reason", FinishReason::Other("custom_reason".into())),
        ];

        for (json_str, expected_enum) in cases {
            let deserialized: FinishReason =
                serde_json::from_value(serde_json::json!(json_str)).unwrap();
            assert_eq!(deserialized.as_str(), expected_enum.as_str());
        }
    }
}
