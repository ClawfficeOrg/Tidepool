//! Typed Rust client for the Tidepool hub API.
//!
//! # API version
//!
//! This crate targets **Tidepool API v1** and the committed contract
//! [`docs/api/openapi-v1.json`](../../docs/api/openapi-v1.json), version
//! [`CONTRACT_VERSION`]. The contract is the only input to the generated
//! modules under [`types`], [`routes`], [`endpoints`] and [`resources`]; a
//! contract change without a regeneration is caught by the crate's tests.
//!
//! # Features
//!
//! | Feature | Default | Adds |
//! |---|---|---|
//! | *(none)* | — | contract types, the route table, request types, errors |
//! | `blocking` | yes | [`blocking::Client`], a blocking facade with no async runtime |
//! | `async` | no | [`asynchronous::Client`], a `reqwest`-backed async client |
//!
//! A default build has no async runtime and no networking dependency at all
//! beyond the contract types. `blocking` uses `ureq`; `async` uses `reqwest`
//! and therefore needs a Tokio runtime, which is the one runtime choice the
//! async feature makes. The blocking facade never asks the caller to build one.
//!
//! # Shape
//!
//! The resource shape mirrors the TypeScript SDK (`sdk/ts`): the same names,
//! the same request and response bodies, the same problem details, the same
//! bounded 429 retry and refresh-once policy. Every route in the contract is a
//! type in [`endpoints`] and a method in [`resources`].
//!
//! ```no_run
//! # #[cfg(feature = "blocking")]
//! # fn main() -> tidepool_client::Result<()> {
//! use tidepool_client::blocking::Client;
//! use tidepool_client::endpoints::GetV1Hosts;
//!
//! let client = Client::from_base_url("https://pool.example.com")?.with_token("operator-jwt");
//! let hosts = client.call(&GetV1Hosts::new())?;
//! for host in &hosts.hosts {
//!     println!("{} {}", host.slug, host.status);
//! }
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "blocking"))]
//! # fn main() {}
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// `Error` intentionally carries the whole problem document (status, code,
// detail, request id, attempts, body) because that is the diagnostic a caller
// needs; boxing it would only move the allocation.
#![allow(clippy::result_large_err)]
// The core modules are shared by the two facades; a no-feature build has no
// caller for their crate-private helpers, which is the point of that build.
#![cfg_attr(not(any(feature = "blocking", feature = "async")), allow(dead_code))]

pub mod config;
pub mod error;
pub mod problem;
pub mod retry;
pub mod route;
pub mod stream;

mod auth;
mod request;

pub mod generated;

#[cfg(feature = "async")]
pub use auth::AsyncRefreshHook;
pub use auth::{RefreshHook, TokenUpdate};
pub use config::{ClientConfig, ClientConfigBuilder};
pub use error::{Error, Result};
pub use problem::ProblemError;
pub use request::CallOptions;
pub use retry::{DEFAULT_RETRY, RetryPolicy};

#[cfg(feature = "async")]
pub mod asynchronous;
#[cfg(feature = "blocking")]
pub mod blocking;
#[cfg(feature = "blocking")]
pub mod paging;

#[cfg(feature = "async")]
pub use asynchronous::Client as AsyncClient;
#[cfg(feature = "blocking")]
pub use blocking::Client as BlockingClient;

/// The API major version this crate targets.
pub const API_VERSION: &str = "v1";

/// The contract version this crate was generated from.
pub const CONTRACT_VERSION: &str = generated::routes::CONTRACT_VERSION;

/// Re-export of the generated contract types.
pub use generated::types;

/// Re-export of the generated typed request types.
pub use generated::endpoints;

/// Re-export of the generated route table.
pub use generated::routes;

/// Re-export of the generated namespace wrappers.
pub use generated::resources;

/// Look up one route's access model and wire shape by operation id.
///
/// Returns [`Error::UnknownOperation`] when the id is not in the contract.
pub fn describe(operation_id: &str) -> Result<&'static routes::RouteInfo> {
    routes::route(operation_id).ok_or_else(|| Error::UnknownOperation(operation_id.to_owned()))
}

/// Answer whether a principal may call an operation, before sending anything.
///
/// This mirrors the TypeScript SDK's `allows()` and
/// `docs/API_CONTRACT.md` §4: a public route is open; a scope satisfies an API
/// key; a member route needs a role and refuses writes from a viewer; an
/// owner-only route needs the owner role.
pub fn allows(operation_id: &str, access: &route::Access<'_>) -> Result<bool> {
    Ok(describe(operation_id)?.allows(access))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_version_is_documented() {
        assert_eq!(API_VERSION, "v1");
        assert!(!CONTRACT_VERSION.is_empty());
    }

    #[test]
    fn describe_rejects_unknown_operations() {
        let error = describe("nope").expect_err("unknown operation must fail");
        assert!(matches!(error, Error::UnknownOperation(id) if id == "nope"));
    }
}
