//! Explorer agent implementation
//!
//! This module provides the ExplorerAgent implementation for codebase exploration and analysis.

#![allow(dead_code)]

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::types::{Agent, AgentRole, Capability, Task, TaskResult, TaskType};
use crate::agent::TaskProcessor;
use crate::shared::SharedMemory;

/// Information about a file in the codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// Path to the file
    pub path: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// Programming language (if detected)
    pub language: Option<String>,
    /// Number of lines
    pub line_count: usize,
    /// Number of functions/methods detected
    pub function_count: usize,
    /// Import/dependency count
    pub import_count: usize,
}

/// Search result from codebase exploration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// File path where match was found
    pub file_path: String,
    /// Line number of match
    pub line_number: usize,
    /// The matching line content
    pub line_content: String,
    /// Surrounding context (lines before and after)
    pub context: Option<String>,
}

/// Symbol information (function, class, module, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol name
    pub name: String,
    /// Symbol type (function, class, module, constant, etc.)
    pub symbol_type: String,
    /// File where symbol is defined
    pub file_path: String,
    /// Line number where symbol is defined
    pub line_number: usize,
    /// Optional signature or definition snippet
    pub signature: Option<String>,
}

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
}

/// Explorer agent for codebase exploration
pub struct ExplorerAgent;

impl TaskProcessor for ExplorerAgent {
    fn process_task(&self, task: &Task, response: &str, _shared_memory: Arc<SharedMemory>) -> Result<TaskResult> {
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
                [Main functions, CLI handlers, etc.]"
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

        // Extract entry points from common patterns
        let entry_patterns = [
            "src/main.rs", "main.py", "index.js", "app.js", "main.go",
            "src/main.java", "Program.cs", "main.rb", "manage.py",
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

    /// Scan a directory and return file information
    pub fn scan_directory(dir_path: &Path, max_depth: Option<usize>) -> Result<Vec<FileInfo>> {
        let mut files = Vec::new();
        let max_depth = max_depth.unwrap_or(10);

        Self::scan_directory_recursive(dir_path, dir_path, max_depth, &mut files)?;

        Ok(files)
    }

    fn scan_directory_recursive(
        base_path: &Path,
        current_path: &Path,
        remaining_depth: usize,
        files: &mut Vec<FileInfo>,
    ) -> Result<()> {
        if remaining_depth == 0 {
            return Ok(());
        }

        let entries = fs::read_dir(current_path)
            .with_context(|| format!("Failed to read directory: {:?}", current_path))?;

        for entry in entries.flatten() {
            let path = entry.path();

            // Skip hidden files and common non-source directories
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "node_modules" || name == "target" 
                    || name == "build" || name == "dist" || name == "__pycache__"
                    || name == ".git" || name == "vendor" || name == "deps" {
                    continue;
                }
            }

            if path.is_dir() {
                Self::scan_directory_recursive(base_path, &path, remaining_depth - 1, files)?;
            } else if path.is_file() {
                if let Some(info) = Self::analyze_file(&path, base_path)? {
                    files.push(info);
                }
            }
        }

        Ok(())
    }

    /// Analyze a single file
    fn analyze_file(path: &Path, base_path: &Path) -> Result<Option<FileInfo>> {
        let metadata = fs::metadata(path)?;
        let size_bytes = metadata.len();

        // Skip very large files (>1MB) and binary files
        if size_bytes > 1024 * 1024 {
            return Ok(None);
        }

        let language = Self::detect_language(path);
        
        // Skip files without recognized language
        if language.is_none() {
            return Ok(None);
        }

        let content = fs::read_to_string(path).unwrap_or_default();
        let line_count = content.lines().count();
        let function_count = Self::count_functions(&content, language.as_deref());
        let import_count = Self::count_imports(&content, language.as_deref());

        let relative_path = path.strip_prefix(base_path)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        Ok(Some(FileInfo {
            path: relative_path,
            size_bytes,
            language,
            line_count,
            function_count,
            import_count,
        }))
    }

    /// Detect programming language from file extension
    fn detect_language(path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| {
                Some(match ext.to_lowercase().as_str() {
                    "rs" => "rust",
                    "py" => "python",
                    "js" => "javascript",
                    "ts" => "typescript",
                    "jsx" => "javascript",
                    "tsx" => "typescript",
                    "java" => "java",
                    "cpp" | "cc" | "cxx" => "cpp",
                    "c" | "h" => "c",
                    "hpp" | "hxx" => "cpp",
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
                    "css" | "scss" | "sass" | "less" => "css",
                    "md" => "markdown",
                    "sql" => "sql",
                    "graphql" => "graphql",
                    "proto" => "protobuf",
                    "dockerfile" => "dockerfile",
                    "ex" | "exs" => "elixir",
                    "erl" => "erlang",
                    "hs" => "haskell",
                    "clj" | "cljs" => "clojure",
                    "lua" => "lua",
                    "r" | "R" => "r",
                    "m" | "mm" => "objectivec",
                    "fs" | "fsx" => "fsharp",
                    "vue" => "vue",
                    "svelte" => "svelte",
                    _ => return None,
                }.to_string())
            })
    }

    /// Count functions in code
    fn count_functions(content: &str, language: Option<&str>) -> usize {
        match language {
            Some("rust") => {
                content.matches("fn ").count()
            }
            Some("python") => {
                content.matches("def ").count()
            }
            Some("javascript") | Some("typescript") => {
                let func_re = Regex::new(r"(?:function\s+\w+|\w+\s*[=:]\s*(?:async\s+)?\(|=>)").unwrap();
                func_re.find_iter(content).count()
            }
            Some("java") | Some("cpp") | Some("c") | Some("csharp") => {
                // Rough heuristic: count method-like patterns
                content.matches('(').count() / 3
            }
            Some("go") => {
                content.matches("func ").count()
            }
            _ => {
                // Generic: count function-like patterns
                let func_re = Regex::new(r"\bfn\b|\bfunction\b|\bdef\b|\bfunc\b").unwrap();
                func_re.find_iter(content).count()
            }
        }
    }

    /// Count imports/dependencies in code
    fn count_imports(content: &str, language: Option<&str>) -> usize {
        match language {
            Some("rust") => {
                content.matches("use ").count()
            }
            Some("python") => {
                content.matches("import ").count() + content.matches("from ").count()
            }
            Some("javascript") | Some("typescript") => {
                content.matches("import ").count() + content.matches("require(").count()
            }
            Some("java") => {
                content.matches("import ").count()
            }
            Some("go") => {
                content.matches("import ").count()
            }
            _ => {
                let import_re = Regex::new(r"\bimport\b|\brequire\b|\buse\b").unwrap();
                import_re.find_iter(content).count()
            }
        }
    }

    /// Search for a pattern in the codebase
    pub fn search_codebase(
        dir_path: &Path,
        pattern: &str,
        file_pattern: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        let regex = Regex::new(pattern)
            .with_context(|| format!("Invalid search pattern: {}", pattern))?;

        let file_regex = file_pattern
            .map(|p| Regex::new(p).context("Invalid file pattern"))
            .transpose()?;

        Self::search_recursive(dir_path, &regex, file_regex.as_ref(), &mut results)?;

        Ok(results)
    }

    fn search_recursive(
        path: &Path,
        regex: &Regex,
        file_regex: Option<&Regex>,
        results: &mut Vec<SearchResult>,
    ) -> Result<()> {
        let entries = fs::read_dir(path)?;

        for entry in entries.flatten() {
            let entry_path = entry.path();

            // Skip hidden files and common non-source directories
            if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "node_modules" || name == "target"
                    || name == "build" || name == "dist" || name == "__pycache__"
                    || name == ".git" || name == "vendor" {
                    continue;
                }
            }

            if entry_path.is_dir() {
                Self::search_recursive(&entry_path, regex, file_regex, results)?;
            } else if entry_path.is_file() {
                // Check file pattern if provided
                if let Some(fr) = file_regex {
                    if let Some(name) = entry_path.to_str() {
                        if !fr.is_match(name) {
                            continue;
                        }
                    }
                }

                // Read and search file content
                if let Ok(content) = fs::read_to_string(&entry_path) {
                    for (line_num, line) in content.lines().enumerate() {
                        if regex.is_match(line) {
                            // Get context (2 lines before and after)
                            let lines: Vec<&str> = content.lines().collect();
                            let start = line_num.saturating_sub(2);
                            let end = (line_num + 3).min(lines.len());
                            let context = lines[start..end].join("\n");

                            results.push(SearchResult {
                                file_path: entry_path.to_string_lossy().to_string(),
                                line_number: line_num + 1, // 1-indexed
                                line_content: line.to_string(),
                                context: Some(context),
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Find symbols (functions, classes, etc.) in a file
    pub fn find_symbols(file_path: &Path) -> Result<Vec<Symbol>> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        let language = Self::detect_language(file_path);
        let mut symbols = Vec::new();

        match language.as_deref() {
            Some("rust") => {
                // Find functions
                let func_re = Regex::new(r"^(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").unwrap();
                for (line_num, line) in content.lines().enumerate() {
                    if let Some(cap) = func_re.captures(line) {
                        symbols.push(Symbol {
                            name: cap[1].to_string(),
                            symbol_type: "function".to_string(),
                            file_path: file_path.to_string_lossy().to_string(),
                            line_number: line_num + 1,
                            signature: Some(line.trim().to_string()),
                        });
                    }
                }

                // Find structs and enums
                let struct_re = Regex::new(r"^(?:pub\s+)?(?:struct|enum|trait)\s+(\w+)").unwrap();
                for (line_num, line) in content.lines().enumerate() {
                    if let Some(cap) = struct_re.captures(line) {
                        let symbol_type = if line.contains("struct") {
                            "struct"
                        } else if line.contains("enum") {
                            "enum"
                        } else {
                            "trait"
                        };
                        symbols.push(Symbol {
                            name: cap[1].to_string(),
                            symbol_type: symbol_type.to_string(),
                            file_path: file_path.to_string_lossy().to_string(),
                            line_number: line_num + 1,
                            signature: Some(line.trim().to_string()),
                        });
                    }
                }
            }
            Some("python") => {
                let class_re = Regex::new(r"^class\s+(\w+)").unwrap();
                let func_re = Regex::new(r"^def\s+(\w+)").unwrap();

                for (line_num, line) in content.lines().enumerate() {
                    if let Some(cap) = class_re.captures(line) {
                        symbols.push(Symbol {
                            name: cap[1].to_string(),
                            symbol_type: "class".to_string(),
                            file_path: file_path.to_string_lossy().to_string(),
                            line_number: line_num + 1,
                            signature: Some(line.trim().to_string()),
                        });
                    } else if let Some(cap) = func_re.captures(line) {
                        symbols.push(Symbol {
                            name: cap[1].to_string(),
                            symbol_type: "function".to_string(),
                            file_path: file_path.to_string_lossy().to_string(),
                            line_number: line_num + 1,
                            signature: Some(line.trim().to_string()),
                        });
                    }
                }
            }
            _ => {
                // Generic symbol detection
                let symbol_re = Regex::new(r"^(?:function|class|def|fn|const|let|var)\s+(\w+)").unwrap();
                for (line_num, line) in content.lines().enumerate() {
                    if let Some(cap) = symbol_re.captures(line) {
                        symbols.push(Symbol {
                            name: cap[1].to_string(),
                            symbol_type: "symbol".to_string(),
                            file_path: file_path.to_string_lossy().to_string(),
                            line_number: line_num + 1,
                            signature: Some(line.trim().to_string()),
                        });
                    }
                }
            }
        }

        Ok(symbols)
    }

    /// Get a summary of the codebase
    pub fn summarize_codebase(dir_path: &Path) -> Result<CodebaseSummary> {
        let files = Self::scan_directory(dir_path, None)?;

        let mut summary = CodebaseSummary {
            total_files: files.len(),
            total_lines: files.iter().map(|f| f.line_count).sum(),
            files_by_language: HashMap::new(),
            directories: Vec::new(),
            entry_points: Vec::new(),
            dependencies: Vec::new(),
        };

        // Count files by language
        for file in &files {
            if let Some(lang) = &file.language {
                *summary.files_by_language.entry(lang.clone()).or_insert(0) += 1;
            }
        }

        // Find entry points
        let entry_patterns = ["main.rs", "main.py", "index.js", "app.js", "main.go", "main.java"];
        for file in &files {
            if let Some(name) = Path::new(&file.path).file_name() {
                if entry_patterns.iter().any(|p| p == &name.to_string_lossy().as_ref()) {
                    summary.entry_points.push(file.path.clone());
                }
            }
        }

        // Collect directories
        let mut dirs = HashSet::new();
        for file in &files {
            if let Some(parent) = Path::new(&file.path).parent() {
                for component in parent.components() {
                    if let Some(name) = component.as_os_str().to_str() {
                        dirs.insert(name.to_string());
                    }
                }
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
    use tempfile::TempDir;

    #[test]
    fn test_detect_language() {
        assert_eq!(ExplorerAgent::detect_language(Path::new("test.rs")), Some("rust".to_string()));
        assert_eq!(ExplorerAgent::detect_language(Path::new("test.py")), Some("python".to_string()));
        assert_eq!(ExplorerAgent::detect_language(Path::new("test.js")), Some("javascript".to_string()));
        assert_eq!(ExplorerAgent::detect_language(Path::new("test.unknown")), None);
    }

    #[test]
    fn test_count_functions_rust() {
        let code = r#"
fn main() {}
fn helper() {}
pub fn public_fn() {}
"#;
        assert_eq!(ExplorerAgent::count_functions(code, Some("rust")), 3);
    }

    #[test]
    fn test_count_functions_python() {
        let code = r#"
def func1():
    pass

def func2():
    pass
"#;
        assert_eq!(ExplorerAgent::count_functions(code, Some("python")), 2);
    }

    #[test]
    fn test_count_imports_rust() {
        let code = r#"
use std::io;
use std::fs::File;
use crate::module;
"#;
        assert_eq!(ExplorerAgent::count_imports(code, Some("rust")), 3);
    }

    #[test]
    fn test_count_imports_python() {
        let code = r#"
import os
from pathlib import Path
import sys
from typing import List
"#;
        assert_eq!(ExplorerAgent::count_imports(code, Some("python")), 4);
    }

    #[test]
    fn test_find_symbols_rust() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        let content = r#"
fn main() {}
pub fn helper() {}
struct MyStruct {}
pub enum MyEnum { A, B }
"#;
        fs::write(&file_path, content).unwrap();

        let symbols = ExplorerAgent::find_symbols(&file_path).unwrap();
        
        assert_eq!(symbols.len(), 4);
        assert!(symbols.iter().any(|s| s.name == "main" && s.symbol_type == "function"));
        assert!(symbols.iter().any(|s| s.name == "MyStruct" && s.symbol_type == "struct"));
        assert!(symbols.iter().any(|s| s.name == "MyEnum" && s.symbol_type == "enum"));
    }

    #[test]
    fn test_find_symbols_python() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.py");
        let content = r#"
class MyClass:
    pass

def my_function():
    pass
"#;
        fs::write(&file_path, content).unwrap();

        let symbols = ExplorerAgent::find_symbols(&file_path).unwrap();
        
        assert_eq!(symbols.len(), 2);
        assert!(symbols.iter().any(|s| s.name == "MyClass" && s.symbol_type == "class"));
        assert!(symbols.iter().any(|s| s.name == "my_function" && s.symbol_type == "function"));
    }

    #[test]
    fn test_scan_directory() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create some test files
        fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(temp_dir.path().join("lib.rs"), "pub fn lib_fn() {}").unwrap();
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::write(temp_dir.path().join("src/module.rs"), "pub fn mod_fn() {}").unwrap();

        let files = ExplorerAgent::scan_directory(temp_dir.path(), None).unwrap();
        
        assert_eq!(files.len(), 3);
        assert!(files.iter().any(|f| f.path.contains("main.rs")));
        assert!(files.iter().any(|f| f.path.contains("lib.rs")));
        assert!(files.iter().any(|f| f.path.contains("module.rs")));
    }

    #[test]
    fn test_search_codebase() {
        let temp_dir = TempDir::new().unwrap();
        
        fs::write(temp_dir.path().join("file1.rs"), "fn hello() {}").unwrap();
        fs::write(temp_dir.path().join("file2.rs"), "fn goodbye() {}").unwrap();

        let results = ExplorerAgent::search_codebase(temp_dir.path(), "fn hello", None).unwrap();
        
        assert_eq!(results.len(), 1);
        assert!(results[0].line_content.contains("fn hello"));
    }

    #[test]
    fn test_process_exploration_task() {
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
        let result = agent.process_task(&task, response, shared_memory).unwrap();

        assert!(result.success);
        assert_eq!(result.metadata.get("total_files").unwrap(), &serde_json::json!(50));
        assert_eq!(result.metadata.get("total_lines").unwrap(), &serde_json::json!(2500));
    }

    #[test]
    fn test_process_unsupported_task_type() {
        let task = Task::new("Write code", TaskType::CodeGeneration);
        let response = "Some response";

        let agent = ExplorerAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory);
        assert!(result.is_err());
    }
}
