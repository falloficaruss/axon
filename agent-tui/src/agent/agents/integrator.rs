//! Integrator agent implementation
//!
//! This module provides the IntegratorAgent implementation for synthesizing results from multiple agents.

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::types::{Agent, AgentRole, Capability, Task, TaskResult, TaskType};
use crate::agent::TaskProcessor;
use crate::shared::SharedMemory;

/// A single result from an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    /// Agent name or ID
    pub agent: String,
    /// Task description
    pub task: String,
    /// Whether the task succeeded
    pub success: bool,
    /// Output content
    pub output: String,
    /// Error message if failed
    pub error: Option<String>,
}

/// Synthesized result from integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    /// Combined output
    pub combined_output: String,
    /// Summary of the integration
    pub summary: String,
    /// List of successful contributions
    pub successful_contributions: Vec<String>,
    /// List of failed or partial contributions
    pub failed_contributions: Vec<String>,
    /// Confidence score (0-100)
    pub confidence: u8,
    /// Recommendations for follow-up
    pub recommendations: Vec<String>,
}

/// Integrator agent for result synthesis
pub struct IntegratorAgent;

impl TaskProcessor for IntegratorAgent {
    fn process_task(&self, task: &Task, response: &str, _shared_memory: Arc<SharedMemory>) -> Result<TaskResult> {
        Self::process_task_internal(task, response)
    }
}

impl IntegratorAgent {
    /// Create a new IntegratorAgent
    pub fn create() -> Agent {
        Agent::new("integrator", AgentRole::Integrator, "gpt-4o")
            .with_description("Synthesizes results from multiple agents into cohesive deliverables")
            .with_capabilities(vec![
                Capability::Document,
                Capability::Review,
                Capability::Plan,
            ])
            .with_system_prompt(
                "You are a results integrator and synthesizer. Your job is to:\n\
                1. Combine outputs from multiple agents into a cohesive whole\n\
                2. Resolve conflicts between different approaches or suggestions\n\
                3. Create well-structured final deliverables\n\
                4. Ensure consistency across all components\n\
                5. Provide clear summaries and documentation\n\n\
                When integrating:\n\
                - Prioritize correctness over style\n\
                - Resolve conflicts by choosing the most defensible approach\n\
                - Maintain consistent terminology and formatting\n\
                - Highlight any unresolved issues or trade-offs\n\
                - Provide actionable recommendations\n\n\
                Format your response as:\n\
                ## Summary\n\
                [High-level overview of what was accomplished]\n\n\
                ## Integrated Result\n\
                [The combined, cohesive output]\n\n\
                ## Contributions\n\
                - [Agent]: [What they contributed]\n\n\
                ## Quality Assessment\n\
                [Confidence level and any concerns]\n\n\
                ## Recommendations\n\
                [Next steps or follow-up actions]"
            )
    }

    /// Process a task (synthesis or integration)
    fn process_task_internal(task: &Task, llm_response: &str) -> Result<TaskResult> {
        match task.task_type {
            TaskType::Synthesis => Self::handle_synthesis(task, llm_response),
            TaskType::General => Self::handle_general(task, llm_response),
            _ => Err(anyhow!(
                "Unsupported task type: {:?}. IntegratorAgent only supports Synthesis and General tasks",
                task.task_type
            )),
        }
    }

    /// Handle synthesis task
    fn handle_synthesis(task: &Task, llm_response: &str) -> Result<TaskResult> {
        let synthesis = Self::parse_synthesis_response(task, llm_response)?;

        let mut metadata = HashMap::new();
        metadata.insert(
            "confidence".to_string(),
            serde_json::json!(synthesis.confidence),
        );
        metadata.insert(
            "summary".to_string(),
            serde_json::json!(synthesis.summary),
        );
        metadata.insert(
            "successful_contributions".to_string(),
            serde_json::json!(synthesis.successful_contributions),
        );
        metadata.insert(
            "failed_contributions".to_string(),
            serde_json::json!(synthesis.failed_contributions),
        );
        metadata.insert(
            "recommendations".to_string(),
            serde_json::json!(synthesis.recommendations),
        );

        Ok(TaskResult {
            success: synthesis.confidence >= 50,
            output: synthesis.combined_output,
            error: if synthesis.confidence < 50 {
                Some(format!("Low confidence synthesis: {}", synthesis.summary))
            } else {
                None
            },
            metadata,
        })
    }

    /// Handle general task
    fn handle_general(_task: &Task, llm_response: &str) -> Result<TaskResult> {
        Ok(TaskResult {
            success: true,
            output: llm_response.to_string(),
            error: None,
            metadata: HashMap::new(),
        })
    }

    /// Parse the LLM response to extract synthesis information
    fn parse_synthesis_response(task: &Task, response: &str) -> Result<SynthesisResult> {
        let mut summary = String::new();
        let mut combined_output = String::new();
        let mut confidence = 70;
        let mut successful_contributions = Vec::new();
        let failed_contributions = Vec::new();
        let mut recommendations = Vec::new();

        // Extract Summary section
        if let Some(section) = Self::extract_section(response, "Summary") {
            summary = section.trim().to_string();
        }

        // Extract Integrated Result section
        if let Some(section) = Self::extract_section(response, "Integrated Result") {
            combined_output = section.trim().to_string();
        }

        // If no Integrated Result, use the full response
        if combined_output.is_empty() {
            combined_output = response.to_string();
        }

        // Extract Contributions section
        if let Some(section) = Self::extract_section(response, "Contributions") {
            for line in section.lines() {
                let line = line.trim();
                if line.starts_with('-') || line.starts_with('*') {
                    let content = line.trim_start_matches(|c| c == '-' || c == '*').trim();
                    if !content.is_empty() {
                        successful_contributions.push(content.to_string());
                    }
                }
            }
        }

        // Extract Quality Assessment section for confidence
        if let Some(section) = Self::extract_section(response, "Quality Assessment") {
            let num_re = regex::Regex::new(r"(\d+)%?").unwrap();
            if let Some(cap) = num_re.captures(&section) {
                if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse::<u8>().ok()) {
                    confidence = n.min(100);
                }
            }
            
            // Look for confidence indicators
            let section_lower = section.to_lowercase();
            if section_lower.contains("high confidence") || section_lower.contains("confident") {
                confidence = confidence.max(80);
            } else if section_lower.contains("low confidence") || section_lower.contains("uncertain") {
                confidence = confidence.min(40);
            }
        }

        // Extract Recommendations section
        if let Some(section) = Self::extract_section(response, "Recommendations") {
            for line in section.lines() {
                let line = line.trim();
                if line.starts_with('-') || line.starts_with('*') || line.starts_with(|c: char| c.is_numeric()) {
                    let content = line.trim_start_matches(|c: char| c == '-' || c == '*' || c.is_numeric() || c == '.').trim();
                    if !content.is_empty() {
                        recommendations.push(content.to_string());
                    }
                }
            }
        }

        // If no summary extracted, create one from the task
        if summary.is_empty() {
            summary = format!("Integrated result for: {}", task.description);
        }

        // If no contributions found, add a generic one
        if successful_contributions.is_empty() {
            successful_contributions.push("Integration completed".to_string());
        }

        Ok(SynthesisResult {
            combined_output,
            summary,
            successful_contributions,
            failed_contributions,
            confidence,
            recommendations,
        })
    }

    /// Extract a section from the response by header
    fn extract_section(response: &str, header: &str) -> Option<String> {
        let header_pattern = format!("## {}", header);
        if let Some(start) = response.find(&header_pattern) {
            let content_start = start + header_pattern.len();
            let remaining = &response[content_start..];
            // Find the next section header or end of string
            if let Some(end) = remaining.find("\n## ") {
                return Some(remaining[..end].trim().to_string());
            }
            return Some(remaining.trim().to_string());
        }
        None
    }

    /// Combine multiple task results into a single input for the integrator
    pub fn combine_results(results: &[AgentResult]) -> String {
        let mut combined = String::new();
        
        combined.push_str("# Agent Results Summary\n\n");
        
        // Group by success/failure
        let successful: Vec<&AgentResult> = results.iter().filter(|r| r.success).collect();
        let failed: Vec<&AgentResult> = results.iter().filter(|r| !r.success).collect();

        if !successful.is_empty() {
            combined.push_str("## Successful Results\n\n");
            for result in &successful {
                combined.push_str(&format!("### {} - {}\n\n", result.agent, result.task));
                combined.push_str(&result.output);
                combined.push_str("\n\n---\n\n");
            }
        }

        if !failed.is_empty() {
            combined.push_str("## Failed/Partial Results\n\n");
            for result in &failed {
                combined.push_str(&format!("### {} - {}\n\n", result.agent, result.task));
                if let Some(error) = &result.error {
                    combined.push_str(&format!("**Error**: {}\n\n", error));
                }
                if !result.output.is_empty() {
                    combined.push_str(&result.output);
                    combined.push_str("\n\n");
                }
                combined.push_str("---\n\n");
            }
        }

        combined
    }

    /// Create a structured summary from multiple results
    pub fn create_summary(results: &[AgentResult]) -> String {
        let mut summary = String::new();
        
        summary.push_str(&format!("## Overview\n\n"));
        summary.push_str(&format!("Total agents: {}\n", results.len()));
        summary.push_str(&format!("Successful: {}\n", results.iter().filter(|r| r.success).count()));
        summary.push_str(&format!("Failed: {}\n\n", results.iter().filter(|r| !r.success).count()));

        // Group by agent type
        let mut by_agent: HashMap<String, Vec<&AgentResult>> = HashMap::new();
        for result in results {
            by_agent.entry(result.agent.clone()).or_default().push(result);
        }

        summary.push_str("## By Agent\n\n");
        for (agent, agent_results) in &by_agent {
            summary.push_str(&format!("### {}\n", agent));
            for result in agent_results {
                let status = if result.success { "✓" } else { "✗" };
                summary.push_str(&format!("- {} {}\n", status, result.task));
            }
            summary.push('\n');
        }

        summary
    }

    /// Detect and report conflicts between results
    pub fn detect_conflicts(results: &[AgentResult]) -> Vec<String> {
        let mut conflicts = Vec::new();
        
        // Simple conflict detection: look for contradictory statements
        let contradiction_pairs = [
            ("should", "should not"),
            ("must", "must not"),
            ("always", "never"),
            ("required", "optional"),
            ("correct", "incorrect"),
            ("error", "correct"),
        ];

        let outputs: Vec<&str> = results.iter().map(|r| r.output.as_str()).collect();
        
        for (i, output1) in outputs.iter().enumerate() {
            for output2 in outputs.iter().skip(i + 1) {
                let lower1 = output1.to_lowercase();
                let lower2 = output2.to_lowercase();
                
                for (pos, neg) in &contradiction_pairs {
                    if lower1.contains(pos) && lower2.contains(neg) {
                        conflicts.push(format!(
                            "Potential conflict: '{}' vs '{}' on '{}'",
                            results[i].task, results[outputs.iter().position(|&o| o == *output2).unwrap()].task, pos
                        ));
                    }
                }
            }
        }

        conflicts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_section() {
        let response = r#"## Summary
This is the summary.

## Integrated Result
The combined output.

## Recommendations
- Do this
- Do that"#;

        assert_eq!(
            IntegratorAgent::extract_section(response, "Summary"),
            Some("This is the summary.".to_string())
        );
        assert_eq!(
            IntegratorAgent::extract_section(response, "Integrated Result"),
            Some("The combined output.".to_string())
        );
    }

    #[test]
    fn test_parse_synthesis_response() {
        let task = Task::new("Integrate results", TaskType::Synthesis);
        let response = r#"## Summary
Successfully integrated all agent outputs.

## Integrated Result
Final combined result here.

## Quality Assessment
High confidence (85%)

## Recommendations
- Run tests to verify
- Review the code changes"#;

        let result = IntegratorAgent::parse_synthesis_response(&task, response).unwrap();
        
        assert!(result.summary.contains("Successfully integrated"));
        assert_eq!(result.combined_output, "Final combined result here.");
        assert!(result.confidence >= 80);
        assert_eq!(result.recommendations.len(), 2);
    }

    #[test]
    fn test_combine_results() {
        let results = vec![
            AgentResult {
                agent: "coder".to_string(),
                task: "Write module".to_string(),
                success: true,
                output: "Module code here".to_string(),
                error: None,
            },
            AgentResult {
                agent: "tester".to_string(),
                task: "Write tests".to_string(),
                success: true,
                output: "Test code here".to_string(),
                error: None,
            },
            AgentResult {
                agent: "reviewer".to_string(),
                task: "Review code".to_string(),
                success: false,
                output: "Partial review".to_string(),
                error: Some("Timeout".to_string()),
            },
        ];

        let combined = IntegratorAgent::combine_results(&results);
        
        assert!(combined.contains("Successful Results"));
        assert!(combined.contains("Failed/Partial Results"));
        assert!(combined.contains("coder"));
        assert!(combined.contains("tester"));
        assert!(combined.contains("reviewer"));
    }

    #[test]
    fn test_create_summary() {
        let results = vec![
            AgentResult {
                agent: "coder".to_string(),
                task: "Task 1".to_string(),
                success: true,
                output: "Output 1".to_string(),
                error: None,
            },
            AgentResult {
                agent: "coder".to_string(),
                task: "Task 2".to_string(),
                success: false,
                output: "Output 2".to_string(),
                error: Some("Error".to_string()),
            },
        ];

        let summary = IntegratorAgent::create_summary(&results);
        
        assert!(summary.contains("Total agents: 2"));
        assert!(summary.contains("Successful: 1"));
        assert!(summary.contains("Failed: 1"));
        assert!(summary.contains("By Agent"));
    }

    #[test]
    fn test_detect_conflicts() {
        let results = vec![
            AgentResult {
                agent: "reviewer1".to_string(),
                task: "Review A".to_string(),
                success: true,
                output: "This approach should be used".to_string(),
                error: None,
            },
            AgentResult {
                agent: "reviewer2".to_string(),
                task: "Review B".to_string(),
                success: true,
                output: "This approach should not be used".to_string(),
                error: None,
            },
        ];

        let conflicts = IntegratorAgent::detect_conflicts(&results);
        
        assert!(!conflicts.is_empty());
        assert!(conflicts.iter().any(|c| c.contains("Potential conflict")));
    }

    #[test]
    fn test_process_synthesis_task() {
        let task = Task::new("Synthesize results", TaskType::Synthesis);
        let response = r#"## Summary
All results integrated successfully.

## Integrated Result
Combined output.

## Quality Assessment
Confidence: 90%

## Recommendations
- Verify the changes"#;

        let agent = IntegratorAgent;
        let result = agent.process_task(&task, response).unwrap();
        
        assert!(result.success);
        assert_eq!(result.metadata.get("confidence").unwrap(), &serde_json::json!(90));
    }

    #[test]
    fn test_process_low_confidence_synthesis() {
        let task = Task::new("Synthesize results", TaskType::Synthesis);
        let response = r#"## Summary
Integration incomplete.

## Integrated Result
Partial output.

## Quality Assessment
Low confidence: 30%"#;

        let agent = IntegratorAgent;
        let result = agent.process_task(&task, response).unwrap();
        
        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(result.metadata.get("confidence").unwrap(), &serde_json::json!(30));
    }

    #[test]
    fn test_process_unsupported_task_type() {
        let task = Task::new("Write code", TaskType::CodeGeneration);
        let response = "Some response";

        let agent = IntegratorAgent;
        let result = agent.process_task(&task, response);
        assert!(result.is_err());
    }
}
