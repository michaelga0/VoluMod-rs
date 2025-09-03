use std::env;

use anyhow::Context as AnyhowContext;
use async_trait::async_trait;
use serenity::all::{Client, Context, EventHandler, GatewayIntents, GuildId, Interaction, Ready};
use songbird::SerenityInit;
use tracing::{error, info};

mod commands;
mod db;
mod utils;
mod audio;
mod volume_listener;

struct Handler {
    pool: db::Pool,
    developer_mode: bool,
    test_guild: Option<GuildId>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);

        let res = if self.developer_mode {
            if let Some(gid) = self.test_guild {
                commands::register_guild_commands(&ctx.http, gid).await
            } else {
                Err(serenity::Error::Other("TEST_GUILD_ID not provided"))
            }
        } else {
            commands::register_global_commands(&ctx.http).await
        };

        if let Err(e) = res {
            error!(?e, "Slash‑command registration failed");
        }

        volume_listener::start(ctx.http.clone());
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(cmd) = interaction {
            commands::dispatch(&ctx, &cmd).await;
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    utils::init_tracing();

    let token = env::var("BOT_TOKEN").context("BOT_TOKEN missing")?;
    let developer_mode = env::var("DEVELOPER_MODE")
        .unwrap_or_else(|_| "false".into())
        .parse::<bool>()
        .unwrap_or(false);
    let test_guild = env::var("TEST_GUILD_ID")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(GuildId::new);

    let pool = db::init_pool().await?;

    let intents = GatewayIntents::non_privileged() 
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            pool,
            developer_mode,
            test_guild,
        })
        .register_songbird()
        .await?;

    if let Err(why) = client.start().await {
        error!(?why, "Client exited with error");
    }

    Ok(())
}
