//! Agent runtime module
//!
//! This module provides the runtime environment for agents to execute tasks.
//! Agents run as async tasks and communicate via message passing.



use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, error, warn};
use anyhow::{anyhow, Result};
use futures::StreamExt;

use crate::{
    llm::{LlmClient, LlmProvider},
    shared::SharedMemory,
    types::{Agent, AgentState, AgentRole, Message, Task, TaskResult, Id, ExecutionContext},
    agent::TaskProcessor,
};

/// Commands that can be sent to an agent
#[derive(Debug, Clone)]
pub enum AgentCommand {
    /// Process a task
    ProcessTask {
        task: Box<Task>,
        context: ExecutionContext,
    },
    /// Send a chat message to the agent
    Chat {
        message: String,
        history: Vec<Message>,
        context: ExecutionContext,
    },
    /// Send a streaming chat message
    StreamChat {
        message: String,
        history: Vec<Message>,
        context: ExecutionContext,
    },
    /// Get the current state of the agent
    #[allow(dead_code)]
    GetState,
    /// Shutdown the agent
    Shutdown,
}

/// Events that an agent can emit
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Agent started processing
    Started { agent_id: Id },
    /// Agent completed processing
    Completed { 
        agent_id: Id, 
        result: TaskResult 
    },
    /// Agent emitted a message (streaming response)
    Message { 
        agent_id: Id, 
        content: String 
    },
    /// Agent encountered an error
    Error { 
        agent_id: Id, 
        error: String 
    },
    /// Agent state changed
    StateChanged { 
        agent_id: Id, 
        state: AgentState 
    },
}

/// Handle to a running agent
#[derive(Debug, Clone)]
pub struct AgentHandle {
    #[allow(dead_code)]
    pub agent_id: Id,
    pub agent_name: String,
    command_tx: mpsc::Sender<(AgentCommand, Option<oneshot::Sender<AgentResponse>>)>,
}

impl AgentHandle {
    /// Send a command to the agent
    pub async fn send_command(&self, command: AgentCommand) -> Result<AgentResponse> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send((command, Some(tx))).await
            .map_err(|_| anyhow!("Agent task has stopped"))?;
        rx.await.map_err(|_| anyhow!("Agent response channel closed"))
    }

    /// Send a command without waiting for a response
    pub async fn send_command_async(&self, command: AgentCommand) -> Result<()> {
        self.command_tx.send((command, None)).await
            .map_err(|_| anyhow!("Agent task has stopped"))
    }

    /// Process a task
    pub async fn process_task(&self, task: Task, context: ExecutionContext) -> Result<TaskResult> {
        match self.send_command(AgentCommand::ProcessTask { task: Box::new(task), context }).await? {
            AgentResponse::TaskCompleted(result) => Ok(result),
            AgentResponse::Error(e) => Err(anyhow!(e)),
            _ => Err(anyhow!("Unexpected response from agent")),
        }
    }

    /// Send a chat message
    pub async fn chat(&self, message: String, history: Vec<Message>, context: ExecutionContext) -> Result<String> {
        match self.send_command(AgentCommand::Chat { message, history, context }).await? {
            AgentResponse::ChatResponse(content) => Ok(content),
            AgentResponse::Error(e) => Err(anyhow!(e)),
            _ => Err(anyhow!("Unexpected response from agent")),
        }
    }

    /// Send a streaming chat message
    pub async fn chat_streaming(&self, message: String, history: Vec<Message>, context: ExecutionContext) -> Result<String> {
        match self.send_command(AgentCommand::StreamChat { message, history, context }).await? {
            AgentResponse::ChatResponse(content) => Ok(content),
            AgentResponse::Error(e) => Err(anyhow!(e)),
            _ => Err(anyhow!("Unexpected response from agent")),
        }
    }

    /// Get agent state
    #[allow(dead_code)]
    pub async fn get_state(&self) -> Result<AgentState> {
        match self.send_command(AgentCommand::GetState).await? {
            AgentResponse::State(state) => Ok(state),
            _ => Err(anyhow!("Unexpected response from agent")),
        }
    }

    /// Shutdown the agent
    pub async fn shutdown(&self) -> Result<()> {
        self.send_command_async(AgentCommand::Shutdown).await
    }
}

/// Response from an agent
#[derive(Debug, Clone)]
pub enum AgentResponse {
    /// Task completed successfully
    TaskCompleted(TaskResult),
    /// Chat response
    ChatResponse(String),
    /// Current state
    #[allow(dead_code)]
    State(AgentState),
    /// Error occurred
    Error(String),
    /// Acknowledgment (no data)
    Ack,
}

/// Agent runtime that manages the execution of a single agent
pub struct AgentRuntime<L: LlmProvider = LlmClient> {
    agent: Arc<RwLock<Agent>>,
    llm_client: Arc<L>,
    shared_memory: Arc<SharedMemory>,
    event_tx: mpsc::Sender<AgentEvent>,
}

impl<L: LlmProvider> AgentRuntime<L> {
    /// Create a new agent runtime
    pub fn new(
        agent: Arc<RwLock<Agent>>,
        llm_client: Arc<L>,
        shared_memory: Arc<SharedMemory>,
        event_tx: mpsc::Sender<AgentEvent>,
    ) -> Self {
        Self {
            agent,
            llm_client,
            shared_memory,
            event_tx,
        }
    }

    /// Get the appropriate processor for the agent's role
    fn get_processor(role: AgentRole) -> Option<Box<dyn TaskProcessor>> {
        match role {
            AgentRole::Coder => Some(Box::new(crate::agent::agents::coder::CoderAgent)),
            AgentRole::Planner => Some(Box::new(crate::agent::agents::PlannerAgent)),
            AgentRole::Reviewer => Some(Box::new(crate::agent::agents::ReviewerAgent)),
            AgentRole::Tester => Some(Box::new(crate::agent::agents::TesterAgent)),
            AgentRole::Explorer => Some(Box::new(crate::agent::agents::ExplorerAgent)),
            AgentRole::Integrator => Some(Box::new(crate::agent::agents::IntegratorAgent)),
        }
    }

    /// Start the agent runtime as a background task
    pub async fn spawn(self) -> AgentHandle {
        let (command_tx, mut command_rx) = mpsc::channel::<(AgentCommand, Option<oneshot::Sender<AgentResponse>>)>(32);
        
        let agent_id = {
            let agent = self.agent.read().await;
            agent.id.clone()
        };
        
        let agent_name = {
            let agent = self.agent.read().await;
            agent.name.clone()
        };

        let handle = AgentHandle {
            agent_id: agent_id.clone(),
            agent_name: agent_name.clone(),
            command_tx: command_tx.clone(),
        };

        // Spawn the agent task
        tokio::spawn(async move {
            let mut this = self;
            info!("Agent {} ({}) started", agent_name, agent_id);
            
            loop {
                match command_rx.recv().await {
                    Some((command, response_tx)) => {
                        let is_shutdown = matches!(command, AgentCommand::Shutdown);
                        let result = this.handle_command(&agent_id, command).await;
                        
                        if let Some(tx) = response_tx {
                            let _ = tx.send(result);
                        }

                        if is_shutdown {
                            info!("Agent {} received shutdown command", agent_id);
                            break;
                        }
                    }
                    None => {
                        info!("Agent {} command channel closed, shutting down", agent_id);
                        break;
                    }
                }
            }
            
            info!("Agent {} ({}) stopped", agent_name, agent_id);
        });

        handle
    }

    /// Handle a command
    async fn handle_command(
        &mut self,
        agent_id: &Id,
        command: AgentCommand,
    ) -> AgentResponse {
        match command {
            AgentCommand::ProcessTask { task, context } => {
                self.handle_process_task(agent_id, *task, context).await
            }
            AgentCommand::Chat { message, history, context } => {
                self.handle_chat(agent_id, message, history, context).await
            }
            AgentCommand::StreamChat { message, history, context } => {
                self.handle_stream_chat(agent_id, message, history, context).await
            }
            AgentCommand::GetState => {
                let state = self.agent.read().await.state;
                AgentResponse::State(state)
            }
            AgentCommand::Shutdown => {
                AgentResponse::Ack
            }
        }
    }

    /// Apply file actions (generated or edited) to the disk
    async fn apply_file_actions(&self, agent_id: &Id, response: &str, result: &mut TaskResult) {
        let metadata_keys = [("generated_files", crate::agent::agents::coder::FileOperation::Create), 
                            ("edited_files", crate::agent::agents::coder::FileOperation::Update)];
        
        for (key, operation) in metadata_keys {
            if let Some(files_value) = result.metadata.get(key) {
                if let Some(files) = files_value.as_array() {
                    if files.is_empty() { continue; }

                    info!("Agent {} {} {} files, applying changes...", agent_id, if key.starts_with("gen") { "generated" } else { "edited" }, files.len());
                    
                    match crate::agent::agents::coder::CoderAgent::extract_code_blocks(response) {
                        Ok(blocks) => {
                            let changes: Vec<_> = blocks.into_iter()
                                .filter(|b| b.file_path.is_some())
                                .map(|b| crate::agent::agents::coder::CodeChange {
                                    file_path: std::path::PathBuf::from(b.file_path.unwrap()),
                                    content: b.code,
                                    operation: operation.clone(),
                                })
                                .collect();
                            
                            // Simple validation: check if metadata file list matches extracted blocks
                            let extracted_paths: std::collections::HashSet<_> = changes.iter()
                                .map(|c| c.file_path.to_string_lossy().to_string())
                                .collect();
                            
                            for file_val in files {
                                if let Some(path) = file_val.as_str() {
                                    if !extracted_paths.contains(path) {
                                        warn!("Agent {} metadata specifies file '{}' but it was not found in code blocks", agent_id, path);
                                    }
                                }
                            }

                            if let Err(e) = crate::agent::agents::coder::CoderAgent::apply_changes(&changes) {
                                error!("Agent {} failed to apply changes: {}", agent_id, e);
                                result.success = false;
                                result.error = Some(format!("Failed to apply changes: {}", e));
                            }
                        }
                        Err(e) => {
                            error!("Agent {} failed to extract code blocks: {}", agent_id, e);
                            result.success = false;
                            result.error = Some(format!("Failed to parse code blocks: {}", e));
                        }
                    }
                }
            }
        }
    }

    /// Inject shared memory context into the system prompt
    async fn inject_shared_memory_context(&self, agent_id: &Id, session_id: &str, prompt: &mut String) {
        let mut shared_context = String::from("\n\nShared Memory Context:\n");
        let mut has_context = false;

        if let Some(val) = self.shared_memory.get_global("project_info").await {
            shared_context.push_str(&format!("Global Project Info: {}\n", val));
            has_context = true;
        }

        if let Some(val) = self.shared_memory.get_session(session_id, "relevant_context").await {
            shared_context.push_str(&format!("Session Context: {}\n", val));
            has_context = true;
        }

        if let Some(val) = self.shared_memory.get_agent(agent_id, "state").await {
            shared_context.push_str(&format!("Agent State: {}\n", val));
            has_context = true;
        }

        if has_context {
            prompt.push_str(&shared_context);
        }
    }

    /// Build the system prompt with shared memory context injected.
    async fn build_system_prompt(&self, agent_id: &Id, session_id: &str) -> String {
        let mut system_prompt = {
            let agent = self.agent.read().await;
            if agent.system_prompt.is_empty() {
                format!("You are a {} agent. {}", agent.role.as_str(), agent.role.description())
            } else {
                agent.system_prompt.clone()
            }
        };

        self.inject_shared_memory_context(agent_id, session_id, &mut system_prompt).await;

        system_prompt
    }

    /// Update the agent's state and emit a StateChanged event.
    async fn set_agent_state(&self, agent_id: &Id, state: AgentState) {
        {
            let mut agent = self.agent.write().await;
            agent.state = state;
        }
        if let Err(e) = self.event_tx.send(AgentEvent::StateChanged {
            agent_id: agent_id.clone(),
            state,
        }).await {
            warn!("Failed to send state change event for agent {}: {}", agent_id, e);
        }
    }

    /// Handle task processing
    async fn handle_process_task(
        &mut self,
        agent_id: &Id,
        task: Task,
        context: ExecutionContext,
    ) -> AgentResponse {
        // Update state to running and capture role
        let agent_role = {
            let agent = self.agent.read().await;
            agent.role
        };
        self.set_agent_state(agent_id, AgentState::Running).await;

        if let Err(e) = self.event_tx.send(AgentEvent::Started {
            agent_id: agent_id.clone(),
        }).await {
            warn!("Failed to send started event for agent {}: {}", agent_id, e);
        }

        // Build messages for LLM
        let system_prompt = self.build_system_prompt(agent_id, &context.session_id).await;
        let mut messages = vec![Message::system(&system_prompt)];
        messages.extend(context.messages);
        messages.push(Message::user(&format!("Task: {}", task.description)));

        // Send to LLM
        match self.llm_client.send_message(&messages).await {
            Ok(response) => {
                self.set_agent_state(agent_id, AgentState::Completed).await;

                // Process the response using specialized agent logic via TaskProcessor trait
                let mut result = if let Some(processor) = Self::get_processor(agent_role) {
                    match processor.process_task(&task, &response, self.shared_memory.clone()) {
                        Ok(res) => res,
                        Err(e) => {
                            warn!("Agent {} specialized processing failed: {}", agent_id, e);
                            TaskResult {
                                success: true,
                                output: response.clone(),
                                error: Some(format!("Specialized processing error: {}", e)),
                                metadata: Default::default(),
                            }
                        }
                    }
                } else {
                    TaskResult {
                        success: true,
                        output: response.clone(),
                        error: None,
                        metadata: Default::default(),
                    }
                };

                // Execute autonomous actions if present in metadata
                self.apply_file_actions(agent_id, &response, &mut result).await;

                if let Err(e) = self.event_tx.send(AgentEvent::Completed {
                    agent_id: agent_id.clone(),
                    result: result.clone(),
                }).await {
                    warn!("Failed to send completed event for agent {}: {}", agent_id, e);
                }

                AgentResponse::TaskCompleted(result)
            }
            Err(e) => {
                let error_msg = e.to_string();
                error!("Agent {} LLM call failed: {}", agent_id, error_msg);
                self.set_agent_state(agent_id, AgentState::Failed).await;

                if let Err(e) = self.event_tx.send(AgentEvent::Error {
                    agent_id: agent_id.clone(),
                    error: error_msg.clone(),
                }).await {
                    warn!("Failed to send error event for agent {}: {}", agent_id, e);
                }

                AgentResponse::Error(error_msg)
            }
        }
    }

    /// Handle streaming chat message
    async fn handle_stream_chat(
        &mut self,
        agent_id: &Id,
        message: String,
        history: Vec<Message>,
        context: ExecutionContext,
    ) -> AgentResponse {
        self.set_agent_state(agent_id, AgentState::Running).await;

        let system_prompt = self.build_system_prompt(agent_id, &context.session_id).await;
        let mut messages = vec![Message::system(&system_prompt)];
        messages.extend(history);
        messages.push(Message::user(&message));

        // Send to LLM for streaming
        match self.llm_client.send_message_streaming(&messages).await {
            Ok(mut stream) => {
                let mut full_response = String::new();
                while let Some(item) = stream.next().await {
                    if let Ok(content) = item {
                        full_response.push_str(&content);
                        let _ = self.event_tx.send(AgentEvent::Message {
                            agent_id: agent_id.clone(),
                            content,
                        }).await;
                    }
                }
                
                self.set_agent_state(agent_id, AgentState::Idle).await;
                AgentResponse::ChatResponse(full_response)
            }
            Err(e) => {
                error!("Agent {} streaming LLM call failed: {}", agent_id, e);
                self.set_agent_state(agent_id, AgentState::Failed).await;
                AgentResponse::Error(e.to_string())
            }
        }
    }

    /// Handle chat message
    async fn handle_chat(
        &mut self,
        agent_id: &Id,
        message: String,
        history: Vec<Message>,
        context: ExecutionContext,
    ) -> AgentResponse {
        self.set_agent_state(agent_id, AgentState::Running).await;

        let system_prompt = self.build_system_prompt(agent_id, &context.session_id).await;
        let mut messages = vec![Message::system(&system_prompt)];
        messages.extend(history);
        messages.push(Message::user(&message));

        match self.llm_client.send_message(&messages).await {
            Ok(response) => {
                self.set_agent_state(agent_id, AgentState::Idle).await;
                AgentResponse::ChatResponse(response)
            }
            Err(e) => {
                error!("Agent {} chat LLM call failed: {}", agent_id, e);
                self.set_agent_state(agent_id, AgentState::Failed).await;
                AgentResponse::Error(e.to_string())
            }
        }
    }
}

/// Agent instance with its handle
#[derive(Debug)]
pub struct AgentInstance {
    pub handle: AgentHandle,
    pub agent: Arc<RwLock<Agent>>,
}

impl AgentInstance {
    /// Get the agent ID
    #[allow(dead_code)]
    pub fn id(&self) -> Id {
        self.handle.agent_id.clone()
    }

    /// Get the agent name
    pub fn name(&self) -> String {
        self.handle.agent_name.clone()
    }

    /// Get the current state
    pub async fn state(&self) -> AgentState {
        self.agent.read().await.state
    }
}

/// Builder for creating agent instances
pub struct AgentRuntimeBuilder<L: LlmProvider = LlmClient> {
    agent: Option<Agent>,
    llm_client: Option<Arc<L>>,
    shared_memory: Option<Arc<SharedMemory>>,
    event_tx: Option<mpsc::Sender<AgentEvent>>,
}

impl<L: LlmProvider> AgentRuntimeBuilder<L> {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            agent: None,
            llm_client: None,
            shared_memory: None,
            event_tx: None,
        }
    }

    /// Set the agent
    pub fn agent(mut self, agent: Agent) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Set the LLM client
    pub fn llm_client(mut self, client: Arc<L>) -> Self {
        self.llm_client = Some(client);
        self
    }

    /// Set the shared memory
    pub fn shared_memory(mut self, memory: Arc<SharedMemory>) -> Self {
        self.shared_memory = Some(memory);
        self
    }

    /// Set the event sender
    pub fn event_tx(mut self, tx: mpsc::Sender<AgentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Build and spawn the agent runtime
    pub async fn spawn(self) -> Result<AgentInstance> {
        let agent = self.agent.ok_or_else(|| anyhow!("Agent not set"))?;
        let llm_client = self.llm_client.ok_or_else(|| anyhow!("LLM client not set"))?;
        let shared_memory = self.shared_memory.ok_or_else(|| anyhow!("Shared memory not set"))?;
        let event_tx = self.event_tx.ok_or_else(|| anyhow!("Event sender not set"))?;

        let agent_arc = Arc::new(RwLock::new(agent));
        let runtime = AgentRuntime::new(agent_arc.clone(), llm_client, shared_memory, event_tx);
        let handle = runtime.spawn().await;

        Ok(AgentInstance {
            handle,
            agent: agent_arc,
        })
    }
}

impl<L: LlmProvider> Default for AgentRuntimeBuilder<L> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentRole, TaskType};

    // ==================== AgentCommand Tests ====================

    #[test]
    fn test_agent_command_process_task() {
        let task = Task::new("Test task", TaskType::CodeGeneration);
        let context = ExecutionContext::new("session-123")
            .with_messages(vec![Message::user("Hello")]);
        let command = AgentCommand::ProcessTask {
            task: Box::new(task.clone()),
            context: context.clone(),
        };

        match command {
            AgentCommand::ProcessTask { task: t, context: c } => {
                assert_eq!(t.description, task.description);
                assert_eq!(c.messages.len(), 1);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_command_chat() {
        let command = AgentCommand::Chat {
            message: "Hello".to_string(),
            history: vec![Message::user("Hi")],
            context: ExecutionContext::new("session-123"),
        };

        match command {
            AgentCommand::Chat { message, history, context } => {
                assert_eq!(message, "Hello");
                assert_eq!(history.len(), 1);
                assert_eq!(context.session_id, "session-123");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_command_stream_chat() {
        let command = AgentCommand::StreamChat {
            message: "Stream me".to_string(),
            history: vec![],
            context: ExecutionContext::new("session-123"),
        };

        match command {
            AgentCommand::StreamChat { message, history, context } => {
                assert_eq!(message, "Stream me");
                assert!(history.is_empty());
                assert_eq!(context.session_id, "session-123");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_command_get_state() {
        let command = AgentCommand::GetState;
        match command {
            AgentCommand::GetState => {}
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_command_shutdown() {
        let command = AgentCommand::Shutdown;
        match command {
            AgentCommand::Shutdown => {}
            _ => panic!("Wrong variant"),
        }
    }

    // ==================== AgentEvent Tests ====================

    #[test]
    fn test_agent_event_started() {
        let event = AgentEvent::Started {
            agent_id: "agent-1".to_string(),
        };

        match event {
            AgentEvent::Started { agent_id } => {
                assert_eq!(agent_id, "agent-1");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_event_completed() {
        let result = TaskResult {
            success: true,
            output: "Done".to_string(),
            error: None,
            metadata: Default::default(),
        };

        let event = AgentEvent::Completed {
            agent_id: "agent-1".to_string(),
            result: result.clone(),
        };

        match event {
            AgentEvent::Completed { agent_id, result: r } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(r.output, "Done");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_event_message() {
        let event = AgentEvent::Message {
            agent_id: "agent-1".to_string(),
            content: "Streaming...".to_string(),
        };

        match event {
            AgentEvent::Message { agent_id, content } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(content, "Streaming...");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_event_error() {
        let event = AgentEvent::Error {
            agent_id: "agent-1".to_string(),
            error: "Something went wrong".to_string(),
        };

        match event {
            AgentEvent::Error { agent_id, error } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(error, "Something went wrong");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_event_state_changed() {
        let event = AgentEvent::StateChanged {
            agent_id: "agent-1".to_string(),
            state: AgentState::Running,
        };

        match event {
            AgentEvent::StateChanged { agent_id, state } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(state, AgentState::Running);
            }
            _ => panic!("Wrong variant"),
        }
    }

    // ==================== AgentResponse Tests ====================

    #[test]
    fn test_agent_response_task_completed() {
        let result = TaskResult {
            success: true,
            output: "Done".to_string(),
            error: None,
            metadata: Default::default(),
        };

        let response = AgentResponse::TaskCompleted(result.clone());

        match response {
            AgentResponse::TaskCompleted(r) => {
                assert!(r.success);
                assert_eq!(r.output, "Done");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_response_chat_response() {
        let response = AgentResponse::ChatResponse("Hello".to_string());

        match response {
            AgentResponse::ChatResponse(content) => {
                assert_eq!(content, "Hello");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_response_state() {
        let response = AgentResponse::State(AgentState::Idle);

        match response {
            AgentResponse::State(state) => {
                assert_eq!(state, AgentState::Idle);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_response_error() {
        let response = AgentResponse::Error("Error occurred".to_string());

        match response {
            AgentResponse::Error(msg) => {
                assert_eq!(msg, "Error occurred");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_response_ack() {
        let response = AgentResponse::Ack;

        match response {
            AgentResponse::Ack => {}
            _ => panic!("Wrong variant"),
        }
    }

    // ==================== AgentRuntimeBuilder Tests ====================

    #[test]
    fn test_agent_runtime_builder_new() {
        let builder = AgentRuntimeBuilder::<LlmClient>::new();
        let _ = builder;
    }

    #[test]
    fn test_agent_runtime_builder_default() {
        let builder = AgentRuntimeBuilder::<LlmClient>::default();
        let _ = builder;
    }

    #[test]
    fn test_agent_runtime_builder_fluent_interface() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let shared_memory = Arc::new(SharedMemory::new());
        let agent = Agent::new("test-agent", AgentRole::Coder, "gpt-4o")
            .with_description("Test agent");

        let builder = AgentRuntimeBuilder::new()
            .agent(agent)
            .llm_client(llm_client)
            .shared_memory(shared_memory)
            .event_tx(event_tx);

        // Builder should be usable after each method call
        let _ = builder;
    }

    #[tokio::test]
    async fn test_agent_runtime_builder_spawn_missing_agent() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let shared_memory = Arc::new(SharedMemory::new());

        let builder = AgentRuntimeBuilder::new()
            .llm_client(llm_client)
            .shared_memory(shared_memory)
            .event_tx(event_tx);

        let result = builder.spawn().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Agent not set"));
    }

    #[tokio::test]
    async fn test_agent_runtime_builder_spawn_missing_llm_client() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let shared_memory = Arc::new(SharedMemory::new());
        let agent = Agent::new("test-agent", AgentRole::Coder, "gpt-4o");

        let builder = AgentRuntimeBuilder::<LlmClient>::new()
            .agent(agent)
            .shared_memory(shared_memory)
            .event_tx(event_tx);

        let result = builder.spawn().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("LLM client not set"));
    }

    #[tokio::test]
    async fn test_agent_runtime_builder_spawn_missing_shared_memory() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let agent = Agent::new("test-agent", AgentRole::Coder, "gpt-4o");

        let builder = AgentRuntimeBuilder::new()
            .agent(agent)
            .llm_client(llm_client)
            .event_tx(event_tx);

        let result = builder.spawn().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Shared memory not set"));
    }

    #[tokio::test]
    async fn test_agent_runtime_builder_spawn_missing_event_tx() {
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let shared_memory = Arc::new(SharedMemory::new());
        let agent = Agent::new("test-agent", AgentRole::Coder, "gpt-4o");

        let builder = AgentRuntimeBuilder::new()
            .agent(agent)
            .llm_client(llm_client)
            .shared_memory(shared_memory);

        let result = builder.spawn().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Event sender not set"));
    }

    // ==================== AgentInstance Tests ====================

    #[test]
    fn test_agent_instance_id() {
        // We can't easily test AgentInstance without spawning a full runtime
        // but we can verify the struct exists and has the expected fields
        let _ = std::mem::size_of::<AgentInstance>();
    }
}
