//! What the status icon shows, and the state that decides it.
//!
//! A menu-bar icon cannot draw three answers at once, so it carries the most
//! urgent one and the tooltip carries all of them. The precedence is fixed and
//! documented here rather than left to a caller:
//!
//! 1. [`StatusIcon::Unreachable`] — the pool could not be reached. Nothing this
//!    client shows is confirmed while that is true, so it outranks activity.
//! 2. [`StatusIcon::AgentsRunning`] — at least one agent session is working on
//!    this machine.
//! 3. [`StatusIcon::Unread`] — a room has a message waiting. Unread loses to a
//!    running agent only because the work in flight is the thing that will ask
//!    for the person next; the unread count stays in the tooltip.
//! 4. [`StatusIcon::Idle`] — nothing to say.
//!
//! Each variant has its own word and its own colour, so the state is legible
//! with colour stripped out (the label is what the tooltip and accessibility
//! read) and distinguishable when it is not.

use crate::tray::unread::Unread;

/// Whether the pool could be reached the last time this client asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PoolState {
    /// No read has succeeded or failed yet, so nothing is claimed.
    #[default]
    Unchecked,
    /// The last read reached the pool.
    Reachable,
    /// The last read did not reach the pool.
    Unreachable,
}

impl PoolState {
    /// The words the tooltip uses.
    pub fn label(self) -> &'static str {
        match self {
            Self::Unchecked => "pool not checked yet",
            Self::Reachable => "pool reachable",
            Self::Unreachable => "the pool cannot be reached",
        }
    }

    /// Whether the pool is known to be away.
    pub fn is_unreachable(self) -> bool {
        self == Self::Unreachable
    }
}

/// Everything the always-present surface knows, in one place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayState {
    pool: PoolState,
    running: usize,
    unread: Unread,
}

impl TrayState {
    /// A state with no known activity and an unchecked pool.
    pub fn new(pool: PoolState) -> Self {
        Self {
            pool,
            running: 0,
            unread: Unread::new(),
        }
    }

    /// Whether the pool is reachable.
    pub fn pool(&self) -> PoolState {
        self.pool
    }

    /// Record the result of the latest pool read.
    pub fn set_pool(&mut self, pool: PoolState) {
        self.pool = pool;
    }

    /// How many agent sessions are working on this machine.
    pub fn running(&self) -> usize {
        self.running
    }

    /// Record how many agent sessions are working.
    pub fn set_running(&mut self, running: usize) {
        self.running = running;
    }

    /// The per-room unread state.
    pub fn unread(&self) -> &Unread {
        &self.unread
    }

    /// The per-room unread state, mutable, for arrivals and explicit clears.
    pub fn unread_mut(&mut self) -> &mut Unread {
        &mut self.unread
    }

    /// The icon this state draws.
    pub fn icon(&self) -> StatusIcon {
        StatusIcon::for_state(self)
    }

    /// The tooltip: every fact behind the single icon, in words.
    pub fn tooltip(&self) -> String {
        let mut parts = vec![self.pool.label().to_owned()];
        parts.push(match self.running {
            0 => "no sessions running".to_owned(),
            1 => "1 session running".to_owned(),
            count => format!("{count} sessions running"),
        });
        if !self.unread.is_empty() {
            parts.push(format!(
                "{} unread in {} {}",
                self.unread.total(),
                self.unread.room_count(),
                if self.unread.room_count() == 1 {
                    "room"
                } else {
                    "rooms"
                }
            ));
        }
        format!("Tidepool \u{2014} {}", parts.join(" \u{00B7} "))
    }

    /// What a screen reader hears for the icon, independent of its colour.
    pub fn accessible_label(&self) -> String {
        format!("{}: {}", self.icon().label(), self.tooltip())
    }
}

/// The four things the status icon can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusIcon {
    /// The pool could not be reached.
    Unreachable,
    /// At least one agent session is working.
    AgentsRunning,
    /// A room has an unread message.
    Unread,
    /// Nothing needs the person's attention.
    Idle,
}

impl StatusIcon {
    /// The state's icon, applying the precedence documented on this module.
    pub fn for_state(state: &TrayState) -> Self {
        if state.pool.is_unreachable() {
            Self::Unreachable
        } else if state.running > 0 {
            Self::AgentsRunning
        } else if !state.unread.is_empty() {
            Self::Unread
        } else {
            Self::Idle
        }
    }

    /// The state in one word, for the tooltip and for a colourless reading.
    pub fn label(self) -> &'static str {
        match self {
            Self::Unreachable => "Pool unreachable",
            Self::AgentsRunning => "Agents running",
            Self::Unread => "Unread message",
            Self::Idle => "Idle",
        }
    }

    /// The tint the status item applies to its template artwork.
    ///
    /// Idle is neutral grey, running is the product hue (`tide-500`), unread is
    /// the accent (`claw-500`) and unreachable is a red that is deliberately
    /// darker than the accent so the two do not read as the same alarm.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Unreachable => (0xD9, 0x3A, 0x3A),
            Self::AgentsRunning => (0x4A, 0x9F, 0xE0),
            Self::Unread => (0xF2, 0x6B, 0x21),
            Self::Idle => (0x8A, 0x8A, 0x8A),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(pool: PoolState) -> TrayState {
        TrayState::new(pool)
    }

    #[test]
    fn an_untouched_surface_is_idle() {
        let state = TrayState::default();
        assert_eq!(state.pool(), PoolState::Unchecked);
        assert_eq!(state.icon(), StatusIcon::Idle);
        assert_eq!(state.running(), 0);
        assert!(state.unread().is_empty());
    }

    #[test]
    fn the_icon_distinguishes_unreachable_idle_and_running() {
        let idle = state(PoolState::Reachable);
        let unreachable = state(PoolState::Unreachable);
        let mut running = state(PoolState::Reachable);
        running.set_running(2);

        assert_eq!(idle.icon(), StatusIcon::Idle);
        assert_eq!(unreachable.icon(), StatusIcon::Unreachable);
        assert_eq!(running.icon(), StatusIcon::AgentsRunning);
        assert_ne!(idle.icon(), unreachable.icon());
        assert_ne!(idle.icon(), running.icon());
        assert_ne!(unreachable.icon(), running.icon());
    }

    #[test]
    fn an_unread_room_shows_until_it_is_cleared() {
        let mut state = state(PoolState::Reachable);
        state.unread_mut().mark("rm_ops");
        assert_eq!(state.icon(), StatusIcon::Unread);

        // Reading it is not clearing it: the icon stays unread until the clear.
        assert_eq!(state.icon(), StatusIcon::Unread);
        assert!(state.unread_mut().clear("rm_ops"));
        assert_eq!(state.icon(), StatusIcon::Idle);
    }

    #[test]
    fn an_unreachable_pool_outranks_running_agents_and_unread_rooms() {
        let mut state = state(PoolState::Unreachable);
        state.set_running(3);
        state.unread_mut().mark("rm_ops");
        assert_eq!(state.icon(), StatusIcon::Unreachable);

        // The facts the icon had to drop are still in the tooltip.
        let tooltip = state.tooltip();
        assert!(tooltip.contains("3 sessions running"), "got {tooltip:?}");
        assert!(tooltip.contains("1 unread in 1 room"), "got {tooltip:?}");
        assert!(tooltip.contains("cannot be reached"), "got {tooltip:?}");
    }

    #[test]
    fn a_running_agent_outranks_an_unread_room_but_keeps_it_in_the_tooltip() {
        let mut state = state(PoolState::Reachable);
        state.set_running(1);
        state.unread_mut().mark("rm_ops");
        assert_eq!(state.icon(), StatusIcon::AgentsRunning);
        assert!(state.tooltip().contains("1 unread in 1 room"));
    }

    #[test]
    fn every_icon_state_has_its_own_word_and_colour() {
        let states = [
            StatusIcon::Unreachable,
            StatusIcon::AgentsRunning,
            StatusIcon::Unread,
            StatusIcon::Idle,
        ];
        for (index, icon) in states.iter().enumerate() {
            assert!(!icon.label().is_empty());
            for other in &states[index + 1..] {
                assert_ne!(
                    icon.label(),
                    other.label(),
                    "{icon:?} and {other:?} share a word"
                );
                assert_ne!(
                    icon.rgb(),
                    other.rgb(),
                    "{icon:?} and {other:?} share a colour"
                );
            }
        }
        let (r, g, b) = StatusIcon::Idle.rgb();
        assert_eq!((r, g, b), (g, b, r), "idle must be neutral");
        let (ur, ug, ub) = StatusIcon::Unreachable.rgb();
        assert!(ur > ug && ur > ub, "unreachable must read as an alarm");
    }

    #[test]
    fn the_tooltip_counts_sessions_and_rooms() {
        assert_eq!(
            state(PoolState::Reachable).tooltip(),
            "Tidepool \u{2014} pool reachable \u{00B7} no sessions running"
        );

        let mut two = state(PoolState::Reachable);
        two.set_running(2);
        two.unread_mut().mark("rm_ops");
        two.unread_mut().mark("rm_general");
        two.unread_mut().mark("rm_general");
        assert_eq!(
            two.tooltip(),
            "Tidepool \u{2014} pool reachable \u{00B7} 2 sessions running \u{00B7} 3 unread in 2 rooms"
        );

        let mut one = state(PoolState::Unreachable);
        one.unread_mut().mark("rm_ops");
        assert_eq!(
            one.tooltip(),
            "Tidepool \u{2014} the pool cannot be reached \u{00B7} no sessions running \u{00B7} 1 unread in 1 room"
        );
    }

    #[test]
    fn the_accessible_label_carries_the_state_and_the_facts() {
        let mut state = state(PoolState::Reachable);
        state.set_running(1);
        let label = state.accessible_label();
        assert!(label.starts_with("Agents running:"), "got {label:?}");
        assert!(label.contains("1 session running"), "got {label:?}");
    }
}
