//! The always-present surface: the status item, its menu and its popover.
//!
//! A Tidepool client is a menu-bar app. The window is the occasional surface;
//! this is the one that is always there. It is assembled from three pieces:
//!
//! - [`icon`] — the state behind the icon, and the mapping from that state to
//!   one legible icon ([`TrayState::icon`]). The precedence between an
//!   unreachable pool, running agents and an unread room is decided there.
//! - [`unread`] — a count per room, cleared only when a person asks.
//! - [`popover`] — what the click-through popover says: the current host and
//!   the sessions still in flight.
//!
//! The menu itself lives in [`crate::menu`], next to the keyboard bindings that
//! reach the same actions from a window, and the notification policy lives in
//! [`crate::notify`]. None of the modules here draw: they are models, so the
//! state mapping and the notification rules are tested without a menu bar.
//!
//! The platform icon seam is [`crate::tray_icon::TrayIcon`]; [`apply_state`]
//! tints it from a [`TrayState`].

pub mod icon;
pub mod popover;
pub mod unread;

pub use icon::{PoolState, StatusIcon, TrayState};
pub use popover::{HostSummary, SessionRow, TrayPopover};
pub use unread::Unread;

/// Tint the platform status item to match `state`.
///
/// On macOS this tints the template artwork; on Windows it replaces the icon
/// with a solid colour; on a platform without a tray it is a no-op. The colour
/// comes from [`StatusIcon::rgb`], so the icon and its tint can never disagree.
pub fn apply_state(tray: &mut crate::tray_icon::TrayIcon, state: &TrayState) {
    let (r, g, b) = state.icon().rgb();
    tray.set_state_color(r, g, b);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applying_a_state_to_the_platform_icon_does_not_panic() {
        // Off the main thread the platform seam returns no icon; the point is
        // that the state mapping reaches it without a window or a panic.
        let mut tray = crate::tray_icon::TrayIcon::new("Tidepool");
        let mut state = TrayState::new(PoolState::Reachable);
        state.set_running(1);
        state.unread_mut().mark("rm_ops");
        apply_state(&mut tray, &state);
    }

    #[test]
    fn an_unreachable_pool_tints_the_icon_with_the_alarm_colour() {
        let state = TrayState::new(PoolState::Unreachable);
        assert_eq!(state.icon().rgb(), StatusIcon::Unreachable.rgb());
    }
}
