use crate::config::Config;

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
pub mod commands;
pub mod config;

#[help]
async fn print_help(ctx: &Context, msg: &Message, args: Args,
        help_options: &'static HelpOptions, groups: &[&'static CommandGroup],
        owners: HashSet<UserId>) -> CommandResult {
    help_commands::with_embeds(ctx, msg, args, help_options, groups, owners).await;
    Ok(())
}

#[tokio::main]
async fn main() {
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let framework = StandardFramework::new()
        .configure(|c| c.prefix(config.prefix()))
        .group(commands::get_commands())
        .help(&PRINT_HELP);
    let client_res = Client::builder(config.token())
        .framework(framework)
        .register_songbird()
        .await;
    let mut client = match client_res {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    if let Err(e) = client.start().await {
        eprintln!("{}", e);
    }
}
