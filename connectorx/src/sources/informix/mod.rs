mod errors;
mod typesystem;

pub use self::errors::InformixSourceError;
pub use self::typesystem::InformixTypeSystem;

use crate::{
    data_order::DataOrder,
    errors::ConnectorXError,
    sources::{PartitionParser, Produce, Source, SourcePartition},
    sql::CXQuery,
};
use anyhow::anyhow;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use fehler::{throw, throws};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use ibm_informix_bridge::{Connection, Statement};
use url::Url;
use urlencoding::decode;



#[throws(InformixSourceError)]
fn build_connection_string(conn: &str) -> String {
    let url = Url::parse(conn)?;
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

    // Allow a raw DRDA connection string via ?conn_str=... query param
    if let Some(raw) = params.get("conn_str") {
        return decode(raw)?.into_owned();
    }

    let mut parts: Vec<String> = vec![];

    // DRDA uses HOSTNAME (not HOST), PROTOCOL=TCPIP, PORT (not SERVICE)
    if let Some(host) = url.host_str() {
        parts.push(format!("HOSTNAME={}", decode(host)?.into_owned()));
    }

    // Default to 9089 (drsoctcp / DRDA).  9088 is the SQLI (libifcli) port.
    let port = url.port().unwrap_or(9089);
    parts.push(format!("PORT={}", port));

    parts.push("PROTOCOL=TCPIP".to_string());

    let db = url.path().trim_start_matches('/');
    if !db.is_empty() {
        parts.push(format!("DATABASE={}", decode(db)?.into_owned()));
    }

    if !url.username().is_empty() {
        parts.push(format!("UID={}", decode(url.username())?.into_owned()));
    }

    if let Some(password) = url.password() {
        parts.push(format!("PWD={}", decode(password)?.into_owned()));
    }

    // Forward unrecognised query params verbatim (server/schema are SQLI-only, skip them)
    for (k, v) in &params {
        if matches!(k.as_str(), "conn_str" | "server" | "schema") {
            continue;
        }
        parts.push(format!("{}={}", k.to_uppercase(), decode(v)?.into_owned()));
    }

    if parts.is_empty() {
        throw!(anyhow!(
            "informix connection string is empty; provide conn_str query parameter or URL fields"
        ));
    }

    parts.join(";")
}

#[throws(InformixSourceError)]
fn describe_columns(conn_str: &str, query: &str) -> (Vec<String>, Vec<InformixTypeSystem>) {
    let conn = Connection::connect(conn_str)?;
    let stmt = Statement::execute(&conn, query)?;

    let ncols = stmt.num_cols()?;
    if ncols == 0 {
        throw!(InformixSourceError::StatementError(
            "query did not return a result set".to_string(),
        ));
    }

    let mut names = Vec::with_capacity(ncols as usize);
    let mut schema = Vec::with_capacity(ncols as usize);

    for i in 1..=ncols {
        let col = stmt.describe_col(i)?;
        let name = if col.name.is_empty() {
            format!("COL{}", i)
        } else {
            col.name.clone()
        };
        names.push(name);
        schema.push(InformixTypeSystem::from(&col));
    }

    (names, schema)
}

#[throws(InformixSourceError)]
fn fetch_rows(conn_str: &str, query: &str, ncols: usize) -> Vec<Vec<Option<String>>> {
    let conn = Connection::connect(conn_str)?;
    let stmt = Statement::execute(&conn, query)?;

    let mut rows: Vec<Vec<Option<String>>> = vec![];

    loop {
        if !stmt.fetch()? {
            break;
        }

        let mut row = Vec::with_capacity(ncols);
        for col in 1..=ncols as u16 {
            row.push(stmt.get_data_string(col, 8192)?);
        }
        rows.push(row);
    }

    rows
}

pub struct InformixSource {
    conn_str: String,
    origin_query: Option<String>,
    queries: Vec<CXQuery<String>>,
    names: Vec<String>,
    schema: Vec<InformixTypeSystem>,
}

impl InformixSource {
    #[throws(InformixSourceError)]
    pub fn new(conn: &str, _nconn: usize) -> Self {
        Self {
            conn_str: build_connection_string(conn)?,
            origin_query: None,
            queries: vec![],
            names: vec![],
            schema: vec![],
        }
    }
}

impl Source for InformixSource
where
    InformixSourcePartition:
        SourcePartition<TypeSystem = InformixTypeSystem, Error = InformixSourceError>,
{
    const DATA_ORDERS: &'static [DataOrder] = &[DataOrder::RowMajor];
    type Partition = InformixSourcePartition;
    type TypeSystem = InformixTypeSystem;
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn set_data_order(&mut self, data_order: DataOrder) {
        if !matches!(data_order, DataOrder::RowMajor) {
            throw!(ConnectorXError::UnsupportedDataOrder(data_order));
        }
    }

    fn set_queries<Q: ToString>(&mut self, queries: &[CXQuery<Q>]) {
        self.queries = queries.iter().map(|q| q.map(Q::to_string)).collect();
    }

    fn set_origin_query(&mut self, query: Option<String>) {
        self.origin_query = query;
    }

    #[throws(InformixSourceError)]
    fn fetch_metadata(&mut self) {
        assert!(!self.queries.is_empty());
        let (names, schema) = describe_columns(&self.conn_str, self.queries[0].as_str())?;
        self.names = names;
        self.schema = schema;
    }

    #[throws(InformixSourceError)]
    fn result_rows(&mut self) -> Option<usize> {
        None
    }

    fn names(&self) -> Vec<String> {
        self.names.clone()
    }

    fn schema(&self) -> Vec<Self::TypeSystem> {
        self.schema.clone()
    }

    #[throws(InformixSourceError)]
    fn partition(self) -> Vec<Self::Partition> {
        let mut ret = vec![];
        for query in self.queries {
            ret.push(InformixSourcePartition::new(
                self.conn_str.clone(),
                query,
                &self.schema,
            ));
        }
        ret
    }
}

pub struct InformixSourcePartition {
    conn_str: String,
    query: CXQuery<String>,
    rows_cache: Option<Vec<Vec<Option<String>>>>,
    nrows: usize,
    ncols: usize,
}

impl InformixSourcePartition {
    pub fn new(conn_str: String, query: CXQuery<String>, schema: &[InformixTypeSystem]) -> Self {
        Self {
            conn_str,
            query,
            rows_cache: None,
            nrows: 0,
            ncols: schema.len(),
        }
    }

    #[throws(InformixSourceError)]
    fn ensure_rows_loaded(&mut self) {
        if self.rows_cache.is_none() {
            self.rows_cache = Some(fetch_rows(&self.conn_str, self.query.as_str(), self.ncols)?);
        }
    }
}

impl SourcePartition for InformixSourcePartition {
    type TypeSystem = InformixTypeSystem;
    type Parser<'a> = InformixSourceParser;
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn result_rows(&mut self) {
        self.ensure_rows_loaded()?;
        self.nrows = self
            .rows_cache
            .as_ref()
            .map(std::vec::Vec::len)
            .unwrap_or_default();
    }

    #[throws(InformixSourceError)]
    fn parser(&mut self) -> Self::Parser<'_> {
        self.ensure_rows_loaded()?;
        let rows = self.rows_cache.take().unwrap_or_default();
        self.nrows = rows.len();
        InformixSourceParser::new(rows, self.ncols)
    }

    fn nrows(&self) -> usize {
        self.nrows
    }

    fn ncols(&self) -> usize {
        self.ncols
    }
}

pub struct InformixSourceParser {
    rows: Vec<Vec<Option<String>>>,
    current_row: usize,
    current_col: usize,
    ncols: usize,
}

impl InformixSourceParser {
    pub fn new(rows: Vec<Vec<Option<String>>>, ncols: usize) -> Self {
        Self {
            rows,
            current_row: 0,
            current_col: 0,
            ncols,
        }
    }

    #[throws(InformixSourceError)]
    fn next_val(&mut self) -> Option<String> {
        if self.current_row >= self.rows.len() || self.current_col >= self.ncols {
            throw!(anyhow!("informix parser out of bounds"));
        }

        let val = self.rows[self.current_row][self.current_col].take();

        self.current_col += 1;
        if self.current_col == self.ncols {
            self.current_col = 0;
            self.current_row += 1;
        }

        val
    }

    #[throws(InformixSourceError)]
    fn next_required<T>(&mut self) -> T
    where
        T: FromStr,
        T::Err: std::fmt::Display,
    {
        let s = self
            .next_val()?
            .ok_or_else(|| ConnectorXError::cannot_produce::<T>(None))?;
        s.parse()
            .map_err(|_| ConnectorXError::cannot_produce::<T>(Some(s.into())))?
    }

    #[throws(InformixSourceError)]
    fn next_optional<T>(&mut self) -> Option<T>
    where
        T: FromStr,
        T::Err: std::fmt::Display,
    {
        match self.next_val()? {
            None => None,
            Some(s) => Some(
                s.parse()
                    .map_err(|_| ConnectorXError::cannot_produce::<T>(Some(s.into())))?,
            ),
        }
    }

    #[throws(InformixSourceError)]
    fn next_bool(&mut self) -> bool {
        let s = self
            .next_val()?
            .ok_or_else(|| ConnectorXError::cannot_produce::<bool>(None))?;
        match s.to_ascii_lowercase().as_str() {
            "t" | "true" | "1" | "y" | "yes" => true,
            "f" | "false" | "0" | "n" | "no" => false,
            _ => throw!(ConnectorXError::cannot_produce::<bool>(Some(s.into()))),
        }
    }

    #[throws(InformixSourceError)]
    fn next_optional_bool(&mut self) -> Option<bool> {
        match self.next_val()? {
            None => None,
            Some(s) => Some(match s.to_ascii_lowercase().as_str() {
                "t" | "true" | "1" | "y" | "yes" => true,
                "f" | "false" | "0" | "n" | "no" => false,
                _ => throw!(ConnectorXError::cannot_produce::<bool>(Some(s.into()))),
            }),
        }
    }
}

impl<'a> PartitionParser<'a> for InformixSourceParser {
    type TypeSystem = InformixTypeSystem;
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn fetch_next(&mut self) -> (usize, bool) {
        (self.rows.len(), true)
    }
}

impl<'r> Produce<'r, String> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> String {
        self.next_required::<String>()?
    }
}

impl<'r> Produce<'r, Option<String>> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> Option<String> {
        self.next_val()?
    }
}

macro_rules! impl_parse_num {
    ($t:ty) => {
        impl<'r> Produce<'r, $t> for InformixSourceParser {
            type Error = InformixSourceError;

            #[throws(InformixSourceError)]
            fn produce(&'r mut self) -> $t {
                self.next_required::<$t>()?
            }
        }

        impl<'r> Produce<'r, Option<$t>> for InformixSourceParser {
            type Error = InformixSourceError;

            #[throws(InformixSourceError)]
            fn produce(&'r mut self) -> Option<$t> {
                self.next_optional::<$t>()?
            }
        }
    };
}

impl_parse_num!(i16);
impl_parse_num!(i32);
impl_parse_num!(i64);
impl_parse_num!(f32);
impl_parse_num!(f64);
impl_parse_num!(Decimal);

impl<'r> Produce<'r, bool> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> bool {
        self.next_bool()?
    }
}

impl<'r> Produce<'r, Option<bool>> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> Option<bool> {
        self.next_optional_bool()?
    }
}

impl<'r> Produce<'r, NaiveDate> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> NaiveDate {
        let s = self.next_required::<String>()?;
        NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|_| ConnectorXError::cannot_produce::<NaiveDate>(Some(s.into())))?
    }
}

impl<'r> Produce<'r, Option<NaiveDate>> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> Option<NaiveDate> {
        match self.next_val()? {
            None => None,
            Some(s) => Some(
                NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                    .map_err(|_| ConnectorXError::cannot_produce::<NaiveDate>(Some(s.into())))?,
            ),
        }
    }
}

impl<'r> Produce<'r, NaiveTime> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> NaiveTime {
        let s = self.next_required::<String>()?;
        NaiveTime::parse_from_str(&s, "%H:%M:%S%.f")
            .map_err(|_| ConnectorXError::cannot_produce::<NaiveTime>(Some(s.into())))?
    }
}

impl<'r> Produce<'r, Option<NaiveTime>> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> Option<NaiveTime> {
        match self.next_val()? {
            None => None,
            Some(s) => Some(
                NaiveTime::parse_from_str(&s, "%H:%M:%S%.f")
                    .map_err(|_| ConnectorXError::cannot_produce::<NaiveTime>(Some(s.into())))?,
            ),
        }
    }
}

impl<'r> Produce<'r, NaiveDateTime> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> NaiveDateTime {
        let s = self.next_required::<String>()?;
        NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f")
            .map_err(|_| ConnectorXError::cannot_produce::<NaiveDateTime>(Some(s.into())))?
    }
}

impl<'r> Produce<'r, Option<NaiveDateTime>> for InformixSourceParser {
    type Error = InformixSourceError;

    #[throws(InformixSourceError)]
    fn produce(&'r mut self) -> Option<NaiveDateTime> {
        match self.next_val()? {
            None => None,
            Some(s) => Some(
                NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f")
                    .map_err(|_| ConnectorXError::cannot_produce::<NaiveDateTime>(Some(s.into())))?,
            ),
        }
    }
}
