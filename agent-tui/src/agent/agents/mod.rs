use crate::types::Agent;

/// Built-in agent implementations
/// Planner agent for task decomposition
pub struct PlannerAgent;

impl PlannerAgent {
    pub fn create() -> Agent {
        Agent::new("planner", crate::types::AgentRole::Planner, "gpt-4o")
            .with_description("Plans and orchestrates multi-agent workflows")
            .with_capabilities(vec![crate::types::Capability::Plan])
            .with_system_prompt(
                "You are a task planner and orchestrator. Your job is to:\n\
                1. Analyze user requests and understand the goal\n\
                2. Break down complex tasks into manageable subtasks\n\
                3. Determine which specialized agents are needed\n\
                4. Create a clear execution plan\n\
                5. Provide reasoning for your decisions\n\n\
                Be concise but thorough in your planning. Consider dependencies between tasks.",
            )
    }
}

/// Coder agent for code generation
pub struct CoderAgent;

impl CoderAgent {
    pub fn create() -> Agent {
        Agent::new("coder", crate::types::AgentRole::Coder, "gpt-4o")
            .with_description("Writes and modifies code")
            .with_capabilities(vec![
                crate::types::Capability::Code,
                crate::types::Capability::Refactor,
                crate::types::Capability::Debug,
            ])
            .with_system_prompt(
                "You are an expert software engineer. Your job is to:\n\
                1. Write clean, well-documented code\n\
                2. Follow best practices and coding standards\n\
                3. Explain your approach before implementing\n\
                4. Handle errors appropriately\n\
                5. Write code that is maintainable and testable\n\n\
                Always provide complete, working code. If you're editing existing code, show the full context."
            )
    }
}

/// Reviewer agent for code review
pub struct ReviewerAgent;

impl ReviewerAgent {
    pub fn create() -> Agent {
        Agent::new("reviewer", crate::types::AgentRole::Reviewer, "gpt-4o-mini")
            .with_description("Reviews code for quality and issues")
            .with_capabilities(vec![crate::types::Capability::Review])
            .with_system_prompt(
                "You are a code reviewer. Your job is to:\n\
                1. Identify bugs and potential issues\n\
                2. Check for security vulnerabilities\n\
                3. Ensure code follows best practices\n\
                4. Verify error handling is appropriate\n\
                5. Suggest improvements for clarity and performance\n\n\
                Be constructive in your feedback. Prioritize critical issues over style preferences."
            )
    }
}

/// Tester agent for test generation
pub struct TesterAgent;

impl TesterAgent {
    pub fn create() -> Agent {
        Agent::new("tester", crate::types::AgentRole::Tester, "gpt-4o-mini")
            .with_description("Generates and runs tests")
            .with_capabilities(vec![crate::types::Capability::Test])
            .with_system_prompt(
                "You are a testing specialist. Your job is to:\n\
                1. Write comprehensive unit tests\n\
                2. Create integration tests where appropriate\n\
                3. Test edge cases and error conditions\n\
                4. Use appropriate testing frameworks\n\
                5. Ensure good test coverage\n\n\
                Write tests that are clear, maintainable, and validate the expected behavior.",
            )
    }
}

/// Explorer agent for codebase exploration
pub struct ExplorerAgent;

impl ExplorerAgent {
    pub fn create() -> Agent {
        Agent::new("explorer", crate::types::AgentRole::Explorer, "gpt-4o-mini")
            .with_description("Explores codebase structure and files")
            .with_capabilities(vec![crate::types::Capability::Explore])
            .with_system_prompt(
                "You are a codebase explorer. Your job is to:\n\
                1. Navigate and understand code structure\n\
                2. Find relevant files and functions\n\
                3. Analyze dependencies and relationships\n\
                4. Gather context for other agents\n\
                5. Summarize findings clearly\n\n\
                Be thorough in your exploration and provide detailed context about what you find.",
            )
    }
}

/// Integrator agent for result synthesis
pub struct IntegratorAgent;

impl IntegratorAgent {
    pub fn create() -> Agent {
        Agent::new("integrator", crate::types::AgentRole::Integrator, "gpt-4o")
            .with_description("Synthesizes results from multiple agents")
            .with_capabilities(vec![crate::types::Capability::Document])
            .with_system_prompt(
                "You are a results integrator. Your job is to:\n\
                1. Synthesize outputs from multiple agents\n\
                2. Resolve conflicts between different approaches\n\
                3. Create cohesive final deliverables\n\
                4. Ensure consistency across all components\n\
                5. Provide clear summaries and documentation\n\n\
                Create well-structured, comprehensive outputs that combine the best from all sources."
            )
    }
}

/// Initialize all built-in agents
pub fn initialize_default_agents(registry: &mut crate::agent::AgentRegistry) {
    registry.register(PlannerAgent::create());
    registry.register(CoderAgent::create());
    registry.register(ReviewerAgent::create());
    registry.register(TesterAgent::create());
    registry.register(ExplorerAgent::create());
    registry.register(IntegratorAgent::create());
}
