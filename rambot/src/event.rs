use serde_json::Value;

use serenity::client::{EventHandler, Context};
use serenity::model::channel::Reaction;
use serenity::model::prelude::Ready;
use serenity::model::event::ResumedEvent;
use serenity::model::guild::Guild;

/// An [EventHandler] that sequentially forwards all received events to two
/// child event handlers. To construct this, use the [EventHandlerComposer].
pub struct CompositeEventHandler<E1, E2> {
    e1: E1,
    e2: E2
}

#[async_trait::async_trait]
impl<E1, E2> EventHandler for CompositeEventHandler<E1, E2>
where
    E1: EventHandler,
    E2: EventHandler
{
    // TODO complete this list

    async fn guild_create(&self, ctx: Context, guild: Guild, is_new: bool) {
        self.e1.guild_create(ctx.clone(), guild.clone(), is_new).await;
        self.e2.guild_create(ctx, guild, is_new).await;
    }

    async fn reaction_add(&self, ctx: Context, add_reaction: Reaction) {
        self.e1.reaction_add(ctx.clone(), add_reaction.clone()).await;
        self.e2.reaction_add(ctx, add_reaction).await;
    }

    async fn ready(&self, ctx: Context, data_about_bot: Ready) {
        self.e1.ready(ctx.clone(), data_about_bot.clone()).await;
        self.e2.ready(ctx, data_about_bot).await;
    }

    async fn resume(&self, ctx: Context, resumed_event: ResumedEvent) {
        self.e1.resume(ctx.clone(), resumed_event.clone()).await;
        self.e2.resume(ctx, resumed_event).await;
    }

    async fn unknown(&self, ctx: Context, name: String, raw: Value) {
        self.e1.unknown(ctx.clone(), name.clone(), raw.clone()).await;
        self.e2.unknown(ctx, name, raw).await;
    }
}

/// Constructs [CompositeEventHandler]s by the builder pattern.
pub struct EventHandlerComposer<E> {
    event_handler: E
}

impl<E> EventHandlerComposer<E> {

    /// Creates a new event handler composer that initially holds the given
    /// event handler as the composite.
    pub fn new(event_handler: E) -> EventHandlerComposer<E> {
        EventHandlerComposer {
            event_handler
        }
    }

    /// Adds a new event handler to the composite. Returns the altered composer
    /// for chaining.
    pub fn push<E2>(self, event_handler: E2)
            -> EventHandlerComposer<CompositeEventHandler<E, E2>> {
        EventHandlerComposer {
            event_handler: CompositeEventHandler {
                e1: self.event_handler,
                e2: event_handler
            }
        }
    }

    /// Builds the final composite event handler.
    pub fn build(self) -> E {
        self.event_handler
    }
}
