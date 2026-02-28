# Agent TUI (Axon)

A robust, multi-agent orchestration system with a Terminal User Interface (TUI), built in Rust. Axon enables autonomous agents to plan, code, review, and execute tasks with minimal human intervention.

## Key Features

*   **Multi-Agent Orchestration:** 
    *   **Planner:** Decomposes complex tasks into manageable subtasks.
    *   **Coder:** Autonomously writes, edits, and debugs code.
    *   **Reviewer:** Analyzes code for quality and security.
    *   **Tester:** Generates and runs test suites.
    *   **Explorer:** Navigates and understands the codebase.
    *   **Integrator:** Synthesizes results into cohesive outputs.

*   **Autonomous Execution:**
    *   **File Operations:** The `CoderAgent` can autonomously create, update, and delete files based on LLM suggestions.
    *   **Smart Parsing:** Automatically extracts code blocks with file paths (e.g., ` ```rust:src/main.rs `) and applies changes to disk.
    *   **Task Processing:** The decoupled `TaskProcessor` trait ensures modular and safe execution of agent-specific logic.

*   **Rich TUI:**
    *   **Interactive Chat:** Real-time communication with agents.
    *   **Task Dashboard:** Visual tracking of multi-step plans (Pending).
    *   **Context Management:** Session and memory persistence.

## Architecture

### Agent Runtime
The core runtime (`src/agent/runtime.rs`) manages the lifecycle of agents. It uses a message-passing architecture to handle:
1.  **Task Execution:** Sending prompts to LLMs and processing responses.
2.  **Autonomous Actions:** If an agent (like `Coder`) returns file operations in its metadata, the runtime validates and applies them to the file system.
3.  **State Management:** Tracking agent states (Idle, Running, Completed, Failed).

### Task Processing
We utilize a `TaskProcessor` trait to decouple the runtime from specific agent implementations. This allows for:
*   **Specialized Logic:** Each agent role can define custom behavior for processing tasks.
*   **Safety:** File operations are validated before execution.
*   **Extensibility:** New agent types can be added without modifying the core runtime.

## Usage

### Prerequisite
*   Rust (latest stable)
*   `OPENAI_API_KEY` environment variable set.

### Running
```bash
cargo run
```

### Commands
*   `/mode auto` - Enable automatic agent routing.
*   `/mode manual` - Manually select an agent.
*   `/agent <name>` - Switch active agent.
*   `/help` - Show all commands.

## Development

### Building
```bash
cargo build
```

### Testing
```bash
cargo test
```
