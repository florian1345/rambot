//! This module defines some internal traits you should never need to
//! reference, but need to be exposed for technical reasons.

use super::{BotDriver, GuildDriver};

// TODO once dyn upcasting coercion is available, these could probably be made
// crate-local and the deref targets changed to XProvider

pub trait CloneableBotDriver : BotDriver {

    fn clone_object(&self) -> Box<dyn CloneableBotDriver>;
}

impl<B: BotDriver + Clone + 'static> CloneableBotDriver for B {
    fn clone_object(&self) -> Box<dyn CloneableBotDriver> {
        Box::new(self.clone())
    }
}

pub trait CloneableGuildDriver : GuildDriver {

    fn clone_object(&self) -> Box<dyn CloneableGuildDriver>;
}

impl<G: GuildDriver + Clone + 'static> CloneableGuildDriver for G {
    fn clone_object(&self) -> Box<dyn CloneableGuildDriver> {
        Box::new(self.clone())
    }
}
