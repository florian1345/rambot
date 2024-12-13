use crate::command::{BoardButtonEventHandler, CommandError, CommandResult};
use crate::command_data::CommandData;
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
use poise::{Command, FrameworkContext, FrameworkError, FrameworkOptions, PrefixFrameworkOptions};
use serenity::all::{FullEvent, UserId};

pub mod audio;
pub mod command_data;
pub mod command;
pub mod config;
pub mod event;
pub mod key_value;
pub mod logging;
pub mod plugin;
pub mod state;

async fn handle_error(err: FrameworkError<'_, CommandData, CommandError>) {
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
        framework_ctx: FrameworkContext<'_, CommandData, CommandError>) -> CommandResult {
    BoardButtonEventHandler.handle_event(serenity_ctx, event, framework_ctx).await?;
    LoggingEventHandler.handle_event(serenity_ctx, event, framework_ctx).await
}

fn get_framework_options<'uid>(
    prefix: Option<&str>,
    owners: impl IntoIterator<Item = &'uid UserId>,
    commands: Vec<Command<CommandData, CommandError>>
) -> FrameworkOptions<CommandData, CommandError> {
    FrameworkOptions {
        commands,
        prefix_options: PrefixFrameworkOptions {
            prefix: prefix.map(str::to_owned),
            ..Default::default()
        },
        owners: owners.into_iter().cloned().collect(),
        on_error: |err| Box::pin(handle_error(err)),
        event_handler: |serenity_ctx, event, framework_ctx, _|
            Box::pin(handle_event(serenity_ctx, event, framework_ctx)),
        ..Default::default()
    }
}

fn get_framework_options_for_configured_modes(
    config: &Config,
    mut commands: Vec<Command<CommandData, CommandError>>
) -> FrameworkOptions<CommandData, CommandError> {
    if !config.allow_slash_commands() {
        commands.iter_mut().for_each(|command| command.slash_action = None);
    }

    get_framework_options(config.prefix(), config.owners(), commands)
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

    let token = config.token().to_owned();
    let framework_options =
        get_framework_options_for_configured_modes(&config, command::commands());
    let programmatic_command_framework_options =
        get_framework_options(config.prefix().or(Some("")), config.owners(), command::commands());
    let command_data =
        CommandData::new(config, plugin_mgr, state, programmatic_command_framework_options);
    let framework = poise::Framework::builder()
        .options(framework_options)
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(command_data)
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use kernal::prelude::*;
    use serenity::all::UserId;
    use super::*;

    /// Just a test command - no further purpose.
    #[poise::command(slash_command, prefix_command)]
    async fn test(_: command::Context<'_>) -> CommandResult {
        Ok(())
    }

    fn get_config(prefix: &str, allow_slash_commands: bool, owners: &[&str]) -> Config {
        serde_json::from_str(&format!("
            {{
                \"prefix\": \"{}\",
                \"allow_slash_commands\": {},
                \"token\": \"testToken\",
                \"owners\": [{}],
                \"plugin_directory\": \"test/plugin/directory\",
                \"plugin_config_directory\": \"test/plugin/config/directory\",
                \"state_directory\": \"test/state/directory\",
                \"root_directory\": \"test/root/directory\",
                \"allow_web_access\": true,
                \"log_level_filter\": \"info\"
            }}
        ", prefix, allow_slash_commands, owners.join(","))).unwrap()
    }

    #[test]
    fn get_framework_options_works_with_slash_commands() {
        let config: Config = get_config("!", true, &["123"]);

        let framework_options = get_framework_options_for_configured_modes(&config, vec![test()]);

        let expected_user_id = UserId::from_str("123").unwrap();
        assert_that!(&framework_options.commands).has_length(1);
        assert_that!(&framework_options.commands[0].slash_action).is_some();
        assert_that!(&framework_options.prefix_options.prefix).contains("!".to_owned());
        assert_that!(&framework_options.owners).contains_exactly_in_any_order(&[expected_user_id]);
    }
    
    #[test]
    fn get_framework_options_works_without_slash_commands() {
        let config: Config = get_config("!", false, &["123", "456"]);

        let framework_options = get_framework_options_for_configured_modes(&config, vec![test()]);

        let expected_user_id_1 = UserId::from_str("123").unwrap();
        let expected_user_id_2 = UserId::from_str("456").unwrap();
        assert_that!(&framework_options.commands).has_length(1);
        assert_that!(&framework_options.commands[0].slash_action).is_none();
        assert_that!(&framework_options.owners)
            .contains_exactly_in_any_order(&[expected_user_id_1, expected_user_id_2]);
    }
}
