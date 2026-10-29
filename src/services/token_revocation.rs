//! Server-side sign-out for operator tokens.
//!
//! A signed JWT stays valid until its `exp`, so sign-out records the token's
//! `jti` in Redis for the rest of that life. The operator extractor refuses any
//! token whose `jti` is listed. Callers fail closed: when the store is missing
//! or unreachable, a token is refused rather than trusted.

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;

/// Redis key prefix for revoked token ids.
const KEY_PREFIX: &str = "pool:revoked:";

/// The Redis key that marks `jti` as revoked.
fn revoked_key(jti: &str) -> String {
    format!("{KEY_PREFIX}{jti}")
}

/// Record `jti` as revoked until `exp` (unix seconds). The entry expires with
/// the token, so the store never grows past the longest live token.
pub async fn revoke(conn: &ConnectionManager, jti: &str, exp: i64, now: i64) -> Result<()> {
    let ttl = (exp - now).max(1);
    let mut conn = conn.clone();
    let _: () = redis::cmd("SET")
        .arg(revoked_key(jti))
        .arg(1)
        .arg("EX")
        .arg(ttl)
        .query_async(&mut conn)
        .await
        .context("failed to record token revocation")?;
    Ok(())
}

/// Whether `jti` has been revoked.
pub async fn is_revoked(conn: &ConnectionManager, jti: &str) -> Result<bool> {
    let mut conn = conn.clone();
    let count: i64 = redis::cmd("EXISTS")
        .arg(revoked_key(jti))
        .query_async(&mut conn)
        .await
        .context("failed to check token revocation")?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revoked_keys_are_namespaced_by_jti() {
        assert_eq!(revoked_key("abc"), "pool:revoked:abc");
        assert_ne!(revoked_key("abc"), revoked_key("abd"));
    }
}
