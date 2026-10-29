//! Generated contract artifacts.
//!
//! Nothing in this module is hand-edited: `scripts/generate.mjs` renders it
//! from `docs/api/openapi-v1.json`.
//!
//! - [`types`] — one serde struct per contract schema;
//! - [`routes`] — the route table and contract version;
//! - [`endpoints`] — one typed request per contract route;
//! - [`resources`] — namespace wrappers for the blocking and async clients.

pub mod endpoints;
pub mod resources;
pub mod routes;
pub mod types;
