//! Transport from Informix Source to Arrow Destination.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use rust_decimal::Decimal;
use crate::{
    destinations::arrow::{
        typesystem::{ArrowTypeSystem, NaiveDateTimeWrapperMicro},
        ArrowDestination, ArrowDestinationError,
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
        { SmallInt[i16]            => Int16[i16]                               | conversion auto }
        { Integer[i32]             => Int32[i32]                               | conversion auto }
        { BigInt[i64]              => Int64[i64]                               | conversion auto }
        { Float[f32]               => Float32[f32]                             | conversion auto }
        { Double[f64]              => Float64[f64]                             | conversion auto }
        { Decimal[Decimal]         => Decimal[Decimal]                         | conversion auto }
        { Boolean[bool]            => Boolean[bool]                            | conversion auto }
        { Date[NaiveDate]          => Date32[NaiveDate]                        | conversion auto }
        { Time[NaiveTime]          => Time64[NaiveTime]                        | conversion auto }
        { Timestamp[NaiveDateTime] => Date64Micro[NaiveDateTimeWrapperMicro]   | conversion option }
        { Text[String]             => LargeUtf8[String]                        | conversion auto }
    }
);

impl TypeConversion<NaiveDateTime, NaiveDateTimeWrapperMicro> for InformixArrowTransport {
    fn convert(val: NaiveDateTime) -> NaiveDateTimeWrapperMicro {
        NaiveDateTimeWrapperMicro(val)
    }
}