# Axon Architecture Baseline (Phase 0 -> Phase 1)

## Document Control
- Status: Frozen for Phase 1 implementation
- Version: 1.0
- Last updated: 2026-05-01
- Applies to: `agent-tui`

## Architecture Goals
1. Deterministic task orchestration
2. Safe-by-default tool execution
3. Inspectable runtime behavior (events, traces, artifacts)
4. Clear module contracts that allow incremental extension

## Layered System
1. UI Layer (`tui/`)
- Input, command handling, task/run views, artifact inspection
- No direct side effects outside runtime API

2. Runtime Layer (`orchestrator/`)
- Event bus, plan execution, DAG scheduling, retries/cancellation
- Owns lifecycle state transitions and backpressure

3. Agent Layer (`agent/`)
- Role-specific logic (router, planner, coder, reviewer, tester)
- Schema-bound inputs/outputs; no direct tool execution

4. Tool Layer (`tools` abstraction via existing modules)
- Shell/file/search operations routed through policy engine
- Audit logging and sandbox profile enforcement

5. Model Layer (`llm/`)
- Provider abstraction, model selection profiles, fallback strategy
- Structured response validation and repair loops

6. Data Layer (`persistence/`)
- Session storage, run artifacts, event traces, memory snapshots

## Runtime Data Flow
1. User submits request in TUI
2. Runtime creates run context and root task
3. Router decides direct execution vs planning path
4. Planner emits structured task graph (DAG)
5. Scheduler executes ready tasks based on dependency resolution
6. Tasks request tool/model operations through runtime gateways
7. Policy engine authorizes/denies tool calls and records audit events
8. Results/events stream to TUI and persistence
9. Synthesizer produces final response + artifact references

## Core Contracts
1. Agent contract
- Input: immutable task context + scoped memory slice + task payload
- Output: typed result (success/failure), optional child tasks, structured metadata

2. Scheduler contract
- Input: validated DAG and execution policy
- Output: ordered task state transitions with deterministic semantics

3. Tool runner contract
- Input: tool request + sandbox profile + policy context
- Output: normalized tool result + audit record + policy decision

4. Event bus contract
- Typed event envelopes
- Monotonic sequence id per run
- Consumer backpressure protections

## Task Lifecycle State Machine
1. `pending`
2. `ready`
3. `running`
4. `blocked`
5. `retrying`
6. `completed`
7. `failed`
8. `cancelled`

Valid transitions are runtime-owned and enforced centrally.

## Determinism Rules
1. Stable ordering for equal-priority ready tasks (topological + tie-breaker)
2. Retry behavior depends on explicit policy, not ad-hoc branch logic
3. Scheduler decisions are evented and reproducible from persisted traces

## Safety Rules
1. Tool calls only through runtime tool runner
2. Policy check required before execution
3. Destructive or escalated actions require explicit approval state
4. File writes must be workspace-root constrained unless escalated

## Observability Requirements
1. Structured logs include run id, task id, agent id, tool id, latency
2. Event timeline is queryable in-session and persisted post-run
3. Failures include typed error class and causal chain

## Phase 1 Implementation Order
1. Introduce typed event bus interfaces and event envelope model
2. Implement DAG scheduler with deterministic ready-queue ordering
3. Add centralized task state machine and transition validation
4. Add cancellation/timeouts/retry policies with typed runtime errors
5. Add bounded queues/backpressure to event and task pipelines

## Exit Criteria Mapping (Phase 1)
1. Deterministic task execution order for equivalent plans
2. No deadlocks in stress tests (1000+ synthetic orchestrations)
3. Cancellation and timeout behavior covered by integration tests
