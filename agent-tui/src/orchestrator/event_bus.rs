use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::broadcast;

use super::events::{RuntimeEvent, RuntimeEventKind};

#[derive(Clone)]
pub struct TypedEventBus {
    tx: broadcast::Sender<RuntimeEvent>,
    sequence: Arc<AtomicU64>,
}

impl TypedEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn publish(&self, kind: RuntimeEventKind) {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.tx.send(RuntimeEvent::new(sequence, kind));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::events::RuntimeEventKind;

    #[test]
    fn test_event_bus_publish_increments_sequence() {
        let bus = TypedEventBus::new(16);
        let mut rx = bus.subscribe();
        bus.publish(RuntimeEventKind::OrchestrationCompleted {
            task_id: "task-1".to_string(),
            success: true,
        });
        bus.publish(RuntimeEventKind::OrchestrationCompleted {
            task_id: "task-2".to_string(),
            success: false,
        });
        let first = rx.try_recv().expect("first event");
        let second = rx.try_recv().expect("second event");

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
    }
}
