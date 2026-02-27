mod agent;
mod config;
mod llm;
mod orchestrator;
mod persistence;
mod shared;
mod tui;
mod types;

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;
    
    info!("Starting Agent TUI...");
    
    // Load configuration
    let config = config::Config::load()?;
    info!("Configuration loaded successfully");
    
    // Run the TUI application
    let mut app = tui::App::new(config)?;
    app.run().await?;
    
    info!("Agent TUI shutdown complete");
    Ok(())
}
