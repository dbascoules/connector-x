//! ibm_informix_bridge — minimal safe Rust wrapper over IBM CLI libraries.
//!
//! Build-time client selection (via `build.rs`):
//! - `libdb2`  (DRDA, typically port 9089)
//! - `libifcli`/`iclit09*` (Informix SQLI onsoctcp, typically port 9088)
//!
//! # Connection string
//! ```
//! DATABASE=mydb;HOSTNAME=host;PORT=9089;PROTOCOL=TCPIP;UID=user;PWD=pass;
//! ```
//!
//! # Usage
//! ```no_run
//! use ibm_informix_bridge::{Connection, Statement};
//!
//! let conn = Connection::connect(
//!     "DATABASE=connectorx;HOSTNAME=localhost;PORT=9089;PROTOCOL=TCPIP;UID=informix;PWD=in4mix;"
//! ).unwrap();
//!
//! let stmt = Statement::execute(&conn, "SELECT id, name FROM test_table").unwrap();
//! let ncols = stmt.num_cols().unwrap();
//!
//! while stmt.fetch().unwrap() {
//!     for col in 1..=ncols {
//!         let v = stmt.get_data_string(col, 256).unwrap();
//!         print!("{:?}  ", v);
//!     }
//!     println!();
//! }
//! ```

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_long, c_short, c_ulong, c_void};

// ---------------------------------------------------------------------------
// ODBC / DB2 CLI constants
// ---------------------------------------------------------------------------

const SQL_SUCCESS: c_short = 0;
const SQL_SUCCESS_WITH_INFO: c_short = 1;
const SQL_NO_DATA: c_short = 100;

pub const SQL_NULL_DATA: c_int = -1;

// Handle types
const SQL_HANDLE_ENV: c_short = 1;
const SQL_HANDLE_DBC: c_short = 2;
const SQL_HANDLE_STMT: c_short = 3;

// SQLGetData / SQLBindCol target types
const SQL_C_CHAR: c_short = 1;

// SQLDriverConnect option
const SQL_DRIVER_NOPROMPT: c_short = 0;

// Environment attribute
const SQL_ATTR_ODBC_VERSION: c_int = 200;
const SQL_OV_ODBC3: usize = 3;

// Null-Terminated String indicator
const SQL_NTS: c_long = -3;

// Diag buffer size
const DIAG_BUF: usize = 1024;

// ---------------------------------------------------------------------------
// Raw FFI declarations (identical API surface for libifcli and libdb2)
// ---------------------------------------------------------------------------

extern "C" {
    fn SQLAllocHandle(
        handle_type: c_short,
        input_handle: *mut c_void,
        output_handle: *mut *mut c_void,
    ) -> c_short;

    fn SQLSetEnvAttr(
        env_handle: *mut c_void,
        attribute: c_int,
        value: usize,
        string_length: c_int,
    ) -> c_short;

    fn SQLFreeHandle(handle_type: c_short, handle: *mut c_void) -> c_short;

    fn SQLDriverConnect(
        dbc_handle: *mut c_void,
        window_handle: *mut c_void,
        in_conn_str: *const c_char,
        in_conn_str_len: c_short,
        out_conn_str: *mut c_char,
        out_conn_str_buf_len: c_short,
        out_conn_str_len: *mut c_short,
        driver_completion: c_short,
    ) -> c_short;

    fn SQLDisconnect(dbc_handle: *mut c_void) -> c_short;

    fn SQLExecDirect(
        stmt_handle: *mut c_void,
        statement_text: *const c_char,
        text_length: c_long,
    ) -> c_short;

    fn SQLNumResultCols(stmt_handle: *mut c_void, column_count: *mut c_short) -> c_short;

    #[allow(clippy::too_many_arguments)]
    fn SQLDescribeCol(
        stmt_handle: *mut c_void,
        col_number: c_short,
        col_name: *mut c_char,
        buf_length: c_short,
        name_length: *mut c_short,
        data_type: *mut c_short,
        col_size: *mut c_ulong,
        decimal_digits: *mut c_short,
        nullable: *mut c_short,
    ) -> c_short;

    fn SQLFetch(stmt_handle: *mut c_void) -> c_short;

    fn SQLGetData(
        stmt_handle: *mut c_void,
        col_number: c_short,
        target_type: c_short,
        target_value: *mut c_void,
        buffer_length: c_int,
        strlen_or_ind: *mut c_int,
    ) -> c_short;

    fn SQLGetDiagRec(
        handle_type: c_short,
        handle: *mut c_void,
        rec_number: c_short,
        sql_state: *mut c_char,
        native_error: *mut c_long,
        message_text: *mut c_char,
        buf_length: c_short,
        text_length: *mut c_short,
    ) -> c_short;
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Handle allocation failed: rc={0}")]
    HandleAlloc(i16),

    #[error("Environment attribute error: rc={0}")]
    EnvAttr(i16),

    #[error("Connection failed: {0}")]
    Connect(String),

    #[error("Statement execution failed: {0}")]
    Execute(String),

    #[error("Column description failed: {0}")]
    DescribeCol(String),

    #[error("Data fetch failed: {0}")]
    Fetch(String),

    #[error("GetData failed: {0}")]
    GetData(String),

    #[error(transparent)]
    NulError(#[from] std::ffi::NulError),
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Collect the first diagnostic record from any CLI handle.
fn get_diag(handle_type: c_short, handle: *mut c_void) -> String {
    let mut state = [0i8; 6];
    let mut native: c_long = 0;
    let mut msg = vec![0i8; DIAG_BUF];
    let mut msg_len: c_short = 0;

    let rc = unsafe {
        SQLGetDiagRec(
            handle_type,
            handle,
            1,
            state.as_mut_ptr(),
            &mut native,
            msg.as_mut_ptr(),
            DIAG_BUF as c_short,
            &mut msg_len,
        )
    };

    if rc == SQL_SUCCESS || rc == SQL_SUCCESS_WITH_INFO {
        let text = unsafe { CStr::from_ptr(msg.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let state_str = unsafe { CStr::from_ptr(state.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        format!("[{}] {}", state_str, text)
    } else {
        format!("(no diagnostic available, SQLGetDiagRec rc={})", rc)
    }
}

#[inline]
fn is_ok(rc: c_short) -> bool {
    rc == SQL_SUCCESS || rc == SQL_SUCCESS_WITH_INFO
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// An active DRDA connection to an Informix server.
///
/// Allocates an ODBC/CLI environment handle (SQL_OV_ODBC3) and a DBC handle.
/// Both are freed on drop.
pub struct Connection {
    env: *mut c_void,
    dbc: *mut c_void,
}

// Required because we expose raw pointers; callers must not share across threads.
unsafe impl Send for Connection {}

impl Connection {
    /// Establish a connection using an ODBC-style connection string.
    ///
    /// Typical Informix DRDA string:
    /// ```
    /// DATABASE=mydb;HOSTNAME=host;PORT=9089;PROTOCOL=TCPIP;UID=user;PWD=pass;
    /// ```
    pub fn connect(dsn: &str) -> Result<Self, BridgeError> {
        let mut env: *mut c_void = std::ptr::null_mut();
        let mut dbc: *mut c_void = std::ptr::null_mut();

        // SQLAllocHandle(SQL_HANDLE_ENV)
        let rc = unsafe {
            SQLAllocHandle(
                SQL_HANDLE_ENV,
                std::ptr::null_mut(),
                &mut env as *mut _,
            )
        };
        if !is_ok(rc) {
            return Err(BridgeError::HandleAlloc(rc));
        }

        // Set ODBC version 3
        let rc = unsafe {
            SQLSetEnvAttr(env, SQL_ATTR_ODBC_VERSION, SQL_OV_ODBC3, 0)
        };
        if !is_ok(rc) {
            unsafe { SQLFreeHandle(SQL_HANDLE_ENV, env) };
            return Err(BridgeError::EnvAttr(rc));
        }

        // SQLAllocHandle(SQL_HANDLE_DBC)
        let rc = unsafe { SQLAllocHandle(SQL_HANDLE_DBC, env, &mut dbc as *mut _) };
        if !is_ok(rc) {
            unsafe { SQLFreeHandle(SQL_HANDLE_ENV, env) };
            return Err(BridgeError::HandleAlloc(rc));
        }

        // SQLDriverConnect
        let dsn_c = CString::new(dsn)?;
        let mut out_buf = vec![0i8; 1024];
        let mut out_len: c_short = 0;

        let rc = unsafe {
            SQLDriverConnect(
                dbc,
                std::ptr::null_mut(),
                dsn_c.as_ptr(),
                SQL_NTS as c_short,
                out_buf.as_mut_ptr(),
                out_buf.len() as c_short,
                &mut out_len,
                SQL_DRIVER_NOPROMPT,
            )
        };

        if !is_ok(rc) {
            let msg = get_diag(SQL_HANDLE_DBC, dbc);
            unsafe {
                SQLFreeHandle(SQL_HANDLE_DBC, dbc);
                SQLFreeHandle(SQL_HANDLE_ENV, env);
            }
            return Err(BridgeError::Connect(msg));
        }

        Ok(Connection { env, dbc })
    }

    fn dbc(&self) -> *mut c_void {
        self.dbc
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            SQLDisconnect(self.dbc);
            SQLFreeHandle(SQL_HANDLE_DBC, self.dbc);
            SQLFreeHandle(SQL_HANDLE_ENV, self.env);
        }
    }
}

// ---------------------------------------------------------------------------
// Statement
// ---------------------------------------------------------------------------

/// An executed SQL statement.  Freed on drop.
pub struct Statement {
    handle: *mut c_void,
}

unsafe impl Send for Statement {}

/// Column metadata returned by [`Statement::describe_col`].
#[derive(Debug, Clone)]
pub struct ColDesc {
    pub name: String,
    pub sql_type: i16,
    pub size: usize,
    pub nullable: bool,
}

impl Statement {
    /// Execute `sql` on `conn` and return a `Statement` ready for fetching.
    pub fn execute(conn: &Connection, sql: &str) -> Result<Self, BridgeError> {
        let mut stmt: *mut c_void = std::ptr::null_mut();

        let rc = unsafe {
            SQLAllocHandle(SQL_HANDLE_STMT, conn.dbc(), &mut stmt as *mut _)
        };
        if !is_ok(rc) {
            return Err(BridgeError::HandleAlloc(rc));
        }

        let sql_c = CString::new(sql)?;
        let rc = unsafe { SQLExecDirect(stmt, sql_c.as_ptr(), SQL_NTS) };

        if !is_ok(rc) {
            let msg = get_diag(SQL_HANDLE_STMT, stmt);
            unsafe { SQLFreeHandle(SQL_HANDLE_STMT, stmt) };
            return Err(BridgeError::Execute(msg));
        }

        Ok(Statement { handle: stmt })
    }

    /// Number of columns in the result set.
    pub fn num_cols(&self) -> Result<u16, BridgeError> {
        let mut ncols: c_short = 0;
        let rc = unsafe { SQLNumResultCols(self.handle, &mut ncols) };
        if !is_ok(rc) {
            let msg = get_diag(SQL_HANDLE_STMT, self.handle);
            return Err(BridgeError::Execute(msg));
        }
        Ok(ncols as u16)
    }

    /// Describe column `col` (1-based).
    pub fn describe_col(&self, col: u16) -> Result<ColDesc, BridgeError> {
        let mut name_buf = vec![0i8; 256];
        let mut name_len: c_short = 0;
        let mut data_type: c_short = 0;
        let mut col_size: c_ulong = 0;
        let mut decimal_digits: c_short = 0;
        let mut nullable: c_short = 0;

        let rc = unsafe {
            SQLDescribeCol(
                self.handle,
                col as c_short,
                name_buf.as_mut_ptr(),
                name_buf.len() as c_short,
                &mut name_len,
                &mut data_type,
                &mut col_size,
                &mut decimal_digits,
                &mut nullable,
            )
        };

        if !is_ok(rc) {
            let msg = get_diag(SQL_HANDLE_STMT, self.handle);
            return Err(BridgeError::DescribeCol(msg));
        }

        let name = unsafe { CStr::from_ptr(name_buf.as_ptr()) }
            .to_string_lossy()
            .into_owned();

        Ok(ColDesc {
            name,
            sql_type: data_type,
            size: col_size as usize,
            nullable: nullable != 0,
        })
    }

    /// Advance to the next row.  Returns `true` if a row was fetched, `false` at EOF.
    pub fn fetch(&self) -> Result<bool, BridgeError> {
        let rc = unsafe { SQLFetch(self.handle) };
        match rc {
            _ if is_ok(rc) => Ok(true),
            SQL_NO_DATA => Ok(false),
            _ => {
                let msg = get_diag(SQL_HANDLE_STMT, self.handle);
                Err(BridgeError::Fetch(msg))
            }
        }
    }

    /// Retrieve column `col` (1-based) from the current row as a UTF-8 string.
    ///
    /// Returns `None` for SQL NULL values.
    /// `buf_len` sets the maximum byte capacity of the internal buffer;
    /// use a larger value for `TEXT` / `LVARCHAR` columns.
    pub fn get_data_string(
        &self,
        col: u16,
        buf_len: usize,
    ) -> Result<Option<String>, BridgeError> {
        let mut buf = vec![0i8; buf_len + 1];
        let mut ind: c_int = 0;

        let rc = unsafe {
            SQLGetData(
                self.handle,
                col as c_short,
                SQL_C_CHAR,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as c_int,
                &mut ind,
            )
        };

        if !is_ok(rc) {
            let msg = get_diag(SQL_HANDLE_STMT, self.handle);
            return Err(BridgeError::GetData(msg));
        }

        if ind == SQL_NULL_DATA {
            return Ok(None);
        }

        let s = unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned();

        Ok(Some(s))
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        unsafe {
            SQLFreeHandle(SQL_HANDLE_STMT, self.handle);
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience builder for DRDA connection strings
// ---------------------------------------------------------------------------

/// Build a DRDA connection string for Informix.
///
/// ```
/// let dsn = ibm_informix_bridge::drda_dsn("connectorx", "localhost", 9089, "informix", "in4mix");
/// assert!(dsn.contains("PROTOCOL=TCPIP"));
/// ```
pub fn drda_dsn(database: &str, hostname: &str, port: u16, uid: &str, pwd: &str) -> String {
    format!(
        "DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={};",
        database, hostname, port, uid, pwd
    )
}
