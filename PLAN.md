# Agent TUI - Multi-Agent Orchestration Terminal Interface

A Rust-based TUI for multi-agent AI orchestration with dynamic task routing, parallel execution, and persistent memory.

## Current Progress

**Last Updated:** February 27, 2026

### ✅ Completed

#### Phase 0: Foundation (100%)
- [x] Project setup with Cargo.toml and all dependencies
- [x] Directory structure created
- [x] Core types defined (Agent, Task, Message, Session, etc.)
- [x] Configuration module with TOML support

#### Phase 1: TUI Foundation (100%)
- [x] Main entry point with async tokio runtime
- [x] App loop with event handling
- [x] Terminal setup/cleanup with raw mode
- [x] Chat component with scrolling and message display
- [x] Input component with history and cursor navigation
- [x] Sidebar component with agent status
- [x] Markdown rendering with pulldown-cmark

#### Phase 1.5: Bug Fixes & Improvements (100%)
- [x] Fixed input mode glitch (command mode entry)
- [x] Fixed agent selector to use dynamic AgentRegistry
- [x] Added task cancellation mechanism (Ctrl+X, /cancel)
- [x] Implemented streaming UI with real-time updates
- [x] Implemented markdown rendering with pulldown-cmark

#### Phase 2: LLM & Agent Runtime (95%)
- [x] OpenAI LLM client integration with streaming
- [x] Agent runtime with command loop
- [x] Agent lifecycle management (spawn, shutdown, state tracking)
- [x] Event system (Started, Completed, Message, Error, StateChanged)
- [x] Built-in agents defined (Planner, Coder, Reviewer, Tester, Explorer, Integrator)

#### Phase 3: Orchestration (85%)
- [x] Dynamic router with LLM-based task analysis
- [x] Task planner with decomposition into subtasks
- [x] Executor with agent pool management
- [x] Parallel execution support via JoinSet
- [x] Auto-routing and manual mode support

#### Phase 4: Persistence & Shared Memory (75%)
- [x] SessionStore with save/load/list/delete
- [x] MemoryStore with scoped key-value storage
- [x] Atomic writes for session saving
- [x] Auto-save on configurable interval
- [x] Shared memory with hierarchical namespaces
- [ ] UI integration for persistence commands
- [ ] Shared memory connected to agent runtime

#### Testing & Quality (60%)
- [x] 140 unit tests across all modules
- [x] Tests for types, config, agent runtime, orchestrator, pool, TUI components
- [ ] Integration tests for agent workflows
- [ ] Mock LLM client for testing without API key
- [ ] End-to-end tests
- [ ] Test coverage reporting
- [ ] CI/CD pipeline with automated testing

### 🚧 In Progress
- [ ] Persistence UI integration (save/load sessions via commands)
- [ ] Shared memory integration with agent runtime
- [ ] Fixing 39 compiler warnings (unused code)

### ⏳ Pending

#### Core Features
- [ ] Memory management UI
- [ ] Agent flow visualization
- [ ] Themes and advanced configuration
- [ ] Custom keybindings from config

#### Advanced Features
- [ ] MCP support
- [ ] Multiple LLM providers (Anthropic, local models)
- [ ] Plugin system for custom agents
- [ ] GitHub integration

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    TUI (Ratatui)                        │
│  ┌────────────┐  ┌────────────┐  ┌─────────────────┐   │
│  │ Chat View  │  │ Agent Flow │  │  MCP Manager    │   │
│  └────────────┘  └────────────┘  └─────────────────┘   │
└─────────────────────────────────────────────────────────┘
                          │
┌─────────────────────────────────────────────────────────┐
│              Multi-Agent Orchestrator                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Task Router  │  │  Agent Pool  │  │ Shared State │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────┘
                          │
┌─────────────────────────────────────────────────────────┐
│              Agent Runtime (Async)                      │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐  ┌─────────────┐  │
│  │  Agent  │ │  Agent  │ │  Agent  │  │   Planner   │  │
│  │  (Code) │ │ (Docs)  │ │ (Test)  │  │   Agent     │  │
│  └─────────┘ └─────────┘ └─────────┘  └─────────────┘  │
└─────────────────────────────────────────────────────────┘
                          │
┌─────────────────────────────────────────────────────────┐
│              OpenAI Integration                         │
│  ┌────────────┐  ┌────────────┐  ┌─────────────────┐   │
│  │   Client   │  │  Streaming │  │ Token Manager   │   │
│  └────────────┘  └────────────┘  └─────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Project Structure

```
agent-tui/
├── Cargo.toml
├── src/
│   ├── main.rs                 # Entry point
│   ├── app.rs                  # App state & loop
│   ├── config.rs               # Configuration
│   ├── tui/
│   │   ├── mod.rs
│   │   ├── ui.rs              # Main UI layout
│   │   └── components/
│   │       ├── chat.rs        # Chat interface
│   │       ├── agent_flow.rs  # Visual agent graph
│   │       ├── sidebar.rs     # Session/agent list
│   │       ├── input.rs       # Command input
│   │       └── memory.rs      # Memory management UI
│   ├── orchestrator/
│   │   ├── mod.rs
│   │   ├── router.rs          # Dynamic task routing
│   │   ├── planner.rs         # Task decomposition
│   │   ├── executor.rs        # Parallel execution
│   │   └── pool.rs            # Agent lifecycle
│   ├── agent/
│   │   ├── mod.rs
│   │   ├── types.rs           # Agent definitions
│   │   ├── runtime.rs         # Agent spawning
│   │   └── agents/            # Built-in agents
│   │       ├── planner.rs
│   │       ├── coder.rs
│   │       ├── reviewer.rs
│   │       ├── tester.rs
│   │       └── explorer.rs
│   ├── llm/
│   │   ├── mod.rs
│   │   ├── client.rs          # OpenAI client
│   │   └── streaming.rs       # Streaming responses
│   ├── shared/
│   │   ├── mod.rs
│   │   ├── memory.rs          # Shared state/memory
│   │   └── context.rs         # Execution context
│   ├── persistence/
│   │   ├── mod.rs
│   │   ├── session.rs         # Session storage
│   │   └── memory.rs          # Memory persistence
│   └── types/
│       └── mod.rs             # Core types
└── config/
    └── agents.toml            # Agent definitions
```

## Implementation Phases

### Phase 0: Foundation (Days 1-2)

**Task 1: Project Setup**
- Create Cargo.toml with dependencies
- Set up directory structure
- Configure rust-toolchain if needed

**Dependencies:**
- `ratatui` - TUI framework
- `crossterm` - Cross-platform terminal
- `tokio` - Async runtime
- `async-openai` - OpenAI client
- `serde` / `serde_json` - Serialization
- `chrono` - Date/time
- `anyhow` / `thiserror` - Errors
- `config` - Configuration
- `tracing` / `tracing-subscriber` - Logging

**Task 2: Core Types**
Define foundational types:

```rust
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: AgentRole,
    pub capabilities: Vec<Capability>,
    pub system_prompt: String,
    pub model: String,
    pub state: AgentState,
}

pub enum AgentRole {
    Planner,      // Orchestrates workflows
    Coder,        // Code generation
    Reviewer,     // Code review
    Tester,       // Test generation
    Explorer,     // Codebase exploration
    Integrator,   // Synthesizes results
}

pub struct Task {
    pub id: String,
    pub description: String,
    pub task_type: TaskType,
    pub assigned_agent: Option<String>,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
    pub result: Option<TaskResult>,
}

pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub agent_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

pub struct Session {
    pub id: String,
    pub title: String,
    pub messages: Vec<Message>,
    pub tasks: Vec<Task>,
    pub mode: SessionMode,  // Auto or Manual
}

pub enum SessionMode {
    Auto,     // Dynamic agent routing
    Manual,   // User selects agent
}
```

### Phase 1: TUI Foundation (Days 3-5)

**Task 3: Basic App Loop**
- Event handling (keyboard input)
- Terminal setup/cleanup
- Component coordination
- State updates

**Task 4: Chat Interface**
- Message list with scrolling
- Markdown rendering (simplified)
- Syntax highlighting
- Agent attribution
- Timestamps

**Task 5: Input Component**
- Multi-line input support
- Command history (↑/↓ arrows)
- Slash commands:
  - `/mode auto` - Enable dynamic routing
  - `/mode manual` - Manual agent selection
  - `/agent <name>` - Force specific agent
  - `/memory` - Open memory manager
  - `/sessions` - List sessions
- Tab autocomplete
- Cursor navigation

**Task 6: Sidebar**
- Session list
- Active agents display
- Agent status indicators:
  - 🟢 Idle
  - 🟡 Running
  - ✅ Completed
  - ❌ Failed
- Quick actions

### Phase 2: LLM & Agent Runtime (Days 6-9)

**Task 7: OpenAI Client**
- Streaming response handling
- Error retry logic with exponential backoff
- Token counting
- Rate limiting

**Task 8: Agent Runtime**
- Agent spawning (tokio tasks)
- Message passing (tokio::sync::mpsc)
- Agent lifecycle management
- Context management

**Task 9: Built-in Agents**

1. **Planner Agent**
   - Analyzes user request
   - Decomposes into subtasks
   - Determines which agents needed

2. **Coder Agent**
   - Code generation
   - File editing
   - Refactoring

3. **Reviewer Agent**
   - Code review
   - Bug detection
   - Style checking

4. **Tester Agent**
   - Test generation
   - Test execution coordination

5. **Explorer Agent**
   - File system navigation
   - Codebase search
   - Context gathering

### Phase 3: Dynamic Orchestration (Days 10-13)

**Task 10: Dynamic Router**
```rust
pub async fn route_task(
    &self,
    task: &Task,
    context: &Context,
) -> RoutingDecision {
    // Uses LLM to analyze task
    // Returns agent(s) to use
    // Confidence score
}
```

Routing logic:
- If `SessionMode::Auto` → LLM decides
- If `SessionMode::Manual` → User must specify
- Confidence threshold for auto-routing (default: 0.8)

**Task 11: User Override System**
Commands:
- `/mode auto` - Enable dynamic routing
- `/mode manual` - Require manual selection
- `/agent <name>` - Force specific agent
- `/route` - Preview routing decision
- `/confirm` - Approve/reject routing

UI indicators:
- Current mode in status bar
- Agent attribution on messages
- Routing confidence score

**Task 12: Agent Pool**
- Max concurrent agents (configurable, default: 5)
- Queue management
- Health checks
- Resource monitoring

### Phase 4: Shared Memory & Persistence (Days 14-17)

**Task 13: Shared Memory**
```rust
pub struct SharedMemory {
    global: Arc<RwLock<HashMap<String, Value>>>,
    session: Arc<RwLock<HashMap<String, Value>>>,
    agent: Arc<RwLock<HashMap<String, HashMap<String, Value>>>>,
}
```

Features:
- Thread-safe read/write
- Hierarchical namespaces
- Conflict resolution
- TTL support

**Task 14: Persistence Layer**
- Sessions: `~/.agent-tui/sessions/`
- Memory: `~/.agent-tui/memory/`
- JSON format
- Auto-save on change
- Compression for large sessions

**Task 15: Memory Management UI**
- List stored memories
- View/edit values
- Clear by scope
- Import/export
- Search/filter

### Phase 5: Advanced UI (Days 18-20)

**Task 16: Agent Flow Visualization**
- Real-time execution graph
- Parallel branches
- Color-coded status
- Click to inspect
- Timeline view

**Task 17: Themes & Configuration**
- TOML config file
- Custom keybindings
- Color themes
- User-defined agents

### Phase 6: Polish (Days 21-23)

**Task 18: Error Handling**
- Comprehensive error types
- User-friendly messages
- Retry logic
- Graceful degradation

**Task 19: Logging**
- Structured logging
- Log rotation
- Debug mode
- Performance metrics

**Task 20: Documentation**
- README
- Usage guide
- Agent reference
- Architecture docs

## Configuration

**`~/.config/agent-tui/config.toml`**

```toml
[llm]
provider = "openai"
api_key = "$OPENAI_API_KEY"
model = "gpt-4o"
max_tokens = 4096
temperature = 0.7

[orchestration]
mode = "auto"  # or "manual"
max_concurrent_agents = 5
routing_confidence_threshold = 0.8
auto_confirm_threshold = 0.95  # Auto-execute if confidence > this

[agents.coder]
enabled = true
model = "gpt-4o"
system_prompt = """You are a skilled programmer. 
Write clean, well-documented code.
Always explain your approach before coding."""

[agents.reviewer]
enabled = true
model = "gpt-4o-mini"
system_prompt = """You are a code reviewer.
Focus on bugs, security issues, and best practices.
Be constructive in your feedback."""

[agents.planner]
enabled = true
model = "gpt-4o"
system_prompt = """You are a task planner.
Break down complex tasks into manageable subtasks.
Assign each subtask to the most appropriate agent."""

[persistence]
session_dir = "~/.agent-tui/sessions"
memory_dir = "~/.agent-tui/memory"
auto_save_interval = 30  # seconds
max_sessions = 100

[ui]
theme = "dark"  # dark, light, or custom
show_agent_flow = true
show_timestamps = true
show_confidence_scores = true
datetime_format = "%H:%M:%S"

[keybindings]
quit = "Ctrl+C"
submit = "Enter"
new_line = "Shift+Enter"
history_up = "Up"
history_down = "Down"
autocomplete = "Tab"
command_palette = "Ctrl+K"
agent_selector = "Ctrl+A"
sidebar_toggle = "Ctrl+B"
memory_manager = "Ctrl+M"
```

## Agent Definitions

**`~/.config/agent-tui/agents.toml`**

```toml
[[agent]]
name = "senior-coder"
role = "coder"
description = "Senior-level code generation"
model = "gpt-4o"
system_prompt = """You are a senior software engineer with 10+ years of experience.
You write production-ready code with proper error handling and tests."""
capabilities = ["code", "refactor", "debug", "optimize"]

[[agent]]
name = "junior-coder"
role = "coder"
description = "Quick prototyping and simple tasks"
model = "gpt-4o-mini"
system_prompt = "You write simple, straightforward code."
capabilities = ["code"]

[[agent]]
name = "security-reviewer"
role = "reviewer"
description = "Security-focused code review"
model = "gpt-4o"
system_prompt = """You are a security expert.
Focus on identifying security vulnerabilities, injection risks, and data exposure issues."""
capabilities = ["security-review"]
```

## Key Design Decisions

1. **User Control**: Always allow mode toggle between Auto/Manual
2. **Transparency**: Show which agent is working and why
3. **Extensibility**: Easy to add new agents via config
4. **Performance**: Async throughout, concurrent execution
5. **Reliability**: Persistence, retries, graceful failures

## Commands Reference

### Navigation
- `Ctrl+C` - Quit
- `Ctrl+B` - Toggle sidebar
- `Tab` - Autocomplete
- `↑/↓` - History navigation

### Session Management
- `/new` - New session
- `/sessions` - List sessions
- `/load <id>` - Load session
- `/save <name>` - Save session
- `/clear` - Clear current session

### Mode Control
- `/mode auto` - Enable auto-routing
- `/mode manual` - Manual mode
- `/agent <name>` - Set active agent (manual mode)
- `/route` - Preview routing decision

### Agent Management
- `/agents` - List available agents
- `/status` - Show agent pool status
- `/cancel` - Cancel current task

### Memory
- `/memory` - Open memory manager
- `/remember <key> <value>` - Store in memory
- `/recall <key>` - Retrieve from memory
- `/forget <key>` - Remove from memory

### Help
- `/help` - Show all commands
- `/help <command>` - Show command details

## Dependencies

```toml
[package]
name = "agent-tui"
version = "0.1.0"
edition = "2021"

[dependencies]
# Async runtime
tokio = { version = "1.0", features = ["full"] }
tokio-util = "0.7"

# TUI
ratatui = "0.29"
crossterm = "0.28"

# OpenAI
async-openai = "0.26"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"

# Date/Time
chrono = { version = "0.4", features = ["serde"] }

# Errors
anyhow = "1.0"
thiserror = "1.0"

# Configuration
config = "0.14"
dirs = "5.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Utilities
uuid = { version = "1.0", features = ["v4"] }
rand = "0.8"
regex = "1.10"
lazy_static = "1.4"
indexmap = "2.0"

# Markdown (for chat display)
pulldown-cmark = "0.12"

# Syntax highlighting
syntect = "5.1"

[dev-dependencies]
tempfile = "3.0"
mockall = "0.13"
```

## Development Roadmap

### MVP (Week 1-2) - 90% Complete
- [x] Basic TUI with chat
- [x] OpenAI integration
- [x] 6 core agents (Coder, Planner, Reviewer, Tester, Explorer, Integrator) - Defined and runtime-ready
- [x] Manual mode - Fully functional
- [x] Simple auto-routing - Implemented with LLM analysis
- [x] Session persistence - Store implemented, UI integration pending
- [x] Basic memory - Store implemented, UI integration pending

### Advanced (Week 3-4) - 40% Complete
- [x] Parallel agent execution - JoinSet-based execution
- [ ] Agent flow visualization
- [ ] Advanced memory management UI
- [x] Custom agents via config - Config structure ready
- [ ] Themes system - Config structure ready, implementation pending

### Future
- [ ] MCP support
- [ ] Additional LLM providers
- [ ] Plugin system
- [ ] Multi-user support
- [ ] Web interface

## Notes

- Initial focus: OpenAI provider only
- MCP implementation: Optional, can be added later
- Memory: File-based with optional remote storage
- User control: Primary design principle
- Performance: Rust native speed throughout
