use std::sync::Arc;
use poise::FrameworkOptions;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use crate::command::CommandError;
use crate::config::Config;
use crate::plugin::PluginManager;
use crate::state::State;

/// Manages access of commands to shared data.
pub struct CommandData {
    config: Config,
    plugin_manager: Arc<PluginManager>,
    state: RwLock<State>,
    programmatic_command_framework_options: FrameworkOptions<CommandData, CommandError>
}

impl CommandData {

    /// Creates new command data.
    ///
    /// # Arguments
    ///
    /// * `config`: The bot's configuration to be read by commands.
    /// * `plugin_mgr`: An arc of the global plugin manager to be used by commands.
    /// * `state`: The initial mutable state shared by commands. Will be wrapped in a lock to manage
    ///   access.
    /// * `programmatic_command_framework_options`: The framework options to use for programmatic
    ///   command execution. This is different to the ordinary framework options as commands have to
    ///   be invokable by their prefix action, even if prefix commands are disabled.
    pub fn new(
        config: Config,
        plugin_mgr: Arc<PluginManager>,
        state: State,
        programmatic_command_framework_options: FrameworkOptions<CommandData, CommandError>
    ) -> CommandData {
        CommandData {
            config,
            plugin_manager: plugin_mgr,
            state: RwLock::new(state),
            programmatic_command_framework_options
        }
    }

    /// Gets the bot's configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Gets a reference the global plugin manager to be used by commands.
    pub fn plugin_manager(&self) -> &PluginManager {
        self.plugin_manager.as_ref()
    }

    /// Clones the arc to the global plugin manager to be used by commands.
    pub fn plugin_manager_arc(&self) -> Arc<PluginManager> {
        Arc::clone(&self.plugin_manager)
    }

    /// Gets immutable access to the mutable state shared by commands.
    pub async fn state(&self) -> RwLockReadGuard<'_, State> {
        self.state.read().await
    }

    /// Gets mutable access to the mutable state shared by commands.
    pub async fn state_mut(&self) -> RwLockWriteGuard<'_, State> {
        self.state.write().await
    }

    /// Gets the framework options to use for programmatic command execution. This is different to
    /// the ordinary framework options as commands have to be invokable by their prefix action, even
    /// if prefix commands are disabled.
    pub fn programmatic_command_framework_options(
        &self
    ) -> &FrameworkOptions<CommandData, CommandError> {
        &self.programmatic_command_framework_options
    }
}
