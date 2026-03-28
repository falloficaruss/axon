use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::persistence::{SessionStore, MemoryStore};
use crate::types::Session;

/// Manages session state and persistence
pub struct SessionManager {
    /// Current session
    pub session: Session,
    /// Saved sessions metadata
    pub sessions: Vec<crate::persistence::SessionMetadata>,
    /// Session store for persistence
    pub session_store: Arc<SessionStore>,
    /// Memory store for persistence
    pub memory_store: Arc<MemoryStore>,
    /// Last auto-save time
    pub last_save: Instant,
    /// Last session list refresh time
    pub last_session_refresh: Instant,
    /// Memory keys for memory manager
    pub memory_keys: Vec<String>,
    /// Selected memory key index
    pub selected_memory_key: usize,
    /// Cached memory values for memory manager
    pub cached_memory_values: HashMap<String, String>,
}

impl SessionManager {
    pub fn new(config: &Config) -> Self {
        let session = Session::new("New Session");
        let session_store = Arc::new(SessionStore::new(config.session_dir()));
        let memory_store = Arc::new(MemoryStore::new(config.memory_dir()));
        
        Self {
            session,
            sessions: Vec::new(),
            session_store,
            memory_store,
            last_save: Instant::now(),
            last_session_refresh: Instant::now(),
            memory_keys: Vec::new(),
            selected_memory_key: 0,
            cached_memory_values: HashMap::new(),
        }
    }

    /// Refresh the session list from disk
    pub async fn refresh_session_list(&mut self) -> Result<()> {
        self.sessions = self.session_store.list().await?;
        self.last_session_refresh = Instant::now();
        Ok(())
    }

    /// Load a session by ID, replacing the current session
    pub async fn load_session(&mut self, session_id: &str) -> Result<String> {
        let session = self.session_store.load(session_id).await?;
        let title = session.title.clone();
        self.session = session;
        info!("Loaded session {}", session_id);
        Ok(title)
    }

    /// Save the current session
    pub async fn save_session(&mut self, title: Option<String>) -> Result<()> {
        if let Some(t) = title {
            self.session.title = t;
        }
        self.session_store.save(&self.session).await?;
        self.last_save = Instant::now();
        Ok(())
    }

    /// Delete a session by ID
    pub async fn delete_session(&mut self, session_id: &str) -> Result<()> {
        self.session_store.delete(session_id).await?;
        Ok(())
    }

    /// Store a value in session memory
    pub async fn remember(&mut self, key: &str, value: &str) -> Result<()> {
        self.memory_store.set(key, value, "session").await?;
        Ok(())
    }

    /// Retrieve a value from session memory
    pub async fn recall(&mut self, key: &str) -> Result<Option<String>> {
        self.memory_store.get(key, "session").await
    }

    /// Delete a value from session memory
    pub async fn forget(&mut self, key: &str) -> Result<()> {
        self.memory_store.delete(key, "session").await?;
        Ok(())
    }

    /// Handle periodic tasks (auto-save, refresh)
    pub async fn on_tick(&mut self, auto_save_interval: Duration) -> Result<bool> {
        let mut session_refreshed = false;
        // Update session list periodically (every 10 seconds)
        if self.last_session_refresh.elapsed() >= Duration::from_secs(10) {
            let _ = self.refresh_session_list().await;
            session_refreshed = true;
        }

        // Handle auto-save
        if self.last_save.elapsed() >= auto_save_interval {
            if !self.session.messages.is_empty() {
                debug!("Auto-saving session...");
                if let Err(e) = self.session_store.save(&self.session).await {
                    error!("Failed to auto-save session: {}", e);
                }
            }
            self.last_save = Instant::now();
        }

        Ok(session_refreshed)
    }

    /// Fetch memory keys from store
    pub async fn refresh_memory_keys(&mut self) -> Result<()> {
        if let Ok(keys) = self.memory_store.list("session").await {
            self.memory_keys = keys;
        }
        Ok(())
    }

    /// Fetch selected memory value from store and cache it
    pub async fn refresh_selected_memory_value(&mut self) -> Result<()> {
        if let Some(key) = self.memory_keys.get(self.selected_memory_key) {
            if let Ok(Some(value)) = self.memory_store.get(key, "session").await {
                self.cached_memory_values.insert(key.clone(), value);
            }
        }
        Ok(())
    }
}
