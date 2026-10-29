//! The application's main window.
//!
//! A tray app normally keeps this window closed: the icon and its menu are the
//! app's primary surfaces, and the window exists for the work the user only
//! occasionally wants. It is opened from the tray menu's "Open App" item, and
//! optionally at launch when the project was generated with
//! `open_on_launch = true`.
//!
//! Zoid's canvas owns the popover's *content* (see `ui/app.json`). This window
//! is deliberately hand-written chrome, so a generated project has a working,
//! plain starting point the user can reshape freely.
use gpui::{
    App, Bounds, Context, DisplayId, Focusable, Render, SharedString, Size, Window, WindowBounds,
    WindowKind, WindowOptions, div, prelude::*, px, size,
};

use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Root;
use gpui_kit::component::button::{Button, ButtonVariants};

use crate::settings_window::open_settings_window;

/// The window's starting size. Unlike the popover, this is a resizable normal
/// window, so these are only the initial bounds.
const MAIN_WINDOW_SIZE: (f32, f32) = (720.0, 480.0);

/// Handle to the open main window, stored as a [`gpui::Global`] so a second
/// "Open App" selection focuses the existing window instead of opening another
/// copy.
///
/// Only constructed by [`open_main_window`]; the global is also what lets the
/// binary ask "is the main window up?" without threading a handle through the
/// tray manager.
pub struct MainWindowHandle {
    /// The live window, used to focus or close it from the tray menu.
    pub handle: gpui::AnyWindowHandle,
}

impl MainWindowHandle {
    fn new(handle: gpui::AnyWindowHandle) -> Self {
        Self { handle }
    }
}

impl gpui::Global for MainWindowHandle {}

/// Opens the main window, or focuses it when it is already open. A new
/// window is centred on `display` (the primary display when `None`); callers
/// pass the tray icon's display.
///
/// Returns the window handle so callers can report success; a failure to open
/// (for example, no display) is logged rather than panicking, since the tray
/// icon stays usable either way.
pub fn open_main_window(
    cx: &mut App,
    project_name: impl Into<SharedString> + Clone,
    icon_tooltip: impl Into<SharedString> + Clone,
    display: Option<DisplayId>,
) -> Option<gpui::AnyWindowHandle> {
    // Already open → bring it forward rather than stacking a duplicate.
    let existing = cx.try_global::<MainWindowHandle>().map(|h| h.handle);
    if let Some(handle) = existing {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            cx.activate(true);
            return Some(handle);
        }
        // The handle is dead (window closed without clearing the global).
        cx.remove_global::<MainWindowHandle>();
    }

    let pn: SharedString = project_name.into();
    let title = pn.to_string();
    let bounds = Bounds::centered(
        display,
        size(px(MAIN_WINDOW_SIZE.0), px(MAIN_WINDOW_SIZE.1)),
        cx,
    );

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        // The bounds alone do not pick the display on macOS, where they are
        // relative to it.
        display_id: display,
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(title.clone().into()),
            ..Default::default()
        }),
        kind: WindowKind::Normal,
        focus: true,
        show: true,
        is_resizable: true,
        ..Default::default()
    };

    let it: SharedString = icon_tooltip.into();
    let result = cx.open_window(options, move |window, cx| {
        let view = cx.new(|cx| MainWindow::new(pn.clone(), it.clone(), cx));
        window.focus(&view.focus_handle(cx), cx);
        crate::theme::follow_theme(window, cx);
        cx.new(|cx| Root::new(view, window, cx))
    });

    match result {
        Ok(handle) => {
            let handle: gpui::AnyWindowHandle = handle.into();
            cx.set_global(MainWindowHandle::new(handle));
            // A tray app is a background (accessory) app, so it is not
            // active when its menu is used; without this the window opens
            // behind the frontmost app.
            cx.activate(true);
            Some(handle)
        }
        Err(error) => {
            eprintln!("failed to open main window: {error}");
            None
        }
    }
}

/// State for the main window view.
pub struct MainWindow {
    focus_handle: gpui::FocusHandle,
    /// Human-readable project name, shown as the window title and heading.
    pub project_name: SharedString,
    /// Tooltip text from the tray icon, shown as the window subtitle.
    pub icon_tooltip: SharedString,
}

impl MainWindow {
    pub fn new(
        project_name: SharedString,
        icon_tooltip: SharedString,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            project_name,
            icon_tooltip,
        }
    }
}

impl gpui::Focusable for MainWindow {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.project_name.clone();
        let tooltip = self.icon_tooltip.clone();
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_6()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(name.to_string()),
                    )
                    .child(div().text_sm().text_color(muted).child(tooltip.to_string())),
            )
            .child(div().h(px(1.0)).bg(border))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow(1.0)
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Replace this with your application's content."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("main-window-settings")
                            .secondary()
                            .label("Settings\u{2026}")
                            .on_click({
                                let name = name.clone();
                                move |_, window, cx| {
                                    let display = window.display(cx).map(|display| display.id());
                                    open_settings_window(cx, name.clone(), display);
                                }
                            }),
                    )
                    .child(
                        Button::new("main-window-close")
                            .secondary()
                            .label("Close")
                            .on_click(cx.listener(|_, _, window, cx| {
                                cx.remove_global::<MainWindowHandle>();
                                window.remove_window();
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    #[gpui::test]
    fn main_window_creates_without_panic(cx: &mut TestAppContext) {
        let entity =
            cx.update(|cx| cx.new(|cx| MainWindow::new("Tidepool".into(), "Tidepool".into(), cx)));

        entity.read_with(cx, |view, _| {
            assert_eq!(view.project_name.as_ref(), "Tidepool");
            assert_eq!(view.icon_tooltip.as_ref(), "Tidepool");
        });
    }
}
