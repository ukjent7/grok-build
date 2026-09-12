use anyhow::Result;
use xai_grok_shell::agent::config::Config as AgentConfig;

#[derive(Debug, clap::Args, Clone)]
pub struct ShareArgs {
    /// Session ID to share
    pub session_id: String,
}

pub async fn run(args: &ShareArgs, agent_config: &AgentConfig) -> Result<()> {
    let _ = (args, agent_config);
    anyhow::bail!(
        "{}",
        crate::locale::ctx().named_text(
            "session.share_disabled",
            "Session sharing is temporarily disabled"
        )
    );
}
