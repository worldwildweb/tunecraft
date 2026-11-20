//! TuneCraft's look, shared with the other Craft apps (ArtCraft Launcher, PhotoCraft,
//! VectorCraft): it follows the Mac's light/dark setting with dark as the fallback, uses the
//! Inter typeface for text and JetBrains Mono for numbers, flat panels with rounded corners,
//! thin line icons, a blue accent (#4e7bfb, the ArtCraft mark's blue), near-black #0b0b0c and
//! warm off-white #f2f1ee as the extremes, and a 48 pt top bar.

use egui::{Color32, FontData, FontDefinitions, FontFamily, Mesh, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const ROW_H: f32 = 24.0;
pub const HEADER_H: f32 = 24.0;
pub const RADIUS: f32 = 8.0;
pub const TOP_BAR_H: f32 = 48.0;

pub struct Palette {
    /// window background (content area)
    pub bg: Color32,
    /// side panels and bars
    pub panel: Color32,
    /// cards, inputs, list rows
    pub surface: Color32,
    pub surface_alt: Color32,
    pub hover: Color32,
    pub line: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    /// selected row / node background
    pub selected: Color32,
    pub star_empty: Color32,
}

const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x16, 0x16, 0x18),
    panel: Color32::from_rgb(0x1e, 0x1e, 0x21),
    surface: Color32::from_rgb(0x19, 0x19, 0x1c),
    surface_alt: Color32::from_rgb(0x1d, 0x1d, 0x20),
    hover: Color32::from_rgb(0x2a, 0x2a, 0x2f),
    line: Color32::from_rgb(0x30, 0x30, 0x35),
    text: Color32::from_rgb(0xf2, 0xf1, 0xee),
    text_dim: Color32::from_rgb(0x96, 0x94, 0x90),
    accent: Color32::from_rgb(0x4e, 0x7b, 0xfb),
    accent_text: Color32::from_rgb(0xff, 0xff, 0xff),
    selected: Color32::from_rgb(0x26, 0x38, 0x66),
    star_empty: Color32::from_rgb(0x44, 0x44, 0x4a),
};

const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf7, 0xf6, 0xf3),
    panel: Color32::from_rgb(0xec, 0xeb, 0xe7),
    surface: Color32::from_rgb(0xfc, 0xfb, 0xf9),
    surface_alt: Color32::from_rgb(0xf4, 0xf3, 0xf0),
    hover: Color32::from_rgb(0xe2, 0xe1, 0xdc),
    line: Color32::from_rgb(0xd9, 0xd7, 0xd2),
    text: Color32::from_rgb(0x0b, 0x0b, 0x0c),
    text_dim: Color32::from_rgb(0x6c, 0x6b, 0x67),
    accent: Color32::from_rgb(0x4e, 0x7b, 0xfb),
    accent_text: Color32::from_rgb(0xff, 0xff, 0xff),
    selected: Color32::from_rgb(0xd8, 0xe2, 0xfe),
    star_empty: Color32::from_rgb(0xcf, 0xcd, 0xc8),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

pub fn set_dark(dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

pub fn p() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}

// The names the panes use, taken from the current palette.
#[allow(non_snake_case)]
pub fn TEXT() -> Color32 {
    p().text
}
#[allow(non_snake_case)]
pub fn TEXT_DIM() -> Color32 {
    p().text_dim
}
#[allow(non_snake_case)]
pub fn ACCENT() -> Color32 {
    p().accent
}
#[allow(non_snake_case)]
pub fn PANE_BG() -> Color32 {
    p().panel
}
#[allow(non_snake_case)]
pub fn DISPLAY_EMPTY() -> Color32 {
    p().panel
}
#[allow(non_snake_case)]
pub fn ROW_A() -> Color32 {
    p().surface
}
#[allow(non_snake_case)]
pub fn ROW_B() -> Color32 {
    p().surface_alt
}
/// Selected rows: the accent, with white text.
#[allow(non_snake_case)]
pub fn SELECT() -> Color32 {
    p().accent
}
#[allow(non_snake_case)]
pub fn FACE_TEXT() -> Color32 {
    p().text
}
#[allow(non_snake_case)]
pub fn FACE_DIM() -> Color32 {
    p().text_dim
}
#[allow(non_snake_case)]
pub fn STAR() -> Color32 {
    p().text
}
#[allow(non_snake_case)]
pub fn STAR_EMPTY() -> Color32 {
    p().star_empty
}

/// A grey from the earlier light-grey design, mapped into the current palette:
/// 0xff (white) is the background end, 0x00 is the text end, in between blends.
pub fn gray(v: u8) -> Color32 {
    let t = 1.0 - v as f32 / 255.0;
    lerp(p().bg, p().text, t * 0.92)
}

/// Inter for text, JetBrains Mono for numbers (both SIL OFL 1.1, see assets/fonts).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "Inter-Medium", include_bytes!("../assets/fonts/Inter-Medium.ttf"));
    add(&mut fonts, "Inter-SemiBold", include_bytes!("../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "JetBrainsMono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Inter".to_owned());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "JetBrainsMono".to_owned());
    fonts
        .families
        .insert(FontFamily::Name("medium".into()), vec!["Inter-Medium".to_owned(), "Inter".to_owned()]);
    fonts
        .families
        .insert(FontFamily::Name("semibold".into()), vec!["Inter-SemiBold".to_owned(), "Inter".to_owned()]);
    // keep egui's own symbol fonts as fallbacks for arrows and stars
    let fallbacks: Vec<String> = FontDefinitions::default()
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for fam in ["medium", "semibold"] {
        if let Some(list) = fonts.families.get_mut(&FontFamily::Name(fam.into())) {
            list.extend(fallbacks.iter().cloned());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn semibold(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn nav_bar(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, p().panel);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p().line));
}

pub fn control_pane(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, p().panel);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p().line));
}

pub fn table_header(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, p().bg);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p().line));
}

pub fn display_header(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, p().panel);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p().line));
}

/// Section heading in the side panel ("LIBRARY", "PLAYLISTS"…): no box, just space.
pub fn container_node(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, p().panel);
}

pub fn selected_node(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect.shrink2(vec2(6.0, 1.0)), 6.0, p().selected);
}

pub fn status_bar(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, p().panel);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p().line));
}

/// Quiet round button (hover shows a soft circle). Returns the icon colour to use.
pub fn round_button(painter: &Painter, center: Pos2, r: f32, hovered: bool, pressed: bool, enabled: bool) -> Color32 {
    if pressed {
        painter.circle_filled(center, r, p().line);
    } else if hovered && enabled {
        painter.circle_filled(center, r, p().hover);
    }
    if enabled { p().text } else { p().text_dim }
}

/// The main Play/Pause button: a solid accent circle. Returns the icon colour.
pub fn primary_button(painter: &Painter, center: Pos2, r: f32, hovered: bool, pressed: bool) -> Color32 {
    let base = p().accent;
    let fill = if pressed {
        lerp(base, Color32::BLACK, 0.2)
    } else if hovered {
        lerp(base, Color32::WHITE, 0.12)
    } else {
        base
    };
    painter.circle_filled(center, r, fill);
    p().accent_text
}

/// The TuneCraft mark: a blue rounded square with a white note (an original mark).
pub fn logo(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, rect.width() * 0.26, p().accent);
    let c = rect.center();
    let s = rect.width() / 22.0;
    let w = Color32::WHITE;
    painter.circle_filled(c + vec2(-3.2 * s, 4.2 * s), 2.8 * s, w);
    painter.circle_filled(c + vec2(4.4 * s, 2.6 * s), 2.8 * s, w);
    painter.line_segment([c + vec2(-0.6 * s, 4.2 * s), c + vec2(-0.6 * s, -5.6 * s)], Stroke::new(1.8 * s, w));
    painter.line_segment([c + vec2(7.0 * s, 2.6 * s), c + vec2(7.0 * s, -7.0 * s)], Stroke::new(1.8 * s, w));
    painter.line_segment([c + vec2(-0.6 * s, -5.6 * s), c + vec2(7.0 * s, -7.0 * s)], Stroke::new(2.6 * s, w));
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

pub fn star(painter: &Painter, center: Pos2, r: f32, fill: Color32) {
    let mut pts = Vec::with_capacity(10);
    for i in 0..10 {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 5.0;
        let rr = if i % 2 == 0 { r } else { r * 0.45 };
        pts.push(center + vec2(a.cos(), a.sin()) * rr);
    }
    // fan from the centre so the concave shape fills correctly
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, fill);
    for q in &pts {
        mesh.colored_vertex(*q, fill);
    }
    for i in 1..=10u32 {
        mesh.add_triangle(0, i, if i == 10 { 1 } else { i + 1 });
    }
    painter.add(Shape::mesh(mesh));
}

pub fn triangle_right(painter: &Painter, c: Pos2, s: f32, color: Color32) {
    painter.add(Shape::convex_polygon(
        vec![c + vec2(-s * 0.45, -s * 0.6), c + vec2(s * 0.65, 0.0), c + vec2(-s * 0.45, s * 0.6)],
        color,
        Stroke::NONE,
    ));
}

pub fn triangle_left(painter: &Painter, c: Pos2, s: f32, color: Color32) {
    painter.add(Shape::convex_polygon(
        vec![c + vec2(s * 0.45, -s * 0.6), c + vec2(-s * 0.65, 0.0), c + vec2(s * 0.45, s * 0.6)],
        color,
        Stroke::NONE,
    ));
}

pub fn pause_bars(painter: &Painter, c: Pos2, s: f32, color: Color32) {
    let w = s * 0.3;
    let h = s * 1.1;
    painter.rect_filled(Rect::from_center_size(c + vec2(-s * 0.28, 0.0), vec2(w, h)), w * 0.4, color);
    painter.rect_filled(Rect::from_center_size(c + vec2(s * 0.28, 0.0), vec2(w, h)), w * 0.4, color);
}

/// Line-style speaker (thin strokes, like the Lucide icons the Craft apps use).
pub fn speaker(painter: &Painter, c: Pos2, s: f32, color: Color32, waves: u8) {
    let st = Stroke::new(1.5, color);
    let x0 = c.x - s * 0.55;
    let pts = vec![
        pos2(x0, c.y - s * 0.18),
        pos2(x0 + s * 0.25, c.y - s * 0.18),
        pos2(c.x + s * 0.05, c.y - s * 0.5),
        pos2(c.x + s * 0.05, c.y + s * 0.5),
        pos2(x0 + s * 0.25, c.y + s * 0.18),
        pos2(x0, c.y + s * 0.18),
        pos2(x0, c.y - s * 0.18),
    ];
    painter.add(Shape::line(pts, st));
    for w in 0..waves {
        let r = s * (0.35 + 0.22 * w as f32);
        let arc: Vec<Pos2> = (0..=8)
            .map(|i| {
                let a = -0.8 + 1.6 * i as f32 / 8.0;
                c + vec2(s * 0.05, 0.0) + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        painter.add(Shape::line(arc, st));
    }
}

pub fn shuffle_icon(painter: &Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new(1.6, color);
    let l = c.x - s * 0.6;
    let r = c.x + s * 0.45;
    let t = c.y - s * 0.35;
    let b = c.y + s * 0.35;
    painter.line_segment([pos2(l, t), pos2(r, b)], st);
    painter.line_segment([pos2(l, b), pos2(r, t)], st);
    for y in [t, b] {
        painter.add(Shape::convex_polygon(
            vec![pos2(r + s * 0.25, y), pos2(r - s * 0.05, y - s * 0.22), pos2(r - s * 0.05, y + s * 0.22)],
            color,
            Stroke::NONE,
        ));
    }
}

pub fn repeat_icon(painter: &Painter, c: Pos2, s: f32, color: Color32, one: bool) {
    let st = Stroke::new(1.6, color);
    let rect = Rect::from_center_size(c, vec2(s * 1.2, s * 0.75));
    painter.rect_stroke(rect, 3.0, st, egui::StrokeKind::Middle);
    let a = pos2(rect.center().x + s * 0.05, rect.top());
    painter.add(Shape::convex_polygon(
        vec![a + vec2(s * 0.25, 0.0), a + vec2(-s * 0.05, -s * 0.22), a + vec2(-s * 0.05, s * 0.22)],
        color,
        Stroke::NONE,
    ));
    if one {
        painter.text(c, egui::Align2::CENTER_CENTER, "1", semibold(s * 0.62), color);
    }
}

/// Placeholder cover: a soft card with a note shape (no artwork copied).
pub fn placeholder_cover(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, RADIUS, p().surface);
    painter.rect_stroke(rect, RADIUS, Stroke::new(1.0, p().line), egui::StrokeKind::Inside);
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.18;
    let col = p().star_empty;
    painter.circle_filled(c + vec2(-s * 0.6, s * 0.9), s * 0.45, col);
    painter.circle_filled(c + vec2(s * 1.0, s * 0.6), s * 0.45, col);
    painter.line_segment([c + vec2(-s * 0.18, s * 0.9), c + vec2(-s * 0.18, -s * 1.1)], Stroke::new(s * 0.18, col));
    painter.line_segment([c + vec2(s * 1.42, s * 0.6), c + vec2(s * 1.42, -s * 1.4)], Stroke::new(s * 0.18, col));
    painter.line_segment([c + vec2(-s * 0.18, -s * 1.1), c + vec2(s * 1.42, -s * 1.4)], Stroke::new(s * 0.35, col));
}

/// A small drawn "x" (close / remove).
pub fn cross(painter: &Painter, c: Pos2, r: f32, color: Color32) {
    let st = Stroke::new(1.5, color);
    painter.line_segment([c + vec2(-r, -r), c + vec2(r, r)], st);
    painter.line_segment([c + vec2(-r, r), c + vec2(r, -r)], st);
}
