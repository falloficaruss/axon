pub mod agents;
pub mod runtime;

pub use runtime::{AgentHandle, AgentInstance, AgentRuntime, AgentRuntimeBuilder, AgentCommand, AgentEvent, AgentResponse};

use crate::types::{Agent, AgentRole, Capability};

/// Registry of available agents
pub struct AgentRegistry {
    agents: Vec<Agent>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: vec![],
        }
    }

    pub fn register(&mut self, agent: Agent) {
        self.agents.push(agent);
    }

    pub fn get(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id == id)
    }

    pub fn list(&self) -> &[Agent] {
        &self.agents
    }

    pub fn by_role(&self, role: AgentRole) -> Vec<&Agent> {
        self.agents.iter().filter(|a| a.role == role).collect()
    }

    pub fn by_capability(&self, capability: Capability) -> Vec<&Agent> {
        self.agents.iter().filter(|a| a.capabilities.contains(&capability)).collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
