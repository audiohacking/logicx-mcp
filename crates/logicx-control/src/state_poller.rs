//! Background AX supplementary poller (logic-pro-mcp StatePoller parity).

use crate::cache::{
    ChannelStripState, MarkerState, ProjectInfo, StateCache, TrackState, TransportState,
};
use crate::channels::{AxChannel, ChannelResult, Params};
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const FAILURE_THRESHOLD: u32 = 3;
const MARKER_POLL_INTERVAL: u32 = 5;

/// Injectable poll source (production uses AX; tests inject JSON fixtures).
pub trait AxPollSource: Send + Sync {
    fn has_visible_window(&self) -> bool;
    fn dialog_present(&self) -> bool;
    fn poll_project_info(&self) -> Option<ProjectInfo>;
    fn poll_tracks(&self) -> Option<Vec<TrackState>>;
    fn poll_transport(&self) -> Option<TransportState>;
    fn poll_mixer_strips(&self) -> Option<Vec<ChannelStripState>>;
    fn poll_markers(&self) -> Option<Vec<MarkerState>>;
}

pub struct ProductionAxPollSource;

impl AxPollSource for ProductionAxPollSource {
    fn has_visible_window(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            crate::macos::is_logic_running()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    fn dialog_present(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            crate::macos::dialog_present()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    fn poll_project_info(&self) -> Option<ProjectInfo> {
        project_from_result(AxChannel::execute("project.get_info", &Params::new()))
    }

    fn poll_tracks(&self) -> Option<Vec<TrackState>> {
        tracks_from_result(AxChannel::execute("track.get_tracks", &Params::new()))
    }

    fn poll_transport(&self) -> Option<TransportState> {
        transport_from_result(AxChannel::execute("transport.get_state", &Params::new()))
    }

    fn poll_mixer_strips(&self) -> Option<Vec<ChannelStripState>> {
        strips_from_result(AxChannel::execute("mixer.get_state", &Params::new()))
    }

    fn poll_markers(&self) -> Option<Vec<MarkerState>> {
        markers_from_result(AxChannel::execute("nav.get_markers", &Params::new()))
    }
}

pub struct MockAxPollSource {
    pub has_window: bool,
    pub dialog: bool,
    pub project: Option<ProjectInfo>,
    pub tracks: Option<Vec<TrackState>>,
    pub transport: Option<TransportState>,
    pub strips: Option<Vec<ChannelStripState>>,
    pub markers: Option<Vec<MarkerState>>,
}

impl AxPollSource for MockAxPollSource {
    fn has_visible_window(&self) -> bool {
        self.has_window
    }

    fn dialog_present(&self) -> bool {
        self.dialog
    }

    fn poll_project_info(&self) -> Option<ProjectInfo> {
        self.project.clone()
    }

    fn poll_tracks(&self) -> Option<Vec<TrackState>> {
        self.tracks.clone()
    }

    fn poll_transport(&self) -> Option<TransportState> {
        self.transport.clone()
    }

    fn poll_mixer_strips(&self) -> Option<Vec<ChannelStripState>> {
        self.strips.clone()
    }

    fn poll_markers(&self) -> Option<Vec<MarkerState>> {
        self.markers.clone()
    }
}

pub struct StatePoller {
    cache: Arc<StateCache>,
    source: Arc<dyn AxPollSource>,
    running: AtomicBool,
    handle: Mutex<Option<JoinHandle<()>>>,
    interval: Duration,
    loop_state: Mutex<PollerCounters>,
}

#[derive(Default)]
struct PollerCounters {
    consecutive_window_misses: u32,
    consecutive_poll_misses: u32,
    marker_poll_tick: u32,
}

impl StatePoller {
    pub fn production(cache: Arc<StateCache>) -> Self {
        Self::new(cache, Arc::new(ProductionAxPollSource), POLL_INTERVAL)
    }

    pub fn new(cache: Arc<StateCache>, source: Arc<dyn AxPollSource>, interval: Duration) -> Self {
        Self {
            cache,
            source,
            running: AtomicBool::new(false),
            handle: Mutex::new(None),
            interval,
            loop_state: Mutex::new(PollerCounters {
                marker_poll_tick: MARKER_POLL_INTERVAL - 1,
                ..Default::default()
            }),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let cache = Arc::clone(&self.cache);
        let source = Arc::clone(&self.source);
        let interval = self.interval;
        let loop_state = &self.loop_state as *const Mutex<PollerCounters> as usize;
        let running = &self.running as *const AtomicBool as usize;
        let handle = thread::spawn(move || {
            let running = unsafe { &*(running as *const AtomicBool) };
            let loop_state = unsafe { &*(loop_state as *const Mutex<PollerCounters>) };
            while running.load(Ordering::SeqCst) {
                {
                    let mut counters = loop_state.lock();
                    poll_once(&cache, source.as_ref(), &mut counters);
                }
                thread::sleep(interval);
            }
        });
        *self.handle.lock() = Some(handle);
    }

    pub fn stop(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        if let Some(h) = self.handle.lock().take() {
            let _ = h.join();
        }
    }

    pub fn refresh_now(&self) {
        let mut counters = self.loop_state.lock();
        poll_once(&self.cache, self.source.as_ref(), &mut counters);
    }
}

static GLOBAL: once_cell::sync::Lazy<Mutex<Option<Arc<StatePoller>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

static CACHE: once_cell::sync::Lazy<Mutex<Option<Arc<StateCache>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

pub fn register_cache(cache: Arc<StateCache>) {
    *CACHE.lock() = Some(cache);
}

pub fn ensure_started() {
    let cache = CACHE.lock().clone();
    let Some(cache) = cache else {
        return;
    };
    let mut guard = GLOBAL.lock();
    if guard.is_none() {
        let poller = Arc::new(StatePoller::production(cache));
        poller.start();
        *guard = Some(poller);
    }
}

fn poll_once(cache: &StateCache, source: &dyn AxPollSource, counters: &mut PollerCounters) {
    if !source.has_visible_window() {
        counters.consecutive_window_misses += 1;
        if counters.consecutive_window_misses >= FAILURE_THRESHOLD {
            cache.set_document_open(false);
            cache.set_ax_occluded(false);
        }
        return;
    }
    counters.consecutive_window_misses = 0;

    let project_ready = source
        .poll_project_info()
        .map(|p| {
            cache.update_project(p);
            true
        })
        .unwrap_or(false);
    let tracks_ready = source
        .poll_tracks()
        .map(|t| {
            cache.replace_tracks(t);
            true
        })
        .unwrap_or(false);
    let has_document = project_ready || tracks_ready;

    if has_document {
        counters.consecutive_poll_misses = 0;
        cache.set_document_open(true);
        cache.set_ax_occluded(false);
    } else if source.dialog_present() {
        cache.set_ax_occluded(true);
        return;
    } else {
        counters.consecutive_poll_misses += 1;
        if counters.consecutive_poll_misses >= FAILURE_THRESHOLD {
            cache.set_document_open(false);
            cache.set_ax_occluded(false);
            cache.clear_markers();
        }
    }

    if !has_document {
        return;
    }

    if let Some(t) = source.poll_transport() {
        cache.update_transport(t);
    }
    if let Some(strips) = source.poll_mixer_strips() {
        cache.replace_channel_strips(strips);
    }
    counters.marker_poll_tick += 1;
    if counters.marker_poll_tick >= MARKER_POLL_INTERVAL {
        counters.marker_poll_tick = 0;
        if let Some(markers) = source.poll_markers() {
            cache.update_markers(markers);
        }
    }
}

fn project_from_result(result: ChannelResult) -> Option<ProjectInfo> {
    let detail = success_detail(result)?;
    Some(ProjectInfo {
        name: detail
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
        track_count: detail
            .get("track_count")
            .or_else(|| detail.get("trackCount"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    })
}

fn tracks_from_result(result: ChannelResult) -> Option<Vec<TrackState>> {
    let detail = success_detail(result)?;
    let arr = detail
        .as_array()
        .or_else(|| detail.get("tracks").and_then(|v| v.as_array()))?;
    Some(
        arr.iter()
            .map(|v| TrackState {
                id: v.get("id").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                name: v.get("name").and_then(|x| x.as_str()).unwrap_or("").into(),
                is_selected: v
                    .get("is_selected")
                    .or_else(|| v.get("isSelected"))
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                automation_mode: v
                    .get("automation_mode")
                    .or_else(|| v.get("automationMode"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("off")
                    .into(),
            })
            .collect(),
    )
}

fn transport_from_result(result: ChannelResult) -> Option<TransportState> {
    let detail = success_detail(result)?;
    Some(TransportState {
        is_playing: detail
            .get("is_playing")
            .or_else(|| detail.get("isPlaying"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        is_recording: detail
            .get("is_recording")
            .or_else(|| detail.get("isRecording"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        tempo: detail
            .get("tempo")
            .and_then(|v| v.as_f64())
            .unwrap_or(120.0),
        position: detail
            .get("position")
            .and_then(|v| v.as_str())
            .unwrap_or("1.1.1.1")
            .into(),
        last_updated_secs: unix_now(),
    })
}

fn strips_from_result(result: ChannelResult) -> Option<Vec<ChannelStripState>> {
    let detail = success_detail(result)?;
    let arr = detail
        .as_array()
        .or_else(|| detail.get("strips").and_then(|v| v.as_array()))?;
    Some(
        arr.iter()
            .map(|v| ChannelStripState {
                track_index: v
                    .get("track_index")
                    .or_else(|| v.get("trackIndex"))
                    .or_else(|| v.get("index"))
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0) as u32,
                volume: v.get("volume").and_then(|x| x.as_f64()).unwrap_or(0.0),
                pan: v.get("pan").and_then(|x| x.as_f64()).unwrap_or(0.0),
            })
            .collect(),
    )
}

fn markers_from_result(result: ChannelResult) -> Option<Vec<MarkerState>> {
    let detail = success_detail(result)?;
    let arr = detail
        .as_array()
        .or_else(|| detail.get("markers").and_then(|v| v.as_array()))?;
    Some(
        arr.iter()
            .map(|v| MarkerState {
                id: v.get("id").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                name: v.get("name").and_then(|x| x.as_str()).unwrap_or("").into(),
                position: v
                    .get("position")
                    .and_then(|x| x.as_str())
                    .unwrap_or("1.1.1.1")
                    .into(),
            })
            .collect(),
    )
}

fn success_detail(result: ChannelResult) -> Option<Value> {
    match result {
        ChannelResult::Success {
            detail: Some(v), ..
        } => Some(v),
        ChannelResult::Success { message, .. } => serde_json::from_str(&message).ok(),
        _ => None,
    }
}

fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poller_updates_project_on_refresh() {
        let cache = Arc::new(StateCache::new());
        let source = Arc::new(MockAxPollSource {
            has_window: true,
            dialog: false,
            project: Some(ProjectInfo {
                name: "Session A".into(),
                track_count: 18,
            }),
            tracks: None,
            transport: None,
            strips: None,
            markers: None,
        });
        let poller = StatePoller::new(Arc::clone(&cache), source, Duration::from_millis(1));
        poller.refresh_now();
        assert_eq!(cache.get_project().name, "Session A");
        assert!(cache.has_document_open());
    }

    #[test]
    fn poller_clears_document_after_three_window_misses() {
        let cache = Arc::new(StateCache::new());
        cache.update_project(ProjectInfo {
            name: "Old".into(),
            track_count: 1,
        });
        cache.set_document_open(true);
        let source = Arc::new(MockAxPollSource {
            has_window: false,
            dialog: false,
            project: None,
            tracks: None,
            transport: None,
            strips: None,
            markers: None,
        });
        let poller = StatePoller::new(Arc::clone(&cache), source, Duration::from_millis(1));
        poller.refresh_now();
        poller.refresh_now();
        assert!(cache.has_document_open());
        poller.refresh_now();
        assert!(!cache.has_document_open());
    }

    #[test]
    fn poller_populates_markers_every_fifth_tick() {
        let cache = Arc::new(StateCache::new());
        let source = Arc::new(MockAxPollSource {
            has_window: true,
            dialog: false,
            project: Some(ProjectInfo {
                name: "M".into(),
                track_count: 1,
            }),
            tracks: Some(vec![TrackState {
                id: 0,
                name: "T".into(),
                ..Default::default()
            }]),
            transport: None,
            strips: None,
            markers: Some(vec![MarkerState {
                id: 0,
                name: "Intro".into(),
                position: "1.1.1.1".into(),
            }]),
        });
        let poller = StatePoller::new(Arc::clone(&cache), source, Duration::from_millis(1));
        poller.refresh_now();
        assert_eq!(cache.get_markers().len(), 1);
        assert_eq!(cache.get_markers()[0].name, "Intro");
    }

    #[test]
    fn dialog_present_preserves_document_during_occlusion() {
        let cache = Arc::new(StateCache::new());
        cache.set_document_open(true);
        let source = Arc::new(MockAxPollSource {
            has_window: true,
            dialog: true,
            project: None,
            tracks: None,
            transport: None,
            strips: None,
            markers: None,
        });
        let poller = StatePoller::new(Arc::clone(&cache), source, Duration::from_millis(1));
        poller.refresh_now();
        assert!(cache.has_document_open());
        assert!(cache.ax_occluded());
    }
}
