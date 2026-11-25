//! r2d2 connection manager for `mysql` connections, from `r2d2_mysql` 25 (MIT).
//!
//! `r2d2_mysql` depends on `mysql` with its default features, which pull native-tls (OpenSSL on Linux) whatever
//! TLS backend is chosen here (`src_mysql` or `src_mysql_rustls`).

use mysql::{error::Error, Conn, Opts, OptsBuilder};

#[derive(Clone, Debug)]
pub struct MySqlConnectionManager {
    params: Opts,
}

impl MySqlConnectionManager {
    pub fn new(params: OptsBuilder) -> MySqlConnectionManager {
        MySqlConnectionManager {
            params: Opts::from(params),
        }
    }
}

impl r2d2::ManageConnection for MySqlConnectionManager {
    type Connection = Conn;
    type Error = Error;

    fn connect(&self) -> Result<Conn, Error> {
        Conn::new(self.params.clone())
    }

    fn is_valid(&self, conn: &mut Conn) -> Result<(), Error> {
        conn.ping()
    }

    fn has_broken(&self, conn: &mut Conn) -> bool {
        self.is_valid(conn).is_err()
    }
}
