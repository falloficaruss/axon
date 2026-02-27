//! Orchestrator module
//!
//! This module handles multi-agent orchestration, task routing, and execution.

pub mod pool;

pub use pool::AgentPool;

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

use crate::{
    agent::{AgentEvent, AgentRegistry},
    llm::LlmClient,
    shared::SharedMemory,
    types::{Agent, AgentState, Task, TaskResult, RoutingDecision, RoutingAnalysis, Session, Id, Message, MessageRole, TaskType},
};

/// Confidence threshold for agent selection in routing
pub const AGENT_CONFIDENCE_THRESHOLD: f32 = 0.6;

/// Dynamic task router
pub struct Router;

impl Router {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a task and determine routing using LLM
    pub async fn analyze(
        &self,
        llm_client: Arc<LlmClient>,
        registry: &AgentRegistry,
        task: &Task,
        session: &Session,
    ) -> Result<RoutingAnalysis> {
        info!("Analyzing task for routing: {}", task.description);
        
        let agents = registry.list();
        let mut agent_descriptions = String::new();
        for agent in agents {
            agent_descriptions.push_str(&format!(
                "- {} (role: {}): {}\n", 
                agent.name, 
                agent.role.as_str(), 
                agent.description
            ));
        }

        // Get recent context from session
        let mut history = String::new();
        for msg in session.messages.iter().rev().take(5).rev() {
            let role = match msg.role {
                MessageRole::User => "User",
                MessageRole::Agent => "Agent",
                MessageRole::System => "System",
            };
            history.push_str(&format!("{}: {}\n", role, msg.content));
        }

        let prompt = format!(
            "You are an AI task router. Analyze the user request and suggest the most appropriate agents.\n\n\
            Available Agents:\n{}\n\
            Recent Conversation:\n{}\n\
            Current Task: {}\n\n\
            Respond ONLY with a JSON object in this format:\n\
            {{\n  \"task_type\": \"CodeGeneration|CodeEdit|CodeReview|TestGeneration|TestExecution|Exploration|Planning|Synthesis|General\",\n  \"suggested_agents\": [[\"agent_name\", confidence_score]],\n  \"can_parallelize\": bool,\n  \"estimated_complexity\": 1-10,\n  \"requires_subtasks\": bool\n}}",
            agent_descriptions, history, task.description
        );

        let messages = vec![Message::system(&prompt)];
        let response = llm_client.send_message(&messages).await?;
        
        // Clean up response in case LLM adds markdown blocks
        let clean_response = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();

        let analysis_json: serde_json::Value = serde_json::from_str(clean_response)
            .map_err(|e| anyhow!("Failed to parse routing analysis: {}. Response was: {}", e, clean_response))?;

        let task_type = match analysis_json["task_type"].as_str().unwrap_or("General") {
            "CodeGeneration" => TaskType::CodeGeneration,
            "CodeEdit" => TaskType::CodeEdit,
            "CodeReview" => TaskType::CodeReview,
            "TestGeneration" => TaskType::TestGeneration,
            "TestExecution" => TaskType::TestExecution,
            "Exploration" => TaskType::Exploration,
            "Planning" => TaskType::Planning,
            "Synthesis" => TaskType::Synthesis,
            _ => TaskType::General,
        };

        let mut suggested_agents = vec![];
        if let Some(agents_arr) = analysis_json["suggested_agents"].as_array() {
            for item in agents_arr {
                if let Some(agent_info) = item.as_array() {
                    if agent_info.len() >= 2 {
                        let name = agent_info[0].as_str().unwrap_or("");
                        let confidence = agent_info[1].as_f64().unwrap_or(0.0) as f32;
                        
                        if let Some(agent) = registry.get(name) {
                            suggested_agents.push((agent.id.clone(), confidence));
                        }
                    }
                }
            }
        }

        Ok(RoutingAnalysis {
            task_type,
            suggested_agents,
            can_parallelize: analysis_json["can_parallelize"].as_bool().unwrap_or(false),
            estimated_complexity: analysis_json["estimated_complexity"].as_u64().unwrap_or(5) as u32,
            requires_subtasks: analysis_json["requires_subtasks"].as_bool().unwrap_or(false),
        })
    }

    /// Make a routing decision
    pub async fn route(&self, task: Task, analysis: RoutingAnalysis) -> Result<RoutingDecision> {
        // Simple routing logic: pick agents with high confidence
        let selected_agents: Vec<Id> = analysis.suggested_agents
            .iter()
            .filter(|(_, conf)| *conf > AGENT_CONFIDENCE_THRESHOLD)
            .map(|(id, _)| id.clone())
            .collect();

        let mut decision = RoutingDecision::new(task, vec![], 0.0);
        decision.selected_agents = selected_agents;

        if !analysis.suggested_agents.is_empty() {
            decision.confidence = analysis.suggested_agents[0].1;
        }

        Ok(decision)
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
    pub async fn plan(&self, _task: &Task) -> Result<Vec<Task>> {
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
    /// Agent pool for managing running agents
    pool: AgentPool,
    /// Shared memory for agents
    shared_memory: Arc<SharedMemory>,
}

impl Executor {
    /// Create a new executor
    pub fn new(
        llm_client: Arc<LlmClient>,
        shared_memory: Arc<SharedMemory>,
        event_tx: mpsc::Sender<AgentEvent>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            pool: AgentPool::new(max_concurrent, llm_client, shared_memory.clone(), event_tx),
            shared_memory,
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

        let agent_id = agent.id.clone();
        
        // Get agent from pool or spawn if not running
        let handle = if let Some(handle) = self.pool.get_agent(&agent_id).await {
            handle
        } else {
            self.pool.spawn_agent(agent).await?
        };

        // Execute the task
        let result = handle.process_task(task, context.messages).await;

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

        let agent_id = agent.id.clone();

        // Get agent from pool or spawn if not running
        let handle = if let Some(handle) = self.pool.get_agent(&agent_id).await {
            handle
        } else {
            self.pool.spawn_agent(agent).await?
        };

        // Execute the chat
        let result = handle.chat(message, history).await;

        result.map_err(|e| anyhow!("Chat execution failed: {}", e))
    }

    /// Execute a simple streaming chat request with an agent
    pub async fn execute_chat_streaming(
        &self,
        agent: Agent,
        message: String,
        history: Vec<Message>,
    ) -> Result<String> {
        debug!("Executing streaming chat with agent {}", agent.name);

        let agent_id = agent.id.clone();

        // Get agent from pool or spawn if not running
        let handle = if let Some(handle) = self.pool.get_agent(&agent_id).await {
            handle
        } else {
            self.pool.spawn_agent(agent).await?
        };

        // Execute the streaming chat
        let result = handle.chat_streaming(message, history).await;

        result.map_err(|e| anyhow!("Streaming chat execution failed: {}", e))
    }

    /// Get count of currently active agents
    pub async fn active_count(&self) -> usize {
        self.pool.active_count().await
    }

    /// Check if at capacity
    pub async fn is_at_capacity(&self) -> bool {
        self.pool.is_at_capacity().await
    }

    /// Get agent state
    pub async fn get_agent_state(&self, agent_id: &Id) -> Option<AgentState> {
        self.pool.get_agent_state(agent_id).await
    }

    /// Shutdown all active agents
    pub async fn shutdown_all(&self) -> Result<()> {
        self.pool.shutdown_all().await
    }
}

/// Orchestrator that coordinates routing, planning, and execution
pub struct Orchestrator {
    /// LLM client for routing and planning
    llm_client: Arc<LlmClient>,
    /// Agent registry
    registry: Arc<RwLock<AgentRegistry>>,
    /// Shared memory for agents
    shared_memory: Arc<SharedMemory>,
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
        registry: Arc<RwLock<AgentRegistry>>,
        shared_memory: Arc<SharedMemory>,
        event_tx: mpsc::Sender<AgentEvent>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            llm_client: llm_client.clone(),
            registry,
            shared_memory: shared_memory.clone(),
            router: Router::new(),
            planner: Planner::new(),
            executor: Executor::new(llm_client, shared_memory, event_tx, max_concurrent),
        }
    }

    /// Execute a task with automatic routing
    pub async fn execute_auto(&self, task: Task, session: &Session) -> Result<TaskResult> {
        // Analyze the task
        let analysis = {
            let registry = self.registry.read().await;
            self.router.analyze(self.llm_client.clone(), &registry, &task, session).await?
        };
        
        // Make routing decision
        let decision = self.router.route(task.clone(), analysis).await?;
        
        // Execute with the first selected agent
        if let Some(agent_id) = decision.selected_agents.first() {
            let agent_opt = {
                let registry = self.registry.read().await;
                registry.get_by_id(agent_id).cloned()
            };

            if let Some(agent) = agent_opt {
                let context = ExecutionContext::new(session.id.clone())
                    .with_messages(session.messages.clone());
                    
                self.executor.execute_task(agent, task, context).await
            } else {
                Err(anyhow!("Selected agent {} not found in registry", agent_id))
            }
        } else {
            Ok(TaskResult {
                success: false,
                output: String::new(),
                error: Some("No suitable agent found for this task".to_string()),
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

    /// Execute a streaming chat with a specific agent
    pub async fn execute_chat_streaming(
        &self,
        agent: Agent,
        message: String,
        history: Vec<Message>,
    ) -> Result<String> {
        self.executor.execute_chat_streaming(agent, message, history).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TaskType, TaskStatus, AgentRole};

    // ==================== Router Tests ====================

    #[test]
    fn test_router_new() {
        let router = Router::new();
        // Router has no state, just verify it can be created
        let _ = router;
    }

    #[test]
    fn test_router_default() {
        let router = Router::default();
        let _ = router;
    }

    #[tokio::test]
    async fn test_route_with_high_confidence_agents() {
        let router = Router::new();
        let task = Task::new("Test task", TaskType::CodeGeneration);

        let analysis = RoutingAnalysis {
            task_type: TaskType::CodeGeneration,
            suggested_agents: vec![
                ("agent-1".to_string(), 0.9),
                ("agent-2".to_string(), 0.8),
                ("agent-3".to_string(), 0.5), // Below threshold
            ],
            can_parallelize: true,
            estimated_complexity: 5,
            requires_subtasks: false,
        };

        let decision = router.route(task, analysis).await.unwrap();

        // Should select agents with confidence > AGENT_CONFIDENCE_THRESHOLD (0.6)
        assert_eq!(decision.selected_agents.len(), 2);
        assert!(decision.selected_agents.contains(&"agent-1".to_string()));
        assert!(decision.selected_agents.contains(&"agent-2".to_string()));
        assert!(!decision.selected_agents.contains(&"agent-3".to_string()));

        // Confidence should be from first agent
        assert_eq!(decision.confidence, 0.9);
    }

    #[tokio::test]
    async fn test_route_with_no_agents() {
        let router = Router::new();
        let task = Task::new("Test task", TaskType::CodeGeneration);
        
        let analysis = RoutingAnalysis {
            task_type: TaskType::CodeGeneration,
            suggested_agents: vec![],
            can_parallelize: false,
            estimated_complexity: 1,
            requires_subtasks: false,
        };

        let decision = router.route(task, analysis).await.unwrap();
        
        assert_eq!(decision.selected_agents.len(), 0);
        assert_eq!(decision.confidence, 0.0);
    }

    #[tokio::test]
    async fn test_route_with_all_low_confidence() {
        let router = Router::new();
        let task = Task::new("Test task", TaskType::CodeGeneration);

        let analysis = RoutingAnalysis {
            task_type: TaskType::CodeGeneration,
            suggested_agents: vec![
                ("agent-1".to_string(), 0.3),
                ("agent-2".to_string(), 0.4),
            ],
            can_parallelize: false,
            estimated_complexity: 2,
            requires_subtasks: false,
        };

        let decision = router.route(task, analysis).await.unwrap();
        

        // No agents should be selected (all below AGENT_CONFIDENCE_THRESHOLD)
        assert_eq!(decision.selected_agents.len(), 0);
    }

    // ==================== RoutingDecision Tests ====================

    #[test]
    fn test_routing_decision_new() {
        let task = Task::new("Test", TaskType::CodeGeneration);
        let agents = vec!["agent-1", "agent-2"];
        let decision = RoutingDecision::new(task, agents, 0.85);

        assert_eq!(decision.selected_agents.len(), 2);
        assert!(decision.selected_agents.contains(&"agent-1".to_string()));
        assert!(decision.selected_agents.contains(&"agent-2".to_string()));
        assert_eq!(decision.confidence, 0.85);
        assert_eq!(decision.reasoning, "");
        // requires_confirmation should be false when confidence >= 0.8
        assert!(!decision.requires_confirmation);
    }

    #[test]
    fn test_routing_decision_low_confidence() {
        let task = Task::new("Test", TaskType::CodeGeneration);
        let agents = vec!["agent-1"];
        let decision = RoutingDecision::new(task, agents, 0.5);

        // requires_confirmation should be true when confidence < 0.8
        assert!(decision.requires_confirmation);
    }

    #[test]
    fn test_routing_decision_with_reasoning() {
        let task = Task::new("Test", TaskType::CodeGeneration);
        let agents = vec!["agent-1"];
        let decision = RoutingDecision::new(task, agents, 0.9)
            .with_reasoning("This task requires code generation expertise");

        assert_eq!(decision.reasoning, "This task requires code generation expertise");
    }

    // ==================== Planner Tests ====================

    #[test]
    fn test_planner_new() {
        let planner = Planner::new();
        let _ = planner;
    }

    #[tokio::test]
    async fn test_planner_plan_returns_empty() {
        let planner = Planner::new();
        let task = Task::new("Test task", TaskType::CodeGeneration);
        
        let result = planner.plan(&task).await.unwrap();
        
        // Currently returns empty vec (not yet implemented)
        assert!(result.is_empty());
    }

    // ==================== ExecutionContext Tests ====================

    #[test]
    fn test_execution_context_new() {
        let ctx = ExecutionContext::new("session-123".to_string());
        
        assert_eq!(ctx.session_id, "session-123");
        assert!(ctx.messages.is_empty());
        assert!(ctx.context.is_empty());
    }

    #[test]
    fn test_execution_context_with_messages() {
        let messages = vec![Message::user("Hello")];
        let ctx = ExecutionContext::new("session-123".to_string())
            .with_messages(messages.clone());
        
        assert_eq!(ctx.messages.len(), 1);
        assert_eq!(ctx.messages[0].content, "Hello");
    }

    // ==================== Executor Tests ====================

    #[tokio::test]
    async fn test_executor_new() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let shared_memory = Arc::new(SharedMemory::new());
        
        let executor = Executor::new(llm_client, shared_memory, event_tx, 5);
        
        // Verify executor was created
        let count = executor.active_count().await;
        assert_eq!(count, 0); // No agents spawned yet
    }

    #[tokio::test]
    async fn test_executor_is_at_capacity() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let shared_memory = Arc::new(SharedMemory::new());
        
        let executor = Executor::new(llm_client, shared_memory, event_tx, 2);
        
        assert!(!executor.is_at_capacity().await);
    }

    // ==================== Orchestrator Tests ====================

    #[tokio::test]
    async fn test_orchestrator_new() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let registry = Arc::new(RwLock::new(AgentRegistry::new()));
        let shared_memory = Arc::new(SharedMemory::new());
        
        let orchestrator = Orchestrator::new(llm_client, registry, shared_memory, event_tx, 5);
        
        assert!(orchestrator.executor().active_count().await == 0);
    }

    #[tokio::test]
    async fn test_orchestrator_execute_auto_no_agents() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let registry = Arc::new(RwLock::new(AgentRegistry::new()));
        let shared_memory = Arc::new(SharedMemory::new());

        let orchestrator = Orchestrator::new(llm_client, registry, shared_memory, event_tx, 5);
        let task = Task::new("Test task", TaskType::CodeGeneration);
        let session = Session::new("Test Session");

        // Should fail gracefully when no agents available
        // The LLM-based routing will fail without agents registered
        let result = orchestrator.execute_auto(task, &session).await;
        
        // Expect an error since no agents are registered for routing
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_orchestrator_execute_chat_streaming() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let mut registry = AgentRegistry::new();

        // Register a test agent
        let agent = Agent::new("test-agent", AgentRole::Coder, "gpt-4o")
            .with_description("Test agent");
        registry.register(agent);

        let registry = Arc::new(RwLock::new(registry));
        let shared_memory = Arc::new(SharedMemory::new());
        let orchestrator = Orchestrator::new(llm_client, registry, shared_memory, event_tx, 5);

        let agent = Agent::new("test-agent", AgentRole::Coder, "gpt-4o");
        let history = vec![Message::user("Hello")];

        // This will call the LLM API - the result depends on API availability
        // The test verifies the function completes without panicking
        let _ = orchestrator.execute_chat_streaming(agent, "Test".to_string(), history).await;
    }
}
