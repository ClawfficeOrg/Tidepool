//! The error type every fallible operation returns.

use crate::problem::ProblemError;

/// The crate's result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong before, during or after one API call.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The client configuration is not usable.
    #[error("invalid client configuration: {0}")]
    Config(String),
    /// A caller used the SDK in a way the contract does not allow.
    #[error("{0}")]
    Usage(String),
    /// The operation id is not in the committed contract.
    #[error("operation `{0}` is not in the contract")]
    UnknownOperation(String),
    /// The transport failed before an HTTP response existed.
    #[error("HTTP transport failed: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A request body could not be encoded as JSON.
    #[error("request body could not be encoded as JSON: {0}")]
    Encode(#[from] serde_json::Error),
    /// A success response body did not match the type the contract names.
    #[error("response body for `{operation_id}` was not the JSON the contract declares: {source}")]
    Decode {
        /// Operation that produced the response.
        operation_id: &'static str,
        /// The serde failure.
        #[source]
        source: serde_json::Error,
    },
    /// The server returned a non-2xx response.
    #[error(transparent)]
    Problem(#[from] ProblemError),
    /// A `Retry-After` longer than the configured ceiling.
    #[error(
        "server asked to retry after {requested_ms} ms, above the {limit_ms} ms ceiling; surfacing the rate limit instead"
    )]
    RetryAfterTooLong {
        /// Delay the server asked for, in milliseconds.
        requested_ms: u64,
        /// Configured ceiling, in milliseconds.
        limit_ms: u64,
    },
    /// A response was a success status but carried no usable body.
    #[error("`{operation_id}` returned {status} without the body the contract declares")]
    MissingBody {
        /// Operation that produced the response.
        operation_id: &'static str,
        /// HTTP status of the empty response.
        status: u16,
    },
}

impl Error {
    /// The problem details, when this error came from a non-2xx response.
    pub fn problem(&self) -> Option<&ProblemError> {
        match self {
            Self::Problem(problem) => Some(problem),
            _ => None,
        }
    }

    /// The stable problem code, when this error came from a non-2xx response.
    pub fn code(&self) -> Option<&str> {
        self.problem().map(ProblemError::code)
    }
}
