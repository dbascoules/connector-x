use crate::errors::ConnectorXPythonError;
use crate::pandas::destination::PandasDestination;
use crate::pandas::typesystem::{DateTimeWrapperMicro, PandasTypeSystem};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use connectorx::{
    impl_transport,
    sources::informix::{InformixSource, InformixTypeSystem},
    typesystem::TypeConversion,
};
use rust_decimal::prelude::*;

#[allow(dead_code)]
pub struct InformixPandasTransport<'py>(&'py ());

impl_transport!(
    name = InformixPandasTransport<'tp>,
    error = ConnectorXPythonError,
    systems = InformixTypeSystem => PandasTypeSystem,
    route = InformixSource => PandasDestination<'tp>,
    mappings = {
        { SmallInt[i16]               => I64[i64]                         | conversion auto }
        { Integer[i32]                => I64[i64]                         | conversion auto }
        { BigInt[i64]                 => I64[i64]                         | conversion auto }
        { Float[f32]                  => F64[f64]                         | conversion auto }
        { Double[f64]                 => F64[f64]                         | conversion auto }
        { Decimal[Decimal]            => F64[f64]                         | conversion option }
        { Boolean[bool]               => Bool[bool]                       | conversion auto }
        { Date[NaiveDate]             => DateTimeMicro[DateTimeWrapperMicro] | conversion option }
        { Time[NaiveTime]             => String[String]                   | conversion option }
        { Timestamp[NaiveDateTime]    => DateTimeMicro[DateTimeWrapperMicro] | conversion option }
        { Text[String]                => String[String]                   | conversion auto }
    }
);

impl<'py> TypeConversion<NaiveDate, DateTimeWrapperMicro> for InformixPandasTransport<'py> {
    fn convert(val: NaiveDate) -> DateTimeWrapperMicro {
        DateTimeWrapperMicro(DateTime::from_naive_utc_and_offset(
            val.and_hms_opt(0, 0, 0)
                .unwrap_or_else(|| panic!("and_hms_opt got None from {:?}", val)),
            Utc,
        ))
    }
}

impl<'py> TypeConversion<NaiveDateTime, DateTimeWrapperMicro> for InformixPandasTransport<'py> {
    fn convert(val: NaiveDateTime) -> DateTimeWrapperMicro {
        DateTimeWrapperMicro(DateTime::from_naive_utc_and_offset(val, Utc))
    }
}

impl<'py> TypeConversion<NaiveTime, String> for InformixPandasTransport<'py> {
    fn convert(val: NaiveTime) -> String {
        val.to_string()
    }
}

impl<'py> TypeConversion<Decimal, f64> for InformixPandasTransport<'py> {
    fn convert(val: Decimal) -> f64 {
        val.to_f64()
            .unwrap_or_else(|| panic!("cannot convert decimal {:?} to float64", val))
    }
}
