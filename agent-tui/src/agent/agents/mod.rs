pub mod coder;
pub mod explorer;
pub mod integrator;
pub mod planner;
pub mod reviewer;
pub mod tester;

pub use coder::CoderAgent;
pub use explorer::ExplorerAgent;
pub use integrator::IntegratorAgent;
pub use planner::PlannerAgent;
pub use reviewer::ReviewerAgent;
pub use tester::TesterAgent;

/// Initialize all built-in agents
pub fn initialize_default_agents(registry: &mut crate::agent::AgentRegistry) {
    registry.register(PlannerAgent::create());
    registry.register(CoderAgent::create());
    registry.register(ReviewerAgent::create());
    registry.register(TesterAgent::create());
    registry.register(ExplorerAgent::create());
    registry.register(IntegratorAgent::create());
}
