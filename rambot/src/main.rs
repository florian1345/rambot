use crate::command::board::BoardButtonEventHandler;
use crate::config::Config;
use crate::plugin::PluginManager;
use crate::state::State;

use serenity::client::{Client, Context};
use serenity::framework::Framework;
use serenity::framework::standard::{
    Args,
    CommandGroup,
    CommandResult,
    help_commands,
    HelpOptions,
    StandardFramework
};
use serenity::framework::standard::macros::help;
use serenity::model::prelude::{Message, UserId};
use serenity::prelude::TypeMapKey;

use songbird::SerenityInit;

use std::collections::HashSet;
use std::sync::Arc;

pub mod audio;
pub mod command;
pub mod config;
pub mod key_value;
pub mod logging;
pub mod plugin;
pub mod state;

pub type FrameworkArc = Arc<Box<dyn Framework + Send + Sync + 'static>>;

pub struct FrameworkTypeMapKey;

impl TypeMapKey for FrameworkTypeMapKey {
    type Value = FrameworkArc;
}

#[help]
async fn print_help(ctx: &Context, msg: &Message, args: Args,
        help_options: &'static HelpOptions, groups: &[&'static CommandGroup],
        owners: HashSet<UserId>) -> CommandResult {
    help_commands::with_embeds(ctx, msg, args, help_options, groups, owners).await;
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = logging::init() {
        eprintln!("{}", e);
        return;
    }

    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Error loading config file: {}", e);
            return;
        }
    };

    log::info!("Successfully loaded config file.");

    let plugin_mgr = match PluginManager::new(&config) {
        Ok(m) => m,
        Err(e) => {
            log::error!("{}", e);
            return;
        }
    };
    let plugin_mgr = Arc::new(plugin_mgr);

    let state_res = State::load(
        config.state_directory(), Arc::clone(&plugin_mgr));
    let state = match state_res {
        Ok(s) => s,
        Err(e) => {
            log::error!("Error loading state files: {}", e);
            return;
        }
    };

    log::info!("Successfully loaded state for {} guilds.",
        state.guild_count());

    // We need to keep the framework, as sound boards need to be able to submit
    // commands programatically.

    let framework: FrameworkArc =
        Arc::new(Box::new(StandardFramework::new()
            .configure(|c| c.prefix(config.prefix()))
            .group(command::get_root_commands())
            .group(command::get_adapter_commands())
            .group(command::get_board_commands())
            .group(command::get_effect_commands())
            .group(command::get_layer_commands())
            .help(&PRINT_HELP)));
    let client_res = Client::builder(config.token())
        .framework_arc(Arc::clone(&framework))
        .event_handler(BoardButtonEventHandler)
        .type_map_insert::<PluginManager>(plugin_mgr)
        .type_map_insert::<Config>(config)
        .type_map_insert::<State>(state)
        .type_map_insert::<FrameworkTypeMapKey>(framework)
        .register_songbird()
        .await;
    let mut client = match client_res {
        Ok(c) => c,
        Err(e) => {
            log::error!("{}", e);
            return;
        }
    };

    if let Err(e) = client.start().await {
        log::error!("{}", e);
    }
}
