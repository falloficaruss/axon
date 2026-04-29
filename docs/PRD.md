# Axon PRD (Lean)

## Goal
Build a production-grade agentic coding TUI for keyboard-first engineering workflows.

## Users
- Engineers doing implementation, refactoring, review, and testing

## Must-Have Workflows
1. `Task -> Plan -> Execute -> Validate -> Summarize`
2. Parallel multi-agent execution with dependencies
3. Safe tool execution with approval gates
4. Long session resume with useful memory

## MVP Scope
- Deterministic orchestrator runtime
- Tool policy engine + audit logs
- Context packing/retrieval
- Fast and inspectable TUI run view

## Non-Goals (MVP)
- IDE replacement
- Multi-user collaboration
- Public plugin marketplace

## MVP Success Metrics
- >=80% eval pass rate
- Stable p95 latency target
- >=95% tool-audit coverage
- 0 unapproved destructive actions
