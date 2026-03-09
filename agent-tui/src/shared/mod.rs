//! Shared state module
//!
//! This module provides shared memory and state management.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared memory with hierarchical namespaces
pub struct SharedMemory {
    /// Global scope (shared across all sessions)
    global: Arc<RwLock<HashMap<String, Value>>>,
    /// Session scope (shared within a session)
    sessions: Arc<RwLock<HashMap<String, HashMap<String, Value>>>>,
    /// Agent scope (per-agent memory)
    agents: Arc<RwLock<HashMap<String, HashMap<String, Value>>>>,
}

impl SharedMemory {
    /// Create a new shared memory instance
    pub fn new() -> Self {
        Self {
            global: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get a value from global scope
    pub async fn get_global(&self, key: &str) -> Option<Value> {
        self.global.read().await.get(key).cloned()
    }

    /// Set a value in global scope
    #[allow(dead_code)]
    pub async fn set_global(&self, key: &str, value: Value) {
        self.global.write().await.insert(key.to_string(), value);
    }

    /// Get a value from session scope
    pub async fn get_session(&self, session_id: &str, key: &str) -> Option<Value> {
        self.sessions
            .read()
            .await
            .get(session_id)?
            .get(key)
            .cloned()
    }

    /// Set a value in session scope
    #[allow(dead_code)]
    pub async fn set_session(&self, session_id: &str, key: &str, value: Value) {
        self.sessions
            .write()
            .await
            .entry(session_id.to_string())
            .or_insert_with(HashMap::new)
            .insert(key.to_string(), value);
    }

    /// Get a value from agent scope
    pub async fn get_agent(&self, agent_id: &str, key: &str) -> Option<Value> {
        self.agents.read().await.get(agent_id)?.get(key).cloned()
    }

    /// Set a value in agent scope
    #[allow(dead_code)]
    pub async fn set_agent(&self, agent_id: &str, key: &str, value: Value) {
        self.agents
            .write()
            .await
            .entry(agent_id.to_string())
            .or_insert_with(HashMap::new)
            .insert(key.to_string(), value);
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new()
    }
}
