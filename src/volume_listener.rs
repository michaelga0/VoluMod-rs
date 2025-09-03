use serenity::{builder::CreateMessage, http::Http};
use std::sync::{Arc, OnceLock};
use tracing::error;

use crate::audio::monitor;

static STARTED: OnceLock<()> = OnceLock::new();

pub fn start(http: Arc<Http>) {
    if STARTED.set(()).is_err() {
        return; 
    }

    tokio::spawn(async move {
        let mut rx = monitor::subscribe();
        loop {
            match rx.recv().await {
                Ok(evt) => {
                    if let Ok(dm) = evt.user_id.create_dm_channel(&http).await {
                        let _ = dm
                            .id
                            .send_message(
                                &http,
                                CreateMessage::new().content(
                                    "You are too loud. Please reduce your microphone volume.",
                                ),
                            )
                            .await;
                    }
                }
                Err(e) => {
                    error!(?e, "volume listener channel error; resubscribing");
                    rx = monitor::subscribe();
                }
            }
        }
    });
}
