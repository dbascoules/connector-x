use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use ibm_informix_bridge::ColDesc;
use rust_decimal::Decimal;

const SQL_CHAR: i16 = 1;
const SQL_NUMERIC: i16 = 2;
const SQL_DECIMAL: i16 = 3;
const SQL_INTEGER: i16 = 4;
const SQL_SMALLINT: i16 = 5;
const SQL_FLOAT: i16 = 6;
const SQL_REAL: i16 = 7;
const SQL_DOUBLE: i16 = 8;
const SQL_DATETIME: i16 = 9;
const SQL_TIME: i16 = 10;
const SQL_TIMESTAMP: i16 = 11;
const SQL_VARCHAR: i16 = 12;
const SQL_BIGINT: i16 = -5;
const SQL_LONGVARCHAR: i16 = -1;
const SQL_DATE: i16 = 91;
const SQL_BINARY: i16 = -2;
const SQL_VARBINARY: i16 = -3;
const SQL_LONGVARBINARY: i16 = -4;
const SQL_WCHAR: i16 = -8;
const SQL_WVARCHAR: i16 = -9;
const SQL_LONGWVARCHAR: i16 = -10;
const SQL_TYPE_TIME: i16 = 92;
const SQL_TYPE_TIMESTAMP: i16 = 93;
const SQL_BOOLEAN: i16 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InformixTypeSystem {
    Text(bool),
    SmallInt(bool),
    Integer(bool),
    BigInt(bool),
    Float(bool),
    Double(bool),
    Decimal(bool),
    Boolean(bool),
    Date(bool),
    Time(bool),
    Timestamp(bool),
}

impl_typesystem! {
    system = InformixTypeSystem,
    mappings = {
        { Text => String }
        { SmallInt => i16 }
        { Integer => i32 }
        { BigInt => i64 }
        { Float => f32 }
        { Double => f64 }
        { Decimal => Decimal }
        { Boolean => bool }
        { Date => NaiveDate }
        { Time => NaiveTime }
        { Timestamp => NaiveDateTime }
    }
}

impl<'a> From<&'a ColDesc> for InformixTypeSystem {
    fn from(col: &'a ColDesc) -> InformixTypeSystem {
        let nullable = col.nullable;
        match col.sql_type {
            SQL_SMALLINT => InformixTypeSystem::SmallInt(nullable),
            SQL_INTEGER => InformixTypeSystem::Integer(nullable),
            SQL_BIGINT => InformixTypeSystem::BigInt(nullable),
            SQL_REAL => InformixTypeSystem::Float(nullable),
            SQL_FLOAT | SQL_DOUBLE => InformixTypeSystem::Double(nullable),
            SQL_DECIMAL | SQL_NUMERIC => InformixTypeSystem::Decimal(nullable),
            SQL_BOOLEAN => InformixTypeSystem::Boolean(nullable),
            SQL_DATE => InformixTypeSystem::Date(nullable),
            SQL_DATETIME | SQL_TIMESTAMP | SQL_TYPE_TIMESTAMP => InformixTypeSystem::Timestamp(nullable),
            SQL_TIME | SQL_TYPE_TIME => InformixTypeSystem::Time(nullable),
            SQL_CHAR
            | SQL_VARCHAR
            | SQL_LONGVARCHAR
            | SQL_BINARY
            | SQL_VARBINARY
            | SQL_LONGVARBINARY
            | SQL_WCHAR
            | SQL_WVARCHAR
            | SQL_LONGWVARCHAR => InformixTypeSystem::Text(nullable),
            _ => InformixTypeSystem::Text(nullable),
        }
    }
}
