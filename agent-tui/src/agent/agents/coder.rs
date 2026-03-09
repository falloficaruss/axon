//! Coder agent implementation
//!
//! This module provides the CoderAgent implementation for code generation and editing.

#![allow(dead_code)]

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent::TaskProcessor;
use crate::shared::SharedMemory;
use crate::types::{Agent, AgentRole, Capability, Task, TaskResult, TaskType};

/// Code block extracted from LLM response
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlock {
    /// Programming language (e.g., "rust", "python", "javascript")
    pub language: String,
    /// The actual code content
    pub code: String,
    /// Optional file path if specified in the code block
    pub file_path: Option<String>,
}

/// Represents a change to be made to a file
#[derive(Debug, Clone, PartialEq)]
pub struct CodeChange {
    /// Path to the file to be changed
    pub file_path: PathBuf,
    /// The new content to write
    pub content: String,
    /// Type of operation
    pub operation: FileOperation,
}

/// Type of file operation
#[derive(Debug, Clone, PartialEq)]
pub enum FileOperation {
    /// Create a new file
    Create,
    /// Update an existing file
    Update,
    /// Delete a file
    Delete,
}

/// Coder agent for code generation and editing
pub struct CoderAgent;

impl TaskProcessor for CoderAgent {
    fn process_task(
        &self,
        task: &Task,
        response: &str,
        _shared_memory: Arc<SharedMemory>,
    ) -> Result<TaskResult> {
        Self::process_task_internal(task, response)
    }
}

impl CoderAgent {
    /// Create a new CoderAgent
    pub fn create() -> Agent {
        Agent::new("coder", AgentRole::Coder, "gpt-4o")
            .with_description("Writes and modifies code")
            .with_capabilities(vec![
                Capability::Code,
                Capability::Refactor,
                Capability::Debug,
            ])
            .with_system_prompt(
                "You are an expert software engineer. Your job is to:\n\
                1. Write clean, well-documented code\n\
                2. Follow best practices and coding standards\n\
                3. Explain your approach before implementing\n\
                4. Handle errors appropriately\n\
                5. Write code that is maintainable and testable\n\n\
                Always provide complete, working code. If you're editing existing code, show the full context.\n\n\
                When providing code, use markdown code blocks with the language specified:\n\
                ```rust\n\
                // your code here\n\
                ```\n\n\
                You can optionally specify a file path:\n\
                ```rust:src/main.rs\n\
                // your code here\n\
                ```"
            )
    }

    /// Process a task (code generation or editing)
    fn process_task_internal(task: &Task, llm_response: &str) -> Result<TaskResult> {
        match task.task_type {
            TaskType::CodeGeneration => Self::handle_code_generation(llm_response),
            TaskType::CodeEdit => Self::handle_code_edit(llm_response),
            _ => Err(anyhow!(
                "Unsupported task type: {:?}. CoderAgent only supports CodeGeneration and CodeEdit",
                task.task_type
            )),
        }
    }

    /// Handle code generation task
    fn handle_code_generation(llm_response: &str) -> Result<TaskResult> {
        let code_blocks = Self::extract_code_blocks(llm_response)?;

        if code_blocks.is_empty() {
            return Ok(TaskResult {
                success: true,
                output: llm_response.to_string(),
                error: None,
                metadata: HashMap::new(),
            });
        }

        let mut metadata = HashMap::new();
        let mut generated_files = Vec::new();

        for block in &code_blocks {
            if let Some(file_path) = &block.file_path {
                generated_files.push(file_path.clone());
            }
        }

        metadata.insert(
            "code_blocks".to_string(),
            serde_json::json!(code_blocks.len()),
        );
        metadata.insert(
            "generated_files".to_string(),
            serde_json::json!(generated_files),
        );

        Ok(TaskResult {
            success: true,
            output: llm_response.to_string(),
            error: None,
            metadata,
        })
    }

    /// Handle code editing task
    fn handle_code_edit(llm_response: &str) -> Result<TaskResult> {
        let code_blocks = Self::extract_code_blocks(llm_response)?;

        if code_blocks.is_empty() {
            return Ok(TaskResult {
                success: true,
                output: llm_response.to_string(),
                error: None,
                metadata: HashMap::new(),
            });
        }

        let mut metadata = HashMap::new();
        let mut edited_files = Vec::new();

        for block in &code_blocks {
            if let Some(file_path) = &block.file_path {
                edited_files.push(file_path.clone());
            }
        }

        metadata.insert(
            "code_blocks".to_string(),
            serde_json::json!(code_blocks.len()),
        );
        metadata.insert("edited_files".to_string(), serde_json::json!(edited_files));

        Ok(TaskResult {
            success: true,
            output: llm_response.to_string(),
            error: None,
            metadata,
        })
    }

    /// Extract code blocks from markdown-formatted LLM response
    ///
    /// Supports formats:
    /// - ```language\ncode\n```
    /// - ```language:path/to/file.ext\ncode\n```
    pub fn extract_code_blocks(text: &str) -> Result<Vec<CodeBlock>> {
        let mut blocks = Vec::new();

        // Regex to match code blocks with optional file path
        // Format: ```language[:filepath]
        let re = Regex::new(r"```(\w+)(?::([^\n]+))?\n([\s\S]*?)```")
            .context("Failed to compile regex")?;

        for cap in re.captures_iter(text) {
            let language = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "text".to_string());

            let file_path = cap.get(2).map(|m| m.as_str().trim().to_string());

            let code = cap
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            blocks.push(CodeBlock {
                language,
                code,
                file_path,
            });
        }

        Ok(blocks)
    }

    /// Read a file from disk
    pub fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
        fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read file: {:?}", path.as_ref()))
    }

    /// Write content to a file (creates parent directories if needed)
    pub fn write_file<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
        let path = path.as_ref();

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directories for: {:?}", path))?;
        }

        fs::write(path, content).with_context(|| format!("Failed to write file: {:?}", path))
    }

    /// Delete a file
    pub fn delete_file<P: AsRef<Path>>(path: P) -> Result<()> {
        fs::remove_file(path.as_ref())
            .with_context(|| format!("Failed to delete file: {:?}", path.as_ref()))
    }

    /// Apply code changes to files
    pub fn apply_changes(changes: &[CodeChange]) -> Result<Vec<String>> {
        let mut applied_files = Vec::new();

        for change in changes {
            match change.operation {
                FileOperation::Create | FileOperation::Update => {
                    Self::write_file(&change.file_path, &change.content)?;
                    applied_files.push(change.file_path.to_string_lossy().to_string());
                }
                FileOperation::Delete => {
                    Self::delete_file(&change.file_path)?;
                    applied_files.push(change.file_path.to_string_lossy().to_string());
                }
            }
        }

        Ok(applied_files)
    }

    /// Detect programming language from file extension
    pub fn detect_language<P: AsRef<Path>>(path: P) -> Option<String> {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                match ext.to_lowercase().as_str() {
                    "rs" => "rust",
                    "py" => "python",
                    "js" => "javascript",
                    "ts" => "typescript",
                    "jsx" => "javascript",
                    "tsx" => "typescript",
                    "java" => "java",
                    "cpp" | "cc" | "cxx" => "cpp",
                    "c" => "c",
                    "h" | "hpp" => "cpp",
                    "go" => "go",
                    "rb" => "ruby",
                    "php" => "php",
                    "cs" => "csharp",
                    "swift" => "swift",
                    "kt" => "kotlin",
                    "scala" => "scala",
                    "sh" | "bash" => "bash",
                    "yaml" | "yml" => "yaml",
                    "json" => "json",
                    "toml" => "toml",
                    "xml" => "xml",
                    "html" => "html",
                    "css" => "css",
                    "md" => "markdown",
                    _ => "text",
                }
                .to_string()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ==================== Code Block Extraction Tests ====================

    #[test]
    fn test_extract_single_code_block() {
        let text =
            "Here's some code:\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\nThat's it!";
        let blocks = CoderAgent::extract_code_blocks(text).unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "rust");
        assert!(blocks[0].code.contains("fn main()"));
        assert!(blocks[0].file_path.is_none());
    }

    #[test]
    fn test_extract_code_block_with_path() {
        let text = "```rust:src/main.rs\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let blocks = CoderAgent::extract_code_blocks(text).unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "rust");
        assert_eq!(blocks[0].file_path, Some("src/main.rs".to_string()));
        assert!(blocks[0].code.contains("fn main()"));
    }

    #[test]
    fn test_extract_multiple_code_blocks() {
        let text = "First file:\n\
```python\nprint('hello')\n```\n\
Second file:\n\
```javascript\nconsole.log('world');\n```";
        let blocks = CoderAgent::extract_code_blocks(text).unwrap();

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].language, "python");
        assert_eq!(blocks[1].language, "javascript");
        assert!(blocks[0].code.contains("print"));
        assert!(blocks[1].code.contains("console.log"));
    }

    #[test]
    fn test_extract_no_code_blocks() {
        let text = "Just plain text without any code blocks.";
        let blocks = CoderAgent::extract_code_blocks(text).unwrap();
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_extract_code_block_with_spaces_in_path() {
        let text = "```typescript:src/my file.ts\nconst x = 1;\n```";
        let blocks = CoderAgent::extract_code_blocks(text).unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "typescript");
        assert_eq!(blocks[0].file_path, Some("src/my file.ts".to_string()));
    }

    #[test]
    fn test_extract_empty_code_block() {
        let text = "```rust\n```";
        let blocks = CoderAgent::extract_code_blocks(text).unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "");
    }

    // ==================== File Operations Tests ====================

    #[test]
    fn test_write_and_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let content = "Hello, World!";
        CoderAgent::write_file(&file_path, content).unwrap();

        let read_content = CoderAgent::read_file(&file_path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_write_file_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested/dir/test.txt");

        let content = "Nested file content";
        CoderAgent::write_file(&file_path, content).unwrap();

        let read_content = CoderAgent::read_file(&file_path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let result = CoderAgent::read_file("/tmp/nonexistent_file_12345.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("to_delete.txt");

        // Create file
        CoderAgent::write_file(&file_path, "content").unwrap();
        assert!(file_path.exists());

        // Delete file
        CoderAgent::delete_file(&file_path).unwrap();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_delete_nonexistent_file() {
        let result = CoderAgent::delete_file("/tmp/nonexistent_file_12345.txt");
        assert!(result.is_err());
    }

    // ==================== Language Detection Tests ====================

    #[test]
    fn test_detect_language_rust() {
        assert_eq!(
            CoderAgent::detect_language("main.rs"),
            Some("rust".to_string())
        );
    }

    #[test]
    fn test_detect_language_python() {
        assert_eq!(
            CoderAgent::detect_language("script.py"),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_detect_language_javascript() {
        assert_eq!(
            CoderAgent::detect_language("app.js"),
            Some("javascript".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("component.jsx"),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn test_detect_language_typescript() {
        assert_eq!(
            CoderAgent::detect_language("app.ts"),
            Some("typescript".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("component.tsx"),
            Some("typescript".to_string())
        );
    }

    #[test]
    fn test_detect_language_go() {
        assert_eq!(
            CoderAgent::detect_language("main.go"),
            Some("go".to_string())
        );
    }

    #[test]
    fn test_detect_language_java() {
        assert_eq!(
            CoderAgent::detect_language("Main.java"),
            Some("java".to_string())
        );
    }

    #[test]
    fn test_detect_language_cpp() {
        assert_eq!(
            CoderAgent::detect_language("main.cpp"),
            Some("cpp".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("main.cc"),
            Some("cpp".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("main.cxx"),
            Some("cpp".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("header.h"),
            Some("cpp".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("header.hpp"),
            Some("cpp".to_string())
        );
    }

    #[test]
    fn test_detect_language_c() {
        assert_eq!(CoderAgent::detect_language("main.c"), Some("c".to_string()));
    }

    #[test]
    fn test_detect_language_config_files() {
        assert_eq!(
            CoderAgent::detect_language("config.yaml"),
            Some("yaml".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("config.yml"),
            Some("yaml".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("data.json"),
            Some("json".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("Cargo.toml"),
            Some("toml".to_string())
        );
    }

    #[test]
    fn test_detect_language_unknown() {
        assert_eq!(
            CoderAgent::detect_language("file.xyz"),
            Some("text".to_string())
        );
    }

    #[test]
    fn test_detect_language_no_extension() {
        assert_eq!(CoderAgent::detect_language("Makefile"), None);
    }

    #[test]
    fn test_detect_language_case_insensitive() {
        assert_eq!(
            CoderAgent::detect_language("Main.RS"),
            Some("rust".to_string())
        );
        assert_eq!(
            CoderAgent::detect_language("Script.PY"),
            Some("python".to_string())
        );
    }

    // ==================== Code Change Application Tests ====================

    #[test]
    fn test_apply_single_change_create() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");

        let changes = vec![CodeChange {
            file_path: file_path.clone(),
            content: "New content".to_string(),
            operation: FileOperation::Create,
        }];

        let applied = CoderAgent::apply_changes(&changes).unwrap();
        assert_eq!(applied.len(), 1);
        assert!(file_path.exists());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "New content");
    }

    #[test]
    fn test_apply_single_change_update() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("existing_file.txt");

        // Create initial file
        fs::write(&file_path, "Old content").unwrap();

        let changes = vec![CodeChange {
            file_path: file_path.clone(),
            content: "Updated content".to_string(),
            operation: FileOperation::Update,
        }];

        let applied = CoderAgent::apply_changes(&changes).unwrap();
        assert_eq!(applied.len(), 1);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Updated content");
    }

    #[test]
    fn test_apply_multiple_changes() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");

        let changes = vec![
            CodeChange {
                file_path: file1.clone(),
                content: "Content 1".to_string(),
                operation: FileOperation::Create,
            },
            CodeChange {
                file_path: file2.clone(),
                content: "Content 2".to_string(),
                operation: FileOperation::Create,
            },
        ];

        let applied = CoderAgent::apply_changes(&changes).unwrap();
        assert_eq!(applied.len(), 2);
        assert!(file1.exists());
        assert!(file2.exists());
    }

    #[test]
    fn test_apply_change_delete() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("to_delete.txt");

        // Create file first
        fs::write(&file_path, "To be deleted").unwrap();
        assert!(file_path.exists());

        let changes = vec![CodeChange {
            file_path: file_path.clone(),
            content: String::new(),
            operation: FileOperation::Delete,
        }];

        let applied = CoderAgent::apply_changes(&changes).unwrap();
        assert_eq!(applied.len(), 1);
        assert!(!file_path.exists());
    }

    // ==================== Task Processing Tests ====================

    #[test]
    fn test_process_code_generation_task() {
        let task = Task::new("Generate a Rust function", TaskType::CodeGeneration);
        let llm_response =
            "Here's the code:\n```rust\nfn hello() {\n    println!(\"Hello\");\n}\n```";

        let agent = CoderAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent
            .process_task(&task, llm_response, shared_memory)
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("fn hello()"));
        assert_eq!(result.error, None);

        let code_blocks_count = result.metadata.get("code_blocks").unwrap();
        assert_eq!(code_blocks_count, &serde_json::json!(1));
    }

    #[test]
    fn test_process_code_generation_with_file_path() {
        let task = Task::new("Generate a main function", TaskType::CodeGeneration);
        let llm_response =
            "```rust:src/main.rs\nfn main() {\n    println!(\"Hello, world!\");\n}\n```";

        let agent = CoderAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent
            .process_task(&task, llm_response, shared_memory)
            .unwrap();
        assert!(result.success);

        let generated_files = result.metadata.get("generated_files").unwrap();
        assert_eq!(generated_files, &serde_json::json!(vec!["src/main.rs"]));
    }

    #[test]
    fn test_process_code_edit_task() {
        let task = Task::new("Edit function", TaskType::CodeEdit);
        let llm_response =
            "Updated:\n```python:app.py\ndef greet(name):\n    print(f\"Hello, {name}!\")\n```";

        let agent = CoderAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent
            .process_task(&task, llm_response, shared_memory)
            .unwrap();
        assert!(result.success);

        let edited_files = result.metadata.get("edited_files").unwrap();
        assert_eq!(edited_files, &serde_json::json!(vec!["app.py"]));
    }

    #[test]
    fn test_process_task_no_code_blocks() {
        let task = Task::new("Explain code", TaskType::CodeGeneration);
        let llm_response = "This is just an explanation without any code blocks.";

        let agent = CoderAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent
            .process_task(&task, llm_response, shared_memory)
            .unwrap();
        assert!(result.success);
        assert_eq!(result.output, llm_response);
        assert!(result.metadata.is_empty());
    }

    #[test]
    fn test_process_unsupported_task_type() {
        let task = Task::new("Review code", TaskType::CodeReview);
        let llm_response = "Some response";

        let agent = CoderAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, llm_response, shared_memory);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported task type"));
    }

    #[test]
    fn test_process_multiple_code_blocks() {
        let task = Task::new("Generate multiple files", TaskType::CodeGeneration);
        let llm_response = "First file:\n\
```rust:src/lib.rs\npub fn lib_fn() {}\n```\n\
Second file:\n\
```rust:src/main.rs\nfn main() {}\n```";

        let agent = CoderAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent
            .process_task(&task, llm_response, shared_memory)
            .unwrap();
        assert!(result.success);

        let code_blocks_count = result.metadata.get("code_blocks").unwrap();
        assert_eq!(code_blocks_count, &serde_json::json!(2));

        let generated_files = result.metadata.get("generated_files").unwrap();
        assert_eq!(
            generated_files,
            &serde_json::json!(vec!["src/lib.rs", "src/main.rs"])
        );
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_full_workflow_create_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("generated.rs");

        // Simulate LLM response
        let llm_response = format!(
            "```rust:{}\nfn test() {{\n    assert!(true);\n}}\n```",
            file_path.to_string_lossy()
        );

        // Extract code blocks
        let blocks = CoderAgent::extract_code_blocks(&llm_response).unwrap();
        assert_eq!(blocks.len(), 1);

        // Apply changes
        let changes = vec![CodeChange {
            file_path: file_path.clone(),
            content: blocks[0].code.clone(),
            operation: FileOperation::Create,
        }];

        CoderAgent::apply_changes(&changes).unwrap();

        // Verify file was created and contains correct content
        let content = CoderAgent::read_file(&file_path).unwrap();
        assert!(content.contains("fn test()"));
        assert!(content.contains("assert!(true)"));
    }
}
