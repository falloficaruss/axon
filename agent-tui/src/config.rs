use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// LLM configuration
    pub llm: LlmConfig,
    /// Orchestration settings
    pub orchestration: OrchestrationConfig,
    /// Agent definitions
    pub agents: HashMap<String, AgentConfig>,
    /// Persistence settings
    pub persistence: PersistenceConfig,
    /// UI settings
    pub ui: UiConfig,
    /// Keybindings
    pub keybindings: KeybindingsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            orchestration: OrchestrationConfig::default(),
            agents: Self::default_agents(),
            persistence: PersistenceConfig::default(),
            ui: UiConfig::default(),
            keybindings: KeybindingsConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file or create default
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(config_dir.join("agent-tui").join("config.toml"))
    }

    /// Get the data directory for persistence
    pub fn data_dir() -> Result<PathBuf> {
        let data_dir =
            dirs::data_dir().ok_or_else(|| anyhow::anyhow!("Could not find data directory"))?;
        Ok(data_dir.join("agent-tui"))
    }

    /// Resolve persistence path (handles tilde)
    pub fn resolve_path(path: &str) -> PathBuf {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        PathBuf::from(path)
    }

    /// Get absolute session directory
    pub fn session_dir(&self) -> PathBuf {
        Self::resolve_path(&self.persistence.session_dir)
    }

    /// Get absolute memory directory
    pub fn memory_dir(&self) -> PathBuf {
        Self::resolve_path(&self.persistence.memory_dir)
    }

    /// Default agent configurations
    fn default_agents() -> HashMap<String, AgentConfig> {
        let mut agents = HashMap::new();

        agents.insert(
            "planner".to_string(),
            AgentConfig {
                enabled: true,
                role: "planner".to_string(),
                description: "Plans and orchestrates multi-agent workflows".to_string(),
                model: "gpt-4o".to_string(),
                system_prompt: r#"You are a task planner. Your job is to:
1. Analyze the user's request
2. Break it down into manageable subtasks
3. Determine which agents are needed for each subtask
4. Provide a clear execution plan

Be concise and focus on the most efficient approach."#
                    .to_string(),
                capabilities: vec!["plan".to_string()],
            },
        );

        agents.insert(
            "coder".to_string(),
            AgentConfig {
                enabled: true,
                role: "coder".to_string(),
                description: "Writes and modifies code".to_string(),
                model: "gpt-4o".to_string(),
                system_prompt: r#"You are a skilled programmer. Your job is to:
1. Write clean, well-documented code
2. Follow best practices and conventions
3. Handle errors appropriately
4. Explain your approach before coding

Always provide complete, working code examples."#
                    .to_string(),
                capabilities: vec![
                    "code".to_string(),
                    "refactor".to_string(),
                    "debug".to_string(),
                ],
            },
        );

        agents.insert(
            "reviewer".to_string(),
            AgentConfig {
                enabled: true,
                role: "reviewer".to_string(),
                description: "Reviews code for quality and issues".to_string(),
                model: "gpt-4o-mini".to_string(),
                system_prompt: r#"You are a code reviewer. Your job is to:
1. Identify bugs and potential issues
2. Check for security vulnerabilities
3. Ensure code follows best practices
4. Provide constructive feedback

Focus on the most important issues first."#
                    .to_string(),
                capabilities: vec!["review".to_string()],
            },
        );

        agents.insert(
            "tester".to_string(),
            AgentConfig {
                enabled: true,
                role: "tester".to_string(),
                description: "Generates and runs tests".to_string(),
                model: "gpt-4o-mini".to_string(),
                system_prompt: r#"You are a testing specialist. Your job is to:
1. Write comprehensive test cases
2. Cover edge cases and error conditions
3. Ensure good test coverage
4. Provide clear test descriptions

Focus on practical, maintainable tests."#
                    .to_string(),
                capabilities: vec!["test".to_string()],
            },
        );

        agents.insert(
            "explorer".to_string(),
            AgentConfig {
                enabled: true,
                role: "explorer".to_string(),
                description: "Explores codebase structure and files".to_string(),
                model: "gpt-4o-mini".to_string(),
                system_prompt: r#"You are an explorer. Your job is to:
1. Navigate file systems
2. Find relevant code and documentation
3. Understand codebase structure
4. Gather context for other agents

Be thorough but concise in your findings."#
                    .to_string(),
                capabilities: vec!["explore".to_string()],
            },
        );

        agents.insert(
            "integrator".to_string(),
            AgentConfig {
                enabled: true,
                role: "integrator".to_string(),
                description: "Synthesizes results from multiple agents".to_string(),
                model: "gpt-4o".to_string(),
                system_prompt: r#"You are an integrator. Your job is to:
1. Combine outputs from multiple agents
2. Resolve conflicts between different approaches
3. Create a cohesive final result
4. Ensure consistency and completeness

Focus on creating the best overall solution."#
                    .to_string(),
                capabilities: vec!["document".to_string()],
            },
        );

        agents
    }
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider name (currently only "openai")
    pub provider: String,
    /// API key (can use env var with $ prefix)
    pub api_key: String,
    /// Model to use
    pub model: String,
    /// Maximum tokens per request
    pub max_tokens: u32,
    /// Temperature (0.0 - 2.0)
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            api_key: "$OPENAI_API_KEY".to_string(),
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

/// Orchestration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    /// Default mode (auto or manual)
    pub mode: String,
    /// Maximum concurrent agents
    pub max_concurrent_agents: usize,
    /// Confidence threshold for auto-routing (0.0 - 1.0)
    pub routing_confidence_threshold: f32,
    /// Auto-execute if confidence above this threshold
    pub auto_confirm_threshold: f32,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            mode: "auto".to_string(),
            max_concurrent_agents: 5,
            routing_confidence_threshold: 0.8,
            auto_confirm_threshold: 0.95,
        }
    }
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Whether the agent is enabled
    pub enabled: bool,
    /// Agent role
    pub role: String,
    /// Description
    pub description: String,
    /// Model to use
    pub model: String,
    /// System prompt
    pub system_prompt: String,
    /// Capabilities
    pub capabilities: Vec<String>,
}

/// Persistence settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceConfig {
    /// Directory for sessions
    pub session_dir: String,
    /// Directory for memory
    pub memory_dir: String,
    /// Auto-save interval in seconds
    pub auto_save_interval: u64,
    /// Maximum number of sessions to keep
    pub max_sessions: usize,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            session_dir: "~/.agent-tui/sessions".to_string(),
            memory_dir: "~/.agent-tui/memory".to_string(),
            auto_save_interval: 30,
            max_sessions: 100,
        }
    }
}

/// UI settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Color theme
    pub theme: String,
    /// Show agent flow visualization
    pub show_agent_flow: bool,
    /// Show timestamps in messages
    pub show_timestamps: bool,
    /// Show confidence scores
    pub show_confidence_scores: bool,
    /// Datetime format
    pub datetime_format: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            show_agent_flow: true,
            show_timestamps: true,
            show_confidence_scores: true,
            datetime_format: "%H:%M:%S".to_string(),
        }
    }
}

/// Keybindings configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
    /// Quit application
    pub quit: String,
    /// Submit input
    pub submit: String,
    /// New line in input
    pub new_line: String,
    /// Navigate history up
    pub history_up: String,
    /// Navigate history down
    pub history_down: String,
    /// Autocomplete
    pub autocomplete: String,
    /// Open command palette
    pub command_palette: String,
    /// Open agent selector
    pub agent_selector: String,
    /// Toggle sidebar
    pub sidebar_toggle: String,
    /// Open memory manager
    pub memory_manager: String,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            quit: "ctrl+c".to_string(),
            submit: "enter".to_string(),
            new_line: "shift+enter".to_string(),
            history_up: "up".to_string(),
            history_down: "down".to_string(),
            autocomplete: "tab".to_string(),
            command_palette: "ctrl+k".to_string(),
            agent_selector: "ctrl+a".to_string(),
            sidebar_toggle: "ctrl+b".to_string(),
            memory_manager: "ctrl+m".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.orchestration.mode, "auto");
        assert!(!config.agents.is_empty());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(!toml_str.is_empty());

        // Verify it can be parsed back
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.llm.model, config.llm.model);
    }
}
