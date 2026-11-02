//! Opt-in notifications, one decision per kind.
//!
//! The client may want to tell a person about an event while they are working
//! elsewhere: a session that finished, a session waiting for input, a message
//! in a room they are not reading, a run that failed, or a pool that cannot be
//! reached. Every one of those is a *kind*, and every kind is off until the
//! person switches it on.
//!
//! Two rules keep that promise mechanical:
//!
//! - [`NotificationPolicy::default`] is [`NotificationPolicy::none`]. There is
//!   no "sensible default" to opt out of, so a fresh install never interrupts
//!   anyone.
//! - [`NotificationPolicy::deliver`] is the only way an event becomes a
//!   [`Notification`], and it answers `None` for a kind the policy does not
//!   allow. A caller cannot render an event by skipping the policy, because the
//!   rendered title and body are only reachable through it.
//!
//! The policy is independent of the settings store: [`NotificationKind::key`]
//! is the stable spelling settings persists, and [`NotificationPolicy::from_keys`]
//! reads it back, ignoring keys written by a newer build.

use std::collections::BTreeSet;

/// A class of thing the client may notify about.
///
/// The declaration order is the order settings and tests list them in; it is
/// not a priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NotificationKind {
    /// An agent session finished its work.
    SessionFinished,
    /// A session is waiting for a person's input.
    SessionNeedsInput,
    /// A message arrived in a chatroom that is not the open one.
    RoomMessage,
    /// A run the pool executed failed.
    RunFailed,
    /// The pool could not be reached.
    PoolUnreachable,
}

impl NotificationKind {
    /// Every kind, in the order settings lists them.
    pub const ALL: [Self; 5] = [
        Self::SessionFinished,
        Self::SessionNeedsInput,
        Self::RoomMessage,
        Self::RunFailed,
        Self::PoolUnreachable,
    ];

    /// The stable spelling the settings file persists.
    pub fn key(self) -> &'static str {
        match self {
            Self::SessionFinished => "session_finished",
            Self::SessionNeedsInput => "session_needs_input",
            Self::RoomMessage => "room_message",
            Self::RunFailed => "run_failed",
            Self::PoolUnreachable => "pool_unreachable",
        }
    }

    /// Resolve a persisted key, refusing one this build does not know.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// The words a settings row uses.
    pub fn label(self) -> &'static str {
        match self {
            Self::SessionFinished => "A session finishes",
            Self::SessionNeedsInput => "A session is waiting for you",
            Self::RoomMessage => "A new message in a room",
            Self::RunFailed => "A run fails",
            Self::PoolUnreachable => "The pool cannot be reached",
        }
    }
}

/// The kinds a person has switched on.
///
/// Empty by default: notifications are opt-in, not opt-out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationPolicy {
    enabled: BTreeSet<NotificationKind>,
}

impl NotificationPolicy {
    /// A policy that fires nothing, which is also [`Default`].
    pub fn none() -> Self {
        Self::default()
    }

    /// A policy that fires every kind. Used by tests and by an explicit
    /// "notify me about everything" choice; never by a default.
    pub fn all() -> Self {
        Self {
            enabled: NotificationKind::ALL.into_iter().collect(),
        }
    }

    /// Switch one kind on.
    pub fn opt_in(&mut self, kind: NotificationKind) {
        self.enabled.insert(kind);
    }

    /// Switch one kind off.
    pub fn opt_out(&mut self, kind: NotificationKind) {
        self.enabled.remove(&kind);
    }

    /// Switch one kind to an explicit state.
    pub fn set(&mut self, kind: NotificationKind, enabled: bool) {
        if enabled {
            self.opt_in(kind);
        } else {
            self.opt_out(kind);
        }
    }

    /// Whether the person has switched this kind on.
    pub fn allows(&self, kind: NotificationKind) -> bool {
        self.enabled.contains(&kind)
    }

    /// Whether nothing at all is switched on.
    pub fn is_empty(&self) -> bool {
        self.enabled.is_empty()
    }

    /// The switched-on kinds, in the order settings lists them.
    pub fn enabled(&self) -> Vec<NotificationKind> {
        NotificationKind::ALL
            .into_iter()
            .filter(|kind| self.allows(*kind))
            .collect()
    }

    /// The persistence keys of the switched-on kinds, in settings order.
    pub fn keys(&self) -> Vec<&'static str> {
        self.enabled()
            .into_iter()
            .map(NotificationKind::key)
            .collect()
    }

    /// Read a policy back from persisted keys.
    ///
    /// A key this build does not know is ignored rather than treated as an
    /// error, so a settings file written by a newer client still opens.
    pub fn from_keys<'a>(keys: impl IntoIterator<Item = &'a str>) -> Self {
        let mut policy = Self::none();
        for key in keys {
            if let Some(kind) = NotificationKind::from_key(key) {
                policy.opt_in(kind);
            }
        }
        policy
    }

    /// Turn one event into a notification, or drop it because its kind is off.
    ///
    /// This is the only path to a rendered [`Notification`]: an event whose
    /// kind the person switched off never produces one, whatever the caller
    /// does with the result.
    pub fn deliver(&self, event: &NotificationEvent) -> Option<Notification> {
        if !self.allows(event.kind) {
            return None;
        }
        Some(Notification {
            kind: event.kind,
            title: event.title.clone(),
            body: event.body.clone(),
        })
    }

    /// Deliver every event whose kind is switched on, keeping their order.
    pub fn deliver_all<'a>(
        &'a self,
        events: impl IntoIterator<Item = &'a NotificationEvent>,
    ) -> Vec<Notification> {
        events
            .into_iter()
            .filter_map(|event| self.deliver(event))
            .collect()
    }
}

/// Something that happened and might be worth a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationEvent {
    /// Which class of event this is; the policy decides from this alone.
    pub kind: NotificationKind,
    /// The one-line title.
    pub title: String,
    /// The body, already written in the person's words.
    pub body: String,
    /// The room the event belongs to, when it belongs to one.
    pub room: Option<String>,
}

impl NotificationEvent {
    /// Build an event with a title and a body.
    pub fn new(kind: NotificationKind, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            body: body.into(),
            room: None,
        }
    }

    /// Attach the room this event belongs to.
    pub fn in_room(mut self, room: impl Into<String>) -> Self {
        self.room = Some(room.into());
        self
    }
}

/// An event a policy allowed, ready to be handed to the platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// The kind that was allowed.
    pub kind: NotificationKind,
    /// The one-line title.
    pub title: String,
    /// The body.
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: NotificationKind) -> NotificationEvent {
        NotificationEvent::new(kind, "Title", "Body")
    }

    #[test]
    fn a_fresh_policy_fires_nothing() {
        let policy = NotificationPolicy::default();
        assert_eq!(policy, NotificationPolicy::none());
        assert!(policy.is_empty());
        for kind in NotificationKind::ALL {
            assert!(!policy.allows(kind), "{kind:?} must start switched off");
            assert!(policy.deliver(&event(kind)).is_none());
        }
        let every_kind: Vec<NotificationEvent> =
            NotificationKind::ALL.into_iter().map(event).collect();
        assert!(policy.deliver_all(every_kind.iter()).is_empty());
    }

    #[test]
    fn only_the_kind_that_was_switched_on_fires() {
        let mut policy = NotificationPolicy::none();
        policy.opt_in(NotificationKind::SessionFinished);

        let allowed = policy
            .deliver(&event(NotificationKind::SessionFinished))
            .expect("an opted-in kind must fire");
        assert_eq!(allowed.kind, NotificationKind::SessionFinished);
        assert_eq!(allowed.title, "Title");
        assert_eq!(allowed.body, "Body");

        for kind in NotificationKind::ALL {
            if kind == NotificationKind::SessionFinished {
                continue;
            }
            assert!(
                policy.deliver(&event(kind)).is_none(),
                "{kind:?} was never switched on"
            );
        }
    }

    #[test]
    fn switching_a_kind_off_stops_it_again() {
        let mut policy = NotificationPolicy::all();
        assert!(
            policy
                .deliver(&event(NotificationKind::RoomMessage))
                .is_some()
        );

        policy.opt_out(NotificationKind::RoomMessage);
        assert!(!policy.allows(NotificationKind::RoomMessage));
        assert!(
            policy
                .deliver(&event(NotificationKind::RoomMessage))
                .is_none()
        );
    }

    #[test]
    fn deliver_all_keeps_order_and_drops_switched_off_kinds() {
        let mut policy = NotificationPolicy::none();
        policy.opt_in(NotificationKind::SessionNeedsInput);
        policy.opt_in(NotificationKind::RunFailed);

        let events = vec![
            event(NotificationKind::RoomMessage),
            NotificationEvent::new(NotificationKind::SessionNeedsInput, "b", "b").in_room("r1"),
            event(NotificationKind::RunFailed),
        ];
        let delivered = policy.deliver_all(&events);
        assert_eq!(
            delivered.iter().map(|one| one.kind).collect::<Vec<_>>(),
            vec![
                NotificationKind::SessionNeedsInput,
                NotificationKind::RunFailed
            ]
        );
    }

    #[test]
    fn keys_round_trip_through_persistence() {
        let mut policy = NotificationPolicy::none();
        policy.opt_in(NotificationKind::PoolUnreachable);
        policy.opt_in(NotificationKind::SessionFinished);
        let keys = policy.keys();
        assert_eq!(keys, vec!["session_finished", "pool_unreachable"]);
        assert_eq!(NotificationPolicy::from_keys(keys), policy);
    }

    #[test]
    fn an_unknown_persisted_key_is_ignored() {
        let policy = NotificationPolicy::from_keys(["run_failed", "from_a_newer_build"]);
        assert_eq!(policy.enabled(), vec![NotificationKind::RunFailed]);
    }

    #[test]
    fn enabled_kinds_follow_the_settings_order() {
        assert_eq!(
            NotificationKind::ALL.map(NotificationKind::key),
            [
                "session_finished",
                "session_needs_input",
                "room_message",
                "run_failed",
                "pool_unreachable"
            ]
        );
        assert_eq!(
            NotificationKind::ALL.map(NotificationKind::label),
            [
                "A session finishes",
                "A session is waiting for you",
                "A new message in a room",
                "A run fails",
                "The pool cannot be reached"
            ]
        );
        assert!(NotificationKind::from_key("nope").is_none());
    }

    #[test]
    fn set_is_the_explicit_form_of_opt_in_and_out() {
        let mut policy = NotificationPolicy::none();
        policy.set(NotificationKind::RunFailed, true);
        assert!(policy.allows(NotificationKind::RunFailed));
        policy.set(NotificationKind::RunFailed, false);
        assert!(!policy.allows(NotificationKind::RunFailed));
    }
}
