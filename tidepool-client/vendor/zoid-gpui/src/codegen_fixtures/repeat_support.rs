//! Host-side types the `Repeat` fixture's bindings refer to.
//!
//! A generated project declares its own row types and names them from the ZML
//! document's bindings, so the fixture does the same: its `item_binding`
//! carries the path `super::repeat_support::HostEntry` rather than a type from
//! the `gpui` / `gpui-component` / `zoid_gpui` globs.  That also pins the
//! emission's contract on the item type — field access, `Clone` on the field,
//! and `Display` on whatever the `key` prop names.

use gpui::SharedString;

/// A row in the fixture's repeated list.
#[derive(Clone, Debug)]
pub struct HostEntry {
    /// Feeds a `Label` through an item-scoped `BindingRef`.
    pub hostname: SharedString,
    /// The `key` prop's field — only `Display` is required of it.
    pub line_number: usize,
    /// Feeds a `Checkbox`, exercising a `Copy` field through the same path.
    pub enabled: bool,
    /// A nested collection, which a `Repeat` inside the row iterates.
    pub members: Vec<Member>,
    /// Colors the row's `Tag` through a bound `variant`.
    pub state: gpui_kit::component::tag::TagVariant,
}

/// A row of the nested list inside a [`HostEntry`].
#[derive(Clone, Debug)]
pub struct Member {
    /// Feeds a `Label` in the inner repeat's template.
    pub name: SharedString,
}

/// The action a row's button dispatches. Its fields are filled from the row the
/// button sits in, through `handler_params` on the emitted handler.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(no_json, no_register)]
pub struct RunAlertAction {
    /// Filled from the row's `line_number` (`entries:line_number`).
    pub line_number: usize,
}

/// The action an inner row's button dispatches: one field from the inner row,
/// one from the enclosing row.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(no_json, no_register)]
pub struct RunMemberAction {
    /// Filled from `members:name`.
    pub name: SharedString,
    /// Filled from the enclosing row's `entries:line_number`.
    pub line_number: usize,
}

/// The action an `Accordion`'s `toggle` handler dispatches, carrying the
/// indices of the sections that are now open.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(no_json, no_register)]
pub struct ToggleSections {
    /// Open section indices, as the accordion reports them.
    pub open: Vec<usize>,
}
