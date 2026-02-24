//! Agent pool for managing concurrent agent execution
//!
//! This module provides management of multiple running agents,
//! including lifecycle management, health checks, and resource limits.

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use anyhow::{anyhow, Result};

use crate::{
    agent::{AgentHandle, AgentInstance, AgentRuntimeBuilder, AgentEvent},
    llm::LlmClient,
    types::{Agent, AgentState, Id},
};

/// Manager for a pool of running agents
pub struct AgentPool {
    /// Maximum number of concurrent agents
    max_concurrent: usize,
    /// Currently running agent instances
    agents: Arc<RwLock<HashMap<Id, AgentInstance>>>,
    /// LLM client for agents
    llm_client: Arc<LlmClient>,
    /// Event sender for agent events
    event_tx: mpsc::Sender<AgentEvent>,
}

impl AgentPool {
    /// Create a new agent pool
    pub fn new(
        max_concurrent: usize,
        llm_client: Arc<LlmClient>,
        event_tx: mpsc::Sender<AgentEvent>,
    ) -> Self {
        Self {
            max_concurrent,
            agents: Arc::new(RwLock::new(HashMap::new())),
            llm_client,
            event_tx,
        }
    }

    /// Get the number of currently running agents
    pub async fn active_count(&self) -> usize {
        self.agents.read().await.len()
    }

    /// Check if the pool is at capacity
    pub async fn is_at_capacity(&self) -> bool {
        self.active_count().await >= self.max_concurrent
    }

    /// Spawn a new agent in the pool
    pub async fn spawn_agent(&self, agent: Agent) -> Result<AgentHandle> {
        // Try to cleanup finished agents if we might be at capacity
        if self.is_at_capacity().await {
            self.cleanup_finished().await;
        }

        // Check capacity again
        if self.is_at_capacity().await {
            return Err(anyhow!(
                "Agent pool at capacity (max {} agents)",
                self.max_concurrent
            ));
        }

        let agent_id = agent.id.clone();
        
        // Build and spawn the agent runtime
        let instance = AgentRuntimeBuilder::new()
            .agent(agent)
            .llm_client(self.llm_client.clone())
            .event_tx(self.event_tx.clone())
            .spawn()
            .await?;

        let handle = instance.handle.clone();

        // Store the instance
        self.agents.write().await.insert(agent_id, instance);

        info!("Spawned agent {} in pool", handle.agent_name);
        Ok(handle)
    }

    /// Get a handle to a running agent
    pub async fn get_agent(&self, agent_id: &Id) -> Option<AgentHandle> {
        self.agents
            .read()
            .await
            .get(agent_id)
            .map(|instance| instance.handle.clone())
    }

    /// Get all running agent handles
    pub async fn list_agents(&self) -> Vec<AgentHandle> {
        self.agents
            .read()
            .await
            .values()
            .map(|instance| instance.handle.clone())
            .collect()
    }

    /// Get the state of a specific agent
    pub async fn get_agent_state(&self, agent_id: &Id) -> Option<AgentState> {
        if let Some(instance) = self.agents.read().await.get(agent_id) {
            Some(instance.state().await)
        } else {
            None
        }
    }

    /// Get states of all running agents
    pub async fn get_all_states(&self) -> Vec<(Id, String, AgentState)> {
        let agents = self.agents.read().await;
        let mut states = Vec::new();
        
        for (id, instance) in agents.iter() {
            states.push((
                id.clone(),
                instance.name(),
                instance.state().await,
            ));
        }
        
        states
    }

    /// Shutdown a specific agent
    pub async fn shutdown_agent(&self, agent_id: &Id) -> Result<()> {
        let mut agents = self.agents.write().await;
        
        if let Some(instance) = agents.get(agent_id) {
            instance.handle.shutdown().await?;
            agents.remove(agent_id);
            info!("Shutdown agent {}", agent_id);
            Ok(())
        } else {
            Err(anyhow!("Agent {} not found in pool", agent_id))
        }
    }

    /// Shutdown all agents in the pool
    pub async fn shutdown_all(&self) -> Result<()> {
        let agents_to_shutdown: Vec<Id> = {
            let agents = self.agents.read().await;
            agents.keys().cloned().collect()
        };

        for agent_id in agents_to_shutdown {
            if let Err(e) = self.shutdown_agent(&agent_id).await {
                warn!("Error shutting down agent {}: {}", agent_id, e);
            }
        }

        info!("All agents in pool shut down");
        Ok(())
    }

    /// Clean up completed/failed agents
    pub async fn cleanup_finished(&self) -> usize {
        let mut agents = self.agents.write().await;
        let mut to_remove = Vec::new();

        for (id, instance) in agents.iter() {
            let state = instance.state().await;
            if state == AgentState::Completed || state == AgentState::Failed {
                to_remove.push(id.clone());
            }
        }

        let count = to_remove.len();
        for id in to_remove {
            if let Some(instance) = agents.remove(&id) {
                // Try to gracefully shutdown
                let _ = instance.handle.shutdown().await;
            }
        }

        if count > 0 {
            debug!("Cleaned up {} finished agents", count);
        }

        count
    }

    /// Check if a specific agent is running
    pub async fn is_running(&self, agent_id: &Id) -> bool {
        self.agents.read().await.contains_key(agent_id)
    }

    /// Get available capacity
    pub async fn available_capacity(&self) -> usize {
        let active = self.active_count().await;
        self.max_concurrent.saturating_sub(active)
    }
}

/// Builder for creating agent pools
pub struct AgentPoolBuilder {
    max_concurrent: Option<usize>,
    llm_client: Option<Arc<LlmClient>>,
    event_tx: Option<mpsc::Sender<AgentEvent>>,
}

impl AgentPoolBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            max_concurrent: None,
            llm_client: None,
            event_tx: None,
        }
    }

    /// Set maximum concurrent agents
    pub fn max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = Some(max);
        self
    }

    /// Set the LLM client
    pub fn llm_client(mut self, client: Arc<LlmClient>) -> Self {
        self.llm_client = Some(client);
        self
    }

    /// Set the event sender
    pub fn event_tx(mut self, tx: mpsc::Sender<AgentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Build the agent pool
    pub fn build(self) -> Result<AgentPool> {
        let max_concurrent = self.max_concurrent.unwrap_or(5);
        let llm_client = self.llm_client.ok_or_else(|| anyhow!("LLM client not set"))?;
        let event_tx = self.event_tx.ok_or_else(|| anyhow!("Event sender not set"))?;

        Ok(AgentPool::new(max_concurrent, llm_client, event_tx))
    }
}

impl Default for AgentPoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}
