pub mod agents;
pub mod runtime;
#[cfg(test)]
pub mod repro_test;

pub use runtime::{AgentHandle, AgentInstance, AgentRuntimeBuilder, AgentEvent};

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

    pub fn get(&self, id_or_name: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id_or_name || a.name == id_or_name)
    }

    pub fn get_mut(&mut self, id_or_name: &str) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id == id_or_name || a.name == id_or_name)
    }

    pub fn get_by_id(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    #[allow(dead_code)]
    pub fn get_by_name(&self, name: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.name == name)
    }

    pub fn list(&self) -> &[Agent] {
        &self.agents
    }

    #[allow(dead_code)]
    pub fn by_role(&self, role: AgentRole) -> Vec<&Agent> {
        self.agents.iter().filter(|a| a.role == role).collect()
    }

    #[allow(dead_code)]
    pub fn by_capability(&self, capability: Capability) -> Vec<&Agent> {
        self.agents.iter().filter(|a| a.capabilities.contains(&capability)).collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
