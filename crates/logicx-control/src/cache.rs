use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TrackState {
    pub id: u32,
    pub name: String,
    pub is_selected: bool,
    pub automation_mode: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ChannelStripState {
    pub track_index: u32,
    pub volume: f64,
    pub pan: f64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MarkerState {
    pub id: u32,
    pub name: String,
    pub position: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RegionState {
    pub id: String,
    pub name: String,
    pub track_index: u32,
    pub start_position: String,
    pub end_position: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub track_count: u32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TransportState {
    pub is_playing: bool,
    pub is_recording: bool,
    pub tempo: f64,
    pub position: String,
    pub last_updated_secs: u64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CacheSnapshot {
    pub poll_mode: String,
    pub region_count: usize,
    pub marker_count: usize,
    pub project_name: String,
    pub transport_age: i64,
}

#[derive(Default)]
pub struct StateCache {
    inner: RwLock<CacheData>,
}

#[derive(Clone)]
struct CacheData {
    last_refresh: Option<String>,
    transport: Value,
    tracks: Value,
    mixer: Value,
    markers: Value,
    project: Value,
    tracks_vec: Vec<TrackState>,
    strips: HashMap<u32, ChannelStripState>,
    markers_vec: Vec<MarkerState>,
    regions_vec: Vec<RegionState>,
    project_info: ProjectInfo,
    transport_state: TransportState,
    poll_mode: String,
    document_open: bool,
}

impl Default for CacheData {
    fn default() -> Self {
        Self {
            last_refresh: None,
            transport: Value::Null,
            tracks: Value::Null,
            mixer: Value::Null,
            markers: Value::Null,
            project: Value::Null,
            tracks_vec: Vec::new(),
            strips: HashMap::new(),
            markers_vec: Vec::new(),
            regions_vec: Vec::new(),
            project_info: ProjectInfo::default(),
            transport_state: TransportState::default(),
            poll_mode: "idle".into(),
            document_open: false,
        }
    }
}

impl StateCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn refresh(&self) {
        #[cfg(target_os = "macos")]
        {
            let mcu = crate::midi::mcu_state::McuStateCache::global();
            let mut guard = self.inner.write().expect("cache lock");
            guard.last_refresh = Some(chrono_lite_now());
            guard.transport = json!({
                "logic_running": crate::macos::is_logic_running(),
                "mcu": mcu.summary(),
            });
            guard.mixer = json!({
                "source": "mcu_feedback",
                "strips": mcu.summary().get("strips").cloned().unwrap_or(json!([])),
            });
            guard.tracks = crate::macos::get_tracks()
                .detail
                .unwrap_or(json!({}));
            guard.project = crate::macos::project_info()
                .detail
                .unwrap_or(json!({}));
            guard.markers = crate::macos::get_markers()
                .detail
                .unwrap_or(json!({ "markers": [] }));
        }
    }

    pub fn update_document_state(&self, open: bool) {
        let mut guard = self.inner.write().expect("cache lock");
        guard.document_open = open;
    }

    pub fn has_document_open(&self) -> bool {
        self.inner.read().expect("cache lock").document_open
    }

    pub fn update_track<F: FnOnce(&mut TrackState)>(&self, index: i32, f: F) {
        if index < 0 {
            return;
        }
        let idx = index as u32;
        let mut guard = self.inner.write().expect("cache lock");
        while guard.tracks_vec.len() <= idx as usize {
            let id = guard.tracks_vec.len() as u32;
            guard.tracks_vec.push(TrackState {
                id,
                name: format!("Track {}", id + 1),
                ..Default::default()
            });
        }
        f(&mut guard.tracks_vec[idx as usize]);
    }

    pub fn get_tracks(&self) -> Vec<TrackState> {
        self.inner.read().expect("cache lock").tracks_vec.clone()
    }

    pub fn get_track(&self, index: i32) -> Option<TrackState> {
        if index < 0 {
            return None;
        }
        self.inner
            .read()
            .expect("cache lock")
            .tracks_vec
            .get(index as usize)
            .cloned()
    }

    pub fn get_selected_track(&self) -> Option<TrackState> {
        self.inner
            .read()
            .expect("cache lock")
            .tracks_vec
            .iter()
            .find(|t| t.is_selected)
            .cloned()
    }

    pub fn select_only(&self, track_at: i32) {
        if track_at < 0 {
            return;
        }
        let mut guard = self.inner.write().expect("cache lock");
        for (i, t) in guard.tracks_vec.iter_mut().enumerate() {
            t.is_selected = i as i32 == track_at;
        }
    }

    pub fn update_fader(&self, strip: i32, volume: f64) {
        if strip < 0 {
            return;
        }
        let mut guard = self.inner.write().expect("cache lock");
        guard
            .strips
            .entry(strip as u32)
            .or_default()
            .track_index = strip as u32;
        guard.strips.get_mut(&(strip as u32)).unwrap().volume = volume;
    }

    pub fn get_channel_strip(&self, index: i32) -> Option<ChannelStripState> {
        if index < 0 {
            return None;
        }
        self.inner
            .read()
            .expect("cache lock")
            .strips
            .get(&(index as u32))
            .cloned()
    }

    pub fn get_channel_strips(&self) -> Vec<ChannelStripState> {
        self.inner
            .read()
            .expect("cache lock")
            .strips
            .values()
            .cloned()
            .collect()
    }

    pub fn update_markers(&self, markers: Vec<MarkerState>) {
        let mut guard = self.inner.write().expect("cache lock");
        guard.markers_vec = markers.clone();
        guard.markers = json!({ "markers": markers });
    }

    pub fn get_markers(&self) -> Vec<MarkerState> {
        self.inner.read().expect("cache lock").markers_vec.clone()
    }

    pub fn update_regions(&self, regions: Vec<RegionState>) {
        let mut guard = self.inner.write().expect("cache lock");
        guard.regions_vec = regions;
    }

    pub fn get_regions(&self) -> Vec<RegionState> {
        self.inner.read().expect("cache lock").regions_vec.clone()
    }

    pub fn update_project(&self, project: ProjectInfo) {
        let mut guard = self.inner.write().expect("cache lock");
        guard.project_info = project.clone();
        guard.project = json!({
            "name": project.name,
            "track_count": project.track_count,
        });
    }

    pub fn get_project(&self) -> ProjectInfo {
        self.inner.read().expect("cache lock").project_info.clone()
    }

    pub fn update_transport(&self, transport: TransportState) {
        let mut guard = self.inner.write().expect("cache lock");
        guard.transport_state = transport.clone();
        guard.transport = json!({
            "is_playing": transport.is_playing,
            "is_recording": transport.is_recording,
            "tempo": transport.tempo,
            "position": transport.position,
        });
    }

    pub fn get_transport(&self) -> TransportState {
        self.inner.read().expect("cache lock").transport_state.clone()
    }

    pub fn clear_project_state(&self) {
        let mut guard = self.inner.write().expect("cache lock");
        guard.transport_state = TransportState {
            tempo: 120.0,
            position: "1.1.1.1".into(),
            ..Default::default()
        };
        guard.project_info = ProjectInfo::default();
        guard.regions_vec.clear();
        guard.markers_vec.clear();
        guard.tracks_vec.clear();
        guard.strips.clear();
    }

    pub fn record_tool_access(&self) {
        let mut guard = self.inner.write().expect("cache lock");
        guard.poll_mode = "active".into();
    }

    pub fn snapshot(&self) -> CacheSnapshot {
        let guard = self.inner.read().expect("cache lock");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let transport_age = now.saturating_sub(guard.transport_state.last_updated_secs as i64);
        CacheSnapshot {
            poll_mode: guard.poll_mode.clone(),
            region_count: guard.regions_vec.len(),
            marker_count: guard.markers_vec.len(),
            project_name: guard.project_info.name.clone(),
            transport_age,
        }
    }

    pub fn summary(&self) -> Value {
        let guard = self.inner.read().expect("cache lock");
        json!({
            "last_refresh": guard.last_refresh,
            "has_transport": !guard.transport.is_null(),
            "has_tracks": !guard.tracks.is_null(),
            "document_open": guard.document_open,
        })
    }

    pub fn resource_index(&self) -> Value {
        json!([
            "logic://system/health",
            "logic://transport/state",
            "logic://tracks",
            "logic://mixer",
            "logic://markers",
            "logic://project/info",
            "logic://midi/ports",
            "logic://mcu/state",
            "logic://library/inventory"
        ])
    }

    pub fn read_resource(&self, uri: &str) -> Value {
        let guard = self.inner.read().expect("cache lock");
        match uri {
            "logic://transport/state" => guard.transport.clone(),
            "logic://tracks" => guard.tracks.clone(),
            "logic://mixer" => guard.mixer.clone(),
            "logic://markers" => guard.markers.clone(),
            "logic://project/info" => guard.project.clone(),
            "logic://midi/ports" => crate::midi::engine::list_midi_ports(),
            "logic://mcu/state" => crate::midi::mcu_state::McuStateCache::global().summary(),
            "logic://system/health" => {
                let mut router = crate::channels::ChannelRouter::global().lock();
                router.ensure_started();
                let channels: serde_json::Map<String, Value> = router
                    .health_report()
                    .into_iter()
                    .map(|(k, h)| {
                        let status = if h.available && h.ready {
                            "ready"
                        } else if !h.available {
                            "unavailable"
                        } else {
                            "setup_required"
                        };
                        (k, json!({ "status": status, "detail": h.detail }))
                    })
                    .collect();
                json!({
                    "logic_pro_running": crate::macos::is_logic_running(),
                    "channels": channels,
                    "operator_approvals": crate::approvals::list(),
                })
            }
            "logic://library/inventory" => {
                #[cfg(target_os = "macos")]
                {
                    return crate::macos::get_tracks()
                        .detail
                        .unwrap_or_else(|| json!({ "status": "scan_library first" }));
                }
                #[cfg(not(target_os = "macos"))]
                json!({ "status": "macOS only" })
            }
            _ => json!({ "uri": uri, "status": "not_cached" }),
        }
    }
}

fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_and_strip_accessors() {
        let cache = StateCache::new();
        cache.update_track(2, |t| {
            t.name = "Lead Vox".into();
            t.is_selected = true;
            t.automation_mode = "touch".into();
        });
        cache.update_fader(2, 0.75);

        let tracks = cache.get_tracks();
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[0].name, "Track 1");
        assert_eq!(cache.get_selected_track().unwrap().id, 2);
        assert_eq!(cache.get_channel_strip(2).unwrap().volume, 0.75);
        assert!(cache.get_track(9).is_none());
        assert!(cache.get_channel_strip(9).is_none());
    }

    #[test]
    fn project_region_marker_snapshot() {
        let cache = StateCache::new();
        cache.update_transport(TransportState {
            last_updated_secs: 1,
            ..Default::default()
        });
        cache.update_regions(vec![RegionState {
            id: "r1".into(),
            name: "Verse".into(),
            track_index: 1,
            start_position: "1.1.1.1".into(),
            end_position: "9.1.1.1".into(),
        }]);
        cache.update_markers(vec![MarkerState {
            id: 1,
            name: "Hook".into(),
            position: "17.1.1.1".into(),
        }]);
        cache.update_project(ProjectInfo {
            name: "Enterprise Mix".into(),
            track_count: 24,
        });

        let idle = cache.snapshot();
        assert_eq!(idle.poll_mode, "idle");
        assert_eq!(idle.region_count, 1);
        assert_eq!(idle.marker_count, 1);
        assert_eq!(idle.project_name, "Enterprise Mix");

        cache.record_tool_access();
        assert_eq!(cache.snapshot().poll_mode, "active");
    }

    #[test]
    fn negative_indices_no_op() {
        let cache = StateCache::new();
        cache.update_track(-1, |t| t.name = "Bad".into());
        cache.update_fader(-1, 0.9);
        assert!(cache.get_tracks().is_empty());
        assert!(cache.get_channel_strips().is_empty());
    }

    #[test]
    fn clear_resets_transport_and_project() {
        let cache = StateCache::new();
        cache.update_transport(TransportState {
            is_playing: true,
            is_recording: true,
            tempo: 128.5,
            position: "5.3.2.1".into(),
            ..Default::default()
        });
        cache.update_project(ProjectInfo {
            name: "Stale".into(),
            track_count: 3,
        });
        cache.clear_project_state();
        let t = cache.get_transport();
        assert!(!t.is_playing);
        assert_eq!(t.position, "1.1.1.1");
        assert_eq!(t.tempo, 120.0);
        assert_eq!(cache.get_project().name, "");
    }

    #[test]
    fn select_only_enforces_single_selection() {
        let cache = StateCache::new();
        for i in 0..3 {
            cache.update_track(i, |t| t.is_selected = true);
        }
        cache.select_only(1);
        let tracks = cache.get_tracks();
        assert!(!tracks[0].is_selected);
        assert!(tracks[1].is_selected);
        assert!(!tracks[2].is_selected);
        assert_eq!(cache.get_selected_track().unwrap().id, 1);
    }
}
