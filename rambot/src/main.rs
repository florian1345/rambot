use crate::command::board::BoardButtonEventHandler;
use crate::config::Config;
use crate::event::EventHandlerComposer;
use crate::logging::LoggingEventHandler;
use crate::plugin::PluginManager;
use crate::state::State;

use serenity::client::{Client, Context};
use serenity::framework::Framework;
use serenity::framework::standard::{
    Args,
    CommandError,
    CommandGroup,
    CommandResult,
    help_commands,
    HelpOptions,
    StandardFramework
};
use serenity::framework::standard::macros::{help, hook};
use serenity::model::prelude::{Message, UserId};
use serenity::prelude::{TypeMapKey, GatewayIntents};

use simplelog::LevelFilter;

use songbird::SerenityInit;

use std::collections::HashSet;
use std::sync::Arc;

pub mod audio;
pub mod command;
pub mod config;
pub mod drivers;
pub mod event;
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
    help_commands::with_embeds(ctx, msg, args, help_options, groups, owners)
        .await?;
    Ok(())
}

#[hook]
async fn after_hook(ctx: &Context, msg: &Message, _: &str,
        error: Result<(), CommandError>) {
    if let Err(e) = error {
        let message = format!("{}", e);

        if let Err(e) = msg.reply(ctx, message).await {
            log::error!("Error replying to message: {}", e);
        }
    }
}

#[tokio::main]
async fn main() {
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            if let Err(e_log) = logging::init(LevelFilter::Error) {
                eprintln!("Error setting up logger: {}", e_log);
                eprintln!("Error loading config file: {}", e);
            }
            else {
                log::error!("Error loading config file: {}", e);
            }

            return;
        }
    };

    if let Err(e) = logging::init(config.log_level_filter()) {
        eprintln!("Error setting up logger: {}", e);
        return;
    }

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
            .configure(|c| c
                .prefix(config.prefix())
                .owners(config.owners().iter().cloned().collect()))
            .group(command::get_root_commands())
            .group(command::get_adapter_commands())
            .group(command::get_board_commands())
            .group(command::get_effect_commands())
            .group(command::get_layer_commands())
            .after(after_hook)
            .help(&PRINT_HELP)));
    let intents = GatewayIntents::non_privileged() |
        GatewayIntents::MESSAGE_CONTENT;
    let client_res = Client::builder(config.token(), intents)
        .framework_arc(Arc::clone(&framework))
        .event_handler(EventHandlerComposer::new(BoardButtonEventHandler)
            .push(LoggingEventHandler)
            .build())
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
