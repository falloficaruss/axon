use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::Id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskLifecycleState {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEventKind {
    OrchestrationStarted { task_id: Id, description: String },
    RoutingCompleted { task_id: Id, suggested_agents: usize, requires_subtasks: bool },
    PlanCreated { task_id: Id, subtask_count: usize },
    TaskStateChanged {
        subtask_id: Id,
        description: String,
        state: TaskLifecycleState,
    },
    OrchestrationCompleted { task_id: Id, success: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub kind: RuntimeEventKind,
}

impl RuntimeEvent {
    pub fn new(sequence: u64, kind: RuntimeEventKind) -> Self {
        Self {
            sequence,
            timestamp: Utc::now(),
            kind,
        }
    }
}
