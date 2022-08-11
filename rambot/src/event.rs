use std::collections::HashMap;

use serde_json::Value;

use serenity::client::bridge::gateway::event::ShardStageUpdateEvent;
use serenity::client::{EventHandler, Context};
use serenity::model::application::interaction::Interaction;
use serenity::model::channel::Reaction;
use serenity::model::event::ResumedEvent;
use serenity::model::guild::Guild;
use serenity::model::id::GuildId;
use serenity::model::prelude::{
    ApplicationId,
    Channel,
    ChannelCategory,
    ChannelId,
    ChannelPinsUpdateEvent,
    Emoji,
    EmojiId,
    GuildChannel,
    GuildMembersChunkEvent,
    GuildScheduledEventUserAddEvent,
    GuildScheduledEventUserRemoveEvent,
    Integration,
    IntegrationId,
    InviteCreateEvent,
    InviteDeleteEvent,
    Member,
    MessageId,
    Message,
    MessageUpdateEvent,
    PartialGuild,
    PartialGuildChannel,
    Presence,
    Ready,
    Role,
    RoleId,
    ScheduledEvent,
    StageInstance,
    StickerId,
    ThreadListSyncEvent,
    ThreadMember,
    ThreadMembersUpdateEvent,
    TypingStartEvent,
    UnavailableGuild,
    VoiceServerUpdateEvent
};
use serenity::model::prelude::automod::{Rule, ActionExecution};
use serenity::model::prelude::command::CommandPermission;
use serenity::model::sticker::Sticker;
use serenity::model::user::{User, CurrentUser};
use serenity::model::voice::VoiceState;

macro_rules! dispatch {
    ($self:expr, $method:ident $(, $args:expr)*) => {
        $self.e1.$method($($args.clone(), )*).await;
        $self.e2.$method($($args, )*).await;
    }
}

macro_rules! dispatch_copy {
    ($self:expr, $method:ident, $ctx:expr $(, $args:expr)*) => {
        $self.e1.$method($ctx.clone(), $($args, )*).await;
        $self.e2.$method($ctx, $($args, )*).await;
    }
}

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
    async fn application_command_permissions_update(&self, ctx: Context,
            permission: CommandPermission) {
        dispatch!(
            self, application_command_permissions_update, ctx, permission);
    }

    async fn auto_moderation_rule_create(&self, ctx: Context, rule: Rule) {
        dispatch!(self, auto_moderation_rule_create, ctx, rule);
    }

    async fn auto_moderation_rule_update(&self, ctx: Context, rule: Rule) {
        dispatch!(self, auto_moderation_rule_update, ctx, rule);
    }

    async fn auto_moderation_rule_delete(&self, ctx: Context, rule: Rule) {
        dispatch!(self, auto_moderation_rule_delete, ctx, rule);
    }

    async fn auto_moderation_action_execution(&self, ctx: Context,
            execution: ActionExecution) {
        dispatch!(self, auto_moderation_action_execution, ctx, execution);
    }

    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        dispatch!(self, cache_ready, ctx, guilds);
    }

    async fn category_create(&self, ctx: Context, category: &ChannelCategory) {
        dispatch_copy!(self, category_create, ctx, category);
    }

    async fn category_delete(&self, ctx: Context, category: &ChannelCategory) {
        dispatch_copy!(self, category_delete, ctx, category);
    }

    async fn channel_create(&self, ctx: Context, channel: &GuildChannel) {
        dispatch_copy!(self, channel_create, ctx, channel);
    }

    async fn channel_delete(&self, ctx: Context, channel: &GuildChannel) {
        dispatch_copy!(self, channel_delete, ctx, channel);
    }

    async fn channel_pins_update(&self, ctx: Context,
            pin: ChannelPinsUpdateEvent) {
        dispatch!(self, channel_pins_update, ctx, pin);
    }

    async fn channel_update(&self, ctx: Context, old: Option<Channel>,
            new: Channel) {
        dispatch!(self, channel_update, ctx, old, new);
    }

    async fn guild_ban_addition(&self, ctx: Context, guild_id: GuildId,
            banned_user: User) {
        dispatch!(self, guild_ban_addition, ctx, guild_id, banned_user);
    }

    async fn guild_ban_removal(&self, ctx: Context, guild_id: GuildId,
            unbanned_user: User) {
        dispatch!(self, guild_ban_removal, ctx, guild_id, unbanned_user);
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, is_new: bool) {
        dispatch!(self, guild_create, ctx, guild, is_new);
    }

    async fn guild_delete(&self, ctx: Context, incomplete: UnavailableGuild,
            full: Option<Guild>) {
        dispatch!(self, guild_delete, ctx, incomplete, full);
    }

    async fn guild_emojis_update(&self, ctx: Context, guild_id: GuildId,
            current_state: HashMap<EmojiId, Emoji>) {
        dispatch!(self, guild_emojis_update, ctx, guild_id, current_state);
    }

    async fn guild_integrations_update(&self, ctx: Context,
            guild_id: GuildId) {
        dispatch!(self, guild_integrations_update, ctx, guild_id);
    }

    async fn guild_member_addition(&self, ctx: Context, new_member: Member) {
        dispatch!(self, guild_member_addition, ctx, new_member);
    }

    async fn guild_member_removal(&self, ctx: Context, guild_id: GuildId,
            user: User, member_data_if_available: Option<Member>) {
        dispatch!(self, guild_member_removal,
            ctx, guild_id, user, member_data_if_available);
    }

    async fn guild_member_update(&self, ctx: Context,
            old_if_available: Option<Member>, new: Member) {
        dispatch!(self, guild_member_update, ctx, old_if_available, new);
    }

    async fn guild_members_chunk(&self, ctx: Context,
            chunk: GuildMembersChunkEvent) {
        dispatch!(self, guild_members_chunk, ctx, chunk);
    }

    async fn guild_role_create(&self, ctx: Context, new: Role) {
        dispatch!(self, guild_role_create, ctx, new);
    }

    async fn guild_role_delete(&self, ctx: Context, guild_id: GuildId,
            removed_role_id: RoleId,
            removed_role_data_if_available: Option<Role>) {
        dispatch!(self, guild_role_delete,
            ctx, guild_id, removed_role_id, removed_role_data_if_available);
    }

    async fn guild_role_update(&self, ctx: Context,
            old_data_if_available: Option<Role>, new: Role) {
        dispatch!(self, guild_role_update, ctx, old_data_if_available, new);
    }

    async fn guild_scheduled_event_create(&self, ctx: Context,
            event: ScheduledEvent) {
        dispatch!(self, guild_scheduled_event_create, ctx, event);
    }

    async fn guild_scheduled_event_delete(&self, ctx: Context,
            event: ScheduledEvent) {
        dispatch!(self, guild_scheduled_event_delete, ctx, event);
    }

    async fn guild_scheduled_event_update(&self, ctx: Context,
            event: ScheduledEvent) {
        dispatch!(self, guild_scheduled_event_update, ctx, event);
    }

    async fn guild_scheduled_event_user_add(&self, ctx: Context,
            subscribed: GuildScheduledEventUserAddEvent) {
        dispatch!(self, guild_scheduled_event_user_add, ctx, subscribed);
    }

    async fn guild_scheduled_event_user_remove(&self, ctx: Context,
            unsubscribed: GuildScheduledEventUserRemoveEvent) {
        dispatch!(self, guild_scheduled_event_user_remove, ctx, unsubscribed);
    }

    async fn guild_stickers_update(&self, ctx: Context, guild_id: GuildId,
            current_state: HashMap<StickerId, Sticker>) {
        dispatch!(self, guild_stickers_update, ctx, guild_id, current_state);
    }

    async fn guild_unavailable(&self, ctx: Context, guild_id: GuildId) {
        dispatch!(self, guild_unavailable, ctx, guild_id);
    }

    async fn guild_update(&self, ctx: Context,
            old_data_if_available: Option<Guild>,
            new_but_incomplete: PartialGuild) {
        dispatch!(self, guild_update,
            ctx, old_data_if_available, new_but_incomplete);
    }

    async fn interaction_create(&self, ctx: Context,
            interaction: Interaction) {
        dispatch!(self, interaction_create, ctx, interaction);
    }

    async fn integration_create(&self, ctx: Context,
            integration: Integration) {
        dispatch!(self, integration_create, ctx, integration);
    }

    async fn integration_delete(&self, ctx: Context,
            integration_id: IntegrationId, guild_id: GuildId,
            application_id: Option<ApplicationId>) {
        dispatch!(self, integration_delete,
            ctx, integration_id, guild_id, application_id);
    }

    async fn integration_update(&self, ctx: Context,
            integration: Integration) {
        dispatch!(self, integration_update, ctx, integration);
    }

    async fn invite_create(&self, ctx: Context, data: InviteCreateEvent) {
        dispatch!(self, invite_create, ctx, data);
    }

    async fn invite_delete(&self, ctx: Context, data: InviteDeleteEvent) {
        dispatch!(self, invite_delete, ctx, data);
    }

    async fn message(&self, ctx: Context, new_message: Message) {
        dispatch!(self, message, ctx, new_message);
    }

    async fn message_delete(&self, ctx: Context, channel_id: ChannelId,
            deleted_message_id: MessageId, guild_id: Option<GuildId>) {
        dispatch!(self, message_delete,
            ctx, channel_id, deleted_message_id, guild_id);
    }

    async fn message_delete_bulk(&self, ctx: Context, channel_id: ChannelId,
            multiple_deleted_message_ids: Vec<MessageId>,
            guild_id: Option<GuildId>) {
        dispatch!(self, message_delete_bulk,
            ctx, channel_id, multiple_deleted_message_ids, guild_id);
    }

    async fn message_update(&self, ctx: Context,
            old_if_available: Option<Message>, new: Option<Message>,
            event: MessageUpdateEvent) {
        dispatch!(self, message_update, ctx, old_if_available, new, event);
    }

    async fn presence_replace(&self, ctx: Context, arg2: Vec<Presence>) {
        dispatch!(self, presence_replace, ctx, arg2);
    }

    async fn presence_update(&self, ctx: Context, new_data: Presence) {
        dispatch!(self, presence_update, ctx, new_data);
    }

    async fn reaction_add(&self, ctx: Context, add_reaction: Reaction) {
        dispatch!(self, reaction_add, ctx, add_reaction);
    }

    async fn reaction_remove(&self, ctx: Context, removed_reaction: Reaction) {
        dispatch!(self, reaction_remove, ctx, removed_reaction);
    }

    async fn reaction_remove_all(&self, ctx: Context, channel_id: ChannelId,
            removed_from_message_id: MessageId) {
        dispatch!(self, reaction_remove_all,
            ctx, channel_id, removed_from_message_id);
    }

    async fn ready(&self, ctx: Context, data_about_bot: Ready) {
        dispatch!(self, ready, ctx, data_about_bot);
    }

    async fn resume(&self, ctx: Context, resumed_event: ResumedEvent) {
        dispatch!(self, resume, ctx, resumed_event);
    }

    async fn shard_stage_update(&self, ctx: Context,
            arg2: ShardStageUpdateEvent) {
        dispatch!(self, shard_stage_update, ctx, arg2);
    }

    async fn stage_instance_create(&self, ctx: Context,
            stage_instance: StageInstance) {
        dispatch!(self, stage_instance_create, ctx, stage_instance);
    }

    async fn stage_instance_delete(&self, ctx: Context,
            stage_instance: StageInstance) {
        dispatch!(self, stage_instance_delete, ctx, stage_instance);
    }

    async fn stage_instance_update(&self, ctx: Context,
            stage_instance: StageInstance) {
        dispatch!(self, stage_instance_update, ctx, stage_instance);
    }

    async fn thread_create(&self, ctx: Context, thread: GuildChannel) {
        dispatch!(self, thread_create, ctx, thread);
    }

    async fn thread_delete(&self, ctx: Context, thread: PartialGuildChannel) {
        dispatch!(self, thread_delete, ctx, thread);
    }

    async fn thread_list_sync(&self, ctx: Context,
            thread_list_sync: ThreadListSyncEvent) {
        dispatch!(self, thread_list_sync, ctx, thread_list_sync);
    }

    async fn thread_member_update(&self, ctx: Context,
            thread_member: ThreadMember) {
        dispatch!(self, thread_member_update, ctx, thread_member);
    }

    async fn thread_members_update(&self, ctx: Context,
            thread_members_update: ThreadMembersUpdateEvent) {
        dispatch!(self, thread_members_update, ctx, thread_members_update);
    }

    async fn thread_update(&self, ctx: Context, thread: GuildChannel) {
        dispatch!(self, thread_update, ctx, thread);
    }

    async fn typing_start(&self, ctx: Context, arg2: TypingStartEvent) {
        dispatch!(self, typing_start, ctx, arg2);
    }

    async fn unknown(&self, ctx: Context, name: String, raw: Value) {
        dispatch!(self, unknown, ctx, name, raw);
    }

    async fn user_update(&self, ctx: Context, old_data: CurrentUser,
            new: CurrentUser) {
        dispatch!(self, user_update, ctx, old_data, new);
    }

    async fn voice_server_update(&self, ctx: Context,
            arg2: VoiceServerUpdateEvent) {
        dispatch!(self, voice_server_update, ctx, arg2);
    }

    async fn voice_state_update(&self, ctx: Context, old: Option<VoiceState>,
            new: VoiceState) {
        dispatch!(self, voice_state_update, ctx, old, new);
    }

    async fn webhook_update(&self, ctx: Context, guild_id: GuildId,
            belongs_to_channel_id: ChannelId) {
        dispatch!(self, webhook_update, ctx, guild_id, belongs_to_channel_id);
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

    /// Adds a new event handler to the composite. Events will be forwarded to
    /// this event handler after all previously pushed and before all
    /// subsequently pushed ones. Returns the altered composer for chaining.
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
