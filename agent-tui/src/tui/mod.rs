pub mod components;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
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
    agent::{AgentEvent, AgentInstance, AgentRuntimeBuilder},
    agent::agents::{CoderAgent, PlannerAgent, ReviewerAgent, TesterAgent, ExplorerAgent, IntegratorAgent},
    agent::AgentRegistry,
    config::Config,
    llm::LlmClient,
    orchestrator::{Orchestrator, ExecutionContext},
    types::{AppEvent, Id, Message, MessageRole, Session, SessionMode, AgentState, Agent, Task, TaskType},
};

use self::components::{Chat, Input, Sidebar};

/// Main application state
pub struct App {
    /// Application configuration
    config: Config,
    /// Current session
    session: Session,
    /// Chat component
    chat: Chat,
    /// Input component
    input: Input,
    /// Sidebar component
    sidebar: Sidebar,
    /// LLM client
    llm_client: Option<Arc<LlmClient>>,
    /// Orchestrator for task execution
    orchestrator: Option<Arc<Orchestrator>>,
    /// Agent registry
    agent_registry: Arc<RwLock<AgentRegistry>>,
    /// Active agent for manual mode
    active_agent: Option<Agent>,
    /// Agent event receiver
    agent_event_rx: mpsc::Receiver<AgentEvent>,
    /// Agent event sender
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
    /// Last tick time
    last_tick: Instant,
    /// Tick rate
    tick_rate: Duration,
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
    /// Confirmation dialog
    Confirm,
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
        
        // Set default active agent to coder for manual mode
        let active_agent = agent_registry.get("coder").cloned();
        
        let agent_registry = Arc::new(RwLock::new(agent_registry));
        
        // Initialize orchestrator if we have an LLM client
        let orchestrator = llm_client.as_ref().map(|client| {
            Arc::new(Orchestrator::new(
                client.clone(),
                agent_registry.clone(),
                agent_event_tx.clone(),
                config.orchestration.max_concurrent_agents,
            ))
        });
        
        Ok(Self {
            config: config.clone(),
            session: session.clone(),
            chat: Chat::new(),
            input: Input::new(),
            sidebar: Sidebar::new(),
            llm_client,
            orchestrator,
            agent_registry,
            active_agent,
            agent_event_rx,
            agent_event_tx,
            should_quit: false,
            show_sidebar: true,
            mode: AppMode::Normal,
            event_rx,
            event_tx,
            last_tick: Instant::now(),
            tick_rate: Duration::from_millis(250),
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

        // Main loop
        let result = self.run_loop(&mut terminal).await;

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
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
                    if key.kind == KeyEventKind::Press {
                        self.handle_key_event(key).await?;
                    }
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
                            self.input.autocomplete();
                        }
                        KeyCode::Char('/') if self.input.is_empty() => {
                            self.input.insert_char('/');
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
                KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    let idx = (c.to_digit(10).unwrap() as usize).saturating_sub(1);
                    let agents = self.agent_registry.list();
                    
                    if idx < agents.len() {
                        let agent = &agents[idx];
                        self.active_agent = Some(agent.clone());
                        let msg = Message::system(&format!("Selected agent: {} ({})", agent.name, agent.role.as_str()));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    }
                    self.mode = AppMode::Normal;
                }
                _ => {}
            },
            AppMode::MemoryManager => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.mode = AppMode::Normal;
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
                if let Some(agent) = self.agent_registry.get_mut(&agent_id) {
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
                let _ = self.event_tx.send(AppEvent::Status(format!("Agent {} started", agent_id))).await;
            }
            AgentEvent::Completed { agent_id, result } => {
                debug!("Agent {} completed processing", agent_id);
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
        // TODO: Periodic tasks (auto-save, health checks, etc.)
        Ok(())
    }

    /// Submit the current input
    async fn submit_input(&mut self) -> Result<()> {
        let content = self.input.get_content();
        if content.trim().is_empty() {
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
                        tokio::spawn(async move {
                            let result = orchestrator.execute_chat_streaming(agent, content, history).await;
                            
                            if let Err(e) = result {
                                let _ = event_tx.send(AppEvent::Error(format!(
                                    "Agent execution failed: {}", e
                                ))).await;
                            }
                        });
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
                    tokio::spawn(async move {
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

    /// Execute a slash command
    async fn execute_command(&mut self) -> Result<()> {
        let content = self.input.get_content();
        let parts: Vec<&str> = content.split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(());
        }

        let command = parts[0];
        let args = &parts[1..];

        match command {
            "/help" => {
                let help_text = r#"Available commands:
/help - Show this help message
/mode auto - Enable automatic agent routing
/mode manual - Enable manual agent selection
/agent <name> - Select specific agent (manual mode)
/clear - Clear current session
/new - Start new session
/sessions - List saved sessions
/memory - Open memory manager
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
                    if let Some(agent) = self.agent_registry.get(agent_name) {
                        self.active_agent = Some(agent.clone());
                        let msg = Message::system(&format!("Selected agent: {} ({})", agent_name, agent.role.as_str()));
                        self.session.add_message(msg.clone());
                        self.chat.add_message(msg);
                    } else {
                        let available: Vec<String> = self.agent_registry.list()
                            .iter()
                            .map(|a| a.name.clone())
                            .collect();
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
                        let available: Vec<String> = self.agent_registry.list()
                            .iter()
                            .map(|a| format!("{} ({})", a.name, a.role.as_str()))
                            .collect();
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
                let agents: Vec<String> = self.agent_registry.list()
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
                self.active_agent = self.agent_registry.get("coder").cloned();
                let msg = Message::system("Started new session.");
                self.session.add_message(msg.clone());
                self.chat.add_message(msg);
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
            let agents: Vec<_> = self.agent_registry.list().to_vec();
            self.sidebar.draw(frame, main_layout[0], &self.session, &agents, self.active_agent.as_ref());
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

        for (idx, agent) in self.agent_registry.list().iter().enumerate() {
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

        let text = vec![
            Line::from("Memory management not yet implemented."),
            Line::from(""),
            Line::from("Press ESC or 'q' to close"),
        ];

        let paragraph = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
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
            " [{}] | Messages: {} | Press Ctrl+C to quit | Ctrl+B: toggle sidebar | Ctrl+H: help ",
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
