pub mod coder;
pub mod explorer;
pub mod integrator;
pub mod planner;
pub mod reviewer;
pub mod tester;

#[allow(unused_imports)]
pub use coder::{CodeBlock, CodeChange, CoderAgent, FileOperation};
pub use explorer::ExplorerAgent;
pub use integrator::IntegratorAgent;
pub use planner::PlannerAgent;
pub use reviewer::ReviewerAgent;
pub use tester::TesterAgent;

use serde::{Deserialize, Serialize};

/// Initialize all built-in agents
pub fn initialize_default_agents(registry: &mut crate::agent::AgentRegistry) {
    registry.register(PlannerAgent::create());
    registry.register(CoderAgent::create());
    registry.register(ReviewerAgent::create());
    registry.register(TesterAgent::create());
    registry.register(ExplorerAgent::create());
    registry.register(IntegratorAgent::create());
}

/// Review comment from ReviewerAgent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewComment {
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
    pub severity: ReviewSeverity,
    pub message: String,
    pub snippet: Option<String>,
}

/// Severity level of a review comment
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewSeverity {
    Critical,
    Major,
    Minor,
    Style,
    Security,
}

impl ReviewSeverity {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "critical" | "blocker" | "error" => ReviewSeverity::Critical,
            "major" | "warning" => ReviewSeverity::Major,
            "minor" | "info" => ReviewSeverity::Minor,
            "style" | "lint" => ReviewSeverity::Style,
            "security" | "vulnerability" => ReviewSeverity::Security,
            _ => ReviewSeverity::Minor,
        }
    }
}

/// Review result with scores and comments
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewResult {
    pub quality_score: u8,
    pub security_score: u8,
    pub maintainability_score: u8,
    pub comments: Vec<ReviewComment>,
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TaskType;
    use crate::shared::SharedMemory;
    use std::sync::Arc;

    #[test]
    fn test_reviewer_parse_scores() {
        let response = "Review complete.\n\nScores:\nQuality: 85\nSecurity: 90\nMaintainability: 80\n---";
        let result = ReviewerAgent::parse_review_response(response).unwrap();
        assert_eq!(result.quality_score, 85);
        assert_eq!(result.security_score, 90);
        assert_eq!(result.maintainability_score, 80);
    }

    #[test]
    fn test_reviewer_parse_comments() {
        let response = "Issues found:\n* [Critical] in src/main.rs:10: Null pointer potential\n* [Style] formatting is off";
        let result = ReviewerAgent::parse_review_response(response).unwrap();
        assert_eq!(result.comments.len(), 2);
        assert_eq!(result.comments[0].severity, ReviewSeverity::Critical);
        assert_eq!(result.comments[0].file_path, Some("src/main.rs".to_string()));
        assert_eq!(result.comments[0].line_number, Some(10));
        assert_eq!(result.comments[1].severity, ReviewSeverity::Style);
    }

    #[test]
    fn test_planner_extract_plan() {
        let response = "Plan:\n```json\n{\"subtasks\": [{\"description\": \"test\", \"agent\": \"coder\"}]}\n```";
        let agent = PlannerAgent;
        let task = Task::new("desc", TaskType::Planning);
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).unwrap();
        assert!(result.metadata.contains_key("plan"));
        assert_eq!(result.metadata["has_structured_plan"], serde_json::json!(true));
    }

    #[test]
    fn test_tester_extract_results() {
        let response = "Results:\n* [PASS] test_1\n* [FAIL] test_2\n```rust\nfn test() {}\n```";
        let agent = TesterAgent;
        let task = Task::new("desc", TaskType::TestExecution);
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).unwrap();
        assert_eq!(result.metadata["passed_count"], serde_json::json!(1));
        assert_eq!(result.metadata["failed_count"], serde_json::json!(1));
        assert_eq!(result.metadata["generated_tests_count"], serde_json::json!(1));
        assert!(!result.success);
    }

    #[test]
    fn test_explorer_extract_info() {
        let response = "Found:\n* File: src/lib.rs\n* Symbol: my_func";
        let agent = ExplorerAgent;
        let task = Task::new("desc", TaskType::Exploration);
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).unwrap();
        assert_eq!(result.metadata["discovered_files"], serde_json::json!(vec!["src/lib.rs"]));
        assert_eq!(result.metadata["discovered_symbols"], serde_json::json!(vec!["my_func"]));
    }
}
