//! Persistence module
//!
//! This module handles saving and loading sessions and memory.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::types::{Id, Run, RunEvent, RunStatus, Session};

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

    /// Validate that a session ID is safe to use as a filename.
    fn validate_session_id(session_id: &str) -> Result<()> {
        if session_id.is_empty() {
            return Err(anyhow!("Session ID cannot be empty"));
        }

        if !session_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            return Err(anyhow!(
                "Invalid session ID '{}': only ASCII letters, numbers, '-' and '_' are allowed",
                session_id
            ));
        }

        Ok(())
    }

    /// Save a session (uses atomic write to prevent corruption)
    pub async fn save(&self, session: &Session) -> Result<()> {
        self.ensure_dir().await?;
        let path = self.session_path(&session.id);
        let json = serde_json::to_string_pretty(session)?;

        // Write to temp file first, then rename for atomic operation
        let temp_path = path.with_extension("json.tmp");
        fs::write(&temp_path, &json).await?;
        fs::rename(&temp_path, &path).await?;

        debug!("Saved session {} to {:?}", session.id, self.base_path);
        Ok(())
    }

    /// Load a session
    pub async fn load(&self, session_id: &str) -> Result<Session> {
        Self::validate_session_id(session_id)?;
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
        Self::validate_session_id(session_id)?;
        let path = self.session_path(session_id);
        if !path.exists() {
            return Err(anyhow!("Session {} not found", session_id));
        }
        fs::remove_file(path).await?;
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

    /// Validate key to prevent path traversal
    fn validate_key(&self, key: &str) -> Result<()> {
        if key.is_empty() || key.len() > 256 {
            return Err(anyhow!("Invalid key: length must be 1-256 characters"));
        }
        if !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(anyhow!("Invalid key: must be alphanumeric, underscore, or hyphen only"));
        }
        if key.starts_with('.') || key.contains("..") {
            return Err(anyhow!("Invalid key: cannot start with dot or contain path traversal"));
        }
        Ok(())
    }

    /// Validate scope to prevent path traversal
    fn validate_scope(&self, scope: &str) -> Result<()> {
        if scope.is_empty() || scope.len() > 64 {
            return Err(anyhow!("Invalid scope: length must be 1-64 characters"));
        }
        if !scope.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(anyhow!("Invalid scope: must be alphanumeric, underscore, or hyphen only"));
        }
        Ok(())
    }

    /// Store a value
    pub async fn set(&self, key: &str, value: &str, scope: &str) -> Result<()> {
        self.validate_key(key)?;
        self.validate_scope(scope)?;
        let scope_dir = self.ensure_dir(scope).await?;
        let path = self.key_path(&scope_dir, key);
        fs::write(path, value).await?;
        Ok(())
    }

    /// Retrieve a value
    pub async fn get(&self, key: &str, scope: &str) -> Result<Option<String>> {
        self.validate_key(key)?;
        self.validate_scope(scope)?;
        let scope_dir = self.ensure_dir(scope).await?;
        let path = self.key_path(&scope_dir, key);
        if !path.exists() {
            return Ok(None);
        }
        let value = fs::read_to_string(path).await?;
        Ok(Some(value))
    }

    /// Delete a value
    pub async fn delete(&self, key: &str, scope: &str) -> Result<()> {
        self.validate_key(key)?;
        self.validate_scope(scope)?;
        let scope_dir = self.ensure_dir(scope).await?;
        let path = self.key_path(&scope_dir, key);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }

    /// List all keys in a scope
    pub async fn list(&self, scope: &str) -> Result<Vec<String>> {
        self.validate_scope(scope)?;
        let scope_dir = self.ensure_dir(scope).await?;

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

/// Append-only persistence for execution runs and their event streams
pub struct RunStore {
    base_path: PathBuf,
}

impl RunStore {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    async fn ensure_dir(&self) -> Result<()> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path).await?;
        }
        Ok(())
    }

    fn validate_run_id(run_id: &str) -> Result<()> {
        if run_id.is_empty() {
            return Err(anyhow!("Run ID cannot be empty"));
        }

        if !run_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            return Err(anyhow!(
                "Invalid run ID '{}': only ASCII letters, numbers, '-' and '_' are allowed",
                run_id
            ));
        }

        Ok(())
    }

    fn run_dir(&self, run_id: &str) -> PathBuf {
        self.base_path.join(run_id)
    }

    fn run_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("run.json")
    }

    fn event_log_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("events.jsonl")
    }

    pub async fn create_run(&self, run: &Run) -> Result<()> {
        Self::validate_run_id(&run.id)?;
        self.ensure_dir().await?;

        let run_dir = self.run_dir(&run.id);
        if !run_dir.exists() {
            fs::create_dir_all(&run_dir).await?;
        }

        self.save_run(run).await
    }

    pub async fn save_run(&self, run: &Run) -> Result<()> {
        Self::validate_run_id(&run.id)?;
        self.ensure_dir().await?;

        let run_dir = self.run_dir(&run.id);
        if !run_dir.exists() {
            fs::create_dir_all(&run_dir).await?;
        }

        let path = self.run_path(&run.id);
        let json = serde_json::to_string_pretty(run)?;
        let temp_path = path.with_extension("json.tmp");
        fs::write(&temp_path, &json).await?;
        fs::rename(&temp_path, &path).await?;

        Ok(())
    }

    pub async fn load_run(&self, run_id: &str) -> Result<Run> {
        Self::validate_run_id(run_id)?;
        let path = self.run_path(run_id);
        if !path.exists() {
            return Err(anyhow!("Run {} not found", run_id));
        }

        let json = fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&json)?)
    }

    pub async fn update_run_status(
        &self,
        run_id: &str,
        status: RunStatus,
        error: Option<String>,
    ) -> Result<Run> {
        let mut run = self.load_run(run_id).await?;
        run.transition_to(status);
        run.error = error;
        self.save_run(&run).await?;
        Ok(run)
    }

    pub async fn append_event(&self, event: &RunEvent) -> Result<()> {
        Self::validate_run_id(&event.run_id)?;
        self.ensure_dir().await?;

        let run_dir = self.run_dir(&event.run_id);
        if !run_dir.exists() {
            fs::create_dir_all(&run_dir).await?;
        }

        let path = self.event_log_path(&event.run_id);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        let line = serde_json::to_string(event)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;

        Ok(())
    }

    pub async fn load_events(&self, run_id: &str) -> Result<Vec<RunEvent>> {
        Self::validate_run_id(run_id)?;
        let path = self.event_log_path(run_id);
        if !path.exists() {
            return Ok(vec![]);
        }

        let contents = fs::read_to_string(path).await?;
        let mut events = Vec::new();
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            events.push(serde_json::from_str(line)?);
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::{RunStore, SessionStore};
    use crate::types::{Run, RunEvent, RunEventKind, RunStatus};
    use tempfile::tempdir;
    use tokio::fs;

    #[tokio::test]
    async fn delete_returns_error_for_missing_session() {
        let dir = tempdir().expect("create tempdir");
        let store = SessionStore::new(dir.path().to_path_buf());

        let err = store
            .delete("missing_session")
            .await
            .expect_err("missing session should be reported as an error");

        assert!(
            err.to_string().contains("not found"),
            "expected not found error, got: {err}"
        );
    }

    #[tokio::test]
    async fn delete_rejects_path_traversal_session_id() {
        let dir = tempdir().expect("create tempdir");
        let store = SessionStore::new(dir.path().to_path_buf());

        let err = store
            .delete("../escape")
            .await
            .expect_err("path traversal session IDs must be rejected");

        assert!(
            err.to_string().contains("Invalid session ID"),
            "expected invalid ID error, got: {err}"
        );
    }

    #[tokio::test]
    async fn delete_removes_existing_session_file() {
        let dir = tempdir().expect("create tempdir");
        let store = SessionStore::new(dir.path().to_path_buf());
        let session_path = dir.path().join("valid_session.json");
        fs::write(&session_path, "{}")
            .await
            .expect("write test session");

        store
            .delete("valid_session")
            .await
            .expect("existing session should delete successfully");

        assert!(
            !session_path.exists(),
            "expected session file to be deleted"
        );
    }

    #[tokio::test]
    async fn run_store_persists_runs_and_events() {
        let dir = tempdir().expect("create tempdir");
        let store = RunStore::new(dir.path().to_path_buf());

        let mut run = Run::new("session-1", "Test run", Some("task-1".to_string()));
        store.create_run(&run).await.expect("create run");

        let event = RunEvent::new(
            &run.id,
            &run.session_id,
            run.task_id.clone(),
            RunEventKind::Created,
        );
        store.append_event(&event).await.expect("append event");

        run.transition_to(RunStatus::Running);
        store.save_run(&run).await.expect("save updated run");

        let loaded_run = store.load_run(&run.id).await.expect("load run");
        let loaded_events = store.load_events(&run.id).await.expect("load events");

        assert_eq!(loaded_run.status, RunStatus::Running);
        assert_eq!(loaded_events.len(), 1);
        assert!(matches!(loaded_events[0].kind, RunEventKind::Created));
    }
}
