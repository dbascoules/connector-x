#![cfg(all(feature = "src_informix", feature = "dst_arrow"))]

use arrow::{
    array::{Array, StringArray},
    record_batch::RecordBatch,
};
use connectorx::{
    destinations::arrow::ArrowDestination,
    prelude::*,
    sources::{informix::InformixSource, PartitionParser},
    sql::CXQuery,
    transports::InformixArrowTransport,
};
use std::env;

#[test]
#[ignore]
fn test_informix_load_and_parse() {
    let _ = env_logger::builder().is_test(true).try_init();

    let dburl = env::var("INFORMIX_URL").unwrap();
    let mut source = InformixSource::new(&dburl, 1)
        .unwrap_or_else(|e| panic!("InformixSource::new failed: {}", e));
    source.set_queries(&[CXQuery::naked(
        "select test_id, test_text, test_nullable from test_table order by test_id",
    )]);
    source
        .fetch_metadata()
        .unwrap_or_else(|e| panic!("fetch_metadata failed: {}", e));

    let mut partitions = source.partition().unwrap();
    assert_eq!(1, partitions.len());

    let mut partition = partitions.remove(0);
    partition.result_rows().expect("run query");
    assert!(partition.nrows() > 0, "Informix test table contains no rows");

    assert_eq!(3, partition.ncols());

    let mut parser = partition.parser().unwrap();

    let mut rows: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    loop {
        let (n, is_last) = parser.fetch_next().unwrap();
        for _ in 0..n {
            rows.push((
                parser.produce().unwrap(),
                parser.produce().unwrap(),
                parser.produce().unwrap(),
            ));
        }
        if is_last {
            break;
        }
    }

    assert!(!rows.is_empty(), "Informix query returned no rows");

    assert_eq!(
        vec![
            (
                "1".to_string(),
                Some("alpha".to_string()),
                Some("note-1".to_string()),
            ),
            ("2".to_string(), Some("beta".to_string()), None),
            ("3".to_string(), None, Some("note-3".to_string())),
        ],
        rows
    );
}

#[test]
#[ignore]
fn test_informix_arrow() {
    let _ = env_logger::builder().is_test(true).try_init();

    let dburl = env::var("INFORMIX_URL").unwrap();

    let queries = [
        CXQuery::naked(
            "select test_id, test_text, test_nullable from test_table where test_id <= '1'",
        ),
        CXQuery::naked(
            "select test_id, test_text, test_nullable from test_table where test_id > '1'",
        ),
    ];

    let builder = InformixSource::new(&dburl, queries.len())
        .unwrap_or_else(|e| panic!("InformixSource::new failed: {}", e));
    let mut destination = ArrowDestination::new();
    let dispatcher = Dispatcher::<_, _, InformixArrowTransport>::new(
        builder,
        &mut destination,
        &queries,
        Some(String::from(
            "select test_id, test_text, test_nullable from test_table",
        )),
    );

    dispatcher
        .run()
        .unwrap_or_else(|e| panic!("dispatcher.run failed: {}", e));

    let result = destination.arrow().unwrap();
    let total_rows: usize = result.iter().map(|rb| rb.num_rows()).sum();
    assert!(total_rows > 0, "Informix arrow query returned no rows");
    verify_arrow_results(result);
}

fn verify_arrow_results(result: Vec<RecordBatch>) {
    assert_eq!(2, result.len());

    let mut rows: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for rb in result {
        assert_eq!(3, rb.num_columns());

        let ids = rb.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        let texts = rb.column(1).as_any().downcast_ref::<StringArray>().unwrap();
        let nullable = rb.column(2).as_any().downcast_ref::<StringArray>().unwrap();

        for i in 0..rb.num_rows() {
            let text = if texts.is_null(i) {
                None
            } else {
                Some(texts.value(i).to_string())
            };

            let nullable_value = if nullable.is_null(i) {
                None
            } else {
                Some(nullable.value(i).to_string())
            };

            rows.push((ids.value(i).to_string(), text, nullable_value));
        }
    }

    rows.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        vec![
            (
                "1".to_string(),
                Some("alpha".to_string()),
                Some("note-1".to_string()),
            ),
            ("2".to_string(), Some("beta".to_string()), None),
            ("3".to_string(), None, Some("note-3".to_string())),
        ],
        rows
    );
}