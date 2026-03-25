use anyhow::Result;
use crate::types::SessionMode;
use crate::tui::App;

/// Handles slash commands for the application
pub struct CommandHandler;

impl CommandHandler {
    /// Execute a slash command
    pub async fn execute(app: &mut App) -> Result<()> {
        let raw_content = app.input.get_content();
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
                app.add_system_message(help_text);
            }
            "/mode" => {
                if let Some(mode) = args.first() {
                    match *mode {
                        "auto" => {
                            app.session_manager.session.set_mode(SessionMode::Auto);
                            app.add_system_message("Switched to AUTO mode. Agents will be selected automatically.");
                        }
                        "manual" => {
                            app.session_manager.session.set_mode(SessionMode::Manual);
                            app.add_system_message("Switched to MANUAL mode. Use /agent <name> to select an agent.");
                        }
                        _ => {
                            app.add_system_message(&format!("Unknown mode: {}. Use 'auto' or 'manual'.", mode));
                        }
                    }
                }
            }
            "/agent" => {
                if let Some(agent_name) = args.first() {
                    let registry = app.agent_registry.read().await;
                    let agent_opt = registry.get(agent_name).cloned();
                    let available: Vec<String> = registry.list()
                        .iter()
                        .map(|a| a.name.clone())
                        .collect();
                    drop(registry);

                    if let Some(agent) = agent_opt {
                        app.active_agent = Some(agent.clone());
                        app.add_system_message(&format!("Selected agent: {} ({})", agent_name, agent.role.as_str()));
                    } else {
                        app.add_system_message(&format!(
                            "Unknown agent: {}. Available agents: {}",
                            agent_name,
                            available.join(", ")
                        ));
                    }
                } else {
                    // Show current agent or list available
                    if let Some(agent) = &app.active_agent {
                        app.add_system_message(&format!(
                            "Current agent: {} ({})",
                            agent.name,
                            agent.role.as_str()
                        ));
                    } else {
                        let registry = app.agent_registry.read().await;
                        let available: Vec<String> = registry.list()
                            .iter()
                            .map(|a| format!("{} ({})", a.name, a.role.as_str()))
                            .collect();
                        drop(registry);
                        app.add_system_message(&format!(
                            "No agent selected. Available agents: {}",
                            available.join(", ")
                        ));
                    }
                }
            }
            "/agents" => {
                let registry = app.agent_registry.read().await;
                let agents: Vec<String> = registry.list()
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let status = if app.active_agent.as_ref().map(|active| active.id == a.id).unwrap_or(false) {
                            " [ACTIVE]"
                        } else {
                            ""
                        };
                        format!("{}. {} ({}){}", i + 1, a.name, a.role.as_str(), status)
                    })
                    .collect();
                drop(registry);
                app.add_system_message(&format!("Available agents:\n{}", agents.join("\n")));
            }
            "/clear" => {
                app.session_manager.session.messages.clear();
                app.chat.clear();
                app.add_system_message("Session cleared.");
            }
            "/new" => {
                app.session_manager.session = crate::types::Session::new("New Session");
                app.chat.clear();
                let registry = app.agent_registry.read().await;
                app.active_agent = registry.get("coder").cloned();
                drop(registry);
                app.add_system_message("Started new session.");
            }
            "/save" => {
                let title = if !args.is_empty() {
                    Some(args.join(" "))
                } else {
                    None
                };
                match app.session_manager.save_session(title).await {
                    Ok(_) => {
                        let msg = format!("Session '{}' saved successfully (ID: {}).", 
                            app.session_manager.session.title, 
                            app.session_manager.session.id
                        );
                        app.add_system_message(&msg);
                    }
                    Err(e) => {
                        app.add_system_message(&format!("Failed to save session: {}", e));
                    }
                }
            }
            "/load" => {
                if let Some(id) = args.first() {
                    match app.session_manager.load_session(id).await {
                        Ok(title) => {
                            app.chat.clear();
                            for msg in &app.session_manager.session.messages {
                                app.chat.add_message(msg.clone());
                            }
                            app.add_system_message(&format!("Session '{}' loaded.", title));
                        }
                        Err(e) => {
                            app.add_system_message(&format!("Failed to load session: {}", e));
                        }
                    }
                } else {
                    app.add_system_message("Usage: /load <session_id>");
                }
            }
            "/sessions" => {
                match app.session_manager.refresh_session_list().await {
                    Ok(_) => {
                        if app.session_manager.sessions.is_empty() {
                            app.add_system_message("No saved sessions found.");
                        } else {
                            let mut list = String::from("Saved sessions:\n");
                            for s in &app.session_manager.sessions {
                                list.push_str(&format!("- {}: {} ({} messages, {})\n", 
                                    s.id, s.title, s.message_count, s.updated_at.format("%Y-%m-%d %H:%M:%S")));
                            }
                            app.add_system_message(&list);
                        }
                    }
                    Err(e) => {
                        app.add_system_message(&format!("Failed to list sessions: {}", e));
                    }
                }
            }
            "/delete" => {
                if let Some(id) = args.first() {
                    match app.session_manager.delete_session(id).await {
                        Ok(_) => {
                            app.add_system_message(&format!("Session '{}' deleted successfully.", id));
                        }
                        Err(e) => {
                            app.add_system_message(&format!("Failed to delete session: {}", e));
                        }
                    }
                } else {
                    app.add_system_message("Usage: /delete <session_id>");
                }
            }
            "/remember" => {
                if args.len() >= 2 {
                    let key = args[0];
                    let value = args[1..].join(" ");
                    match app.session_manager.remember(key, &value).await {
                        Ok(_) => {
                            app.add_system_message(&format!("Stored in memory: {} = {}", key, value));
                        }
                        Err(e) => {
                            app.add_system_message(&format!("Failed to store memory: {}", e));
                        }
                    }
                } else {
                    app.add_system_message("Usage: /remember <key> <value>");
                }
            }
            "/recall" => {
                if let Some(key) = args.first() {
                    match app.session_manager.recall(key).await {
                        Ok(Some(value)) => {
                            app.add_system_message(&format!("Memory recall: {} = {}", key, value));
                        }
                        Ok(None) => {
                            app.add_system_message(&format!("Key '{}' not found in memory.", key));
                        }
                        Err(e) => {
                            app.add_system_message(&format!("Failed to recall memory: {}", e));
                        }
                    }
                } else {
                    app.add_system_message("Usage: /recall <key>");
                }
            }
            "/forget" => {
                if let Some(key) = args.first() {
                    match app.session_manager.forget(key).await {
                        Ok(_) => {
                            app.add_system_message(&format!("Forgotten: {}", key));
                        }
                        Err(e) => {
                            app.add_system_message(&format!("Failed to forget memory: {}", e));
                        }
                    }
                } else {
                    app.add_system_message("Usage: /forget <key>");
                }
            }
            "/cancel" => {
                app.cancel_current_task().await?;
            }
            "/quit" | "/exit" => {
                app.should_quit = true;
            }
            _ => {
                app.add_system_message(&format!("Unknown command: {}. Type /help for available commands.", command));
            }
        }

        app.input.clear();
        Ok(())
    }
}
