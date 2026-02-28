//! Tester agent implementation
//!
//! This module provides the TesterAgent implementation for test generation and execution.

#![allow(dead_code)]

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::{Agent, AgentRole, Capability, Task, TaskResult, TaskType};
use crate::agent::TaskProcessor;

/// Type of test
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestType {
    /// Unit test
    Unit,
    /// Integration test
    Integration,
    /// End-to-end test
    E2E,
    /// Property-based test
    Property,
    /// Benchmark
    Benchmark,
}

impl TestType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TestType::Unit => "unit",
            TestType::Integration => "integration",
            TestType::E2E => "e2e",
            TestType::Property => "property",
            TestType::Benchmark => "benchmark",
        }
    }
}

/// Result of a single test case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseResult {
    /// Name of the test
    pub name: String,
    /// Whether the test passed
    pub passed: bool,
    /// Execution time in milliseconds
    pub duration_ms: Option<u64>,
    /// Error message if failed
    pub error: Option<String>,
    /// Type of test
    pub test_type: TestType,
}

/// Result of test execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestExecutionResult {
    /// Total number of tests
    pub total: usize,
    /// Number of passed tests
    pub passed: usize,
    /// Number of failed tests
    pub failed: usize,
    /// Number of skipped tests
    pub skipped: usize,
    /// Total execution time in milliseconds
    pub total_duration_ms: Option<u64>,
    /// Individual test results
    pub test_results: Vec<TestCaseResult>,
    /// Standard output from test runner
    pub stdout: String,
    /// Standard error from test runner
    pub stderr: String,
}

/// Represents a test file to be created
#[derive(Debug, Clone, PartialEq)]
pub struct TestFile {
    /// Path to the test file
    pub file_path: PathBuf,
    /// Test file content
    pub content: String,
    /// Target file being tested
    pub target_file: Option<PathBuf>,
}

/// Tester agent for test generation and execution
pub struct TesterAgent;

impl TaskProcessor for TesterAgent {
    fn process_task(&self, task: &Task, response: &str) -> Result<TaskResult> {
        Self::process_task_internal(task, response)
    }
}

impl TesterAgent {
    /// Create a new TesterAgent
    pub fn create() -> Agent {
        Agent::new("tester", AgentRole::Tester, "gpt-4o")
            .with_description("Generates and runs comprehensive tests")
            .with_capabilities(vec![
                Capability::Test,
                Capability::Debug,
                Capability::Review,
            ])
            .with_system_prompt(
                "You are a testing expert. Your job is to:\n\
                1. Write comprehensive unit, integration, and end-to-end tests\n\
                2. Test edge cases, error conditions, and boundary values\n\
                3. Use appropriate testing frameworks for the language\n\
                4. Ensure good code coverage\n\
                5. Write clear, maintainable test code\n\n\
                When generating tests:\n\
                - Follow the AAA pattern (Arrange, Act, Assert)\n\
                - Use descriptive test names that explain the scenario\n\
                - Include both positive and negative test cases\n\
                - Mock external dependencies appropriately\n\n\
                When providing test code, use markdown code blocks with file paths:\n\
                ```rust:tests/test_module.rs\n\
                // your test code here\n\
                ```"
            )
    }

    /// Process a task (test generation or execution)
    fn process_task_internal(task: &Task, llm_response: &str) -> Result<TaskResult> {
        match task.task_type {
            TaskType::TestGeneration => Self::handle_test_generation(llm_response),
            TaskType::TestExecution => Self::handle_test_execution(llm_response),
            _ => Err(anyhow!(
                "Unsupported task type: {:?}. TesterAgent only supports TestGeneration and TestExecution",
                task.task_type
            )),
        }
    }

    /// Handle test generation task
    fn handle_test_generation(llm_response: &str) -> Result<TaskResult> {
        let test_files = Self::extract_test_files(llm_response)?;

        if test_files.is_empty() {
            return Ok(TaskResult {
                success: true,
                output: llm_response.to_string(),
                error: None,
                metadata: HashMap::new(),
            });
        }

        let mut metadata = HashMap::new();
        let mut generated_files = Vec::new();

        for test_file in &test_files {
            if let Some(path) = test_file.file_path.to_str() {
                generated_files.push(path.to_string());
            }
        }

        metadata.insert(
            "test_files".to_string(),
            serde_json::json!(generated_files),
        );
        metadata.insert(
            "generated_files".to_string(),
            serde_json::json!(generated_files),
        );
        metadata.insert(
            "test_count".to_string(),
            serde_json::json!(test_files.len()),
        );

        Ok(TaskResult {
            success: true,
            output: llm_response.to_string(),
            error: None,
            metadata,
        })
    }

    /// Handle test execution task
    fn handle_test_execution(llm_response: &str) -> Result<TaskResult> {
        // Parse the response for test execution results
        let execution_result = Self::parse_test_output(llm_response)?;

        let mut metadata = HashMap::new();
        metadata.insert(
            "total_tests".to_string(),
            serde_json::json!(execution_result.total),
        );
        metadata.insert("passed".to_string(), serde_json::json!(execution_result.passed));
        metadata.insert("failed".to_string(), serde_json::json!(execution_result.failed));
        metadata.insert("skipped".to_string(), serde_json::json!(execution_result.skipped));
        metadata.insert(
            "success_rate".to_string(),
            serde_json::json!((execution_result.passed as f32 / execution_result.total as f32 * 100.0).round()),
        );

        Ok(TaskResult {
            success: execution_result.failed == 0,
            output: llm_response.to_string(),
            error: if execution_result.failed > 0 {
                Some(format!("{} tests failed", execution_result.failed))
            } else {
                None
            },
            metadata,
        })
    }

    /// Extract test files from the LLM response
    pub fn extract_test_files(text: &str) -> Result<Vec<TestFile>> {
        let mut files = Vec::new();
        let re = Regex::new(r"```(\w+)(?::([^\n]+))?\n([\s\S]*?)```").unwrap();

        for cap in re.captures_iter(text) {
            let language = cap.get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "text".to_string());

            let file_path = cap.get(2)
                .map(|m| m.as_str().trim().to_string());

            let code = cap.get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            // Only include files that look like test files
            if let Some(path) = &file_path {
                if Self::is_test_file(path, &language, &code) {
                    files.push(TestFile {
                        file_path: PathBuf::from(path),
                        content: code,
                        target_file: None,
                    });
                }
            }
        }

        Ok(files)
    }

    /// Check if a file appears to be a test file
    fn is_test_file(path: &str, language: &str, content: &str) -> bool {
        let path_lower = path.to_lowercase();
        
        // Check filename patterns
        let test_patterns = ["test", "spec", "_test", "_spec"];
        let is_test_named = test_patterns.iter().any(|p| path_lower.contains(p));

        // Check content for test indicators
        let test_indicators: &[&str] = match language {
            "rust" => &["#[test]", "#[cfg(test)]", "mod tests", "assert!"],
            "python" => &["def test_", "unittest", "pytest", "assert "],
            "javascript" | "typescript" => &["describe(", "it(", "test(", "expect(", "assert."],
            "java" => &["@Test", "JUnit", "assert", "Mockito"],
            "go" => &["func Test", "testing.T", "t.Run"],
            _ => &["test", "assert", "expect", "should"],
        };

        let content_lower = content.to_lowercase();
        let has_test_content = test_indicators.iter().any(|i: &&str| content_lower.contains(i.to_lowercase().as_str()));

        is_test_named || has_test_content
    }

    /// Parse test output to extract results
    pub fn parse_test_output(output: &str) -> Result<TestExecutionResult> {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut test_results = Vec::new();

        // Try to parse common test output formats

        // Rust cargo test format: "test result: ok. 5 passed; 0 failed; 0 ignored"
        let rust_re = Regex::new(r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored").unwrap();
        if let Some(cap) = rust_re.captures(output) {
            let _success = cap.get(1).map(|m| m.as_str()) == Some("ok");
            passed = cap.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            failed = cap.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            skipped = cap.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            total = passed + failed + skipped;

            return Ok(TestExecutionResult {
                total,
                passed,
                failed,
                skipped,
                total_duration_ms: None,
                test_results,
                stdout: output.to_string(),
                stderr: String::new(),
            });
        }

        // Python pytest format: "=== 5 passed, 1 failed in 0.12s ==="
        let pytest_re = Regex::new(r"=== (\d+) passed, (\d+) failed").unwrap();
        if let Some(cap) = pytest_re.captures(output) {
            passed = cap.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            failed = cap.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            total = passed + failed;

            // Extract duration if present
            let duration_re = Regex::new(r"in ([\d.]+)s").unwrap();
            let duration_ms = duration_re.captures(output)
                .and_then(|cap| cap.get(1))
                .and_then(|m| m.as_str().parse::<f64>().ok())
                .map(|d| (d * 1000.0) as u64);

            return Ok(TestExecutionResult {
                total,
                passed,
                failed,
                skipped: 0,
                total_duration_ms: duration_ms,
                test_results,
                stdout: output.to_string(),
                stderr: String::new(),
            });
        }

        // Generic: look for PASS/FAIL patterns line by line
        let pass_re = Regex::new(r"(?:✓|PASS|passed|ok)\s*:?\s*(.+)").unwrap();
        let fail_re = Regex::new(r"(?:✗|FAIL|failed|FAILED|Error)\s*:?\s*(.+)").unwrap();
        let skip_re = Regex::new(r"(?:○|SKIP|skipped|ignored)\s*:?\s*(.+)").unwrap();

        for line in output.lines() {
            if let Some(cap) = pass_re.captures(line) {
                passed += 1;
                test_results.push(TestCaseResult {
                    name: cap.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default(),
                    passed: true,
                    duration_ms: None,
                    error: None,
                    test_type: TestType::Unit,
                });
            } else if let Some(cap) = fail_re.captures(line) {
                failed += 1;
                test_results.push(TestCaseResult {
                    name: cap.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default(),
                    passed: false,
                    duration_ms: None,
                    error: Some(line.to_string()),
                    test_type: TestType::Unit,
                });
            } else if let Some(cap) = skip_re.captures(line) {
                skipped += 1;
                test_results.push(TestCaseResult {
                    name: cap.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default(),
                    passed: false,
                    duration_ms: None,
                    error: None,
                    test_type: TestType::Unit,
                });
            }
        }

        if total == 0 {
            total = passed + failed + skipped;
        }

        Ok(TestExecutionResult {
            total,
            passed,
            failed,
            skipped,
            total_duration_ms: None,
            test_results,
            stdout: output.to_string(),
            stderr: String::new(),
        })
    }

    /// Apply test files to disk
    pub fn apply_test_files(test_files: &[TestFile]) -> Result<Vec<String>> {
        let mut applied_files = Vec::new();

        for test_file in test_files {
            // Create parent directories if needed
            if let Some(parent) = test_file.file_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directories for {:?}", test_file.file_path))?;
            }

            fs::write(&test_file.file_path, &test_file.content)
                .with_context(|| format!("Failed to write test file {:?}", test_file.file_path))?;

            applied_files.push(test_file.file_path.to_string_lossy().to_string());
        }

        Ok(applied_files)
    }

    /// Run tests using the appropriate command for the project type
    pub fn run_tests(project_root: &Path, test_command: Option<&str>) -> Result<TestExecutionResult> {
        let (cmd, args) = if let Some(custom) = test_command {
            // Parse custom command
            let mut parts = custom.split_whitespace();
            let cmd = parts.next().unwrap_or("cargo");
            let args: Vec<&str> = parts.collect();
            (cmd, args)
        } else {
            // Auto-detect project type
            if project_root.join("Cargo.toml").exists() {
                ("cargo", vec!["test"])
            } else if project_root.join("package.json").exists() {
                ("npm", vec!["test"])
            } else if project_root.join("pytest.ini").exists() || project_root.join("setup.py").exists() {
                ("pytest", vec![])
            } else if project_root.join("go.mod").exists() {
                ("go", vec!["test", "./..."])
            } else {
                return Err(anyhow!("Could not detect project type. Specify a test command."));
            }
        };

        let output = Command::new(cmd)
            .args(&args)
            .current_dir(project_root)
            .output()
            .with_context(|| format!("Failed to run test command: {} {}", cmd, args.join(" ")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Combine outputs for parsing
        let combined = format!("{}\n{}", stdout, stderr);

        let mut result = Self::parse_test_output(&combined)?;
        result.stdout = stdout;
        result.stderr = stderr;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_test_files_rust() {
        let text = r#"Here are the tests:
```rust:tests/test_calculator.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_add() {
        assert_eq!(2 + 2, 4);
    }
}
```"#;

        let files = TesterAgent::extract_test_files(text).unwrap();
        
        assert_eq!(files.len(), 1);
        assert!(files[0].file_path.ends_with("test_calculator.rs"));
        assert!(files[0].content.contains("#[test]"));
    }

    #[test]
    fn test_extract_test_files_python() {
        let text = r#"```python:test_math.py
def test_add():
    assert 2 + 2 == 4

def test_subtract():
    assert 5 - 3 == 2
```"#;

        let files = TesterAgent::extract_test_files(text).unwrap();
        
        assert_eq!(files.len(), 1);
        assert!(files[0].content.contains("def test_"));
    }

    #[test]
    fn test_is_test_file() {
        assert!(TesterAgent::is_test_file("test_module.rs", "rust", "#[test]"));
        assert!(TesterAgent::is_test_file("module_test.py", "python", "def test_"));
        assert!(TesterAgent::is_test_file("spec.js", "javascript", "describe("));
        assert!(!TesterAgent::is_test_file("main.rs", "rust", "fn main()"));
    }

    #[test]
    fn test_parse_rust_test_output() {
        let output = r#"running 5 tests
test tests::test_add ... ok
test tests::test_subtract ... ok
test tests::test_multiply ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured"#;

        let result = TesterAgent::parse_test_output(output).unwrap();
        
        assert_eq!(result.total, 5);
        assert_eq!(result.passed, 5);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn test_parse_pytest_output() {
        let output = "===================== 10 passed, 2 failed in 1.23s =====================";

        let result = TesterAgent::parse_test_output(output).unwrap();
        
        assert_eq!(result.total, 12);
        assert_eq!(result.passed, 10);
        assert_eq!(result.failed, 2);
        assert_eq!(result.total_duration_ms, Some(1230));
    }

    #[test]
    fn test_parse_generic_test_output() {
        let output = r#"✓ test_add
✓ test_subtract
✗ test_multiply - assertion failed
○ test_divide - skipped"#;

        let result = TesterAgent::parse_test_output(output).unwrap();
        
        assert_eq!(result.passed, 2);
        assert_eq!(result.failed, 1);
        assert_eq!(result.skipped, 1);
    }

    #[test]
    fn test_process_test_generation_task() {
        let task = Task::new("Write unit tests", TaskType::TestGeneration);
        let response = r#"```rust:tests/test_lib.rs
#[test]
fn test_function() {
    assert!(true);
}
```"#;

        let agent = TesterAgent;
        let result = agent.process_task(&task, response).unwrap();

        assert!(result.success);
        assert_eq!(result.metadata.get("test_count").unwrap(), &serde_json::json!(1));
    }

    #[test]
    fn test_process_test_execution_task_success() {
        let task = Task::new("Run tests", TaskType::TestExecution);
        let response = "test result: ok. 10 passed; 0 failed; 0 ignored";

        let agent = TesterAgent;
        let result = agent.process_task(&task, response).unwrap();

        assert!(result.success);
        assert_eq!(result.metadata.get("passed").unwrap(), &serde_json::json!(10));
    }

    #[test]
    fn test_process_test_execution_task_failure() {
        let task = Task::new("Run tests", TaskType::TestExecution);
        let response = "test result: FAILED. 8 passed; 2 failed; 0 ignored";

        let agent = TesterAgent;
        let result = agent.process_task(&task, response).unwrap();

        assert!(!result.success);
        assert_eq!(result.metadata.get("failed").unwrap(), &serde_json::json!(2));
        assert!(result.error.is_some());
    }

    #[test]
    fn test_process_unsupported_task_type() {
        let task = Task::new("Write code", TaskType::CodeGeneration);
        let response = "Some response";

        let agent = TesterAgent;
        let result = agent.process_task(&task, response);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_test_files() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = TestFile {
            file_path: temp_dir.path().join("tests/test_module.rs"),
            content: "#[test]\nfn test_it() { assert!(true); }".to_string(),
            target_file: None,
        };

        let applied = TesterAgent::apply_test_files(&[test_file]).unwrap();
        
        assert_eq!(applied.len(), 1);
        assert!(temp_dir.path().join("tests/test_module.rs").exists());
    }
}
