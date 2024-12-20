use poise::FrameworkContext;

use serenity::all::FullEvent;
use serenity::client::Context;

use crate::command::{CommandError, CommandResult};
use crate::command_data::CommandData;

/// A trait for structs which can handle any Discord events.
pub(crate) trait FrameworkEventHandler {

    /// Called whenever a Discord event occurs.
    ///
    /// # Arguments
    ///
    /// * `serenity_ctx`: The serenity context.
    /// * `event`: The event which occurred.
    /// * `framework_ctx`: The context of the command framework.
    ///
    /// # Returns
    ///
    /// A [CommandResult].
    async fn handle_event(&self, serenity_ctx: &Context, event: &FullEvent,
        framework_ctx: FrameworkContext<'_, CommandData, CommandError>) -> CommandResult;
}
