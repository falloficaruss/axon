# ADR-0001: Typed Event Bus as Runtime Backbone

- Status: Accepted
- Date: 2026-05-01

## Context
Phase 1 requires deterministic orchestration, observability, and clear state transitions. Existing prototype flows are functional but rely on loosely-coupled runtime signaling.

## Decision
Adopt a typed event bus as the single runtime signaling mechanism for task lifecycle, tool execution, model activity, and UI updates.

## Consequences
1. Improved traceability and replayability of runs
2. Cleaner boundaries between UI, runtime, and persistence
3. Initial implementation overhead for event schema and adapters
