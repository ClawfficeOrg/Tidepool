//! Settings window for the tray application.
//!
//! A GPUI view shown when the person selects "Settings…" from the tray menu or
//! a window. It owns the client's editable preferences: the Clawffice palette,
//! the pool URL, which notification kinds are allowed, the project directories
//! the person works in and the default agent. Every change is written through
//! [`crate::settings::update`], which is atomic and keeps the in-memory store in
//! step with the file.
//!
//! The palette is applied through [`crate::theme::apply`], so a switch repaints
//! every open window rather than only this one. A corrupt settings file is
//! reported at the top of the window; the store has already set the bad file
//! aside by then.

use gpui::{
    App, Bounds, Context, DisplayId, Focusable, Render, SharedString, Subscription, Window,
    WindowBounds, WindowKind, WindowOptions, div, prelude::*, px, size,
};
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Root;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};

use crate::notify::NotificationKind;
use crate::settings::{self, ClientSettings};
use crate::theme::{self, PaletteName};

/// Opens a settings window, centred on `display` (the primary display when
/// `None`).
///
/// Callers pass the display the person is on: the tray icon's, or that of the
/// window the request came from.
pub fn open_settings_window(
    cx: &mut App,
    project_name: impl Into<SharedString>,
    display: Option<DisplayId>,
) {
    let pn: SharedString = project_name.into();
    let bounds = Bounds::centered(display, size(px(560.0), px(680.0)), cx);

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
        is_resizable: true,
        ..Default::default()
    };

    let _ = cx.open_window(options, move |window, cx| {
        let view = cx.new(|cx| SettingsWindow::new(pn.clone(), cx));
        window.focus(&view.focus_handle(cx), cx);
        theme::follow_theme(window, cx);
        // No fixed `bg`: `Root` paints the theme's background itself, so the
        // window repaints when the palette changes.
        cx.new(|cx| Root::new(view, window, cx))
    });
    // A tray app is a background (accessory) app: it is not the active app
    // when its menu is used, and a window opened by an inactive app appears
    // behind the frontmost one. Bring the app forward with its window.
    cx.activate(true);
}

/// The text inputs the settings window keeps between frames.
///
/// Created through `Window::use_keyed_state` because an [`InputState`] needs a
/// window. The subscriptions persist the pool URL and default agent as the
/// person types; the directory field is committed by its Add button so a
/// half-typed path is never saved.
struct SettingsInputs {
    pool_url: gpui::Entity<InputState>,
    default_agent: gpui::Entity<InputState>,
    directory: gpui::Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

/// Settings window state.
pub struct SettingsWindow {
    focus_handle: gpui::FocusHandle,
    project_name: SharedString,
}

impl SettingsWindow {
    /// Build the view. The inputs are created on the first render, when a
    /// window exists.
    pub fn new(project_name: SharedString, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            project_name,
        }
    }

    /// The inputs, created once per window and reused on every render.
    fn inputs(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        settings: &ClientSettings,
    ) -> gpui::Entity<SettingsInputs> {
        let pool_url = settings.pool_url.clone().unwrap_or_default();
        let default_agent = settings.default_agent.clone().unwrap_or_default();
        window.use_keyed_state("tidepool-settings-inputs", cx, |window, cx| {
            let pool = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(pool_url)
                    .placeholder("http://127.0.0.1:8080")
            });
            let agent = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(default_agent)
                    .placeholder("the agent a new session starts with")
            });
            let directory = cx.new(|cx| {
                InputState::new(window, cx).placeholder("/path/to/a/project you work in")
            });

            let mut subscriptions = Vec::new();
            subscriptions.push(cx.subscribe(
                &pool,
                |_: &mut SettingsInputs, input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value().trim().to_owned();
                        let _ = settings::update(cx, |settings| {
                            settings.pool_url = (!value.is_empty()).then_some(value);
                        });
                    }
                },
            ));
            subscriptions.push(cx.subscribe(
                &agent,
                |_: &mut SettingsInputs, input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value().to_string();
                        let _ = settings::update(cx, |settings| {
                            settings.set_default_agent(value);
                        });
                    }
                },
            ));

            SettingsInputs {
                pool_url: pool,
                default_agent: agent,
                directory,
                _subscriptions: subscriptions,
            }
        })
    }
}

impl gpui::Focusable for SettingsWindow {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

/// A small section heading.
fn heading(text: &'static str) -> impl IntoElement {
    div()
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(text)
}

/// A label/value row.
fn labelled(label: &'static str, value: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .child(div().text_sm().child(label))
        .child(value)
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = settings::settings(cx).cloned().unwrap_or_default();
        let problem = settings::problem(cx).map(str::to_owned);
        let inputs = self.inputs(window, cx, &settings);
        let current_palette = theme::current(cx);
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let border = cx.theme().border;
        let policy = settings.notification_policy();
        let directories = settings.project_directories.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_6()
            .gap_3()
            .child(
                div()
                    .text_xl()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("Settings"),
            )
            .when_some(problem, |this, problem| {
                this.child(
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(danger)
                        .p_2()
                        .text_sm()
                        .text_color(danger)
                        .child(problem),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("Project Name:"))
                    .child(div().text_sm().child(self.project_name.to_string())),
            )
            .child(heading("Palette"))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .children(PaletteName::ALL.map(|palette| {
                        let button = Button::new(format!("palette-{}", palette.as_str()))
                            .child(palette.title());
                        let button = if palette == current_palette {
                            button.primary()
                        } else {
                            button.secondary()
                        };
                        button.on_click(cx.listener(move |_, _, _, cx| {
                            theme::apply(palette, cx);
                            let _ = settings::update(cx, |settings| settings.palette = palette);
                            cx.notify();
                        }))
                    })),
            )
            .child(heading("Pool"))
            .child(labelled("Pool URL:", Input::new(&inputs.read(cx).pool_url)))
            .child(heading("Notifications"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(NotificationKind::ALL.map(|kind| {
                        let enabled = policy.allows(kind);
                        let button = Button::new(format!("notify-{}", kind.key()))
                            .child(if enabled { "On" } else { "Off" });
                        let button = if enabled {
                            button.primary()
                        } else {
                            button.secondary()
                        };
                        labelled(
                            kind.label(),
                            button.on_click(cx.listener(move |_, _, _, cx| {
                                let _ = settings::update(cx, |settings| {
                                    settings.set_notification(kind, !enabled);
                                });
                                cx.notify();
                            })),
                        )
                    })),
            )
            .child(heading("Project directories"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(directories.into_iter().map(|directory| {
                        let remove = directory.clone();
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(div().flex_1().text_sm().text_color(muted).child(directory))
                            .child(
                                Button::new(format!("remove-directory-{remove}"))
                                    .secondary()
                                    .child("Remove")
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        let _ = settings::update(cx, |settings| {
                                            settings.forget_project_directory(&remove);
                                        });
                                        cx.notify();
                                    })),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&inputs.read(cx).directory)))
                    .child(
                        Button::new("add-directory")
                            .secondary()
                            .child("Add")
                            .on_click({
                                let inputs = inputs.clone();
                                cx.listener(move |_, _, _, cx| {
                                    let directory = {
                                        let inputs = inputs.read(cx);
                                        inputs.directory.read(cx).value().to_string()
                                    };
                                    let _ = settings::update(cx, |settings| {
                                        settings.remember_project_directory(directory);
                                    });
                                    cx.notify();
                                })
                            }),
                    ),
            )
            .child(heading("Agent"))
            .child(labelled(
                "Default agent:",
                Input::new(&inputs.read(cx).default_agent),
            ))
            .child(div().h(px(1.0)).bg(border))
            .child(
                div().flex().flex_row().justify_end().mt_2().child(
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
    fn the_palette_picker_lists_every_palette() {
        assert_eq!(PaletteName::ALL.len(), 5);
        assert_eq!(PaletteName::ALL[0], PaletteName::Biolume);
    }

    #[gpui::test]
    fn settings_window_creates_without_panic(cx: &mut TestAppContext) {
        let entity = cx.update(|cx| cx.new(|cx| SettingsWindow::new("Tidepool".into(), cx)));

        entity.read_with(cx, |view, _| {
            assert_eq!(view.project_name.as_ref(), "Tidepool");
        });
    }
}
