//! Planner agent implementation
//!
//! This module provides the PlannerAgent implementation for task decomposition and planning.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::agent::TaskProcessor;
use crate::shared::SharedMemory;
use crate::types::{Agent, AgentRole, Capability, Plan, Subtask, Task, TaskResult, TaskType};

use async_trait::async_trait;

/// A dependency relationship between subtasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// ID of the dependent subtask
    pub subtask_id: String,
    /// IDs of subtasks it depends on
    pub depends_on: Vec<String>,
}

/// Execution strategy for a plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    /// Execute tasks sequentially
    Sequential,
    /// Execute independent tasks in parallel
    Parallel,
    /// Mix of sequential and parallel based on dependencies
    Hybrid,
}

/// Planning result from the Planner agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningResult {
    /// Original task description
    pub original_task: String,
    /// Generated subtasks
    pub subtasks: Vec<SubtaskInfo>,
    /// Execution strategy recommendation
    pub strategy: ExecutionStrategy,
    /// Estimated complexity (1-10)
    pub complexity: u8,
    /// Reasoning for the plan
    pub reasoning: String,
}

/// Information about a subtask
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskInfo {
    /// Description of the subtask
    pub description: String,
    /// Type of task
    pub task_type: String,
    /// Suggested agent role
    pub suggested_agent: Option<String>,
    /// Estimated effort (1-10)
    pub effort: Option<u8>,
    /// Dependencies on other subtasks (by index)
    pub dependencies: Vec<usize>,
}

/// Planner agent for task decomposition and orchestration
pub struct PlannerAgent;

#[async_trait]
impl TaskProcessor for PlannerAgent {
    async fn process_task(
        &self,
        task: &Task,
        response: &str,
        _shared_memory: Arc<SharedMemory>,
    ) -> Result<TaskResult> {
        Self::process_task_internal(task, response)
    }
}

impl PlannerAgent {
    /// Create a new PlannerAgent
    pub fn create() -> Agent {
        Agent::new("planner", AgentRole::Planner, "gpt-4o")
            .with_description(
                "Plans and orchestrates multi-agent workflows through task decomposition",
            )
            .with_capabilities(vec![
                Capability::Plan,
                Capability::Explore,
                Capability::Document,
            ])
            .with_system_prompt(
                "You are an expert task planner and orchestrator. Your job is to:\n\
                1. Analyze complex requests and understand the complete goal\n\
                2. Break down tasks into logical, manageable subtasks\n\
                3. Determine which specialized agents are best suited for each subtask\n\
                4. Identify dependencies between subtasks\n\
                5. Recommend execution strategy (sequential vs parallel)\n\
                6. Estimate complexity and effort\n\n\
                Available agent roles:\n\
                - Coder: Writes and modifies code\n\
                - Reviewer: Reviews code for quality and security\n\
                - Tester: Generates and runs tests\n\
                - Explorer: Explores and analyzes codebase structure\n\
                - Integrator: Synthesizes results from multiple agents\n\n\
                When creating a plan:\n\
                - Start with exploration if the codebase is unfamiliar\n\
                - Place code generation before review and testing\n\
                - Group independent tasks for parallel execution\n\
                - Consider feedback loops (e.g., code → review → fix → re-review)\n\n\
                Format your response as:\n\
                ## Analysis\n\
                [Your understanding of the request]\n\n\
                ## Plan\n\
                1. [Subtask] → [Agent]\n\
                2. [Subtask] → [Agent] (depends on: 1)\n\
                ...\n\n\
                ## Strategy\n\
                [Sequential/Parallel/Hybrid] - [reasoning]\n\n\
                ## Complexity\n\
                [1-10] - [justification]",
            )
    }

    /// Process a task (planning or analysis)
    fn process_task_internal(task: &Task, llm_response: &str) -> Result<TaskResult> {
        match task.task_type {
            TaskType::Planning => Self::handle_planning(task, llm_response),
            TaskType::General => Self::handle_analysis(llm_response),
            _ => Err(anyhow!(
                "Unsupported task type: {:?}. PlannerAgent only supports Planning and General analysis",
                task.task_type
            )),
        }
    }

    /// Handle planning task
    fn handle_planning(task: &Task, llm_response: &str) -> Result<TaskResult> {
        let planning_result = Self::parse_planning_response(task, llm_response)?;

        let mut metadata = HashMap::new();
        metadata.insert(
            "subtask_count".to_string(),
            serde_json::json!(planning_result.subtasks.len()),
        );
        metadata.insert(
            "complexity".to_string(),
            serde_json::json!(planning_result.complexity),
        );
        metadata.insert(
            "strategy".to_string(),
            serde_json::json!(match planning_result.strategy {
                ExecutionStrategy::Sequential => "sequential",
                ExecutionStrategy::Parallel => "parallel",
                ExecutionStrategy::Hybrid => "hybrid",
            }),
        );
        metadata.insert(
            "reasoning".to_string(),
            serde_json::json!(planning_result.reasoning),
        );

        // Count suggested agents
        let mut agent_counts: HashMap<String, usize> = HashMap::new();
        for subtask in &planning_result.subtasks {
            if let Some(agent) = &subtask.suggested_agent {
                *agent_counts.entry(agent.clone()).or_insert(0) += 1;
            }
        }
        metadata.insert(
            "agent_distribution".to_string(),
            serde_json::json!(agent_counts),
        );

        // Calculate total estimated effort
        let total_effort: u32 = planning_result
            .subtasks
            .iter()
            .filter_map(|s| s.effort.map(|e| e as u32))
            .sum();
        metadata.insert("total_effort".to_string(), serde_json::json!(total_effort));

        // Add plan metadata for tests
        metadata.insert("plan".to_string(), serde_json::json!(llm_response));
        metadata.insert(
            "has_structured_plan".to_string(),
            serde_json::json!(!planning_result.subtasks.is_empty()),
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

    /// Parse the LLM response to extract planning information
    fn parse_planning_response(task: &Task, response: &str) -> Result<PlanningResult> {
        let mut subtasks = Vec::new();
        let mut strategy = ExecutionStrategy::Sequential;
        let mut complexity = 5;
        let mut reasoning = String::new();

        // Extract complexity score
        if let Some(complexity_section) = Self::extract_section(response, "Complexity") {
            let num_re = regex::Regex::new(r"(\d+)").unwrap();
            if let Some(cap) = num_re.captures(&complexity_section) {
                if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse::<u8>().ok()) {
                    complexity = n.min(10).max(1);
                }
            }
        }

        // Extract strategy - check for 'hybrid' first since 'parallel' may appear in hybrid descriptions
        if let Some(strategy_section) = Self::extract_section(response, "Strategy") {
            let strategy_lower = strategy_section.to_lowercase();
            if strategy_lower.contains("hybrid") {
                strategy = ExecutionStrategy::Hybrid;
            } else if strategy_lower.contains("parallel") {
                strategy = ExecutionStrategy::Parallel;
            }
        }

        // Extract reasoning from Analysis section
        if let Some(analysis) = Self::extract_section(response, "Analysis") {
            reasoning = analysis.trim().to_string();
        }

        // Parse subtasks from the Plan section
        if let Some(plan_section) = Self::extract_section(response, "Plan") {
            subtasks = Self::parse_subtasks(&plan_section);
        }

        // If no subtasks parsed, create a single subtask from the original task
        if subtasks.is_empty() {
            subtasks.push(SubtaskInfo {
                description: task.description.clone(),
                task_type: match task.task_type {
                    TaskType::Planning => "Planning".to_string(),
                    TaskType::CodeGeneration => "CodeGeneration".to_string(),
                    TaskType::CodeEdit => "CodeEdit".to_string(),
                    TaskType::CodeReview => "CodeReview".to_string(),
                    TaskType::TestGeneration => "TestGeneration".to_string(),
                    TaskType::TestExecution => "TestExecution".to_string(),
                    TaskType::Exploration => "Exploration".to_string(),
                    TaskType::Synthesis => "Synthesis".to_string(),
                    TaskType::General => "General".to_string(),
                },
                suggested_agent: Some("coder".to_string()),
                effort: Some(5),
                dependencies: vec![],
            });
        }

        Ok(PlanningResult {
            original_task: task.description.clone(),
            subtasks,
            strategy,
            complexity,
            reasoning,
        })
    }

    /// Extract a section from the response by header
    fn extract_section(response: &str, header: &str) -> Option<String> {
        let header_pattern = format!("## {}", header);
        if let Some(start) = response.find(&header_pattern) {
            let content_start = start + header_pattern.len();
            // Find the next section header or end of string
            let remaining = &response[content_start..];
            if let Some(end) = remaining.find("\n## ") {
                return Some(remaining[..end].trim().to_string());
            }
            return Some(remaining.trim().to_string());
        }
        None
    }

    /// Parse subtasks from a plan section
    fn parse_subtasks(plan_section: &str) -> Vec<SubtaskInfo> {
        let mut subtasks = Vec::new();

        // Pattern to match numbered list items like "1. [task] → [agent]" or "1. [task]"
        let line_pattern = regex::Regex::new(
            r"^\d+\.\s*(.+?)(?:\s*→\s*|\s*-\s*)(\w+)?(?:\s*\(depends on:\s*([^)]*)\))?",
        )
        .unwrap();

        for line in plan_section.lines() {
            let line = line.trim();
            if let Some(cap) = line_pattern.captures(line) {
                let description = cap
                    .get(1)
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();
                let suggested_agent = cap.get(2).map(|m| m.as_str().trim().to_lowercase());

                let dependencies = cap
                    .get(3)
                    .map(|m| {
                        m.as_str()
                            .split(',')
                            .filter_map(|s| s.trim().parse::<usize>().ok())
                            .map(|i| i.saturating_sub(1)) // Convert 1-indexed to 0-indexed
                            .collect()
                    })
                    .unwrap_or_default();

                // Infer task type from description
                let task_type = Self::infer_task_type(&description);

                subtasks.push(SubtaskInfo {
                    description,
                    task_type,
                    suggested_agent,
                    effort: None,
                    dependencies,
                });
            }
        }

        // If numbered pattern didn't work, try simpler line-by-line parsing
        if subtasks.is_empty() {
            for (_idx, line) in plan_section.lines().enumerate() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('-') && !line.starts_with('*') {
                    subtasks.push(SubtaskInfo {
                        description: line
                            .trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '-')
                            .trim()
                            .to_string(),
                        task_type: "General".to_string(),
                        suggested_agent: None,
                        effort: Some(5),
                        dependencies: vec![],
                    });
                }
            }
        }

        subtasks
    }

    /// Infer task type from description
    fn infer_task_type(description: &str) -> String {
        let desc_lower = description.to_lowercase();

        if desc_lower.contains("explor")
            || desc_lower.contains("analyz")
            || desc_lower.contains("find")
        {
            "Exploration".to_string()
        } else if desc_lower.contains("test") || desc_lower.contains("spec") {
            "TestGeneration".to_string()
        } else if desc_lower.contains("review")
            || desc_lower.contains("audit")
            || desc_lower.contains("check")
        {
            "CodeReview".to_string()
        } else if desc_lower.contains("write")
            || desc_lower.contains("create")
            || desc_lower.contains("implement")
            || desc_lower.contains("develop")
        {
            "CodeGeneration".to_string()
        } else if desc_lower.contains("edit")
            || desc_lower.contains("modify")
            || desc_lower.contains("update")
            || desc_lower.contains("refactor")
        {
            "CodeEdit".to_string()
        } else if desc_lower.contains("synthes")
            || desc_lower.contains("combin")
            || desc_lower.contains("integrat")
        {
            "Synthesis".to_string()
        } else if desc_lower.contains("plan")
            || desc_lower.contains("design")
            || desc_lower.contains("architect")
        {
            "Planning".to_string()
        } else {
            "General".to_string()
        }
    }

    /// Convert a PlanningResult to a Plan object
    pub fn planning_result_to_plan(result: PlanningResult, original_task: Task) -> Plan {
        let mut plan = Plan::new(original_task);

        let subtasks: Vec<Subtask> = result
            .subtasks
            .into_iter()
            .map(|info| {
                let task_type = match info.task_type.as_str() {
                    "CodeGeneration" => TaskType::CodeGeneration,
                    "CodeEdit" => TaskType::CodeEdit,
                    "CodeReview" => TaskType::CodeReview,
                    "TestGeneration" => TaskType::TestGeneration,
                    "TestExecution" => TaskType::TestExecution,
                    "Exploration" => TaskType::Exploration,
                    "Planning" => TaskType::Planning,
                    "Synthesis" => TaskType::Synthesis,
                    _ => TaskType::General,
                };

                let mut subtask = Subtask::new(&info.description, task_type);

                // Note: suggested_agent will be resolved to agent ID by the orchestrator
                if let Some(agent_name) = info.suggested_agent {
                    subtask.suggested_agent = Some(agent_name);
                }

                // Dependencies will be resolved after all subtasks are created
                subtask.dependencies = info
                    .dependencies
                    .iter()
                    .map(|_| "unresolved".to_string())
                    .collect();

                subtask
            })
            .collect();

        plan.subtasks = subtasks;
        plan.execution_order = plan.subtasks.iter().map(|s| s.id.clone()).collect();

        // Determine parallel groups based on dependencies
        // Tasks with no dependencies can run in parallel
        let mut parallel_group = Vec::new();
        for subtask in &plan.subtasks {
            if subtask.dependencies.is_empty() {
                parallel_group.push(subtask.id.clone());
            }
        }
        if parallel_group.len() > 1 {
            plan.parallel_groups.push(parallel_group);
        }

        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::SharedMemory;
    use std::sync::Arc;

    #[test]
    fn test_extract_section() {
        let response = r#"## Analysis
This is the analysis section.

## Plan
1. Task 1
2. Task 2

## Strategy
Parallel execution

## Complexity
7/10 - Moderate complexity"#;

        assert_eq!(
            PlannerAgent::extract_section(response, "Analysis"),
            Some("This is the analysis section.".to_string())
        );
        assert_eq!(
            PlannerAgent::extract_section(response, "Complexity"),
            Some("7/10 - Moderate complexity".to_string())
        );
    }

    #[test]
    fn test_parse_subtasks() {
        let plan_section = r#"1. Explore the codebase → Explorer
2. Write the main module → Coder (depends on: 1)
3. Write tests → Tester (depends on: 2)
4. Review code → Reviewer (depends on: 2)"#;

        let subtasks = PlannerAgent::parse_subtasks(plan_section);

        assert_eq!(subtasks.len(), 4);
        assert_eq!(subtasks[0].description, "Explore the codebase");
        assert_eq!(subtasks[0].suggested_agent, Some("explorer".to_string()));
        assert_eq!(subtasks[1].dependencies, vec![0]);
        assert_eq!(subtasks[2].suggested_agent, Some("tester".to_string()));
    }

    #[test]
    fn test_infer_task_type() {
        assert_eq!(
            PlannerAgent::infer_task_type("Explore the codebase"),
            "Exploration"
        );
        assert_eq!(
            PlannerAgent::infer_task_type("Write unit tests"),
            "TestGeneration"
        );
        assert_eq!(
            PlannerAgent::infer_task_type("Review the code"),
            "CodeReview"
        );
        assert_eq!(
            PlannerAgent::infer_task_type("Implement the feature"),
            "CodeGeneration"
        );
        assert_eq!(
            PlannerAgent::infer_task_type("Refactor the module"),
            "CodeEdit"
        );
        assert_eq!(
            PlannerAgent::infer_task_type("Combine results"),
            "Synthesis"
        );
    }

    #[test]
    fn test_parse_planning_response() {
        let task = Task::new("Build a new feature", TaskType::Planning);
        let response = r#"## Analysis
The user wants to build a new feature which requires multiple steps.

## Plan
1. Explore existing code → Explorer
2. Implement feature → Coder (depends on: 1)
3. Write tests → Tester (depends on: 2)

## Strategy
Hybrid - some tasks can run in parallel after exploration

## Complexity
6/10 - Medium complexity due to multiple components"#;

        let result = PlannerAgent::parse_planning_response(&task, response).unwrap();

        assert_eq!(result.subtasks.len(), 3);
        assert_eq!(result.complexity, 6);
        assert_eq!(result.strategy, ExecutionStrategy::Hybrid);
        assert!(result.reasoning.contains("multiple steps"));
    }

    #[tokio::test]
    async fn test_process_planning_task() {
        let task = Task::new("Plan a project", TaskType::Planning);
        let response = r#"## Plan
1. Setup project → Coder
2. Write code → Coder (depends on: 1)

## Complexity
5/10"#;

        let agent = PlannerAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).await.unwrap();

        assert!(result.success);
        assert_eq!(
            result.metadata.get("subtask_count").unwrap(),
            &serde_json::json!(2)
        );
        assert_eq!(
            result.metadata.get("complexity").unwrap(),
            &serde_json::json!(5)
        );
    }

    #[tokio::test]
    async fn test_process_unsupported_task_type() {
        let task = Task::new("Write code", TaskType::CodeGeneration);
        let response = "Some response";

        let agent = PlannerAgent;
        let shared_memory = Arc::new(SharedMemory::new());
        let result = agent.process_task(&task, response, shared_memory).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_planning_result_to_plan() {
        let original_task = Task::new("Build feature", TaskType::Planning);
        let planning_result = PlanningResult {
            original_task: "Build feature".to_string(),
            subtasks: vec![
                SubtaskInfo {
                    description: "Explore codebase".to_string(),
                    task_type: "Exploration".to_string(),
                    suggested_agent: Some("explorer".to_string()),
                    effort: Some(3),
                    dependencies: vec![],
                },
                SubtaskInfo {
                    description: "Implement feature".to_string(),
                    task_type: "CodeGeneration".to_string(),
                    suggested_agent: Some("coder".to_string()),
                    effort: Some(7),
                    dependencies: vec![0],
                },
            ],
            strategy: ExecutionStrategy::Hybrid,
            complexity: 5,
            reasoning: "Standard feature development".to_string(),
        };

        let plan = PlannerAgent::planning_result_to_plan(planning_result, original_task);

        assert_eq!(plan.subtasks.len(), 2);
        assert!(!plan.execution_order.is_empty());
    }
}
