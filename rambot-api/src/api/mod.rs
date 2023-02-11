use std::ops::Deref;

use crate::api::internal::*;

mod internal;

/// API for the entire bot.
pub trait BotDriver {

    /// Gets an API for the guild with the given ID.
    /// 
    /// * `id`: The Discord ID of the guild for which to get an API.
    fn guild(&self, id: u64) -> Guild;
}

pub struct Bot(Box<dyn CloneableBotDriver>);

impl Deref for Bot {
    type Target = dyn CloneableBotDriver;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl Clone for Bot {
    fn clone(&self) -> Bot {
        Bot(self.0.clone_object())
    }
}

impl<T: CloneableBotDriver + 'static> From<T> for Bot {
    fn from(bot: T) -> Bot {
        Bot(Box::new(bot))
    }
}

/// API for the bot on a specific guild.
pub trait GuildDriver {

    /// Returns a vector containing the names of all layers defined on this
    /// guild in an undefined order.
    fn layers(&self) -> Vec<String>;
}

pub struct Guild(Box<dyn CloneableGuildDriver>);

impl Deref for Guild {
    type Target = dyn CloneableGuildDriver;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl Clone for Guild {
    fn clone(&self) -> Guild {
        Guild(self.0.clone_object())
    }
}
