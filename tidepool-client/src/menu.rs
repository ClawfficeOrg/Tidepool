//! The menu a person sees without opening the window, and its keyboard twin.
//!
//! The same actions exist twice on purpose: as entries in the status item's
//! native menu ([`MenuState::entries`]) and as key bindings in every window
//! ([`key_bindings`]). One [`MenuAction`] describes both, so a new action cannot
//! be added to the menu without a shortcut to reach it, and the two can never
//! drift apart. [`install`] registers the bindings with the app; pressing one
//! dispatches [`MenuCommand`], which carries the action as data so one handler
//! serves all of them.
//!
//! # The rooms are menu items, in a reserved id range
//!
//! The platform menu seam takes flat `(id, label)` pairs, so "go to a room" is
//! both a fixed action that opens the room picker and one direct entry per room
//! the person belongs to ([`ROOM_ID_BASE`] upward). Action ids stay below the
//! base, which is what keeps the two ranges from colliding as either grows.
//!
//! # Pausing is a label, not a second entry
//!
//! [`MenuAction::PauseDispatch`] keeps its id and changes its words:
//! "Pause Dispatch" while dispatch is running, "Resume Dispatch" while it is
//! paused. A caller that dispatches the id does not need to know which state
//! it was read in.

use gpui::{Action as _, App, KeyBinding, Result};

/// The id the first room entry gets, one past the last fixed action.
///
/// Fixed actions use [`MenuAction::id`] (0 through 4), so this is the boundary
/// between the two id spaces.
pub const ROOM_ID_BASE: u32 = 100;

/// One thing the menu can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MenuAction {
    /// Bring the main window forward.
    OpenWindow,
    /// Ask for a new agent session.
    NewSession,
    /// Go to a chatroom.
    GoToRoom,
    /// Pause manual dispatch, or resume it when it is paused.
    PauseDispatch,
    /// Open the settings window.
    OpenSettings,
    /// Quit the client.
    Quit,
}

impl MenuAction {
    /// Every action, in the order the menu lists them.
    pub const ALL: [Self; 6] = [
        Self::OpenWindow,
        Self::NewSession,
        Self::GoToRoom,
        Self::PauseDispatch,
        Self::OpenSettings,
        Self::Quit,
    ];

    /// The menu entry's id. Stable across launches and below [`ROOM_ID_BASE`].
    pub fn id(self) -> u32 {
        match self {
            Self::OpenWindow => 0,
            Self::NewSession => 1,
            Self::GoToRoom => 2,
            Self::PauseDispatch => 3,
            Self::OpenSettings => 4,
            Self::Quit => 5,
        }
    }

    /// Resolve a menu entry id back to its action.
    pub fn from_id(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }

    /// The stable name the action is bound and dispatched under.
    pub fn action_name(self) -> &'static str {
        match self {
            Self::OpenWindow => "open_window",
            Self::NewSession => "new_session",
            Self::GoToRoom => "go_to_room",
            Self::PauseDispatch => "pause_dispatch",
            Self::OpenSettings => "open_settings",
            Self::Quit => "quit",
        }
    }

    /// Resolve an action name back to its action.
    pub fn from_action_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.action_name() == name)
    }

    /// The menu entry's words in the unpaused state.
    ///
    /// Use [`MenuAction::label_for`] for an entry whose wording depends on the
    /// dispatch state.
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenWindow => "Open Window",
            Self::NewSession => "New Session\u{2026}",
            Self::GoToRoom => "Go to Room\u{2026}",
            Self::PauseDispatch => "Pause Dispatch",
            Self::OpenSettings => "Settings\u{2026}",
            Self::Quit => "Quit",
        }
    }

    /// The menu entry's words in a given dispatch state.
    pub fn label_for(self, paused: bool) -> String {
        match self {
            Self::PauseDispatch if paused => "Resume Dispatch".to_owned(),
            _ => self.label().to_owned(),
        }
    }

    /// The keystroke spec that reaches this action from a window.
    ///
    /// `secondary` is the suite's spelling for Command on macOS and Control on
    /// Windows and Linux, so one binding serves every platform.
    pub fn keystroke(self) -> &'static str {
        match self {
            Self::OpenWindow => "secondary-o",
            Self::NewSession => "secondary-n",
            Self::GoToRoom => "secondary-k",
            Self::PauseDispatch => "secondary-shift-p",
            Self::OpenSettings => "secondary-,",
            Self::Quit => "secondary-q",
        }
    }

    /// The shortcut as a person reads it on this platform.
    ///
    /// The modifiers are in the platform's canonical order; an unparseable
    /// spec (which a test forbids) renders as an empty string rather than
    /// panicking a menu row.
    pub fn shortcut_label(self) -> String {
        let Ok(keystroke) = gpui::Keystroke::parse(self.keystroke()) else {
            return String::new();
        };
        let macos = cfg!(target_os = "macos");
        let mut label = String::new();
        if keystroke.modifiers.control {
            label.push_str(if macos { "\u{2303}" } else { "Ctrl+" });
        }
        if keystroke.modifiers.alt {
            label.push_str(if macos { "\u{2325}" } else { "Alt+" });
        }
        if keystroke.modifiers.shift {
            label.push_str(if macos { "\u{21E7}" } else { "Shift+" });
        }
        if keystroke.modifiers.platform {
            label.push_str(if macos { "\u{2318}" } else { "Super+" });
        }
        label.push_str(&keystroke.key.to_uppercase());
        label
    }
}

/// One room the menu offers direct access to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuRoom {
    /// The room's immutable public id.
    pub id: String,
    /// The room's display name.
    pub name: String,
}

impl MenuRoom {
    /// Build a room entry.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    /// The room's words on a menu row: `#name`, falling back to its id.
    ///
    /// A name that already carries a leading `#` is not given a second one.
    pub fn menu_label(&self) -> String {
        let name = self.name.trim();
        let name = if name.is_empty() { &self.id } else { name };
        if name.starts_with('#') {
            name.to_owned()
        } else {
            format!("#{name}")
        }
    }
}

/// What a chosen menu id means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuChoice {
    /// A fixed action.
    Action(MenuAction),
    /// A room's id, from a direct room entry.
    Room(String),
}

impl MenuChoice {
    /// The fixed action this choice carries, when it carries one.
    pub fn action(&self) -> Option<MenuAction> {
        match self {
            Self::Action(action) => Some(*action),
            Self::Room(_) => None,
        }
    }
}

/// The menu's content: its rooms and whether dispatch is paused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuState {
    rooms: Vec<MenuRoom>,
    dispatch_paused: bool,
}

impl MenuState {
    /// Build a menu over the given rooms.
    ///
    /// Rooms with an empty id are unusable and skipped, and a repeated id is
    /// listed once (the first entry wins), so a menu cannot carry two rows
    /// that resolve to the same room.
    pub fn new(rooms: impl IntoIterator<Item = MenuRoom>) -> Self {
        let mut state = Self::default();
        state.set_rooms(rooms);
        state
    }

    /// Replace the room list, applying the same deduplication as [`Self::new`].
    pub fn set_rooms(&mut self, rooms: impl IntoIterator<Item = MenuRoom>) {
        self.rooms.clear();
        for room in rooms {
            if room.id.is_empty() {
                continue;
            }
            if self.rooms.iter().any(|known| known.id == room.id) {
                continue;
            }
            self.rooms.push(room);
        }
    }

    /// The rooms the menu lists, in menu order.
    pub fn rooms(&self) -> &[MenuRoom] {
        &self.rooms
    }

    /// Whether manual dispatch is paused.
    pub fn dispatch_paused(&self) -> bool {
        self.dispatch_paused
    }

    /// Set the dispatch state the Pause/Resume row words itself from.
    pub fn set_dispatch_paused(&mut self, paused: bool) {
        self.dispatch_paused = paused;
    }

    /// Flip the dispatch state, returning the new one.
    pub fn toggle_dispatch_pause(&mut self) -> bool {
        self.dispatch_paused = !self.dispatch_paused;
        self.dispatch_paused
    }

    /// The `(id, label)` pairs the native status-item menu installs.
    ///
    /// The fixed actions keep their order; direct room entries follow "Go to
    /// Room…" and take ids from [`ROOM_ID_BASE`] upward.
    pub fn entries(&self) -> Vec<(u32, String)> {
        let mut entries = Vec::with_capacity(MenuAction::ALL.len() + self.rooms.len());
        for action in MenuAction::ALL {
            entries.push((action.id(), action.label_for(self.dispatch_paused)));
            if action == MenuAction::GoToRoom {
                entries.extend(self.room_entries());
            }
        }
        entries
    }

    /// The direct room entries, as `(id, label)` pairs.
    pub fn room_entries(&self) -> Vec<(u32, String)> {
        self.rooms
            .iter()
            .enumerate()
            .map(|(index, room)| {
                (
                    ROOM_ID_BASE + index as u32,
                    format!("Go to {}", room.menu_label()),
                )
            })
            .collect()
    }

    /// Resolve a chosen menu id to an action or a room.
    pub fn resolve(&self, id: u32) -> Option<MenuChoice> {
        if let Some(action) = MenuAction::from_id(id) {
            return Some(MenuChoice::Action(action));
        }
        let index = usize::try_from(id.checked_sub(ROOM_ID_BASE)?).ok()?;
        self.rooms
            .get(index)
            .map(|room| MenuChoice::Room(room.id.clone()))
    }

    /// The direct menu id for a room, when the menu lists it.
    pub fn id_for_room(&self, room: &str) -> Option<u32> {
        self.rooms
            .iter()
            .position(|known| known.id == room)
            .map(|index| ROOM_ID_BASE + index as u32)
    }
}

/// A [`MenuAction`] as a GPUI action, so one handler serves the keyboard.
///
/// The action travels as data ([`MenuCommand::action`]) rather than as five
/// separate unit types, which keeps the binding table and the menu table fed
/// from the same [`MenuAction::ALL`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuCommand {
    action: MenuAction,
}

impl MenuCommand {
    /// Wrap an action for dispatch.
    pub fn new(action: MenuAction) -> Self {
        Self { action }
    }

    /// The action this command carries.
    pub fn action(&self) -> MenuAction {
        self.action
    }
}

impl gpui::Action for MenuCommand {
    fn boxed_clone(&self) -> Box<dyn gpui::Action> {
        Box::new(self.clone())
    }

    fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| other.action == self.action)
    }

    fn name(&self) -> &'static str {
        self.action.action_name()
    }

    fn name_for_type() -> &'static str {
        "tidepool_menu_command"
    }

    fn build(value: gpui::private::serde_json::Value) -> Result<Box<dyn gpui::Action>> {
        let name = value
            .get("action")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let action = MenuAction::from_action_name(name)
            .ok_or_else(|| gpui::private::anyhow::anyhow!("`{name}` is not a menu action"))?;
        Ok(Box::new(Self::new(action)))
    }
}

/// The key bindings that reach every menu action from a window.
///
/// One binding per [`MenuAction`], generated from the same table the menu uses,
/// so the two cannot list different actions.
pub fn key_bindings() -> Vec<KeyBinding> {
    MenuAction::ALL
        .into_iter()
        .map(|action| KeyBinding::new(action.keystroke(), MenuCommand::new(action), None))
        .collect()
}

/// Register the menu's keyboard twins with the app.
///
/// Call once at startup, before any window opens. The handler for
/// [`MenuCommand`] is the caller's: this only installs the table.
pub fn install(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rooms() -> Vec<MenuRoom> {
        vec![
            MenuRoom::new("rm_general", "general"),
            MenuRoom::new("rm_ops", "ops"),
        ]
    }

    #[test]
    fn every_action_has_a_unique_id_and_name() {
        let ids: Vec<u32> = MenuAction::ALL.iter().map(|action| action.id()).collect();
        let names: Vec<&str> = MenuAction::ALL
            .iter()
            .map(|action| action.action_name())
            .collect();
        for (index, action) in MenuAction::ALL.iter().enumerate() {
            assert_eq!(MenuAction::from_id(action.id()), Some(*action));
            assert_eq!(
                MenuAction::from_action_name(action.action_name()),
                Some(*action)
            );
            assert_eq!(ids[index], index as u32, "action ids must be dense");
            assert!(
                action.id() < ROOM_ID_BASE,
                "a fixed action id must stay below the room range"
            );
            assert!(
                !names[..index].contains(&action.action_name()),
                "{action:?} repeats an action name"
            );
        }
        assert_eq!(MenuAction::from_id(MenuAction::ALL.len() as u32), None);
        assert_eq!(MenuAction::from_action_name("nope"), None);
    }

    #[test]
    fn every_menu_action_has_a_distinct_keyboard_twin() {
        let mut specs: Vec<&str> = Vec::new();
        for action in MenuAction::ALL {
            let spec = action.keystroke();
            let keystroke = gpui::Keystroke::parse(spec)
                .unwrap_or_else(|error| panic!("{action:?}'s keystroke `{spec}`: {error}"));
            assert!(
                keystroke.modifiers.platform || keystroke.modifiers.control,
                "{action:?} must carry the platform modifier so it is a shortcut, not a bare key"
            );
            assert!(!keystroke.key.is_empty(), "{action:?} has no key");
            assert!(
                !specs.contains(&spec),
                "{action:?} repeats the shortcut {spec}"
            );
            specs.push(spec);
        }
    }

    #[test]
    fn the_bindings_cover_exactly_the_menu_actions() {
        let bindings = key_bindings();
        assert_eq!(bindings.len(), MenuAction::ALL.len());

        let mut bound: Vec<MenuAction> = Vec::new();
        for binding in &bindings {
            let command = binding
                .action()
                .as_any()
                .downcast_ref::<MenuCommand>()
                .unwrap_or_else(|| panic!("{binding:?} is not a MenuCommand"));
            assert_eq!(binding.action().name(), command.action().action_name());
            bound.push(command.action());
        }
        for action in MenuAction::ALL {
            assert!(bound.contains(&action), "{action:?} has no key binding");
        }
    }

    #[test]
    fn a_shortcut_label_names_the_modifier_and_the_key() {
        let open = MenuAction::OpenWindow.shortcut_label();
        let pause = MenuAction::PauseDispatch.shortcut_label();
        let settings = MenuAction::OpenSettings.shortcut_label();
        assert!(open.ends_with('O'), "got {open:?}");
        assert!(pause.ends_with('P'), "got {pause:?}");
        assert!(settings.ends_with(','), "got {settings:?}");
        if cfg!(target_os = "macos") {
            assert_eq!(open, "\u{2318}O", "Command on macOS");
            assert_eq!(pause, "\u{21E7}\u{2318}P", "shift before Command on macOS");
            assert_eq!(settings, "\u{2318},");
        } else {
            assert_eq!(open, "Ctrl+O");
            assert_eq!(pause, "Ctrl+Shift+P");
            assert_eq!(settings, "Ctrl+,");
        }
    }

    #[test]
    fn labels_say_what_the_menu_does() {
        assert_eq!(MenuAction::OpenWindow.label(), "Open Window");
        assert_eq!(MenuAction::NewSession.label(), "New Session\u{2026}");
        assert_eq!(MenuAction::GoToRoom.label(), "Go to Room\u{2026}");
        assert_eq!(MenuAction::PauseDispatch.label_for(false), "Pause Dispatch");
        assert_eq!(MenuAction::PauseDispatch.label_for(true), "Resume Dispatch");
        assert_eq!(MenuAction::OpenSettings.label(), "Settings\u{2026}");
        assert_eq!(MenuAction::Quit.label(), "Quit");
    }

    #[test]
    fn the_menu_starts_with_open_window_and_lists_every_action() {
        let entries = MenuState::default().entries();
        assert_eq!(entries.len(), MenuAction::ALL.len());
        assert_eq!(entries[0].0, MenuAction::OpenWindow.id());
        assert_eq!(entries[0].1, "Open Window");
        for action in MenuAction::ALL {
            assert!(
                entries
                    .iter()
                    .any(|(id, label)| *id == action.id() && label == &action.label_for(false)),
                "{action:?} is missing from the menu"
            );
        }
    }

    #[test]
    fn a_paused_menu_offers_resume_under_the_same_id() {
        let mut state = MenuState::default();
        let id = MenuAction::PauseDispatch.id();
        assert_eq!(state.entries()[id as usize].1, "Pause Dispatch");

        assert!(state.toggle_dispatch_pause());
        assert!(state.dispatch_paused());
        assert_eq!(state.entries()[id as usize].1, "Resume Dispatch");

        assert!(!state.toggle_dispatch_pause());
        assert_eq!(state.entries()[id as usize].1, "Pause Dispatch");
    }

    #[test]
    fn rooms_are_direct_entries_after_go_to_room() {
        let state = MenuState::new(rooms());
        let entries = state.entries();
        let open = MenuAction::OpenWindow.id() as usize;
        let new = MenuAction::NewSession.id() as usize;
        let go = MenuAction::GoToRoom.id() as usize;
        assert_eq!(entries[open].1, "Open Window");
        assert_eq!(entries[new].1, "New Session\u{2026}");
        assert_eq!(entries[go].1, "Go to Room\u{2026}");
        assert_eq!(entries[go + 1].0, ROOM_ID_BASE);
        assert_eq!(entries[go + 1].1, "Go to #general");
        assert_eq!(entries[go + 2].0, ROOM_ID_BASE + 1);
        assert_eq!(entries[go + 2].1, "Go to #ops");

        assert_eq!(
            state.resolve(ROOM_ID_BASE),
            Some(MenuChoice::Room("rm_general".to_owned()))
        );
        assert_eq!(
            state.resolve(ROOM_ID_BASE + 1),
            Some(MenuChoice::Room("rm_ops".to_owned()))
        );
        assert_eq!(state.id_for_room("rm_ops"), Some(ROOM_ID_BASE + 1));
    }

    #[test]
    fn a_room_entry_resolves_to_its_room_and_nothing_else() {
        let state = MenuState::new(rooms());
        assert_eq!(
            state.resolve(MenuAction::Quit.id()),
            Some(MenuChoice::Action(MenuAction::Quit))
        );
        assert_eq!(
            state.resolve(ROOM_ID_BASE + 2),
            None,
            "no third room exists"
        );
        assert_eq!(
            state
                .resolve(ROOM_ID_BASE + 1)
                .and_then(|choice| choice.action()),
            None
        );
        assert_eq!(state.id_for_room("absent"), None);
    }

    #[test]
    fn unusable_and_repeated_rooms_are_not_listed_twice() {
        let state = MenuState::new([
            MenuRoom::new("", "nameless"),
            MenuRoom::new("rm_general", "general"),
            MenuRoom::new("rm_general", "general again"),
        ]);
        assert_eq!(state.rooms(), &[MenuRoom::new("rm_general", "general")]);
        assert_eq!(state.room_entries().len(), 1);
    }

    #[test]
    fn a_room_label_falls_back_to_its_id_and_keeps_one_hash() {
        assert_eq!(MenuRoom::new("rm_1", "").menu_label(), "#rm_1");
        assert_eq!(MenuRoom::new("rm_1", "  ops  ").menu_label(), "#ops");
        assert_eq!(MenuRoom::new("rm_1", "#already").menu_label(), "#already");
    }

    #[test]
    fn a_command_round_trips_through_the_keymap_json() {
        let value = gpui::private::serde_json::json!({ "action": "new_session" });
        let built = MenuCommand::build(value).expect("a known action builds");
        let command = built
            .as_any()
            .downcast_ref::<MenuCommand>()
            .expect("the built action is a MenuCommand");
        assert_eq!(command.action(), MenuAction::NewSession);

        let unknown = gpui::private::serde_json::json!({ "action": "launch_missiles" });
        assert!(MenuCommand::build(unknown).is_err());
    }

    #[test]
    fn a_command_only_equals_the_same_action() {
        let open = MenuCommand::new(MenuAction::OpenWindow);
        assert_eq!(open, MenuCommand::new(MenuAction::OpenWindow));
        assert_ne!(open, MenuCommand::new(MenuAction::Quit));
        assert!(open.partial_eq(&MenuCommand::new(MenuAction::OpenWindow)));
        assert!(!open.partial_eq(&MenuCommand::new(MenuAction::Quit)));
        assert_eq!(open.name(), "open_window");
        assert_eq!(open.boxed_clone().name(), "open_window");
    }

    /// A minimal view whose only purpose is to give a test window somewhere to
    /// dispatch a keystroke.
    struct FocusView;

    impl gpui::Render for FocusView {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::div()
        }
    }

    #[gpui::test]
    fn every_shortcut_dispatches_its_action_inside_a_window(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext as _;
        use std::cell::RefCell;
        use std::rc::Rc;

        let fired = Rc::new(RefCell::new(Vec::new()));
        let sink = fired.clone();
        cx.update(|cx| {
            install(cx);
            cx.on_action(move |command: &MenuCommand, _cx: &mut App| {
                sink.borrow_mut().push(command.action());
            });
        });

        let window = cx.update(|cx| {
            let view = cx.new(|_| FocusView);
            cx.open_window(gpui::WindowOptions::default(), |_, _| view)
                .expect("open a window for the keystroke test")
        });
        let window: gpui::AnyWindowHandle = window.into();

        for action in MenuAction::ALL {
            cx.simulate_keystrokes(window, action.keystroke());
        }

        // One keystroke per action, in order, each the action its menu row
        // runs. This is the window's keyboard path, not the binding table.
        assert_eq!(*fired.borrow(), MenuAction::ALL.to_vec());
    }
}
