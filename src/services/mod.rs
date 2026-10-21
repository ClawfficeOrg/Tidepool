//! Business-logic services (auth, players, leaderboards, ...).
//!
//! Each service module owns a slice of behaviour and is consumed by the
//! corresponding `api/*` handler. Populated in Phase 0.2.

pub mod activity;
pub mod api_keys;
pub mod auth;
pub mod cache;
pub mod configuration;
pub mod connections;
pub mod dispatch;
pub mod enrollment;
pub mod environments;
pub mod events;
pub mod feedback;
pub mod game_settings;
pub mod host_capability;
pub mod host_presence;
pub mod host_protocol;
pub mod hosts;
pub mod leaderboards;
pub mod outputs;
pub mod players;
pub mod pool_identity;
pub mod projects;
pub mod redis;
pub mod runs;
pub mod saves;
pub mod triggers;
pub mod workflows;
