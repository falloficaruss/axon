//! Explorer agent implementation
//!
//! This module provides the ExplorerAgent implementation for codebase exploration and analysis.

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::agent::TaskProcessor;
use crate::shared::SharedMemory;
use crate::types::{Agent, AgentRole, Capability, Task, TaskResult, TaskType};

use async_trait::async_trait;

/// Codebase structure summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseSummary {
    /// Total number of files
    pub total_files: usize,
    /// Total lines of code
    pub total_lines: usize,
    /// Files by language
    pub files_by_language: HashMap<String, usize>,
    /// Directory structure (depth-limited)
    pub directories: Vec<String>,
    /// Entry points detected (main files, etc.)
    pub entry_points: Vec<String>,
    /// Dependencies detected
    pub dependencies: Vec<String>,
    /// Discovered files from response (e.g., "File: path/to/file")
    pub discovered_files: Vec<String>,
    /// Discovered symbols from response (e.g., "Symbol: my_func")
    pub discovered_symbols: Vec<String>,
}

/// Explorer agent for codebase exploration
pub struct ExplorerAgent;

#[async_trait]
impl TaskProcessor for ExplorerAgent {
    async fn process_task(
        &self,
        task: &Task,
        response: &str,
        _shared_memory: Arc<SharedMemory>,
    ) -> Result<TaskResult> {
        Self::process_task_internal(task, response)
    }
}

impl ExplorerAgent {
    /// Create a new ExplorerAgent
    pub fn create() -> Agent {
        Agent::new("explorer", AgentRole::Explorer, "gpt-4o")
            .with_description("Explores and analyzes codebase structure and files")
            .with_capabilities(vec![
                Capability::Explore,
                Capability::Document,
                Capability::Review,
            ])
            .with_system_prompt(
                "You are a codebase exploration expert. Your job is to:\n\
                1. Navigate and understand code structure\n\
                2. Find relevant files, functions, and symbols\n\
                3. Analyze dependencies and relationships between components\n\
                4. Gather context for other agents\n\
                5. Summarize findings clearly and concisely\n\n\
                When exploring:\n\
                - Start from entry points (main files, index files, etc.)\n\
                - Trace imports and dependencies\n\
                - Identify key abstractions and patterns\n\
                - Note any architectural decisions or conventions\n\n\
                Present findings in a structured format:\n\
                ## Overview\n\
                [High-level summary]\n\n\
                ## Structure\n\
                [Directory/file organization]\n\n\
                ## Key Components\n\
                [Important files and their purposes]\n\n\
                ## Dependencies\n\
                [External and internal dependencies]\n\n\
                ## Entry Points\n\
                [Main functions, CLI handlers, etc.]",
            )
    }

    /// Process a task (exploration or search)
    fn process_task_internal(task: &Task, llm_response: &str) -> Result<TaskResult> {
        match task.task_type {
            TaskType::Exploration => Self::handle_exploration(llm_response),
            TaskType::General => Self::handle_general_query(llm_response),
            _ => Err(anyhow!(
                "Unsupported task type: {:?}. ExplorerAgent only supports Exploration and General queries",
                task.task_type
            )),
        }
    }

    /// Handle exploration task
    fn handle_exploration(llm_response: &str) -> Result<TaskResult> {
        let summary = Self::parse_exploration_response(llm_response)?;

        let mut metadata = HashMap::new();
        metadata.insert(
            "total_files".to_string(),
            serde_json::json!(summary.total_files),
        );
        metadata.insert(
            "total_lines".to_string(),
            serde_json::json!(summary.total_lines),
        );
        metadata.insert(
            "languages".to_string(),
            serde_json::json!(summary.files_by_language),
        );
        metadata.insert(
            "entry_points".to_string(),
            serde_json::json!(summary.entry_points),
        );
        metadata.insert(
            "dependencies".to_string(),
            serde_json::json!(summary.dependencies),
        );
        metadata.insert(
            "discovered_files".to_string(),
            serde_json::json!(summary.discovered_files),
        );
        metadata.insert(
            "discovered_symbols".to_string(),
            serde_json::json!(summary.discovered_symbols),
        );

        Ok(TaskResult {
            success: true,
            output: llm_response.to_string(),
            error: None,
            metadata,
        })
    }

    /// Handle general query task
    fn handle_general_query(llm_response: &str) -> Result<TaskResult> {
        Ok(TaskResult {
            success: true,
            output: llm_response.to_string(),
            error: None,
            metadata: HashMap::new(),
        })
    }

    /// Parse the LLM response to extract exploration summary
    fn parse_exploration_response(response: &str) -> Result<CodebaseSummary> {
        let mut summary = CodebaseSummary {
            total_files: 0,
            total_lines: 0,
            files_by_language: HashMap::new(),
            directories: Vec::new(),
            entry_points: Vec::new(),
            dependencies: Vec::new(),
            discovered_files: Vec::new(),
            discovered_symbols: Vec::new(),
        };

        // Try to extract numbers from patterns like "X files" or "X lines"
        let files_re = Regex::new(r"(\d+)\s+files?").unwrap();
        let lines_re = Regex::new(r"(\d+)\s+lines?").unwrap();

        if let Some(cap) = files_re.captures(response) {
            if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse().ok()) {
                summary.total_files = n;
            }
        }

        if let Some(cap) = lines_re.captures(response) {
            if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse().ok()) {
                summary.total_lines = n;
            }
        }

        // Extract discovered files from patterns like "File: path" or "* File: path"
        let file_re = Regex::new(r"(?:^|\n)\*?\s*File:\s*(\S+)").unwrap();
        for cap in file_re.captures_iter(response) {
            if let Some(file) = cap.get(1) {
                summary.discovered_files.push(file.as_str().to_string());
            }
        }

        // Extract discovered symbols from patterns like "Symbol: name" or "* Symbol: name"
        let symbol_re = Regex::new(r"(?:^|\n)\*?\s*Symbol:\s*(\S+)").unwrap();
        for cap in symbol_re.captures_iter(response) {
            if let Some(symbol) = cap.get(1) {
                summary.discovered_symbols.push(symbol.as_str().to_string());
            }
        }

        // Extract entry points from common patterns
        let entry_patterns = [
            "src/main.rs",
            "main.py",
            "index.js",
            "app.js",
            "main.go",
            "src/main.java",
            "Program.cs",
            "main.rb",
            "manage.py",
        ];
        for pattern in &entry_patterns {
            if response.contains(pattern) {
                summary.entry_points.push(pattern.to_string());
            }
        }

        // Extract directory names (look for patterns like "src/", "lib/", etc.)
        let dir_re = Regex::new(r"\b(src|lib|tests?|specs?|docs?|examples?|bin|dist|build|config|scripts?|utils?|components?|modules?|packages?|controllers?|models?|views?|services?|handlers?|middleware|routes?|api|core|common|shared|internal|external|public|private)/\b").unwrap();
        let mut dirs = HashSet::new();
        for cap in dir_re.captures_iter(response) {
            if let Some(dir) = cap.get(1) {
                dirs.insert(dir.as_str().to_string());
            }
        }
        summary.directories = dirs.into_iter().collect();

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::SharedMemory;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_process_exploration_task() {
        let task = Task::new("Explore codebase", TaskType::Exploration);
        let response = r#"## Overview
Found 50 files with 2500 lines of code.

## Entry Points
- src/main.rs

## Dependencies
- serde
- tokio
"#;

        let agent = ExplorerAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).await.unwrap();

        assert!(result.success);
        assert_eq!(
            result.metadata.get("total_files").unwrap(),
            &serde_json::json!(50)
        );
        assert_eq!(
            result.metadata.get("total_lines").unwrap(),
            &serde_json::json!(2500)
        );
    }

    #[tokio::test]
    async fn test_process_unsupported_task_type() {
        let task = Task::new("Write code", TaskType::CodeGeneration);
        let response = "Some response";

        let agent = ExplorerAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).await;
        assert!(result.is_err());
    }
}
