//! Desktop front end for the Il2CppInspector CLI.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod cli;
mod input;
mod job;
mod picker;
mod ui;
mod workspace;

use gpui::{App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

fn main() {
    Application::new().run(|cx: &mut App| {
        input::bind_keys(cx);
        workspace::bind_keys(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1440.), px(880.)), cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1200.), px(600.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Il2CppInspector".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| workspace::Workspace::new(window, cx)),
        );
        if let Err(error) = window {
            eprintln!("Cannot create the application window: {error}");
            cx.quit();
        }
        cx.activate(true);
    });
}
