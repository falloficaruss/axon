# Axon Architecture (Lean)

## Layers
1. TUI: input, views, run status, artifacts
2. Runtime: routing, planning, scheduling, retries/cancel
3. Agents: planner/coder/reviewer/tester/explorer/integrator
4. Tools: shell/file/search behind policy checks
5. Model: provider abstraction + fallbacks
6. Data: sessions, memory, traces, artifacts

## Runtime Flow
1. User task
2. Router analysis
3. Planner decomposition (optional)
4. DAG scheduler execution
5. Tool calls through policy gate
6. Synthesis + persistence

## Hard Rules
- No direct tool execution from TUI or agent implementations
- Structured outputs for router/planner
- Single tool-runner path for all side effects
- Bounded concurrency and explicit task states

## Phase 1 Build Order
1. Typed event bus
2. DAG scheduler
3. Task state machine
4. Retry/timeout/cancellation plumbing
