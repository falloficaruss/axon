# Axon Product Requirements Document (Phase 0 Baseline)

## Document Control
- Status: Frozen for Phase 1 implementation
- Version: 1.0
- Last updated: 2026-05-01
- Scope: `agent-tui` production hardening roadmap

## Product Objective
Axon is a keyboard-first agentic coding TUI for engineers who need predictable, safe, and inspectable automation for day-to-day software work.

The product must reliably support end-to-end coding loops without leaving the terminal:
- task intake
- plan generation
- controlled tool execution
- patch/test/review loops
- final summary with artifacts

## Target Users
1. Individual software engineers implementing and refactoring code
2. Tech leads reviewing generated changes and execution traces
3. DevEx/platform engineers validating automation safety in CI-like workflows

## Core User Jobs
1. Turn a natural-language coding request into an executable plan
2. Run multi-step tasks with bounded parallelism and clear dependencies
3. Enforce safe-by-default tool usage with explicit approval for risk
4. Inspect what happened (who did what, which tools ran, what changed)
5. Resume long-running work with minimal context loss

## Top Workflows (Must Work End-to-End)
1. Issue to patch
- Ask for a feature/fix
- Generate plan + sub-tasks
- Execute tools and edit files
- Run tests/lint
- Return summary + changed files + validation status

2. Review and repair
- Analyze an existing patch
- Produce findings ordered by severity
- Apply requested fixes
- Re-run validation and summarize deltas

3. Long-session continuation
- Resume prior session
- Recover key facts/decisions/tasks
- Continue from last actionable state

4. Safe shell assistance
- Propose and run commands under policy
- Require approval for destructive or escalated actions
- Record complete audit trail

## Non-Goals (Phase 0-1)
1. Full IDE replacement
2. Real-time multi-user collaboration
3. Public plugin marketplace and external monetization
4. Autonomous background execution without user-initiated context

## Functional Requirements
1. Deterministic orchestration
- Equivalent plans produce equivalent execution ordering
- Task lifecycle states are explicit and observable

2. Multi-agent execution model
- Agents operate with role contracts and schema-validated IO
- Planner output is structured for scheduler consumption

3. Tool safety and policy
- Every tool invocation is policy checked before execution
- Destructive actions and escalations are blocked pending approval
- Tool inputs/outputs are auditable

4. Context and memory
- Context packing uses recency + relevance + token budget constraints
- Session resume restores task graph, artifacts, and key decisions

5. TUI observability
- Live view of active tasks, statuses, tool runs, and artifacts
- Run timeline inspectable post-completion

## Non-Functional Requirements
1. Reliability
- No deadlocks under stress workload
- Graceful handling of retries/timeouts/cancellations

2. Security
- Default sandbox profile for tool execution
- Path traversal protections for file operations
- Secret redaction in logs/persisted artifacts

3. Performance
- Responsive UI under concurrent task execution
- Bounded memory growth per session

4. Extensibility
- New agent/tool integrations do not require core rewrites

## Success Metrics (Phase 1-2 Gates)
1. Quality
- Scenario eval pass rate >= 80% for baseline suite

2. Safety
- 100% tool calls include policy decision and audit record
- 0 unapproved destructive actions
- 0 out-of-root writes without explicit escalation

3. Runtime robustness
- No deadlocks in 1000+ synthetic orchestrations
- Retry recovery succeeds for >= 95% transient tool/model failures

4. UX
- Power workflow (issue -> patch -> test -> summary) completed in one run
- Interactive latency remains stable under concurrent streaming output

## Constraints and Assumptions
1. Primary runtime is terminal-first and local-workspace oriented
2. Model provider starts with OpenAI, with abstraction for future providers
3. Security defaults take precedence over convenience shortcuts
4. Feature work that bypasses policy/runtime contracts is out of scope

## Release Criteria for Phase 0 Completion
1. `docs/PRD.md` and `docs/ARCHITECTURE.md` are approved and versioned
2. Critical architecture decisions are captured as ADRs in `docs/DECISIONS/`
3. Phase 1 implementation work references this PRD as source of truth
