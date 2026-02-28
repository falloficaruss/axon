#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tokio::sync::{mpsc, RwLock};
    use crate::agent::AgentRuntimeBuilder;
    use crate::llm::MockLlmClient;
    use crate::shared::SharedMemory;
    use crate::types::{Agent, AgentRole, Task, TaskType, ExecutionContext};

    #[tokio::test]
    async fn test_reproduce_missing_metadata() {
        let (event_tx, mut event_rx) = mpsc::channel(100);
        let shared_memory = Arc::new(SharedMemory::new());
        
        // Mock LLM response with a code block
        let mock_llm = Arc::new(MockLlmClient::new("```rust:test.rs\nfn main() {}\n```"));
        
        // This is a bit tricky because AgentRuntime expects Arc<LlmClient>
        // and LlmClient is not a trait. We might need to modify LlmClient 
        // to be a trait or use a different approach for testing.
        
        // Let's check how LlmClient is used in AgentRuntime.
    }
}
