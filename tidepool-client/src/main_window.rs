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

use gpui::AsyncApp;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Disableable;
use gpui_kit::component::Root;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};

use crate::host::client::HttpPool;
use crate::host::enroll::{EnrolledHost, Machine};
use crate::runs::http::HttpRuns;
use crate::runs::{RunsController, RunsSnapshot};
use crate::runs_view::RunsView;
use crate::settings_window::open_settings_window;
use std::time::Instant;

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
    /// The sign-in and enrol inputs, created on first render because an
    /// input needs a window.
    form: Option<MachineForm>,
    /// This machine's status lines, read when the form is created and after
    /// every enrol attempt.
    machine_lines: Vec<String>,
    /// Progress or the outcome of the last enrol attempt.
    machine_status: SharedString,
    /// True while a sign-in and enrol runs off the UI thread.
    working: bool,
    /// The activity pane, created on the first successful refresh.
    runs: Option<gpui::Entity<RunsView>>,
    /// Progress or the reason the last runs refresh failed.
    runs_status: SharedString,
    /// True while a runs refresh runs off the UI thread.
    runs_loading: bool,
}

/// The inputs for signing in to a pool and enrolling this machine.
struct MachineForm {
    pool_url: gpui::Entity<InputState>,
    email: gpui::Entity<InputState>,
    password: gpui::Entity<InputState>,
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
            form: None,
            machine_lines: Vec::new(),
            machine_status: SharedString::default(),
            working: false,
            runs: None,
            runs_status: SharedString::default(),
            runs_loading: false,
        }
    }

    /// Loads the activity pane from the pool, off the UI thread.
    fn refresh_runs(&mut self, cx: &mut Context<Self>) {
        if self.runs_loading {
            return;
        }
        self.runs_loading = true;
        self.runs_status = "Loading runs…".into();
        cx.notify();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let outcome = cx
                .background_executor()
                .spawn(async move { load_runs() })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.finish_runs(outcome, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Applies a finished runs refresh: the pane is created once, then updated.
    fn finish_runs(&mut self, outcome: Result<RunsSnapshot, String>, cx: &mut Context<Self>) {
        self.runs_loading = false;
        match outcome {
            Ok(snapshot) => {
                self.runs_status = SharedString::default();
                if let Some(view) = self.runs.clone() {
                    view.update(cx, |view, cx| {
                        view.set_snapshot(snapshot);
                        cx.notify();
                    });
                } else {
                    self.runs = Some(cx.new(|cx| RunsView::new(snapshot, cx)));
                }
            }
            Err(reason) => self.runs_status = format!("Could not load runs: {reason}").into(),
        }
    }

    /// Signs in to the pool and enrols this machine, off the UI thread.
    fn enrol(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.working {
            return;
        }
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let pool_url = form.pool_url.read(cx).value().trim().to_string();
        let email = form.email.read(cx).value().trim().to_string();
        let password = form.password.read(cx).value().to_string();
        // The password leaves the input as soon as it is read.
        form.password
            .clone()
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.working = true;
        self.machine_status = "Signing in and enrolling this machine…".into();
        cx.notify();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let outcome = cx
                .background_executor()
                .spawn(async move { sign_in_and_enrol(&pool_url, &email, &password) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.finish_enrol(outcome);
                cx.notify();
            });
        })
        .detach();
    }

    /// Applies a finished enrol: the enrolled slug, or the reason it failed.
    fn finish_enrol(&mut self, outcome: Result<(EnrolledHost, Vec<String>), String>) {
        self.working = false;
        match outcome {
            Ok((host, lines)) => {
                self.machine_status = format!("Enrolled as {}.", host.slug).into();
                self.machine_lines = lines;
            }
            Err(reason) => {
                self.machine_status = format!("Enrol failed: {reason}").into();
                self.machine_lines = Machine::open(HttpPool::new()).status_lines();
            }
        }
    }
}

/// Signs in, then enrols this machine with that session. Blocking I/O, so it
/// runs on the background executor.
fn sign_in_and_enrol(
    pool_url: &str,
    email: &str,
    password: &str,
) -> Result<(EnrolledHost, Vec<String>), String> {
    let mut machine = Machine::open(HttpPool::new());
    machine
        .sign_in(pool_url, email, password)
        .map_err(|error| error.to_string())?;
    let host = machine
        .enroll_this_machine(&machine_hostname())
        .map_err(|error| error.to_string())?;
    Ok((host, machine.status_lines()))
}

/// Loads the operator's activity from the pool. Blocking I/O, so it runs on the
/// background executor. Needs a stored sign-in.
fn load_runs() -> Result<RunsSnapshot, String> {
    let machine = Machine::open(HttpPool::new());
    let session = machine
        .session()
        .cloned()
        .ok_or_else(|| "sign in first".to_string())?;
    let local_host = machine.host_id().map(str::to_string);
    let mut controller = RunsController::new(HttpRuns::new(session), local_host);
    controller
        .refresh(Instant::now())
        .map_err(|error| error.to_string())?;
    Ok(controller.snapshot())
}

/// This machine's hostname, or a placeholder when the OS does not report one.
fn machine_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "this-machine".to_string())
}

impl gpui::Focusable for MainWindow {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.project_name.clone();
        let tooltip = self.icon_tooltip.clone();
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;

        if self.form.is_none() {
            self.form = Some(MachineForm {
                pool_url: cx.new(|cx| {
                    InputState::new(window, cx).placeholder("https://tidepool.example.com")
                }),
                email: cx.new(|cx| InputState::new(window, cx).placeholder("Operator email")),
                password: cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Password")
                        .masked(true)
                }),
            });
            self.machine_lines = Machine::open(HttpPool::new()).status_lines();
        }

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
            .child({
                let form = self.form.as_ref().expect("form is created on first render");
                let mut column = div().flex().flex_col().gap_2().flex_grow(1.0).min_h_0();
                for line in &self.machine_lines {
                    column = column.child(div().text_sm().text_color(muted).child(line.clone()));
                }
                column
                    .child(Input::new(&form.pool_url))
                    .child(Input::new(&form.email))
                    .child(Input::new(&form.password))
                    .child(
                        Button::new("main-window-enrol")
                            .primary()
                            .label("Sign in and enrol this machine")
                            .disabled(self.working)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.enrol(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(self.machine_status.clone()),
                    )
                    .child(
                        Button::new("main-window-runs")
                            .secondary()
                            .label("Refresh runs")
                            .disabled(self.runs_loading)
                            .on_click(cx.listener(|view, _, _, cx| view.refresh_runs(cx))),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(self.runs_status.clone()),
                    )
                    .children(self.runs.clone())
            })
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
