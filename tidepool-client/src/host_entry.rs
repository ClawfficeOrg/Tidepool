//! Row type for the tray popover's repeat-driven host list.
//!
//! The popover's editable region (rendered by the generated `zml_layout`) is a
//! `Repeat` over the `entries` collection binding, so this is the element type
//! `Vec<HostEntry>` carries. The UI source's binding `value_type` names it as
//! `crate::host_entry::HostEntry`, which is why it lives in its own module —
//! both the binary (`main.rs`) and the integration test (`tests/tray.rs`) mount
//! it via `#[path]`, so the path resolves in both crates.

use gpui::SharedString;

/// One row of the popover's host list.
#[derive(Clone, Debug)]
pub struct HostEntry {
    /// Host name, shown in the row's label.
    pub hostname: SharedString,
    /// Row number, used as the `Repeat`'s `key` — the per-row element id and
    /// the per-row `InputState` handle key.
    pub line_number: usize,
}

/// Sample rows so a freshly generated project renders a non-empty list.
///
/// Replace this with real data from the host application; the `Repeat` renders
/// whatever `ZmlBindings.entries` holds.
pub fn seed_entries() -> Vec<HostEntry> {
    Vec::new()
}
