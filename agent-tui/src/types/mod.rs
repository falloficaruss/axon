use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for entities
pub type Id = String;

/// Generate a new unique ID
pub fn generate_id() -> Id {
    Uuid::new_v4().to_string()
}

/// Agent roles define the primary function of an agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    /// Orchestrates multi-agent workflows and decomposes tasks
    Planner,
    /// Writes and edits code
    Coder,
    /// Reviews code for quality and issues
    Reviewer,
    /// Generates and executes tests
    Tester,
    /// Explores files and codebase
    Explorer,
    /// Synthesizes results from multiple agents
    Integrator,
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Planner => "planner",
            AgentRole::Coder => "coder",
            AgentRole::Reviewer => "reviewer",
            AgentRole::Tester => "tester",
            AgentRole::Explorer => "explorer",
            AgentRole::Integrator => "integrator",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AgentRole::Planner => "Orchestrates workflows and plans tasks",
            AgentRole::Coder => "Writes and modifies code",
            AgentRole::Reviewer => "Reviews code for quality",
            AgentRole::Tester => "Creates and runs tests",
            AgentRole::Explorer => "Explores codebase structure",
            AgentRole::Integrator => "Combines results from multiple agents",
        }
    }
}

/// Current state of an agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    /// Agent is idle and available
    Idle,
    /// Agent is currently processing a task
    Running,
    /// Agent has completed its task
    Completed,
    /// Agent encountered an error
    Failed,
}

/// Capabilities that agents can have
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    Code,
    Refactor,
    Debug,
    Optimize,
    Review,
    Test,
    Explore,
    Plan,
    Document,
}

/// Represents an agent that can process tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Id,
    pub name: String,
    pub role: AgentRole,
    pub description: String,
    pub capabilities: Vec<Capability>,
    pub system_prompt: String,
    pub model: String,
    pub state: AgentState,
    pub enabled: bool,
    pub config: HashMap<String, serde_json::Value>,
}

impl Agent {
    pub fn new(name: &str, role: AgentRole, model: &str) -> Self {
        Self {
            id: generate_id(),
            name: name.to_string(),
            role,
            description: role.description().to_string(),
            capabilities: vec![],
            system_prompt: String::new(),
            model: model.to_string(),
            state: AgentState::Idle,
            enabled: true,
            config: HashMap::new(),
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<Capability>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_system_prompt(mut self, prompt: &str) -> Self {
        self.system_prompt = prompt.to_string();
        self
    }
}

/// Types of tasks that can be assigned
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    /// Generate new code
    CodeGeneration,
    /// Edit existing code
    CodeEdit,
    /// Review code
    CodeReview,
    /// Generate tests
    TestGeneration,
    /// Run tests
    TestExecution,
    /// Explore codebase
    Exploration,
    /// Plan workflow
    Planning,
    /// Synthesize results
    Synthesis,
    /// General question/answer
    General,
}

/// Status of a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is pending execution
    Pending,
    /// Task is currently running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A unit of work to be executed by an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Id,
    pub description: String,
    pub task_type: TaskType,
    pub assigned_agent: Option<Id>,
    pub dependencies: Vec<Id>,
    pub status: TaskStatus,
    pub result: Option<TaskResult>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub parent_task: Option<Id>,
    pub subtasks: Vec<Id>,
}

impl Task {
    pub fn new(description: &str, task_type: TaskType) -> Self {
        Self {
            id: generate_id(),
            description: description.to_string(),
            task_type,
            assigned_agent: None,
            dependencies: vec![],
            status: TaskStatus::Pending,
            result: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            parent_task: None,
            subtasks: vec![],
        }
    }

    pub fn assign_to(mut self, agent_id: &str) -> Self {
        self.assigned_agent = Some(agent_id.to_string());
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<&str>) -> Self {
        self.dependencies = deps.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// Role of a message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    /// Message from the user
    User,
    /// Message from an agent
    Agent,
    /// System message
    System,
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Id,
    pub role: MessageRole,
    pub content: String,
    pub agent_id: Option<Id>,
    pub timestamp: DateTime<Utc>,
    pub task_id: Option<Id>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    pub fn user(content: &str) -> Self {
        Self {
            id: generate_id(),
            role: MessageRole::User,
            content: content.to_string(),
            agent_id: None,
            timestamp: Utc::now(),
            task_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn agent(content: &str, agent_id: &str) -> Self {
        Self {
            id: generate_id(),
            role: MessageRole::Agent,
            content: content.to_string(),
            agent_id: Some(agent_id.to_string()),
            timestamp: Utc::now(),
            task_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn system(content: &str) -> Self {
        Self {
            id: generate_id(),
            role: MessageRole::System,
            content: content.to_string(),
            agent_id: None,
            timestamp: Utc::now(),
            task_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_task(mut self, task_id: &str) -> Self {
        self.task_id = Some(task_id.to_string());
        self
    }
}

/// Session mode determines how tasks are routed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMode {
    /// Automatically route tasks to appropriate agents
    Auto,
    /// Require user to manually select agents
    Manual,
}

impl SessionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionMode::Auto => "auto",
            SessionMode::Manual => "manual",
        }
    }
}

/// A conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Id,
    pub title: String,
    pub messages: Vec<Message>,
    pub tasks: Vec<Task>,
    pub mode: SessionMode,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub active_agent: Option<Id>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Session {
    pub fn new(title: &str) -> Self {
        let now = Utc::now();
        Self {
            id: generate_id(),
            title: title.to_string(),
            messages: vec![],
            tasks: vec![],
            mode: SessionMode::Auto,
            created_at: now,
            updated_at: now,
            active_agent: None,
            metadata: HashMap::new(),
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    pub fn add_task(&mut self, task: Task) {
        self.tasks.push(task);
        self.updated_at = Utc::now();
    }

    pub fn set_mode(&mut self, mode: SessionMode) {
        self.mode = mode;
        self.updated_at = Utc::now();
    }

    pub fn set_active_agent(&mut self, agent_id: Option<&str>) {
        self.active_agent = agent_id.map(|s| s.to_string());
        self.updated_at = Utc::now();
    }
}

/// Routing decision from the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub task: Task,
    pub selected_agents: Vec<Id>,
    pub confidence: f32,
    pub reasoning: String,
    pub requires_confirmation: bool,
}

impl RoutingDecision {
    pub fn new(task: Task, agents: Vec<&str>, confidence: f32) -> Self {
        Self {
            task,
            selected_agents: agents.iter().map(|s| s.to_string()).collect(),
            confidence,
            reasoning: String::new(),
            requires_confirmation: confidence < 0.8,
        }
    }

    pub fn with_reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = reasoning.to_string();
        self
    }
}

/// Context for task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub session_id: Id,
    pub parent_task: Option<Id>,
    pub messages: Vec<Message>,
    pub depth: u32,
    pub max_depth: u32,
}

impl ExecutionContext {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            parent_task: None,
            messages: vec![],
            depth: 0,
            max_depth: 5,
        }
    }

    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }

    pub fn child(&self, task_id: &str) -> Self {
        Self {
            session_id: self.session_id.clone(),
            parent_task: Some(task_id.to_string()),
            messages: self.messages.clone(),
            depth: self.depth + 1,
            max_depth: self.max_depth,
        }
    }
}

/// Application events for UI updates
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// New message received
    MessageReceived(Message),
    /// Part of a streaming message received
    MessageUpdate { agent_id: Id, content: String },
    /// Agent state changed
    AgentStateChanged(Id, AgentState),
    /// Task status changed
    TaskStatusChanged(Id, TaskStatus),
    /// Routing decision made
    RoutingDecision(RoutingDecision),
    /// Error occurred
    Error(String),
    /// Status update
    Status(String),
}

/// Result of a routing analysis
#[derive(Debug, Clone)]
pub struct RoutingAnalysis {
    pub task_type: TaskType,
    pub suggested_agents: Vec<(Id, f32)>, // agent_id, confidence
    pub can_parallelize: bool,
    pub estimated_complexity: u32, // 1-10
    pub requires_subtasks: bool,
}

/// A subtask generated by the planner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: Id,
    pub description: String,
    pub task_type: TaskType,
    pub suggested_agent: Option<Id>,
    pub dependencies: Vec<Id>,
    pub status: TaskStatus,
    pub result: Option<TaskResult>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Subtask {
    pub fn new(description: &str, task_type: TaskType) -> Self {
        Self {
            id: generate_id(),
            description: description.to_string(),
            task_type,
            suggested_agent: None,
            dependencies: vec![],
            status: TaskStatus::Pending,
            result: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }

    pub fn with_suggested_agent(mut self, agent_id: &str) -> Self {
        self.suggested_agent = Some(agent_id.to_string());
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<&str>) -> Self {
        self.dependencies = deps.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// Plan generated by the Planner agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: Id,
    pub original_task: Task,
    pub subtasks: Vec<Subtask>,
    pub execution_order: Vec<Id>,
    pub parallel_groups: Vec<Vec<Id>>,
    pub created_at: DateTime<Utc>,
}

impl Plan {
    pub fn new(original_task: Task) -> Self {
        Self {
            id: generate_id(),
            original_task,
            subtasks: vec![],
            execution_order: vec![],
            parallel_groups: vec![],
            created_at: Utc::now(),
        }
    }

    pub fn with_subtasks(mut self, subtasks: Vec<Subtask>) -> Self {
        self.subtasks = subtasks;
        self.execution_order = self.subtasks.iter().map(|s| s.id.clone()).collect();
        self
    }

    pub fn with_parallel_groups(mut self, groups: Vec<Vec<Id>>) -> Self {
        self.parallel_groups = groups;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let agent = Agent::new("coder-1", AgentRole::Coder, "gpt-4o")
            .with_description("A coding agent")
            .with_capabilities(vec![Capability::Code, Capability::Refactor]);

        assert_eq!(agent.name, "coder-1");
        assert_eq!(agent.role, AgentRole::Coder);
        assert_eq!(agent.model, "gpt-4o");
        assert_eq!(agent.capabilities.len(), 2);
        assert!(agent.enabled);
    }

    #[test]
    fn test_task_creation() {
        let task = Task::new("Write a function", TaskType::CodeGeneration)
            .assign_to("agent-1")
            .with_dependencies(vec!["task-1"]);

        assert_eq!(task.description, "Write a function");
        assert_eq!(task.task_type, TaskType::CodeGeneration);
        assert_eq!(task.assigned_agent, Some("agent-1".to_string()));
        assert_eq!(task.dependencies.len(), 1);
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(msg.agent_id.is_none());

        let agent_msg = Message::agent("Response", "agent-1");
        assert_eq!(agent_msg.role, MessageRole::Agent);
        assert_eq!(agent_msg.agent_id, Some("agent-1".to_string()));
    }

    #[test]
    fn test_session_management() {
        let mut session = Session::new("Test Session");
        assert_eq!(session.messages.len(), 0);

        session.add_message(Message::user("Hello"));
        assert_eq!(session.messages.len(), 1);

        session.add_task(Task::new("Task 1", TaskType::General));
        assert_eq!(session.tasks.len(), 1);
    }

    #[test]
    fn test_routing_decision() {
        let task = Task::new("Test", TaskType::CodeGeneration);
        let decision = RoutingDecision::new(task, vec!["agent-1"], 0.9);

        assert_eq!(decision.selected_agents.len(), 1);
        assert_eq!(decision.confidence, 0.9);
        assert!(!decision.requires_confirmation);
    }

    #[test]
    fn test_subtask_creation() {
        let subtask = Subtask::new("Write unit tests", TaskType::TestGeneration)
            .with_suggested_agent("tester")
            .with_dependencies(vec!["task-1"]);

        assert_eq!(subtask.description, "Write unit tests");
        assert_eq!(subtask.task_type, TaskType::TestGeneration);
        assert_eq!(subtask.suggested_agent, Some("tester".to_string()));
        assert_eq!(subtask.dependencies.len(), 1);
        assert_eq!(subtask.status, TaskStatus::Pending);
    }

    #[test]
    fn test_plan_creation() {
        let task = Task::new("Build a feature", TaskType::CodeGeneration);
        let subtasks = vec![
            Subtask::new("Explore codebase", TaskType::Exploration),
            Subtask::new("Write code", TaskType::CodeGeneration),
            Subtask::new("Write tests", TaskType::TestGeneration),
        ];

        let plan = Plan::new(task.clone()).with_subtasks(subtasks);

        assert_eq!(plan.subtasks.len(), 3);
        assert_eq!(plan.execution_order.len(), 3);
        assert_eq!(plan.original_task.description, "Build a feature");
        assert!(plan.parallel_groups.is_empty());
    }

    #[test]
    fn test_plan_with_parallel_groups() {
        let task = Task::new("Build a feature", TaskType::CodeGeneration);
        let subtasks = vec![
            Subtask::new("Explore codebase", TaskType::Exploration),
            Subtask::new("Write module A", TaskType::CodeGeneration),
            Subtask::new("Write module B", TaskType::CodeGeneration),
        ];

        let mut plan = Plan::new(task.clone()).with_subtasks(subtasks);

        // Create parallel groups after plan is created
        let parallel_group = vec![plan.subtasks[1].id.clone(), plan.subtasks[2].id.clone()];
        plan.parallel_groups = vec![parallel_group];

        assert_eq!(plan.parallel_groups.len(), 1);
        assert_eq!(plan.parallel_groups[0].len(), 2);
    }
}
