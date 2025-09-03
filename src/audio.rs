use anyhow::Context as _;
use serenity::all::{ChannelId, Context, GuildId};
use songbird::events::CoreEvent;

pub mod monitor;

pub async fn join(ctx: &Context, guild: GuildId, channel: ChannelId) -> anyhow::Result<()> {
    let manager = songbird::get(ctx).await.context("songbird not initialised")?;
    let call = manager.join(guild, channel).await?;
    {
        let mut lock = call.lock().await;
        let monitor = monitor::Monitor::new(guild);
        lock.add_global_event(CoreEvent::RtpPacket.into(), monitor.clone());
        // Track speaking updates to map SSRC to UserId.
        lock.add_global_event(CoreEvent::SpeakingStateUpdate.into(), monitor);
    }
    Ok(())
}

pub async fn leave(ctx: &Context, guild: GuildId) -> anyhow::Result<()> {
    let manager = songbird::get(ctx).await.context("songbird not initialised")?;
    manager.remove(guild).await?;
    Ok(())
}
