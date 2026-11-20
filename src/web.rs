//! The integrated web browser: tabs of native web views, and the web playlist of
//! playable links found on each page (Songbird's webScraper / scraperSteps.js).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Where the Home button goes. Songbird's own start page (birdhouse.songbirdnest.com)
/// no longer exists, so a plain search page stands in (estimated).
pub const DEFAULT_HOME: &str = "https://duckduckgo.com/";
pub const SEARCH_URL: &str = "https://duckduckgo.com/?q=";

/// Runs in every page. Mirrors scraperSteps.js: look at href, embed/@src, object/@data,
/// param/@value and enclosure/@url, keep URLs whose file extension is an audio type,
/// drop the "playerID…" false positive, drop duplicates, name each by its file name.
/// Clicking a playable link plays it in the player instead of opening it.
pub const SCRAPER_JS: &str = r#"
(function () {
  if (window.__sbScan) { window.__sbScan(); return; }
  var EXT = ['mp3','m4a','mp4','aac','flac','wav','aif','aiff','ogg','oga','mka','caf'];
  function media(raw) {
    if (!raw) return null;
    try {
      var u = new URL(raw, document.baseURI);
      if (u.protocol !== 'http:' && u.protocol !== 'https:') return null;
      var file = u.pathname.split('/').pop();
      if (file.indexOf('playerID') === 0) return null;
      var ext = (file.split('.').pop() || '').toLowerCase();
      return EXT.indexOf(ext) >= 0 ? u.href : null;
    } catch (e) { return null; }
  }
  function name(u) {
    var f = new URL(u).pathname.split('/').pop();
    try { f = decodeURIComponent(f); } catch (e) {}
    return f;
  }
  var last = '';
  function collect() {
    var out = [], seen = {};
    var nodes = document.querySelectorAll('a[href],embed[src],object[data],param[value],enclosure[url],audio[src],source[src]');
    for (var i = 0; i < nodes.length; i++) {
      var n = nodes[i];
      var raw = n.getAttribute('href') || n.getAttribute('src') || n.getAttribute('data') ||
                n.getAttribute('value') || n.getAttribute('url');
      var u = media(raw);
      if (u && !seen[u]) { seen[u] = 1; out.push([u, name(u)]); }
    }
    return out;
  }
  function scan() {
    var out = collect();
    var msg = JSON.stringify({ t: 'media', items: out });
    if (msg !== last && window.ipc) { last = msg; window.ipc.postMessage(msg); }
  }
  window.__sbScan = scan;
  window.__sbList = function () { return JSON.stringify(collect()); };
  if (document.readyState !== 'loading') scan();
  document.addEventListener('DOMContentLoaded', scan);
  window.addEventListener('load', function () { scan(); setTimeout(scan, 2000); });
  // ⌘V / ⌘C / ⌘X / ⌘A inside the page: with no Edit menu in the app, the page can't reach
  // the clipboard by itself, so the app does it (see Web::insert_text)
  function selected() {
    var el = document.activeElement;
    if (el && typeof el.selectionStart === 'number' && typeof el.value === 'string') {
      return el.value.substring(el.selectionStart, el.selectionEnd);
    }
    return String(window.getSelection ? window.getSelection() : '');
  }
  window.__sbInsert = function (t) {
    var el = document.activeElement;
    if (document.execCommand && document.execCommand('insertText', false, t)) return;
    if (el && typeof el.selectionStart === 'number' && typeof el.value === 'string') {
      var s = el.selectionStart, e2 = el.selectionEnd;
      el.value = el.value.slice(0, s) + t + el.value.slice(e2);
      el.selectionStart = el.selectionEnd = s + t.length;
      el.dispatchEvent(new Event('input', { bubbles: true }));
    }
  };
  document.addEventListener('keydown', function (e) {
    if (!e.metaKey || e.ctrlKey || e.altKey || !window.ipc) return;
    var k = (e.key || '').toLowerCase();
    if (k === 'v') {
      e.preventDefault();
      window.ipc.postMessage(JSON.stringify({ t: 'paste' }));
    } else if (k === 'c' || k === 'x') {
      var s = selected();
      if (s) {
        window.ipc.postMessage(JSON.stringify({ t: 'copy', text: s }));
        if (k === 'x') document.execCommand('delete');
        e.preventDefault();
      }
    } else if (k === 'a') {
      var el = document.activeElement;
      if (el && el.select) el.select(); else document.execCommand('selectAll');
      e.preventDefault();
    } else if (k === 'l' || k === 't' || k === 'w' || k === 'd') {
      e.preventDefault();
      window.ipc.postMessage(JSON.stringify({ t: 'key', key: k }));
    }
  }, true);
  document.addEventListener('click', function (e) {
    var a = e.target && e.target.closest ? e.target.closest('a[href]') : null;
    if (!a) return;
    var u = media(a.getAttribute('href'));
    if (u) {
      e.preventDefault(); e.stopPropagation();
      if (window.ipc) window.ipc.postMessage(JSON.stringify({ t: 'play', url: u, name: name(u) }));
    }
  }, true);
})();
"#;

pub enum WebEvent {
    Media(u64, Vec<(String, String)>),
    Play(u64, String, String),
    Title(u64, String),
    Loading(u64, bool, String),
    OpenTab(String),
    /// ⌘V in a page: put the clipboard's text where the page's cursor is.
    Paste(u64),
    /// ⌘C / ⌘X in a page: the selected text.
    Copy(String),
    /// App shortcuts pressed while the page had the keyboard (l, t, w).
    Key(u64, String),
    /// A picture of a tab's page (TIFF bytes), shown while menus cover the page.
    Snapshot(u64, Vec<u8>),
}

pub struct WebTab {
    pub id: u64,
    pub view: Option<wry::WebView>,
    /// Waiting for the window to create its web view.
    pub pending_url: Option<String>,
    pub title: String,
    pub url: String,
    pub address: String,
    pub loading: bool,
    /// Track ids of this page's web playlist (in Library::web_tracks).
    pub media: Vec<u64>,
    pub selection: Option<usize>,
}

pub struct Web {
    pub tabs: Vec<WebTab>,
    pub tx: Sender<WebEvent>,
    pub rx: Receiver<WebEvent>,
    pub next_id: u64,
    pub error: Option<String>,
}

impl Web {
    pub fn new() -> Web {
        let (tx, rx) = channel();
        Web { tabs: vec![], tx, rx, next_id: 1, error: None }
    }

    pub fn tab(&self, id: u64) -> Option<&WebTab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn tab_mut(&mut self, id: u64) -> Option<&mut WebTab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    /// Adds a tab; its web view is made on the next frame, when the window is at hand.
    pub fn open(&mut self, url: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let url = normalize(url);
        self.tabs.push(WebTab {
            id,
            view: None,
            pending_url: Some(url.clone()),
            title: "Loading…".into(),
            url: url.clone(),
            address: url,
            loading: true,
            media: vec![],
            selection: None,
        });
        id
    }

    pub fn close(&mut self, id: u64) {
        self.tabs.retain(|t| t.id != id);
    }

    /// Creates web views for new tabs as children of the main window.
    pub fn create_pending(&mut self, window: &impl wry::raw_window_handle::HasWindowHandle, bounds: wry::Rect) {
        for tab in &mut self.tabs {
            let Some(url) = tab.pending_url.take() else { continue };
            let id = tab.id;
            let tx_ipc = self.tx.clone();
            let tx_nav = self.tx.clone();
            let tx_title = self.tx.clone();
            let tx_load = self.tx.clone();
            let tx_new = self.tx.clone();
            let built = wry::WebViewBuilder::new()
                .with_url(url)
                .with_bounds(bounds)
                .with_visible(false)
                .with_initialization_script(SCRAPER_JS)
                .with_ipc_handler(move |req| {
                    let body = req.body();
                    if std::env::var_os("SB_DEBUG").is_some() { eprintln!("ipc: {body}"); }
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return };
                    match v.get("t").and_then(|t| t.as_str()) {
                        Some("media") => {
                            let items = v
                                .get("items")
                                .and_then(|i| i.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|p| {
                                            let u = p.get(0)?.as_str()?.to_string();
                                            let n = p.get(1)?.as_str()?.to_string();
                                            Some((u, n))
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let _ = tx_ipc.send(WebEvent::Media(id, items));
                        }
                        Some("paste") => {
                            let _ = tx_ipc.send(WebEvent::Paste(id));
                        }
                        Some("copy") => {
                            let t = v.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            let _ = tx_ipc.send(WebEvent::Copy(t));
                        }
                        Some("key") => {
                            let k = v.get("key").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            let _ = tx_ipc.send(WebEvent::Key(id, k));
                        }
                        Some("play") => {
                            let u = v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            let n = v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            let _ = tx_ipc.send(WebEvent::Play(id, u, n));
                        }
                        _ => {}
                    }
                })
                .with_navigation_handler(move |u| {
                    // a page that is itself an audio file plays in the player
                    if is_media_url(&u) {
                        let name = file_name(&u);
                        let _ = tx_nav.send(WebEvent::Play(id, u, name));
                        return false;
                    }
                    true
                })
                .with_document_title_changed_handler(move |t| {
                    let _ = tx_title.send(WebEvent::Title(id, t));
                })
                .with_on_page_load_handler(move |ev, u| {
                    let started = matches!(ev, wry::PageLoadEvent::Started);
                    if std::env::var_os("SB_DEBUG").is_some() { eprintln!("load started={started} {u}"); }
                    let _ = tx_load.send(WebEvent::Loading(id, started, u));
                })
                .with_new_window_req_handler(move |u, _| {
                    // links that open a new window open a new tab instead
                    let _ = tx_new.send(WebEvent::OpenTab(u));
                    wry::NewWindowResponse::Deny
                })
                .build_as_child(window);
            match built {
                Ok(v) => tab.view = Some(v),
                Err(e) => {
                    self.error = Some(format!("The web page couldn't open: {e}"));
                    tab.title = "Couldn't open".into();
                    tab.loading = false;
                }
            }
        }
    }

    /// Asks a tab's page for its playable links and reports them as a Media event.
    /// (Messages sent by file:// pages never reach the app, so this asks directly.)
    pub fn rescan(&self, id: u64) {
        let Some(v) = self.tab(id).and_then(|t| t.view.as_ref()) else { return };
        let _ = v.evaluate_script(SCRAPER_JS);
        let tx = self.tx.clone();
        let _ = v.evaluate_script_with_callback("window.__sbList ? window.__sbList() : '[]'", move |r| {
            // the result is the JSON form of the returned string
            let inner: String = serde_json::from_str(&r).unwrap_or_default();
            let Ok(list) = serde_json::from_str::<Vec<(String, String)>>(&inner) else { return };
            let _ = tx.send(WebEvent::Media(id, list));
        });
    }

    /// Asks a tab's web view for a picture of what it shows; it arrives as WebEvent::Snapshot.
    /// The native page sits above the app's drawing, so while a menu or dialog is open the page
    /// is hidden and this picture is drawn in its place (instead of a blank area).
    #[cfg(target_os = "macos")]
    pub fn snapshot(&self, id: u64, ctx: &egui::Context) -> bool {
        use wry::WebViewExtMacOS;
        let Some(v) = self.tab(id).and_then(|t| t.view.as_ref()) else { return false };
        let wk = v.webview();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let done = block2::RcBlock::new(
            move |img: *mut objc2_app_kit::NSImage, _err: *mut objc2_foundation::NSError| {
                // Safety: WebKit hands over a valid image or null.
                if std::env::var_os("SB_DEBUG").is_some() {
                    eprintln!("snapshot callback: image={} error={}", !img.is_null(), !_err.is_null());
                }
                let Some(img) = (unsafe { img.as_ref() }) else { return };
                if let Some(data) = img.TIFFRepresentation() {
                    let _ = tx.send(WebEvent::Snapshot(id, data.to_vec()));
                    ctx.request_repaint();
                }
            },
        );
        // Safety: called on the main thread with a live web view; the block is copied by WebKit.
        unsafe { wk.takeSnapshotWithConfiguration_completionHandler(None, &done) };
        true
    }

    #[cfg(not(target_os = "macos"))]
    pub fn snapshot(&self, _id: u64, _ctx: &egui::Context) -> bool {
        false
    }

    /// Types `text` into the focused field of a tab's page.
    pub fn insert_text(&self, id: u64, text: &str) {
        let Some(v) = self.tab(id).and_then(|t| t.view.as_ref()) else { return };
        let arg = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        let _ = v.evaluate_script(&format!("window.__sbInsert && window.__sbInsert({arg})"));
    }

    /// Shows only the active tab's web view, at `bounds`.
    pub fn layout(&self, active: Option<u64>, bounds: wry::Rect, hidden: bool) {
        for tab in &self.tabs {
            if let Some(v) = &tab.view {
                let show = !hidden && active == Some(tab.id);
                if show {
                    let _ = v.set_bounds(bounds);
                }
                let _ = v.set_visible(show);
            }
        }
    }
}

/// Turns what was typed into an address: adds https:// or makes it a web search.
pub fn normalize(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return DEFAULT_HOME.to_string();
    }
    if s.contains("://") || s.starts_with("about:") {
        return s.to_string();
    }
    if !s.contains(' ') && s.contains('.') {
        return format!("https://{s}");
    }
    search_url(s)
}

pub fn search_url(q: &str) -> String {
    let mut out = String::from(SEARCH_URL);
    for b in q.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn is_media_url(u: &str) -> bool {
    if !(u.starts_with("http://") || u.starts_with("https://")) {
        return false;
    }
    let path = u.split(['?', '#']).next().unwrap_or("");
    let file = path.rsplit('/').next().unwrap_or("");
    if file.starts_with("playerID") {
        return false;
    }
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    crate::library::AUDIO_EXTS.contains(&ext.as_str())
}

pub fn file_name(u: &str) -> String {
    let path = u.split(['?', '#']).next().unwrap_or("");
    let f = path.rsplit('/').next().unwrap_or(path);
    percent_decode(f)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Folder for files fetched to be played (Library/Caches/TuneCraft/web).
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("TuneCraft").join("web")
}

/// Folder the Download button saves into (Music/TuneCraft Downloads).
pub fn download_dir() -> PathBuf {
    dirs::audio_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Music")))
        .unwrap_or_else(std::env::temp_dir)
        .join("TuneCraft Downloads")
}

pub enum FetchMsg {
    /// (track id, saved file, true when it was a Download into the library)
    Done(u64, PathBuf, bool),
    Failed(u64, String),
}

/// Fetches a URL into `dir` on a background thread.
pub fn fetch(id: u64, url: String, dir: PathBuf, keep: bool, tx: Sender<FetchMsg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let result = (|| -> Result<PathBuf, String> {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let name = sanitize(&file_name(&url));
            let target = if keep { unique(&dir, &name) } else { dir.join(format!("{:016x}-{name}", hash(&url))) };
            if !keep && target.exists() {
                return Ok(target);
            }
            let resp = ureq::get(&url).call().map_err(|e| e.to_string())?;
            let mut reader = resp.into_body().into_reader();
            let part = target.with_extension("part");
            let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
            std::io::copy(&mut reader, &mut f).map_err(|e| e.to_string())?;
            std::fs::rename(&part, &target).map_err(|e| e.to_string())?;
            Ok(target)
        })();
        let _ = tx.send(match result {
            Ok(p) => FetchMsg::Done(id, p, keep),
            Err(e) => FetchMsg::Failed(id, e),
        });
        ctx.request_repaint();
    });
}

fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c == '/' || c == ':' || c.is_control() { '_' } else { c }).collect();
    if s.trim().is_empty() { "track.mp3".into() } else { s }
}

fn unique(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (2..).map(|n| dir.join(format!("{stem} {n}.{ext}"))).find(|p| !p.exists()).unwrap()
}

fn hash(s: &str) -> u64 {
    // FNV-1a, enough to name cache files
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_urls() {
        assert!(is_media_url("https://example.com/a/Song%20One.mp3?x=1"));
        assert!(!is_media_url("https://example.com/page.html"));
        assert!(!is_media_url("https://example.com/playerID=1.mp3"));
        assert_eq!(file_name("https://e.com/x/Song%20One.mp3?x"), "Song One.mp3");
    }

    #[test]
    fn addresses() {
        assert_eq!(normalize("example.com"), "https://example.com");
        assert_eq!(normalize("free music"), "https://duckduckgo.com/?q=free+music");
        assert_eq!(normalize("http://a.b/c"), "http://a.b/c");
    }
}

#[cfg(test)]
mod net_tests {
    /// Fetches an audio file and decodes it (needs the network):
    /// TUNECRAFT_TEST_AUDIO_URL=<http(s) URL of an audio file> cargo test -- --ignored
    #[test]
    #[ignore]
    fn fetch_and_decode_remote_mp3() {
        let Ok(url) = std::env::var("TUNECRAFT_TEST_AUDIO_URL") else {
            eprintln!("set TUNECRAFT_TEST_AUDIO_URL to run this test");
            return;
        };
        let url = url.as_str();
        let (tx, rx) = std::sync::mpsc::channel();
        let dir = std::env::temp_dir().join("songbird_player_net_test");
        super::fetch(1, url.to_string(), dir, false, tx, egui::Context::default());
        match rx.recv_timeout(std::time::Duration::from_secs(120)).expect("no answer") {
            super::FetchMsg::Done(_, path, _) => {
                let len = std::fs::metadata(&path).unwrap().len();
                eprintln!("saved {} ({} bytes)", path.display(), len);
                let t = crate::library::Track::from_file(&path, 1).expect("track");
                eprintln!("title={} artist={} ms={}", t.title, t.artist, t.duration_ms);
                rodio::Decoder::try_from(std::fs::File::open(&path).unwrap()).expect("decode");
            }
            super::FetchMsg::Failed(_, e) => panic!("fetch failed: {e}"),
        }
    }
}
