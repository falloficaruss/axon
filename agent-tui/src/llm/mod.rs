//! LLM integration module
//!
//! This module handles communication with LLM providers (currently OpenAI).

#![allow(dead_code)]

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

#[cfg(any(test, feature = "mock-llm"))]
use std::sync::{Arc, Mutex};
#[cfg(any(test, feature = "mock-llm"))]
use tokio::sync::RwLock;

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

/// Mock LLM client for testing
/// 
/// This client simulates LLM responses without making actual API calls.
/// It supports configurable responses, streaming simulation, and call tracking.
#[cfg(any(test, feature = "mock-llm"))]
#[derive(Clone)]
pub struct MockLlmClient {
    /// Default response to return when no specific response is configured
    default_response: Arc<RwLock<String>>,
    /// Track all calls made to this mock for assertions
    call_history: Arc<Mutex<Vec<MockLlmCall>>>,
    /// Simulated delay for responses (in milliseconds)
    latency_ms: Arc<RwLock<u64>>,
    /// Whether to simulate streaming responses
    streaming_enabled: Arc<RwLock<bool>>,
}

/// Represents a call made to the mock LLM client
#[cfg(any(test, feature = "mock-llm"))]
#[derive(Debug, Clone)]
pub struct MockLlmCall {
    pub messages: Vec<Message>,
    pub is_streaming: bool,
}

#[cfg(any(test, feature = "mock-llm"))]
impl MockLlmClient {
    /// Create a new mock LLM client with default response
    pub fn new(default_response: &str) -> Self {
        Self {
            default_response: Arc::new(RwLock::new(default_response.to_string())),
            call_history: Arc::new(Mutex::new(Vec::new())),
            latency_ms: Arc::new(RwLock::new(0)),
            streaming_enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// Set the default response text
    pub async fn set_response(&self, response: &str) {
        let mut resp = self.default_response.write().await;
        *resp = response.to_string();
    }

    /// Set simulated latency in milliseconds
    pub async fn set_latency(&self, latency_ms: u64) {
        let mut lat = self.latency_ms.write().await;
        *lat = latency_ms;
    }

    /// Enable or disable streaming simulation
    pub async fn set_streaming(&self, enabled: bool) {
        let mut stream = self.streaming_enabled.write().await;
        *stream = enabled;
    }

    /// Get the call history for assertions
    pub fn get_call_history(&self) -> Vec<MockLlmCall> {
        self.call_history.lock().unwrap().clone()
    }

    /// Clear the call history
    pub fn clear_history(&self) {
        self.call_history.lock().unwrap().clear();
    }

    /// Get the number of calls made
    pub fn call_count(&self) -> usize {
        self.call_history.lock().unwrap().len()
    }

    /// Get the last message sent by the user
    pub fn get_last_user_message(&self) -> Option<String> {
        self.call_history.lock().unwrap().last().and_then(|call| {
            call.messages.iter()
                .filter(|m| m.role == MessageRole::User)
                .last()
                .map(|m| m.content.clone())
        })
    }

    /// Record a call in the history
    fn record_call(&self, messages: &[Message], is_streaming: bool) {
        let call = MockLlmCall {
            messages: messages.to_vec(),
            is_streaming,
        };
        self.call_history.lock().unwrap().push(call);
    }

    /// Send a message and get a response (mock implementation)
    pub async fn send_message(&self, messages: &[Message]) -> Result<String> {
        self.record_call(messages, false);

        // Simulate latency
        let latency = *self.latency_ms.read().await;
        if latency > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(latency)).await;
        }

        // Return the configured response
        let response = self.default_response.read().await.clone();
        Ok(response)
    }

    /// Send a streaming message (mock implementation)
    pub async fn send_message_streaming(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        self.record_call(messages, true);

        // Simulate latency
        let latency = *self.latency_ms.read().await;
        
        let response = self.default_response.read().await.clone();
        
        // Create a stream that yields characters one by one to simulate typing
        let chars: Vec<char> = response.chars().collect();
        let stream = futures::stream::iter(chars.into_iter().map(move |c| {
            Ok(c.to_string())
        }));

        let boxed_stream: Pin<Box<dyn Stream<Item = Result<String>> + Send>> = Box::pin(stream);
        
        if latency > 0 {
            // For simplicity, we don't add per-character latency in this mock
            // but the total stream could be delayed if needed
        }

        Ok(boxed_stream)
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

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_basic() {
        let mock = MockLlmClient::new("Hello, I am a mock response!");
        
        let messages = vec![Message::user("Test message")];
        let response = mock.send_message(&messages).await.unwrap();
        
        assert_eq!(response, "Hello, I am a mock response!");
        assert_eq!(mock.call_count(), 1);
    }

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_set_response() {
        let mock = MockLlmClient::new("Initial response");
        
        mock.set_response("Updated response").await;
        
        let messages = vec![Message::user("Test")];
        let response = mock.send_message(&messages).await.unwrap();
        
        assert_eq!(response, "Updated response");
    }

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_call_history() {
        let mock = MockLlmClient::new("Test response");
        
        let messages1 = vec![Message::user("First message")];
        let messages2 = vec![Message::user("Second message"), Message::agent("Response", "agent")];
        
        mock.send_message(&messages1).await.unwrap();
        mock.send_message(&messages2).await.unwrap();
        
        assert_eq!(mock.call_count(), 2);
        
        let history = mock.get_call_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].messages.len(), 1);
        assert_eq!(history[1].messages.len(), 2);
    }

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_get_last_user_message() {
        let mock = MockLlmClient::new("Response");
        
        mock.send_message(&vec![Message::user("First")]).await.unwrap();
        mock.send_message(&vec![Message::user("Second")]).await.unwrap();
        
        let last_user_msg = mock.get_last_user_message().unwrap();
        assert_eq!(last_user_msg, "Second");
    }

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_streaming() {
        let mock = MockLlmClient::new("Streaming response");
        
        let messages = vec![Message::user("Test")];
        let mut stream = mock.send_message_streaming(&messages).await.unwrap();
        
        let mut collected = String::new();
        while let Some(chunk) = stream.next().await {
            collected.push_str(&chunk.unwrap());
        }
        
        assert_eq!(collected, "Streaming response");
        assert_eq!(mock.call_count(), 1);
        assert!(mock.get_call_history()[0].is_streaming);
    }

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_clear_history() {
        let mock = MockLlmClient::new("Response");
        
        mock.send_message(&vec![Message::user("Test")]).await.unwrap();
        assert_eq!(mock.call_count(), 1);
        
        mock.clear_history();
        assert_eq!(mock.call_count(), 0);
    }
}
