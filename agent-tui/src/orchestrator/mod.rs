//! Orchestrator module
//!
//! This module handles multi-agent orchestration, task routing, and execution.

pub mod pool;

pub use pool::{AgentPool, AgentPoolBuilder};

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{
    agent::{AgentHandle, AgentEvent, AgentRuntimeBuilder, AgentInstance},
    llm::LlmClient,
    types::{Agent, Task, TaskResult, TaskStatus, RoutingDecision, RoutingAnalysis, Session, Id, Message, AgentState},
};

/// Dynamic task router
pub struct Router;

impl Router {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a task and determine routing
    pub async fn analyze(&self, task: &Task, session: &Session) -> Result<RoutingAnalysis> {
        // TODO: Implement LLM-based routing analysis
        Ok(RoutingAnalysis {
            task_type: task.task_type,
            suggested_agents: vec![],
            can_parallelize: false,
            estimated_complexity: 5,
            requires_subtasks: false,
        })
    }

    /// Make a routing decision
    pub async fn route(&self, task: Task, analysis: RoutingAnalysis) -> Result<RoutingDecision> {
        // TODO: Implement routing logic
        Ok(RoutingDecision::new(task, vec![], 0.5))
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

/// Task planner for decomposition
pub struct Planner;

impl Planner {
    pub fn new() -> Self {
        Self
    }

    /// Decompose a task into subtasks
    pub async fn plan(&self, task: &Task) -> Result<Vec<Task>> {
        // TODO: Implement task decomposition
        Ok(vec![])
    }
}

/// Execution context for a task
pub struct ExecutionContext {
    /// Session ID
    pub session_id: Id,
    /// Message history
    pub messages: Vec<Message>,
    /// Additional context data
    pub context: std::collections::HashMap<String, serde_json::Value>,
}

impl ExecutionContext {
    pub fn new(session_id: Id) -> Self {
        Self {
            session_id,
            messages: vec![],
            context: std::collections::HashMap::new(),
        }
    }

    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }
}

/// Task executor that manages agent execution
pub struct Executor {
    /// LLM client for agents
    llm_client: Arc<LlmClient>,
    /// Event sender
    event_tx: mpsc::Sender<AgentEvent>,
    /// Active agent instances
    active_agents: Arc<tokio::sync::RwLock<std::collections::HashMap<Id, AgentInstance>>>,
    /// Maximum concurrent agents
    max_concurrent: usize,
}

impl Executor {
    /// Create a new executor
    pub fn new(
        llm_client: Arc<LlmClient>,
        event_tx: mpsc::Sender<AgentEvent>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            llm_client,
            event_tx,
            active_agents: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            max_concurrent,
        }
    }

    /// Execute a task with a specific agent
    pub async fn execute_task(
        &self,
        agent: Agent,
        task: Task,
        context: ExecutionContext,
    ) -> Result<TaskResult> {
        info!("Executing task {} with agent {}", task.id, agent.name);

        // Build and spawn the agent runtime
        let instance = AgentRuntimeBuilder::new()
            .agent(agent.clone())
            .llm_client(self.llm_client.clone())
            .event_tx(self.event_tx.clone())
            .spawn()?;

        let handle = instance.handle.clone();
        let agent_id = agent.id.clone();

        // Store the active agent
        self.active_agents.write().await.insert(agent_id.clone(), instance);

        // Execute the task
        let result = handle.process_task(task, context.messages).await;

        // Clean up
        self.active_agents.write().await.remove(&agent_id);

        result.map_err(|e| anyhow!("Task execution failed: {}", e))
    }

    /// Execute a simple chat request with an agent
    pub async fn execute_chat(
        &self,
        agent: Agent,
        message: String,
        history: Vec<Message>,
    ) -> Result<String> {
        debug!("Executing chat with agent {}", agent.name);

        // Build and spawn the agent runtime
        let instance = AgentRuntimeBuilder::new()
            .agent(agent)
            .llm_client(self.llm_client.clone())
            .event_tx(self.event_tx.clone())
            .spawn()?;

        let handle = instance.handle.clone();
        let agent_id = instance.id();

        // Store the active agent
        self.active_agents.write().await.insert(agent_id.clone(), instance);

        // Execute the chat
        let result = handle.chat(message, history).await;

        // Clean up
        self.active_agents.write().await.remove(&agent_id);

        result.map_err(|e| anyhow!("Chat execution failed: {}", e))
    }

    /// Get count of currently active agents
    pub async fn active_count(&self) -> usize {
        self.active_agents.read().await.len()
    }

    /// Check if at capacity
    pub async fn is_at_capacity(&self) -> bool {
        self.active_count().await >= self.max_concurrent
    }

    /// Get agent state
    pub async fn get_agent_state(&self, agent_id: &Id) -> Option<AgentState> {
        if let Some(instance) = self.active_agents.read().await.get(agent_id) {
            Some(instance.state().await)
        } else {
            None
        }
    }

    /// Shutdown all active agents
    pub async fn shutdown_all(&self) -> Result<()> {
        let agents: Vec<Id> = self.active_agents.read().await.keys().cloned().collect();
        
        for agent_id in agents {
            if let Some(instance) = self.active_agents.write().await.remove(&agent_id) {
                let _ = instance.handle.shutdown().await;
            }
        }

        info!("All agents shut down");
        Ok(())
    }
}

/// Orchestrator that coordinates routing, planning, and execution
pub struct Orchestrator {
    /// Task router
    router: Router,
    /// Task planner
    planner: Planner,
    /// Task executor
    executor: Executor,
}

impl Orchestrator {
    /// Create a new orchestrator
    pub fn new(
        llm_client: Arc<LlmClient>,
        event_tx: mpsc::Sender<AgentEvent>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            router: Router::new(),
            planner: Planner::new(),
            executor: Executor::new(llm_client, event_tx, max_concurrent),
        }
    }

    /// Execute a task with automatic routing
    pub async fn execute_auto(&self, task: Task, session: &Session) -> Result<TaskResult> {
        // Analyze the task
        let analysis = self.router.analyze(&task, session).await?;
        
        // Make routing decision
        let decision = self.router.route(task.clone(), analysis).await?;
        
        // For now, just use the first selected agent or default to a generic response
        // TODO: Actually spawn the selected agents
        if let Some(agent_id) = decision.selected_agents.first() {
            // We would look up the agent and execute with it
            // For now, return a placeholder result
            Ok(TaskResult {
                success: true,
                output: format!("Task routed to agent: {}", agent_id),
                error: None,
                metadata: Default::default(),
            })
        } else {
            Ok(TaskResult {
                success: false,
                output: String::new(),
                error: Some("No agent selected for task".to_string()),
                metadata: Default::default(),
            })
        }
    }

    /// Execute a chat with a specific agent
    pub async fn execute_chat(
        &self,
        agent: Agent,
        message: String,
        history: Vec<Message>,
    ) -> Result<String> {
        self.executor.execute_chat(agent, message, history).await
    }

    /// Execute a task with a specific agent
    pub async fn execute_with_agent(
        &self,
        agent: Agent,
        task: Task,
        context: ExecutionContext,
    ) -> Result<TaskResult> {
        self.executor.execute_task(agent, task, context).await
    }

    /// Get executor reference
    pub fn executor(&self) -> &Executor {
        &self.executor
    }
}
