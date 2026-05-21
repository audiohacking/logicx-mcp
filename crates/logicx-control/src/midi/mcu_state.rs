//! Shared MCU feedback state (logic-pro-mcp StateCache MCU fields).

use parking_lot::RwLock;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Default)]
pub struct ChannelStripState {
    pub volume: Option<f64>,
    pub pan: Option<f64>,
    pub volume_at: Option<u64>,
    pub pan_at: Option<u64>,
}

#[derive(Clone, Default)]
pub struct McuConnectionState {
    pub is_connected: bool,
    pub registered_as_device: bool,
    pub port_name: String,
    pub last_feedback_at: Option<u64>,
}

#[derive(Default)]
struct Inner {
    connection: McuConnectionState,
    strips: [ChannelStripState; 9],
    transport_play: bool,
    transport_record: bool,
}

pub struct McuStateCache {
    inner: RwLock<Inner>,
}

impl Default for McuStateCache {
    fn default() -> Self {
        Self {
            inner: RwLock::new(Inner::default()),
        }
    }
}

impl McuStateCache {
    pub fn global() -> Arc<Self> {
        static CACHE: once_cell::sync::Lazy<Arc<McuStateCache>> =
            once_cell::sync::Lazy::new(|| Arc::new(McuStateCache::default()));
        Arc::clone(&CACHE)
    }

    pub fn touch_feedback(&self) {
        let mut g = self.inner.write();
        let now = unix_now();
        g.connection.is_connected = true;
        g.connection.registered_as_device = true;
        g.connection.last_feedback_at = Some(now);
        if g.connection.port_name.is_empty() {
            g.connection.port_name = super::mcu_protocol::PORT_NAME.into();
        }
    }

    pub fn set_disconnected(&self) {
        let mut g = self.inner.write();
        g.connection.is_connected = false;
    }

    pub fn update_fader(&self, strip: usize, volume: f64) {
        if strip >= 9 {
            return;
        }
        let mut g = self.inner.write();
        g.strips[strip].volume = Some(volume.clamp(0.0, 1.0));
        g.strips[strip].volume_at = Some(unix_now());
        self.touch_feedback_inner(&mut g);
    }

    pub fn update_pan(&self, strip: usize, pan: f64) {
        if strip >= 9 {
            return;
        }
        let mut g = self.inner.write();
        g.strips[strip].pan = Some(pan.clamp(-1.0, 1.0));
        g.strips[strip].pan_at = Some(unix_now());
        self.touch_feedback_inner(&mut g);
    }

    pub fn update_transport_leds(&self, play: bool, record: bool) {
        let mut g = self.inner.write();
        g.transport_play = play;
        g.transport_record = record;
        self.touch_feedback_inner(&mut g);
    }

    fn touch_feedback_inner(&self, g: &mut Inner) {
        let now = unix_now();
        g.connection.is_connected = true;
        g.connection.last_feedback_at = Some(now);
    }

    pub fn connection(&self) -> McuConnectionState {
        self.inner.read().connection.clone()
    }

    pub fn is_feedback_fresh(&self, max_age_secs: u64) -> bool {
        let g = self.inner.read();
        let Some(ts) = g.connection.last_feedback_at else {
            return false;
        };
        unix_now().saturating_sub(ts) <= max_age_secs
    }

    pub fn get_fader(&self, strip: usize) -> Option<f64> {
        self.inner.read().strips.get(strip)?.volume
    }

    pub fn summary(&self) -> Value {
        let g = self.inner.read();
        let age = g
            .connection
            .last_feedback_at
            .map(|ts| unix_now().saturating_sub(ts));
        json!({
            "is_connected": g.connection.is_connected,
            "registered_as_device": g.connection.registered_as_device,
            "port_name": g.connection.port_name,
            "last_feedback_age_secs": age,
            "transport": {
                "play": g.transport_play,
                "record": g.transport_record,
            },
            "strips": (0..9).map(|i| {
                json!({
                    "index": i,
                    "volume": g.strips[i].volume,
                    "pan": g.strips[i].pan,
                })
            }).collect::<Vec<_>>(),
        })
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn poll_fader_echo(
    cache: &McuStateCache,
    strip: usize,
    target: f64,
    timeout_ms: u64,
    tolerance: f64,
) -> Option<f64> {
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if let Some(observed) = cache.get_fader(strip)
            && (observed - target).abs() <= tolerance
        {
            return Some(observed);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    cache.get_fader(strip)
}
