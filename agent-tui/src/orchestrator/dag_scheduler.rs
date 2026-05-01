use std::collections::{BTreeSet, HashMap};

use anyhow::{anyhow, Result};

use crate::types::{Id, Plan, Subtask};

#[derive(Debug, Clone)]
pub struct DagScheduler {
    subtasks: Vec<Subtask>,
}

impl DagScheduler {
    pub fn new(plan: Plan) -> Self {
        Self {
            subtasks: plan.subtasks,
        }
    }

    pub fn execution_batches(&self) -> Result<Vec<Vec<Subtask>>> {
        let mut remaining: HashMap<Id, Subtask> = self
            .subtasks
            .iter()
            .cloned()
            .map(|s| (s.id.clone(), s))
            .collect();

        for subtask in remaining.values() {
            for dep in &subtask.dependencies {
                if !remaining.contains_key(dep) {
                    return Err(anyhow!(
                        "Invalid plan: subtask {} depends on missing task {}",
                        subtask.id,
                        dep
                    ));
                }
            }
        }

        let mut completed: BTreeSet<Id> = BTreeSet::new();
        let mut batches: Vec<Vec<Subtask>> = Vec::new();

        while !remaining.is_empty() {
            let mut ready_ids: Vec<Id> = remaining
                .values()
                .filter(|task| task.dependencies.iter().all(|dep| completed.contains(dep)))
                .map(|task| task.id.clone())
                .collect();

            if ready_ids.is_empty() {
                return Err(anyhow!(
                    "Deadlock detected in plan execution: circular dependency or unresolved prerequisite"
                ));
            }

            ready_ids.sort_by(|a, b| {
                let left = remaining.get(a).expect("left task exists");
                let right = remaining.get(b).expect("right task exists");
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            });

            let mut batch = Vec::with_capacity(ready_ids.len());
            for ready_id in ready_ids {
                if let Some(task) = remaining.remove(&ready_id) {
                    completed.insert(ready_id);
                    batch.push(task);
                }
            }

            batches.push(batch);
        }

        Ok(batches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Plan, Task, TaskType};

    #[test]
    fn test_scheduler_returns_topological_batches() {
        let task = Task::new("main", TaskType::Planning);
        let a = Subtask::new("a", TaskType::Exploration);
        let mut b = Subtask::new("b", TaskType::CodeGeneration);
        let mut c = Subtask::new("c", TaskType::TestGeneration);

        b.dependencies = vec![a.id.clone()];
        c.dependencies = vec![a.id.clone()];

        let plan = Plan::new(task).with_subtasks(vec![a.clone(), b.clone(), c.clone()]);
        let scheduler = DagScheduler::new(plan);
        let batches = scheduler.execution_batches().expect("valid schedule");

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0][0].id, a.id);
        assert_eq!(batches[1].len(), 2);
    }

    #[test]
    fn test_scheduler_rejects_cycles() {
        let task = Task::new("main", TaskType::Planning);
        let mut a = Subtask::new("a", TaskType::Exploration);
        let mut b = Subtask::new("b", TaskType::CodeGeneration);

        a.dependencies = vec![b.id.clone()];
        b.dependencies = vec![a.id.clone()];

        let plan = Plan::new(task).with_subtasks(vec![a, b]);
        let scheduler = DagScheduler::new(plan);

        assert!(scheduler.execution_batches().is_err());
    }
}
