# TuneCraft (rebuilt from Songbird 2.2.0) — mechanics sheet

Taken from /Applications/Songbird.app (XULRunner 1.9.2, 32-bit i386, build 2453).
Sources: `chrome/songbird.jar` (XUL/JS), `chrome/gonzo.jar` (default "Gonzo" skin),
`chrome/locales.jar` (en-US strings), `jsmodules/*.jsm`,
`components/sbMediacoreManager.dylib` (playback sequencer, disassembled),
`components/sbPlaybackHistoryService.dylib`.

## Layout (gonzo `content/xul/mainplayer.xul`)
- Top: menu bar, then the nav bar (grey gradient #b9b9b9 → #5a5a5a, 28 px) with the search box.
- Middle: left **service pane** (Library ▸ Music; Playlists with smart + user playlists),
  with an **album-art display pane** at its bottom; centre **content pane** (media page:
  filter lists Genre | Artist | Album above the track list); right **sidebar display pane**
  (Play Queue); status bar under the content pane.
- Bottom **control pane** (gradient #b9b9b9 → #3c3c3c, 53 px): Back, Play/Pause, Forward;
  volume slider with min/max buttons; **faceplate** (dark #0d0d0d, 44 px) with
  title / artist / album, now-playing rating, elapsed time, seek bar with 4 notches, total
  time; shuffle and repeat buttons.
- Faceplate shows an inactive splash until something has played (`faceplate.seenplaying`).

## Colours (sampled from gonzo images/CSS)
| part | value |
|---|---|
| service pane background | #d4d4d4 (tile #dcdcdc/#cfcfcf) |
| service pane selected node | #f7f7f7 (white gloss) |
| container header node | #f2f2f2 with #bfbfbf lines |
| track rows | alternate #ffffff / #f7f7f7, 18 px (bg-empty-table.png 36 px tile) |
| table header | gradient #e6e6e6 → #b9bbbd, 15 px, 9 px labels |
| text | 11 px, rgb(40,40,40); secondary rgb(131,131,131) |
| link/accent | #1694d9 |
| status bar | #f0f0f0 → #fdfdfe |
| display pane empty | #d9d9d9 |

## Track list (`jsmodules/sbColumnSpecParser.jsm`, hard-coded fallback)
Columns and widths (px): Title 229, Time 45, Artist 137 (**sorted ascending**), Album 210,
Genre 90, Rating 90, Type 78.
Filter lists on the library page (`mediapages/filtersPage.js`): default filter set
Genre, Artist, Album (cascading — each narrows the next), search is the first filter.

## Playback controls (`content/songbird/bindings/playerControls.xml`)
- Repeat button cycles **none(0) → all(2) → one(1) → none(0)** (`onRepeat`).
- Shuffle toggles sequencer mode FORWARD ↔ SHUFFLE (`onShuffle`).
- Forward/Back: a click calls `sequencer.next()` / `previous()`. Holding the button while
  playing starts a 1000 ms interval (`skipLoop`): each tick seeks **+15000 ms** (forward,
  capped at duration − 1000 ms) or **−15000 ms** (back, floored at 1 ms); after a hold the
  release does not change track (`seen_skip`).
- Play/Pause button: shows Pause while playing and not paused; holding it 2000 ms
  (`delayDuration="2000"`) opens Play / Pause / Stop menu.
- Clicking the total-time label toggles **remaining time** (`faceplate.showremainingtime`);
  ignored (reset to off) while duration is 0.
- Volume slider 0..1 (scale 10000); dragging to 0 mutes; un-mute restores the last
  non-zero volume (`faceplate.volume.last`).
- Mouse wheel over the window: volume ±**0.03** (`scripts/mouseWheelVolume.js`), not over lists.
- Menu/keys (`scripts/mainPlayerWindow.js`, `xul/layoutBaseOverlay.xul`): volume up/down
  ⌘↑/⌘↓ = ±**0.05**; mute ⌘⌥↑/↓; next Ctrl→; previous Ctrl←; play/pause Space;
  New Playlist ⌘N; New Smart Playlist ⌘⌥N; Open file ⌘O; Search focus ⌘K.

## Sequencer (`sbMediacoreManager.dylib`)
- Shuffle order: `sbMediacoreShuffleSequenceGenerator::Init` (0x99d0–0x9b8d) fills
  0..n−1, `srand(time)`, then for i = 1..n−1 swaps item i with item `rand() % (i+1)`
  (Fisher–Yates over the whole view).
- Previous: no restart-current-song time threshold exists in the sequencer code (searched
  for 1000/2000/3000/5000 ms constants: none) → Previous steps to the previous item.
  *(unknown unk_9234597b0c216905 tracks this)*
- Resume position is only saved when position + **10000 ms** < duration
  (`UpdateLastPositionProperty`, 0x12a23) and only for video (`songbird.mediacore.resumePlaybackPosition`).
- Duration/position are re-read on a timer (`HandleSequencerTimer`, 0x122d0); time strings
  come from `EmitMillisecondsToTimeString` (0x184c0).
- At the end of the view: repeat all wraps to the start, repeat one replays, none stops.
  *(behaviour from data-remote names `playlist.repeat`; the auto-advance vs user Next detail
  for repeat-one is estimated: user Next moves on.)*

## Smart playlists (`scripts/mainWinInit.js`)
Defaults created on first run (all exclude video, match ALL, no limit, auto-update):
- **Highest Rated**: rating > 3, sorted by rating (descending).
- **Recently Added**: date added in the last 30 days (2 592 000 000 ms), sorted by date added.
- **Recently Played**: last played in the last 7 days (604 800 000 ms), sorted by last played.
Operators (locale `smart.*`): text contains / does not contain / is / is not / starts with /
ends with; numbers is / is not / less / greater / ≤ / ≥; dates in the last / not in the last.

## Strings
- "Library", "Music", "Playlists", "Create Playlist" (`servicesource.*`).
- Item count "%S items" / "1 item"; time summary `formatSingle` (`sbTimeFormatter.jsm`):
  ≥3600 s → hours with one decimal (floor), ≥60 s → minutes with one decimal, else seconds.
- `formatHMS`: "00:32", "01:06", "137:42:12".
- Empty library: "To Import Media, Choose a Folder".

## Play count
`sbPlaybackHistoryService::VerifyDataAndCreateNewEntry` (0xc140) records an entry when a
track stops; the exact minimum-play rule wasn't recoverable → **estimated**: play count +1 and
last-played = now when a track plays to its end.

## Not rebuilt (decided)
Web browser tabs, device sync, CD ripping, podcasts, Last.fm, add-ons, feathers switching,
mini player, equalizer.

## Integrated web browser (added on request)
Evidence: `defaults/preferences/songbird-prefs.js`, `content/songbird/bindings/browser/*`,
`content/songbird/scripts/webscraper/scraperSteps.js`, gonzo `mainplayer.xul`.
- Tabs (`sb-tabbrowser`, `hasmediatab="true"`): the first tab is always the media tab
  (library/playlists); web pages open in further tabs. The tab strip hides while only the
  media tab exists (`browser.tabs.autoHide = true`); closing a tab selects its neighbour
  (`browser.tabs.selectOwnerOnClose`). New Tab ⌘T (`menu.file.newtab.key`), Close ⌘W,
  Location ⌘L. Links that open a new window open a new tab.
- Nav bar: Back, Forward, Reload/Stop, Home, location bar; the search box filters the
  library on the media tab and searches the web on a web tab.
- Home page: Songbird's `browser.startup.homepage` was http://birdhouse.songbirdnest.com/,
  which no longer exists → a search page stands in (**estimated**); web search engine is
  likewise a stand-in (**estimated**).
- Web playlist (`songbird.webplaylist.enabled = 1`): every page is scanned for URLs in
  href, embed/@src, object/@data, param/@value, enclosure/@url (plus audio/source src),
  kept when the file extension is an audio type, dropping names starting with "playerID"
  (Hacks_DropBadUrls), dropping duplicates; the track name defaults to the file name
  (scraperSteps.js line 280). Columns (`songbird.webplaylist.columnspec`): # 28, Title 280,
  Time 38, Artist 169, Album 159, Download 83.
- Clicking a playable link on a page plays it in the player; double-click in the web
  playlist plays the page's list in order; Download saves the file into
  Music/Songbird Player Downloads and adds it to the Library. A web track is fetched before
  it plays (faceplate shows "Buffering…", like `faceplate.buffering`).
- Pages are scanned again every 3 s while their tab is in front (**estimated**, for pages that
  add links later).
- Copy/paste (added on request): the location bar keeps its text on the media tab too, and
  has a right-click menu with Paste, Paste and Go, Copy and Clear. Clicking any of the app's
  own controls takes the keyboard back from the web page. Inside web pages ⌘V/⌘C/⌘X/⌘A are
  handled by the app (no Edit menu: winit's window can't take menu paste commands, which
  would break ⌘V in the app's own fields). Local file:// pages can't send these messages
  (a web-view library limit), so page shortcuts work on http/https pages only.

## Sound output (fix)
rodio 0.23 opens the output with a fixed 50 ms buffer (2048 frames at 44.1 kHz). The Mac's
current output (a Samsung soundbar) only accepts 14..=1024 frames, so nothing could play.
The player now asks for a buffer inside the range the device reports (here 1024), then the
device's default buffer, then rodio's own fallbacks (`player.rs` open_output).

## Bookmarks (added on request)
Songbird 2.2 kept only traces of bookmarks: a "bookmarks" service-pane container type
(gonzo colors.css `servicesource-bookmarks`), `service-pane/icon-bookmark.png`, and
`bookmarkPage.key = D` (songbird.dtd). Rebuilt as: a BOOKMARKS section in the service pane
(click opens in the front web tab or a new tab from the media tab; right-click Open, Open in
New Tab, Rename, Copy Address, Move Up/Down, Delete), a star button beside the location bar
(filled when the page is saved), ⌘D (also inside web pages) and a Bookmarks menu. Saved
with the library. Layout and wording are **estimated** (no 2.2 bookmark UI to copy).

## Resizable web playlist (added on request)
A grip bar between the web page and its web playlist (like Songbird's `sb-smart-splitter`
with `grippy`) is dragged up/down to set the list's height (at least the header plus one row,
leaving 60 pt of page). Double-click the bar, or the Expand/Restore button on the list's
header, fills the area / goes back. The height is saved in the settings. Sizes **estimated**.

## TuneCraft name and Craft-apps design (on request)
The app is now **TuneCraft** (window, menus, `ai.storyteller.tunecraft` app id; library in
Application Support/TuneCraft, copied over once from the old Songbird Player folder;
downloads in Music/TuneCraft Downloads). Its look follows the other Craft apps:
- ArtCraft Launcher, PhotoCraft and VectorCraft are egui apps whose saved settings use
  `theme_preference: System, fallback_theme: Dark`: TuneCraft follows the Mac's light/dark
  setting (dark when unknown), with View > Appearance to pick Light or Dark.
- VectorCraft/PhotoCraft attribution lists Inter (Regular/Medium/SemiBold) as the UI
  font, JetBrains Mono for numbers and Lucide line icons: the same two fonts are bundled
  (SIL OFL 1.1, assets/fonts) and the icons are drawn as thin lines.
- ArtCraft Launcher's artwork colours: near-black #0b0b0c, off-white #f2f1ee, blue
  #4e7bfb: the accent and the light/dark extremes; its window layout keeps a 48 pt top bar.
- Exact panel greys of the "proMedium" dark theme weren't readable from the files, so the
  greys in between are **estimated**. The TuneCraft mark (blue rounded square, white note) is
  original; no Craft app logo was copied.

## Web page behind menus (fix)
The web page is a native view drawn above the app, so menus and dialogs would open behind it;
it is hidden while they're open. To avoid a blank area, TuneCraft takes a picture of the page
(WKWebView snapshot) when the pointer moves toward the top bar or a menu opens, and draws that
picture in the page's place until the menu closes. Without a picture yet, the page stays up for
at most 0.4 s first. Pictures older than 0.6 s are retaken.
