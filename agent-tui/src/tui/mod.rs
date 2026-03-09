pub mod components;
pub mod markdown;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Modifier},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use std::sync::Arc;

use crate::{
    agent::{AgentEvent},
    agent::AgentRegistry,
    config::Config,
    llm::LlmClient,
    orchestrator::Orchestrator,
    types::{AppEvent, Message, MessageRole, Session, SessionMode, Agent, Task, TaskType},
};

use self::components::{Chat, Input, Sidebar};
use crate::persistence::{SessionStore, MemoryStore, SessionMetadata};
use crate::shared::SharedMemory;

/// Main application state
pub struct App {
    /// Application configuration
    config: Config,
    /// Current session
    session: Session,
    /// Saved sessions metadata
    sessions: Vec<SessionMetadata>,
    /// Chat component
    chat: Chat,
    /// Input component
    input: Input,
    /// Sidebar component
    sidebar: Sidebar,
    /// LLM client (used by orchestrator; retained for direct-access use cases)
    #[allow(dead_code)]
    llm_client: Option<Arc<LlmClient>>,
    /// Orchestrator for task execution
    orchestrator: Option<Arc<Orchestrator>>,
    /// Agent registry
    agent_registry: Arc<RwLock<AgentRegistry>>,
    /// Shared memory for agents (used by orchestrator; retained for direct-access use cases)
    #[allow(dead_code)]
    shared_memory: Arc<SharedMemory>,
    /// Session store for persistence
    session_store: Arc<SessionStore>,
    /// Memory store for persistence
    memory_store: Arc<MemoryStore>,
    /// Active agent for manual mode
    active_agent: Option<Agent>,
    /// Agent event receiver
    agent_event_rx: mpsc::Receiver<AgentEvent>,
    /// Agent event sender (cloned into orchestrator; retained for future direct use)
    #[allow(dead_code)]
    agent_event_tx: mpsc::Sender<AgentEvent>,
    /// Whether the app should quit
    should_quit: bool,
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
    /// Last auto-save time
    last_save: Instant,
    /// Last session list refresh time
    last_session_refresh: Instant,
    /// Memory keys for memory manager
    memory_keys: Vec<String>,
    /// Selected memory key index
    selected_memory_key: usize,
    /// Handle to the currently running task (for cancellation)
    current_task_handle: Option<tokio::task::JoinHandle<()>>,
    /// Cached agent list for UI rendering
    cached_agents: Vec<Agent>,
    /// Selected agent index for agent selector (separate from sidebar)
    agent_selected_index: usize,
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
        
        let session = Session::new("New Session");
        
        // Initialize LLM client if API key is available
        let llm_client = if config.llm.api_key.starts_with("$") {
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
        let session_store = Arc::new(SessionStore::new(config.session_dir()));
        let memory_store = Arc::new(MemoryStore::new(config.memory_dir()));
        
        // Initialize shared memory
        let shared_memory = Arc::new(SharedMemory::new());
        
        // Initialize session list (empty for now, will be loaded on tick or first draw)
        let sessions = Vec::new();
        
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
            ))
        });
        
        Ok(Self {
            config: config.clone(),
            session: session.clone(),
            sessions,
            chat: Chat::new(),
            input: Input::new(),
            sidebar: Sidebar::new(),
            llm_client,
            orchestrator,
            agent_registry,
            shared_memory,
            session_store,
            memory_store,
            active_agent,
            agent_event_rx,
            agent_event_tx,
            should_quit: false,
            show_sidebar: true,
            mode: AppMode::Normal,
            event_rx,
            event_tx,
            tick_rate: Duration::from_millis(250),
            last_save: Instant::now(),
            last_session_refresh: Instant::now(),
            memory_keys: Vec::new(),
            selected_memory_key: 0,
            current_task_handle: None,
            cached_agents: Vec::new(),
            agent_selected_index: 0,
        })
    }

    /// Run the main application loop
    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Main loop — use a closure to ensure terminal restoration even on panic
        let result = self.run_loop(&mut terminal).await;

        // Restore terminal (always runs, even if run_loop errored)
        Self::restore_terminal(&mut terminal)?;

        result
    }

    /// Restore terminal to normal state. Called on both normal exit and cleanup.
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
                            self.sidebar.next_session(self.sessions.len());
                        }
                        KeyCode::Char('l') => {
                            // Load selected session from sidebar
                            let idx = self.sidebar.selected_session();
                            if idx < self.sessions.len() {
                                let session_id = self.sessions[idx].id.clone();
                                self.load_session(&session_id).await;
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
                            } else {
                                self.input.autocomplete();
                            }
                        }
                        KeyCode::Char('/') if self.input.is_empty() => {
                            // Enter command mode without inserting the slash
                            self.mode = AppMode::Command;
                        }
                        KeyCode::Char(c) => {
                            self.input.insert_char(c);
                        }
                        KeyCode::Backspace => {
                            self.input.delete_char();
                            if self.input.is_empty() {
                                self.mode = AppMode::Normal;
                            }
                        }
                        KeyCode::Left => {
                            self.input.move_cursor_left();
                        }
                        KeyCode::Right => {
                            self.input.move_cursor_right();
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
                KeyCode::Char(c) => {
                    self.input.insert_char(c);
                }
                KeyCode::Backspace => {
                    self.input.delete_char();
                    if self.input.is_empty() {
                        self.mode = AppMode::Normal;
                    }
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
                    if self.selected_memory_key > 0 {
                        self.selected_memory_key -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('n') => {
                    if self.selected_memory_key < self.memory_keys.len().saturating_sub(1) {
                        self.selected_memory_key += 1;
                    }
                }
                KeyCode::Char('r') => {
                    if let Ok(keys) = self.memory_store.list("session").await {
                        self.memory_keys = keys;
                        info!("Manual memory refresh");
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
                    self.sidebar.next_session(self.sessions.len());
                }
                KeyCode::Char('r') => {
                    self.refresh_session_list().await;
                    info!("Manual sidebar refresh");
                }
                KeyCode::Char('l') | KeyCode::Enter => {
                    // Load selected session from sidebar
                    let idx = self.sidebar.selected_session();
                    if idx < self.sessions.len() {
                        let session_id = self.sessions[idx].id.clone();
                        self.load_session(&session_id).await;
                        self.mode = AppMode::Normal;
                    }
                }
                _ => {}
            },
            AppMode::Confirm => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    // TODO: Confirm action
                    self.mode = AppMode::Normal;
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    // TODO: Cancel action
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
                self.session.add_message(msg.clone());
                self.chat.add_message(msg);
            }
            AppEvent::MessageUpdate { agent_id, content } => {
                if let Some(last_message) = self.session.messages.last_mut() {
                    if last_message.role == MessageRole::Agent && last_message.agent_id.as_deref() == Some(agent_id.as_str()) {
                        last_message.content.push_str(&content);
                    } else {
                        // Last message is not from the same agent, create a new one
                        let msg = Message::agent(&content, &agent_id);
                        self.session.add_message(msg);
                    }
                } else {
                    // No messages yet, create a new one
                    let msg = Message::agent(&content, &agent_id);
                    self.session.add_message(msg);
                }
            }
            AppEvent::TaskStatusChanged(task_id, status) => {
                debug!("Task {} status changed to {:?}", task_id, status);
                // TODO: Update task tracking
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
                let _ = self.event_tx.send(AppEvent::Status(format!("Agent {} started", agent_id))).await;
            }
            AgentEvent::Completed { agent_id, result } => {
                debug!("Agent {} completed processing", agent_id);
                self.chat.set_streaming(false);
                if result.success {
                    let msg = Message::agent(&result.output, &agent_id);
                    self.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                } else {
                    let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
                    let msg = Message::system(&format!("Agent {} failed: {}", agent_id, error_msg));
                    self.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                }
            }
            AgentEvent::Message { agent_id, content } => {
                let _ = self.event_tx.send(AppEvent::MessageUpdate { agent_id, content }).await;
            }
            AgentEvent::Error { agent_id, error } => {
                self.chat.set_streaming(false);
                error!("Agent {} error: {}", agent_id, error);
                let msg = Message::system(&format!("Agent {} error: {}", agent_id, error));
                self.session.add_message(msg.clone());
                self.chat.add_message(msg);
            }
            AgentEvent::StateChanged { agent_id, state } => {
                let _ = self.event_tx.send(AppEvent::AgentStateChanged(agent_id, state)).await;
            }
        }
        Ok(())
    }

    /// Handle tick event
    async fn on_tick(&mut self) -> Result<()> {
        // Update session list periodically (every 10 seconds, not every tick)
        if self.last_session_refresh.elapsed() >= Duration::from_secs(10) {
            self.refresh_session_list().await;
        }

        // Update cached agent list
        {
            let registry = self.agent_registry.read().await;
            self.cached_agents = registry.list().to_vec();
        }

        // Fetch memory keys if in MemoryManager mode
        if self.mode == AppMode::MemoryManager {
            if let Ok(keys) = self.memory_store.list("session").await {
                self.memory_keys = keys;
            }
        }

        // Handle auto-save
        let auto_save_interval = Duration::from_secs(self.config.persistence.auto_save_interval);
        if self.last_save.elapsed() >= auto_save_interval {
            if !self.session.messages.is_empty() {
                debug!("Auto-saving session...");
                if let Err(e) = self.session_store.save(&self.session).await {
                    error!("Failed to auto-save session: {}", e);
                }
            }
            self.last_save = Instant::now();
        }

        Ok(())
    }

    /// Refresh the session list from disk
    async fn refresh_session_list(&mut self) {
        if let Ok(sessions) = self.session_store.list().await {
            self.sessions = sessions;
            self.sidebar.set_last_refresh(chrono::Local::now());
        }
        self.last_session_refresh = Instant::now();
    }

    /// Load a session by ID, replacing the current session
    async fn load_session(&mut self, session_id: &str) {
        match self.session_store.load(session_id).await {
            Ok(session) => {
                self.session = session;
                self.chat.clear();
                for msg in &self.session.messages {
                    self.chat.add_message(msg.clone());
                }
                self.add_system_message(&format!("Session '{}' loaded.", self.session.title));
                info!("Loaded session {}", session_id);
            }
            Err(e) => {
                self.add_system_message(&format!("Failed to load session: {}", e));
                error!("Failed to load session {}: {}", session_id, e);
            }
        }
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
        self.session.add_message(msg.clone());
        self.chat.add_message(msg);
    }

    /// Submit the current input
    async fn submit_input(&mut self) -> Result<()> {
        let content = self.input.get_content();
        if content.trim().is_empty() {
            return Ok(());
        }

        // Check if a task is already running
        if self.current_task_handle.is_some() {
            let error_msg = Message::system("A task is already running. Use /cancel or press Ctrl+X to cancel it first.");
            self.session.add_message(error_msg.clone());
            self.chat.add_message(error_msg);
            return Ok(());
        }

        // Add user message
        let msg = Message::user(&content);
        self.session.add_message(msg.clone());
        self.chat.add_message(msg);

        // Clear input
        self.input.clear();

        // Check if we have an orchestrator
        if let Some(orchestrator) = &self.orchestrator {
            // Get message history for context (last 10 messages)
            let history: Vec<Message> = self
                .session
                .messages
                .iter()
                .rev()
                .take(10)
                .rev()
                .cloned()
                .collect();

            let session_clone = self.session.clone();
            let event_tx = self.event_tx.clone();
            let orchestrator = orchestrator.clone();

            match self.session.mode {
                SessionMode::Manual => {
                    // Use the currently selected agent
                    if let Some(agent) = &self.active_agent {
                        let agent = agent.clone();
                        let agent_name = agent.name.clone();

                        // Show that we're processing
                        let processing_msg = Message::system(&format!("Agent '{}' is processing...", agent_name));
                        self.session.add_message(processing_msg.clone());
                        self.chat.add_message(processing_msg);

                        // Spawn the agent execution in a background task
                        let handle = tokio::spawn(async move {
                            let result = orchestrator.execute_chat_streaming(agent, content, history, &session_clone.id).await;

                            if let Err(e) = result {
                                let _ = event_tx.send(AppEvent::Error(format!(
                                    "Agent execution failed: {}", e
                                ))).await;
                            }
                        });
                        self.current_task_handle = Some(handle);
                    } else {
                        let error_msg = Message::system("No agent selected. Use /agent <name> to select an agent.");
                        self.session.add_message(error_msg.clone());
                        self.chat.add_message(error_msg);
                    }
                }
                SessionMode::Auto => {
                    // Show that we're routing
                    let routing_msg = Message::system("Analyzing task and routing to appropriate agent...");
                    self.session.add_message(routing_msg.clone());
                    self.chat.add_message(routing_msg);

                    // Create a task
                    let task = Task::new(&content, TaskType::General);

                    // Spawn the auto-orchestration in a background task
                    let handle = tokio::spawn(async move {
                        let result = orchestrator.execute_auto(task, &session_clone).await;

                        match result {
                            Ok(res) => {
                                if !res.success {
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
                    self.current_task_handle = Some(handle);
                }
            }
        } else {
            let response = Message::system(
                "No LLM client configured. Please set OPENAI_API_KEY environment variable or configure api_key in ~/.config/agent-tui/config.toml"
            );
            self.session.add_message(response.clone());
            self.chat.add_message(response);
        }

        Ok(())
    }

    /// Cancel the currently running task
    async fn cancel_current_task(&mut self) -> Result<()> {
        if let Some(handle) = self.current_task_handle.take() {
            handle.abort();
            let msg = Message::system("Task cancelled by user.");
            self.session.add_message(msg.clone());
            self.chat.add_message(msg);
            
            // Shutdown any running agents in the orchestrator
            if let Some(orchestrator) = &self.orchestrator {
                let _ = orchestrator.executor().shutdown_all().await;
            }
            
            info!("Cancelled running task");
        } else {
            let msg = Message::system("No task is currently running.");
            self.session.add_message(msg.clone());
            self.chat.add_message(msg);
        }
        Ok(())
    }

    /// Execute a slash command
    async fn execute_command(&mut self) -> Result<()> {
        let raw_content = self.input.get_content();
        // Prepend "/" because command mode entry consumes the slash character
        let content = if raw_content.starts_with('/') {
            raw_content
        } else {
            format!("/{}", raw_content)
        };
        let parts: Vec<&str> = content.split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(());
        }

        let command = parts[0];
        let args = &parts[1..];

        match command {
            "/help" | "help" => {
                let help_text = r#"Available commands:
/help - Show this help message
/mode auto - Enable automatic agent routing
/mode manual - Enable manual agent selection
/agent <name> - Select specific agent (manual mode)
/agents - List all available agents
/clear - Clear current session
/new - Start a new session
/save <name> - Save current session to file
/load <id> - Load a session by ID
/sessions - List all saved sessions
/delete <id> - Delete a session by ID
/remember <key> <value> - Store a value in session memory
/recall <key> - Retrieve a value from session memory
/forget <key> - Delete a value from session memory
/cancel - Cancel the currently running task
/quit - Exit application"#;
                let msg = Message::system(help_text);
                self.session.add_message(msg.clone());
                self.chat.add_message(msg);
            }
            "/mode" => {
                if let Some(mode) = args.first() {
                    match *mode {
                        "auto" => {
                            self.session.set_mode(SessionMode::Auto);
                            let msg = Message::system("Switched to AUTO mode. Agents will be selected automatically.");
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                        "manual" => {
                            self.session.set_mode(SessionMode::Manual);
                            let msg = Message::system("Switched to MANUAL mode. Use /agent <name> to select an agent.");
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                        _ => {
                            let msg = Message::system(&format!("Unknown mode: {}. Use 'auto' or 'manual'.", mode));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                    }
                }
            }
            "/agent" => {
                if let Some(agent_name) = args.first() {
                    let registry = self.agent_registry.read().await;
                    let agent_opt = registry.get(agent_name).cloned();
                    let available: Vec<String> = registry.list()
                        .iter()
                        .map(|a| a.name.clone())
                        .collect();
                    drop(registry);

                    if let Some(agent) = agent_opt {
                        self.active_agent = Some(agent.clone());
                        let msg = Message::system(&format!("Selected agent: {} ({})", agent_name, agent.role.as_str()));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    } else {
                        let msg = Message::system(&format!(
                            "Unknown agent: {}. Available agents: {}",
                            agent_name,
                            available.join(", ")
                        ));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    }
                } else {
                    // Show current agent or list available
                    if let Some(agent) = &self.active_agent {
                        let msg = Message::system(&format!(
                            "Current agent: {} ({})",
                            agent.name,
                            agent.role.as_str()
                        ));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    } else {
                        let registry = self.agent_registry.read().await;
                        let available: Vec<String> = registry.list()
                            .iter()
                            .map(|a| format!("{} ({})", a.name, a.role.as_str()))
                            .collect();
                        drop(registry);
                        let msg = Message::system(&format!(
                            "No agent selected. Available agents: {}",
                            available.join(", ")
                        ));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    }
                }
            }
            "/agents" => {
                let registry = self.agent_registry.read().await;
                let agents: Vec<String> = registry.list()
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let status = if self.active_agent.as_ref().map(|active| active.id == a.id).unwrap_or(false) {
                            " [ACTIVE]"
                        } else {
                            ""
                        };
                        format!("{}. {} ({}){}", i + 1, a.name, a.role.as_str(), status)
                    })
                    .collect();
                drop(registry);
                let msg = Message::system(&format!("Available agents:\n{}", agents.join("\n")));
                self.session.add_message(msg.clone());
                self.chat.add_message(msg);
            }
            "/clear" => {
                self.session.messages.clear();
                self.chat.clear();
                let msg = Message::system("Session cleared.");
                self.session.add_message(msg.clone());
                self.chat.add_message(msg);
            }
            "/new" => {
                self.session = Session::new("New Session");
                self.chat.clear();
                let registry = self.agent_registry.read().await;
                self.active_agent = registry.get("coder").cloned();
                drop(registry);
                let msg = Message::system("Started new session.");
                self.session.add_message(msg.clone());
                self.chat.add_message(msg);
            }
            "/save" => {
                if !args.is_empty() {
                    self.session.title = args.join(" ");
                }
                match self.session_store.save(&self.session).await {
                    Ok(_) => {
                        let msg = Message::system(&format!("Session '{}' saved successfully (ID: {}).", self.session.title, self.session.id));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    }
                    Err(e) => {
                        let msg = Message::system(&format!("Failed to save session: {}", e));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    }
                }
            }
            "/load" => {
                if let Some(id) = args.first() {
                    self.load_session(id).await;
                } else {
                    self.add_system_message("Usage: /load <session_id>");
                }
            }
            "/sessions" => {
                match self.session_store.list().await {
                    Ok(sessions) => {
                        if sessions.is_empty() {
                            let msg = Message::system("No saved sessions found.");
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        } else {
                            let mut list = String::from("Saved sessions:\n");
                            for s in sessions {
                                list.push_str(&format!("- {}: {} ({} messages, {})\n", 
                                    s.id, s.title, s.message_count, s.updated_at.format("%Y-%m-%d %H:%M:%S")));
                            }
                            let msg = Message::system(&list);
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                    }
                    Err(e) => {
                        let msg = Message::system(&format!("Failed to list sessions: {}", e));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    }
                }
            }
            "/delete" => {
                if let Some(id) = args.first() {
                    match self.session_store.delete(id).await {
                        Ok(_) => {
                            let msg = Message::system(&format!("Session '{}' deleted successfully.", id));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                        Err(e) => {
                            let msg = Message::system(&format!("Failed to delete session: {}", e));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                    }
                } else {
                    let msg = Message::system("Usage: /delete <session_id>");
                    self.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                }
            }
            "/remember" => {
                if args.len() >= 2 {
                    let key = args[0];
                    let value = args[1..].join(" ");
                    match self.memory_store.set(key, &value, "session").await {
                        Ok(_) => {
                            let msg = Message::system(&format!("Stored in memory: {} = {}", key, value));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                        Err(e) => {
                            let msg = Message::system(&format!("Failed to store memory: {}", e));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                    }
                } else {
                    let msg = Message::system("Usage: /remember <key> <value>");
                    self.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                }
            }
            "/recall" => {
                if let Some(key) = args.first() {
                    match self.memory_store.get(key, "session").await {
                        Ok(Some(value)) => {
                            let msg = Message::system(&format!("Memory recall: {} = {}", key, value));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                        Ok(None) => {
                            let msg = Message::system(&format!("Key '{}' not found in memory.", key));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                        Err(e) => {
                            let msg = Message::system(&format!("Failed to recall memory: {}", e));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                    }
                } else {
                    let msg = Message::system("Usage: /recall <key>");
                    self.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                }
            }
            "/forget" => {
                if let Some(key) = args.first() {
                    match self.memory_store.delete(key, "session").await {
                        Ok(_) => {
                            let msg = Message::system(&format!("Forgotten: {}", key));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                        Err(e) => {
                            let msg = Message::system(&format!("Failed to forget memory: {}", e));
                            self.session.add_message(msg.clone());
                            self.chat.add_message(msg);
                        }
                    }
                } else {
                    let msg = Message::system("Usage: /forget <key>");
                    self.session.add_message(msg.clone());
                    self.chat.add_message(msg);
                }
            }
            "/cancel" => {
                self.cancel_current_task().await?;
            }
            "/quit" | "/exit" => {
                self.should_quit = true;
            }
            _ => {
                let msg = Message::system(&format!("Unknown command: {}. Type /help for available commands.", command));
                self.session.add_message(msg.clone());
                self.chat.add_message(msg);
            }
        }

        self.input.clear();
        Ok(())
    }

    /// Draw the UI
    fn draw(&mut self, frame: &mut Frame) {
        let main_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(if self.show_sidebar {
                vec![Constraint::Percentage(20), Constraint::Percentage(80)]
            } else {
                vec![Constraint::Percentage(0), Constraint::Percentage(100)]
            })
            .split(frame.area());

        // Sidebar
        if self.show_sidebar {
            let agents: Vec<_> = self.cached_agents.clone();
            self.sidebar.focused = self.mode == AppMode::Sidebar;
            self.sidebar.draw(frame, main_layout[0], &self.session, &agents, self.active_agent.as_ref(), &self.sessions);
        }

        // Main content area
        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(main_layout[1]);

        // Chat area
        self.chat.draw(frame, content_layout[0], &self.session);

        // Input area
        self.input.draw(frame, content_layout[1], self.mode);

        // Draw overlays based on mode
        match self.mode {
            AppMode::AgentSelect => {
                self.draw_agent_selector(frame);
            }
            AppMode::MemoryManager => {
                self.draw_memory_manager(frame);
            }
            _ => {}
        }

        // Status bar
        self.draw_status_bar(frame);
    }

    /// Draw agent selector popup
    fn draw_agent_selector(&self, frame: &mut Frame) {
        let area = Self::centered_rect(60, 60, frame.area());

        let block = Block::default()
            .title("Select Agent")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        // Build dynamic agent list from registry
        let mut text: Vec<Line> = vec![
            Line::from("Available agents:"),
            Line::from(""),
        ];

        for (idx, agent) in self.cached_agents.iter().enumerate() {
            let line = Line::from(format!(
                "{}. {} - {}",
                idx + 1,
                agent.name,
                agent.description
            ));
            text.push(line);
        }

        text.push(Line::from(""));
        text.push(Line::from("Press number to select, ESC to cancel"));

        let paragraph = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    /// Draw memory manager popup
    fn draw_memory_manager(&self, frame: &mut Frame) {
        let area = Self::centered_rect(80, 80, frame.area());
        
        let block = Block::default()
            .title("Memory Manager")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        if self.memory_keys.is_empty() {
            let text = vec![
                Line::from("No memory entries found."),
                Line::from(""),
                Line::from("Press ESC or 'q' to close, 'r' to refresh"),
            ];

            let paragraph = Paragraph::new(text)
                .block(block)
                .wrap(Wrap { trim: true });

            frame.render_widget(Clear, area);
            frame.render_widget(paragraph, area);
        } else {
            let list_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(area);

            let items: Vec<ListItem> = self.memory_keys
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    let style = if i == self.selected_memory_key {
                        Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(key.clone()).style(style)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().title(" Keys ").borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD));

            frame.render_widget(Clear, area);
            frame.render_widget(list, list_layout[0]);

            // Show value of selected key
            if let Some(key) = self.memory_keys.get(self.selected_memory_key) {
                // We need to fetch the value synchronously or have it pre-cached
                // For now, let's just say "Press Enter to view" or try to show it if we can
                // Since this is a draw call, we can't await. 
                // In a real app we'd have the values cached or use a reactive state.
                let value_text = "Value view not yet implemented (needs async fetch)";
                let paragraph = Paragraph::new(value_text)
                    .block(Block::default().title(format!(" Value: {} ", key)).borders(Borders::ALL))
                    .wrap(Wrap { trim: true });
                frame.render_widget(paragraph, list_layout[1]);
            }
        }
    }

    /// Draw status bar
    fn draw_status_bar(&self, frame: &mut Frame) {
        let status_area = Rect {
            x: frame.area().x,
            y: frame.area().height - 1,
            width: frame.area().width,
            height: 1,
        };

        let mode_text = match self.session.mode {
            SessionMode::Auto => "AUTO",
            SessionMode::Manual => "MANUAL",
        };

        let status = format!(
            " [{}] | Messages: {} | Ctrl+C: quit | Ctrl+B: sidebar | /help: commands ",
            mode_text,
            self.session.messages.len()
        );

        let status_bar = Paragraph::new(status)
            .style(Style::default().bg(Color::Blue).fg(Color::White));

        frame.render_widget(status_bar, status_area);
    }

    /// Calculate centered rectangle
    fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}
