use serde_json::Value;

use serenity::client::{EventHandler, Context};
use serenity::model::application::interaction::Interaction;
use serenity::model::channel::Reaction;
use serenity::model::prelude::Ready;
use serenity::model::event::ResumedEvent;
use serenity::model::guild::Guild;

use std::pin::Pin;
use std::future::Future;

/// An [EventHandler] that sequentially forwards all received events to two
/// child event handlers. To construct this, use the [EventHandlerComposer].
pub struct CompositeEventHandler<E1, E2> {
    e1: E1,
    e2: E2
}

impl<E1, E2> EventHandler for CompositeEventHandler<E1, E2>
where
    E1: EventHandler,
    E2: EventHandler
{
    // TODO complete this list

    fn guild_create<'life0, 'async_trait>(&'life0 self, ctx: Context,
        guild: Guild, is_new: bool)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        Box::pin(async move {
            self.e1.guild_create(ctx.clone(), guild.clone(), is_new).await;
            self.e2.guild_create(ctx, guild, is_new).await;
        })
    }

    fn interaction_create<'life0, 'async_trait>(&'life0 self, ctx: Context,
        interaction: Interaction)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        Box::pin(async move {
            self.e1.interaction_create(ctx.clone(), interaction.clone()).await;
            self.e2.interaction_create(ctx, interaction).await;
        })
    }

    fn reaction_add<'life0, 'async_trait>(&'life0 self, ctx: Context,
        add_reaction: Reaction)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        Box::pin(async move {
            self.e1.reaction_add(ctx.clone(), add_reaction.clone()).await;
            self.e2.reaction_add(ctx, add_reaction).await;
        })
    }

    fn ready<'life0, 'async_trait>(&'life0 self, ctx: Context,
        data_about_bot: Ready)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        Box::pin(async move {
            self.e1.ready(ctx.clone(), data_about_bot.clone()).await;
            self.e2.ready(ctx, data_about_bot).await;
        })
    }

    fn resume<'life0, 'async_trait>(&'life0 self, ctx: Context,
        resumed_event: ResumedEvent)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        Box::pin(async move {
            self.e1.resume(ctx.clone(), resumed_event.clone()).await;
            self.e2.resume(ctx, resumed_event).await;
        })
    }

    fn unknown<'life0, 'async_trait>(&'life0 self, ctx: Context,
        name: String, raw: Value)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        Box::pin(async move {
            self.e1.unknown(ctx.clone(), name.clone(), raw.clone()).await;
            self.e2.unknown(ctx, name, raw).await;
        })
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
