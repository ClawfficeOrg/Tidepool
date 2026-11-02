//! Ambient credentials shared by the blocking and async facades.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::config::ClientConfig;

/// A refreshed token, optionally with the lifetime the server reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenUpdate {
    /// The new operator token.
    pub token: String,
    /// How long the token is usable.
    pub expires_in: Option<Duration>,
}

impl TokenUpdate {
    /// A token with no known expiry.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            expires_in: None,
        }
    }

    /// Add the token lifetime.
    pub fn with_expiry(mut self, expires_in: Duration) -> Self {
        self.expires_in = Some(expires_in);
        self
    }
}

/// A synchronous refresh hook, called at most once per request.
pub type RefreshHook = std::sync::Arc<dyn Fn() -> crate::Result<Option<TokenUpdate>> + Send + Sync>;

/// An async refresh hook, called at most once per request.
#[cfg(feature = "async")]
pub type AsyncRefreshHook = std::sync::Arc<
    dyn Fn() -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = crate::Result<Option<TokenUpdate>>>
                    + Send
                    + 'static,
            >,
        > + Send
        + Sync,
>;

/// The mutable part of a client: the credentials currently in effect.
#[derive(Debug, Clone)]
pub(crate) struct AuthState {
    token: Option<String>,
    expires_at: Option<Instant>,
    api_key: Option<String>,
    host_credential: Option<String>,
}

impl AuthState {
    /// Adopt the credentials baked into a client configuration.
    pub(crate) fn from_config(config: &ClientConfig) -> Self {
        let expires_at = config
            .token_expiry()
            .and_then(|expiry| Instant::now().checked_add(expiry));
        Self {
            token: config.token().map(str::to_owned),
            expires_at,
            api_key: config.api_key().map(str::to_owned),
            host_credential: config.host_credential().map(str::to_owned),
        }
    }

    /// The ambient credential headers, in the contract's precedence.
    ///
    /// A token, an API key and a host credential can all be set; the server
    /// decides which one a route accepts.
    pub(crate) fn headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if let Some(token) = &self.token {
            headers.push(("authorization".to_owned(), format!("Bearer {token}")));
        }
        if let Some(api_key) = &self.api_key {
            headers.push(("x-api-key".to_owned(), api_key.clone()));
        }
        if let Some(credential) = &self.host_credential {
            headers.push(("x-host-credential".to_owned(), credential.clone()));
        }
        headers
    }

    /// The current operator token.
    pub(crate) fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// True when a token is set and its known lifetime has passed.
    pub(crate) fn expired(&self) -> bool {
        self.expires_at
            .is_some_and(|expires_at| Instant::now() >= expires_at)
            && self.token.is_some()
    }

    /// Store a token, optionally with a lifetime.
    pub(crate) fn set_token(&mut self, token: Option<String>, expiry: Option<Duration>) {
        self.token = token;
        self.expires_at = expiry.and_then(|expiry| Instant::now().checked_add(expiry));
    }

    /// Replace the API key.
    pub(crate) fn set_api_key(&mut self, api_key: Option<String>) {
        self.api_key = api_key;
    }

    /// Replace the host credential.
    pub(crate) fn set_host_credential(&mut self, credential: Option<String>) {
        self.host_credential = credential;
    }

    /// Apply a refresh hook's result.
    pub(crate) fn apply(&mut self, update: TokenUpdate) {
        self.set_token(Some(update.token), update.expires_in);
    }
}

/// Lock a mutex, recovering the data if a previous holder panicked.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AuthState {
        AuthState {
            token: Some("jwt".to_owned()),
            expires_at: None,
            api_key: Some("key".to_owned()),
            host_credential: Some("secret".to_owned()),
        }
    }

    #[test]
    fn credentials_become_the_contract_headers() {
        assert_eq!(
            state().headers(),
            vec![
                ("authorization".to_owned(), "Bearer jwt".to_owned()),
                ("x-api-key".to_owned(), "key".to_owned()),
                ("x-host-credential".to_owned(), "secret".to_owned()),
            ]
        );
    }

    #[test]
    fn unset_credentials_produce_no_headers() {
        let empty = AuthState {
            token: None,
            expires_at: None,
            api_key: None,
            host_credential: None,
        };
        assert!(empty.headers().is_empty());
    }

    #[test]
    fn expiry_is_only_known_when_a_lifetime_was_given() {
        let mut state = state();
        assert!(!state.expired());
        state.set_token(Some("jwt".to_owned()), Some(Duration::from_secs(60)));
        assert!(!state.expired());
        state.set_token(Some("jwt".to_owned()), Some(Duration::ZERO));
        assert!(state.expired());
    }

    #[test]
    fn applying_a_refresh_replaces_the_token_and_expiry() {
        let mut state = state();
        state.apply(TokenUpdate::new("new").with_expiry(Duration::ZERO));
        assert_eq!(state.token(), Some("new"));
        assert!(state.expired());
    }

    #[test]
    fn clearing_the_token_clears_its_expiry() {
        let mut state = state();
        state.set_token(None, None);
        assert_eq!(state.token(), None);
        assert!(!state.expired());
    }
}
