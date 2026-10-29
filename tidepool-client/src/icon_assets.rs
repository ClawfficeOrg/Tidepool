//! The application's icon, embedded into the binary at compile time.
//!
//! This is the single place the icon's *source format* is decided. The platform
//! code in [`crate::tray_icon`] asks this module for bytes and never cares where
//! they came from, so swapping the source later — for example rasterizing an SVG
//! at build time instead of shipping pre-rendered files — means changing this
//! module only.
//!
//! The default assets are generated from `assets/icons/ZoidborgMk3-dk.svg` in
//! the Zoid repository and copied into the project's `assets/` directory by the
//! template. Replace those files to change the icon; no code change is needed
//! as long as the names stay the same.

/// Windows icon, a multi-image `.ico`.
///
/// `Shell_NotifyIconW` wants an `HICON`, which Windows builds from this by way
/// of `CreateIconFromResourceEx`, so it must be a real icon file rather than a
/// raw bitmap.
#[cfg(target_os = "windows")]
pub const ICO_BYTES: &[u8] = include_bytes!("../assets/tray-icon.ico");

/// Menubar/popover icon at 1x, a PNG.
///
/// On macOS this is a template image: AppKit tints it for the current menubar
/// appearance, so the artwork should be a solid mark rather than full-colour.
/// See [`crate::tray_icon`] for where it is marked as a template.
/// On other platforms the PNG is used directly in the popover header.
pub const PNG_BYTES: &[u8] = include_bytes!("../assets/tray-icon.png");

/// Menubar/popover icon at 2x, a PNG, for Retina / HiDPI displays.
pub const PNG_2X_BYTES: &[u8] = include_bytes!("../assets/tray-icon@2x.png");

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    #[test]
    fn embedded_windows_icon_is_a_real_ico() {
        // ICONDIR: reserved(2) == 0, type(2) == 1. Checking the magic here means
        // a mistyped asset path fails the build's test run rather than producing
        // an app with an invisible tray icon.
        assert!(super::ICO_BYTES.len() > 6, "icon must not be empty");
        assert_eq!(&super::ICO_BYTES[0..2], &[0, 0], "ICONDIR.reserved");
        assert_eq!(&super::ICO_BYTES[2..4], &[1, 0], "ICONDIR.type == 1 (icon)");
        assert!(
            super::ICO_BYTES[4] > 0,
            "icon must contain at least one image"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn embedded_macos_icons_are_pngs() {
        const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(&super::PNG_BYTES[0..8], PNG_MAGIC, "1x icon must be a PNG");
        assert_eq!(
            &super::PNG_2X_BYTES[0..8],
            PNG_MAGIC,
            "2x icon must be a PNG"
        );
    }
}
