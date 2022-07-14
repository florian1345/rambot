use crate::config::Config;
use crate::plugin::PluginManager;
use crate::state::State;

use serenity::client::{Client, Context};
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

use songbird::SerenityInit;

use std::collections::HashSet;

pub mod audio;
pub mod command;
pub mod config;
pub mod logging;
pub mod plugin;
pub mod state;

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

    let state = match State::load(config.state_directory()) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Error loading state files: {}", e);
            return;
        }
    };

    log::info!("Successfully loaded state for {} guilds.",
        state.guild_count());

    let plugin_mgr = match PluginManager::new(config.plugin_directory()) {
        Ok(m) => m,
        Err(e) => {
            log::error!("{}", e);
            return;
        }
    };

    let framework = StandardFramework::new()
        .configure(|c| c.prefix(config.prefix()))
        .group(command::get_root_commands())
        .group(command::get_layer_commands())
        .help(&PRINT_HELP);
    let client_res = Client::builder(config.token())
        .framework(framework)
        .type_map_insert::<PluginManager>(plugin_mgr)
        .type_map_insert::<Config>(config)
        .type_map_insert::<State>(state)
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
