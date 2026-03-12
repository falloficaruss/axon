# Agent TUI

A multi-agent orchestration terminal user interface (TUI) with dynamic task routing. Built with Rust, `ratatui`, and powered by LLMs.

## Features

- **Multi-Agent System**: Pre-configured specialized agents for different tasks:
  - **Planner**: Analyzes and breaks down complex tasks
  - **Coder**: Writes and modifies code
  - **Reviewer**: Reviews code for quality and issues
  - **Tester**: Generates and runs tests
  - **Explorer**: Explores codebase structure
  - **Integrator**: Synthesizes results from multiple agents

- **Dynamic Routing**: Automatic task analysis and routing to the most appropriate agent based on confidence scores

- **Two Operation Modes**:
  - **Auto Mode**: Automatically routes tasks to the best agent
  - **Manual Mode**: Manually select which agent to use

- **Task Decomposition**: Complex tasks are automatically broken into subtasks with dependency tracking and parallel execution support

- **Session Persistence**: Auto-save conversations and session state

- **Shared Memory**: Agents can share context and information across tasks

- **Streaming Responses**: Real-time streaming output from agents

- **Markdown Rendering**: Rich markdown display with syntax highlighting

## Installation

### Prerequisites

- Rust 1.70+ (edition 2021)
- An OpenAI API key (or configure alternative LLM provider)

### Build from Source

```bash
git clone https://github.com/falloficarus22/axon.git
cd axon
cargo build --release
```

The binary will be available at `target/release/agent-tui`.

### Run Directly

```bash
cargo run
```

## Configuration

On first run, Agent TUI creates a default configuration file at:
- **Linux**: `~/.config/agent-tui/config.toml`
- **macOS**: `~/Library/Application Support/agent-tui/config.toml`

### Environment Variables

Set your API key via environment variable:

```bash
export OPENAI_API_KEY="your-api-key-here"
```

### Configuration Options

Edit `~/.config/agent-tui/config.toml`:

```toml
# LLM Configuration
[llm]
provider = "openai"
api_key = "$OPENAI_API_KEY"  # Use $ prefix for env vars
model = "gpt-4o"
max_tokens = 4096
temperature = 0.7

# Orchestration Settings
[orchestration]
mode = "auto"  # or "manual"
max_concurrent_agents = 5
routing_confidence_threshold = 0.8
auto_confirm_threshold = 0.95

# Persistence Settings
[persistence]
session_dir = "~/.agent-tui/sessions"
memory_dir = "~/.agent-tui/memory"
auto_save_interval = 30  # seconds
max_sessions = 100

# UI Settings
[ui]
theme = "dark"
show_agent_flow = true
show_timestamps = true
show_confidence_scores = true
datetime_format = "%H:%M:%S"

# Keybindings
[keybindings]
quit = "ctrl+c"
submit = "enter"
new_line = "shift+enter"
history_up = "up"
history_down = "down"
autocomplete = "tab"
command_palette = "ctrl+k"
agent_selector = "ctrl+a"
sidebar_toggle = "ctrl+b"
memory_manager = "ctrl+m"
```

### Custom Agents

Define custom agents in the `[agents]` section:

```toml
[agents.my_custom_agent]
enabled = true
role = "coder"
description = "My custom coding agent"
model = "gpt-4o"
system_prompt = """
You are a specialized coding assistant.
Your job is to write clean, efficient code.
"""
capabilities = ["code", "refactor", "debug"]
```

## Usage

### Starting the Application

```bash
agent-tui
```

### Basic Interaction

1. Type your request in the input field at the bottom
2. Press `Enter` to submit
3. The system analyzes your request and routes it to the appropriate agent
4. View the agent's response in the chat area

### Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show help message |
| `/mode auto` | Enable automatic agent routing |
| `/mode manual` | Enable manual agent selection |
| `/agent <name>` | Select specific agent (manual mode) |
| `/agents` | List all available agents |
| `/clear` | Clear current session |
| `/new` | Start a new session |
| `/save <name>` | Save current session to file |
| `/load <id>` | Load a session by ID |
| `/sessions` | List all saved sessions |
| `/delete <id>` | Delete a session by ID |
| `/remember <key> <value>` | Store a value in session memory |
| `/recall <key>` | Retrieve a value from session memory |
| `/forget <key>` | Delete a value from session memory |
| `/cancel` | Cancel the currently running task |
| `/quit` | Exit application |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Submit input |
| `Ctrl+C` | Quit application |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+M` | Open memory manager |
| `Ctrl+A` | Open agent selector |
| `Ctrl+X` | Cancel running task |
| `Ctrl+K` | Open command palette |
| `Up/Down` | Navigate history |
| `Tab` | Autocomplete |
| `/` | Enter command mode (when input is empty) |
| `Shift+Enter` | New line in input |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         TUI Layer                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Chat      │  │   Input     │  │   Sidebar/Status    │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────▼──────────────────────────────┐
│                     Orchestrator                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │   Router     │  │   Planner    │  │    Executor      │  │
│  │  (Analysis)  │  │ (Decompose)  │  │  (Agent Pool)    │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────▼──────────────────────────────┐
│                      Agent Layer                            │
│  ┌────────┐ ┌────────┐ ┌─────────┐ ┌────────┐ ┌─────────┐  │
│  │Planner │ │ Coder  │ │Reviewer │ │Tester  │ │Explorer │  │
│  └────────┘ └────────┘ └─────────┘ └────────┘ └─────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────▼──────────────────────────────┐
│                      LLM Client                             │
│                   (OpenAI API / Other)                      │
└─────────────────────────────────────────────────────────────┘
```

### Key Components

- **`tui/`**: Terminal UI components using `ratatui`
- **`orchestrator/`**: Task routing, planning, and execution coordination
- **`agent/`**: Agent definitions, registry, and runtime
- **`llm/`**: LLM client abstraction
- **`persistence/`**: Session and memory storage
- **`types/`**: Core data types and structures
- **`shared/`**: Shared memory for inter-agent communication

## Development

### Run in Debug Mode

```bash
cargo run
```

### Run Tests

```bash
cargo test
```

### Build with Features

```bash
# Enable mock LLM for testing without API key
cargo run --features mock-llm
```

### Code Formatting

```bash
cargo fmt
```

### Linting

```bash
cargo clippy
```

## Project Structure

```
axon/
├── README.md            # This file
├── .github/             # GitHub workflows and templates
│   └── workflows/
│       └── opencode.yml # AI-powered coding workflow
├── agent-tui/           # Main Rust crate
│   ├── Cargo.toml       # Project dependencies and metadata
│   ├── src/
│   │   ├── main.rs      # Application entry point
│   │   ├── config.rs    # Configuration management
│   │   ├── agent/       # Agent system
│   │   │   ├── mod.rs   # Agent registry
│   │   │   ├── runtime.rs   # Agent runtime
│   │   │   └── agents/  # Default agent definitions
│   │   ├── llm/         # LLM client
│   │   ├── orchestrator/# Task orchestration
│   │   │   ├── mod.rs   # Router, Planner, Executor
│   │   │   └── pool.rs  # Agent pool management
│   │   ├── persistence/ # Session/memory storage
│   │   ├── tui/         # Terminal UI
│   │   │   ├── mod.rs   # Main app loop
│   │   │   ├── components/  # UI components
│   │   │   └── markdown.rs  # Markdown rendering
│   │   ├── types/       # Core types
│   │   ├── shared/      # Shared memory
│   │   └── persistence/ # Persistence layer
│   └── target/          # Build artifacts
└── .gitignore           # Git ignore patterns
```

## License

MIT License.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request
