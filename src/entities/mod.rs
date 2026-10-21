//! Database entity / domain model definitions.
//!
//! Concrete types land here as persistence and upstream-parity modelling
//! progress (see `docs/TALO_API.md`).

pub mod analytics_event;
pub mod api_key;
pub mod configuration_revision;
pub mod connection;
pub mod enrollment_token;
pub mod environment;
pub mod game_channel;
pub mod game_setting;
pub mod host;
pub mod host_capability;
pub mod host_credential;
pub mod host_execution;
pub mod leaderboard;
pub mod organization;
pub mod player;
pub mod player_alias;
pub mod player_auth;
pub mod pool_invitation;
pub mod pool_user;
pub mod project;
pub mod prop;
pub mod save;
