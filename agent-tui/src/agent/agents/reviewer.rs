//! Reviewer agent implementation
//!
//! This module provides the ReviewerAgent implementation for code review and analysis.

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::agent::TaskProcessor;
use crate::shared::SharedMemory;
use crate::types::{Agent, AgentRole, Capability, Task, TaskResult, TaskType};

use async_trait::async_trait;

/// Severity level for a code review issue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    /// Critical: bug, security vulnerability, data loss
    Critical,
    /// Major: logic error, performance issue
    Major,
    /// Minor: code style, naming, documentation
    Minor,
    /// Suggestion: optional improvement
    Suggestion,
}

impl IssueSeverity {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueSeverity::Critical => "critical",
            IssueSeverity::Major => "major",
            IssueSeverity::Minor => "minor",
            IssueSeverity::Suggestion => "suggestion",
        }
    }
}

/// A single code review issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewIssue {
    /// Severity of the issue
    pub severity: IssueSeverity,
    /// Category (e.g., "bug", "security", "performance", "style")
    pub category: String,
    /// Line number if applicable
    pub line: Option<usize>,
    /// Description of the issue
    pub description: String,
    /// Suggested fix or improvement
    pub suggestion: Option<String>,
}

/// Result of a code review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    /// Overall quality score (0-10)
    pub quality_score: u8,
    /// List of identified issues
    pub issues: Vec<ReviewIssue>,
    /// Summary of findings
    pub summary: String,
    /// Files reviewed
    pub files: Vec<String>,
}

/// Reviewer agent for code review and analysis
pub struct ReviewerAgent;

#[async_trait]
impl TaskProcessor for ReviewerAgent {
    async fn process_task(
        &self,
        task: &Task,
        response: &str,
        _shared_memory: Arc<SharedMemory>,
    ) -> Result<TaskResult> {
        Self::process_task_internal(task, response)
    }
}

impl ReviewerAgent {
    /// Create a new ReviewerAgent
    pub fn create() -> Agent {
        Agent::new("reviewer", AgentRole::Reviewer, "gpt-4o")
            .with_description("Reviews code for quality, security, and best practices")
            .with_capabilities(vec![
                Capability::Review,
                Capability::Debug,
                Capability::Optimize,
            ])
            .with_system_prompt(
                "You are an expert code reviewer. Your job is to:\n\
                1. Identify bugs, security vulnerabilities, and logic errors\n\
                2. Check for adherence to best practices and coding standards\n\
                3. Evaluate error handling and edge cases\n\
                4. Assess code clarity, maintainability, and documentation\n\
                5. Suggest performance improvements where applicable\n\n\
                When reviewing, provide:\n\
                - A summary of your overall assessment\n\
                - Specific issues with severity levels (critical/major/minor/suggestion)\n\
                - Line numbers where applicable\n\
                - Concrete suggestions for fixes\n\n\
                Format your response as:\n\
                ## Summary\n\
                [Your overall assessment]\n\n\
                ## Issues\n\
                ### [Severity] Category\n\
                - **Line X**: Issue description\n\
                  Suggestion: [fix]\n\n\
                ## Quality Score: X/10",
            )
    }

    /// Process a task (code review or analysis)
    fn process_task_internal(task: &Task, llm_response: &str) -> Result<TaskResult> {
        match task.task_type {
            TaskType::CodeReview => Self::handle_code_review(llm_response),
            TaskType::General => Self::handle_analysis(llm_response),
            _ => Err(anyhow!(
                "Unsupported task type: {:?}. ReviewerAgent only supports CodeReview and General analysis",
                task.task_type
            )),
        }
    }

    /// Handle code review task
    fn handle_code_review(llm_response: &str) -> Result<TaskResult> {
        let review = Self::parse_review_response(llm_response)?;

        let mut metadata = HashMap::new();
        metadata.insert(
            "quality_score".to_string(),
            serde_json::json!(review.quality_score),
        );
        metadata.insert(
            "issue_count".to_string(),
            serde_json::json!(review.issues.len()),
        );
        metadata.insert("files".to_string(), serde_json::json!(review.files));

        // Count issues by severity
        let critical_count = review
            .issues
            .iter()
            .filter(|i| i.severity == IssueSeverity::Critical)
            .count();
        let major_count = review
            .issues
            .iter()
            .filter(|i| i.severity == IssueSeverity::Major)
            .count();
        let minor_count = review
            .issues
            .iter()
            .filter(|i| i.severity == IssueSeverity::Minor)
            .count();
        let suggestion_count = review
            .issues
            .iter()
            .filter(|i| i.severity == IssueSeverity::Suggestion)
            .count();

        metadata.insert(
            "critical_issues".to_string(),
            serde_json::json!(critical_count),
        );
        metadata.insert("major_issues".to_string(), serde_json::json!(major_count));
        metadata.insert("minor_issues".to_string(), serde_json::json!(minor_count));
        metadata.insert(
            "suggestions".to_string(),
            serde_json::json!(suggestion_count),
        );

        Ok(TaskResult {
            success: true,
            output: llm_response.to_string(),
            error: None,
            metadata,
        })
    }

    /// Handle general analysis task
    fn handle_analysis(llm_response: &str) -> Result<TaskResult> {
        Ok(TaskResult {
            success: true,
            output: llm_response.to_string(),
            error: None,
            metadata: HashMap::new(),
        })
    }

    /// Parse the LLM response to extract review information
    fn parse_review_response(response: &str) -> Result<ReviewResult> {
        let mut issues = Vec::new();
        let mut files = Vec::new();
        let mut quality_score = 5; // Default score
        let mut summary = String::new();

        // Extract quality score
        let score_re = Regex::new(r"Quality Score:\s*(\d+)/10").unwrap();
        if let Some(cap) = score_re.captures(response) {
            if let Some(score) = cap.get(1).and_then(|m| m.as_str().parse::<u8>().ok()) {
                quality_score = score.min(10);
            }
        }

        // Extract summary
        if let Some(summary_start) = response.find("## Summary") {
            let summary_end = response.find("## Issues").unwrap_or(response.len());
            summary = response[summary_start + 11..summary_end].trim().to_string();
        }

        // Extract file references (look for patterns like "in file.rs" or "file.rs:")
        let file_re = Regex::new(r"[\s`(]([a-zA-Z0-9_\-./]+\.(?:rs|py|js|ts|jsx|tsx|java|cpp|c|go|rb|php|cs|swift|kt|scala|sh|yaml|yml|json|toml|xml|html|css|md))[\s`:)]").unwrap();
        for cap in file_re.captures_iter(response) {
            if let Some(file) = cap.get(1) {
                let file_path = file.as_str().to_string();
                if !files.contains(&file_path) {
                    files.push(file_path);
                }
            }
        }

        // Parse issues - look for severity markers
        let severity_patterns = [
            ("Critical", IssueSeverity::Critical),
            ("critical", IssueSeverity::Critical),
            ("CRITICAL", IssueSeverity::Critical),
            ("Major", IssueSeverity::Major),
            ("major", IssueSeverity::Major),
            ("MAJOR", IssueSeverity::Major),
            ("Minor", IssueSeverity::Minor),
            ("minor", IssueSeverity::Minor),
            ("MINOR", IssueSeverity::Minor),
            ("Suggestion", IssueSeverity::Suggestion),
            ("suggestion", IssueSeverity::Suggestion),
            ("SUGGESTION", IssueSeverity::Suggestion),
        ];

        // Simple line-by-line parsing for issues
        for line in response.lines() {
            let line_trimmed = line.trim();

            // Look for line number patterns
            let line_re = Regex::new(r"Line\s*(\d+)").unwrap();
            let line_num = line_re
                .captures(line_trimmed)
                .and_then(|cap| cap.get(1))
                .and_then(|m| m.as_str().parse::<usize>().ok());

            for (pattern, severity) in &severity_patterns {
                if line_trimmed.contains(pattern) {
                    // Extract category if present (word after severity)
                    let category = if let Some(idx) = line_trimmed.find(pattern) {
                        let after = &line_trimmed[idx + pattern.len()..].trim();
                        after
                            .split_whitespace()
                            .next()
                            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()))
                            .filter(|s| !s.is_empty())
                            .unwrap_or("general")
                            .to_string()
                    } else {
                        "general".to_string()
                    };

                    // Extract description (rest of line or next line)
                    let description = line_trimmed.to_string();

                    issues.push(ReviewIssue {
                        severity: *severity,
                        category,
                        line: line_num,
                        description,
                        suggestion: None,
                    });
                    break;
                }
            }
        }

        // If no issues found but response exists, create a positive review
        if issues.is_empty() && !response.trim().is_empty() {
            issues.push(ReviewIssue {
                severity: IssueSeverity::Suggestion,
                category: "general".to_string(),
                line: None,
                description: "No significant issues found".to_string(),
                suggestion: Some("Code appears to follow good practices".to_string()),
            });
        }

        if summary.is_empty() {
            summary = "Code review completed".to_string();
        }

        Ok(ReviewResult {
            quality_score,
            issues,
            summary,
            files,
        })
    }

    /// Extract code blocks from the response for review
    #[allow(dead_code)]
    pub fn extract_code_for_review(text: &str) -> Result<Vec<(String, String)>> {
        let mut files = Vec::new();
        let re = Regex::new(r"```(\w+)?(?::([^\n]+))?\n([\s\S]*?)```").unwrap();

        for cap in re.captures_iter(text) {
            let file_path = cap
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let code = cap
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            files.push((file_path, code));
        }

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::SharedMemory;
    use std::sync::Arc;

    #[test]
    fn test_parse_review_with_issues() {
        let response = r#"## Summary
Good code overall with minor issues.

## Issues
### Critical Security
- **Line 15**: Potential SQL injection vulnerability
  Suggestion: Use parameterized queries

### Major Performance
- **Line 42**: Inefficient loop inside loop
  Suggestion: Use a HashMap for O(1) lookup

## Quality Score: 7/10"#;

        let review = ReviewerAgent::parse_review_response(response).unwrap();

        assert_eq!(review.quality_score, 7);
        assert!(!review.issues.is_empty());
        assert!(review.summary.contains("Good code"));
    }

    #[test]
    fn test_parse_review_no_issues() {
        let response = r#"## Summary
Excellent code! No issues found.

## Quality Score: 10/10"#;

        let review = ReviewerAgent::parse_review_response(response).unwrap();

        assert_eq!(review.quality_score, 10);
        // Should have a placeholder suggestion
        assert!(!review.issues.is_empty());
    }

    #[test]
    fn test_extract_code_for_review() {
        let text = r#"Here's the code:
```rust:src/main.rs
fn main() {
    println!("Hello");
}
```
And another file:
```python:utils.py
def helper():
    pass
```"#;

        let files = ReviewerAgent::extract_code_for_review(text).unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "src/main.rs");
        assert!(files[0].1.contains("fn main()"));
        assert_eq!(files[1].0, "utils.py");
    }

    #[tokio::test]
    async fn test_process_code_review_task() {
        let task = Task::new("Review this code", TaskType::CodeReview);
        let response = r#"## Summary
Code looks good.

## Quality Score: 8/10"#;

        let agent = ReviewerAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).await.unwrap();

        assert!(result.success);
        assert_eq!(
            result.metadata.get("quality_score").unwrap(),
            &serde_json::json!(8)
        );
    }

    #[tokio::test]
    async fn test_process_unsupported_task_type() {
        let task = Task::new("Write code", TaskType::CodeGeneration);
        let response = "Some response";

        let agent = ReviewerAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported task type"));
    }
}
