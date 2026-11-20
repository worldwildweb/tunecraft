//! The main window: service pane, media page (filters + track list), play queue,
//! album art and the control pane with the faceplate.

use crate::library::{
    self, Condition, Field, Library, Op, Playlist, PlaylistKind, ScanMsg, SmartRules, Track,
};
use crate::player::{self, Audio, Sequencer, Source, State};
use crate::theme::{self, gray};
use crate::web::{self, FetchMsg, Web, WebEvent};
use egui::{Color32, FontId, Key, Modifiers, Rect, Stroke, Ui, pos2, vec2};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Col {
    Title,
    Time,
    Artist,
    Album,
    Genre,
    Rating,
    Type,
}

/// Songbird's fallback column spec (sbColumnSpecParser.jsm): widths in px.
pub const COLUMNS: [(Col, &str, f32); 7] = [
    (Col::Title, "Title", 229.0),
    (Col::Time, "Time", 45.0),
    (Col::Artist, "Artist", 137.0),
    (Col::Album, "Album", 210.0),
    (Col::Genre, "Genre", 90.0),
    (Col::Rating, "Rating", 90.0),
    (Col::Type, "Type", 78.0),
];

/// Self-test switch (SB_TEST_COVER): pretend a menu is open.
static TEST_COVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct SmartEditor {
    pub id: Option<u64>,
    pub name: String,
    pub rules: SmartRules,
}

/// Holding Forward/Back while playing seeks ±15 s every second (playerControls.xml skipLoop).
pub struct SkipHold {
    pub dir: i64,
    pub started: Instant,
    pub last_tick: Instant,
    pub seen_skip: bool,
}

pub struct TuneCraftApp {
    pub lib: Library,
    pub demo: bool,
    pub audio: Audio,
    pub seq: Sequencer,
    pub state: State,
    pub seen_playing: bool,
    pub cur_duration_ms: u64,
    pub fake_pos_ms: Option<u64>,
    pub last_vol: f32,

    pub source: Source,
    pub search: String,
    pub focus_search: bool,
    pub filters: [Option<String>; 3],
    pub sort: Option<(Col, bool)>,
    pub selection: HashSet<u64>,
    pub anchor: Option<usize>,
    pub view: Vec<u64>,
    pub view_dirty: bool,
    pub scroll_to_current: bool,

    pub scan_rx: Option<Receiver<ScanMsg>>,
    pub scan_count: usize,
    pub scan_added: usize,
    pub message: Option<(String, Instant)>,

    pub skip: Option<SkipHold>,
    pub seek_drag: Option<f32>,
    pub marquee: f32,
    pub art: Option<(u64, Option<egui::TextureHandle>)>,

    pub show_art: bool,
    pub show_queue: bool,
    pub renaming: Option<(u64, String)>,
    pub renaming_bookmark: Option<(u64, String)>,
    pub smart: Option<SmartEditor>,
    pub confirm_remove: Option<Vec<u64>>,

    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    pub shot: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
    pub shot_requested: bool,
    pub last_save: Instant,

    pub web: Web,
    /// The browser tab in front; None is the media tab (library and playlists).
    pub active_tab: Option<u64>,
    pub focus_address: bool,
    pub fetch_tx: std::sync::mpsc::Sender<FetchMsg>,
    pub fetch_rx: Receiver<FetchMsg>,
    /// A web track waiting for its file before it can play (faceplate.buffering).
    pub buffering: Option<u64>,
    pub downloading: HashSet<u64>,
    pub downloaded: HashSet<u64>,
    pub web_rect: Option<Rect>,
    pub ctx: egui::Context,
    pub last_rescan: Instant,
    /// What's typed in the location bar while the media tab is in front.
    pub media_address: String,
    pub address_focused: bool,
    /// Web playlist height to go back to after a double-click expanded it.
    pub web_playlist_restore: Option<f32>,
    /// Latest picture of a web page: (tab, when, picture), drawn while the page is hidden.
    pub page_shot: Option<(u64, Instant, egui::TextureHandle)>,
    pub page_shot_requested: Option<Instant>,
    pub shot_tx: std::sync::mpsc::Sender<(u64, egui::ColorImage)>,
    pub shot_rx: Receiver<(u64, egui::ColorImage)>,
    /// The page was hidden last frame (menus or a dialog open).
    pub web_hidden: bool,
}

impl TuneCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>) -> Self {
        setup_style(&cc.egui_ctx);
        let demo = screenshot.is_some();
        let lib = if demo { Library::demo() } else { Library::load() };
        if lib.settings.appearance != 0 && std::env::var_os("TUNECRAFT_APPEARANCE").is_none() {
            set_appearance(&cc.egui_ctx, lib.settings.appearance);
        }
        let seq = Sequencer::new(lib.settings.shuffle, lib.settings.repeat);
        let (fetch_tx, fetch_rx) = channel();
        let (shot_tx, shot_rx) = channel();
        let mut app = TuneCraftApp {
            last_vol: lib.settings.volume.max(0.05),
            lib,
            demo,
            audio: Audio::new(),
            seq,
            state: State::Stopped,
            seen_playing: false,
            cur_duration_ms: 0,
            fake_pos_ms: None,
            source: Source::Library,
            search: String::new(),
            focus_search: false,
            filters: [None, None, None],
            sort: Some((Col::Artist, true)),
            selection: HashSet::new(),
            anchor: None,
            view: vec![],
            view_dirty: true,
            scroll_to_current: false,
            scan_rx: None,
            scan_count: 0,
            scan_added: 0,
            message: None,
            skip: None,
            seek_drag: None,
            marquee: 0.0,
            art: None,
            show_art: true,
            show_queue: true,
            renaming: None,
            renaming_bookmark: None,
            smart: None,
            confirm_remove: None,
            screenshot,
            frames: 0,
            shot: Arc::new(Mutex::new(None)),
            shot_requested: false,
            last_save: Instant::now(),
            web: Web::new(),
            active_tab: None,
            focus_address: false,
            fetch_tx,
            fetch_rx,
            buffering: None,
            downloading: HashSet::new(),
            downloaded: HashSet::new(),
            web_rect: None,
            ctx: cc.egui_ctx.clone(),
            last_rescan: Instant::now(),
            media_address: String::new(),
            address_focused: false,
            web_playlist_restore: None,
            page_shot: None,
            page_shot_requested: None,
            shot_tx,
            shot_rx,
            web_hidden: false,
        };
        if app.demo {
            // A "now playing" look for the check picture, without touching the sound device.
            app.rebuild_view();
            let start = 4.min(app.view.len().saturating_sub(1));
            let view = app.view.clone();
            if let Some(id) = app.seq.play_view(view, Source::Library, start) {
                app.cur_duration_ms = app.lib.track(id).map(|t| t.duration_ms).unwrap_or(0);
                app.state = State::Playing;
                app.seen_playing = true;
                app.fake_pos_ms = Some(72_000);
                app.selection.insert(id);
                app.seq.queue.extend(app.view.iter().skip(9).take(3).copied());
            }
        }
        app
    }

    // ------------------------------------------------------------------ view

    pub fn rebuild_view(&mut self) {
        let base: Vec<u64> = match self.source {
            Source::Library | Source::Web(_) => self.lib.tracks.iter().map(|t| t.id).collect(),
            Source::Playlist(pid) => match self.lib.playlist(pid) {
                Some(pl) => self.lib.playlist_tracks(pl),
                None => {
                    self.source = Source::Library;
                    self.lib.tracks.iter().map(|t| t.id).collect()
                }
            },
        };
        let words = search_words(&self.search);
        let mut ids: Vec<u64> = base
            .into_iter()
            .filter(|id| {
                let Some(t) = self.lib.track(*id) else { return false };
                if !matches_search(t, &words) {
                    return false;
                }
                if self.source == Source::Library {
                    for (i, f) in self.filters.iter().enumerate() {
                        if let Some(v) = f {
                            if filter_value(t, i) != *v {
                                return false;
                            }
                        }
                    }
                }
                true
            })
            .collect();
        if let Some((col, asc)) = self.sort {
            let lib = &self.lib;
            ids.sort_by(|a, b| {
                let (ta, tb) = (lib.track(*a).unwrap(), lib.track(*b).unwrap());
                let o = compare(ta, tb, col);
                if asc { o } else { o.reverse() }
            });
        }
        self.view = ids;
        self.view_dirty = false;
    }

    /// Distinct values for filter list `i`, narrowed by search and the lists before it.
    pub fn filter_values(&self, i: usize) -> Vec<String> {
        let words = search_words(&self.search);
        let mut set: Vec<String> = self
            .lib
            .tracks
            .iter()
            .filter(|t| {
                matches_search(t, &words)
                    && (0..i).all(|j| self.filters[j].as_ref().map(|v| filter_value(t, j) == *v).unwrap_or(true))
            })
            .map(|t| filter_value(t, i))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        set.sort_by_key(|s| s.to_lowercase());
        set
    }

    pub fn select_source(&mut self, s: Source) {
        if self.source != s {
            self.source = s;
            self.selection.clear();
            self.anchor = None;
            self.sort = match s {
                Source::Library | Source::Web(_) => Some((Col::Artist, true)),
                Source::Playlist(_) => None,
            };
            self.view_dirty = true;
        }
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
    }

    // ------------------------------------------------------------------ playback

    pub fn volume(&self) -> f32 {
        if self.lib.settings.muted { 0.0 } else { self.lib.settings.volume }
    }

    pub fn set_volume(&mut self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        self.lib.settings.volume = v;
        // dragging to zero mutes; any other value un-mutes (playerControls.xml onTrackVolume)
        self.lib.settings.muted = v == 0.0;
        if v > 0.0 {
            self.last_vol = v;
        }
        self.audio.set_volume(self.volume());
        self.lib.dirty = true;
    }

    /// Un-muting restores the last non-zero volume (faceplate.volume.last).
    pub fn toggle_mute(&mut self) {
        if self.lib.settings.muted {
            self.lib.settings.muted = false;
            if self.lib.settings.volume == 0.0 {
                self.lib.settings.volume = self.last_vol;
            }
        } else {
            self.lib.settings.muted = true;
        }
        self.audio.set_volume(self.volume());
        self.lib.dirty = true;
    }

    pub fn start_track(&mut self, id: Option<u64>) {
        if std::env::var_os("SB_DEBUG").is_some() {
            eprintln!("start_track {:?} state={:?} buffering={:?}", id, self.state, self.buffering);
        }
        let Some(t) = id.and_then(|id| self.lib.track(id).cloned()) else {
            self.stop();
            return;
        };
        self.cur_duration_ms = t.duration_ms;
        self.fake_pos_ms = None;
        self.scroll_to_current = true;
        self.marquee = 0.0;
        self.buffering = None;
        if let Some(url) = &t.web_url {
            if t.path.as_os_str().is_empty() || !t.path.exists() {
                // fetch the file first, then play it (shown as buffering)
                self.audio.stop();
                self.buffering = Some(t.id);
                self.state = State::Playing;
                self.seen_playing = true;
                web::fetch(t.id, url.clone(), web::cache_dir(), false, self.fetch_tx.clone(), self.egui_ctx());
                return;
            }
        }
        if t.demo {
            self.state = State::Playing;
            self.seen_playing = true;
            self.fake_pos_ms = Some(0);
            return;
        }
        let result = self.audio.play_file(&t.path, self.volume());
        if std::env::var_os("SB_DEBUG").is_some() {
            eprintln!("play_file {} -> {:?}", t.path.display(), result.as_ref().map(|d| d.map(|d| d.as_millis())));
        }
        match result {
            Ok(total) => {
                if let Some(d) = total {
                    if d.as_millis() > 0 {
                        self.cur_duration_ms = d.as_millis() as u64;
                    }
                }
                self.state = State::Playing;
                self.seen_playing = true;
            }
            Err(e) => {
                self.flash(e);
                self.state = State::Stopped;
            }
        }
    }

    pub fn play_view_at(&mut self, index: usize) {
        if self.view_dirty {
            self.rebuild_view();
        }
        let view = self.view.clone();
        let id = self.seq.play_view(view, self.source, index);
        self.start_track(id);
    }

    pub fn play_pause(&mut self) {
        match self.state {
            State::Playing => {
                self.audio.pause();
                self.state = State::Paused;
            }
            State::Paused => {
                self.audio.resume();
                self.state = State::Playing;
            }
            State::Stopped => {
                if let Some(id) = self.seq.current() {
                    self.start_track(Some(id));
                } else {
                    if self.view_dirty {
                        self.rebuild_view();
                    }
                    let idx = self.view.iter().position(|id| self.selection.contains(id)).unwrap_or(0);
                    if !self.view.is_empty() {
                        self.play_view_at(idx);
                    }
                }
            }
        }
    }

    pub fn stop(&mut self) {
        self.buffering = None;
        self.audio.stop();
        self.state = State::Stopped;
        self.fake_pos_ms = None;
    }

    pub fn next(&mut self, user: bool) {
        let id = self.seq.next(user);
        self.start_track(id);
    }

    pub fn previous(&mut self) {
        let id = self.seq.previous();
        self.start_track(id);
    }

    pub fn position_ms(&self) -> u64 {
        if let Some(p) = self.fake_pos_ms {
            return p;
        }
        if self.state == State::Stopped || self.buffering.is_some() {
            return 0;
        }
        self.audio.position().as_millis() as u64
    }

    pub fn seek_ms(&mut self, ms: u64) {
        if self.fake_pos_ms.is_some() {
            self.fake_pos_ms = Some(ms);
        } else {
            self.audio.seek(Duration::from_millis(ms));
        }
    }

    fn tick(&mut self, ctx: &egui::Context) {
        if std::env::var_os("SB_DEBUG").is_some() && self.frames % 60 == 0 && self.buffering.is_none() && self.state == State::Playing {
            eprintln!("tick pos={}ms finished={}", self.position_ms(), self.audio.finished());
        }
        // song ended → history + next
        if self.state == State::Playing && self.fake_pos_ms.is_none() && self.audio.finished() {
            if let Some(id) = self.seq.current() {
                if let Some(t) = self.lib.track_mut(id) {
                    t.play_count += 1;
                    t.last_played = Some(library::now_ms());
                }
                self.lib.dirty = true;
                if matches!(self.source, Source::Playlist(_)) {
                    self.view_dirty = true;
                }
            }
            self.next(false);
        }
        // hold-to-skip: after 1 s held, seek ±15 s each second
        if let Some(h) = &mut self.skip {
            if self.state == State::Playing
                && h.started.elapsed() >= Duration::from_millis(1000)
                && h.last_tick.elapsed() >= Duration::from_millis(1000)
            {
                h.last_tick = Instant::now();
                h.seen_skip = true;
                let dir = h.dir;
                let pos = self.position_ms() as i64;
                let dur = self.cur_duration_ms as i64;
                if dur > 0 {
                    let target = if dir > 0 { (pos + 15000).min(dur - 1000) } else { (pos - 15000).max(1) };
                    self.seek_ms(target.max(0) as u64);
                }
            }
        }
        // scanning
        if let Some(rx) = &self.scan_rx {
            let mut done = None;
            let mut found = vec![];
            while let Ok(m) = rx.try_recv() {
                match m {
                    ScanMsg::Found(t) => found.push(t),
                    ScanMsg::Progress(n) => self.scan_count = n,
                    ScanMsg::Done { added, unsupported } => done = Some((added, unsupported)),
                }
            }
            for mut t in found {
                t.id = self.lib.new_id();
                self.lib.tracks.push(t);
                self.scan_added += 1;
                self.view_dirty = true;
                self.lib.dirty = true;
            }
            if let Some((added, unsupported)) = done {
                self.scan_rx = None;
                let items = if added == 1 { "1 item".to_string() } else { format!("{added} items") };
                let msg = if unsupported > 0 {
                    format!("{items} added to your Library ({unsupported} unsupported)")
                } else {
                    format!("{items} added to your Library")
                };
                self.flash(msg);
            }
        }
        self.web_events();
        self.fetch_events();
        while let Ok((tab, img)) = self.shot_rx.try_recv() {
            let tex = ctx.load_texture("page-shot", img, egui::TextureOptions::LINEAR);
            self.page_shot = Some((tab, Instant::now(), tex));
            self.page_shot_requested = None;
        }
        // pages that add links later are looked at again every few seconds
        if let Some(id) = self.active_tab {
            if self.last_rescan.elapsed() > Duration::from_secs(3) {
                self.last_rescan = Instant::now();
                self.web.rescan(id);
            }
        }
        // album art for the current track
        let cur = self.seq.current();
        if self.art.as_ref().map(|a| a.0) != cur {
            self.art = cur.map(|id| (id, self.lib.track(id).and_then(|t| load_cover(ctx, t))));
        }
        // save now and then
        if self.lib.dirty && !self.demo && self.last_save.elapsed() > Duration::from_secs(3) {
            self.lib.settings.shuffle = self.seq.shuffle;
            self.lib.settings.repeat = self.seq.repeat;
            self.lib.save();
            self.last_save = Instant::now();
        }
        let web_busy = self.web.tabs.iter().any(|t| t.loading) || !self.downloading.is_empty() || self.buffering.is_some();
        if self.state == State::Playing || self.scan_rx.is_some() || self.skip.is_some() || self.lib.dirty || web_busy
            || !self.web.tabs.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub fn import_paths(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        if paths.is_empty() || self.scan_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.scan_rx = Some(rx);
        self.scan_count = 0;
        self.scan_added = 0;
        library::scan(paths, self.lib.known_paths(), tx, ctx.clone());
    }

    pub fn open_and_play(&mut self, path: PathBuf) {
        let id = match self.lib.tracks.iter().find(|t| t.path == path) {
            Some(t) => t.id,
            None => {
                let nid = self.lib.new_id();
                match Track::from_file(&path, nid) {
                    Some(t) => {
                        self.lib.tracks.push(t);
                        self.lib.dirty = true;
                        nid
                    }
                    None => {
                        self.flash("That file type can't be played.");
                        return;
                    }
                }
            }
        };
        self.select_source(Source::Library);
        self.search.clear();
        self.filters = [None, None, None];
        self.rebuild_view();
        if let Some(i) = self.view.iter().position(|v| *v == id) {
            self.play_view_at(i);
        }
    }

    pub fn new_playlist(&mut self, tracks: Vec<u64>) -> u64 {
        let id = self.lib.new_id();
        let n = self.lib.playlists.iter().filter(|p| p.name.starts_with("Playlist")).count() + 1;
        let name = format!("Playlist {n}");
        self.lib.playlists.push(Playlist { id, name: name.clone(), kind: PlaylistKind::Manual(tracks) });
        self.lib.dirty = true;
        self.renaming = Some((id, name));
        id
    }

    pub fn add_to_playlist(&mut self, pid: u64, ids: &[u64]) {
        if let Some(Playlist { kind: PlaylistKind::Manual(list), name, .. }) = self.lib.playlist_mut(pid) {
            list.extend_from_slice(ids);
            let items = if ids.len() == 1 { "1 item".to_string() } else { format!("{} items", ids.len()) };
            let msg = format!("{items} added to {name}");
            self.lib.dirty = true;
            if self.source == Source::Playlist(pid) {
                self.view_dirty = true;
            }
            self.flash(msg);
        }
    }

    pub fn selected_in_order(&self) -> Vec<u64> {
        self.view.iter().copied().filter(|id| self.selection.contains(id)).collect()
    }

    pub fn remove_selected(&mut self) {
        let ids = self.selected_in_order();
        if ids.is_empty() {
            return;
        }
        match self.source {
            Source::Playlist(pid) => {
                if let Some(Playlist { kind: PlaylistKind::Manual(list), .. }) = self.lib.playlist_mut(pid) {
                    let set: HashSet<u64> = ids.into_iter().collect();
                    list.retain(|id| !set.contains(id));
                    self.lib.dirty = true;
                    self.view_dirty = true;
                    self.selection.clear();
                }
            }
            Source::Library => self.confirm_remove = Some(ids),
            Source::Web(_) => {}
        }
    }

    // ------------------------------------------------------------------ keyboard & menus

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let mut acts: Vec<&str> = vec![];
        ctx.input_mut(|i| {
            let cmd = Modifiers::COMMAND;
            if i.consume_key(cmd | Modifiers::ALT, Key::N) {
                acts.push("smart");
            }
            if i.consume_key(cmd, Key::N) {
                acts.push("newpl");
            }
            if i.consume_key(cmd, Key::O) {
                acts.push("open");
            }
            if i.consume_key(cmd, Key::K) {
                acts.push("search");
            }
            if i.consume_key(cmd, Key::T) {
                acts.push("newtab");
            }
            if i.consume_key(cmd, Key::W) {
                acts.push("closetab");
            }
            if i.consume_key(cmd, Key::L) {
                acts.push("address");
            }
            if i.consume_key(cmd, Key::D) {
                acts.push("bookmark");
            }
            if i.consume_key(cmd | Modifiers::ALT, Key::ArrowDown) || i.consume_key(cmd | Modifiers::ALT, Key::ArrowUp) {
                acts.push("mute");
            }
            if i.consume_key(cmd, Key::ArrowUp) {
                acts.push("volup");
            }
            if i.consume_key(cmd, Key::ArrowDown) {
                acts.push("voldown");
            }
            if i.consume_key(Modifiers::CTRL, Key::ArrowRight) {
                acts.push("next");
            }
            if i.consume_key(Modifiers::CTRL, Key::ArrowLeft) {
                acts.push("prev");
            }
            if !typing {
                if i.consume_key(Modifiers::NONE, Key::Space) {
                    acts.push("playpause");
                }
                if i.consume_key(Modifiers::NONE, Key::Enter) {
                    acts.push("playsel");
                }
                if i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace) {
                    acts.push("remove");
                }
                if i.consume_key(cmd, Key::A) {
                    acts.push("all");
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    acts.push("down");
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    acts.push("up");
                }
            }
        });
        for a in acts {
            match a {
                "smart" => self.open_smart_editor(None),
                "newpl" => {
                    let id = self.new_playlist(vec![]);
                    self.select_source(Source::Playlist(id));
                }
                "open" => self.menu_open_file(),
                "search" => self.focus_search = true,
                "newtab" => self.new_tab(web::DEFAULT_HOME),
                "closetab" => {
                    if let Some(id) = self.active_tab {
                        self.close_tab(id);
                    }
                }
                "address" => self.focus_address = true,
                "bookmark" => self.toggle_bookmark(),
                "mute" => self.toggle_mute(),
                "volup" => self.set_volume(self.lib.settings.volume + 0.05),
                "voldown" => self.set_volume(self.lib.settings.volume - 0.05),
                "next" => self.next(true),
                "prev" => self.previous(),
                "playpause" => self.play_pause(),
                "playsel" => {
                    if let Some(i) = self.view.iter().position(|id| self.selection.contains(id)) {
                        self.play_view_at(i);
                    }
                }
                "remove" => self.remove_selected(),
                "all" => self.selection = self.view.iter().copied().collect(),
                "down" | "up" => {
                    if !self.view.is_empty() {
                        let cur = self.anchor.unwrap_or(0) as i64;
                        let step = if a == "down" { 1 } else { -1 };
                        let n = (cur + step).clamp(0, self.view.len() as i64 - 1) as usize;
                        self.selection.clear();
                        self.selection.insert(self.view[n]);
                        self.anchor = Some(n);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn menu_import_folder(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new().set_title("To Import Media, Choose a Folder").pick_folder() {
            self.import_paths(vec![dir], ctx);
        }
    }

    pub fn menu_open_file(&mut self) {
        if let Some(f) = rfd::FileDialog::new()
            .set_title("Select A Media File")
            .add_filter("Audio", library::AUDIO_EXTS)
            .pick_file()
        {
            self.open_and_play(f);
        }
    }

    pub fn menu_import_playlist(&mut self) {
        let Some(f) = rfd::FileDialog::new()
            .set_title("Select A Playlist File")
            .add_filter("Playlists", &["m3u", "m3u8", "pls"])
            .pick_file()
        else {
            return;
        };
        let mut ids = vec![];
        for p in library::read_playlist_file(&f) {
            if let Some(t) = self.lib.tracks.iter().find(|t| t.path == p) {
                ids.push(t.id);
            } else {
                let nid = self.lib.new_id();
                if let Some(t) = Track::from_file(&p, nid) {
                    self.lib.tracks.push(t);
                    ids.push(nid);
                }
            }
        }
        let pid = self.lib.new_id();
        let name = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("Playlist".into());
        self.lib.playlists.push(Playlist { id: pid, name, kind: PlaylistKind::Manual(ids) });
        self.lib.dirty = true;
        self.select_source(Source::Playlist(pid));
        self.view_dirty = true;
    }

    pub fn export_playlist(&mut self, pid: u64) {
        let Some(pl) = self.lib.playlist(pid) else { return };
        let name = pl.name.clone();
        let ids = self.lib.playlist_tracks(pl);
        if let Some(f) = rfd::FileDialog::new()
            .set_file_name(format!("{name}.m3u"))
            .add_filter("M3U playlist", &["m3u"])
            .save_file()
        {
            let tracks: Vec<&Track> = ids.iter().filter_map(|id| self.lib.track(*id)).filter(|t| !t.demo).collect();
            if let Err(e) = library::write_m3u(&f, &tracks) {
                self.flash(format!("Couldn't save the playlist: {e}"));
            }
        }
    }

    pub fn open_smart_editor(&mut self, id: Option<u64>) {
        let existing = id.and_then(|id| self.lib.playlist(id)).and_then(|p| match &p.kind {
            PlaylistKind::Smart(r) => Some((p.name.clone(), r.clone())),
            _ => None,
        });
        self.smart = Some(match existing {
            Some((name, rules)) => SmartEditor { id, name, rules },
            None => SmartEditor {
                id: None,
                name: "Smart Playlist".into(),
                rules: SmartRules {
                    conditions: vec![Condition { field: Field::Artist, op: Op::Contains, value: String::new() }],
                    match_all: true,
                    limit: 0,
                    sort_by: Field::Artist,
                    descending: false,
                },
            },
        });
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // plain menu buttons (not a full-width menu bar) so the browser bar fits beside them
        ui.horizontal(|ui| {
            let w = &mut ui.visuals_mut().widgets;
            w.inactive.weak_bg_fill = Color32::TRANSPARENT;
            w.inactive.bg_stroke = Stroke::NONE;
            w.hovered.bg_stroke = Stroke::NONE;
            w.open.bg_stroke = Stroke::NONE;
            ui.menu_button("File", |ui| {
                if ui.add(egui::Button::new("New Playlist").shortcut_text("⌘N")).clicked() {
                    let id = self.new_playlist(vec![]);
                    self.select_source(Source::Playlist(id));
                }
                if ui.add(egui::Button::new("New Smart Playlist…").shortcut_text("⌥⌘N")).clicked() {
                    self.open_smart_editor(None);
                }
                ui.separator();
                if ui.add(egui::Button::new("New Tab").shortcut_text("⌘T")).clicked() {
                    self.new_tab(web::DEFAULT_HOME);
                }
                if ui.add(egui::Button::new("Open Location…").shortcut_text("⌘L")).clicked() {
                    self.focus_address = true;
                }
                if ui.add_enabled(self.active_tab.is_some(), egui::Button::new("Close Tab").shortcut_text("⌘W")).clicked() {
                    if let Some(id) = self.active_tab {
                        self.close_tab(id);
                    }
                }
                ui.separator();
                if ui.add(egui::Button::new("Open File…").shortcut_text("⌘O")).clicked() {
                    self.menu_open_file();
                }
                if ui.button("Import Media…").clicked() {
                    self.menu_import_folder(&ctx);
                }
                if ui.button("Import Playlist…").clicked() {
                    self.menu_import_playlist();
                }
                let export = match self.source {
                    Source::Playlist(p) => Some(p),
                    _ => None,
                };
                if ui.add_enabled(export.is_some(), egui::Button::new("Export Playlist…")).clicked() {
                    if let Some(p) = export {
                        self.export_playlist(p);
                    }
                }
            });
            ui.menu_button("Controls", |ui| {
                let label = if self.state == State::Playing { "Pause" } else { "Play" };
                if ui.add(egui::Button::new(label).shortcut_text("Space")).clicked() {
                    self.play_pause();
                }
                if ui.add_enabled(self.state != State::Stopped, egui::Button::new("Stop")).clicked() {
                    self.stop();
                }
                if ui.add(egui::Button::new("Next").shortcut_text("⌃→")).clicked() {
                    self.next(true);
                }
                if ui.add(egui::Button::new("Previous").shortcut_text("⌃←")).clicked() {
                    self.previous();
                }
                ui.separator();
                let mut sh = self.seq.shuffle;
                if ui.checkbox(&mut sh, "Shuffle").changed() {
                    self.seq.set_shuffle(sh);
                    self.lib.dirty = true;
                }
                ui.menu_button("Repeat", |ui| {
                    for (v, name) in [(0u8, "Off"), (1, "One"), (2, "All")] {
                        if ui.radio(self.seq.repeat == v, name).clicked() {
                            self.seq.repeat = v;
                            self.lib.dirty = true;
                        }
                    }
                });
                ui.separator();
                if ui.add(egui::Button::new("Volume Up").shortcut_text("⌘↑")).clicked() {
                    self.set_volume(self.lib.settings.volume + 0.05);
                }
                if ui.add(egui::Button::new("Volume Down").shortcut_text("⌘↓")).clicked() {
                    self.set_volume(self.lib.settings.volume - 0.05);
                }
                let mute_label = if self.lib.settings.muted { "Unmute" } else { "Mute" };
                if ui.add(egui::Button::new(mute_label).shortcut_text("⌥⌘↓")).clicked() {
                    self.toggle_mute();
                }
            });
            ui.menu_button("Bookmarks", |ui| {
                let here = self.current_page();
                let marked = here.as_ref().map(|(u, _)| self.bookmark_for(u).is_some()).unwrap_or(false);
                let label = if marked { "Remove Bookmark" } else { "Bookmark This Page" };
                if ui.add_enabled(here.is_some(), egui::Button::new(label).shortcut_text("⌘D")).clicked() {
                    self.toggle_bookmark();
                }
                if !self.lib.bookmarks.is_empty() {
                    ui.separator();
                }
                let list: Vec<(String, String)> = self.lib.bookmarks.iter().map(|b| (b.name.clone(), b.url.clone())).collect();
                for (name, url) in list {
                    if ui.button(ellipsize(&name, 260.0, 13.0, ui)).on_hover_text(&url).clicked() {
                        self.open_bookmark(&url, false);
                    }
                }
            });
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut self.show_art, "Album Art");
                ui.checkbox(&mut self.show_queue, "Play Queue");
                ui.separator();
                ui.menu_button("Appearance", |ui| {
                    for (v, name) in [(0u8, "Match System"), (1, "Light"), (2, "Dark")] {
                        if ui.radio(self.lib.settings.appearance == v, name).clicked() {
                            self.lib.settings.appearance = v;
                            self.lib.dirty = true;
                            set_appearance(ui.ctx(), v);
                        }
                    }
                });
                if ui.checkbox(&mut self.lib.settings.show_remaining, "Show Remaining Time").changed() {
                    self.lib.dirty = true;
                }
            });
        });
    }

    // ------------------------------------------------------------------ browser

    fn egui_ctx(&self) -> egui::Context {
        self.ctx.clone()
    }

    /// The page in the front tab: (address, title).
    pub fn current_page(&self) -> Option<(String, String)> {
        let t = self.active_tab.and_then(|id| self.web.tab(id))?;
        if t.url.is_empty() || t.url.starts_with("about:") {
            return None;
        }
        let title = if t.title.trim().is_empty() || t.title == "Loading…" { t.url.clone() } else { t.title.clone() };
        Some((t.url.clone(), title))
    }

    pub fn bookmark_for(&self, url: &str) -> Option<u64> {
        self.lib.bookmarks.iter().find(|b| b.url == url).map(|b| b.id)
    }

    /// ⌘D / the bookmark button: saves the front page, or removes it when it's already saved.
    pub fn toggle_bookmark(&mut self) {
        let Some((url, title)) = self.current_page() else {
            self.flash("Open a web page first, then bookmark it.");
            return;
        };
        match self.bookmark_for(&url) {
            Some(id) => {
                self.lib.bookmarks.retain(|b| b.id != id);
                self.flash(format!("Removed bookmark \"{title}\""));
            }
            None => {
                let id = self.lib.new_id();
                self.lib.bookmarks.push(library::Bookmark { id, name: title.clone(), url });
                self.flash(format!("Bookmarked \"{title}\""));
            }
        }
        self.lib.dirty = true;
    }

    /// Opens a bookmark in the front web tab (or a new tab from the media tab).
    pub fn open_bookmark(&mut self, url: &str, new_tab: bool) {
        if new_tab {
            self.new_tab(url);
        } else {
            self.go_to(url);
        }
    }

    pub fn new_tab(&mut self, url: &str) {
        let id = self.web.open(url);
        self.active_tab = Some(id);
    }

    pub fn close_tab(&mut self, id: u64) {
        let idx = self.web.tabs.iter().position(|t| t.id == id);
        self.web.close(id);
        if self.active_tab == Some(id) {
            // select the tab to the left, or the media tab (browser.tabs.selectOwnerOnClose)
            self.active_tab = idx.and_then(|i| i.checked_sub(1)).and_then(|i| self.web.tabs.get(i)).map(|t| t.id);
        }
    }

    /// Goes to an address in the front tab, or a new tab when the media tab is in front.
    pub fn go_to(&mut self, input: &str) {
        let url = web::normalize(input);
        match self.active_tab.and_then(|id| self.web.tab_mut(id)) {
            Some(tab) => {
                tab.address = url.clone();
                tab.url = url.clone();
                match &tab.view {
                    Some(v) => {
                        let _ = v.load_url(&url);
                    }
                    None => tab.pending_url = Some(url),
                }
            }
            None => self.new_tab(&url),
        }
    }

    /// Finds or makes the session track for a URL found on a page.
    fn web_track(&mut self, url: &str, name: &str) -> u64 {
        if let Some(t) = self.lib.web_tracks.iter().find(|t| t.web_url.as_deref() == Some(url)) {
            return t.id;
        }
        let id = self.lib.new_id();
        let ext = web::file_name(url).rsplit_once('.').map(|(_, e)| e.to_uppercase()).unwrap_or_default();
        self.lib.web_tracks.push(Track {
            id,
            title: if name.is_empty() { web::file_name(url) } else { name.to_string() },
            kind: ext,
            added: library::now_ms(),
            web_url: Some(url.to_string()),
            ..Default::default()
        });
        id
    }

    fn web_events(&mut self) {
        while let Ok(ev) = self.web.rx.try_recv() {
            match ev {
                WebEvent::Media(tab, items) => {
                    let ids: Vec<u64> = items.iter().map(|(u, n)| self.web_track(u, n)).collect();
                    if let Some(t) = self.web.tab_mut(tab) {
                        if t.media != ids {
                            t.media = ids;
                            t.selection = None;
                        }
                    }
                }
                WebEvent::Play(tab, url, name) => {
                    if std::env::var_os("SB_DEBUG").is_some() {
                        eprintln!("play event {url}");
                    }
                    let id = self.web_track(&url, &name);
                    let list = match self.web.tab_mut(tab) {
                        Some(t) => {
                            if !t.media.contains(&id) {
                                t.media.push(id);
                            }
                            t.media.clone()
                        }
                        None => vec![id],
                    };
                    let idx = list.iter().position(|x| *x == id).unwrap_or(0);
                    let next = self.seq.play_view(list, Source::Web(tab), idx);
                    self.start_track(next);
                }
                WebEvent::Title(tab, title) => {
                    if let Some(t) = self.web.tab_mut(tab) {
                        t.title = title;
                    }
                }
                WebEvent::Loading(tab, started, url) => {
                    // don't overwrite what the user is typing or pasting
                    let editing = self.focus_address || self.address_focused;
                    if let Some(t) = self.web.tab_mut(tab) {
                        t.loading = started;
                        if !url.is_empty() {
                            t.url = url.clone();
                            if !editing {
                                t.address = url;
                            }
                        }
                        if started {
                            t.media.clear();
                        }
                        if started && self.page_shot.as_ref().map(|s| s.0 == tab).unwrap_or(false) {
                            // a new page: the old picture no longer matches
                            self.page_shot = None;
                        }
                    }
                    if !started {
                        // scan the finished page for playable links
                        self.web.rescan(tab);
                        // self-test hook: type text into the page and report what the field holds
                        if std::env::var_os("SB_TEST_BOOKMARK").is_some() && self.current_page().map(|(u, _)| self.bookmark_for(&u).is_none()).unwrap_or(false) {
                            self.toggle_bookmark();
                        }
                        if std::env::var_os("SB_TEST_COVER").is_some() && self.web.snapshot(tab, &self.ctx) {
                            // self-test: behave as if a menu opened once the picture is taken
                            self.page_shot_requested = Some(Instant::now());
                            TEST_COVER.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        if std::env::var_os("SB_TEST_EXPAND").is_some() {
                            self.lib.settings.web_playlist_height = 100_000.0;
                        }
                        if std::env::var_os("SB_TEST_CLICK").is_some() {
                            self.lib.settings.muted = true;
                            if let Some(v) = self.web.tab(tab).and_then(|t| t.view.as_ref()) {
                                let _ = v.evaluate_script("var a = document.querySelector('a[href$=\".mp3\"]'); if (a) a.click();");
                            }
                        }
                        if let Some(t) = std::env::var_os("SB_TEST_INSERT") {
                            if let Some(v) = self.web.tab(tab).and_then(|t| t.view.as_ref()) {
                                let _ = v.evaluate_script("var f = document.querySelector('input'); if (f) f.focus();");
                            }
                            self.web.insert_text(tab, &t.to_string_lossy());
                            if let Some(v) = self.web.tab(tab).and_then(|t| t.view.as_ref()) {
                                let _ = v.evaluate_script_with_callback(
                                    "document.activeElement && document.activeElement.value",
                                    |r| eprintln!("field now holds: {r}"),
                                );
                            }
                        }
                    }
                }
                WebEvent::OpenTab(url) => self.new_tab(&url),
                WebEvent::Paste(tab) => {
                    let text = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok());
                    if let Some(t) = text.filter(|t| !t.is_empty()) {
                        self.web.insert_text(tab, &t);
                    }
                }
                WebEvent::Copy(text) => {
                    if let Ok(mut c) = arboard::Clipboard::new() {
                        let _ = c.set_text(text);
                    }
                }
                WebEvent::Snapshot(tab, bytes) => {
                    // decode off the main thread; the result comes back through shot_rx
                    let tx = self.shot_tx.clone();
                    let ctx = self.ctx.clone();
                    std::thread::spawn(move || {
                        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Tiff);
                        if std::env::var_os("SB_DEBUG").is_some() {
                            eprintln!("snapshot {} bytes -> {:?}", bytes.len(), decoded.as_ref().map(|i| (i.width(), i.height())).map_err(|e| e.to_string()));
                        }
                        if let Ok(img) = decoded {
                            let rgba = img.to_rgba8();
                            let size = [rgba.width() as usize, rgba.height() as usize];
                            let ci = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                            let _ = tx.send((tab, ci));
                            ctx.request_repaint();
                        }
                    });
                }
                WebEvent::Key(tab, key) => match key.as_str() {
                    "l" => {
                        if let Some(v) = self.web.tab(tab).and_then(|t| t.view.as_ref()) {
                            let _ = v.focus_parent();
                        }
                        self.focus_address = true;
                    }
                    "t" => self.new_tab(web::DEFAULT_HOME),
                    "w" => self.close_tab(tab),
                    "d" => self.toggle_bookmark(),
                    _ => {}
                },
            }
        }
    }

    fn fetch_events(&mut self) {
        while let Ok(m) = self.fetch_rx.try_recv() {
            match m {
                FetchMsg::Done(id, path, true) => {
                    self.downloading.remove(&id);
                    self.downloaded.insert(id);
                    let nid = self.lib.new_id();
                    match Track::from_file(&path, nid) {
                        Some(t) => {
                            let title = t.title.clone();
                            self.lib.tracks.push(t);
                            self.lib.dirty = true;
                            self.view_dirty = true;
                            self.flash(format!("\"{title}\" downloaded to your Library"));
                        }
                        None => self.flash("Downloaded, but the file can't be played."),
                    }
                }
                FetchMsg::Done(id, path, false) => {
                    if std::env::var_os("SB_DEBUG").is_some() {
                        eprintln!("fetched {id} -> {} (current {:?}, buffering {:?})", path.display(), self.seq.current(), self.buffering);
                    }
                    if let Some(t) = self.lib.track_mut(id) {
                        if let Some(read) = Track::from_file(&path, id) {
                            t.duration_ms = read.duration_ms;
                            if !read.artist.is_empty() {
                                t.title = read.title;
                                t.artist = read.artist;
                                t.album = read.album;
                                t.genre = read.genre;
                            }
                        }
                        t.path = path;
                    }
                    if self.buffering == Some(id) && self.seq.current() == Some(id) {
                        self.start_track(Some(id));
                    }
                }
                FetchMsg::Failed(id, e) => {
                    self.downloading.remove(&id);
                    if self.buffering == Some(id) {
                        self.buffering = None;
                        self.state = State::Stopped;
                    }
                    self.flash(format!("Couldn't get the file: {e}"));
                }
            }
        }
    }

    pub fn download(&mut self, id: u64) {
        let Some(url) = self.lib.track(id).and_then(|t| t.web_url.clone()) else { return };
        if self.downloading.insert(id) {
            web::fetch(id, url, web::download_dir(), true, self.fetch_tx.clone(), self.egui_ctx());
        }
    }

    // ------------------------------------------------------------------ screenshot

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frames += 1;
        ctx.request_repaint();
        let shot_at: u32 = std::env::var("SB_TEST_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
        if self.frames >= shot_at && !self.shot_requested {
            self.shot_requested = true;
            let slot = self.shot.clone();
            let c2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                *slot.lock().unwrap() = Some(img);
                c2.request_repaint();
            });
        }
        let img = self.shot.lock().unwrap().take();
        if let Some(img) = img {
            let [w, h] = img.size;
            let mut bytes = Vec::with_capacity(w * h * 4);
            for p in &img.pixels {
                bytes.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
            }
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                Some(buf) => {
                    if let Err(e) = buf.save(&path) {
                        eprintln!("could not save screenshot: {e}");
                    }
                }
                None => eprintln!("screenshot had the wrong size"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.frames > shot_at + 900 {
            eprintln!("screenshot never arrived");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for TuneCraftApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // follow the Mac switching between light and dark while running
        if (ctx.theme() == egui::Theme::Dark) != theme::is_dark() {
            apply_style(&ctx);
        }
        self.tick(&ctx);
        self.handle_keys(&ctx);

        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !dropped.is_empty() {
            self.import_paths(dropped, &ctx);
        }

        // one 48 pt top bar, as in the other Craft apps: mark, name, menus, browser bar
        egui::Panel::top("topbar")
            .exact_size(theme::TOP_BAR_H)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| {
                let rect = ui.max_rect();
                theme::nav_bar(ui.painter(), rect);
                let mark = Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(24.0, 24.0));
                theme::logo(ui.painter(), mark);
                let name_end = ui
                    .painter()
                    .text(
                        pos2(mark.right() + 9.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "TuneCraft",
                        theme::semibold(15.0),
                        theme::TEXT(),
                    )
                    .right();
                let menu_rect = Rect::from_min_max(pos2(name_end + 14.0, rect.top()), pos2(rect.right(), rect.bottom()));
                let mut menu_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(menu_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                self.menu_bar(&mut menu_ui);
                let used = menu_ui.min_rect().right();
                let nav_rect = Rect::from_min_max(pos2(used + 12.0, rect.top()), pos2(rect.right(), rect.bottom() - 1.0));
                let mut nav_ui = ui.new_child(egui::UiBuilder::new().max_rect(nav_rect));
                self.nav_bar(&mut nav_ui);
            });
        if !self.web.tabs.is_empty() {
            egui::Panel::top("tabstrip")
                .exact_size(24.0)
                .frame(egui::Frame::NONE)
                .show_separator_line(false)
                .show(ui, |ui| self.tab_strip(ui));
        }
        egui::Panel::bottom("controls")
            .exact_size(64.0)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| self.control_pane(ui));
        egui::Panel::left("servicepane")
            .default_size(190.0)
            .size_range(150.0..=320.0)
            .resizable(true)
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.service_pane(ui));
        if self.show_queue {
            egui::Panel::right("queue")
                .default_size(200.0)
                .size_range(150.0..=340.0)
                .resizable(true)
                .frame(egui::Frame::NONE)
                .show(ui, |ui| self.queue_pane(ui));
        }
        egui::Panel::bottom("status")
            .exact_size(26.0)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| self.status_bar(ui));
        self.web_rect = None;
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let rect = ui.max_rect();
            if let Some(tab) = self.active_tab {
                self.web_page(ui, rect, tab);
            } else if self.source == Source::Library && !self.lib.tracks.is_empty() {
                let fh = (rect.height() * 0.28).clamp(90.0, 180.0);
                let frect = Rect::from_min_size(rect.min, vec2(rect.width(), fh));
                self.filter_lists(ui, frect);
                ui.painter().hline(rect.x_range(), frect.bottom(), Stroke::new(1.0, gray(0x90)));
                let trect = Rect::from_min_max(pos2(rect.min.x, frect.bottom() + 1.0), rect.max);
                self.track_list(ui, trect);
            } else {
                self.track_list(ui, rect);
            }
        });

        self.dialogs(&ctx);
        self.screenshot_step(&ctx);

        // native web views sit above the drawing, so hide them while menus or dialogs are open
        if !self.web.tabs.is_empty() {
            let zoom = ctx.zoom_factor();
            let r = self.web_rect.unwrap_or(Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)));
            let bounds = wry::Rect {
                position: wry::dpi::LogicalPosition::new((r.min.x * zoom) as f64, (r.min.y * zoom) as f64).into(),
                size: wry::dpi::LogicalSize::new((r.width() * zoom).max(1.0) as f64, (r.height() * zoom).max(1.0) as f64).into(),
            };
            self.web.create_pending(&*frame, bounds);
            let want_cover = TEST_COVER.load(std::sync::atomic::Ordering::Relaxed)
                || egui::Popup::is_any_open(&ctx)
                || self.renaming.is_some()
                || self.renaming_bookmark.is_some()
                || self.smart.is_some()
                || self.confirm_remove.is_some();
            // Menus and dialogs are drawn under the native page, so the page is hidden while
            // they're open and a picture of it is shown in its place. The picture is taken when
            // the pointer heads for the menus (or a menu opens), while the page is still showing.
            if let Some(tab) = self.active_tab {
                let near_menus = ctx
                    .pointer_hover_pos()
                    .map(|p| p.y < theme::TOP_BAR_H + 26.0)
                    .unwrap_or(false);
                let shot_age = self.page_shot.as_ref().filter(|s| s.0 == tab).map(|s| s.1.elapsed());
                let pending = self.page_shot_requested.map(|r| r.elapsed() < Duration::from_secs(1)).unwrap_or(false);
                let stale = shot_age.map(|a| a > Duration::from_millis(600)).unwrap_or(true);
                if (near_menus || want_cover) && stale && !pending && !self.web_hidden {
                    if self.web.snapshot(tab, &ctx) {
                        self.page_shot_requested = Some(Instant::now());
                    }
                }
            }
            let have_shot = self.page_shot.as_ref().map(|s| Some(s.0) == self.active_tab).unwrap_or(false);
            // without a picture yet, keep the page up a moment (at most 0.4 s) rather than blank it
            let waited = self.page_shot_requested.map(|r| r.elapsed() > Duration::from_millis(400)).unwrap_or(true);
            let covered = self.web_rect.is_none() || (want_cover && (have_shot || waited || self.web_hidden));
            if want_cover && !covered {
                ctx.request_repaint_after(Duration::from_millis(30));
            }
            self.web_hidden = covered && self.web_rect.is_some();
            self.web.layout(self.active_tab, bounds, covered);
            // a click on the app's own controls takes the keyboard back from the web page,
            // so typing and pasting reach the location bar and search box
            let pressed_outside = ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().map(|p| !self.web_rect.map(|r| r.contains(p)).unwrap_or(false)).unwrap_or(false)
            });
            if pressed_outside || self.focus_address {
                if let Some(v) = self.active_tab.and_then(|id| self.web.tab(id)).and_then(|t| t.view.as_ref()) {
                    let _ = v.focus_parent();
                }
            }
            if let Some(e) = self.web.error.take() {
                self.flash(e);
            }
        }
        let page = self.active_tab.and_then(|id| self.web.tab(id)).map(|t| t.title.clone());
        let title = match self.seq.current().and_then(|id| self.lib.track(id)).filter(|_| self.state != State::Stopped) {
            Some(t) if !t.artist.is_empty() => format!("{} - {} — TuneCraft", t.title, t.artist),
            Some(t) => format!("{} — TuneCraft", t.title),
            None => match page {
                Some(p) if !p.is_empty() => format!("{p} — TuneCraft"),
                _ => "TuneCraft".to_string(),
            },
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }
}

impl Drop for TuneCraftApp {
    fn drop(&mut self) {
        if !self.demo {
            self.lib.settings.shuffle = self.seq.shuffle;
            self.lib.settings.repeat = self.seq.repeat;
            self.lib.save();
        }
    }
}

fn setup_style(ctx: &egui::Context) {
    theme::install_fonts(ctx);
    // like the other Craft apps: follow the Mac's light/dark setting, dark when unknown
    ctx.options_mut(|o| {
        o.theme_preference = egui::ThemePreference::System;
        o.fallback_theme = egui::Theme::Dark;
    });
    if let Some(v) = std::env::var("TUNECRAFT_APPEARANCE").ok().and_then(|v| v.parse().ok()) {
        set_appearance(ctx, v);
    }
    apply_style(ctx);
}

/// 0 = follow the Mac, 1 = light, 2 = dark (View ▸ Appearance).
pub fn set_appearance(ctx: &egui::Context, v: u8) {
    ctx.set_theme(match v {
        1 => egui::ThemePreference::Light,
        2 => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    });
    apply_style(ctx);
}

/// Sets egui's own widgets (menus, buttons, text boxes, dialogs) to the current palette.
pub fn apply_style(ctx: &egui::Context) {
    let dark = ctx.theme() == egui::Theme::Dark;
    theme::set_dark(dark);
    let pal = theme::p();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = pal.panel;
    v.window_fill = pal.panel;
    v.extreme_bg_color = pal.surface;
    v.faint_bg_color = pal.surface_alt;
    v.override_text_color = Some(pal.text);
    v.hyperlink_color = pal.accent;
    v.selection.bg_fill = pal.accent;
    v.selection.stroke = Stroke::new(1.0, pal.accent_text);
    v.window_corner_radius = egui::CornerRadius::same(10);
    v.menu_corner_radius = egui::CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0, pal.line);
    let r = egui::CornerRadius::same(6);
    v.widgets.noninteractive.bg_fill = pal.panel;
    v.widgets.noninteractive.weak_bg_fill = pal.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal.line);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, pal.text);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.bg_fill = pal.surface;
    v.widgets.inactive.weak_bg_fill = pal.surface;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, pal.line);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, pal.text);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.bg_fill = pal.hover;
    v.widgets.hovered.weak_bg_fill = pal.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, pal.line);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, pal.text);
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.bg_fill = pal.accent;
    v.widgets.active.weak_bg_fill = pal.accent;
    v.widgets.active.bg_stroke = Stroke::new(1.0, pal.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0, pal.accent_text);
    v.widgets.active.corner_radius = r;
    v.widgets.open.bg_fill = pal.hover;
    v.widgets.open.weak_bg_fill = pal.hover;
    v.widgets.open.corner_radius = r;
    ctx.set_visuals_of(if dark { egui::Theme::Dark } else { egui::Theme::Light }, v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(10.0, 4.0);
        s.spacing.interact_size.y = 24.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.0));
        s.text_styles.insert(egui::TextStyle::Heading, theme::semibold(18.0));
        s.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(12.0));
    });
}

pub fn search_words(s: &str) -> Vec<String> {
    s.to_lowercase().split_whitespace().map(String::from).collect()
}

/// Every word must appear in the title, artist, album or genre.
pub fn matches_search(t: &Track, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let hay = format!("{} {} {} {}", t.title, t.artist, t.album, t.genre).to_lowercase();
    words.iter().all(|w| hay.contains(w))
}

pub fn filter_value(t: &Track, i: usize) -> String {
    match i {
        0 => t.genre.clone(),
        1 => t.artist.clone(),
        _ => t.album.clone(),
    }
}

pub fn cell_text(t: &Track, col: Col) -> String {
    match col {
        Col::Title => t.title.clone(),
        Col::Time => {
            if t.duration_ms > 0 { player::fmt_time(t.duration_ms) } else { String::new() }
        }
        Col::Artist => t.artist.clone(),
        Col::Album => t.album.clone(),
        Col::Genre => t.genre.clone(),
        Col::Rating => String::new(),
        Col::Type => t.kind.clone(),
    }
}

/// Column sort with Songbird-like secondary keys (artist → album → disc → track).
pub fn compare(a: &Track, b: &Track, col: Col) -> std::cmp::Ordering {
    let lc = |s: &str| s.to_lowercase();
    let album_order = |a: &Track, b: &Track| {
        lc(&a.album)
            .cmp(&lc(&b.album))
            .then(a.disc_no.cmp(&b.disc_no))
            .then(a.track_no.cmp(&b.track_no))
            .then(lc(&a.title).cmp(&lc(&b.title)))
    };
    match col {
        Col::Title => lc(&a.title).cmp(&lc(&b.title)),
        Col::Time => a.duration_ms.cmp(&b.duration_ms),
        Col::Artist => lc(&a.artist).cmp(&lc(&b.artist)).then_with(|| album_order(a, b)),
        Col::Album => album_order(a, b),
        Col::Genre => lc(&a.genre)
            .cmp(&lc(&b.genre))
            .then(lc(&a.artist).cmp(&lc(&b.artist)))
            .then_with(|| album_order(a, b)),
        Col::Rating => a.rating.cmp(&b.rating).then(lc(&a.artist).cmp(&lc(&b.artist))),
        Col::Type => a.kind.cmp(&b.kind).then(lc(&a.artist).cmp(&lc(&b.artist))),
    }
}

pub fn group_thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Cuts text with "…" to fit `width` pixels.
pub fn ellipsize(s: &str, width: f32, size: f32, ui: &Ui) -> String {
    if width <= 8.0 {
        return String::new();
    }
    let font = FontId::proportional(size);
    let measure = |t: &str| ui.fonts_mut(|f| f.layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE).size().x);
    if measure(s) <= width {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if measure(&cand) <= width { lo = mid } else { hi = mid - 1 }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

/// Front cover from the file's tags, else a cover image beside it (cover/folder/front/album).
fn load_cover(ctx: &egui::Context, t: &Track) -> Option<egui::TextureHandle> {
    if t.demo {
        return None;
    }
    use lofty::prelude::*;
    let mut bytes: Option<Vec<u8>> = None;
    if let Ok(file) = lofty::read_from_path(&t.path) {
        for tag in file.tags() {
            let pics = tag.pictures();
            let front = pics
                .iter()
                .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
                .or_else(|| pics.first());
            if let Some(p) = front {
                bytes = Some(p.data().to_vec());
                break;
            }
        }
    }
    if bytes.is_none() {
        if let Some(dir) = t.path.parent() {
            'outer: for name in ["cover", "folder", "front", "album", "Cover", "Folder", "Front", "Album"] {
                for ext in ["jpg", "jpeg", "png"] {
                    if let Ok(b) = std::fs::read(dir.join(format!("{name}.{ext}"))) {
                        bytes = Some(b);
                        break 'outer;
                    }
                }
            }
        }
    }
    let img = image::load_from_memory(&bytes?).ok()?.thumbnail(512, 512).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(format!("cover-{}", t.id), ci, egui::TextureOptions::LINEAR))
}

