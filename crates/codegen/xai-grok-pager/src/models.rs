//! `grok models` subcommand.

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use xai_grok_shell::agent::config::Config as AgentConfig;
use xai_grok_shell::cli_models::{AuthStatus, list_models};

use crate::client_identity::{PAGER_CLIENT_TYPE, PAGER_CLIENT_VERSION};

pub async fn list_available_models(agent_config: &AgentConfig) -> Result<()> {
    let locale = crate::locale::ctx();
    match AuthStatus::resolve(agent_config) {
        AuthStatus::ApiKey => println!("{}", locale.named_text("models.auth.api_key", "You are using XAI_API_KEY.")),
        AuthStatus::LoggedIn(host) => println!(
            "{}",
            locale.format_named(
                "models.auth.logged_in",
                "You are logged in with {host}.",
                &[("host", host.as_str())],
            )
        ),
        AuthStatus::ModelCredentials(model) => {
            println!(
                "{}",
                locale.format_named(
                    "models.auth.model_api_key",
                    "Model '{model}' is using its own API key.",
                    &[("model", model.as_str())],
                )
            );
        }
        AuthStatus::DeploymentKey => println!(
            "{}",
            locale.named_text(
                "models.auth.deployment_key",
                "You are authenticated via deployment key."
            )
        ),
        AuthStatus::NotAuthenticated => println!(
            "{}",
            locale.named_text("models.auth.not_authenticated", "You are not authenticated.")
        ),
    }
    println!();

    let cancel = CancellationToken::new();
    xai_grok_telemetry::startup::mark_utility_process();
    let spawned = crate::acp::spawn::spawn_grok_shell(agent_config.clone(), &cancel, None).await?;
    // Cancel and join on every return path, including the `?` below
    let _agent_guard =
        crate::acp::spawn::AgentShutdownGuard::new(cancel.clone(), Some(spawned.thread_handle));

    let state = list_models(&spawned.channel.tx, PAGER_CLIENT_TYPE, PAGER_CLIENT_VERSION).await?;

    let current_model = state.current_model_id.0.to_string();
    println!(
        "{}",
        locale.format_named(
            "models.default",
            "Default model: {model}",
            &[("model", current_model.as_str())],
        )
    );
    println!();
    println!(
        "{}",
        locale.named_text("models.available", "Available models:")
    );
    for m in state.available_models {
        if m.model_id == state.current_model_id {
            println!(
                "  * {} {}",
                m.model_id.0,
                locale.named_text("models.default_marker", "(default)")
            );
        } else {
            println!("  - {}", m.model_id.0);
        }
    }

    Ok(())
}
