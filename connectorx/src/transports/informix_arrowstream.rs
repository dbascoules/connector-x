//! Transport from Informix Source to Arrow stream destination.

use crate::{
    destinations::arrowstream::{
        typesystem::ArrowTypeSystem, ArrowDestination, ArrowDestinationError,
    },
    sources::informix::{InformixSource, InformixSourceError, InformixTypeSystem},
    typesystem::TypeConversion,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InformixArrowTransportError {
    #[error(transparent)]
    Source(#[from] InformixSourceError),

    #[error(transparent)]
    Destination(#[from] ArrowDestinationError),

    #[error(transparent)]
    ConnectorX(#[from] crate::errors::ConnectorXError),
}

pub struct InformixArrowTransport;

impl_transport!(
    name = InformixArrowTransport,
    error = InformixArrowTransportError,
    systems = InformixTypeSystem => ArrowTypeSystem,
    route = InformixSource => ArrowDestination,
    mappings = {
        { Text[String] => LargeUtf8[String] | conversion auto }
    }
);