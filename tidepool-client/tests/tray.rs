#![allow(dead_code, unused_imports, unused_unsafe)]
// The tray template's integration test mounts the app's modules with `#[path]`
// so each surface can be exercised without launching the binary. Some of the
// mounted helpers are only called from the binary's own entry point, so this
// test crate sees them as unused; the generated `src/ui/` modules import the
// full GPUI prelude and use a subset; and objc2 0.6 makes some template `unsafe`
// blocks redundant on macOS.

#[path = "../src/icon_assets.rs"]
mod icon_assets;

#[path = "../src/lifecycle.rs"]
mod lifecycle;

#[path = "../src/tray_icon.rs"]
mod tray_icon;

#[path = "../src/popover_view.rs"]
mod popover_view;

#[path = "../src/main_window.rs"]
mod main_window;

// `popover_view` mounts the generated layout as `crate::zml_layout`, so the
// integration binary has to provide that module too — same pattern as the
// other `#[path]` re-declarations above.

#[path = "../src/ui/zml_layout.rs"]
mod zml_layout;

// Likewise for the window module: `open_popover_window` builds its options from
// `crate::window_options`.
#[path = "../src/ui/window_options.rs"]
mod window_options;

#[path = "../src/settings_window.rs"]
mod settings_window;

// The settings window reads and writes the client's preferences, and paints
// with the Clawffice palette, so the integration binary mounts those modules
// too.
#[path = "../src/settings.rs"]
mod settings;

#[path = "../src/theme.rs"]
mod theme;

// The menu and the notification policy are mounted here as well, so the
// template's own tray integration test proves the menu actions, their
// keyboard twins and the opt-in rule rather than only the unit tests doing so.
#[path = "../src/menu.rs"]
mod menu;

#[path = "../src/notify.rs"]
mod notify;

// The generated layout references the row type as `crate::host_entry::HostEntry`,
// so the integration binary mounts the same module the binary does.
#[path = "../src/host_entry.rs"]
mod host_entry;

// The main window signs in and enrols through the host module, so the test
// binary mounts it the same way the app binary does.
#[path = "../src/host/client.rs"]
pub mod host_client;
#[path = "../src/host/enroll.rs"]
pub mod host_enroll;

mod host {
    pub use crate::host_client as client;
    pub use crate::host_enroll as enroll;
}

use gpui::{AppContext, TestAppContext};
use tray_icon::TrayIcon;

// ── TrayIcon smoke tests ───────────────────────────────────────────────────────

#[test]
fn tray_icon_new_does_not_panic() {
    // On macOS/Windows this may return None when not on the main thread,
    // which is correct. The important thing is no panic.
    let mut icon = TrayIcon::new("Tidepool");
    let _ = icon.poll_click();
    let _ = icon.poll_context_menu();
    icon.set_state_color(10, 20, 30);
    icon.set_menu(&[(0, "Item".to_string())]);
    let _ = icon.anchor();
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn platform_inner_is_correct_type_on_linux() {
    // On Linux the inner is always None (no libappindicator dependency).
    let icon = TrayIcon::new("tooltip");
    assert!(!icon.poll_click());
    assert!(icon.poll_context_menu().is_none());
    assert!(icon.anchor().is_none());
}

// ── Tray icon asset tests ──────────────────────────────────────────────────────
//
// The menu-bar icon is the clawffice-design 12x12 `pool` cell sprite, not a
// placeholder. These tests are what a person would otherwise have to squint at
// a menu bar to check: the asset is the right size, it is crisp pixel art
// rather than a blurry resize, it covers the canvas, and it carries the design
// system's four sprite colours. Windows ships the design's own app icon.

/// The product hue (`tide-500`, dark scheme) the sprite's `#` cells use.
const TIDE: [u8; 3] = [0x4A, 0x9F, 0xE0];
/// The accent (`claw-500`, dark scheme) the sprite's `o` cells use.
const CLAW: [u8; 3] = [0xF2, 0x6B, 0x21];
/// Biolume `mist-50`, the sprite's `w` highlight cells.
const MIST: [u8; 3] = [0xE9, 0xFB, 0xF6];
/// Biolume `ink-950`, the sprite's `k` body cells.
const INK: [u8; 3] = [0x03, 0x0C, 0x11];

/// Decodes an RGBA PNG into `(width, height, pixels)`.
fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<[u8; 4]>) {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().expect("asset must have a PNG header");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).expect("asset must decode");
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "tray art must keep its alpha channel"
    );
    let pixels = buffer
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
        .collect();
    (info.width, info.height, pixels)
}

/// Returns the opaque bounding box as `(min_x, min_y, max_x, max_y)`.
fn opaque_bounds(pixels: &[[u8; 4]], width: u32) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (index, pixel) in pixels.iter().enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = index as u32 % width;
        let y = index as u32 / width;
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
        });
    }
    bounds
}

#[test]
fn menu_bar_icon_is_the_cell_sprite_at_menubar_size() {
    let (width, height, pixels) = decode_png(icon_assets::PNG_BYTES);

    // `tray_icon.rs` pins the logical size to 16pt; the 1x asset is that size.
    assert_eq!((width, height), (16, 16));
    assert_eq!(pixels.len(), 16 * 16);

    // Crisp pixel art: the sprite is scaled by whole pixels, so antialiasing
    // would show up as an alpha value that is neither clear nor opaque.
    assert!(
        pixels.iter().all(|p| p[3] == 0 || p[3] == 255),
        "the sprite must be rendered without antialiasing"
    );

    let opaque = pixels.iter().filter(|p| p[3] != 0).count();
    assert!(
        opaque >= 100,
        "the creature must fill the canvas, not sit as a speck: {opaque} opaque pixels"
    );

    let (min_x, min_y, max_x, max_y) =
        opaque_bounds(&pixels, width).expect("the icon must not be empty");
    assert!(
        max_x - min_x + 1 >= 12 && max_y - min_y + 1 >= 10,
        "the creature must be legible at menu-bar size, but its bounds are {}x{}",
        max_x - min_x + 1,
        max_y - min_y + 1
    );

    let mut colours: Vec<[u8; 3]> = pixels
        .iter()
        .filter(|pixel| pixel[3] != 0)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    colours.sort_unstable();
    colours.dedup();
    for (expected, role) in [
        (TIDE, "tide product hue"),
        (CLAW, "claw accent"),
        (MIST, "mist highlight"),
        (INK, "ink body"),
    ] {
        assert!(
            colours.contains(&expected),
            "the sprite must use the {role} ({expected:?}); found {colours:?}"
        );
    }
}

#[test]
fn retina_icon_is_the_same_artwork_at_double_scale() {
    let (width_1x, height_1x, pixels_1x) = decode_png(icon_assets::PNG_BYTES);
    let (width_2x, height_2x, pixels_2x) = decode_png(icon_assets::PNG_2X_BYTES);

    assert_eq!((width_2x, height_2x), (width_1x * 2, height_1x * 2));

    // Nearest-neighbour scaling: every 2x2 block is its 1x pixel. If the 2x
    // asset were a separate drawing it would drift from the 1x icon between
    // displays, which is exactly what the retina asset exists to prevent.
    for y in 0..height_2x {
        for x in 0..width_2x {
            let source = pixels_1x[(y / 2 * width_1x + x / 2) as usize];
            let scaled = pixels_2x[(y * width_2x + x) as usize];
            assert_eq!(scaled, source, "2x pixel ({x}, {y}) is not a 2x1x pixel");
        }
    }
}

#[test]
fn windows_icon_contains_the_sizes_the_tray_asks_for() {
    // Read the asset directly: `icon_assets::ICO_BYTES` is `cfg(windows)`.
    const WINDOWS_ICON: &[u8] = include_bytes!("../assets/tray-icon.ico");
    let bytes = WINDOWS_ICON;
    assert!(bytes.len() > 6, "icon must not be empty");
    assert_eq!(&bytes[0..2], &[0, 0], "ICONDIR.reserved");
    assert_eq!(&bytes[2..4], &[1, 0], "ICONDIR.type == 1 (icon)");

    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    assert!(count >= 5, "a Windows tray icon should ship several sizes");

    let mut sizes = Vec::new();
    for index in 0..count {
        let entry = 6 + index * 16;
        let width = if bytes[entry] == 0 {
            256
        } else {
            u32::from(bytes[entry])
        };
        let height = if bytes[entry + 1] == 0 {
            256
        } else {
            u32::from(bytes[entry + 1])
        };
        sizes.push((width, height));
    }

    for expected in [16, 24, 32, 48, 64, 128, 256] {
        assert!(
            sizes.contains(&(expected, expected)),
            "the design icon must provide a {expected}x{expected} image; sizes: {sizes:?}"
        );
    }
}

// ── Menu and notification tests ────────────────────────────────────────────────

#[test]
fn the_tray_menu_offers_the_actions_a_person_wants() {
    let entries = menu::MenuState::default().entries();
    let labels: Vec<&str> = entries.iter().map(|(_, label)| label.as_str()).collect();
    for expected in [
        "Open Window",
        "New Session\u{2026}",
        "Go to Room\u{2026}",
        "Pause Dispatch",
        "Settings\u{2026}",
        "Quit",
    ] {
        assert!(
            labels.contains(&expected),
            "the tray menu is missing `{expected}`: {labels:?}"
        );
    }
    assert_eq!(entries.len(), menu::MenuAction::ALL.len());
    assert_eq!(entries[0].1, "Open Window");
}

#[test]
fn every_menu_action_has_a_keyboard_twin_in_the_window() {
    use gpui::Action as _;

    let bindings = menu::key_bindings();
    assert_eq!(bindings.len(), menu::MenuAction::ALL.len());

    let mut bound = Vec::new();
    for binding in &bindings {
        let command = binding
            .action()
            .as_any()
            .downcast_ref::<menu::MenuCommand>()
            .expect("a tray-menu binding must carry a MenuCommand");
        bound.push(command.action());
    }
    for action in menu::MenuAction::ALL {
        assert!(bound.contains(&action), "{action:?} has no keyboard twin");
        assert!(
            !action.shortcut_label().is_empty(),
            "{action:?} has no readable shortcut"
        );
    }
}

#[test]
fn only_an_opted_in_notification_kind_fires() {
    let event = notify::NotificationEvent::new(
        notify::NotificationKind::RoomMessage,
        "New message",
        "in #ops",
    );

    let mut policy = notify::NotificationPolicy::none();
    assert!(
        policy.deliver(&event).is_none(),
        "a switched-off kind must never produce a notification"
    );

    policy.opt_in(notify::NotificationKind::RoomMessage);
    let delivered = policy.deliver(&event).expect("the opted-in kind fires");
    assert_eq!(delivered.title, "New message");
}

// ── Generated-project tests ────────────────────────────────────────────────────

#[test]
fn readme_records_the_generator_and_the_ui_is_generated() {
    let readme = include_str!("../README.md").to_lowercase();
    assert!(
        readme.contains("zoid") && readme.contains("9.0.0"),
        "the README must record the Zoid version the project came from"
    );
    assert!(
        readme.contains("`tray`"),
        "the README must name the template"
    );
    assert!(
        readme.contains("src/ui/") && readme.contains("generated"),
        "the README must say src/ui/ is generated"
    );
    assert!(
        readme.contains("not hand-edited") || readme.contains("do not edit"),
        "the README must say the generated modules are not hand-edited"
    );

    let layout = include_str!("../src/ui/zml_layout.rs");
    assert!(
        layout.starts_with("// Generated by Zoid"),
        "src/ui/zml_layout.rs must keep Zoid's generated-file banner"
    );

    let app = include_str!("../ui/app.json");
    assert!(
        app.contains("\"window_title\": \"Tidepool\""),
        "ui/app.json must be the Tidepool UI source the generated modules come from"
    );
}

// ── PopoverView tests ──────────────────────────────────────────────────────────

#[gpui::test]
fn popover_view_contains_project_name(cx: &mut TestAppContext) {
    use popover_view::PopoverView;

    let entity = cx.update(|cx| cx.new(|_cx| PopoverView::new("Tidepool", "Tidepool")));

    entity.read_with(cx, |view, _| {
        assert_eq!(view.project_name.as_ref(), "Tidepool");
        assert_eq!(view.icon_tooltip.as_ref(), "Tidepool");
    });
}

#[gpui::test]
fn popover_view_renders_without_panic(cx: &mut TestAppContext) {
    use popover_view::PopoverView;

    cx.update(gpui_kit::init);

    let entity = cx.update(|cx| cx.new(|_cx| PopoverView::new("Tidepool", "Tidepool")));

    // Opening a window exercises the full render path.
    cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, _cx| entity.clone())
            .expect("open window for render test");
    });

    cx.run_until_parked();
}

// ── MainWindow tests ───────────────────────────────────────────────────────────

#[gpui::test]
fn main_window_contains_project_name(cx: &mut TestAppContext) {
    use main_window::MainWindow;

    let entity =
        cx.update(|cx| cx.new(|cx| MainWindow::new("Tidepool".into(), "Tidepool".into(), cx)));

    entity.read_with(cx, |view, _| {
        assert_eq!(view.project_name.as_ref(), "Tidepool");
        assert_eq!(view.icon_tooltip.as_ref(), "Tidepool");
    });
}
