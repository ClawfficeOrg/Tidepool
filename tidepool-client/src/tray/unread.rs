//! Unread state, kept per room and cleared only when asked.
//!
//! A busy person reads one room at a time, so "is there something unread?"
//! cannot be a single flag: a message in `#ops` must not make `#general` look
//! read. [`Unread`] is a count per room id, and there are exactly three things
//! that change it:
//!
//! - [`Unread::record_arrival`] counts an arriving message for every room
//!   except the one that is open and being watched.
//! - [`Unread::clear`] forgets one room's count. This is the explicit act:
//!   opening a room, scrolling it or redrawing it never clears anything by
//!   itself.
//! - [`Unread::clear_all`] forgets every room, for "mark everything read".
//!
//! The counts are what the status icon summarises and what the menu's room
//! entries draw, so they are stored here once rather than recomputed per
//! surface.

use std::collections::BTreeMap;

/// How many messages are unread, per room id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unread {
    rooms: BTreeMap<String, u32>,
}

impl Unread {
    /// No room has anything unread.
    pub fn new() -> Self {
        Self::default()
    }

    /// Count one arriving message in `room`.
    ///
    /// An empty room id is not a room and is ignored, so a malformed frame
    /// cannot create a phantom entry.
    pub fn mark(&mut self, room: &str) {
        if room.is_empty() {
            return;
        }
        let count = self.rooms.entry(room.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
    }

    /// Count an arriving message unless it landed in the room that is open.
    ///
    /// `open` is the room the person is currently reading; a message there is
    /// already in front of them and is not unread. Passing `None` means no room
    /// is open, so every arrival counts.
    pub fn record_arrival(&mut self, room: &str, open: Option<&str>) {
        if open == Some(room) {
            return;
        }
        self.mark(room);
    }

    /// Forget one room's unread count, explicitly.
    ///
    /// Returns whether the room had anything unread, so a caller can tell a
    /// real clear from a no-op.
    pub fn clear(&mut self, room: &str) -> bool {
        self.rooms.remove(room).is_some_and(|count| count > 0)
    }

    /// Forget every room's unread count.
    pub fn clear_all(&mut self) {
        self.rooms.clear();
    }

    /// How many messages are unread in `room`.
    pub fn count(&self, room: &str) -> u32 {
        self.rooms.get(room).copied().unwrap_or(0)
    }

    /// Whether `room` has anything unread.
    pub fn is_unread(&self, room: &str) -> bool {
        self.count(room) > 0
    }

    /// Whether no room has anything unread.
    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// How many rooms have something unread.
    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }

    /// The total unread count across rooms.
    pub fn total(&self) -> u32 {
        self.rooms.values().copied().sum()
    }

    /// The unread rooms and their counts, in room-id order.
    pub fn rooms(&self) -> impl Iterator<Item = (&str, u32)> {
        self.rooms
            .iter()
            .map(|(room, count)| (room.as_str(), *count))
    }

    /// The unread room ids, in room-id order.
    pub fn unread_rooms(&self) -> Vec<&str> {
        self.rooms.keys().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_reader_has_nothing_unread() {
        let unread = Unread::new();
        assert!(unread.is_empty());
        assert_eq!(unread.total(), 0);
        assert_eq!(unread.room_count(), 0);
        assert_eq!(unread.count("rm_ops"), 0);
        assert!(!unread.is_unread("rm_ops"));
    }

    #[test]
    fn unread_is_counted_per_room() {
        let mut unread = Unread::new();
        unread.mark("rm_ops");
        unread.mark("rm_ops");
        unread.mark("rm_general");

        assert_eq!(unread.count("rm_ops"), 2);
        assert_eq!(unread.count("rm_general"), 1);
        assert_eq!(unread.room_count(), 2);
        assert_eq!(unread.total(), 3);
        assert!(unread.is_unread("rm_ops"));
        assert!(unread.is_unread("rm_general"));
        assert!(!unread.is_unread("rm_absent"));
        assert_eq!(unread.unread_rooms(), vec!["rm_general", "rm_ops"]);
    }

    #[test]
    fn clearing_one_room_leaves_the_other_unread() {
        let mut unread = Unread::new();
        unread.mark("rm_ops");
        unread.mark("rm_general");

        assert!(unread.clear("rm_ops"));
        assert!(!unread.is_unread("rm_ops"));
        assert!(
            unread.is_unread("rm_general"),
            "clearing one room must not clear another"
        );
        assert_eq!(unread.room_count(), 1);

        assert!(!unread.clear("rm_ops"), "clearing a read room is a no-op");
        unread.clear_all();
        assert!(unread.is_empty());
        assert_eq!(unread.total(), 0);
    }

    #[test]
    fn opening_a_room_does_not_clear_it_by_itself() {
        let mut unread = Unread::new();
        unread.mark("rm_ops");

        // Reading the room is not what clears it; only `clear` is.
        assert!(unread.is_unread("rm_ops"));
        assert!(unread.clear("rm_ops"));
    }

    #[test]
    fn a_message_in_the_open_room_is_not_unread() {
        let mut unread = Unread::new();
        unread.record_arrival("rm_ops", Some("rm_ops"));
        assert!(
            !unread.is_unread("rm_ops"),
            "the open room is in front of the person"
        );

        unread.record_arrival("rm_general", Some("rm_ops"));
        assert!(unread.is_unread("rm_general"));
        assert_eq!(unread.count("rm_general"), 1);

        // With no room open, every arrival counts.
        unread.record_arrival("rm_ops", None);
        assert_eq!(unread.count("rm_ops"), 1);
    }

    #[test]
    fn a_new_message_makes_a_cleared_room_unread_again() {
        let mut unread = Unread::new();
        unread.mark("rm_ops");
        unread.clear("rm_ops");
        assert!(!unread.is_unread("rm_ops"));

        unread.mark("rm_ops");
        assert_eq!(unread.count("rm_ops"), 1);
    }

    #[test]
    fn an_empty_room_id_is_not_a_room() {
        let mut unread = Unread::new();
        unread.mark("");
        unread.record_arrival("", None);
        assert!(unread.is_empty());
        assert_eq!(unread.total(), 0);
    }
}
