//! Drawing the parts of the main window.

use crate::app::{COLUMNS, Col, SkipHold, TuneCraftApp, cell_text, ellipsize, group_thousands};
use crate::library::{Condition, Field, FieldKind, Op, Playlist, PlaylistKind};
use crate::player::{self, Source, State};
use crate::theme::{self, gray};
use egui::{Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashSet;
use std::time::{Duration, Instant};

const FILTER_NAMES: [&str; 3] = ["Genre", "Artist", "Album"];
const FILTER_PLURALS: [&str; 3] = ["genres", "artists", "albums"];
const FACE_NOTE: Color32 = Color32::from_rgb(90, 90, 90);

/// Alternating row background, like Gonzo's bg-empty-table.png.
fn stripes(p: &egui::Painter, body: Rect) {
    let mut y = body.top();
    let mut i = 0;
    while y < body.bottom() {
        let r = Rect::from_min_max(pos2(body.left(), y), pos2(body.right(), (y + theme::ROW_H).min(body.bottom())));
        p.rect_filled(r, 0.0, if i % 2 == 0 { theme::ROW_A() } else { theme::ROW_B() });
        y += theme::ROW_H;
        i += 1;
    }
}

enum NodeIcon {
    Library,
    Playlist,
    Smart,
    Bookmark,
}

enum RowAction {
    Click(usize, Modifiers),
    Play(usize),
    Rate(u64, u8),
}

impl TuneCraftApp {
    // ------------------------------------------------------------------ nav bar

    pub fn nav_bar(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let cy = rect.center().y;
        let tab = self.active_tab;
        let (can_back, can_fwd, loading) = tab
            .and_then(|id| self.web.tab(id))
            .map(|t| {
                let v = t.view.as_ref();
                (
                    v.and_then(|v| v.can_go_back().ok()).unwrap_or(false),
                    v.and_then(|v| v.can_go_forward().ok()).unwrap_or(false),
                    t.loading,
                )
            })
            .unwrap_or((false, false, false));

        // browser buttons: back, forward, reload/stop, home (browserControls.xml)
        let mut x = rect.left() + 14.0;
        let nav_btn = |ui: &mut Ui, x: &mut f32, kind: u8, enabled: bool, tip: &str| -> bool {
            let c = pos2(*x, cy);
            *x += 28.0;
            let r = ui.interact(Rect::from_center_size(c, vec2(24.0, 22.0)), ui.scope_id().with(("nav", kind)), Sense::click());
            let r = r.on_hover_text(tip);
            let icon = theme::round_button(ui.painter(), c, 10.5, r.hovered(), r.is_pointer_button_down_on() && enabled, enabled);
            let p = ui.painter();
            match kind {
                0 => theme::triangle_left(p, c + vec2(-0.5, 0.0), 9.0, icon),
                1 => theme::triangle_right(p, c + vec2(0.5, 0.0), 9.0, icon),
                2 => {
                    let pts: Vec<Pos2> = (0..=20)
                        .map(|i| {
                            let a = 0.6 + 5.0 * i as f32 / 20.0;
                            c + vec2(a.cos(), a.sin()) * 5.0
                        })
                        .collect();
                    let end = *pts.last().unwrap();
                    p.add(egui::Shape::line(pts, Stroke::new(1.6, icon)));
                    p.add(egui::Shape::convex_polygon(
                        vec![end + vec2(-1.0, -3.5), end + vec2(3.5, 0.5), end + vec2(-1.5, 2.0)],
                        icon,
                        Stroke::NONE,
                    ));
                }
                3 => {
                    p.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], Stroke::new(1.8, icon));
                    p.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], Stroke::new(1.8, icon));
                }
                _ => {
                    p.add(egui::Shape::convex_polygon(
                        vec![c + vec2(0.0, -6.0), c + vec2(6.0, 0.0), c + vec2(-6.0, 0.0)],
                        icon,
                        Stroke::NONE,
                    ));
                    p.rect_filled(Rect::from_min_max(c + vec2(-4.0, 0.0), c + vec2(4.0, 5.0)), 0.0, icon);
                }
            }
            enabled && r.clicked()
        };
        let back = nav_btn(ui, &mut x, 0, can_back, "Back");
        let fwd = nav_btn(ui, &mut x, 1, can_fwd, "Forward");
        let reload = if loading {
            nav_btn(ui, &mut x, 3, true, "Stop")
        } else {
            nav_btn(ui, &mut x, 2, tab.is_some(), "Reload")
        };
        let home = nav_btn(ui, &mut x, 4, true, "Home");
        if let Some(v) = tab.and_then(|id| self.web.tab(id)).and_then(|t| t.view.as_ref()) {
            if back {
                let _ = v.go_back();
            }
            if fwd {
                let _ = v.go_forward();
            }
            if reload {
                if loading {
                    let _ = v.evaluate_script("window.stop()");
                } else {
                    let _ = v.reload();
                }
            }
        }
        if home {
            self.go_to(crate::web::DEFAULT_HOME);
        }

        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(x - 6.0, rect.top()), rect.max))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut bar;
        if tab.is_none() {
            let b = egui::Button::new(egui::RichText::new("+  Import Media").color(theme::TEXT()).size(11.0))
                .fill(gray(0xe4))
                .stroke(Stroke::new(1.0, gray(0x55)));
            if ui.add(b).on_hover_text("Choose a folder of music to add").clicked() {
                let ctx = ui.ctx().clone();
                self.menu_import_folder(&ctx);
            }
            if self.scan_rx.is_some() {
                ui.add(egui::Spinner::new().size(12.0).color(Color32::WHITE));
                ui.label(
                    egui::RichText::new(format!("Importing… {} added", self.scan_added))
                        .color(Color32::WHITE)
                        .size(11.0),
                );
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            // search box: filters the library on the media tab, searches the web on a web tab
            let hint = match (tab, self.source) {
                (Some(_), _) => "Search the web".to_string(),
                (None, Source::Playlist(p)) => {
                    format!("Search {}", self.lib.playlist(p).map(|p| p.name.as_str()).unwrap_or(""))
                }
                _ => "Search Library".to_string(),
            };
            if !self.search.is_empty() && ui.small_button("×").on_hover_text("Clear").clicked() {
                self.search.clear();
                self.view_dirty = true;
            }
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(hint)
                    .desired_width(180.0)
                    .font(FontId::proportional(11.0)),
            );
            if self.focus_search {
                r.request_focus();
                self.focus_search = false;
            }
            if r.changed() && tab.is_none() {
                self.view_dirty = true;
            }
            if tab.is_some() && r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.search.trim().is_empty() {
                let q = crate::web::search_url(&self.search);
                self.search.clear();
                self.go_to(&q);
            }
            ui.add_space(6.0);
            // bookmark button: filled star when this page is saved
            if let Some((url, _)) = self.current_page() {
                let marked = self.bookmark_for(&url).is_some();
                let (rect, r) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                let col = if marked {
                    Color32::from_rgb(0xf2, 0xc2, 0x2e)
                } else if r.hovered() {
                    Color32::WHITE
                } else {
                    gray(0xe8)
                };
                theme::star(ui.painter(), rect.center(), 8.0, gray(0x40));
                theme::star(ui.painter(), rect.center(), 6.8, col);
                let tip = if marked { "Remove Bookmark (⌘D)" } else { "Bookmark This Page (⌘D)" };
                if r.on_hover_text(tip).clicked() {
                    self.toggle_bookmark();
                }
                ui.add_space(2.0);
            }
            // location bar (with the bookmark star on its right)
            // the media tab keeps its own address text, so typing or pasting there sticks
            let mut address = match tab.and_then(|id| self.web.tab(id)) {
                Some(t) => t.address.clone(),
                None => self.media_address.clone(),
            };
            let w = ui.available_width() - 6.0;
            let r = ui.add(
                egui::TextEdit::singleline(&mut address)
                    .id_salt("location")
                    .hint_text("Enter or paste a web address")
                    .desired_width(w.max(80.0))
                    .font(FontId::proportional(11.0)),
            );
            if self.focus_address {
                r.request_focus();
                self.focus_address = false;
            }
            self.address_focused = r.has_focus();
            let mut go = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            // right-click: Paste / Paste and Go (reads the clipboard directly)
            r.context_menu(|ui| {
                let clip = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok()).unwrap_or_default();
                let clip = clip.trim().to_string();
                if ui.add_enabled(!clip.is_empty(), egui::Button::new("Paste")).clicked() {
                    address.push_str(&clip);
                    ui.close();
                }
                if ui.add_enabled(!clip.is_empty(), egui::Button::new("Paste and Go")).clicked() {
                    address = clip.clone();
                    go = true;
                    ui.close();
                }
                if ui.add_enabled(!address.is_empty(), egui::Button::new("Copy")).clicked() {
                    ui.ctx().copy_text(address.clone());
                    ui.close();
                }
                if ui.add_enabled(!address.is_empty(), egui::Button::new("Clear")).clicked() {
                    address.clear();
                    ui.close();
                }
            });
            match tab.and_then(|id| self.web.tab_mut(id)) {
                Some(t) => t.address = address.clone(),
                None => self.media_address = address.clone(),
            }
            if go && !address.trim().is_empty() {
                if tab.is_none() {
                    self.media_address.clear();
                }
                self.go_to(&address);
            }
        });
    }

    // ------------------------------------------------------------------ tabs and pages

    /// Songbird's tab strip: the media tab first, then web pages (shown once a page is open).
    pub fn tab_strip(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, theme::p().panel);
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, gray(0x80)));
        let mut x = rect.left() + 4.0;
        let h = rect.height() - 4.0;
        let mut clicked: Option<Option<u64>> = None;
        let mut close: Option<u64> = None;
        let mut entries: Vec<(Option<u64>, String, bool)> = vec![(None, "Media".to_string(), false)];
        for t in &self.web.tabs {
            let title = if t.title.trim().is_empty() { t.url.clone() } else { t.title.clone() };
            entries.push((Some(t.id), title, t.loading));
        }
        let avail = rect.width() - 40.0;
        let tab_w = ((avail - 90.0) / (entries.len() - 1).max(1) as f32).clamp(80.0, 200.0);
        for (id, title, loading) in entries {
            let w = if id.is_none() { 84.0 } else { tab_w };
            let r = Rect::from_min_size(pos2(x, rect.top() + 3.0), vec2(w, h));
            let resp = ui.interact(r, ui.scope_id().with(("tab", id)), Sense::click());
            let selected = self.active_tab == id;
            let p = ui.painter();
            if selected {
                p.rect_filled(r, 6.0, theme::p().bg);
                p.hline(r.shrink2(vec2(8.0, 0.0)).x_range(), r.bottom() - 1.0, Stroke::new(2.0, theme::ACCENT()));
            } else if resp.hovered() {
                p.rect_filled(r, 6.0, theme::p().hover);
            }
            p.vline(r.right(), r.shrink2(vec2(0.0, 5.0)).y_range(), Stroke::new(1.0, theme::p().line));
            let mut tx = r.left() + 8.0;
            if id.is_none() {
                let ic = pos2(tx + 5.0, r.center().y);
                p.rect_stroke(Rect::from_center_size(ic, vec2(10.0, 8.0)), 1.0, Stroke::new(1.2, gray(0x50)), egui::StrokeKind::Middle);
                tx += 15.0;
            } else if loading {
                let t = ui.input(|i| i.time) as f32;
                let c = pos2(tx + 5.0, r.center().y);
                p.circle_stroke(c, 4.5, Stroke::new(1.0, gray(0x90)));
                p.line_segment([c, c + vec2((t * 6.0).cos(), (t * 6.0).sin()) * 4.5], Stroke::new(1.5, gray(0x40)));
                tx += 15.0;
            }
            let text_w = r.right() - tx - if id.is_some() { 22.0 } else { 6.0 };
            p.text(
                pos2(tx, r.center().y),
                Align2::LEFT_CENTER,
                ellipsize(&title, text_w, 11.0, ui),
                FontId::proportional(11.0),
                if selected { theme::TEXT() } else { theme::TEXT_DIM() },
            );
            if let Some(tid) = id {
                let xr = Rect::from_center_size(pos2(r.right() - 11.0, r.center().y), vec2(14.0, 14.0));
                let xresp = ui.interact(xr, ui.scope_id().with(("tabx", tid)), Sense::click()).on_hover_text("Close Tab");
                let col = if xresp.hovered() { theme::ACCENT() } else { gray(0x60) };
                theme::cross(ui.painter(), xr.center(), 3.5, col);
                if xresp.clicked() {
                    close = Some(tid);
                } else if resp.middle_clicked() {
                    close = Some(tid);
                }
            }
            if resp.clicked() && close.is_none() {
                clicked = Some(id);
            }
            x += w + 1.0;
        }
        let plus = Rect::from_center_size(pos2(x + 14.0, rect.center().y + 1.0), vec2(20.0, 18.0));
        let pr = ui.interact(plus, ui.scope_id().with("newtab"), Sense::click()).on_hover_text("New Tab");
        ui.painter().text(plus.center(), Align2::CENTER_CENTER, "+", FontId::proportional(16.0), if pr.hovered() { theme::ACCENT() } else { theme::TEXT_DIM() });
        if pr.clicked() {
            self.new_tab(crate::web::DEFAULT_HOME);
        }
        if let Some(id) = close {
            self.close_tab(id);
        } else if let Some(id) = clicked {
            self.active_tab = id;
        }
    }

    /// A web tab: the page above, its web playlist below (when the page has playable links).
    pub fn web_page(&mut self, ui: &mut Ui, rect: Rect, tab: u64) {
        let media = self.web.tab(tab).map(|t| t.media.clone()).unwrap_or_default();
        // the web playlist's height is set by dragging the splitter above it
        // (sb-smart-splitter with a grippy); double-click the splitter to fill / restore
        const SPLIT: f32 = 7.0;
        const MIN_LIST: f32 = 56.0; // header + column titles + one row
        const MIN_PAGE: f32 = 60.0;
        let max_list = (rect.height() - MIN_PAGE - SPLIT).max(MIN_LIST);
        let list_h = if media.is_empty() {
            0.0
        } else {
            self.lib.settings.web_playlist_height.clamp(MIN_LIST, max_list)
        };
        let split_h = if media.is_empty() { 0.0 } else { SPLIT };
        let page = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - list_h - split_h));
        if split_h > 0.0 {
            let bar = Rect::from_min_max(pos2(rect.min.x, page.max.y), pos2(rect.max.x, page.max.y + SPLIT));
            let r = ui
                .interact(bar.expand2(vec2(0.0, 2.0)), ui.scope_id().with("webpl-split"), Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                .on_hover_text("Drag to resize the web playlist; double-click to expand or restore");
            if r.dragged() {
                let dy = r.drag_delta().y;
                self.lib.settings.web_playlist_height = (list_h - dy).clamp(MIN_LIST, max_list);
                self.lib.dirty = true;
            }
            if r.double_clicked() {
                // same as the Expand / Restore button on the list's header
                if self.lib.settings.web_playlist_height >= 10_000.0 {
                    self.lib.settings.web_playlist_height = self.web_playlist_restore.take().unwrap_or(180.0);
                } else {
                    self.web_playlist_restore = Some(list_h);
                    self.lib.settings.web_playlist_height = 100_000.0;
                }
                self.lib.dirty = true;
            }
            let p = ui.painter();
            p.rect_filled(bar, 0.0, theme::p().panel);
            p.hline(bar.x_range(), bar.top() + 0.5, Stroke::new(1.0, gray(0x90)));
            p.hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, gray(0x90)));
            // grippy dots in the middle
            let c = bar.center();
            let col = if r.hovered() || r.dragged() { gray(0x30) } else { gray(0x70) };
            for k in -2..=2 {
                p.circle_filled(c + vec2(k as f32 * 5.0, 0.0), 1.2, col);
            }
        }
        ui.painter().rect_filled(page, 0.0, Color32::WHITE);
        // under the native page: its latest picture, which shows while menus or dialogs are open
        if let Some((_, _, tex)) = self.page_shot.as_ref().filter(|s| s.0 == tab) {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            ui.painter().image(tex.id(), page, uv, Color32::WHITE);
        }
        let loading = self.web.tab(tab).map(|t| t.loading && t.view.is_none()).unwrap_or(false);
        if loading {
            ui.painter().text(page.center(), Align2::CENTER_CENTER, "Loading…", FontId::proportional(13.0), theme::TEXT_DIM());
        }
        self.web_rect = Some(page);
        if list_h > 0.0 {
            let list = Rect::from_min_max(pos2(rect.min.x, page.max.y + split_h), rect.max);
            self.web_playlist(ui, list, tab, &media);
        }
    }

    /// Columns from songbird.webplaylist.columnspec: # 28, Title 280, Time 38,
    /// Artist 169, Album 159, Download 83.
    fn web_playlist(&mut self, ui: &mut Ui, rect: Rect, tab: u64, media: &[u64]) {
        const WCOLS: [(&str, f32); 6] = [("#", 28.0), ("Title", 280.0), ("Time", 38.0), ("Artist", 169.0), ("Album", 159.0), ("", 83.0)];
        let total: f32 = WCOLS.iter().map(|c| c.1).sum();
        let scale = rect.width() / total;
        let widths: Vec<f32> = WCOLS.iter().map(|c| c.1 * scale).collect();
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 20.0));
        theme::display_header(ui.painter(), bar);
        let n = media.len();
        let label = if n == 1 { "Web Playlist — 1 playable item on this page".to_string() } else { format!("Web Playlist — {n} playable items on this page") };
        ui.painter().text(bar.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(11.0), theme::TEXT());
        // Expand / Restore: the list fills the page area, or goes back to its earlier height
        let expanded = self.lib.settings.web_playlist_height >= 10_000.0;
        let btn = Rect::from_center_size(bar.right_center() - vec2(42.0, 0.0), vec2(72.0, 15.0));
        let br = ui
            .interact(btn, ui.scope_id().with("webpl-expand"), Sense::click())
            .on_hover_text("Or drag the bar above the list");
        ui.painter().rect_filled(btn, 7.0, if br.hovered() { gray(0xf4) } else { gray(0xe2) });
        ui.painter().rect_stroke(btn, 7.0, Stroke::new(1.0, gray(0x90)), egui::StrokeKind::Inside);
        let tri = btn.left_center() + vec2(10.0, 0.0);
        let pts = if expanded {
            vec![tri + vec2(-3.5, -2.0), tri + vec2(3.5, -2.0), tri + vec2(0.0, 2.5)]
        } else {
            vec![tri + vec2(-3.5, 2.0), tri + vec2(3.5, 2.0), tri + vec2(0.0, -2.5)]
        };
        ui.painter().add(egui::Shape::convex_polygon(pts, gray(0x40), Stroke::NONE));
        ui.painter().text(
            btn.center() + vec2(6.0, 0.0),
            Align2::CENTER_CENTER,
            if expanded { "Restore" } else { "Expand" },
            FontId::proportional(9.5),
            theme::TEXT(),
        );
        if br.clicked() {
            if expanded {
                self.lib.settings.web_playlist_height = self.web_playlist_restore.take().unwrap_or(180.0);
            } else {
                self.web_playlist_restore = Some(rect.height());
                self.lib.settings.web_playlist_height = 100_000.0;
            }
            self.lib.dirty = true;
        }
        let header = Rect::from_min_size(pos2(rect.min.x, bar.max.y), vec2(rect.width(), theme::HEADER_H));
        theme::table_header(ui.painter(), header);
        let mut x = rect.left();
        for (ci, (name, _)) in WCOLS.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(x, header.top()), vec2(widths[ci], header.height()));
            ui.painter().text(cell.left_center() + vec2(5.0, 0.0), Align2::LEFT_CENTER, *name, FontId::proportional(9.5), theme::TEXT());
            ui.painter().vline(cell.right(), cell.y_range(), Stroke::new(1.0, gray(0xa8)));
            x += widths[ci];
        }
        let body = Rect::from_min_max(pos2(rect.min.x, header.max.y), rect.max);
        stripes(ui.painter(), body);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        let current = self.seq.current();
        let playing = self.state != State::Stopped;
        let sel = self.web.tab(tab).and_then(|t| t.selection);
        let mut play: Option<usize> = None;
        let mut select: Option<usize> = None;
        let mut download: Option<u64> = None;
        let mut queue: Option<u64> = None;
        egui::ScrollArea::vertical().id_salt(("webpl", tab)).auto_shrink(false).show_rows(&mut child, theme::ROW_H, n, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in range {
                let id = media[row];
                let Some(t) = self.lib.track(id) else { continue };
                let (rr, resp) = ui.allocate_exact_size(vec2(ui.available_width(), theme::ROW_H), Sense::click_and_drag());
                let selected = sel == Some(row);
                let p = ui.painter();
                let bg = if selected { theme::SELECT() } else if row % 2 == 0 { theme::ROW_A() } else { theme::ROW_B() };
                p.rect_filled(rr, 0.0, bg);
                let tc = if selected { Color32::WHITE } else { theme::TEXT() };
                let font = FontId::proportional(11.0);
                let mut x = rr.left();
                let cells = [
                    format!("{}", row + 1),
                    t.title.clone(),
                    if t.duration_ms > 0 { player::fmt_time(t.duration_ms) } else { String::new() },
                    t.artist.clone(),
                    t.album.clone(),
                ];
                for (ci, text) in cells.iter().enumerate() {
                    let cell = Rect::from_min_size(pos2(x, rr.top()), vec2(widths[ci], rr.height()));
                    let mut tx = cell.left() + 5.0;
                    if ci == 1 && playing && current == Some(id) {
                        let ic = if selected { Color32::WHITE } else { theme::ACCENT() };
                        if self.buffering == Some(id) {
                            let tt = ui.input(|i| i.time) as f32;
                            let c = pos2(tx + 4.0, cell.center().y);
                            p.circle_stroke(c, 4.0, Stroke::new(1.0, ic));
                            p.line_segment([c, c + vec2((tt * 6.0).cos(), (tt * 6.0).sin()) * 4.0], Stroke::new(1.5, ic));
                        } else {
                            theme::speaker(p, pos2(tx + 6.0, cell.center().y), 9.0, ic, 1);
                        }
                        tx += 14.0;
                    }
                    if ci == 0 || ci == 2 {
                        p.text(pos2(cell.right() - 5.0, cell.center().y), Align2::RIGHT_CENTER, text, font.clone(), tc);
                    } else {
                        p.text(pos2(tx, cell.center().y), Align2::LEFT_CENTER, ellipsize(text, cell.right() - tx - 5.0, 11.0, ui), font.clone(), tc);
                    }
                    x += widths[ci];
                }
                // download button
                let cell = Rect::from_min_size(pos2(x, rr.top()), vec2(widths[5], rr.height()));
                let btn = cell.shrink2(vec2(6.0, 2.0));
                let state = if self.downloaded.contains(&id) {
                    "In Library"
                } else if self.downloading.contains(&id) {
                    "Downloading"
                } else {
                    "Download"
                };
                let br = ui.interact(btn, ui.scope_id().with(("dl", id)), Sense::click());
                let active = state == "Download";
                let fill = if active && br.hovered() { gray(0xf4) } else { gray(0xe2) };
                ui.painter().rect_filled(btn, 7.0, fill);
                ui.painter().rect_stroke(btn, 7.0, Stroke::new(1.0, gray(0x90)), egui::StrokeKind::Inside);
                ui.painter().text(btn.center(), Align2::CENTER_CENTER, state, FontId::proportional(9.5), if active { theme::TEXT() } else { theme::TEXT_DIM() });
                if active && br.on_hover_text("Save this file and add it to your Library").clicked() {
                    download = Some(id);
                } else if resp.clicked() {
                    select = Some(row);
                }
                if resp.double_clicked() {
                    play = Some(row);
                }
                if resp.dragged() {
                    resp.dnd_set_drag_payload(vec![id]);
                }
                resp.context_menu(|ui| {
                    if ui.button("Play").clicked() {
                        play = Some(row);
                    }
                    if ui.button("Add to Play Queue").clicked() {
                        queue = Some(id);
                    }
                    if ui.add_enabled(active, egui::Button::new("Download")).clicked() {
                        download = Some(id);
                    }
                    if ui.button("Copy Address").clicked() {
                        if let Some(u) = self.lib.track(id).and_then(|t| t.web_url.clone()) {
                            ui.ctx().copy_text(u);
                        }
                    }
                });
            }
        });
        if let Some(r) = select {
            if let Some(t) = self.web.tab_mut(tab) {
                t.selection = Some(r);
            }
        }
        if let Some(id) = download {
            self.download(id);
        }
        if let Some(id) = queue {
            self.seq.queue.push_back(id);
        }
        if let Some(r) = play {
            let next = self.seq.play_view(media.to_vec(), Source::Web(tab), r);
            self.start_track(next);
        }
    }

    // ------------------------------------------------------------------ service pane

    pub fn service_pane(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, theme::PANE_BG());
        let art_h = if self.show_art { full.width() + 20.0 } else { 0.0 };
        let list_rect = Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - art_h));
        let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
        egui::ScrollArea::vertical().id_salt("svc").auto_shrink(false).show(&mut list_ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            self.node_header(ui, "Library", None);
            let n = self.lib.tracks.len();
            let sel = self.source == Source::Library;
            let r = self.node(ui, "Music", &group_thousands(n), sel, NodeIcon::Library);
            if r.clicked() {
                self.select_source(Source::Library);
            }
            ui.add_space(6.0);
            if self.node_header(ui, "Playlists", Some("Create Playlist")) {
                let id = self.new_playlist(vec![]);
                self.select_source(Source::Playlist(id));
            }
            let pls: Vec<(u64, String, bool)> = self
                .lib
                .playlists
                .iter()
                .map(|p| (p.id, p.name.clone(), matches!(p.kind, PlaylistKind::Smart(_))))
                .collect();
            for (pid, name, smart) in pls {
                let sel = self.source == Source::Playlist(pid);
                let icon = if smart { NodeIcon::Smart } else { NodeIcon::Playlist };
                let r = self.node(ui, &name, "", sel, icon);
                if r.clicked() {
                    self.select_source(Source::Playlist(pid));
                }
                if r.double_clicked() {
                    self.play_playlist(pid);
                }
                if !smart {
                    if let Some(ids) = r.dnd_release_payload::<Vec<u64>>() {
                        self.add_to_playlist(pid, &ids);
                    }
                    if r.dnd_hover_payload::<Vec<u64>>().is_some() {
                        ui.painter().rect_stroke(
                            r.rect.shrink(1.0),
                            2.0,
                            Stroke::new(2.0, theme::ACCENT()),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                r.context_menu(|ui| {
                    if ui.button("Play").clicked() {
                        self.play_playlist(pid);
                    }
                    if ui.button("Rename").clicked() {
                        self.renaming = Some((pid, name.clone()));
                    }
                    if smart && ui.button("Edit Smart Playlist…").clicked() {
                        self.open_smart_editor(Some(pid));
                    }
                    if ui.button("Export…").clicked() {
                        self.export_playlist(pid);
                    }
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        self.lib.playlists.retain(|p| p.id != pid);
                        self.lib.dirty = true;
                        if self.source == Source::Playlist(pid) {
                            self.select_source(Source::Library);
                        }
                    }
                });
            }
            ui.add_space(6.0);
            self.bookmark_nodes(ui);
        });
        if self.show_art {
            let area = Rect::from_min_max(pos2(full.min.x, full.max.y - art_h), full.max);
            let header = Rect::from_min_size(area.min, vec2(area.width(), 20.0));
            theme::display_header(ui.painter(), header);
            ui.painter().text(
                header.left_center() + vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                "Album Art",
                FontId::proportional(11.0),
                theme::TEXT(),
            );
            let body = Rect::from_min_max(pos2(area.min.x, header.max.y), area.max);
            ui.painter().rect_filled(body, 0.0, theme::DISPLAY_EMPTY());
            let img = body.shrink(8.0);
            let side = img.width().min(img.height());
            let img = Rect::from_center_size(img.center(), vec2(side, side));
            match self.art.as_ref().and_then(|a| a.1.as_ref()) {
                Some(tex) => {
                    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                    ui.painter().image(tex.id(), img, uv, Color32::WHITE);
                }
                None => theme::placeholder_cover(ui.painter(), img),
            }
        }
    }

    /// The Bookmarks section: click opens the page, right-click for more.
    fn bookmark_nodes(&mut self, ui: &mut Ui) {
        if self.node_header(ui, "Bookmarks", Some("Bookmark This Page (⌘D)")) {
            self.toggle_bookmark();
        }
        if self.lib.bookmarks.is_empty() {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
            ui.painter().text(
                rect.left_top() + vec2(12.0, 6.0),
                Align2::LEFT_TOP,
                "Open a web page and press ⌘D\nor the star by the address bar",
                FontId::proportional(10.0),
                theme::TEXT_DIM(),
            );
            return;
        }
        let front = self.current_page().map(|(u, _)| u);
        let list: Vec<(u64, String, String)> =
            self.lib.bookmarks.iter().map(|b| (b.id, b.name.clone(), b.url.clone())).collect();
        let mut remove = None;
        let mut moved: Option<(usize, usize)> = None;
        let n = list.len();
        for (i, (id, name, url)) in list.into_iter().enumerate() {
            let sel = self.active_tab.is_some() && front.as_deref() == Some(url.as_str());
            let r = self.node(ui, &name, "", sel, NodeIcon::Bookmark).on_hover_text(&url);
            if r.clicked() {
                self.open_bookmark(&url, false);
            }
            if r.middle_clicked() {
                self.open_bookmark(&url, true);
            }
            r.context_menu(|ui| {
                if ui.button("Open").clicked() {
                    self.open_bookmark(&url, false);
                }
                if ui.button("Open in New Tab").clicked() {
                    self.open_bookmark(&url, true);
                }
                ui.separator();
                if ui.button("Rename").clicked() {
                    self.renaming_bookmark = Some((id, name.clone()));
                }
                if ui.button("Copy Address").clicked() {
                    ui.ctx().copy_text(url.clone());
                }
                if ui.add_enabled(i > 0, egui::Button::new("Move Up")).clicked() {
                    moved = Some((i, i - 1));
                }
                if ui.add_enabled(i + 1 < n, egui::Button::new("Move Down")).clicked() {
                    moved = Some((i, i + 1));
                }
                ui.separator();
                if ui.button("Delete").clicked() {
                    remove = Some(id);
                }
            });
        }
        if let Some(id) = remove {
            self.lib.bookmarks.retain(|b| b.id != id);
            self.lib.dirty = true;
        }
        if let Some((a, b)) = moved {
            self.lib.bookmarks.swap(a, b);
            self.lib.dirty = true;
        }
    }

    fn play_playlist(&mut self, pid: u64) {
        self.select_source(Source::Playlist(pid));
        self.rebuild_view();
        if !self.view.is_empty() {
            self.play_view_at(0);
        }
    }

    /// Container node ("Library", "Playlists"). Returns true when its "+" was clicked.
    fn node_header(&mut self, ui: &mut Ui, label: &str, plus: Option<&str>) -> bool {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        theme::container_node(ui.painter(), rect);
        ui.painter().text(
            rect.left_center() + vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            label.to_uppercase(),
            FontId::proportional(10.0),
            gray(0x60),
        );
        if let Some(tip) = plus {
            let b = Rect::from_center_size(rect.right_center() - vec2(14.0, 0.0), vec2(16.0, 16.0));
            let r = ui.interact(b, ui.scope_id().with(label), Sense::click()).on_hover_text(tip);
            let col = if r.hovered() { theme::ACCENT() } else { gray(0x70) };
            ui.painter().text(b.center(), Align2::CENTER_CENTER, "+", FontId::proportional(15.0), col);
            return r.clicked();
        }
        false
    }

    fn node(&mut self, ui: &mut Ui, label: &str, badge: &str, selected: bool, icon: NodeIcon) -> egui::Response {
        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
        let p = ui.painter();
        if selected {
            theme::selected_node(p, rect);
        } else if r.hovered() {
            p.rect_filled(rect, 0.0, gray(0xdf));
        }
        let ic = rect.left_center() + vec2(18.0, 0.0);
        let col = gray(0x5a);
        match icon {
            NodeIcon::Library => {
                p.rect_stroke(
                    Rect::from_center_size(ic, vec2(11.0, 9.0)),
                    1.0,
                    Stroke::new(1.3, col),
                    egui::StrokeKind::Middle,
                );
                p.hline(ic.x - 3.0..=ic.x + 3.0, ic.y, Stroke::new(1.0, col));
            }
            NodeIcon::Playlist => {
                for k in 0..3 {
                    p.hline(ic.x - 5.0..=ic.x + 5.0, ic.y - 3.0 + k as f32 * 3.0, Stroke::new(1.3, col));
                }
            }
            NodeIcon::Smart => {
                p.circle_stroke(ic, 4.5, Stroke::new(1.3, col));
                p.circle_filled(ic, 1.6, col);
            }
            NodeIcon::Bookmark => theme::star(p, ic, 5.5, col),
        }
        let text_col = if selected { theme::TEXT() } else { gray(0x30) };
        let avail = rect.width() - 30.0 - if badge.is_empty() { 6.0 } else { 40.0 };
        p.text(
            rect.left_center() + vec2(30.0, 0.0),
            Align2::LEFT_CENTER,
            ellipsize(label, avail, 11.5, ui),
            FontId::proportional(11.5),
            text_col,
        );
        if !badge.is_empty() {
            p.text(
                rect.right_center() - vec2(8.0, 0.0),
                Align2::RIGHT_CENTER,
                badge,
                FontId::proportional(10.0),
                theme::TEXT_DIM(),
            );
        }
        r
    }

    // ------------------------------------------------------------------ queue pane

    pub fn queue_pane(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, theme::ROW_A());
        ui.painter().vline(full.left(), full.y_range(), Stroke::new(1.0, gray(0x90)));
        let header = Rect::from_min_size(full.min, vec2(full.width(), 20.0));
        theme::display_header(ui.painter(), header);
        ui.painter().text(
            header.left_center() + vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            "Play Queue",
            FontId::proportional(11.0),
            theme::TEXT(),
        );
        if !self.seq.queue.is_empty() {
            let clear = Rect::from_center_size(header.right_center() - vec2(24.0, 0.0), vec2(40.0, 16.0));
            let r = ui.interact(clear, ui.scope_id().with("qclear"), Sense::click());
            let col = if r.hovered() { theme::ACCENT() } else { gray(0x50) };
            ui.painter().text(clear.center(), Align2::CENTER_CENTER, "Clear", FontId::proportional(10.0), col);
            if r.clicked() {
                self.seq.queue.clear();
            }
        }
        let body = Rect::from_min_max(pos2(full.min.x + 1.0, header.max.y), full.max);
        let drop = ui.interact(body, ui.scope_id().with("qdrop"), Sense::hover());
        if let Some(ids) = drop.dnd_release_payload::<Vec<u64>>() {
            self.seq.queue.extend(ids.iter().copied());
        }
        if drop.dnd_hover_payload::<Vec<u64>>().is_some() {
            ui.painter().rect_stroke(body.shrink(2.0), 2.0, Stroke::new(2.0, theme::ACCENT()), egui::StrokeKind::Inside);
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        if self.seq.queue.is_empty() {
            child.painter().text(
                body.center_top() + vec2(0.0, 40.0),
                Align2::CENTER_CENTER,
                "Drag tracks here to queue them",
                FontId::proportional(11.0),
                theme::TEXT_DIM(),
            );
        }
        let items: Vec<u64> = self.seq.queue.iter().copied().collect();
        let mut remove = None;
        let mut play = None;
        egui::ScrollArea::vertical().id_salt("queue").auto_shrink(false).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, id) in items.iter().enumerate() {
                let Some(t) = self.lib.track(*id) else { continue };
                let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                let p = ui.painter();
                p.rect_filled(rect, 0.0, if i % 2 == 0 { theme::ROW_A() } else { theme::ROW_B() });
                let w = rect.width() - 34.0;
                p.text(
                    rect.left_top() + vec2(8.0, 4.0),
                    Align2::LEFT_TOP,
                    ellipsize(&t.title, w, 11.0, ui),
                    FontId::proportional(11.0),
                    theme::TEXT(),
                );
                p.text(
                    rect.left_top() + vec2(8.0, 18.0),
                    Align2::LEFT_TOP,
                    ellipsize(&t.artist, w, 10.0, ui),
                    FontId::proportional(10.0),
                    theme::TEXT_DIM(),
                );
                let x = Rect::from_center_size(rect.right_center() - vec2(12.0, 0.0), vec2(16.0, 16.0));
                let xr = ui.interact(x, ui.scope_id().with(("qx", i)), Sense::click());
                if r.hovered() || xr.hovered() {
                    let col = if xr.hovered() { theme::ACCENT() } else { gray(0x80) };
                    theme::cross(ui.painter(), x.center(), 3.5, col);
                }
                if xr.clicked() {
                    remove = Some(i);
                } else if r.double_clicked() {
                    play = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            self.seq.queue.remove(i);
        }
        if let Some(i) = play {
            if let Some(id) = self.seq.queue.remove(i) {
                self.seq.playing_from_queue = Some(id);
                self.start_track(Some(id));
            }
        }
    }

    // ------------------------------------------------------------------ media page

    pub fn filter_lists(&mut self, ui: &mut Ui, rect: Rect) {
        let w = rect.width() / 3.0;
        for i in 0..3 {
            let r = Rect::from_min_size(rect.min + vec2(w * i as f32, 0.0), vec2(w, rect.height()));
            let header = Rect::from_min_size(r.min, vec2(r.width(), theme::HEADER_H));
            theme::table_header(ui.painter(), header);
            ui.painter().text(
                header.left_center() + vec2(5.0, 0.0),
                Align2::LEFT_CENTER,
                FILTER_NAMES[i],
                FontId::proportional(9.5),
                theme::TEXT(),
            );
            let body = Rect::from_min_max(pos2(r.min.x, header.max.y), r.max);
            stripes(ui.painter(), body);
            let values = self.filter_values(i);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
            let mut clicked: Option<Option<String>> = None;
            egui::ScrollArea::vertical().id_salt(("filter", i)).auto_shrink(false).show_rows(
                &mut child,
                theme::ROW_H,
                values.len() + 1,
                |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row in range {
                        let (rr, resp) =
                            ui.allocate_exact_size(vec2(ui.available_width(), theme::ROW_H), Sense::click());
                        let (label, value) = if row == 0 {
                            (format!("All ({} {})", values.len(), FILTER_PLURALS[i]), None)
                        } else {
                            let v = values[row - 1].clone();
                            let label = if v.is_empty() { format!("Unknown {}", FILTER_NAMES[i]) } else { v.clone() };
                            (label, Some(v))
                        };
                        let selected = self.filters[i] == value;
                        let bg = if selected {
                            theme::p().selected
                        } else if row % 2 == 0 {
                            theme::ROW_A()
                        } else {
                            theme::ROW_B()
                        };
                        ui.painter().rect_filled(rr, 0.0, bg);
                        let col = theme::TEXT();
                        ui.painter().text(
                            rr.left_center() + vec2(6.0, 0.0),
                            Align2::LEFT_CENTER,
                            ellipsize(&label, rr.width() - 10.0, 11.0, ui),
                            FontId::proportional(11.0),
                            col,
                        );
                        if resp.clicked() {
                            clicked = Some(value);
                        }
                    }
                },
            );
            if i > 0 {
                ui.painter().vline(r.left(), r.y_range(), Stroke::new(1.0, gray(0xa0)));
            }
            if let Some(v) = clicked {
                self.filters[i] = v;
                for j in i + 1..3 {
                    self.filters[j] = None;
                }
                self.view_dirty = true;
            }
        }
    }

    pub fn track_list(&mut self, ui: &mut Ui, rect: Rect) {
        // widths: Songbird's spec, scaled to fill (sbColumnSpecParser.reduceWidthsProportionally)
        let total: f32 = COLUMNS.iter().map(|c| c.2).sum();
        let scale = rect.width() / total;
        let widths: Vec<f32> = COLUMNS.iter().map(|c| c.2 * scale).collect();

        let header = Rect::from_min_size(rect.min, vec2(rect.width(), theme::HEADER_H));
        theme::table_header(ui.painter(), header);
        let mut x = rect.left();
        for (ci, (col, name, _)) in COLUMNS.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(x, header.top()), vec2(widths[ci], header.height()));
            let r = ui.interact(cell, ui.scope_id().with(("col", ci)), Sense::click());
            if r.hovered() {
                ui.painter().rect_filled(cell, 0.0, Color32::from_white_alpha(60));
            }
            ui.painter().text(
                cell.left_center() + vec2(5.0, 0.0),
                Align2::LEFT_CENTER,
                *name,
                FontId::proportional(9.5),
                theme::TEXT(),
            );
            if let Some((sc, asc)) = self.sort {
                if sc == *col {
                    let c = cell.right_center() - vec2(9.0, 0.0);
                    let tri = if asc {
                        vec![c + vec2(-3.5, 2.0), c + vec2(3.5, 2.0), c + vec2(0.0, -2.5)]
                    } else {
                        vec![c + vec2(-3.5, -2.0), c + vec2(3.5, -2.0), c + vec2(0.0, 2.5)]
                    };
                    ui.painter().add(egui::Shape::convex_polygon(tri, gray(0x50), Stroke::NONE));
                }
            }
            ui.painter().vline(cell.right(), cell.y_range(), Stroke::new(1.0, gray(0xa8)));
            if r.clicked() {
                self.sort = match self.sort {
                    Some((c, asc)) if c == *col => Some((*col, !asc)),
                    _ => Some((*col, true)),
                };
                self.view_dirty = true;
            }
            x += widths[ci];
        }

        let body = Rect::from_min_max(pos2(rect.min.x, header.max.y), rect.max);
        stripes(ui.painter(), body);
        if self.view_dirty {
            self.rebuild_view();
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        if self.view.is_empty() {
            self.empty_state(&mut child, body);
            return;
        }
        let current = self.seq.current();
        let playing = self.state != State::Stopped;
        let n = self.view.len();
        let mut actions: Vec<RowAction> = vec![];
        let mut sa = egui::ScrollArea::vertical()
            .id_salt(("tracks", format!("{:?}", self.source)))
            .auto_shrink(false);
        if self.scroll_to_current {
            self.scroll_to_current = false;
            if let Some(i) = current.and_then(|c| self.view.iter().position(|v| *v == c)) {
                let off = (i as f32 * theme::ROW_H - body.height() * 0.4).max(0.0);
                sa = sa.vertical_scroll_offset(off);
            }
        }
        sa.show_rows(&mut child, theme::ROW_H, n, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in range {
                let id = self.view[row];
                let Some(t) = self.lib.track(id) else { continue };
                let (rr, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), theme::ROW_H), Sense::click_and_drag());
                let selected = self.selection.contains(&id);
                let is_cur = playing && current == Some(id);
                let p = ui.painter();
                let bg = if selected {
                    theme::SELECT()
                } else if row % 2 == 0 {
                    theme::ROW_A()
                } else {
                    theme::ROW_B()
                };
                p.rect_filled(rr, 0.0, bg);
                let tc = if selected { Color32::WHITE } else { theme::TEXT() };
                let mut x = rr.left();
                for (ci, (col, _, _)) in COLUMNS.iter().enumerate() {
                    let cell = Rect::from_min_size(pos2(x, rr.top()), vec2(widths[ci], rr.height()));
                    let pad = 5.0;
                    if *col == Col::Rating {
                        for s in 0..5 {
                            let c = pos2(cell.left() + pad + 6.0 + s as f32 * 13.0, cell.center().y);
                            let fill = if (s as u8) < t.rating {
                                if selected { Color32::WHITE } else { theme::STAR() }
                            } else if selected {
                                Color32::from_white_alpha(70)
                            } else {
                                theme::STAR_EMPTY()
                            };
                            theme::star(p, c, 5.5, fill);
                        }
                    } else {
                        let mut text_x = cell.left() + pad;
                        if *col == Col::Title && is_cur {
                            let c = pos2(text_x + 4.0, cell.center().y);
                            let ic = if selected { Color32::WHITE } else { theme::ACCENT() };
                            if self.state == State::Paused {
                                theme::pause_bars(p, c, 7.0, ic);
                            } else {
                                theme::speaker(p, c + vec2(2.0, 0.0), 9.0, ic, 1);
                            }
                            text_x += 14.0;
                        }
                        let s = cell_text(t, *col);
                        let avail = cell.right() - text_x - pad;
                        let font = FontId::proportional(11.0);
                        if *col == Col::Time {
                            p.text(pos2(cell.right() - pad, cell.center().y), Align2::RIGHT_CENTER, s, font, tc);
                        } else {
                            p.text(
                                pos2(text_x, cell.center().y),
                                Align2::LEFT_CENTER,
                                ellipsize(&s, avail, 11.0, ui),
                                font,
                                tc,
                            );
                        }
                    }
                    x += widths[ci];
                }
                let rating_x0 = rr.left() + widths[..5].iter().sum::<f32>();
                if resp.clicked() {
                    let mut rated = false;
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if pos.x >= rating_x0 && pos.x < rating_x0 + widths[5] {
                            let star = ((pos.x - rating_x0 - 5.0) / 13.0).floor() as i32 + 1;
                            if (1..=5).contains(&star) {
                                actions.push(RowAction::Rate(id, star as u8));
                                rated = true;
                            }
                        }
                    }
                    if !rated {
                        let m = ui.input(|i| i.modifiers);
                        actions.push(RowAction::Click(row, m));
                    }
                }
                if resp.double_clicked() {
                    actions.push(RowAction::Play(row));
                }
                if resp.drag_started() && !self.selection.contains(&id) {
                    actions.push(RowAction::Click(row, Modifiers::NONE));
                }
                if resp.dragged() {
                    let ids: Vec<u64> =
                        if self.selection.contains(&id) { self.selected_in_order() } else { vec![id] };
                    let count = ids.len();
                    resp.dnd_set_drag_payload(ids);
                    if let Some(pos) = ui.ctx().pointer_interact_pos() {
                        let label = if count == 1 { t.title.clone() } else { format!("{count} tracks") };
                        let painter = ui
                            .ctx()
                            .layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("dragtip")));
                        let r = Rect::from_min_size(pos + vec2(12.0, 4.0), vec2(170.0, 18.0));
                        painter.rect_filled(r, 3.0, Color32::from_black_alpha(180));
                        painter.text(
                            r.left_center() + vec2(6.0, 0.0),
                            Align2::LEFT_CENTER,
                            ellipsize(&label, 158.0, 11.0, ui),
                            FontId::proportional(11.0),
                            Color32::WHITE,
                        );
                    }
                }
                resp.context_menu(|ui| {
                    if !self.selection.contains(&id) {
                        self.selection.clear();
                        self.selection.insert(id);
                        self.anchor = Some(row);
                    }
                    self.row_menu(ui, row);
                });
            }
        });
        for a in actions {
            match a {
                RowAction::Click(row, m) => {
                    let id = self.view[row];
                    if m.shift {
                        let a = self.anchor.unwrap_or(row);
                        let (lo, hi) = (a.min(row), a.max(row));
                        if !m.command {
                            self.selection.clear();
                        }
                        for r in lo..=hi {
                            self.selection.insert(self.view[r]);
                        }
                    } else if m.command {
                        if !self.selection.remove(&id) {
                            self.selection.insert(id);
                        }
                        self.anchor = Some(row);
                    } else {
                        self.selection.clear();
                        self.selection.insert(id);
                        self.anchor = Some(row);
                    }
                }
                RowAction::Play(row) => self.play_view_at(row),
                RowAction::Rate(id, stars) => {
                    if let Some(t) = self.lib.track_mut(id) {
                        // clicking the current rating again clears it (estimated)
                        t.rating = if t.rating == stars { 0 } else { stars };
                        self.lib.dirty = true;
                        if matches!(self.source, Source::Playlist(_)) {
                            self.view_dirty = true;
                        }
                    }
                }
            }
        }
    }

    fn row_menu(&mut self, ui: &mut Ui, row: usize) {
        let ids = self.selected_in_order();
        if ui.button("Play").clicked() {
            self.play_view_at(row);
        }
        if ui.button("Play Next").clicked() {
            for id in ids.iter().rev() {
                self.seq.queue.push_front(*id);
            }
        }
        if ui.button("Add to Play Queue").clicked() {
            self.seq.queue.extend(ids.iter().copied());
        }
        ui.menu_button("Add to Playlist", |ui| {
            if ui.button("New Playlist…").clicked() {
                self.new_playlist(ids.clone());
            }
            ui.separator();
            let manual: Vec<(u64, String)> = self
                .lib
                .playlists
                .iter()
                .filter(|p| matches!(p.kind, PlaylistKind::Manual(_)))
                .map(|p| (p.id, p.name.clone()))
                .collect();
            for (pid, name) in manual {
                if ui.button(name).clicked() {
                    self.add_to_playlist(pid, &ids);
                }
            }
        });
        ui.menu_button("Rating", |ui| {
            for stars in (0..=5u8).rev() {
                let label = if stars == 0 { "None".to_string() } else { "★".repeat(stars as usize) };
                if ui.button(label).clicked() {
                    for id in &ids {
                        if let Some(t) = self.lib.track_mut(*id) {
                            t.rating = stars;
                        }
                    }
                    self.lib.dirty = true;
                    self.view_dirty = true;
                }
            }
        });
        ui.separator();
        if ids.len() == 1 {
            if let Some(t) = self.lib.track(ids[0]) {
                if !t.demo && ui.button("Show in Finder").clicked() {
                    let _ = std::process::Command::new("open").arg("-R").arg(&t.path).spawn();
                }
            }
        }
        let editable = match self.source {
            Source::Library => true,
            Source::Web(_) => false,
            Source::Playlist(p) => matches!(self.lib.playlist(p).map(|p| &p.kind), Some(PlaylistKind::Manual(_))),
        };
        let label = if self.source == Source::Library { "Remove from Library" } else { "Remove from Playlist" };
        if ui.add_enabled(editable, egui::Button::new(label)).clicked() {
            self.remove_selected();
        }
    }

    fn empty_state(&mut self, ui: &mut Ui, body: Rect) {
        let p = ui.painter().clone();
        stripes(&p, body);
        if !self.lib.tracks.is_empty() {
            let msg = if self.search.is_empty() { "No items" } else { "Nothing matches your search" };
            p.text(body.center_top() + vec2(0.0, 60.0), Align2::CENTER_CENTER, msg, FontId::proportional(13.0), theme::TEXT_DIM());
            return;
        }
        let card = Rect::from_center_size(body.center() - vec2(0.0, 30.0), vec2(340.0, 130.0));
        let mut cu = ui.new_child(
            egui::UiBuilder::new().max_rect(card).layout(egui::Layout::top_down(egui::Align::Center)),
        );
        cu.label(egui::RichText::new("Your library is empty").size(16.0).color(theme::TEXT()));
        cu.add_space(6.0);
        cu.label(egui::RichText::new("To Import Media, Choose a Folder").size(11.5).color(theme::TEXT_DIM()));
        cu.add_space(10.0);
        if cu.button("Choose a Folder…").clicked() {
            let ctx = cu.ctx().clone();
            self.menu_import_folder(&ctx);
        }
        cu.add_space(4.0);
        cu.label(
            egui::RichText::new("…or drop music files and folders onto this window")
                .size(10.5)
                .color(theme::TEXT_DIM()),
        );
    }

    pub fn status_bar(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        theme::status_bar(ui.painter(), rect);
        let flash = match &self.message {
            Some((m, at)) if at.elapsed() < Duration::from_secs(6) => Some(m.clone()),
            _ => None,
        };
        let page = self.active_tab.and_then(|id| self.web.tab(id)).map(|t| {
            if t.loading { format!("Loading {}…", t.url) } else { "Done".to_string() }
        });
        let text = flash.or_else(|| self.audio.error.clone()).or(page).unwrap_or_else(|| {
            let n = self.view.len();
            let secs: u64 = self.view.iter().filter_map(|id| self.lib.track(*id)).map(|t| t.duration_ms / 1000).sum();
            let items = match n {
                0 => "no items".to_string(),
                1 => "1 item".to_string(),
                _ => format!("{} items", group_thousands(n)),
            };
            let sel = self.selection.len();
            if sel > 1 {
                format!("{sel} of {items} selected")
            } else {
                format!("{items}, {}", player::fmt_single(secs))
            }
        });
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(10.5), gray(0x50));
    }

    // ------------------------------------------------------------------ control pane

    pub fn control_pane(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        theme::control_pane(ui.painter(), rect);
        let cy = rect.center().y;

        // mouse wheel over the control pane changes the volume by 0.03 a notch
        if ui.rect_contains_pointer(rect) {
            let dy: f32 = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                        _ => None,
                    })
                    .sum()
            });
            if dy.abs() > 0.01 {
                self.set_volume(self.lib.settings.volume + if dy > 0.0 { 0.03 } else { -0.03 });
            }
        }

        // back / play-pause / forward
        let back_c = pos2(rect.left() + 26.0, cy);
        let play_c = pos2(rect.left() + 62.0, cy);
        let fwd_c = pos2(rect.left() + 98.0, cy);
        let back = self.skip_button(ui, back_c, -1);
        let fwd = self.skip_button(ui, fwd_c, 1);
        let pr = ui.interact(Rect::from_center_size(play_c, vec2(36.0, 36.0)), ui.scope_id().with("play"), Sense::click());
        let icon = theme::primary_button(ui.painter(), play_c, 18.0, pr.hovered(), pr.is_pointer_button_down_on());
        if self.state == State::Playing {
            theme::pause_bars(ui.painter(), play_c, 13.0, icon);
        } else {
            theme::triangle_right(ui.painter(), play_c + vec2(1.5, 0.0), 14.0, icon);
        }
        let pr = pr.on_hover_text(if self.state == State::Playing { "Pause" } else { "Play" });
        if pr.clicked() {
            self.play_pause();
        }
        pr.context_menu(|ui| {
            if ui.add_enabled(self.state != State::Playing, egui::Button::new("Play")).clicked() {
                self.play_pause();
            }
            if ui.add_enabled(self.state == State::Playing, egui::Button::new("Pause")).clicked() {
                self.play_pause();
            }
            if ui.add_enabled(self.state != State::Stopped, egui::Button::new("Stop")).clicked() {
                self.stop();
            }
        });
        if back {
            self.previous();
        }
        if fwd {
            self.next(true);
        }

        // volume: min button (mute), slider, max button
        let vx = rect.left() + 124.0;
        let min_r = Rect::from_center_size(pos2(vx + 8.0, cy), vec2(16.0, 16.0));
        let mr = ui.interact(min_r, ui.scope_id().with("volmin"), Sense::click()).on_hover_text("Mute");
        let vcol = if self.lib.settings.muted { theme::TEXT_DIM() } else { theme::TEXT() };
        theme::speaker(ui.painter(), min_r.center(), 11.0, vcol, 0);
        if self.lib.settings.muted {
            ui.painter().line_segment(
                [min_r.left_top() + vec2(2.0, 2.0), min_r.right_bottom() - vec2(2.0, 2.0)],
                Stroke::new(1.5, Color32::from_rgb(200, 60, 60)),
            );
        }
        if mr.clicked() {
            self.toggle_mute();
        }
        let track = Rect::from_min_size(pos2(vx + 20.0, cy - 3.0), vec2(76.0, 6.0));
        let vr = ui
            .interact(track.expand2(vec2(4.0, 6.0)), ui.scope_id().with("vol"), Sense::click_and_drag())
            .on_hover_text("Volume");
        if vr.dragged() || vr.clicked() {
            if let Some(p) = vr.interact_pointer_pos() {
                self.set_volume(((p.x - track.left()) / track.width()).clamp(0.0, 1.0));
            }
        }
        let v = self.volume();
        let thin = Rect::from_center_size(track.center(), vec2(track.width(), 4.0));
        ui.painter().rect_filled(thin, 2.0, theme::p().line);
        ui.painter().rect_filled(Rect::from_min_size(thin.min, vec2(thin.width() * v, thin.height())), 2.0, theme::ACCENT());
        let thumb = pos2(track.left() + track.width() * v, cy);
        ui.painter().circle_filled(thumb, 6.0, theme::TEXT());
        ui.painter().circle_stroke(thumb, 6.0, Stroke::new(1.0, theme::p().panel));
        let max_r = Rect::from_center_size(pos2(track.right() + 14.0, cy), vec2(16.0, 16.0));
        let xr = ui.interact(max_r, ui.scope_id().with("volmax"), Sense::click()).on_hover_text("Maximum volume");
        theme::speaker(ui.painter(), max_r.center(), 11.0, theme::TEXT(), 2);
        if xr.clicked() {
            self.set_volume(1.0);
        }

        // shuffle / repeat on the right
        let sh_c = pos2(rect.right() - 54.0, cy);
        let rp_c = pos2(rect.right() - 24.0, cy);
        let on_col = theme::ACCENT();
        let sr = ui
            .interact(Rect::from_center_size(sh_c, vec2(26.0, 26.0)), ui.scope_id().with("shuffle"), Sense::click())
            .on_hover_text("Shuffle");
        let ic = theme::round_button(ui.painter(), sh_c, 12.0, sr.hovered(), sr.is_pointer_button_down_on(), true);
        theme::shuffle_icon(ui.painter(), sh_c, 12.0, if self.seq.shuffle { on_col } else { ic });
        if sr.clicked() {
            let on = !self.seq.shuffle;
            self.seq.set_shuffle(on);
            self.lib.dirty = true;
        }
        let tip = match self.seq.repeat {
            1 => "Repeat One",
            2 => "Repeat All",
            _ => "Repeat Off",
        };
        let rr = ui
            .interact(Rect::from_center_size(rp_c, vec2(26.0, 26.0)), ui.scope_id().with("repeat"), Sense::click())
            .on_hover_text(tip);
        let ic = theme::round_button(ui.painter(), rp_c, 12.0, rr.hovered(), rr.is_pointer_button_down_on(), true);
        theme::repeat_icon(ui.painter(), rp_c, 12.0, if self.seq.repeat != 0 { on_col } else { ic }, self.seq.repeat == 1);
        if rr.clicked() {
            self.seq.cycle_repeat();
            self.lib.dirty = true;
        }

        // faceplate between the volume and shuffle
        let face = Rect::from_min_max(pos2(max_r.right() + 14.0, cy - 22.0), pos2(sh_c.x - 24.0, cy + 22.0));
        if face.width() > 120.0 {
            self.faceplate(ui, face);
        }
    }

    /// Back/Forward with hold-to-seek. Returns true when it should change track.
    fn skip_button(&mut self, ui: &mut Ui, c: Pos2, dir: i64) -> bool {
        let enabled = if dir < 0 { self.seq.has_previous() } else { self.seq.has_next() };
        let r = ui.interact(Rect::from_center_size(c, vec2(28.0, 28.0)), ui.scope_id().with(("skip", dir)), Sense::click());
        let r = r.on_hover_text(if dir < 0 { "Previous" } else { "Next" });
        let down = r.is_pointer_button_down_on();
        let icon = theme::round_button(ui.painter(), c, 12.5, r.hovered(), down && enabled, enabled);
        let p = ui.painter();
        if dir < 0 {
            theme::triangle_left(p, c + vec2(-3.0, 0.0), 8.0, icon);
            theme::triangle_left(p, c + vec2(3.5, 0.0), 8.0, icon);
        } else {
            theme::triangle_right(p, c + vec2(-3.5, 0.0), 8.0, icon);
            theme::triangle_right(p, c + vec2(3.0, 0.0), 8.0, icon);
        }
        if !enabled {
            return false;
        }
        if down && self.state == State::Playing && self.skip.as_ref().map(|s| s.dir) != Some(dir) {
            let now = Instant::now();
            self.skip = Some(SkipHold { dir, started: now, last_tick: now, seen_skip: false });
        }
        if r.clicked() {
            let seen = self.skip.take().map(|s| s.seen_skip).unwrap_or(false);
            return !seen;
        }
        if !down && self.skip.as_ref().map(|s| s.dir) == Some(dir) {
            self.skip = None;
        }
        false
    }

    fn faceplate(&mut self, ui: &mut Ui, face: Rect) {
        let p = ui.painter().clone();
        p.rect_filled(face, 10.0, theme::p().bg);
        p.rect_stroke(face, 10.0, Stroke::new(1.0, theme::p().line), egui::StrokeKind::Inside);
        let inner = face.shrink2(vec2(12.0, 5.0));
        let cur = self.seq.current().and_then(|id| self.lib.track(id).cloned());
        let Some(t) = cur.filter(|_| self.seen_playing) else {
            // inactive faceplate (faceplate.seenplaying = false)
            p.text(face.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER, "♪", FontId::proportional(14.0), FACE_NOTE);
            p.text(
                face.center() + vec2(0.0, 9.0),
                Align2::CENTER_CENTER,
                "Double-click a song to play it",
                FontId::proportional(10.5),
                theme::FACE_DIM(),
            );
            return;
        };

        // line 1: title • artist • album, scrolling when too wide; rating on the right
        let stars_w = 5.0 * 11.0 + 6.0;
        let line1 = Rect::from_min_max(inner.min, pos2(inner.right() - stars_w, inner.min.y + 16.0));
        let mut job = egui::text::LayoutJob::default();
        let fmt = |c: Color32| egui::TextFormat { font_id: FontId::proportional(11.5), color: c, ..Default::default() };
        job.append(&t.title, 0.0, fmt(theme::FACE_TEXT()));
        if !t.artist.is_empty() {
            job.append("  •  ", 0.0, fmt(theme::FACE_DIM()));
            job.append(&t.artist, 0.0, fmt(theme::FACE_TEXT()));
        }
        if !t.album.is_empty() {
            job.append("  •  ", 0.0, fmt(theme::FACE_DIM()));
            job.append(&t.album, 0.0, fmt(theme::FACE_TEXT()));
        }
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let clip = p.with_clip_rect(line1);
        if galley.size().x <= line1.width() {
            clip.galley(line1.left_top(), galley, theme::FACE_TEXT());
        } else {
            // autoscroll back and forth (sb-autoscroll)
            let over = galley.size().x - line1.width();
            if self.state == State::Playing && !self.demo {
                self.marquee += ui.input(|i| i.stable_dt).min(0.1) * 25.0;
            }
            let period = over * 2.0 + 120.0;
            let ph = self.marquee % period;
            let off = if ph < 60.0 {
                0.0
            } else if ph < 60.0 + over {
                ph - 60.0
            } else if ph < 120.0 + over {
                over
            } else {
                over - (ph - 120.0 - over)
            };
            clip.galley(line1.left_top() - vec2(off.clamp(0.0, over), 0.0), galley, theme::FACE_TEXT());
        }
        // now-playing rating
        for s in 0..5 {
            let c = pos2(inner.right() - stars_w + 8.0 + s as f32 * 11.0, line1.center().y);
            let r = ui.interact(Rect::from_center_size(c, vec2(11.0, 14.0)), ui.scope_id().with(("fstar", s)), Sense::click());
            let lit = (s as u8) < t.rating;
            theme::star(&p, c, 4.8, if lit { theme::ACCENT() } else { theme::STAR_EMPTY() });
            if r.clicked() {
                let stars = s as u8 + 1;
                if let Some(tt) = self.lib.track_mut(t.id) {
                    tt.rating = if tt.rating == stars { 0 } else { stars };
                    self.lib.dirty = true;
                    self.view_dirty |= matches!(self.source, Source::Playlist(_));
                }
            }
        }

        // line 2: elapsed | seek bar with 4 notches | total
        let dur = self.cur_duration_ms;
        let live = self.position_ms();
        let pos = self.seek_drag.map(|f| (f * dur as f32) as u64).unwrap_or(live).min(dur.max(1));
        let y2 = inner.bottom() - 8.0;
        let font = FontId::monospace(10.5);
        if self.buffering.is_some() {
            p.text(pos2(inner.left(), y2), Align2::LEFT_CENTER, "Buffering…", FontId::proportional(10.5), theme::FACE_DIM());
            return;
        }
        p.text(pos2(inner.left() + 34.0, y2), Align2::RIGHT_CENTER, player::fmt_time(pos), font.clone(), theme::FACE_TEXT());
        let total_txt = if self.lib.settings.show_remaining && dur > 0 {
            format!("-{}", player::fmt_time(dur.saturating_sub(pos)))
        } else {
            player::fmt_time(dur)
        };
        p.text(pos2(inner.right(), y2), Align2::RIGHT_CENTER, total_txt, font, theme::FACE_TEXT());
        let total_rect = Rect::from_center_size(pos2(inner.right() - 20.0, y2), vec2(44.0, 14.0));
        let tr = ui
            .interact(total_rect, ui.scope_id().with("total"), Sense::click())
            .on_hover_text("Click to show remaining time");
        if tr.clicked() {
            // ignored (reset) while the duration is unknown (onTotalDown)
            self.lib.settings.show_remaining = dur > 0 && !self.lib.settings.show_remaining;
            self.lib.dirty = true;
        }
        let bar = Rect::from_min_max(pos2(inner.left() + 42.0, y2 - 3.0), pos2(inner.right() - 50.0, y2 + 3.0));
        if bar.width() > 20.0 {
            let br = ui.interact(bar.expand2(vec2(4.0, 6.0)), ui.scope_id().with("seek"), Sense::click_and_drag());
            if dur > 0 && (br.dragged() || br.drag_started() || br.clicked()) {
                if let Some(pp) = br.interact_pointer_pos() {
                    self.seek_drag = Some(((pp.x - bar.left()) / bar.width()).clamp(0.0, 1.0));
                }
            }
            if (br.drag_stopped() || br.clicked()) && self.seek_drag.is_some() {
                let f = self.seek_drag.take().unwrap_or(0.0);
                self.seek_ms((f * dur as f32) as u64);
            }
            let thin = Rect::from_center_size(bar.center(), vec2(bar.width(), 4.0));
            p.rect_filled(thin, 2.0, theme::p().line);
            let frac = if dur > 0 { pos as f32 / dur as f32 } else { 0.0 };
            p.rect_filled(Rect::from_min_size(thin.min, vec2(thin.width() * frac, thin.height())), 2.0, theme::ACCENT());
            // Songbird's seek bar had 4 notches: kept as small ticks
            for k in 1..4 {
                let x = bar.left() + bar.width() * k as f32 / 4.0;
                p.vline(x, thin.y_range(), Stroke::new(1.0, theme::p().panel));
            }
            if dur > 0 {
                let th = pos2(bar.left() + bar.width() * frac, bar.center().y);
                p.circle_filled(th, 5.5, theme::TEXT());
            }
        }
    }

    // ------------------------------------------------------------------ dialogs

    pub fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some((bid, mut name)) = self.renaming_bookmark.take() {
            let mut open = true;
            let mut done = false;
            egui::Window::new("Rename Bookmark")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, -60.0))
                .show(ctx, |ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(260.0));
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        done = true;
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            done = true;
                        }
                        if ui.button("Cancel").clicked() {
                            open = false;
                        }
                    });
                });
            if done {
                if let Some(b) = self.lib.bookmarks.iter_mut().find(|b| b.id == bid) {
                    if !name.trim().is_empty() {
                        b.name = name.trim().to_string();
                    }
                }
                self.lib.dirty = true;
            } else if open {
                self.renaming_bookmark = Some((bid, name));
            }
        }

        if let Some((pid, mut name)) = self.renaming.take() {
            let mut open = true;
            let mut done = false;
            egui::Window::new("Rename Playlist")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, -60.0))
                .show(ctx, |ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0));
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        done = true;
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            done = true;
                        }
                        if ui.button("Cancel").clicked() {
                            open = false;
                        }
                    });
                });
            if done {
                if let Some(pl) = self.lib.playlist_mut(pid) {
                    if !name.trim().is_empty() {
                        pl.name = name.trim().to_string();
                    }
                }
                self.lib.dirty = true;
            } else if open {
                self.renaming = Some((pid, name));
            }
        }

        if let Some(ids) = self.confirm_remove.take() {
            let mut keep = true;
            let title = if ids.len() == 1 { "Remove Item" } else { "Remove Items" };
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, -60.0))
                .show(ctx, |ui| {
                    let msg = if ids.len() == 1 {
                        let name = self.lib.track(ids[0]).map(|t| t.title.clone()).unwrap_or_default();
                        format!("Are you sure you want to remove \"{name}\" from your Library?")
                    } else {
                        format!("Are you sure you want to remove {} items from your Library?", ids.len())
                    };
                    ui.label(msg);
                    ui.label(egui::RichText::new("The files stay on your disk.").size(10.5).color(theme::TEXT_DIM()));
                    ui.horizontal(|ui| {
                        if ui.button("Remove").clicked() {
                            let set: HashSet<u64> = ids.iter().copied().collect();
                            if self.seq.current().map(|c| set.contains(&c)).unwrap_or(false) {
                                self.stop();
                            }
                            self.lib.remove_tracks(&set);
                            self.seq.forget(&set);
                            self.selection.clear();
                            self.view_dirty = true;
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            if keep {
                self.confirm_remove = Some(ids);
            }
        }

        if let Some(mut ed) = self.smart.take() {
            let mut keep = true;
            let mut save = false;
            egui::Window::new("Smart Playlist")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(260.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Match");
                        egui::ComboBox::from_id_salt("matchall")
                            .selected_text(if ed.rules.match_all { "all" } else { "any" })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut ed.rules.match_all, true, "all");
                                ui.selectable_value(&mut ed.rules.match_all, false, "any");
                            });
                        ui.label("of the following:");
                    });
                    let mut remove = None;
                    for (i, c) in ed.rules.conditions.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let before = c.field.kind();
                            egui::ComboBox::from_id_salt(("field", i))
                                .width(100.0)
                                .selected_text(c.field.label())
                                .show_ui(ui, |ui| {
                                    for f in Field::ALL {
                                        ui.selectable_value(&mut c.field, f, f.label());
                                    }
                                });
                            if c.field.kind() != before {
                                c.op = Op::for_kind(c.field.kind())[0];
                                c.value.clear();
                            }
                            egui::ComboBox::from_id_salt(("op", i))
                                .width(130.0)
                                .selected_text(c.op.label())
                                .show_ui(ui, |ui| {
                                    for o in Op::for_kind(c.field.kind()) {
                                        ui.selectable_value(&mut c.op, *o, o.label());
                                    }
                                });
                            ui.add(egui::TextEdit::singleline(&mut c.value).desired_width(110.0));
                            if c.field.kind() == FieldKind::Date {
                                ui.label("days");
                            }
                            if ui.small_button("−").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        ed.rules.conditions.remove(i);
                    }
                    if ui.small_button("+ Add condition").clicked() {
                        ed.rules.conditions.push(Condition { field: Field::Title, op: Op::Contains, value: String::new() });
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        let mut limited = ed.rules.limit > 0;
                        if ui.checkbox(&mut limited, "Limit to").changed() {
                            ed.rules.limit = if limited { 25 } else { 0 };
                        }
                        let mut l = ed.rules.limit.max(1) as u32;
                        if ui.add_enabled(limited, egui::DragValue::new(&mut l).range(1..=10000)).changed() {
                            ed.rules.limit = l as usize;
                        }
                        ui.label("items, sorted by");
                        egui::ComboBox::from_id_salt("sortby").selected_text(ed.rules.sort_by.label()).show_ui(ui, |ui| {
                            for f in Field::ALL {
                                ui.selectable_value(&mut ed.rules.sort_by, f, f.label());
                            }
                        });
                        ui.checkbox(&mut ed.rules.descending, "descending");
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            save = true;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            if save {
                let name = if ed.name.trim().is_empty() { "Smart Playlist".to_string() } else { ed.name.trim().to_string() };
                let pid = match ed.id {
                    Some(pid) => {
                        if let Some(pl) = self.lib.playlist_mut(pid) {
                            pl.name = name;
                            pl.kind = PlaylistKind::Smart(ed.rules);
                        }
                        pid
                    }
                    None => {
                        let pid = self.lib.new_id();
                        self.lib.playlists.push(Playlist { id: pid, name, kind: PlaylistKind::Smart(ed.rules) });
                        pid
                    }
                };
                self.lib.dirty = true;
                self.select_source(Source::Playlist(pid));
                self.view_dirty = true;
            } else if keep {
                self.smart = Some(ed);
            }
        }
    }
}
