# Axon Threat Model (Lean)

## Main Risks
1. Prompt injection driving unsafe commands
2. Unapproved destructive actions
3. Path traversal / writes outside workspace
4. Secret leaks in logs/prompts/artifacts
5. Runaway loops or unbounded task spawning

## Required Controls
- Policy check before every tool call
- Sandbox-by-default execution
- Explicit approval for destructive/escalated actions
- Root-bound path validation
- Secret redaction before logging/persistence
- Concurrency/recursion/task limits + timeouts

## Security Gate (MVP)
- 100% tool calls audited
- 0 unapproved destructive actions
- 0 out-of-root writes without escalation
