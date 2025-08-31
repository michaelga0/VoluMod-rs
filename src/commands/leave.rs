use serenity::{
    all::{CommandInteraction, Context, InteractionResponseFlags},
    builder::{
        CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage,
    },
};

use crate::{
    audio,
    commands::command::{Command, CommandFuture},
};

pub struct Leave;
pub const LEAVE: Leave = Leave;

impl Command for Leave {
    fn name(&self) -> &'static str {
        "leave"
    }

    fn register(&self) -> CreateCommand {
        CreateCommand::new(self.name()).description("Leaves the current voice channel")
    }

    fn run<'a>(&'a self, ctx: &'a Context, itx: &'a CommandInteraction) -> CommandFuture<'a> {
        Box::pin(async move {
            let guild = itx.guild_id.ok_or(serenity::Error::Other("no guild"))?;
            audio::leave(ctx, guild).await.map_err(|_| serenity::Error::Other("voice leave failed"))?;
            itx.create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new().content("Left the voice channel.").flags(InteractionResponseFlags::EPHEMERAL),
                ),
            )
            .await
        })
    }
}
