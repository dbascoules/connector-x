#![cfg(all(feature = "src_informix", feature = "dst_arrow"))]

use chrono::{NaiveDate, NaiveDateTime};
use connectorx::{
    prelude::*,
    sources::{informix::InformixSource, PartitionParser},
    sql::CXQuery,
};
use rust_decimal::Decimal;
use std::env;

#[test]
fn test_informix_types() {
    let _ = env_logger::builder().is_test(true).try_init();

    if env::var("INFORMIX_URL").is_err() {
        return;
    }

    let dburl = env::var("INFORMIX_URL").unwrap();
    let mut source = InformixSource::new(&dburl, 1)
        .unwrap_or_else(|e| panic!("InformixSource::new failed: {}", e));
    source.set_queries(&[CXQuery::naked(
        "select test_bool, test_smallint, test_int, test_bigint, test_float, test_double, test_decimal, test_char, test_varchar, test_text, test_date, test_datetime, test_nullable from test_types order by case when test_int is null then 1 else 0 end, test_int",
    )]);
    source
        .fetch_metadata()
        .unwrap_or_else(|e| panic!("fetch_metadata failed: {}", e));

    let mut partitions = source.partition().unwrap();
    assert_eq!(1, partitions.len());

    let mut partition = partitions.remove(0);
    partition.result_rows().expect("run query");
    assert_eq!(13, partition.ncols());

    let mut parser = partition.parser().unwrap();
    let mut rows: Vec<(
        Option<bool>,
        Option<i16>,
        Option<i32>,
        Option<i64>,
        Option<f32>,
        Option<f64>,
        Option<Decimal>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<NaiveDate>,
        Option<NaiveDateTime>,
        Option<String>,
    )> = Vec::new();

    loop {
        let (n, is_last) = parser.fetch_next().unwrap();
        for _ in 0..n {
            rows.push((
                parser.parse::<Option<bool>>().unwrap(),
                parser.parse::<Option<i16>>().unwrap(),
                parser.parse::<Option<i32>>().unwrap(),
                parser.parse::<Option<i64>>().unwrap(),
                parser.parse::<Option<f32>>().unwrap(),
                parser.parse::<Option<f64>>().unwrap(),
                parser.parse::<Option<Decimal>>().unwrap(),
                parser.parse::<Option<String>>().unwrap(),
                parser.parse::<Option<String>>().unwrap(),
                parser.parse::<Option<String>>().unwrap(),
                parser.parse::<Option<NaiveDate>>().unwrap(),
                parser.parse::<Option<NaiveDateTime>>().unwrap(),
                parser.parse::<Option<String>>().unwrap(),
            ));
        }
        if is_last {
            break;
        }
    }

    assert_eq!(3, rows.len());
    
    // First row: true, -32767, 0, 9223372036854775807, -1.1, -1.12345, -1234.56, 'abc', 'varchar', 'informix text', '1970-01-01', '1970-01-01 00:00:01.12345', 'row-one'
    let first = &rows[0];
    assert_eq!(first.0, Some(true), "Row 0: test_bool");
    assert_eq!(first.1, Some(-32767), "Row 0: test_smallint");
    assert_eq!(first.2, Some(0), "Row 0: test_int");
    assert_eq!(first.3, Some(9223372036854775807), "Row 0: test_bigint");
    assert_eq!(first.4, Some(-1.1), "Row 0: test_float");
    assert_eq!(first.5, Some(-1.12345), "Row 0: test_double");
    assert_eq!(first.6, Some(Decimal::new(-123456, 2)), "Row 0: test_decimal");
    assert_eq!(first.7, Some("abc  ".to_string()), "Row 0: test_char");
    assert_eq!(first.8, Some("varchar".to_string()), "Row 0: test_varchar");
    assert_eq!(first.9, Some("informix text".to_string()), "Row 0: test_text");
    assert_eq!(first.10, Some(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()), "Row 0: test_date");
    assert_eq!(
        first.11,
        Some(
            NaiveDateTime::parse_from_str("1970-01-01 00:00:01.12345", "%Y-%m-%d %H:%M:%S%.f")
                .unwrap(),
        ),
        "Row 0: test_datetime"
    );
    assert_eq!(first.12, Some("row-one".to_string()), "Row 0: test_nullable");

    // Second row: false, 32767, 2147483647, -9223372036854775807, 3.14159, 2.71828, 12345.67, 'xyz', 'varchar2', 'longer informix text', '9999-12-31', '9999-12-31 23:59:59.99999', NULL
    let second = &rows[1];
    assert_eq!(second.0, Some(false), "Row 1: test_bool");
    assert_eq!(second.1, Some(32767), "Row 1: test_smallint");
    assert_eq!(second.2, Some(2147483647), "Row 1: test_int");
    assert_eq!(second.3, Some(-9223372036854775807), "Row 1: test_bigint");
    assert_eq!(second.4, Some(3.14159), "Row 1: test_float");
    assert_eq!(second.5, Some(2.71828), "Row 1: test_double");
    assert_eq!(second.6, Some(Decimal::new(1234567, 2)), "Row 1: test_decimal");
    assert_eq!(second.7, Some("xyz  ".to_string()), "Row 1: test_char");
    assert_eq!(second.8, Some("varchar2".to_string()), "Row 1: test_varchar");
    assert_eq!(second.9, Some("longer informix text".to_string()), "Row 1: test_text");
    assert_eq!(second.10, Some(NaiveDate::from_ymd_opt(9999, 12, 31).unwrap()), "Row 1: test_date");
    assert_eq!(
        second.11,
        Some(
            NaiveDateTime::parse_from_str("9999-12-31 23:59:59.99999", "%Y-%m-%d %H:%M:%S%.f")
                .unwrap(),
        ),
        "Row 1: test_datetime"
    );
    assert_eq!(second.12, None, "Row 1: test_nullable");

    // Third row: all NULL
    let third = &rows[2];
    assert_eq!(third.0, None, "Row 2: test_bool");
    assert_eq!(third.1, None, "Row 2: test_smallint");
    assert_eq!(third.2, None, "Row 2: test_int");
    assert_eq!(third.3, None, "Row 2: test_bigint");
    assert_eq!(third.4, None, "Row 2: test_float");
    assert_eq!(third.5, None, "Row 2: test_double");
    assert_eq!(third.6, None, "Row 2: test_decimal");
    assert_eq!(third.7, None, "Row 2: test_char");
    assert_eq!(third.8, None, "Row 2: test_varchar");
    assert_eq!(third.9, None, "Row 2: test_text");
    assert_eq!(third.10, None, "Row 2: test_date");
    assert_eq!(third.11, None, "Row 2: test_datetime");
    assert_eq!(third.12, None, "Row 2: test_nullable");
}
