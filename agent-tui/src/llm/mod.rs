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
use tracing::warn;
use crate::types::{Message, MessageRole};

#[cfg(any(test, feature = "mock-llm"))]
use std::sync::{Arc, Mutex};
#[cfg(any(test, feature = "mock-llm"))]
use tokio::sync::RwLock;

use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    /// Send a message and get a response
    async fn send_message(&self, messages: &[Message]) -> Result<String>;

    /// Send a streaming message
    async fn send_message_streaming(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
}

/// LLM client for making API calls
pub struct LlmClient {
    client: Client<OpenAIConfig>,
    model: String,
    max_tokens: u32,
    temperature: f32,
    retry_max: u32,
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
            retry_max: 3,
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
            retry_max: 3,
        }
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

#[async_trait]
impl LlmProvider for LlmClient {
    /// Send a message and get a response
    async fn send_message(&self, messages: &[Message]) -> Result<String> {
        let request_messages = Self::convert_messages(messages);

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(request_messages)
            .max_tokens(self.max_tokens)
            .temperature(self.temperature)
            .build()?;

        let mut backoff = 1;
        let mut last_error = None;

        for attempt in 0..=self.retry_max {
            if attempt > 0 {
                tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
                backoff *= 2;
                tracing::warn!("Retrying LLM call (attempt {})", attempt);
            }

            match self.client.chat().create(request.clone()).await {
                Ok(response) => {
                    // Extract the content from the first choice
                    let content = response
                        .choices
                        .first()
                        .and_then(|choice| choice.message.content.clone())
                        .ok_or_else(|| anyhow!("No response from LLM"))?;

                    return Ok(content);
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(anyhow!("LLM call failed after {} retries: {:?}", self.retry_max, last_error))
    }

    /// Send a streaming message
    async fn send_message_streaming(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        const MAX_RETRIES: u32 = 3;
        const INITIAL_BACKOFF_MS: u64 = 1000;

        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            let request_messages = Self::convert_messages(messages);

            let request = CreateChatCompletionRequestArgs::default()
                .model(&self.model)
                .messages(request_messages)
                .max_tokens(self.max_tokens)
                .temperature(self.temperature)
                .stream(true)
                .build()?;

            match self.client.chat().create_stream(request).await {
                Ok(stream) => {
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

                    return Ok(Box::pin(content_stream));
                }
                Err(e) => {
                    warn!("Streaming request attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    
                    if attempt < MAX_RETRIES - 1 {
                        let backoff_ms = INITIAL_BACKOFF_MS * 2u64.pow(attempt);
                        tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                    }
                }
            }
        }

        Err(anyhow!("Failed after {} retries: {}", MAX_RETRIES, last_error.unwrap()))
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

    /// Return deterministic JSON for orchestrator flows that expect structured output.
    fn structured_response(messages: &[Message]) -> Option<String> {
        let prompt = messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.as_str())
            .unwrap_or_default();

        if prompt.contains("You are an AI task router") {
            return Some(
                r#"{
  "task_type": "CodeGeneration",
  "suggested_agents": [["coder", 0.95]],
  "can_parallelize": false,
  "estimated_complexity": 3,
  "requires_subtasks": false
}"#
                .to_string(),
            );
        }

        if prompt.contains("You are an expert task planner") {
            return Some(
                r#"{
  "subtasks": [
    {
      "description": "Handle the user's request",
      "task_type": "CodeGeneration",
      "suggested_agent": "coder",
      "dependencies": []
    }
  ],
  "parallel_groups": [[0]]
}"#
                .to_string(),
            );
        }

        None
    }
}

#[cfg(any(test, feature = "mock-llm"))]
#[async_trait]
impl LlmProvider for MockLlmClient {
    /// Send a message and get a response (mock implementation)
    async fn send_message(&self, messages: &[Message]) -> Result<String> {
        self.record_call(messages, false);

        // Simulate latency
        let latency = *self.latency_ms.read().await;
        if latency > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(latency)).await;
        }

        // Return a deterministic structured response for router/planner prompts.
        let response = match Self::structured_response(messages) {
            Some(response) => response,
            None => self.default_response.read().await.clone(),
        };
        Ok(response)
    }

    /// Send a streaming message (mock implementation)
    async fn send_message_streaming(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let is_streaming = *self.streaming_enabled.read().await;
        self.record_call(messages, is_streaming);

        let latency = *self.latency_ms.read().await;
        let response = self.default_response.read().await.clone();
        
        let boxed_stream: Pin<Box<dyn Stream<Item = Result<String>> + Send>> = if is_streaming {
            // Character-by-character streaming with optional latency
            if latency > 0 {
                let chars: Vec<char> = response.chars().collect();
                let delay_per_char = latency / chars.len().max(1) as u64;
                
                Box::pin(futures::stream::unfold(0, move |idx| {
                    let chars = chars.clone();
                    async move {
                        if idx < chars.len() {
                            if delay_per_char > 0 {
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay_per_char)).await;
                            }
                            let chunk = chars[idx].to_string();
                            Some((Ok(chunk), idx + 1))
                        } else {
                            None
                        }
                    }
                }))
            } else {
                let chars: Vec<Result<String, anyhow::Error>> = response
                    .chars()
                    .map(|c| Ok(c.to_string()))
                    .collect();
                Box::pin(futures::stream::iter(chars))
            }
        } else {
            // Non-streaming: return full response at once (with latency delay)
            if latency > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(latency)).await;
            }
            Box::pin(futures::stream::iter(vec![Ok(response)]))
        };

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

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_streaming_disabled() {
        let mock = MockLlmClient::new("Streaming response");
        
        mock.set_streaming(false).await;
        
        let messages = vec![Message::user("Test")];
        let mut stream = mock.send_message_streaming(&messages).await.unwrap();
        
        // When streaming disabled, should get single chunk with full response
        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk, "Streaming response");
        
        // Should not have any more chunks
        assert!(stream.next().await.is_none());
        
        // Verify call history records streaming as disabled
        let history = mock.get_call_history();
        assert_eq!(history.len(), 1);
        assert!(!history[0].is_streaming);
    }

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_streaming_enabled() {
        let mock = MockLlmClient::new("ABC");
        
        mock.set_streaming(true).await;
        
        let messages = vec![Message::user("Test")];
        let mut stream = mock.send_message_streaming(&messages).await.unwrap();
        
        // When streaming enabled, should get character-by-character chunks
        let mut collected = String::new();
        while let Some(chunk) = stream.next().await {
            collected.push_str(&chunk.unwrap());
        }
        
        assert_eq!(collected, "ABC");
        
        // Verify call history records streaming as enabled
        let history = mock.get_call_history();
        assert!(history[0].is_streaming);
    }

    #[cfg(any(test, feature = "mock-llm"))]
    #[tokio::test]
    async fn test_mock_llm_streaming_with_latency() {
        let mock = MockLlmClient::new("AB");
        
        mock.set_streaming(true).await;
        mock.set_latency(20).await; // 20ms total, 10ms per char
        
        let messages = vec![Message::user("Test")];
        
        let start = std::time::Instant::now();
        let mut stream = mock.send_message_streaming(&messages).await.unwrap();
        
        let mut collected = String::new();
        while let Some(chunk) = stream.next().await {
            collected.push_str(&chunk.unwrap());
        }
        let elapsed = start.elapsed().as_millis();
        
        assert_eq!(collected, "AB");
        // With 20ms latency and 2 chars, should take at least ~10ms
        assert!(elapsed >= 10, "Expected at least 10ms, got {}ms", elapsed);
    }
}
