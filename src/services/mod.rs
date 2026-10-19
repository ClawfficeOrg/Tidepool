//! Business-logic services (auth, players, leaderboards, ...).
//!
//! Each service module owns a slice of behaviour and is consumed by the
//! corresponding `api/*` handler. Populated in Phase 0.2.

pub mod api_keys;
pub mod auth;
pub mod cache;
pub mod enrollment;
pub mod events;
pub mod feedback;
pub mod game_settings;
pub mod hosts;
pub mod leaderboards;
pub mod players;
pub mod pool_identity;
pub mod redis;
pub mod saves;
