pub mod components;
pub mod markdown;
pub mod session_manager;
pub mod command_handler;
pub mod popups;
pub mod theme;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::Block,
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use std::sync::Arc;

use crate::{
    agent::{AgentEvent},
    agent::AgentRegistry,
    config::Config,
    llm::LlmProvider,
    orchestrator::Orchestrator,
    types::{AppEvent, Message, MessageRole, SessionMode, Agent, Task, TaskType},
};
#[cfg(feature = "mock-llm")]
use crate::llm::MockLlmClient;
#[cfg(not(feature = "mock-llm"))]
use crate::llm::LlmClient;

use self::components::{Chat, Input, Sidebar};
use crate::shared::SharedMemory;

pub use self::session_manager::SessionManager;

/// Represents a pending confirmation request
pub struct PendingConfirmation {
    pub title: String,
    pub message: String,
    pub changes: Vec<crate::agent::agents::coder::CodeChange>,
    pub response_tx: tokio::sync::oneshot::Sender<bool>,
}

/// Models the current task state in the application
pub struct TaskState {
    /// The task's unique identifier
    pub task_id: Option<crate::types::Id>,
    /// Current status of the task
    pub status: crate::types::TaskStatus,
    /// Result of the task (if completed)
    pub result: Option<crate::types::TaskResult>,
    /// The async handle for the task (for cancellation)
    pub handle: Option<tokio::task::JoinHandle<()>>,
}

impl Default for TaskState {
    fn default() -> Self {
        Self {
            task_id: None,
            status: crate::types::TaskStatus::Pending,
            result: None,
            handle: None,
        }
    }
}

impl TaskState {
    /// Start a new task
    pub fn start(&mut self, task_id: crate::types::Id, handle: tokio::task::JoinHandle<()>) {
        self.task_id = Some(task_id);
        self.status = crate::types::TaskStatus::Running;
        self.result = None;
        self.handle = Some(handle);
    }

    /// Mark task as completed successfully
    pub fn complete(&mut self, result: crate::types::TaskResult) {
        self.status = crate::types::TaskStatus::Completed;
        self.result = Some(result);
        self.handle = None;
    }

    /// Mark task as failed
    pub fn fail(&mut self, error: String) {
        self.status = crate::types::TaskStatus::Failed;
        self.result = Some(crate::types::TaskResult {
            success: false,
            output: String::new(),
            error: Some(error),
            metadata: std::collections::HashMap::new(),
        });
        self.handle = None;
    }

    /// Cancel the task
    pub fn cancel(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
        self.status = crate::types::TaskStatus::Cancelled;
        self.handle = None;
    }

    /// Clear the task state
    pub fn clear(&mut self) {
        self.task_id = None;
        self.status = crate::types::TaskStatus::Pending;
        self.result = None;
        self.handle = None;
    }

    /// Check if a task is currently running
    pub fn is_running(&self) -> bool {
        self.status == crate::types::TaskStatus::Running && self.handle.is_some()
    }
}

/// Main application state
pub struct App {
    /// Application configuration
    config: Config,
    /// Session manager (handles session state and persistence)
    pub session_manager: SessionManager,
    /// Chat component
    chat: Chat,
    /// Input component
    input: Input,
    /// Sidebar component
    sidebar: Sidebar,
    /// LLM client (used by orchestrator; retained for direct-access use cases)
    #[allow(dead_code)]
    llm_client: Option<Arc<dyn LlmProvider>>,
    /// Orchestrator for task execution
    orchestrator: Option<Arc<Orchestrator>>,
    /// Agent registry
    agent_registry: Arc<RwLock<AgentRegistry>>,
    /// Shared memory for agents (used by orchestrator; retained for direct-access use cases)
    #[allow(dead_code)]
    shared_memory: Arc<SharedMemory>,
    /// Active agent for manual mode
    pub active_agent: Option<Agent>,
    /// Agent event receiver
    agent_event_rx: mpsc::Receiver<AgentEvent>,
    /// Agent event sender (cloned into orchestrator; retained for future direct use)
    #[allow(dead_code)]
    agent_event_tx: mpsc::Sender<AgentEvent>,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Show sidebar
    show_sidebar: bool,
    /// Current mode
    mode: AppMode,
    /// Event receiver
    event_rx: mpsc::Receiver<AppEvent>,
    /// Event sender
    event_tx: mpsc::Sender<AppEvent>,
    /// Tick rate
    tick_rate: Duration,
    /// Current task state (models task lifecycle)
    task_state: TaskState,
    /// Cached agent list for UI rendering
    cached_agents: Vec<Agent>,
    /// Selected agent index for agent selector (separate from sidebar)
    agent_selected_index: usize,
    /// Pending confirmation request
    pending_confirmation: Option<PendingConfirmation>,
    /// Selected slash-command suggestion index
    command_selected_index: usize,
}

/// Application modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Normal mode - chat and input
    Normal,
    /// Command mode - entering a slash command
    Command,
    /// Agent selection mode
    AgentSelect,
    /// Memory manager mode
    MemoryManager,
    /// Confirmation dialog (not yet implemented)
    #[allow(dead_code)]
    Confirm,
    /// Sidebar focus mode
    Sidebar,
}

impl App {
    /// Create a new application instance
    pub fn new(config: Config) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let (agent_event_tx, agent_event_rx) = mpsc::channel(100);
        
        // Initialize LLM client if API key is available, or force a mock provider when built
        // with the `mock-llm` feature for local UI testing without external API calls.
        #[cfg(feature = "mock-llm")]
        let llm_client: Option<Arc<dyn LlmProvider>> = {
            info!("Initializing mock LLM client (`mock-llm` feature enabled)");
            Some(Arc::new(MockLlmClient::new(
                "Mock response from agent-tui.",
            )))
        };

        #[cfg(not(feature = "mock-llm"))]
        let llm_client: Option<Arc<dyn LlmProvider>> = if config.llm.api_key.starts_with("$") {
            // Try to get from environment variable
            let env_var = &config.llm.api_key[1..];
            match std::env::var(env_var) {
                Ok(api_key) => {
                    info!("Initializing LLM client with API key from environment");
                    Some(Arc::new(LlmClient::new(
                        &api_key,
                        &config.llm.model,
                        config.llm.max_tokens,
                        config.llm.temperature,
                    )))
                }
                Err(_) => {
                    warn!("LLM API key environment variable '{}' not set", env_var);
                    None
                }
            }
        } else {
            info!("Initializing LLM client with configured API key");
            Some(Arc::new(LlmClient::new(
                &config.llm.api_key,
                &config.llm.model,
                config.llm.max_tokens,
                config.llm.temperature,
            )))
        };
        
        // Initialize agent registry with default agents
        let mut agent_registry = AgentRegistry::new();
        crate::agent::agents::initialize_default_agents(&mut agent_registry);
        
        // Initialize persistence stores
        let session_manager = SessionManager::new(&config);
        
        // Initialize shared memory
        let shared_memory = Arc::new(SharedMemory::new());
        
        // Set default active agent to coder for manual mode
        let active_agent = agent_registry.get("coder").cloned();
        
        let agent_registry = Arc::new(RwLock::new(agent_registry));
        
        // Initialize orchestrator if we have an LLM client
        let orchestrator = llm_client.as_ref().map(|client| {
            Arc::new(Orchestrator::new(
                client.clone(),
                agent_registry.clone(),
                shared_memory.clone(),
                agent_event_tx.clone(),
                config.orchestration.max_concurrent_agents,
                config.workspace_root(),
            ))
        });
        
        Ok(Self {
            config: config.clone(),
            session_manager,
            chat: Chat::new(),
            input: Input::new(),
            sidebar: Sidebar::new(),
            llm_client,
            orchestrator,
            agent_registry,
            shared_memory,
            active_agent,
            agent_event_rx,
            agent_event_tx,
            should_quit: false,
            show_sidebar: true,
            mode: AppMode::Normal,
            event_rx,
            event_tx,
            tick_rate: Duration::from_millis(250),
            task_state: TaskState::default(),
            cached_agents: Vec::new(),
            agent_selected_index: 0,
            pending_confirmation: None,
            command_selected_index: 0,
        })
    }

    /// Run the main application loop
    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        
        // Use a guard to ensure terminal is restored even on panic
        struct CleanupGuard;
        impl Drop for CleanupGuard {
            fn drop(&mut self) {
                let _ = disable_raw_mode();
                let mut stdout = io::stdout();
                let _ = execute!(
                    stdout,
                    LeaveAlternateScreen,
                    DisableMouseCapture,
                    crossterm::cursor::Show
                );
            }
        }
        let _guard = CleanupGuard;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Run the main loop
        self.run_loop(&mut terminal).await
    }

    /// Restore terminal to normal state. Called on both normal exit and cleanup.
    #[allow(dead_code)]
    fn restore_terminal<B: Backend + io::Write>(terminal: &mut Terminal<B>) -> Result<()> {
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        Ok(())
    }

    /// Main application loop
    async fn run_loop<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        let mut last_tick = Instant::now();

        while !self.should_quit {
            // Draw UI
            terminal.draw(|f| self.draw(f))?;

            // Handle timing
            let timeout = self.tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            // Handle events
            if crossterm::event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    // Handle all key events, not just Press
                    self.handle_key_event(key).await?;
                }
            }

            // Handle tick
            if last_tick.elapsed() >= self.tick_rate {
                self.on_tick().await?;
                last_tick = Instant::now();
            }

            // Handle app events
            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_app_event(event).await?;
            }

            // Handle agent events
            while let Ok(event) = self.agent_event_rx.try_recv() {
                self.handle_agent_event(event).await?;
            }
        }

        Ok(())
    }

    /// Handle key events
    async fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return Ok(());
        }

        match self.mode {
            AppMode::Normal => {
                // Check for Ctrl+key combinations first
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('c') => {
                            self.should_quit = true;
                        }
                        KeyCode::Char('b') => {
                            self.show_sidebar = !self.show_sidebar;
                        }
                        KeyCode::Char('m') => {
                            self.mode = AppMode::MemoryManager;
                            let _ = self.session_manager.refresh_memory_keys().await;
                            let _ = self.session_manager.refresh_selected_memory_value().await;
                        }
                        KeyCode::Char('a') => {
                            self.mode = AppMode::AgentSelect;
                        }
                        KeyCode::Char('x') => {
                            // Cancel current running task
                            self.cancel_current_task().await?;
                        }
                        KeyCode::Char('p') => {
                            self.sidebar.previous_session();
                        }
                        KeyCode::Char('n') => {
                            self.sidebar.next_session(self.session_manager.sessions.len());
                        }
                        KeyCode::Char('l') => {
                            let idx = self.sidebar.selected_session();
                            if idx < self.session_manager.sessions.len() {
                                let session_id = self.session_manager.sessions[idx].id.clone();
                                match self.session_manager.load_session(&session_id).await {
                                    Ok(title) => {
                                        self.chat.clear();
                                        for msg in &self.session_manager.session.messages {
                                            self.chat.add_message(msg.clone());
                                        }
                                        self.add_system_message(&format!("Session '{}' loaded.", title));
                                    }
                                    Err(e) => {
                                        self.add_system_message(&format!("Failed to load session: {}", e));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Regular key handling without Ctrl modifier
                    match key.code {
                        KeyCode::Enter => {
                            self.submit_input().await?;
                        }
                        KeyCode::Up => {
                            self.input.previous_history();
                        }
                        KeyCode::Down => {
                            self.input.next_history();
                        }
                        KeyCode::Tab => {
                            if self.show_sidebar {
                                self.mode = AppMode::Sidebar;
                            } else if let Some(completion) = self.input.autocomplete() {
                                self.input.set_content(&completion);
                            }
                        }
                        KeyCode::Char('/') if self.input.is_empty() => {
                            // Enter command mode without inserting the slash
                            self.mode = AppMode::Command;
                            self.command_selected_index = 0;
                        }
                        KeyCode::Char(c) => {
                            self.input.insert_char(c);
                            self.input.clear_autocomplete();
                        }
                        KeyCode::Backspace => {
                            self.input.delete_char();
                            if self.input.is_empty() {
                                self.mode = AppMode::Normal;
                            }
                            self.input.clear_autocomplete();
                        }
                        KeyCode::Left => {
                            self.input.move_cursor_left();
                        }
                        KeyCode::Right => {
                            self.input.move_cursor_right();
                        }
                        KeyCode::Home => {
                            self.input.move_cursor_home();
                        }
                        KeyCode::End => {
                            self.input.move_cursor_end();
                        }
                        _ => {}
                    }
                }
            }
            AppMode::Command => match key.code {
                KeyCode::Enter => {
                    self.execute_command().await?;
                    self.mode = AppMode::Normal;
                }
                KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                    self.input.clear();
                }
                KeyCode::Up => {
                    if self.command_selected_index > 0 {
                        self.command_selected_index -= 1;
                    }
                }
                KeyCode::Down => {
                    let suggestions = self.filtered_commands();
                    if self.command_selected_index + 1 < suggestions.len() {
                        self.command_selected_index += 1;
                    }
                }
                KeyCode::Tab => {
                    self.autocomplete_selected_command();
                }
                KeyCode::Char(c) => {
                    self.input.insert_char(c);
                    self.command_selected_index = 0;
                }
                KeyCode::Backspace => {
                    self.input.delete_char();
                    if self.input.is_empty() {
                        self.mode = AppMode::Normal;
                    } else {
                        self.command_selected_index = 0;
                    }
                }
                KeyCode::Left => {
                    self.input.move_cursor_left();
                }
                KeyCode::Right => {
                    self.input.move_cursor_right();
                }
                KeyCode::Home => {
                    self.input.move_cursor_home();
                }
                KeyCode::End => {
                    self.input.move_cursor_end();
                }
                _ => {}
            },
            AppMode::AgentSelect => match key.code {
                KeyCode::Esc | KeyCode::Tab => {
                    self.mode = AppMode::Normal;
                }
                KeyCode::Up | KeyCode::Char('p') => {
                    if self.agent_selected_index > 0 {
                        self.agent_selected_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('n') => {
                    if self.agent_selected_index < self.cached_agents.len().saturating_sub(1) {
                        self.agent_selected_index += 1;
                    }
                }
                KeyCode::Enter | KeyCode::Char('l') => {
                    self.select_agent_by_index(self.agent_selected_index);
                    self.mode = AppMode::Normal;
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    let idx = c.to_digit(10).unwrap_or(0) as usize;
                    let idx = idx.saturating_sub(1);
                    self.select_agent_by_index(idx);
                    self.mode = AppMode::Normal;
                }
                _ => {}
            },
            AppMode::MemoryManager => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.mode = AppMode::Normal;
                }
                KeyCode::Up | KeyCode::Char('p') => {
                    if self.session_manager.selected_memory_key > 0 {
                        self.session_manager.selected_memory_key -= 1;
                        let _ = self.session_manager.refresh_selected_memory_value().await;
                    }
                }
                KeyCode::Down | KeyCode::Char('n') => {
                    if self.session_manager.selected_memory_key < self.session_manager.memory_keys.len().saturating_sub(1) {
                        self.session_manager.selected_memory_key += 1;
                        let _ = self.session_manager.refresh_selected_memory_value().await;
                    }
                }
                KeyCode::Char('r') => {
                    let _ = self.session_manager.refresh_memory_keys().await;
                    let _ = self.session_manager.refresh_selected_memory_value().await;
                    info!("Manual memory refresh");
                }
                KeyCode::Enter => {
                    if let Some(key) = self.session_manager.memory_keys.get(self.session_manager.selected_memory_key) {
                        match self.session_manager.memory_store.get(key, "session").await {
                            Ok(Some(value)) => {
                                self.session_manager.cached_memory_values.insert(key.clone(), value.clone());
                                let msg = Message::system(&format!("Memory [{}]: {}", key, value));
                                self.session_manager.session.add_message(msg.clone());
                                self.chat.add_message(msg);
                            }
                            Ok(None) => {
                                let msg = Message::system(&format!("Key '{}' not found in memory.", key));
                                self.session_manager.session.add_message(msg.clone());
                                self.chat.add_message(msg);
                            }
                            Err(e) => {
                                let msg = Message::system(&format!("Failed to recall memory: {}", e));
                                self.session_manager.session.add_message(msg.clone());
                                self.chat.add_message(msg);
                            }
                        }
                    }
                }
                _ => {}
            },
            AppMode::Sidebar => match key.code {
                KeyCode::Tab | KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                }
                KeyCode::Char('p') | KeyCode::Up => {
                    self.sidebar.previous_session();
                }
                KeyCode::Char('n') | KeyCode::Down => {
                    self.sidebar.next_session(self.session_manager.sessions.len());
                }
                KeyCode::Char('r') => {
                    let _ = self.session_manager.refresh_session_list().await;
                    info!("Manual sidebar refresh");
                }
                KeyCode::Char('l') | KeyCode::Enter => {
                    let idx = self.sidebar.selected_session();
                    if idx < self.session_manager.sessions.len() {
                        let session_id = self.session_manager.sessions[idx].id.clone();
                        match self.session_manager.load_session(&session_id).await {
                            Ok(title) => {
                                self.chat.clear();
                                for msg in &self.session_manager.session.messages {
                                    self.chat.add_message(msg.clone());
                                }
                                self.add_system_message(&format!("Session '{}' loaded.", title));
                            }
                            Err(e) => {
                                self.add_system_message(&format!("Failed to load session: {}", e));
                            }
                        }
                    }
                    self.mode = AppMode::Normal;
                }
                _ => {}
            },
            AppMode::Confirm => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    if let Some(pending) = self.pending_confirmation.take() {
                        let _ = pending.response_tx.send(true);
                        self.add_system_message("Action confirmed.");
                    }
                    self.mode = AppMode::Normal;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    if let Some(pending) = self.pending_confirmation.take() {
                        let _ = pending.response_tx.send(false);
                        self.add_system_message("Action rejected.");
                    }
                    self.mode = AppMode::Normal;
                }
                _ => {}
            },
        }
        Ok(())
    }

    /// Handle application events
    async fn handle_app_event(&mut self, event: AppEvent) -> Result<()> {
        match event {
            AppEvent::MessageReceived(msg) => {
                self.session_manager.session.add_message(msg.clone());
                self.chat.add_message(msg);
            }
            AppEvent::MessageUpdate { agent_id, content } => {
                if let Some(last_message) = self.session_manager.session.messages.last_mut() {
                    if last_message.role == MessageRole::Agent && last_message.agent_id.as_deref() == Some(agent_id.as_str()) {
                        last_message.content.push_str(&content);
                    } else {
                        // Last message is not from the same agent, create a new one
                        let msg = Message::agent(&content, &agent_id);
                        self.session_manager.session.add_message(msg);
                    }
                } else {
                    // No messages yet, create a new one
                    let msg = Message::agent(&content, &agent_id);
                    self.session_manager.session.add_message(msg);
                }
            }
            AppEvent::TaskStatusChanged(task_id, status) => {
                debug!("Task {} status changed to {:?}", task_id, status);
                self.task_state.task_id = Some(task_id);
                self.task_state.status = status;
            }
            AppEvent::TaskCompleted => {
                debug!("Task execution completed");
                self.task_state.clear();
            }
            AppEvent::TaskSuccess(result) => {
                debug!("Task succeeded with result");
                if result.success {
                    let msg = Message::agent(&result.output, "task");
                    self.session_manager.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                }
                self.task_state.complete(result);
            }
            AppEvent::AutoResult(result) => {
                debug!("Auto-orchestration result received");
                if result.success {
                    let msg = Message::agent(&result.output, "orchestrator");
                    self.session_manager.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                } else {
                    let error_msg = result.error.unwrap_or_else(|| "Unknown routing error".to_string());
                    let msg = Message::system(&format!("Auto-orchestration failed: {}", error_msg));
                    self.session_manager.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                }
                self.task_state.clear();
            }
            AppEvent::RoutingDecision(decision) => {
                info!(
                    "Routing decision: {:?} with confidence {}",
                    decision.selected_agents, decision.confidence
                );
                // TODO: Handle routing decision
            }
            AppEvent::Error(msg) => {
                error!("Application error: {}", msg);
                self.chat.add_message(Message::system(&format!("Error: {}", msg)));
                self.task_state.fail(msg);
            }
            AppEvent::Status(msg) => {
                info!("Status: {}", msg);
                // Optionally display status messages in chat
                if msg.starts_with("Agent") && (msg.contains("started") || msg.contains("completed")) {
                    self.chat.add_message(Message::system(&msg));
                }
            }
            AppEvent::AgentStateChanged(agent_id, state) => {
                debug!("Agent {} state changed to {:?}", agent_id, state);
                let mut registry = self.agent_registry.write().await;
                if let Some(agent) = registry.get_mut(&agent_id) {
                    agent.state = state;
                }
            }
        }
        Ok(())
    }

    /// Handle agent events
    async fn handle_agent_event(&mut self, event: AgentEvent) -> Result<()> {
        match event {
            AgentEvent::Started { agent_id } => {
                debug!("Agent {} started processing", agent_id);
                self.chat.set_streaming(true);
                if let Err(e) = self.event_tx.send(AppEvent::Status(format!("Agent {} started", agent_id))).await {
                    warn!("Failed to send agent started status: {}", e);
                }
            }
            AgentEvent::Completed { agent_id, result } => {
                debug!("Agent {} completed processing", agent_id);
                self.chat.set_streaming(false);
                if result.success {
                    let _ = self.event_tx.send(AppEvent::Status(format!("Agent {} completed successfully", agent_id))).await;
                    let _ = self.event_tx.send(AppEvent::TaskCompleted).await;
                } else {
                    let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
                    if let Err(e) = self.event_tx.send(AppEvent::Error(format!("Agent {} failed: {}", agent_id, error_msg))).await {
                        warn!("Failed to send agent failed error: {}", e);
                    }
                }
            }
            AgentEvent::Message { agent_id, content } => {
                if let Err(e) = self.event_tx.send(AppEvent::MessageUpdate { agent_id, content }).await {
                    warn!("Failed to send message update: {}", e);
                }
            }
            AgentEvent::Error { agent_id, error } => {
                self.chat.set_streaming(false);
                error!("Agent {} error: {}", agent_id, error);
                if let Err(e) = self.event_tx.send(AppEvent::Error(format!("Agent {} error: {}", agent_id, error))).await {
                    warn!("Failed to send agent error: {}", e);
                }
            }
            AgentEvent::StateChanged { agent_id, state } => {
                if let Err(e) = self.event_tx.send(AppEvent::AgentStateChanged(agent_id, state)).await {
                    warn!("Failed to send agent state changed: {}", e);
                }
            }
            AgentEvent::ConfirmationRequest { agent_id, title, message, changes, response_tx } => {
                debug!("Agent {} requested confirmation: {}", agent_id, title);
                self.pending_confirmation = Some(PendingConfirmation {
                    title: title.clone(),
                    message: message.clone(),
                    changes,
                    response_tx,
                });
                self.mode = AppMode::Confirm;
                self.add_system_message(&format!("Agent {} is requesting confirmation for file operations.", agent_id));
            }
        }
        Ok(())
    }

    /// Handle tick event
    async fn on_tick(&mut self) -> Result<()> {
        // Update session list and handle auto-save via SessionManager
        let _ = self.session_manager.on_tick(Duration::from_secs(self.config.persistence.auto_save_interval)).await;

        // Update cached agent list
        {
            let registry = self.agent_registry.read().await;
            self.cached_agents = registry.list().to_vec();
        }

        // Fetch memory keys if in MemoryManager mode
        if self.mode == AppMode::MemoryManager {
            let _ = self.session_manager.refresh_memory_keys().await;
            let _ = self.session_manager.refresh_selected_memory_value().await;
        }

        // Update sidebar refresh timestamp
        self.sidebar.set_last_refresh(chrono::Local::now());

        Ok(())
    }

    /// Select an agent by index from the cached agents list
    fn select_agent_by_index(&mut self, idx: usize) {
        if idx < self.cached_agents.len() {
            let agent = &self.cached_agents[idx];
            self.active_agent = Some(agent.clone());
            self.add_system_message(&format!("Selected agent: {} ({})", agent.name, agent.role.as_str()));
        }
    }

    /// Helper: add a system message to both session and chat
    fn add_system_message(&mut self, text: &str) {
        let msg = Message::system(text);
        self.session_manager.session.add_message(msg.clone());
        self.chat.add_message(msg);
    }

    /// Submit the current input
    async fn submit_input(&mut self) -> Result<()> {
        let content = self.input.get_content();
        if content.trim().is_empty() {
            return Ok(());
        }

        // Check if a task is already running
        if self.task_state.is_running() {
            let error_msg = Message::system("A task is already running. Use /cancel or press Ctrl+X to cancel it first.");
            self.session_manager.session.add_message(error_msg.clone());
            self.chat.add_message(error_msg);
            return Ok(());
        }

        // Add user message
        let msg = Message::user(&content);
        self.session_manager.session.add_message(msg.clone());
        self.chat.add_message(msg);

        // Add to input history
        self.input.add_to_history(&content);

        // Clear input
        self.input.clear();

        // Check if we have an orchestrator
        if let Some(orchestrator) = &self.orchestrator {
            // Get message history for context (last 10 messages)
            let history: Vec<Message> = self
                .session_manager.session
                .messages
                .iter()
                .rev()
                .take(10)
                .rev()
                .cloned()
                .collect();

            let session_clone = self.session_manager.session.clone();
            let event_tx = self.event_tx.clone();
            let orchestrator = orchestrator.clone();

            match self.session_manager.session.mode {
                SessionMode::Manual => {
                    // Use the currently selected agent
                    if let Some(agent) = &self.active_agent {
                        let agent = agent.clone();
                        let agent_name = agent.name.clone();

                        // Show that we're processing
                        let processing_msg = Message::system(&format!("Agent '{}' is processing...", agent_name));
                        self.session_manager.session.add_message(processing_msg.clone());
                        self.chat.add_message(processing_msg);

                        // Clone ID for task state before moving into async block
                        let session_id = session_clone.id.clone();

                        // Spawn the agent execution in a background task
                        let handle = tokio::spawn(async move {
                            let result = orchestrator.execute_chat_streaming(agent, content, history, &session_clone.id).await;

                            if let Err(e) = result {
                                let _ = event_tx.send(AppEvent::Error(format!(
                                    "Agent execution failed: {}", e
                                ))).await;
                            }
                        });
                        self.task_state.start(session_id, handle);
                    } else {
                        let error_msg = Message::system("No agent selected. Use /agent <name> to select an agent.");
                        self.session_manager.session.add_message(error_msg.clone());
                        self.chat.add_message(error_msg);
                    }
                }
                SessionMode::Auto => {
                    // Show that we're routing
                    let routing_msg = Message::system("Analyzing task and routing to appropriate agent...");
                    self.session_manager.session.add_message(routing_msg.clone());
                    self.chat.add_message(routing_msg);

                    // Create a task
                    let task = Task::new(&content, TaskType::General);
                    let task_id = task.id.clone();

                    // Spawn the auto-orchestration in a background task
                    let handle = tokio::spawn(async move {
                        let result = orchestrator.execute_auto(task, &session_clone).await;

                        match result {
                            Ok(res) => {
                                if res.success {
                                    let _ = event_tx.send(AppEvent::TaskSuccess(res)).await;
                                } else {
                                    let error_msg = res.error.unwrap_or_else(|| "Unknown routing error".to_string());
                                    let _ = event_tx.send(AppEvent::Error(format!(
                                        "Auto-routing failed: {}", error_msg
                                    ))).await;
                                }
                            }
                            Err(e) => {
                                let _ = event_tx.send(AppEvent::Error(format!(
                                    "Auto-orchestration failed: {}", e
                                ))).await;
                            }
                        }
                    });
                    self.task_state.start(task_id, handle);
                }
            }
        } else {
            let response = Message::system(
                "No LLM client configured. Please set OPENAI_API_KEY environment variable or configure api_key in ~/.config/agent-tui/config.toml"
            );
            self.session_manager.session.add_message(response.clone());
            self.chat.add_message(response);
        }

        Ok(())
    }

    /// Cancel the currently running task
    async fn cancel_current_task(&mut self) -> Result<()> {
        if self.task_state.is_running() {
            self.task_state.cancel();
            let msg = Message::system("Task cancelled by user.");
            self.session_manager.session.add_message(msg.clone());
            self.chat.add_message(msg);
            
            // Shutdown any running agents in the orchestrator
            if let Some(orchestrator) = &self.orchestrator {
                let _ = orchestrator.executor().shutdown_all().await;
            }
            
            info!("Cancelled running task");
        } else {
            let msg = Message::system("No task is currently running.");
            self.session_manager.session.add_message(msg.clone());
            self.chat.add_message(msg);
        }
        Ok(())
    }

    /// Execute a slash command
    async fn execute_command(&mut self) -> Result<()> {
        use self::command_handler::CommandHandler;
        CommandHandler::execute(self).await
    }

    /// Draw the UI
    fn draw(&mut self, frame: &mut Frame) {
        use self::popups::PopupRenderer;
        use self::theme;

        frame.render_widget(
            Block::default().style(Style::default().bg(theme::app_bg())),
            frame.area(),
        );

        let shell = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
            .margin(1)
            .split(frame.area());

        PopupRenderer::draw_header(
            frame,
            &self.session_manager.session,
            self.task_state.is_running(),
            self.active_agent.as_ref(),
        );

        let main_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(if self.show_sidebar {
                vec![Constraint::Length(30), Constraint::Min(0)]
            } else {
                vec![Constraint::Percentage(0), Constraint::Percentage(100)]
            })
            .split(shell[1]);

        // Sidebar
        if self.show_sidebar {
            let agents: Vec<_> = self.cached_agents.clone();
            self.sidebar.focused = self.mode == AppMode::Sidebar;
            self.sidebar.draw(frame, main_layout[0], &self.session_manager.session, &agents, self.active_agent.as_ref(), &self.session_manager.sessions);
        }

        // Main content area
        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(4)])
            .margin(1)
            .split(main_layout[1]);

        // Chat area
        self.chat.draw(frame, content_layout[0], &self.session_manager.session);

        // Input area
        self.input.draw(frame, content_layout[1], self.mode);

        // Draw overlays based on mode
        match self.mode {
            AppMode::Command => {
                let suggestions = self.filtered_commands();
                let selected_index = self
                    .command_selected_index
                    .min(suggestions.len().saturating_sub(1));
                PopupRenderer::draw_command_suggestions(frame, &suggestions, selected_index);
            }
            AppMode::AgentSelect => {
                PopupRenderer::draw_agent_selector(frame, &self.cached_agents, self.agent_selected_index);
            }
            AppMode::MemoryManager => {
                PopupRenderer::draw_memory_manager(
                    frame,
                    &self.session_manager.memory_keys,
                    self.session_manager.selected_memory_key,
                    &self.session_manager.cached_memory_values,
                );
            }
            AppMode::Confirm => {
                PopupRenderer::draw_confirmation_dialog(frame, self.pending_confirmation.as_ref());
            }
            _ => {}
        }

        // Status bar
        PopupRenderer::draw_status_bar(frame, &self.session_manager.session);
    }

    /// Draw agent selector popup
    fn draw_agent_selector(&self, _frame: &mut Frame) {
        // Now delegated to PopupRenderer
    }

    /// Draw confirmation dialog popup
    fn draw_confirmation_dialog(&self, _frame: &mut Frame) {
        // Now delegated to PopupRenderer
    }

    /// Draw memory manager popup
    fn draw_memory_manager(&self, _frame: &mut Frame) {
        // Now delegated to PopupRenderer
    }

    /// Draw status bar
    fn draw_status_bar(&self, _frame: &mut Frame) {
        // Now delegated to PopupRenderer
    }

    fn command_catalog() -> &'static [(&'static str, &'static str)] {
        &[
            ("/help", "Show help"),
            ("/mode auto", "Enable automatic routing"),
            ("/mode manual", "Enable manual agent selection"),
            ("/agent", "Select or inspect an agent"),
            ("/agents", "List available agents"),
            ("/clear", "Clear current session"),
            ("/new", "Start a new session"),
            ("/save", "Save current session"),
            ("/load", "Load a saved session"),
            ("/sessions", "List saved sessions"),
            ("/delete", "Delete a saved session"),
            ("/remember", "Store a memory value"),
            ("/recall", "Fetch a memory value"),
            ("/forget", "Delete a memory value"),
            ("/cancel", "Cancel the running task"),
            ("/quit", "Exit the app"),
        ]
    }

    fn filtered_commands(&self) -> Vec<(&'static str, &'static str)> {
        let query = self.input.get_content().trim().to_lowercase();
        let slash_query = if query.starts_with('/') {
            query.clone()
        } else {
            format!("/{}", query)
        };

        let mut commands: Vec<_> = Self::command_catalog()
            .iter()
            .copied()
            .filter(|(command, _)| query.is_empty() || command.starts_with(&slash_query))
            .collect();

        if commands.is_empty() {
            commands = Self::command_catalog().to_vec();
        }

        commands
    }

    fn autocomplete_selected_command(&mut self) {
        let suggestions = self.filtered_commands();
        if suggestions.is_empty() {
            return;
        }

        let selected_index = self
            .command_selected_index
            .min(suggestions.len().saturating_sub(1));
        let (command, _) = suggestions[selected_index];
        let trimmed = command.trim_start_matches('/');
        self.input.set_content(trimmed);
    }

    /// Calculate centered rectangle
    fn centered_rect(&self, percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        use self::popups::PopupRenderer;
        PopupRenderer::centered_rect(percent_x, percent_y, r)
    }
}
