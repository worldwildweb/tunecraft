//! TuneCraft: a music player for current macOS, rebuilt from Songbird 2.2, in the Craft apps' style.

mod app;
mod library;
mod panes;
mod player;
mod theme;
mod web;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let mut screenshot: Option<PathBuf> = None;
    let mut open_url: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--screenshot" {
            screenshot = args.next().map(PathBuf::from);
        } else if a == "--open-url" {
            open_url = args.next();
        } else if let Some(p) = a.strip_prefix("--screenshot=") {
            screenshot = Some(PathBuf::from(p));
        }
    }
    let checking = screenshot.is_some();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("TuneCraft")
        .with_app_id("ai.storyteller.tunecraft")
        .with_inner_size([1180.0, 760.0])
        .with_min_inner_size([760.0, 480.0])
        .with_drag_and_drop(true);
    if checking {
        // never take focus from the user while a check runs
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if checking {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            builder.with_activate_ignoring_other_apps(false);
        }));
    }
    // keep drawing while unfocused so the screenshot frames arrive
    options.run_and_return = true;

    eframe::run_native(
        "TuneCraft",
        options,
        Box::new(move |cc| {
            let mut app = app::TuneCraftApp::new(cc, screenshot);
            if let Some(u) = open_url {
                app.new_tab(&u);
            }
            Ok(Box::new(app))
        }),
    )
}
