//! The popover acts as a compact tray menu — it contains the menu items
//! (Open Settings, Quit) as GPUI elements. It is opened by a left-click on
//! the tray icon and anchored near the icon. The native right-click context
//! menu remains the primary menu on tray platforms; the popover doubles as
//! the fallback window on platforms without tray support.
use gpui::{
    App, Bounds, Context, DisplayId, Hsla, IntoElement, Pixels, Render, SharedString, Size, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};

use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Root;

use crate::host_entry::seed_entries;
use crate::settings_window::open_settings_window;
use crate::tray_icon::IconAnchor;

/// Fallback popover size, used only when the generated window module hands us
/// something other than windowed bounds. The real size is canvas-editable —
/// see [`open_popover_window`].
const FALLBACK_POPOVER_SIZE: (f32, f32) = (280.0, 240.0);

/// The small window shown when the tray icon is left-clicked.
pub struct PopoverView {
    /// Human-readable project name, shown as the window title.
    pub project_name: SharedString,
    /// Tooltip text from the tray icon — shown in the popover header.
    pub icon_tooltip: SharedString,
    /// Data the generated layout reads: the `entries` collection the popover's
    /// `Repeat` region iterates. Built once; swap it for real host data.
    bindings: crate::zml_layout::ZmlBindings,
    /// Retained per-row controls the layout owns (e.g. `TextInput`). Created on
    /// first render, then kept so input state survives redraws.
    entities: Option<crate::zml_layout::ZmlEntities>,
}

impl PopoverView {
    /// Creates a new `PopoverView` with the given project name and tooltip.
    pub fn new(
        project_name: impl Into<SharedString>,
        icon_tooltip: impl Into<SharedString>,
    ) -> Self {
        Self {
            project_name: project_name.into(),
            icon_tooltip: icon_tooltip.into(),
            bindings: crate::zml_layout::ZmlBindings {
                entries: seed_entries(),
            },
            entities: None,
        }
    }
}

/// Style shared by all tray-menu item rows.
///
/// `hover_bg` is the theme accent colour used for the hover highlight.
fn menu_item_row(hover_bg: Hsla) -> gpui::Div {
    div()
        .px_3()
        .py_2()
        .rounded_md()
        .hover(move |el| el.bg(hover_bg))
        .cursor_pointer()
}

impl Render for PopoverView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.project_name.clone();
        let tooltip = self.icon_tooltip.clone();

        let bg = cx.theme().background;
        let fg = cx.theme().foreground;
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent;
        let danger = cx.theme().danger;

        // The generated layout owns a per-row `TextInput`, so its retained
        // entities are created once (on the first frame, when a window exists)
        // and synced against the collection before every render.
        let entities = self
            .entities
            .get_or_insert_with(|| crate::zml_layout::ZmlEntities::new(window, cx));
        entities.sync(&self.bindings, window, cx);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .text_color(fg)
            // App header with icon
            .child({
                let icon_bytes = crate::icon_assets::PNG_BYTES;
                let icon = std::sync::Arc::new(gpui::Image {
                    format: gpui::ImageFormat::Png,
                    bytes: icon_bytes.to_vec(),
                    id: 0,
                });
                div()
                    .px_3()
                    .py_3()
                    .border_b_1()
                    .border_color(border)
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(gpui::img(icon).w(px(24.0)).h(px(24.0)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(name.to_string()),
                            )
                            .child(div().text_xs().text_color(muted).child(tooltip.to_string())),
                    )
            })
            // Editable content region. Everything around it — the header and the
            // menu rows below — is chrome: those rows carry click handlers that
            // open settings and quit the app, so they stay hand-written. This
            // child is the part the Zoid canvas owns, regenerated from
            // `ui/app.json` on export.
            .child(crate::zml_layout::zml_layout(
                &self.bindings,
                entities,
                window,
                cx,
            ))
            // Open Settings: dismiss the popover first so the settings window
            // opens in front (the popover is a topmost PopUp on Windows).
            .child(menu_item_row(accent).child("Open Settings").on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |_, _, window, cx| {
                    // Read before the popover goes: settings open on the
                    // display the popover was on.
                    let display = window.display(cx).map(|display| display.id());
                    if cx.has_global::<PopoverHandle>() {
                        cx.remove_global::<PopoverHandle>();
                        window.remove_window();
                    }
                    open_settings_window(cx, name.clone(), display);
                }),
            ))
            // Separator
            .child(div().h(px(1.0)).bg(border).mx_2())
            // Quit
            .child(
                menu_item_row(accent)
                    .child("Quit")
                    .text_color(danger)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|_, _, _window, cx| {
                            let _ = std::fs::remove_file(
                                crate::lifecycle::default_lock_path().unwrap_or_default(),
                            );
                            cx.quit();
                        }),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Window creation
// ---------------------------------------------------------------------------

/// Handle to the open popover window, stored as a [`gpui::Global`] so a
/// second tray left-click dismisses the popover instead of stacking a copy.
///
/// Only exercised through the binary (`main.rs`), which is not pulled into
/// the `tests/` integration build — hence `dead_code`.
#[allow(dead_code)]
pub struct PopoverHandle {
    handle: gpui::AnyWindowHandle,
}

impl PopoverHandle {
    fn new(handle: gpui::AnyWindowHandle) -> Self {
        Self { handle }
    }
}

impl gpui::Global for PopoverHandle {}

/// Toggles the tray popover: opens it anchored near the tray icon when no
/// popover is open, dismisses the existing one otherwise.
///
/// Called from the binary's tray left-click path.
#[allow(dead_code)]
pub fn toggle_popover_window(
    cx: &mut App,
    project_name: impl Into<SharedString> + Clone,
    icon_tooltip: impl Into<SharedString> + Clone,
    icon_rect: Option<IconAnchor>,
) {
    if cx.has_global::<PopoverHandle>() {
        let handle = cx.remove_global::<PopoverHandle>();
        // If the window is still alive, dismiss it. If it was already closed
        // (e.g. after Open Settings), fall through and open a fresh popover.
        let closed = handle
            .handle
            .update(cx, |_, window, _| window.remove_window())
            .is_ok();
        if closed {
            return;
        }
    }
    if let Some(handle) = open_popover_window(cx, project_name, icon_tooltip, icon_rect) {
        cx.set_global(PopoverHandle::new(handle));
    }
}

/// Surfaces the tray popover: opens it when no live popover window exists,
/// does nothing when one is already visible.
///
/// Called from the binary's second-instance surface-request path
/// (`lifecycle::signal_existing_instance`).
#[allow(dead_code)]
pub fn ensure_popover_window(
    cx: &mut App,
    project_name: impl Into<SharedString> + Clone,
    icon_tooltip: impl Into<SharedString> + Clone,
    icon_rect: Option<IconAnchor>,
) {
    // Copy the handle out so the global borrow ends before we mutate `cx`.
    let existing = cx.try_global::<PopoverHandle>().map(|h| h.handle);
    if let Some(handle) = existing {
        // Live window → already visible, nothing to do. A dead handle (the
        // window was closed without removing the global) falls through and a
        // fresh popover is opened.
        if handle.update(cx, |_, _, _| {}).is_ok() {
            return;
        }
        cx.remove_global::<PopoverHandle>();
    }
    if let Some(handle) = open_popover_window(cx, project_name, icon_tooltip, icon_rect) {
        cx.set_global(PopoverHandle::new(handle));
    }
}

/// The display containing the tray icon, as its id (when known) and bounds,
/// falling back to the primary display, then to a 1920×1080 placeholder.
///
/// The id matters on macOS: every display reports its bounds at origin
/// `(0, 0)` there, so the bounds alone cannot say which display to open on
/// (see [`IconAnchor::display_id`]).
#[allow(dead_code)]
fn display_for_anchor(
    cx: &mut App,
    anchor: Option<IconAnchor>,
) -> (Option<DisplayId>, Bounds<Pixels>) {
    let display = anchor
        .and_then(|anchor| anchor.display_id(cx))
        .and_then(|id| cx.find_display(id))
        .or_else(|| cx.primary_display());
    match display {
        Some(display) => (Some(display.id()), display.bounds()),
        None => (
            None,
            Bounds {
                origin: gpui::point(px(0.0), px(0.0)),
                size: size(px(1920.0), px(1080.0)),
            },
        ),
    }
}

/// Compute the popover bounds for a given screen and anchor rect.
///
/// Places the popover just below the anchor when it fits on screen; flips it
/// above when the anchor sits near the bottom edge. The horizontal origin is
/// clamped so the popover never extends past the screen edge. When no anchor
/// is given, the popover is placed in the bottom-right corner of the screen.
fn popover_bounds(
    screen: &Bounds<Pixels>,
    anchor: Option<(f32, f32, f32, f32)>,
    size: Size<Pixels>,
) -> Bounds<Pixels> {
    let gap = px(4.0);
    match anchor {
        Some((x, y, _w, h)) => {
            let below = px(y) + px(h) + gap;
            let bottom_limit = screen.origin.y + screen.size.height;
            let right_limit = screen.origin.x + screen.size.width;
            let top = if below + size.height <= bottom_limit {
                below
            } else {
                px(y) - size.height - gap
            };
            let origin_x = px(x)
                .max(screen.origin.x)
                .min((right_limit - size.width).max(screen.origin.x));
            Bounds {
                origin: gpui::point(origin_x, top),
                size,
            }
        }
        None => Bounds {
            origin: gpui::point(
                screen.origin.x + screen.size.width - size.width,
                screen.origin.y + screen.size.height - size.height,
            ),
            size,
        },
    }
}

/// Opens the tray popover window anchored near the tray icon.
///
/// If `icon_rect` is `None` (no tray available) the window is placed in the
/// bottom-right corner of the primary display instead of centre-screen, which
/// put a topmost menu-sized window in the middle of the user's work area.
#[allow(dead_code)]
pub fn open_popover_window(
    cx: &mut App,
    project_name: impl Into<SharedString> + Clone,
    icon_tooltip: impl Into<SharedString> + Clone,
    icon_rect: Option<IconAnchor>,
) -> Option<gpui::AnyWindowHandle> {
    // The generated module carries the window props from `ui/app.json`, so the
    // popover's kind and size are canvas-editable.
    let generated = crate::window_options::window_options(cx);
    let popover_size = match generated.window_bounds {
        Some(WindowBounds::Windowed(bounds)) => bounds.size,
        // The UI source asks for a maximised or fullscreen popover, which makes
        // no sense for a tray menu — fall back rather than honouring it.
        _ => size(px(FALLBACK_POPOVER_SIZE.0), px(FALLBACK_POPOVER_SIZE.1)),
    };

    let (display_id, screen) = display_for_anchor(cx, icon_rect);
    let anchor_rect = icon_rect.map(|anchor| anchor.rect);
    let window_bounds = WindowBounds::Windowed(popover_bounds(&screen, anchor_rect, popover_size));

    let options = WindowOptions {
        // Position is anchored to the tray icon at runtime, so it overrides the
        // generated (centred) bounds — no static prop can express "next to the
        // icon". The size inside those bounds is still the canvas-editable one.
        window_bounds: Some(window_bounds),
        // Name the icon's display. On macOS the bounds above are relative to
        // it, and without an id GPUI opens on the primary display wherever
        // the icon is.
        display_id,
        // A tray popover is chrome-less and cannot be minimised, whatever the
        // UI source says; `kind` and `is_resizable` come from `generated`.
        titlebar: None,
        window_decorations: None,
        is_minimizable: false,
        focus: true,
        show: true,
        ..generated
    };

    let pn: SharedString = project_name.into();
    let it: SharedString = icon_tooltip.into();
    let result = cx.open_window(options, move |window, cx| {
        let view = cx.new(|_cx| PopoverView::new(pn.clone(), it.clone()));
        crate::settings_window::follow_system_theme(window, cx);
        cx.new(|cx| Root::new(view, window, cx))
    });

    match result {
        Ok(handle) => Some(handle.into()),
        Err(error) => {
            eprintln!("failed to open popover window: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    #[gpui::test]
    fn popover_view_stores_fields(cx: &mut TestAppContext) {
        let entity = cx.update(|cx| cx.new(|_cx| PopoverView::new("Tidepool", "Tidepool")));

        entity.read_with(cx, |view, _| {
            assert_eq!(view.project_name.as_ref(), "Tidepool");
            assert_eq!(view.icon_tooltip.as_ref(), "Tidepool");
        });
    }

    /// The size the tray template's own `ui/app.json` declares. The bounds
    /// maths is independent of where the size came from, so the tests pin a
    /// fixed one rather than reaching into the generated module.
    fn test_size() -> Size<Pixels> {
        size(px(280.0), px(240.0))
    }

    #[test]
    fn popover_bounds_anchor_below_icon() {
        let screen = Bounds {
            origin: gpui::point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1080.0)),
        };
        // Icon near the top of the screen → popover goes just below it.
        let bounds = popover_bounds(&screen, Some((900.0, 20.0, 32.0, 32.0)), test_size());
        assert_eq!(bounds.origin.x, px(900.0));
        assert_eq!(bounds.origin.y, px(20.0 + 32.0 + 4.0));
        assert_eq!(bounds.size, test_size());
    }

    #[test]
    fn popover_bounds_flips_above_near_bottom_edge() {
        let screen = Bounds {
            origin: gpui::point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1080.0)),
        };
        // Icon at the bottom of the screen → below placement would overflow,
        // so the popover must be placed above the icon instead.
        let bounds = popover_bounds(&screen, Some((1600.0, 1048.0, 32.0, 32.0)), test_size());
        assert_eq!(bounds.origin.y, px(1048.0) - test_size().height - px(4.0));
        assert_eq!(bounds.origin.x, px(1600.0));
    }

    #[test]
    fn popover_bounds_clamps_horizontal_origin() {
        let screen = Bounds {
            origin: gpui::point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1080.0)),
        };
        // Icon too far right → popover right edge snaps to the screen edge.
        let bounds = popover_bounds(&screen, Some((2000.0, 20.0, 32.0, 32.0)), test_size());
        assert_eq!(bounds.origin.x, px(1920.0) - test_size().width);
        assert_eq!(bounds.origin.y, px(20.0 + 32.0 + 4.0));
    }

    #[test]
    fn popover_bounds_no_anchor_uses_bottom_right() {
        let screen = Bounds {
            origin: gpui::point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1080.0)),
        };
        let bounds = popover_bounds(&screen, None, test_size());
        assert_eq!(bounds.origin.x, px(1920.0) - test_size().width);
        assert_eq!(bounds.origin.y, px(1080.0) - test_size().height);
    }
}
