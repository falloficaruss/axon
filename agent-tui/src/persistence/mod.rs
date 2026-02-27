//! Persistence module
//!
//! This module handles saving and loading sessions and memory.

use anyhow::{anyhow, Result};
use crate::types::{Session, Id};
use std::path::{Path, PathBuf};
use tokio::fs;
use serde::{Serialize, Deserialize};
use tracing::{info, error, debug};
use chrono::{DateTime, Utc};

/// Metadata for a session (used for listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: Id,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}

/// Session persistence
pub struct SessionStore {
    base_path: PathBuf,
}

impl SessionStore {
    /// Create a new session store
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Ensure the sessions directory exists
    async fn ensure_dir(&self) -> Result<()> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path).await?;
        }
        Ok(())
    }

    /// Get path for a specific session
    fn session_path(&self, session_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.json", session_id))
    }

    /// Save a session
    pub async fn save(&self, session: &Session) -> Result<()> {
        self.ensure_dir().await?;
        let path = self.session_path(&session.id);
        let json = serde_json::to_string_pretty(session)?;
        fs::write(path, json).await?;
        debug!("Saved session {} to {:?}", session.id, self.base_path);
        Ok(())
    }

    /// Load a session
    pub async fn load(&self, session_id: &str) -> Result<Session> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Err(anyhow!("Session {} not found", session_id));
        }
        let json = fs::read_to_string(path).await?;
        let session: Session = serde_json::from_str(&json)?;
        Ok(session)
    }

    /// List all sessions
    pub async fn list(&self) -> Result<Vec<SessionMetadata>> {
        self.ensure_dir().await?;
        let mut sessions = Vec::new();
        let mut entries = fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(json) = fs::read_to_string(&path).await {
                    if let Ok(session) = serde_json::from_str::<Session>(&json) {
                        sessions.push(SessionMetadata {
                            id: session.id,
                            title: session.title,
                            created_at: session.created_at,
                            updated_at: session.updated_at,
                            message_count: session.messages.len(),
                        });
                    }
                }
            }
        }

        // Sort by updated_at descending
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Delete a session
    pub async fn delete(&self, session_id: &str) -> Result<()> {
        let path = self.session_path(session_id);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }
}

/// Memory persistence
pub struct MemoryStore {
    base_path: PathBuf,
}

impl MemoryStore {
    /// Create a new memory store
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Ensure the memory directory exists
    async fn ensure_dir(&self, scope: &str) -> Result<PathBuf> {
        let dir = self.base_path.join(scope);
        if !dir.exists() {
            fs::create_dir_all(&dir).await?;
        }
        Ok(dir)
    }

    /// Get path for a specific key in a scope
    fn key_path(&self, scope_dir: &Path, key: &str) -> PathBuf {
        scope_dir.join(format!("{}.memory", key))
    }

    /// Store a value
    pub async fn set(&self, key: &str, value: &str, scope: &str) -> Result<()> {
        let scope_dir = self.ensure_dir(scope).await?;
        let path = self.key_path(&scope_dir, key);
        fs::write(path, value).await?;
        Ok(())
    }

    /// Retrieve a value
    pub async fn get(&self, key: &str, scope: &str) -> Result<Option<String>> {
        let scope_dir = self.base_path.join(scope);
        let path = self.key_path(&scope_dir, key);
        if !path.exists() {
            return Ok(None);
        }
        let value = fs::read_to_string(path).await?;
        Ok(Some(value))
    }

    /// Delete a value
    pub async fn delete(&self, key: &str, scope: &str) -> Result<()> {
        let scope_dir = self.base_path.join(scope);
        let path = self.key_path(&scope_dir, key);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }

    /// List all keys in a scope
    pub async fn list(&self, scope: &str) -> Result<Vec<String>> {
        let scope_dir = self.base_path.join(scope);
        if !scope_dir.exists() {
            return Ok(vec![]);
        }
        
        let mut keys = Vec::new();
        let mut entries = fs::read_dir(scope_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("memory") {
                if let Some(key) = path.file_stem().and_then(|s| s.to_str()) {
                    keys.push(key.to_string());
                }
            }
        }

        Ok(keys)
    }
}
