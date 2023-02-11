use rambot_api::api::BotDriver;

use serenity::prelude::Context;

#[derive(Clone)]
pub struct BotDriverImpl {
    ctx: Context
}

impl BotDriverImpl {
    pub fn new(ctx: &Context) -> BotDriverImpl {
        BotDriverImpl {
            ctx: ctx.clone()
        }
    }
}

impl BotDriver for BotDriverImpl {

}

#[derive(Clone)]
pub struct GuildDriverImpl {
    ctx: Context
}
