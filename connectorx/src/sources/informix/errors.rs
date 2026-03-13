use std::ffi::NulError;
use std::string::FromUtf8Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InformixSourceError {
    #[error(transparent)]
    ConnectorXError(#[from] crate::errors::ConnectorXError),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error(transparent)]
    UrlDecodeError(#[from] FromUtf8Error),

    #[error(transparent)]
    CStringError(#[from] NulError),

    #[error(transparent)]
    Bridge(#[from] ibm_informix_bridge::BridgeError),

    #[error("Informix handle allocation failed: {0}")]
    HandleAllocationError(i16),

    #[error("Informix connection failed: {0}")]
    ConnectionError(String),

    #[error("Informix statement failed: {0}")]
    StatementError(String),

    #[error("Informix fetch failed: {0}")]
    DataFetchError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
