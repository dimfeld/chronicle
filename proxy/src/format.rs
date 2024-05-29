//! The common format for requests. This mostly hews to the OpenAI format except
//! some response fields are made optional to accomodate different model providers.

use std::{borrow::Cow, collections::BTreeMap};

use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub usage: UsageResponse,
}

pub type StreamingChatResponse = ChatResponse<ChatChoiceDelta>;
pub type SingleChatResponse = ChatResponse<ChatChoice>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatChoiceDelta {
    pub index: usize,
    pub delta: ChatMessage,
    pub finish_reason: Option<String>,
}

/// A single message in a chat
#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UsageResponse {
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
}

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
    // /// Send the response back as a stream of chunks.
    // #[serde(default)]
    // pub stream: bool,
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
    pub id: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub function: ToolCallFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}
