use thiserror::Error;

#[derive(Error, Debug)]
pub enum MySQLSourceError {
    #[error(transparent)]
    ConnectorXError(#[from] crate::errors::ConnectorXError),

    #[error(transparent)]
    MySQLError(#[from] mysql::Error),

    #[error(transparent)]
    MySQLUrlError(#[from] mysql::UrlError),

    #[error(transparent)]
    MySQLPoolError(#[from] r2d2::Error),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error("unsupported ssl-mode value: {0}")]
    InvalidSslMode(String),

    /// Any other errors that are too trivial to be put here explicitly.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
