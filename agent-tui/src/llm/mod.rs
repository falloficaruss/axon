//! LLM integration module
//!
//! This module handles communication with LLM providers (currently OpenAI).

use anyhow::{anyhow, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, CreateChatCompletionRequestArgs,
    },
    Client,
};
use futures::{Stream, StreamExt};
use std::pin::Pin;

use crate::types::{Message, MessageRole};

/// LLM client for making API calls
pub struct LlmClient {
    client: Client<OpenAIConfig>,
    model: String,
    max_tokens: u32,
    temperature: f32,
}

impl LlmClient {
    /// Create a new LLM client
    pub fn new(api_key: &str, model: &str, max_tokens: u32, temperature: f32) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        Self {
            client,
            model: model.to_string(),
            max_tokens,
            temperature,
        }
    }

    /// Create a new LLM client from environment (reads OPENAI_API_KEY)
    pub fn from_env(model: &str, max_tokens: u32, temperature: f32) -> Self {
        let client = Client::new();

        Self {
            client,
            model: model.to_string(),
            max_tokens,
            temperature,
        }
    }

    /// Send a message and get a response
    pub async fn send_message(&self, messages: &[Message]) -> Result<String> {
        let request_messages = Self::convert_messages(messages);

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(request_messages)
            .max_tokens(self.max_tokens)
            .temperature(self.temperature)
            .build()?;

        let response = self.client.chat().create(request).await?;

        // Extract the content from the first choice
        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| anyhow!("No response from LLM"))?;

        Ok(content)
    }

    /// Send a streaming message
    pub async fn send_message_streaming(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let request_messages = Self::convert_messages(messages);

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(request_messages)
            .max_tokens(self.max_tokens)
            .temperature(self.temperature)
            .stream(true)
            .build()?;

        let stream = self.client.chat().create_stream(request).await?;

        // Transform the stream to extract content strings
        let content_stream = stream.filter_map(|chunk| async move {
            match chunk {
                Ok(chunk) => {
                    let content = chunk
                        .choices
                        .first()
                        .and_then(|choice| choice.delta.content.clone());
                    content.map(Ok)
                }
                Err(e) => Some(Err(anyhow!("Stream error: {}", e))),
            }
        });

        Ok(Box::pin(content_stream))
    }

    /// Convert our internal Message types to OpenAI's message types
    fn convert_messages(messages: &[Message]) -> Vec<ChatCompletionRequestMessage> {
        messages
            .iter()
            .map(|msg| match msg.role {
                MessageRole::User => {
                    let content = ChatCompletionRequestUserMessageContent::Text(msg.content.clone());
                    ChatCompletionRequestUserMessage {
                        content,
                        name: msg.agent_id.clone(),
                    }
                    .into()
                }
                MessageRole::Agent => {
                    #[allow(deprecated)]
                    ChatCompletionRequestMessage::Assistant(
                        ChatCompletionRequestAssistantMessage {
                            content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                                msg.content.clone(),
                            )),
                            name: msg.agent_id.clone(),
                            tool_calls: None,
                            function_call: None,
                            refusal: None,
                            audio: None,
                        },
                    )
                }
                MessageRole::System => {
                    let content =
                        ChatCompletionRequestSystemMessageContent::Text(msg.content.clone());
                    ChatCompletionRequestSystemMessage {
                        content,
                        name: msg.agent_id.clone(),
                    }
                    .into()
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("You are a helpful assistant."),
            Message::user("Hello!"),
            Message::agent("Hi there!", "assistant"),
        ];

        let converted = LlmClient::convert_messages(&messages);
        assert_eq!(converted.len(), 3);
    }
}
