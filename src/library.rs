//! The music library: tracks, playlists, smart playlists, saving and folder scanning.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// File types the player can decode (a subset of the types Songbird's Info.plist claims).
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "m4a", "mp4", "aac", "flac", "wav", "aif", "aiff", "ogg", "oga", "mka", "caf",
];

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Track {
    pub id: u64,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: u32,
    pub track_no: u32,
    pub disc_no: u32,
    pub duration_ms: u64,
    /// 0..=5 stars, as Songbird's rating property.
    pub rating: u8,
    pub play_count: u32,
    pub last_played: Option<i64>,
    pub added: i64,
    /// Upper-case file type, shown in the "Type" column.
    pub kind: String,
    /// Placeholder tracks used for the screenshot have no file.
    #[serde(skip)]
    pub demo: bool,
    /// Where a track found on a web page lives (web playlist items).
    #[serde(skip)]
    pub web_url: Option<String>,
}

impl Track {
    pub fn from_file(path: &Path, id: u64) -> Option<Track> {
        use lofty::prelude::*;
        let ext = path.extension()?.to_string_lossy().to_lowercase();
        if !AUDIO_EXTS.contains(&ext.as_str()) {
            return None;
        }
        let mut t = Track {
            id,
            path: path.to_path_buf(),
            added: now_ms(),
            kind: ext.to_uppercase(),
            ..Default::default()
        };
        if let Ok(file) = lofty::read_from_path(path) {
            t.duration_ms = file.properties().duration().as_millis() as u64;
            if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
                t.title = tag.title().map(|s| s.trim().to_string()).unwrap_or_default();
                t.artist = tag.artist().map(|s| s.trim().to_string()).unwrap_or_default();
                t.album = tag.album().map(|s| s.trim().to_string()).unwrap_or_default();
                t.genre = tag.genre().map(|s| s.trim().to_string()).unwrap_or_default();
                t.track_no = tag.track().unwrap_or(0);
                t.disc_no = tag.disk().unwrap_or(0);
                t.year = tag.date().map(|d| d.year as u32).unwrap_or(0);
            }
        }
        if t.title.is_empty() {
            t.title = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
        }
        Some(t)
    }
}

// ---------------------------------------------------------------- smart playlists

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Title,
    Artist,
    Album,
    Genre,
    Year,
    Rating,
    PlayCount,
    DateAdded,
    LastPlayed,
}

impl Field {
    pub const ALL: [Field; 9] = [
        Field::Title,
        Field::Artist,
        Field::Album,
        Field::Genre,
        Field::Year,
        Field::Rating,
        Field::PlayCount,
        Field::DateAdded,
        Field::LastPlayed,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Field::Title => "Title",
            Field::Artist => "Artist",
            Field::Album => "Album",
            Field::Genre => "Genre",
            Field::Year => "Year",
            Field::Rating => "Rating",
            Field::PlayCount => "Play Count",
            Field::DateAdded => "Date Added",
            Field::LastPlayed => "Last Played",
        }
    }
    pub fn kind(self) -> FieldKind {
        match self {
            Field::Title | Field::Artist | Field::Album | Field::Genre => FieldKind::Text,
            Field::Year | Field::Rating | Field::PlayCount => FieldKind::Number,
            Field::DateAdded | Field::LastPlayed => FieldKind::Date,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum FieldKind {
    Text,
    Number,
    Date,
}

/// Operators, named after Songbird's `smart.*` locale strings.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Contains,
    NotContains,
    Is,
    IsNot,
    StartsWith,
    EndsWith,
    Less,
    Greater,
    LessEq,
    GreaterEq,
    InTheLast,
    NotInTheLast,
}

impl Op {
    pub fn for_kind(kind: FieldKind) -> &'static [Op] {
        match kind {
            FieldKind::Text => &[
                Op::Contains,
                Op::NotContains,
                Op::Is,
                Op::IsNot,
                Op::StartsWith,
                Op::EndsWith,
            ],
            FieldKind::Number => &[
                Op::Is,
                Op::IsNot,
                Op::Less,
                Op::Greater,
                Op::LessEq,
                Op::GreaterEq,
            ],
            FieldKind::Date => &[Op::InTheLast, Op::NotInTheLast],
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Op::Contains => "contains",
            Op::NotContains => "does not contain",
            Op::Is => "is",
            Op::IsNot => "is not",
            Op::StartsWith => "starts with",
            Op::EndsWith => "ends with",
            Op::Less => "is less than",
            Op::Greater => "is greater than",
            Op::LessEq => "is or is less than",
            Op::GreaterEq => "is or is greater than",
            Op::InTheLast => "is in the last",
            Op::NotInTheLast => "is not in the last",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Condition {
    pub field: Field,
    pub op: Op,
    /// Text, a number, or a number of days for date fields.
    pub value: String,
}

impl Condition {
    pub fn matches(&self, t: &Track, now: i64) -> bool {
        match self.field.kind() {
            FieldKind::Text => {
                let hay = match self.field {
                    Field::Title => &t.title,
                    Field::Artist => &t.artist,
                    Field::Album => &t.album,
                    _ => &t.genre,
                }
                .to_lowercase();
                let v = self.value.to_lowercase();
                match self.op {
                    Op::Contains => hay.contains(&v),
                    Op::NotContains => !hay.contains(&v),
                    Op::Is => hay == v,
                    Op::IsNot => hay != v,
                    Op::StartsWith => hay.starts_with(&v),
                    Op::EndsWith => hay.ends_with(&v),
                    _ => false,
                }
            }
            FieldKind::Number => {
                let x = match self.field {
                    Field::Year => t.year as f64,
                    Field::Rating => t.rating as f64,
                    _ => t.play_count as f64,
                };
                let v: f64 = self.value.trim().parse().unwrap_or(0.0);
                match self.op {
                    Op::Is => x == v,
                    Op::IsNot => x != v,
                    Op::Less => x < v,
                    Op::Greater => x > v,
                    Op::LessEq => x <= v,
                    Op::GreaterEq => x >= v,
                    _ => false,
                }
            }
            FieldKind::Date => {
                let when = match self.field {
                    Field::DateAdded => Some(t.added),
                    _ => t.last_played,
                };
                let days: f64 = self.value.trim().parse().unwrap_or(0.0);
                let since = now - (days * DAY_MS as f64) as i64;
                let inside = when.map(|w| w >= since).unwrap_or(false);
                match self.op {
                    Op::InTheLast => inside,
                    Op::NotInTheLast => !inside,
                    _ => false,
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SmartRules {
    pub conditions: Vec<Condition>,
    pub match_all: bool,
    /// Most tracks to keep (0 = no limit).
    pub limit: usize,
    pub sort_by: Field,
    pub descending: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlaylistKind {
    Manual(Vec<u64>),
    Smart(SmartRules),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Playlist {
    pub id: u64,
    pub name: String,
    pub kind: PlaylistKind,
}

// ---------------------------------------------------------------- library

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    /// 0 = none, 1 = one, 2 = all (Songbird's playlist.repeat values).
    pub repeat: u8,
    pub show_remaining: bool,
    pub filterset: [usize; 3],
    /// Height of the web playlist under a web page, in points (dragged by its splitter).
    pub web_playlist_height: f32,
    /// 0 = match the Mac, 1 = light, 2 = dark.
    pub appearance: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            volume: 0.5,
            muted: false,
            shuffle: false,
            repeat: 0,
            show_remaining: false,
            filterset: [0, 1, 2],
            web_playlist_height: 180.0,
            appearance: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Library {
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub next_id: u64,
    pub settings: Settings,
    #[serde(skip)]
    pub dirty: bool,
    /// Tracks found on web pages; kept for this session only, outside the library.
    #[serde(skip)]
    pub web_tracks: Vec<Track>,
    /// Saved web pages (the service pane's Bookmarks).
    pub bookmarks: Vec<Bookmark>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bookmark {
    pub id: u64,
    pub name: String,
    pub url: String,
}

impl Library {
    pub fn file_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("TuneCraft").join("library.json")
    }

    pub fn load() -> Library {
        // a library saved before the rename to TuneCraft is moved over once
        let old = dirs::data_dir().map(|d| d.join("Songbird Player").join("library.json"));
        if let Some(old) = old.filter(|o| o.exists() && !Self::file_path().exists()) {
            if let Some(dir) = Self::file_path().parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::copy(&old, Self::file_path());
        }
        let mut lib = std::fs::read_to_string(Self::file_path())
            .ok()
            .and_then(|s| serde_json::from_str::<Library>(&s).ok())
            .unwrap_or_default();
        if lib.playlists.is_empty() && lib.next_id == 0 {
            lib.add_default_smart_playlists();
        }
        lib
    }

    pub fn save(&mut self) {
        let path = Self::file_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(self) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
        self.dirty = false;
    }

    pub fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Songbird's first-run smart playlists (scripts/mainWinInit.js).
    pub fn add_default_smart_playlists(&mut self) {
        let defs = [
            ("Highest Rated", Field::Rating, Op::Greater, "3", Field::Rating),
            ("Recently Added", Field::DateAdded, Op::InTheLast, "30", Field::DateAdded),
            ("Recently Played", Field::LastPlayed, Op::InTheLast, "7", Field::LastPlayed),
        ];
        for (name, field, op, value, sort_by) in defs {
            let id = self.new_id();
            self.playlists.push(Playlist {
                id,
                name: name.to_string(),
                kind: PlaylistKind::Smart(SmartRules {
                    conditions: vec![Condition { field, op, value: value.to_string() }],
                    match_all: true,
                    limit: 0,
                    sort_by,
                    descending: true,
                }),
            });
        }
    }

    pub fn track(&self, id: u64) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id).or_else(|| self.web_tracks.iter().find(|t| t.id == id))
    }

    pub fn track_mut(&mut self, id: u64) -> Option<&mut Track> {
        match self.tracks.iter().position(|t| t.id == id) {
            Some(i) => self.tracks.get_mut(i),
            None => self.web_tracks.iter_mut().find(|t| t.id == id),
        }
    }

    pub fn playlist(&self, id: u64) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    pub fn playlist_mut(&mut self, id: u64) -> Option<&mut Playlist> {
        self.playlists.iter_mut().find(|p| p.id == id)
    }

    pub fn known_paths(&self) -> HashSet<PathBuf> {
        self.tracks.iter().map(|t| t.path.clone()).collect()
    }

    /// Track ids of a playlist in its own order (smart playlists are evaluated now).
    pub fn playlist_tracks(&self, pl: &Playlist) -> Vec<u64> {
        match &pl.kind {
            PlaylistKind::Manual(ids) => ids
                .iter()
                .copied()
                .filter(|id| self.track(*id).is_some())
                .collect(),
            PlaylistKind::Smart(rules) => {
                let now = now_ms();
                let mut hits: Vec<&Track> = self
                    .tracks
                    .iter()
                    .filter(|t| {
                        if rules.conditions.is_empty() {
                            return true;
                        }
                        if rules.match_all {
                            rules.conditions.iter().all(|c| c.matches(t, now))
                        } else {
                            rules.conditions.iter().any(|c| c.matches(t, now))
                        }
                    })
                    .collect();
                hits.sort_by(|a, b| {
                    let o = match rules.sort_by {
                        Field::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                        Field::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                        Field::Album => a.album.to_lowercase().cmp(&b.album.to_lowercase()),
                        Field::Genre => a.genre.to_lowercase().cmp(&b.genre.to_lowercase()),
                        Field::Year => a.year.cmp(&b.year),
                        Field::Rating => a.rating.cmp(&b.rating),
                        Field::PlayCount => a.play_count.cmp(&b.play_count),
                        Field::DateAdded => a.added.cmp(&b.added),
                        Field::LastPlayed => a.last_played.cmp(&b.last_played),
                    };
                    if rules.descending { o.reverse() } else { o }
                });
                if rules.limit > 0 {
                    hits.truncate(rules.limit);
                }
                hits.into_iter().map(|t| t.id).collect()
            }
        }
    }

    pub fn remove_tracks(&mut self, ids: &HashSet<u64>) {
        self.tracks.retain(|t| !ids.contains(&t.id));
        for pl in &mut self.playlists {
            if let PlaylistKind::Manual(list) = &mut pl.kind {
                list.retain(|id| !ids.contains(id));
            }
        }
        self.dirty = true;
    }

    /// Placeholder library shown by `--screenshot`, so a check never touches the user's music.
    pub fn demo() -> Library {
        let mut lib = Library::default();
        lib.add_default_smart_playlists();
        let rows: &[(&str, &str, &str, &str, u64, u8)] = &[
            ("Morning Light", "Lantern Coast", "Harbour Songs", "Folk", 214, 4),
            ("Northern Road", "Lantern Coast", "Harbour Songs", "Folk", 251, 3),
            ("Tidewater", "Lantern Coast", "Harbour Songs", "Folk", 198, 5),
            ("Glass Houses", "Amber Fields", "Low Sun", "Indie", 233, 0),
            ("Low Sun", "Amber Fields", "Low Sun", "Indie", 276, 4),
            ("Static Bloom", "Amber Fields", "Low Sun", "Indie", 189, 0),
            ("Copper Wire", "Night Index", "Signals", "Electronic", 305, 2),
            ("Signal Lost", "Night Index", "Signals", "Electronic", 342, 0),
            ("Parallel", "Night Index", "Signals", "Electronic", 287, 5),
            ("Blue Hour", "Sara Lindqvist", "Quiet Rooms", "Jazz", 262, 4),
            ("Small Talk", "Sara Lindqvist", "Quiet Rooms", "Jazz", 221, 0),
            ("Window Seat", "Sara Lindqvist", "Quiet Rooms", "Jazz", 248, 3),
            ("Overpass", "Hollow Pines", "County Line", "Rock", 201, 0),
            ("Gravel", "Hollow Pines", "County Line", "Rock", 186, 4),
            ("Last Exit", "Hollow Pines", "County Line", "Rock", 239, 0),
            ("Paper Moon", "Ines Moreau", "Salt & Sugar", "Pop", 195, 5),
            ("Sugar Rush", "Ines Moreau", "Salt & Sugar", "Pop", 177, 0),
            ("Cartwheels", "Ines Moreau", "Salt & Sugar", "Pop", 208, 2),
        ];
        let now = now_ms();
        for (i, (title, artist, album, genre, secs, rating)) in rows.iter().enumerate() {
            let id = lib.new_id();
            lib.tracks.push(Track {
                id,
                path: PathBuf::new(),
                title: title.to_string(),
                artist: artist.to_string(),
                album: album.to_string(),
                genre: genre.to_string(),
                year: 2008 + (i as u32 % 4),
                track_no: (i as u32 % 3) + 1,
                disc_no: 1,
                duration_ms: secs * 1000,
                rating: *rating,
                play_count: (i as u32 * 7) % 11,
                last_played: Some(now - (i as i64) * DAY_MS),
                added: now - (i as i64) * 3 * DAY_MS,
                kind: "MP3".into(),
                demo: true,
                web_url: None,
            });
        }
        let id = lib.new_id();
        let ids: Vec<u64> = lib.tracks.iter().step_by(3).map(|t| t.id).collect();
        lib.playlists.push(Playlist { id, name: "Road Trip".into(), kind: PlaylistKind::Manual(ids) });
        let bid = lib.new_id();
        lib.bookmarks.push(Bookmark { id: bid, name: "Internet Archive: Audio".into(), url: "https://archive.org/details/audio".into() });
        lib
    }
}

// ---------------------------------------------------------------- scanning

pub enum ScanMsg {
    Found(Track),
    Progress(usize),
    Done { added: usize, unsupported: usize },
}

/// Walks the given files and folders on a background thread and sends new tracks back.
/// Ids are assigned by the receiver.
pub fn scan(paths: Vec<PathBuf>, known: HashSet<PathBuf>, tx: Sender<ScanMsg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let mut added = 0;
        let mut unsupported = 0;
        let mut seen = 0;
        for root in paths {
            for entry in walkdir::WalkDir::new(&root).follow_links(true).into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                let p = entry.path();
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if name.starts_with('.') {
                    continue;
                }
                seen += 1;
                if seen % 50 == 0 {
                    let _ = tx.send(ScanMsg::Progress(seen));
                    ctx.request_repaint();
                }
                if known.contains(p) {
                    continue;
                }
                match Track::from_file(p, 0) {
                    Some(t) => {
                        added += 1;
                        let _ = tx.send(ScanMsg::Found(t));
                    }
                    None => {
                        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        if ["wma", "wmv", "opus", "ape", "mpc", "ra", "rm"].contains(&ext.as_str()) {
                            unsupported += 1;
                        }
                    }
                }
            }
        }
        let _ = tx.send(ScanMsg::Done { added, unsupported });
        ctx.request_repaint();
    });
}

// ---------------------------------------------------------------- playlist files

/// Reads an M3U/M3U8 or PLS playlist file into a list of file paths.
pub fn read_playlist_file(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(path) else { return vec![] };
    let base = path.parent().unwrap_or(Path::new("."));
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut out = vec![];
    for line in text.lines() {
        let line = line.trim();
        let entry = if ext == "pls" {
            match line.split_once('=') {
                Some((k, v)) if k.to_lowercase().starts_with("file") => v.trim(),
                _ => continue,
            }
        } else {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            line
        };
        let entry = entry.strip_prefix("file://").unwrap_or(entry);
        let entry = percent_decode(entry);
        let p = PathBuf::from(&entry);
        out.push(if p.is_absolute() { p } else { base.join(p) });
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Writes tracks as an extended M3U file.
pub fn write_m3u(path: &Path, tracks: &[&Track]) -> std::io::Result<()> {
    let mut s = String::from("#EXTM3U\n");
    for t in tracks {
        s.push_str(&format!(
            "#EXTINF:{},{} - {}\n{}\n",
            t.duration_ms / 1000,
            t.artist,
            t.title,
            t.path.display()
        ));
    }
    std::fs::write(path, s)
}
