use crate::command::{BoardButtonEventHandler, CommandData, CommandError, CommandResult};
use crate::config::Config;
use crate::event::FrameworkEventHandler;
use crate::logging::LoggingEventHandler;
use crate::plugin::PluginManager;
use crate::state::State;

use serenity::client::{Client, Context};
use serenity::prelude::GatewayIntents;

use simplelog::LevelFilter;

use songbird::SerenityInit;

use std::sync::Arc;
use poise::{FrameworkContext, FrameworkError, PrefixFrameworkOptions};
use serenity::all::FullEvent;
use tokio::sync::RwLock;

pub mod audio;
pub mod command;
pub mod config;
pub mod event;
pub mod key_value;
pub mod logging;
pub mod plugin;
pub mod state;

async fn handle_error(err: FrameworkError<'_, RwLock<CommandData>, CommandError>) {
    match err.ctx() {
        Some(ctx) => {
            if let Err(reply_err) = ctx.reply(format!("{}", err)).await {
                log::error!("Error replying to message: {}", reply_err);
            }
        },
        None => {
            log::error!("Error without loaded context: {}", err);
        }
    }
}

async fn handle_event(serenity_ctx: &Context, event: &FullEvent,
        framework_ctx: FrameworkContext<'_, RwLock<CommandData>, CommandError>) -> CommandResult {
    BoardButtonEventHandler.handle_event(serenity_ctx, event, framework_ctx).await?;
    LoggingEventHandler.handle_event(serenity_ctx, event, framework_ctx).await
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

    log::info!("Successfully loaded state for {} guilds.", state.guild_count());

    let prefix = config.prefix().to_owned();
    let owners = config.owners().iter().cloned().collect();
    let token = config.token().to_owned();
    let mut command_data = CommandData::new();
    command_data.insert::<PluginManager>(plugin_mgr);
    command_data.insert::<Config>(config);
    command_data.insert::<State>(state);
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: command::commands(),
            prefix_options: PrefixFrameworkOptions {
                prefix: Some(prefix),
                ..Default::default()
            },
            owners,
            on_error: |err| Box::pin(handle_error(err)),
            event_handler: |serenity_ctx, event, framework_ctx, _|
                Box::pin(handle_event(serenity_ctx, event, framework_ctx)),
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(RwLock::new(command_data))
            })
        })
        .build();
    let intents = GatewayIntents::non_privileged() |
        GatewayIntents::MESSAGE_CONTENT;
    let client_res = Client::builder(token, intents)
        .framework(framework)
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
