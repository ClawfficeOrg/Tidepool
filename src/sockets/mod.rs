//! WebSocket scaffold

pub mod channels;
pub mod chatroom_ws;
pub mod groups;
pub mod host_ws;
pub mod presence;
pub mod rooms;
pub mod tickets;
pub mod ws;

#[allow(dead_code)]
pub fn init() {
    // placeholder: module initialization if needed
}

#[cfg(test)]
mod tests;
