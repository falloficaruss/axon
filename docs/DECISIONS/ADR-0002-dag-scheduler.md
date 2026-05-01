# ADR-0002: DAG Scheduler for Plan Execution

- Status: Accepted
- Date: 2026-05-01

## Context
Ad-hoc execution logic limits deterministic behavior and makes failure handling brittle under concurrent tasks.

## Decision
Represent executable plans as DAGs and execute with a deterministic scheduler that resolves dependencies, enforces bounded concurrency, and supports cancellation/retry policies.

## Consequences
1. Deterministic ordering for equivalent plans
2. Better control of parallelism and deadlock prevention strategies
3. Requires plan validation and explicit task state machine integration
