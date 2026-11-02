//! Opt-in integration test against a running dev stack.
//!
//! Ignored by default because the crate's normal test run must not require a
//! live pool (`AGENTS.md` and the task's no-live-server rule). Run it by hand:
//!
//! ```sh
//! TIDEPOOL_BASE_URL=http://localhost:8080 TIDEPOOL_TOKEN=... \
//!   cargo test --test live -- --ignored --nocapture
//! ```

#![cfg(feature = "blocking")]
#![allow(clippy::result_large_err)]

use tidepool_client::blocking::Client;
use tidepool_client::endpoints::{GetV1Activity, GetV1Hosts};

#[test]
#[ignore = "requires a dev stack; set TIDEPOOL_BASE_URL and TIDEPOOL_TOKEN"]
fn reads_hosts_and_activity_from_a_dev_stack() {
    let base_url = std::env::var("TIDEPOOL_BASE_URL").expect("TIDEPOOL_BASE_URL");
    let token = std::env::var("TIDEPOOL_TOKEN").expect("TIDEPOOL_TOKEN");
    let client = Client::from_base_url(base_url)
        .expect("client")
        .with_token(token);

    let hosts = client.call(&GetV1Hosts::new()).expect("hosts");
    println!("hosts visible to this principal: {}", hosts.hosts.len());

    let page = client.call(&GetV1Activity::new()).expect("activity");
    println!(
        "activity entries on the first page: {}, more: {}",
        page.items.len(),
        page.has_more
    );
}
