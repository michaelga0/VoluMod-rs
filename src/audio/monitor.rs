use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use discortp::{rtp::RtpPacket, Packet};
use std::sync::OnceLock;
use opus::Decoder as OpusDecoder;
use serenity::all::{GuildId, UserId};
use songbird::events::{EventContext, EventHandler};
use tokio::sync::{broadcast, Mutex};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;

// 48k samples/sec, 2 channels, 16-bit => 192_000 bytes/sec
const WINDOW_BYTES: usize = (SAMPLE_RATE as usize) * 4; // 1 second window
const DEFAULT_THRESHOLD: f64 = 10_000.0;
const RECORDING_DELAY: Duration = Duration::from_millis(3000);

#[derive(Clone, Debug)]
pub struct ThresholdEvent {
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub rms: f64,
}

// Global broadcast for threshold events.
static EVENT_TX: OnceLock<broadcast::Sender<ThresholdEvent>> = OnceLock::new();

fn event_tx() -> broadcast::Sender<ThresholdEvent> {
    EVENT_TX
        .get_or_init(|| broadcast::channel(64).0)
        .clone()
}

pub fn subscribe() -> broadcast::Receiver<ThresholdEvent> {
    event_tx().subscribe()
}

struct Stream {
    dec: OpusDecoder,
    window: Vec<i16>,
    paused_until: Option<Instant>,
}

#[derive(Clone)]
pub struct Monitor {
    streams: Arc<Mutex<HashMap<u32, Stream>>>,
    // Track SSRC -> UserId
    speakers: Arc<Mutex<HashMap<u32, UserId>>>,
    threshold: f64,
    guild_id: GuildId,
}

impl Monitor {
    pub fn new(guild_id: GuildId) -> Self {
        Self {
            streams: Arc::new(Mutex::new(HashMap::new())),
            speakers: Arc::new(Mutex::new(HashMap::new())),
            threshold: DEFAULT_THRESHOLD,
            guild_id,
        }
    }

    fn compute_rms(window: &[i16]) -> f64 {
        if window.is_empty() {
            return 0.0;
        }
        let sum_sq: f64 = window.iter().map(|&s| {
            let x = s as f64;
            x * x
        }).sum();
        (sum_sq / (window.len() as f64)).sqrt()
    }
}

#[async_trait]
impl EventHandler for Monitor {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<songbird::events::Event> {
        match ctx {
            EventContext::SpeakingStateUpdate(update) => {
                let ssrc = update.ssrc as u32;
                if let Some(uid) = update.user_id {
                    self.speakers
                        .lock()
                        .await
                        .insert(ssrc, UserId::new(uid.0));
                }
                None
            }
            EventContext::RtpPacket(pkt) => {
                let rtp = RtpPacket::new(&pkt.packet)?;
                let ssrc = rtp.get_ssrc().into();

                let payload = rtp.payload();
                let start = pkt.payload_offset;
                let tail = pkt.payload_end_pad;
                if payload.len() < start + tail {
                    return None;
                }
                let opus_frame = &payload[start..payload.len() - tail];
                if opus_frame.is_empty() {
                    return None;
                }

                if !self.streams.lock().await.contains_key(&ssrc) {
                    let dec = OpusDecoder::new(SAMPLE_RATE, opus::Channels::Stereo).ok()?;
                    self.streams.lock().await.insert(
                        ssrc,
                        Stream {
                            dec,
                            window: Vec::with_capacity(WINDOW_BYTES / 2),
                            paused_until: None,
                        },
                    );
                }

                if let Some(stream) = self.streams.lock().await.get_mut(&ssrc) {
                    if let Some(until) = stream.paused_until {
                        if Instant::now() < until {
                            return None;
                        } else {
                            stream.paused_until = None;
                        }
                    }

                    let mut pcm = [0i16; 1920];
                    if let Ok(samples_per_ch) = stream.dec.decode(opus_frame, &mut pcm, false) {
                        let total_i16 = samples_per_ch as usize * CHANNELS;
                        stream.window.extend_from_slice(&pcm[..total_i16]);

                        if stream.window.len() * 2 >= WINDOW_BYTES {
                            let rms = Self::compute_rms(&stream.window);
                            if rms > self.threshold {
                                if let Some(user_id) = self.speakers.lock().await.get(&ssrc).cloned() {
                                    let _ = event_tx().send(ThresholdEvent {
                                        guild_id: self.guild_id,
                                        user_id,
                                        rms,
                                    });
                                }
                                stream.window.clear();
                                stream.paused_until = Some(Instant::now() + RECORDING_DELAY);
                            } else {
                                stream.window.clear();
                            }
                        }
                    }
                }

                None
            }
            _ => None,
        }
    }
}
