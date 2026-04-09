//! Orchestrator module
//!
//! This module handles multi-agent orchestration, task routing, and execution.

pub mod pool;

pub use pool::AgentPool;

use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::{
    agent::{AgentEvent, AgentRegistry},
    llm::LlmProvider,
    shared::SharedMemory,
    types::{Agent, AgentState, Task, TaskResult, RoutingDecision, RoutingAnalysis, Session, Id, Message, MessageRole, TaskType, Plan, Subtask, ExecutionContext},
};

/// Confidence threshold for agent selection in routing
pub const AGENT_CONFIDENCE_THRESHOLD: f32 = 0.6;

/// Dynamic task router
#[derive(Clone)]
pub struct Router;

impl Router {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a task and determine routing using LLM
    pub async fn analyze<L: LlmProvider + ?Sized>(
        &self,
        llm_client: Arc<L>,
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
#[derive(Clone)]
pub struct Planner {
    llm_client: Option<Arc<dyn LlmProvider>>,
}

impl Planner {
    pub fn new(llm_client: Option<Arc<dyn LlmProvider>>) -> Self {
        Self { llm_client }
    }

    /// Decompose a task into subtasks using LLM
    pub async fn plan(&self, task: &Task, session: &Session, registry: &AgentRegistry) -> Result<Plan> {
        info!("Planning task decomposition: {}", task.description);

        // If no LLM client, return a simple plan with the original task
        let llm_client = match &self.llm_client {
            Some(client) => client,
            None => {
                warn!("No LLM client available for planning, returning single-task plan");
                let mut plan = Plan::new(task.clone());
                let subtask = Subtask::new(&task.description, task.task_type);
                plan.subtasks = vec![subtask];
                return Ok(plan);
            }
        };

        // Get agent descriptions for the LLM
        let agents = registry.list();
        let mut agent_descriptions = String::new();
        for agent in agents {
            let caps: Vec<String> = agent.capabilities.iter().map(|c| {
                match c {
                    crate::types::Capability::Code => "code".to_string(),
                    crate::types::Capability::Refactor => "refactor".to_string(),
                    crate::types::Capability::Debug => "debug".to_string(),
                    crate::types::Capability::Optimize => "optimize".to_string(),
                    crate::types::Capability::Review => "review".to_string(),
                    crate::types::Capability::Test => "test".to_string(),
                    crate::types::Capability::Explore => "explore".to_string(),
                    crate::types::Capability::Plan => "plan".to_string(),
                    crate::types::Capability::Document => "document".to_string(),
                }
            }).collect();
            agent_descriptions.push_str(&format!(
                "- {} (role: {}): {} | capabilities: {}\n",
                agent.name,
                agent.role.as_str(),
                agent.description,
                caps.join(", ")
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
            "You are an expert task planner. Decompose the following task into logical subtasks.\n\n\
            Available Agents:\n{}\n\
            Recent Conversation:\n{}\n\
            Current Task: {}\n\n\
            For complex tasks, break them down into 2-6 subtasks.\n\
            For each subtask, suggest the most appropriate agent.\n\
            Identify which subtasks can run in parallel.\n\
            Specify dependencies between subtasks.\n\n\
            Respond ONLY with a JSON object in this format:\n\
            {{\n  \
            \"subtasks\": [\n    \
              {{\n      \
              \"description\": \"subtask description\",\n      \
              \"task_type\": \"CodeGeneration|CodeEdit|CodeReview|TestGeneration|TestExecution|Exploration|Planning|Synthesis|General\",\n      \
              \"suggested_agent\": \"agent_name\",\n      \
              \"dependencies\": [\"index of prior subtasks this depends on, empty if none\"]\n    \
              }}\n  \
            ],\n  \
            \"parallel_groups\": [[indices of subtasks that can run together]]\n\
            }}\n\n\
            Example:\n\
            {{\n  \
            \"subtasks\": [\n    \
              {{\"description\": \"Explore the codebase structure\", \"task_type\": \"Exploration\", \"suggested_agent\": \"explorer\", \"dependencies\": []}},\n    \
              {{\"description\": \"Write the main function\", \"task_type\": \"CodeGeneration\", \"suggested_agent\": \"coder\", \"dependencies\": [0]}},\n    \
              {{\"description\": \"Write unit tests\", \"task_type\": \"TestGeneration\", \"suggested_agent\": \"tester\", \"dependencies\": [1]}}\n  \
            ],\n  \
            \"parallel_groups\": [[0]]\n\
            }}",
            agent_descriptions, history, task.description
        );

        let messages = vec![Message::system(&prompt)];
        let response = llm_client.send_message(&messages).await?;

        // Clean up response in case LLM adds markdown blocks
        let clean_response = response
            .trim()
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim();

        let analysis_json: serde_json::Value = serde_json::from_str(clean_response)
            .map_err(|e| anyhow!("Failed to parse plan: {}. Response was: {}", e, clean_response))?;

        let mut subtasks = Vec::new();
        let mut parallel_groups = Vec::new();

        // Parse subtasks
        if let Some(subtasks_arr) = analysis_json["subtasks"].as_array() {
            for (idx, item) in subtasks_arr.iter().enumerate() {
                let default_desc = format!("Subtask {}", idx);
                let description = item["description"].as_str().unwrap_or(&default_desc);
                let task_type_str = item["task_type"].as_str().unwrap_or("General");
                let suggested_agent = item["suggested_agent"].as_str();
                let deps_arr = item["dependencies"].as_array();

                let task_type = match task_type_str {
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

                let mut subtask = Subtask::new(description, task_type);

                // Map agent name to ID and validate existence
                if let Some(agent_name) = suggested_agent {
                    if let Some(agent) = registry.get(agent_name) {
                        subtask.suggested_agent = Some(agent.id.clone());
                    } else {
                        warn!("LLM suggested non-existent agent: {}. Will use routing instead.", agent_name);
                    }
                }

                // Parse dependencies (indices to actual IDs)
                if let Some(deps_arr) = deps_arr {
                    let mut deps = Vec::new();
                    for dep_idx in deps_arr {
                        if let Some(idx) = dep_idx.as_u64() {
                            // We'll resolve these to IDs after all subtasks are created
                            deps.push(format!("idx:{}", idx));
                        }
                    }
                    subtask.dependencies = deps;
                }

                subtasks.push(subtask);
            }
        }

        // Resolve dependency indices to actual subtask IDs
        // Collect indices and IDs first to avoid borrow checker issues
        let subtask_ids: Vec<Id> = subtasks.iter().map(|s| s.id.clone()).collect();
        for subtask in &mut subtasks {
            let mut resolved_deps = Vec::new();
            for dep in &subtask.dependencies {
                if let Some(idx_str) = dep.strip_prefix("idx:") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if idx < subtask_ids.len() {
                            resolved_deps.push(subtask_ids[idx].clone());
                        }
                    }
                }
            }
            subtask.dependencies = resolved_deps;
        }

        // Parse parallel groups
        if let Some(groups_arr) = analysis_json["parallel_groups"].as_array() {
            for group in groups_arr {
                if let Some(group_arr) = group.as_array() {
                    let mut parallel_group = Vec::new();
                    for idx in group_arr {
                        if let Some(idx) = idx.as_u64() {
                            if (idx as usize) < subtasks.len() {
                                parallel_group.push(subtasks[idx as usize].id.clone());
                            }
                        }
                    }
                    if !parallel_group.is_empty() {
                        parallel_groups.push(parallel_group);
                    }
                }
            }
        }

        // If no subtasks were generated, create a single subtask from the original
        if subtasks.is_empty() {
            info!("LLM did not generate subtasks, using original task as single subtask");
            let subtask = Subtask::new(&task.description, task.task_type);
            subtasks.push(subtask);
        }

        let plan = Plan::new(task.clone())
            .with_subtasks(subtasks)
            .with_parallel_groups(parallel_groups);

        info!("Generated plan with {} subtasks", plan.subtasks.len());
        Ok(plan)
    }
}

/// Task executor that manages agent execution
#[derive(Clone)]
pub struct Executor {
    /// Agent pool for managing running agents
    pool: AgentPool,
    /// Shared memory for agents
    #[allow(dead_code)]
    shared_memory: Arc<SharedMemory>,
    /// Maximum concurrent agents (for creating new executors if needed)
    max_concurrent: usize,
}

impl Executor {
    /// Create a new executor
    pub fn new(
        llm_client: Arc<dyn LlmProvider>,
        shared_memory: Arc<SharedMemory>,
        event_tx: mpsc::Sender<AgentEvent>,
        max_concurrent: usize,
        workspace_root: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            pool: AgentPool::new(max_concurrent, llm_client, shared_memory.clone(), event_tx, workspace_root),
            shared_memory,
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

        let agent_id = agent.id.clone();
        
        // Get agent from pool or spawn if not running
        let handle = if let Some(handle) = self.pool.get_agent(&agent_id).await {
            handle
        } else {
            self.pool.spawn_agent(agent).await?
        };

        // Execute the task
        let result = handle.process_task(task, context).await;

        result.map_err(|e| anyhow!("Task execution failed: {}", e))
    }

    /// Execute a simple chat request with an agent
    #[allow(dead_code)]
    pub async fn execute_chat(
        &self,
        agent: Agent,
        message: String,
        history: Vec<Message>,
        session_id: &str,
    ) -> Result<String> {
        debug!("Executing chat with agent {}", agent.name);

        let agent_id = agent.id.clone();

        // Get agent from pool or spawn if not running
        let handle = if let Some(handle) = self.pool.get_agent(&agent_id).await {
            handle
        } else {
            self.pool.spawn_agent(agent).await?
        };

        let context = ExecutionContext::new(session_id).with_messages(history.clone());

        // Execute the chat
        let result = handle.chat(message, history, context).await;

        result.map_err(|e| anyhow!("Chat execution failed: {}", e))
    }

    /// Execute a simple streaming chat request with an agent
    pub async fn execute_chat_streaming(
        &self,
        agent: Agent,
        message: String,
        history: Vec<Message>,
        session_id: &str,
    ) -> Result<String> {
        debug!("Executing streaming chat with agent {}", agent.name);

        let agent_id = agent.id.clone();

        // Get agent from pool or spawn if not running
        let handle = if let Some(handle) = self.pool.get_agent(&agent_id).await {
            handle
        } else {
            self.pool.spawn_agent(agent).await?
        };

        let context = ExecutionContext::new(session_id).with_messages(history.clone());

        // Execute the streaming chat
        let result = handle.chat_streaming(message, history, context).await;

        result.map_err(|e| anyhow!("Streaming chat execution failed: {}", e))
    }

    /// Get count of currently active agents
    #[allow(dead_code)]
    pub async fn active_count(&self) -> usize {
        self.pool.active_count().await
    }

    /// Check if at capacity
    #[allow(dead_code)]
    pub async fn is_at_capacity(&self) -> bool {
        self.pool.is_at_capacity().await
    }

    /// Get agent state
    #[allow(dead_code)]
    pub async fn get_agent_state(&self, agent_id: &Id) -> Option<AgentState> {
        self.pool.get_agent_state(agent_id).await
    }

    /// Shutdown all active agents
    pub async fn shutdown_all(&self) -> Result<()> {
        self.pool.shutdown_all().await
    }
}

/// Orchestrator that coordinates routing, planning, and execution
#[derive(Clone)]
pub struct Orchestrator {
    /// LLM client for routing and planning
    llm_client: Arc<dyn LlmProvider>,
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
    /// Workspace root for file operations
    workspace_root: Option<std::path::PathBuf>,
}

impl Orchestrator {
    /// Create a new orchestrator
    pub fn new(
        llm_client: Arc<dyn LlmProvider>,
        registry: Arc<RwLock<AgentRegistry>>,
        shared_memory: Arc<SharedMemory>,
        event_tx: mpsc::Sender<AgentEvent>,
        max_concurrent: usize,
        workspace_root: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            llm_client: llm_client.clone(),
            registry,
            shared_memory: shared_memory.clone(),
            router: Router::new(),
            planner: Planner::new(Some(llm_client.clone())),
            executor: Executor::new(llm_client, shared_memory, event_tx, max_concurrent, workspace_root.clone()),
            workspace_root,
        }
    }

    /// Execute a task with automatic routing and planning
    pub async fn execute_auto(&self, task: Task, session: &Session) -> Result<TaskResult> {
        // Analyze the task for routing
        let analysis = {
            let registry = self.registry.read().await;
            self.router.analyze(self.llm_client.clone(), &registry, &task, session).await?
        };

        // Check if task decomposition is needed
        let plan = if analysis.requires_subtasks || analysis.estimated_complexity > 5 {
            info!("Task requires decomposition, generating plan...");
            let registry = self.registry.read().await;
            self.planner.plan(&task, session, &registry).await?
        } else {
            // Simple task - create a single-step plan
            info!("Task is simple, executing directly...");
            let mut plan = Plan::new(task.clone());
            let subtask = Subtask::new(&task.description, analysis.task_type);
            plan.subtasks = vec![subtask];
            plan
        };

        info!("Executing plan with {} subtasks", plan.subtasks.len());

        // Execute subtasks according to the plan
        self.execute_plan(plan, session).await
    }

    /// Execute a plan (sequence of subtasks)
    async fn execute_plan(&self, plan: Plan, session: &Session) -> Result<TaskResult> {
        let mut results: HashMap<Id, TaskResult> = HashMap::new();
        let mut completed: HashSet<Id> = HashSet::new();
        let mut in_progress: HashSet<Id> = HashSet::new();
        let mut join_set = JoinSet::new();

        let total_subtasks = plan.subtasks.len();
        info!("Starting execution of plan with {} subtasks", total_subtasks);

        while completed.len() < total_subtasks {
            // Find subtasks that are ready to run (not started, all dependencies completed)
            let ready_subtasks: Vec<Subtask> = plan.subtasks.iter()
                .filter(|s| !completed.contains(&s.id) && !in_progress.contains(&s.id))
                .filter(|s| s.dependencies.iter().all(|dep| completed.contains(dep)))
                .cloned()
                .collect();

            for subtask in ready_subtasks {
                let subtask_id = subtask.id.clone();
                in_progress.insert(subtask_id.clone());

                // Use shared executor instead of creating new clones for each subtask
                let executor = self.executor.clone();
                let registry = self.registry.clone();
                let llm_client = self.llm_client.clone();
                let session_clone = session.clone();
                let mut dep_results = HashMap::new();
                
                for dep_id in &subtask.dependencies {
                    if let Some(res) = results.get(dep_id) {
                        dep_results.insert(dep_id.clone(), res.clone());
                    }
                }

                join_set.spawn(async move {
                    let res = Self::execute_subtask_with_executor(
                        executor,
                        registry,
                        llm_client,
                        subtask,
                        &session_clone,
                        dep_results,
                    ).await;
                    (subtask_id, res)
                });
            }

            // If nothing is running and nothing is ready, but we're not done, we have a cycle or dead end
            if in_progress.is_empty() && completed.len() < total_subtasks {
                return Err(anyhow!("Deadlock detected in plan execution: circular dependency or missing tasks"));
            }

            // Wait for at least one task to complete
            if let Some(res) = join_set.join_next().await {
                match res {
                    Ok((id, Ok(result))) => {
                        results.insert(id.clone(), result);
                        completed.insert(id.clone());
                        in_progress.remove(&id);
                    }
                    Ok((id, Err(e))) => {
                        return Err(anyhow!("Subtask {} failed with error: {}", id, e));
                    }
                    Err(e) => {
                        return Err(anyhow!("Task join error: {}", e));
                    }
                }
            }
        }

        // Synthesize final result
        let mut final_output = String::new();
        let mut all_success = true;
        let mut errors = Vec::new();

        // Sort subtasks by creation for consistent output
        let mut sorted_subtasks = plan.subtasks.clone();
        sorted_subtasks.sort_by_key(|s| s.created_at);

        for subtask in sorted_subtasks {
            if let Some(result) = results.get(&subtask.id) {
                if result.success {
                    if !final_output.is_empty() {
                        final_output.push_str("\n\n---\n\n");
                    }
                    final_output.push_str(&format!("**{}**: {}\n", subtask.description, result.output));
                } else {
                    all_success = false;
                    if let Some(err) = &result.error {
                        errors.push(format!("{}: {}", subtask.description, err));
                    }
                }
            }
        }

        Ok(TaskResult {
            success: all_success,
            output: final_output,
            error: if errors.is_empty() { None } else { Some(errors.join("\n")) },
            metadata: Default::default(),
        })
    }

    /// Internal clone for task spawning
    #[allow(dead_code)]
    fn clone_internal(&self) -> Self {
        Self {
            llm_client: self.llm_client.clone(),
            registry: self.registry.clone(),
            shared_memory: self.shared_memory.clone(),
            router: Router::new(),
            planner: Planner::new(Some(self.llm_client.clone())),
            executor: Executor::new(
                self.llm_client.clone(),
                self.shared_memory.clone(),
                self.executor.pool.event_tx.clone(),
                self.executor.pool.max_concurrent,
                self.workspace_root.clone(),
            ),
            workspace_root: self.workspace_root.clone(),
        }
    }

    /// Internal subtask execution for parallel tasks
    async fn execute_subtask_internal(
        &self,
        subtask: Subtask,
        session: &Session,
        dependency_results: HashMap<Id, TaskResult>,
    ) -> Result<TaskResult> {
        info!("Executing subtask: {}", subtask.description);

        // Determine which agent to use, respecting routing confidence
        let agent = if let Some(agent_id) = &subtask.suggested_agent {
            let registry = self.registry.read().await;
            registry.get_by_id(agent_id).cloned()
        } else {
            // Fall back to routing
            let temp_task = Task::new(&subtask.description, subtask.task_type);
            let registry = self.registry.read().await;
            let analysis = self.router.analyze(self.llm_client.clone(), &registry, &temp_task, session).await?;
            let decision = self.router.route(temp_task, analysis).await?;

            // Only pick the agent if confidence is high enough
            if decision.confidence > AGENT_CONFIDENCE_THRESHOLD && !decision.selected_agents.is_empty() {
                let agent_id = &decision.selected_agents[0];
                let registry = self.registry.read().await;
                registry.get_by_id(agent_id).cloned()
            } else {
                None
            }
        };

        if let Some(agent) = agent {
            // Build context with dependency results
            let mut context_messages = session.messages.clone();
            for (dep_id, dep_result) in dependency_results {
                context_messages.push(Message::system(&format!(
                    "Previous subtask result (ID: {}): {}",
                    dep_id, dep_result.output
                )));
            }

            let context = ExecutionContext::new(&session.id)
                .with_messages(context_messages);

            let task = Task::new(&subtask.description, subtask.task_type);
            self.executor.execute_task(agent, task, context).await
        } else {
            Ok(TaskResult {
                success: false,
                output: String::new(),
                error: Some(format!("No high-confidence agent found for subtask: {}", subtask.description)),
                metadata: Default::default(),
            })
        }
    }

    /// Execute subtask with shared executor (for parallel execution without creating new pools)
    async fn execute_subtask_with_executor(
        executor: Executor,
        registry: Arc<RwLock<AgentRegistry>>,
        llm_client: Arc<dyn LlmProvider>,
        subtask: Subtask,
        session: &Session,
        dependency_results: HashMap<Id, TaskResult>,
    ) -> Result<TaskResult> {
        info!("Executing subtask with shared executor: {}", subtask.description);

        let agent = if let Some(agent_id) = &subtask.suggested_agent {
            let reg = registry.read().await;
            reg.get_by_id(agent_id).cloned()
        } else {
            let router = Router::new();
            let temp_task = Task::new(&subtask.description, subtask.task_type);
            let reg = registry.read().await;
            let analysis = router.analyze(llm_client.clone(), &reg, &temp_task, session).await?;
            let decision = router.route(temp_task, analysis).await?;

            if decision.confidence > AGENT_CONFIDENCE_THRESHOLD && !decision.selected_agents.is_empty() {
                let agent_id = &decision.selected_agents[0];
                let reg = registry.read().await;
                reg.get_by_id(agent_id).cloned()
            } else {
                None
            }
        };

        if let Some(agent) = agent {
            let mut context_messages = session.messages.clone();
            for (dep_id, dep_result) in dependency_results {
                context_messages.push(Message::system(&format!(
                    "Previous subtask result (ID: {}): {}",
                    dep_id, dep_result.output
                )));
            }

            let context = ExecutionContext::new(&session.id)
                .with_messages(context_messages);

            let task = Task::new(&subtask.description, subtask.task_type);
            executor.execute_task(agent, task, context).await
        } else {
            Ok(TaskResult {
                success: false,
                output: String::new(),
                error: Some(format!("No high-confidence agent found for subtask: {}", subtask.description)),
                metadata: Default::default(),
            })
        }
    }

    /// Execute a chat with a specific agent
    #[allow(dead_code)]
    pub async fn execute_chat(
        &self,
        agent: Agent,
        message: String,
        history: Vec<Message>,
        session_id: &str,
    ) -> Result<String> {
        self.executor.execute_chat(agent, message, history, session_id).await
    }

    /// Execute a streaming chat with a specific agent
    pub async fn execute_chat_streaming(
        &self,
        agent: Agent,
        message: String,
        history: Vec<Message>,
        session_id: &str,
    ) -> Result<String> {
        self.executor.execute_chat_streaming(agent, message, history, session_id).await
    }

    /// Execute a task with a specific agent
    #[allow(dead_code)]
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
    use crate::types::{TaskType, AgentRole};
    use crate::llm::{LlmClient, MockLlmClient};

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
    fn test_planner_new_with_llm() {
        let llm_client: Arc<dyn LlmProvider> = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let planner = Planner::new(Some(llm_client));
        let _ = planner;
    }

    #[test]
    fn test_planner_new_without_llm() {
        let planner = Planner::new(None);
        let _ = planner;
    }

    #[tokio::test]
    async fn test_planner_plan_no_llm_returns_single_subtask() {
        let planner = Planner::new(None);
        let task = Task::new("Test task", TaskType::CodeGeneration);
        let session = Session::new("Test Session");
        let registry = AgentRegistry::new();

        let plan = planner.plan(&task, &session, &registry).await.unwrap();

        assert_eq!(plan.subtasks.len(), 1);
        assert_eq!(plan.subtasks[0].description, "Test task");
        assert_eq!(plan.subtasks[0].task_type, TaskType::CodeGeneration);
    }

    #[tokio::test]
    async fn test_planner_plan_with_llm_calls_api() {
        let llm_client: Arc<dyn LlmProvider> = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let planner = Planner::new(Some(llm_client));
        let task = Task::new("Write a hello world function", TaskType::CodeGeneration);
        let session = Session::new("Test Session");
        let mut registry = AgentRegistry::new();
        registry.register(Agent::new("coder", AgentRole::Coder, "gpt-4o")
            .with_description("Writes code"));

        // This will call the LLM API - may fail if API key is invalid
        // but should not panic
        let result = planner.plan(&task, &session, &registry).await;
        
        // Either success or API error is acceptable
        match result {
            Ok(plan) => {
                assert!(!plan.subtasks.is_empty());
            }
            Err(e) => {
                // API error is expected if key is invalid
                assert!(e.to_string().contains("API") || e.to_string().contains("http") || e.to_string().contains("401"));
            }
        }
    }

    #[test]
    fn test_subtask_creation() {
        let subtask = Subtask::new("Write tests", TaskType::TestGeneration)
            .with_suggested_agent("tester")
            .with_dependencies(vec!["task-1", "task-2"]);

        assert_eq!(subtask.description, "Write tests");
        assert_eq!(subtask.task_type, TaskType::TestGeneration);
        assert_eq!(subtask.suggested_agent, Some("tester".to_string()));
        assert_eq!(subtask.dependencies.len(), 2);
    }

    #[test]
    fn test_plan_creation() {
        let task = Task::new("Build a feature", TaskType::CodeGeneration);
        let subtasks = vec![
            Subtask::new("Explore codebase", TaskType::Exploration),
            Subtask::new("Write code", TaskType::CodeGeneration),
        ];

        let plan = Plan::new(task.clone()).with_subtasks(subtasks.clone());

        assert_eq!(plan.subtasks.len(), 2);
        assert_eq!(plan.execution_order.len(), 2);
        assert_eq!(plan.original_task.description, "Build a feature");
    }

    // ==================== ExecutionContext Tests ====================

    #[test]
    fn test_execution_context_new() {
        let ctx = ExecutionContext::new("session-123");
        
        assert_eq!(ctx.session_id, "session-123");
        assert!(ctx.messages.is_empty());
    }

    #[test]
    fn test_execution_context_with_messages() {
        let messages = vec![Message::user("Hello")];
        let ctx = ExecutionContext::new("session-123")
            .with_messages(messages.clone());

        assert_eq!(ctx.messages.len(), 1);
        assert_eq!(ctx.messages[0].content, "Hello");
    }

    // ==================== Executor Tests ====================

    #[tokio::test]
    async fn test_executor_new() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client: Arc<dyn LlmProvider> = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let shared_memory = Arc::new(SharedMemory::new());
        
        let executor = Executor::new(llm_client, shared_memory, event_tx, 5, None);
        
        // Verify executor was created
        let count = executor.active_count().await;
        assert_eq!(count, 0); // No agents spawned yet
    }

    #[tokio::test]
    async fn test_executor_is_at_capacity() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client: Arc<dyn LlmProvider> = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let shared_memory = Arc::new(SharedMemory::new());
        
        let executor = Executor::new(llm_client, shared_memory, event_tx, 2, None);
        
        assert!(!executor.is_at_capacity().await);
    }

    // ==================== Orchestrator Tests ====================

    #[tokio::test]
    async fn test_orchestrator_new() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client: Arc<dyn LlmProvider> = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let registry = Arc::new(RwLock::new(AgentRegistry::new()));
        let shared_memory = Arc::new(SharedMemory::new());
        
        let orchestrator = Orchestrator::new(llm_client, registry, shared_memory, event_tx, 5, None);
        
        assert!(orchestrator.executor().active_count().await == 0);
    }

    #[tokio::test]
    async fn test_orchestrator_execute_auto_no_agents() {
        let (event_tx, _event_rx) = mpsc::channel(10);
        let llm_client: Arc<dyn LlmProvider> = Arc::new(LlmClient::new("test-key", "gpt-4o", 4096, 0.7));
        let registry = Arc::new(RwLock::new(AgentRegistry::new()));
        let shared_memory = Arc::new(SharedMemory::new());

        let orchestrator = Orchestrator::new(llm_client, registry, shared_memory, event_tx, 5, None);
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
        let mock_llm = MockLlmClient::new("mock stream response");
        mock_llm.set_streaming(false).await;
        let llm_client: Arc<dyn LlmProvider> = Arc::new(mock_llm);
        let mut registry = AgentRegistry::new();

        // Register a test agent
        let agent = Agent::new("test-agent", AgentRole::Coder, "gpt-4o")
            .with_description("Test agent");
        registry.register(agent);

        let registry = Arc::new(RwLock::new(registry));
        let shared_memory = Arc::new(SharedMemory::new());
        let orchestrator = Orchestrator::new(llm_client, registry, shared_memory, event_tx, 5, None);

        let agent = Agent::new("test-agent", AgentRole::Coder, "gpt-4o");
        let history = vec![Message::user("Hello")];

        let result = orchestrator
            .execute_chat_streaming(agent, "Test".to_string(), history, "test-session")
            .await;

        assert!(result.is_ok());
    }
}
