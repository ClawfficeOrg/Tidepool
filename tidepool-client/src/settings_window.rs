//! Settings window for the tray application.
//!
//! A GPUI view shown when the user selects "Open Settings" from the tray menu.
//! It also owns the app's theme choice ([`ThemeChoice`]): system, light or
//! dark. The choice lives in a global for the life of the process; persist it
//! with your own settings if it should survive a restart.

use gpui::{
    App, Bounds, Context, DisplayId, Focusable, Render, SharedString, Window, WindowBounds,
    WindowKind, WindowOptions, div, prelude::*, px, size,
};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Root, Theme, ThemeMode};

/// Which appearance the app uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    /// Follow the system's light/dark setting.
    #[default]
    System,
    Light,
    Dark,
}

impl gpui::Global for ThemeChoice {}

impl ThemeChoice {
    /// Every choice, in the order the settings window lists them.
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    /// The label shown in the settings window.
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// The choice in effect; [`ThemeChoice::System`] until one is applied.
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }
}

/// Makes `choice` the app's theme and repaints every open window.
///
/// Call once from `main` after `gpui_kit::init`, which installs the light
/// theme whatever the system setting is.
pub fn apply_theme(choice: ThemeChoice, cx: &mut App) {
    cx.set_global(choice);
    match choice {
        ThemeChoice::System => Theme::sync_system_appearance(None, cx),
        ThemeChoice::Light => Theme::change(ThemeMode::Light, None, cx),
        ThemeChoice::Dark => Theme::change(ThemeMode::Dark, None, cx),
    }
    cx.refresh_windows();
}

/// Keeps the theme in step with the system appearance while the choice is
/// [`ThemeChoice::System`]. Call for each window as it opens.
///
/// A tray app can sit with no window for hours, so the appearance is read
/// again here as well as on every change the window reports.
pub fn follow_system_theme(window: &mut Window, cx: &mut App) {
    if ThemeChoice::current(cx) == ThemeChoice::System {
        Theme::sync_system_appearance(Some(window), cx);
    }
    window
        .observe_window_appearance(|window, cx| {
            if ThemeChoice::current(cx) == ThemeChoice::System {
                Theme::sync_system_appearance(Some(window), cx);
                cx.refresh_windows();
            }
        })
        .detach();
}

/// Opens a settings window, centred on `display` (the primary display when
/// `None`).
///
/// Callers pass the display the user is on: the tray icon's, or that of the
/// window the request came from.
pub fn open_settings_window(
    cx: &mut App,
    project_name: impl Into<SharedString>,
    display: Option<DisplayId>,
) {
    let pn: SharedString = project_name.into();
    let bounds = Bounds::centered(display, size(px(420.0), px(320.0)), cx);

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        // The bounds alone do not pick the display on macOS, where they are
        // relative to it.
        display_id: display,
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(format!("{pn} Settings").into()),
            ..Default::default()
        }),
        kind: WindowKind::Normal,
        focus: true,
        show: true,
        is_resizable: false,
        ..Default::default()
    };

    let _ = cx.open_window(options, move |window, cx| {
        let view = cx.new(|cx| SettingsWindow::new(pn.clone(), cx));
        window.focus(&view.focus_handle(cx), cx);
        follow_system_theme(window, cx);
        // No fixed `bg`: `Root` paints the theme's background itself, so the
        // window repaints when the theme changes.
        cx.new(|cx| Root::new(view, window, cx))
    });
    // A tray app is a background (accessory) app: it is not the active app
    // when its menu is used, and a window opened by an inactive app appears
    // behind the frontmost one. Bring the app forward with its window.
    cx.activate(true);
}

/// Settings window state.
pub struct SettingsWindow {
    focus_handle: gpui::FocusHandle,
    project_name: SharedString,
}

impl SettingsWindow {
    pub fn new(project_name: SharedString, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            project_name,
        }
    }
}

impl gpui::Focusable for SettingsWindow {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.project_name.clone();
        let current = ThemeChoice::current(cx);
        div()
            .flex()
            .flex_col()
            .size_full()
            .p_6()
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("Settings"),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("Project Name:"))
                    .child(div().text_sm().child(name.to_string())),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("Theme:"))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .children(ThemeChoice::ALL.map(|choice| {
                                let button = Button::new(choice.label()).child(choice.label());
                                let button = if choice == current {
                                    button.primary()
                                } else {
                                    button.secondary()
                                };
                                button.on_click(cx.listener(move |_, _, _window, cx| {
                                    apply_theme(choice, cx);
                                    cx.notify();
                                }))
                            })),
                    ),
            )
            .child(
                div().flex().flex_row().justify_end().mt_4().child(
                    Button::new("close")
                        .secondary()
                        .child("Close")
                        .on_click(cx.listener(|_, _, window, _| {
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

    #[test]
    fn theme_choice_defaults_to_system() {
        assert_eq!(ThemeChoice::default(), ThemeChoice::System);
        assert_eq!(ThemeChoice::ALL[0], ThemeChoice::System);
    }

    #[gpui::test]
    fn settings_window_creates_without_panic(cx: &mut TestAppContext) {
        let entity = cx.update(|cx| cx.new(|cx| SettingsWindow::new("Tidepool".into(), cx)));

        entity.read_with(cx, |view, _| {
            assert_eq!(view.project_name.as_ref(), "Tidepool");
        });
    }
}
