//! The CLI's error type.
//!
//! Every message here is safe to print: no variant carries credential material,
//! and the SDK's problem errors describe a request, never a secret.

use std::path::PathBuf;

/// The crate's result alias.
pub type Result<T> = std::result::Result<T, CliError>;

/// Everything a command can fail with.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CliError {
    /// The command line does not name a command the CLI implements.
    #[error("{0}")]
    Usage(String),
    /// The SDK refused or the pool answered with an error.
    #[error(transparent)]
    Sdk(tidepool_client::Error),
    /// A file could not be read.
    #[error("could not read `{path}`: {source}")]
    Read {
        /// File that could not be read.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A file or directory could not be written.
    #[error("could not write `{path}`: {source}")]
    Write {
        /// Path that could not be written.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A JSON input file did not match the shape a command accepts.
    #[error("`{path}` is not the JSON that command accepts: {source}")]
    DecodeFile {
        /// File that did not decode.
        path: PathBuf,
        /// The serde failure.
        #[source]
        source: serde_json::Error,
    },
    /// A response could not be rendered as JSON.
    #[error("could not encode the output as JSON: {0}")]
    Encode(serde_json::Error),
    /// The credential store would have landed inside a repository.
    #[error(
        "refusing to write credentials inside a repository: `{0}`; set TIDEPOOL_HOME to a directory outside the checkout"
    )]
    RepositoryStore(PathBuf),
    /// No pool URL was given and none was remembered.
    #[error(
        "no pool configured: pass --pool <url> or run `tidepool login --pool <url> --email <you> --password-stdin`"
    )]
    NoPool,
    /// The command needs an operator token and the store has none.
    #[error("not signed in to the pool: run `tidepool login` first")]
    NotSignedIn,
    /// Neither `TIDEPOOL_HOME`, `XDG_CONFIG_HOME` nor `HOME` is set.
    #[error("cannot locate a credential store: set TIDEPOOL_HOME or HOME")]
    NoHome,
    /// The store file exists but is not the JSON the CLI wrote.
    #[error(
        "the credential store `{path}` is not readable as JSON; fix or remove it (its contents are not printed)"
    )]
    CorruptStore {
        /// Store that failed to parse.
        path: PathBuf,
    },
    /// Standard input failed.
    #[error("could not read stdin: {0}")]
    Input(#[source] std::io::Error),
    /// Standard output failed.
    #[error("could not write output: {0}")]
    Output(#[source] std::io::Error),
}

impl From<tidepool_client::Error> for CliError {
    fn from(error: tidepool_client::Error) -> Self {
        Self::Sdk(error)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::Encode(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_errors_keep_their_problem_code() {
        let error = CliError::Sdk(tidepool_client::Error::Usage("bad".to_owned()));
        assert!(error.to_string().contains("bad"));
    }

    #[test]
    fn a_repository_store_error_names_the_path_but_no_secret() {
        let error = CliError::RepositoryStore(PathBuf::from("/tmp/repo/credentials.json"));
        let rendered = error.to_string();
        assert!(rendered.contains("/tmp/repo/credentials.json"));
        assert!(rendered.contains("TIDEPOOL_HOME"));
    }
}
