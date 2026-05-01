# ADR-0003: Centralized Tool Policy Gate and Audit Log

- Status: Accepted
- Date: 2026-05-01

## Context
Tooling power is increasing; safety and compliance depend on uniform enforcement. Distributed checks create bypass risk.

## Decision
Route all tool calls through a centralized policy gate that decides allow/deny/approval-required and emits immutable audit records for each invocation.

## Consequences
1. Consistent enforcement across all agents and UI interactions
2. Enables measurable safety guarantees and forensic debugging
3. Adds integration work for existing direct tool call paths
