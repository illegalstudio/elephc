//! Purpose:
//! Pure-Rust Firebird backend matching php-src's `pdo_firebird` surface.
//!
//! Called from:
//! - The bridge root when built with the optional `firebird` feature.
//!
//! Key details:
//! - Uses Firebird's wire protocol through `rsfbclient` on every supported target.
//! - Preserves PDO positional/named binding, scalar shapes, transaction controls,
//!   date formatting attributes, and driver-native diagnostics.

use std::collections::HashMap;
use std::str::FromStr;

use rsfbclient::prelude::{transaction_builder, Execute, Queryable, TrRecordVersion};
use rsfbclient::{builder_pure_rust, Dialect, FbError, Row, SimpleConnection, SqlType};

const ATTR_DATE_FORMAT: i64 = 1000;
const ATTR_TIME_FORMAT: i64 = 1001;
const ATTR_TIMESTAMP_FORMAT: i64 = 1002;
const TRANSACTION_ISOLATION_LEVEL: i64 = 1003;
const READ_COMMITTED: i64 = 1004;
const REPEATABLE_READ: i64 = 1005;
const SERIALIZABLE: i64 = 1006;
const WRITABLE_TRANSACTION: i64 = 1007;

/// Parsed Firebird DSN fields with php-src-compatible defaults.
struct DsnOptions {
    host: String,
    port: u16,
    dbname: String,
    user: String,
    password: String,
    charset: String,
    role: Option<String>,
    dialect: Dialect,
    date_format: String,
    time_format: String,
    timestamp_format: String,
    isolation: i64,
    writable: bool,
}

/// Decodes constructor credentials folded into the bridge DSN.
fn percent_decode_credential(raw: &str) -> String {
    raw.replace("%3B", ";")
        .replace("%3b", ";")
        .replace("%25", "%")
}

/// Splits php-src's Firebird `dbname` connection string into wire host, port,
/// and server-side database path/alias.
fn split_remote_dbname(dbname: String) -> (String, u16, String) {
    for prefix in ["inet://", "inet4://", "inet6://"] {
        if let Some(rest) = dbname.strip_prefix(prefix) {
            let Some((authority, path)) = rest.split_once('/') else {
                return ("localhost".to_string(), 3050, rest.to_string());
            };
            let (host, port) = split_host_port(authority, ':');
            let path = if path.starts_with('/') {
                path.to_string()
            } else {
                path.to_string()
            };
            return (host, port, path);
        }
    }

    let separator = if dbname.starts_with('[') {
        dbname.find(']').and_then(|end| {
            dbname[end + 1..]
                .find(':')
                .map(|offset| end + 1 + offset)
        })
    } else {
        dbname.find(':').filter(|index| *index != 1)
    };
    let Some(separator) = separator else {
        return ("localhost".to_string(), 3050, dbname);
    };
    let authority = &dbname[..separator];
    let path = dbname[separator + 1..].to_string();
    let (host, port) = split_host_port(authority, '/');
    (host, port, path)
}

/// Parses a host plus optional numeric port from one Firebird authority.
fn split_host_port(authority: &str, separator: char) -> (String, u16) {
    let unbracketed = authority
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(authority);
    let split = if authority.starts_with('[') && separator == ':' {
        authority.rfind("]:").map(|index| (&authority[..=index], &authority[index + 2..]))
    } else {
        authority.rsplit_once(separator)
    };
    if let Some((host, port)) = split {
        if let Ok(port) = port.parse::<u16>() {
            return (host.trim_matches(['[', ']']).to_string(), port);
        }
    }
    (unbracketed.to_string(), 3050)
}

/// Parses the semicolon-separated `firebird:` DSN and internal constructor options.
fn parse_dsn(dsn: &str) -> Result<DsnOptions, String> {
    let body = dsn
        .strip_prefix("firebird:")
        .ok_or_else(|| "could not find driver".to_string())?;
    let mut values = HashMap::new();
    for pair in body.split(';').filter(|pair| !pair.is_empty()) {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        values.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let dbname = values
        .remove("dbname")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PDO_FIREBIRD: DSN requires dbname".to_string())?;
    let dialect = Dialect::from_str(values.get("dialect").map_or("3", String::as_str))
        .map_err(|error| error.to_string())?;
    let isolation = values
        .get("transaction_isolation")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(REPEATABLE_READ);
    if !matches!(isolation, READ_COMMITTED | REPEATABLE_READ | SERIALIZABLE) {
        return Err("Pdo\\Firebird::TRANSACTION_ISOLATION_LEVEL must be a valid transaction isolation level (Pdo\\Firebird::READ_COMMITTED, Pdo\\Firebird::REPEATABLE_READ, or Pdo\\Firebird::SERIALIZABLE)".to_string());
    }
    let (dsn_host, dsn_port, dbname) = split_remote_dbname(dbname);
    Ok(DsnOptions {
        host: values.remove("host").unwrap_or(dsn_host),
        port: values.remove("port").and_then(|value| value.parse().ok()).unwrap_or(dsn_port),
        dbname,
        user: percent_decode_credential(
            &values.remove("user").unwrap_or_else(|| "SYSDBA".to_string()),
        ),
        password: percent_decode_credential(
            &values
                .remove("password")
                .unwrap_or_else(|| "masterkey".to_string()),
        ),
        charset: values.remove("charset").unwrap_or_else(|| "UTF-8".to_string()),
        role: values.remove("role").filter(|value| !value.is_empty()),
        dialect,
        date_format: values
            .remove("date_format")
            .unwrap_or_else(|| "%Y-%m-%d".to_string()),
        time_format: values
            .remove("time_format")
            .unwrap_or_else(|| "%H:%M:%S".to_string()),
        timestamp_format: values
            .remove("timestamp_format")
            .unwrap_or_else(|| "%Y-%m-%d %H:%M:%S".to_string()),
        isolation,
        writable: values
            .get("writable_transaction")
            .and_then(|value| value.parse::<i64>().ok())
            .map_or(true, |value| value != 0),
    })
}

/// One PDO-visible diagnostic produced by Firebird or the bridge.
#[derive(Clone)]
struct ErrorState {
    sqlstate: String,
    native_code: i64,
    message: String,
}

impl Default for ErrorState {
    /// Creates the PDO no-error state.
    fn default() -> Self {
        Self {
            sqlstate: "00000".to_string(),
            native_code: 0,
            message: String::new(),
        }
    }
}

/// Maps a Firebird SQLCODE into the closest SQLSTATE emitted by php-src.
fn sqlstate_for_code(code: i32) -> &'static str {
    match code {
        -803 | -530 | -625 => "23000",
        -204 => "42S02",
        -206 => "42S22",
        -104 => "42000",
        -902 | -923 | -924 => "08006",
        _ => "HY000",
    }
}

/// Converts an `rsfbclient` error into PDO diagnostic state.
fn error_state(error: FbError) -> ErrorState {
    match error {
        FbError::Sql { msg, code } => ErrorState {
            sqlstate: sqlstate_for_code(code).to_string(),
            native_code: i64::from(code),
            message: msg,
        },
        other => ErrorState {
            sqlstate: "HY000".to_string(),
            native_code: 0,
            message: other.to_string(),
        },
    }
}

/// Live Firebird connection and its PDO-visible configuration.
pub struct FirebirdConn {
    connection: SimpleConnection,
    error: ErrorState,
    pub changes: i64,
    pub in_transaction: bool,
    auto_commit: bool,
    fetch_table_names: bool,
    date_format: String,
    time_format: String,
    timestamp_format: String,
    isolation: i64,
    writable: bool,
}

// The bridge serializes all uses under its connection-table mutex.
unsafe impl Send for FirebirdConn {}

impl FirebirdConn {
    /// Opens a remote Firebird attachment from a PDO DSN.
    pub fn open(dsn: &str) -> Result<Self, String> {
        let options = parse_dsn(dsn)?;
        let charset = rsfbclient::Charset::from_str(&options.charset)
            .map_err(|error| error.to_string())?;
        let mut builder = builder_pure_rust();
        builder
            .host(options.host)
            .port(options.port)
            .db_name(options.dbname)
            .user(options.user)
            .pass(options.password)
            .charset(charset)
            .dialect(options.dialect);
        if let Some(role) = options.role {
            builder.role(role);
        }
        let connection = builder.connect().map_err(|error| error.to_string())?.into();
        Ok(Self {
            connection,
            error: ErrorState::default(),
            changes: 0,
            in_transaction: false,
            auto_commit: true,
            fetch_table_names: false,
            date_format: options.date_format,
            time_format: options.time_format,
            timestamp_format: options.timestamp_format,
            isolation: options.isolation,
            writable: options.writable,
        })
    }

    /// Clears the diagnostic before a new native operation.
    fn clear_error(&mut self) {
        self.error = ErrorState::default();
    }

    /// Records one client error and returns its text to the caller.
    fn record_error(&mut self, error: FbError) -> String {
        self.error = error_state(error);
        self.error.message.clone()
    }

    /// Reports whether a lightweight system-table query succeeds.
    pub fn is_alive(&mut self) -> bool {
        self.connection
            .query_first::<_, (i32,)>("SELECT 1 FROM RDB$DATABASE", ())
            .is_ok()
    }

    /// Executes SQL and returns one materialized result set.
    pub fn execute(&mut self, sql: &str, params: Vec<SqlType>) -> Result<FirebirdResult, String> {
        self.clear_error();
        let keyword = leading_keyword(sql);
        if keyword == "SELECT" || keyword == "WITH" {
            match self.connection.query::<_, Row>(sql, params) {
                Ok(rows) => {
                    let result = FirebirdResult::from_rows(rows, self);
                    self.changes = result.rows.len() as i64;
                    Ok(result)
                }
                Err(error) => Err(self.record_error(error)),
            }
        } else if has_sql_keyword(sql, "RETURNING") {
            match self.connection.execute_returnable::<_, Row>(sql, params) {
                Ok(row) => {
                    let result = FirebirdResult::from_rows(vec![row], self);
                    self.changes = 1;
                    Ok(result)
                }
                Err(error) => Err(self.record_error(error)),
            }
        } else {
            match self.connection.execute(sql, params) {
                Ok(changes) => {
                    self.changes = changes as i64;
                    Ok(FirebirdResult::empty(changes as i64))
                }
                Err(error) => Err(self.record_error(error)),
            }
        }
    }

    /// Starts a manual transaction with the configured PDO_FIREBIRD mode.
    pub fn begin(&mut self) -> bool {
        if self.in_transaction {
            return false;
        }
        let mut builder = transaction_builder();
        match self.isolation {
            READ_COMMITTED => {
                builder.with_read_commited(TrRecordVersion::NoRecordVersion);
            }
            REPEATABLE_READ => {
                builder.with_concurrency();
            }
            SERIALIZABLE => {
                builder.with_consistency();
            }
            _ => return false,
        }
        if self.writable {
            builder.read_write();
        } else {
            builder.read_only();
        }
        match self.connection.begin_transaction_config(builder.build()) {
            Ok(()) => {
                self.in_transaction = true;
                true
            }
            Err(error) => {
                self.record_error(error);
                false
            }
        }
    }

    /// Commits the active manual transaction.
    pub fn commit(&mut self) -> bool {
        match self.connection.commit() {
            Ok(()) => {
                self.in_transaction = false;
                true
            }
            Err(error) => {
                self.record_error(error);
                false
            }
        }
    }

    /// Rolls back the active manual transaction.
    pub fn rollback(&mut self) -> bool {
        match self.connection.rollback() {
            Ok(()) => {
                self.in_transaction = false;
                true
            }
            Err(error) => {
                self.record_error(error);
                false
            }
        }
    }

    /// Returns the current SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        &self.error.sqlstate
    }

    /// Returns the current Firebird SQLCODE.
    pub fn errcode(&self) -> i64 {
        self.error.native_code
    }

    /// Returns the current driver diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }

    /// Stores a bridge-generated PDO error.
    pub fn set_error(&mut self, sqlstate: &str, message: String) {
        self.error = ErrorState {
            sqlstate: sqlstate.to_string(),
            native_code: 0,
            message,
        };
    }

    /// Returns the connected server's version string.
    pub fn server_version(&mut self) -> String {
        self.connection
            .query_first::<_, (String,)>(
                "SELECT RDB$GET_CONTEXT('SYSTEM', 'ENGINE_VERSION') FROM RDB$DATABASE",
                (),
            )
            .ok()
            .flatten()
            .map(|row| row.0)
            .unwrap_or_default()
    }

    /// Returns the pure-Rust Firebird client identity.
    pub fn client_version(&self) -> String {
        "rsfbclient-rust 0.27".to_string()
    }

    /// Returns a stable connection-status string.
    pub fn connection_status(&mut self) -> String {
        if self.is_alive() {
            "1".to_string()
        } else {
            "0".to_string()
        }
    }

    /// Reads a driver-specific integer/boolean attribute.
    pub fn attribute_int(&self, attribute: i64) -> Option<i64> {
        match attribute {
            0 => Some(self.auto_commit as i64),
            14 => Some(self.fetch_table_names as i64),
            TRANSACTION_ISOLATION_LEVEL => Some(self.isolation),
            WRITABLE_TRANSACTION => Some(self.writable as i64),
            _ => None,
        }
    }

    /// Reads a driver-specific format-string attribute.
    pub fn attribute_text(&self, attribute: i64) -> Option<&str> {
        match attribute {
            ATTR_DATE_FORMAT => Some(&self.date_format),
            ATTR_TIME_FORMAT => Some(&self.time_format),
            ATTR_TIMESTAMP_FORMAT => Some(&self.timestamp_format),
            _ => None,
        }
    }

    /// Updates an integer/boolean PDO_FIREBIRD attribute outside a transaction.
    pub fn set_attribute_int(&mut self, attribute: i64, value: i64) -> bool {
        if self.in_transaction && matches!(attribute, 0 | TRANSACTION_ISOLATION_LEVEL | WRITABLE_TRANSACTION) {
            self.set_error("HY000", "Cannot change transaction settings while a transaction is already open".to_string());
            return false;
        }
        match attribute {
            0 => {
                self.auto_commit = value != 0;
                true
            }
            14 => {
                self.fetch_table_names = value != 0;
                true
            }
            TRANSACTION_ISOLATION_LEVEL
                if matches!(value, READ_COMMITTED | REPEATABLE_READ | SERIALIZABLE) =>
            {
                self.isolation = value;
                true
            }
            WRITABLE_TRANSACTION => {
                self.writable = value != 0;
                true
            }
            _ => false,
        }
    }

    /// Updates a PDO_FIREBIRD date/time formatting string.
    pub fn set_attribute_text(&mut self, attribute: i64, value: String) -> bool {
        match attribute {
            ATTR_DATE_FORMAT => self.date_format = value,
            ATTR_TIME_FORMAT => self.time_format = value,
            ATTR_TIMESTAMP_FORMAT => self.timestamp_format = value,
            _ => return false,
        }
        true
    }
}

/// Returns the first executable SQL keyword while skipping leading whitespace/comments.
fn leading_keyword(sql: &str) -> String {
    sql_keywords(sql).into_iter().next().unwrap_or_default()
}

/// Reports whether executable SQL contains one standalone keyword outside
/// strings, quoted identifiers, and comments.
fn has_sql_keyword(sql: &str, expected: &str) -> bool {
    sql_keywords(sql).iter().any(|keyword| keyword == expected)
}

/// Tokenizes executable ASCII SQL words while ignoring quoted/commented text.
fn sql_keywords(sql: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut words = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if bytes[index] == b'-' && bytes.get(index + 1) == Some(&b'-') {
            index += 2;
            while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len()
                && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
            {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"') {
            let quote = bytes[index];
            index += 1;
            while index < bytes.len() {
                if bytes[index] == quote {
                    if bytes.get(index + 1) == Some(&quote) {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric()
                    || matches!(bytes[index], b'_' | b'$'))
            {
                index += 1;
            }
            words.push(sql[start..index].to_ascii_uppercase());
            continue;
        }
        index += 1;
    }
    words
}

/// One materialized Firebird result column.
pub struct FirebirdColumn {
    pub name: String,
    pub raw_type: i64,
}

/// One materialized PDO scalar.
#[derive(Clone)]
pub enum FirebirdCell {
    Null,
    Int(i64),
    Float(f64),
    Bytes(Vec<u8>, bool),
}

/// One materialized Firebird result set.
pub struct FirebirdResult {
    columns: Vec<FirebirdColumn>,
    rows: Vec<Vec<FirebirdCell>>,
}

impl FirebirdResult {
    /// Creates an empty DML/DDL result with an affected-row count.
    fn empty(_row_count: i64) -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// Converts wire rows into stable bridge-owned metadata and scalar values.
    fn from_rows(rows: Vec<Row>, connection: &FirebirdConn) -> Self {
        let columns = rows.first().map_or_else(Vec::new, |row| {
            row.cols
                .iter()
                .map(|column| FirebirdColumn {
                    name: column.name.clone(),
                    raw_type: i64::from(column.raw_type),
                })
                .collect()
        });
        let rows = rows
            .into_iter()
            .map(|row| {
                row.cols
                    .into_iter()
                    .map(|column| decode_cell(column.raw_type, column.value, connection))
                    .collect()
            })
            .collect::<Vec<_>>();
        Self { columns, rows }
    }
}

/// Converts one Firebird wire scalar using the active PDO date-format attributes.
fn decode_cell(raw_type: u32, value: SqlType, connection: &FirebirdConn) -> FirebirdCell {
    match value {
        SqlType::Null => FirebirdCell::Null,
        SqlType::Integer(value) => FirebirdCell::Int(value),
        SqlType::Floating(value) => FirebirdCell::Float(value),
        SqlType::Boolean(value) => FirebirdCell::Int(value as i64),
        SqlType::Binary(value) => FirebirdCell::Bytes(value, true),
        SqlType::Text(value) => FirebirdCell::Bytes(value.into_bytes(), false),
        SqlType::Timestamp(value) => {
            let format = match raw_type & !1 {
                570 => &connection.date_format,
                560 => &connection.time_format,
                _ => &connection.timestamp_format,
            };
            FirebirdCell::Bytes(value.format(format).to_string().into_bytes(), false)
        }
    }
}

/// Prepared Firebird statement with PDO binding and buffered result state.
pub struct FirebirdStmt {
    pub conn_id: i64,
    sql: String,
    named_map: HashMap<String, i64>,
    order: Vec<i64>,
    binds: Vec<SqlType>,
    bound: Vec<bool>,
    result: FirebirdResult,
    cursor: isize,
    executed: bool,
    pub sent_sql: String,
    cursor_name: Option<String>,
    error: ErrorState,
}

impl FirebirdStmt {
    /// Creates a Firebird statement and normalizes named placeholders to `?`.
    pub fn new(conn_id: i64, sql: &str) -> Result<Self, String> {
        let (translated, named_map, order, mixed) = crate::my::translate_placeholders(sql, false);
        if mixed {
            return Err("Invalid parameter number: mixed named and positional parameters".to_string());
        }
        let slots = order.iter().copied().max().unwrap_or(0).max(0) as usize;
        Ok(Self {
            conn_id,
            sql: translated,
            named_map,
            order,
            binds: vec![SqlType::Null; slots],
            bound: vec![false; slots],
            result: FirebirdResult::empty(0),
            cursor: -1,
            executed: false,
            sent_sql: String::new(),
            cursor_name: None,
            error: ErrorState::default(),
        })
    }

    /// Resolves a named placeholder to its one-based PDO slot.
    pub fn parameter_index(&self, name: &str) -> i64 {
        self.named_map
            .get(name.trim_start_matches(':'))
            .copied()
            .unwrap_or(-1)
    }

    /// Stores one parameter value in a one-based slot.
    fn set_bind(&mut self, index: i64, value: SqlType) -> bool {
        let Some(slot) = usize::try_from(index).ok().and_then(|index| index.checked_sub(1)) else {
            return false;
        };
        if slot >= self.binds.len() {
            return false;
        }
        self.binds[slot] = value;
        self.bound[slot] = true;
        true
    }

    /// Stores an integer bind.
    pub fn bind_int(&mut self, index: i64, value: i64) -> bool {
        self.set_bind(index, SqlType::Integer(value))
    }

    /// Stores a floating-point bind.
    pub fn bind_double(&mut self, index: i64, value: f64) -> bool {
        self.set_bind(index, SqlType::Floating(value))
    }

    /// Stores a string bind.
    pub fn bind_text(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.set_bind(index, SqlType::Text(String::from_utf8_lossy(&value).into_owned()))
    }

    /// Stores a BLOB bind.
    pub fn bind_blob(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.set_bind(index, SqlType::Binary(value))
    }

    /// Stores SQL NULL.
    pub fn bind_null(&mut self, index: i64) -> bool {
        self.set_bind(index, SqlType::Null)
    }

    /// Clears execution state while retaining binds.
    pub fn reset(&mut self) {
        self.result = FirebirdResult::empty(0);
        self.cursor = -1;
        self.executed = false;
        self.sent_sql.clear();
        self.error = ErrorState::default();
    }

    /// Clears execution state and every bound parameter.
    pub fn clear_bindings(&mut self) {
        self.reset();
        self.binds.fill(SqlType::Null);
        self.bound.fill(false);
    }

    /// Executes with parameters expanded into native occurrence order.
    pub fn execute(&mut self, connection: &mut FirebirdConn) -> Result<(), String> {
        if self.bound.iter().any(|bound| !bound) {
            self.error = ErrorState {
                sqlstate: "HY093".to_string(),
                native_code: 0,
                message: "Invalid parameter number: number of bound variables does not match number of tokens".to_string(),
            };
            return Err(self.error.message.clone());
        }
        let params = self
            .order
            .iter()
            .filter_map(|slot| usize::try_from(*slot).ok()?.checked_sub(1))
            .map(|slot| self.binds[slot].clone())
            .collect();
        self.sent_sql = self.sql.clone();
        match connection.execute(&self.sql, params) {
            Ok(result) => {
                self.result = result;
                self.cursor = -1;
                self.executed = true;
                self.error = ErrorState::default();
                Ok(())
            }
            Err(message) => {
                self.error = connection.error.clone();
                Err(message)
            }
        }
    }

    /// Reports whether the statement still needs its first execution.
    pub fn needs_execute(&self) -> bool {
        !self.executed
    }

    /// Advances to the next buffered row.
    pub fn step(&mut self) -> i64 {
        let next = self.cursor + 1;
        if next < self.result.rows.len() as isize {
            self.cursor = next;
            1
        } else {
            0
        }
    }

    /// Returns the current result row.
    fn row(&self) -> Option<&[FirebirdCell]> {
        usize::try_from(self.cursor)
            .ok()
            .and_then(|index| self.result.rows.get(index))
            .map(Vec::as_slice)
    }

    /// Returns one current cell.
    fn cell(&self, index: usize) -> Option<&FirebirdCell> {
        self.row()?.get(index)
    }

    /// Returns the common bridge type tag for one current cell.
    pub fn column_type(&self, index: i64) -> i64 {
        let Ok(index) = usize::try_from(index) else {
            return 5;
        };
        match self.cell(index) {
            Some(FirebirdCell::Int(_)) => 1,
            Some(FirebirdCell::Float(_)) => 2,
            Some(FirebirdCell::Bytes(_, true)) => 4,
            Some(FirebirdCell::Bytes(_, false)) => 3,
            Some(FirebirdCell::Null) | None => 5,
        }
    }

    /// Returns one current cell as an integer.
    pub fn column_int(&self, index: i64) -> i64 {
        let Ok(index) = usize::try_from(index) else {
            return 0;
        };
        match self.cell(index) {
            Some(FirebirdCell::Int(value)) => *value,
            Some(FirebirdCell::Float(value)) => *value as i64,
            Some(FirebirdCell::Bytes(value, _)) => String::from_utf8_lossy(value).parse().unwrap_or(0),
            Some(FirebirdCell::Null) | None => 0,
        }
    }

    /// Returns one current cell as a double.
    pub fn column_double(&self, index: i64) -> f64 {
        let Ok(index) = usize::try_from(index) else {
            return 0.0;
        };
        match self.cell(index) {
            Some(FirebirdCell::Int(value)) => *value as f64,
            Some(FirebirdCell::Float(value)) => *value,
            Some(FirebirdCell::Bytes(value, _)) => String::from_utf8_lossy(value).parse().unwrap_or(0.0),
            Some(FirebirdCell::Null) | None => 0.0,
        }
    }

    /// Returns one current cell as bytes.
    pub fn column_data(&self, index: i64) -> Vec<u8> {
        let Ok(index) = usize::try_from(index) else {
            return Vec::new();
        };
        match self.cell(index) {
            Some(FirebirdCell::Int(value)) => value.to_string().into_bytes(),
            Some(FirebirdCell::Float(value)) => value.to_string().into_bytes(),
            Some(FirebirdCell::Bytes(value, _)) => value.clone(),
            Some(FirebirdCell::Null) | None => Vec::new(),
        }
    }

    /// Returns the result column count.
    pub fn column_count(&self) -> i64 {
        self.result.columns.len() as i64
    }

    /// Returns one result column name.
    pub fn column_name(&self, index: i64) -> String {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.result.columns.get(index))
            .map(|column| column.name.clone())
            .unwrap_or_default()
    }

    /// Returns one result column's Firebird wire type ID.
    pub fn column_raw_type(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.result.columns.get(index))
            .map_or(0, |column| column.raw_type)
    }

    /// Returns php-src's PDO_FIREBIRD native type name.
    pub fn column_native_type(&self, index: i64) -> String {
        firebird_type_name(self.column_raw_type(index)).to_string()
    }

    /// Returns php-src PDO_FIREBIRD's sole `getColumnMeta()` field for one column.
    pub fn column_pdo_type(&self, index: i64) -> i64 {
        let Some(index) = usize::try_from(index).ok() else {
            return 2;
        };
        let Some(column) = self.result.columns.get(index) else {
            return 2;
        };
        if column.raw_type & !1 == 32764 {
            return 5;
        }
        match self.result.rows.first().and_then(|row| row.get(index)) {
            Some(FirebirdCell::Int(_)) if matches!(column.raw_type & !1, 496 | 500 | 580) => 1,
            _ => 2,
        }
    }

    /// Stores the PDO-visible Firebird cursor name after validating its native limit.
    pub fn set_cursor_name(&mut self, name: String) -> bool {
        if name.len() > 31 {
            self.error = ErrorState {
                sqlstate: "HY000".to_string(),
                native_code: 0,
                message: "Cursor name must not be longer than 31 bytes".to_string(),
            };
            return false;
        }
        self.cursor_name = Some(name);
        true
    }

    /// Returns the configured Firebird cursor name, or `None` before one is set.
    pub fn cursor_name(&self) -> Option<&str> {
        self.cursor_name.as_deref()
    }

    /// Returns the statement SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        &self.error.sqlstate
    }

    /// Returns the statement native SQLCODE.
    pub fn errcode(&self) -> i64 {
        self.error.native_code
    }

    /// Returns the statement diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }
}

/// Maps Firebird's XSQLDA type IDs to stable PDO metadata spellings.
pub fn firebird_type_name(raw_type: i64) -> &'static str {
    match raw_type & !1 {
        448 => "VARCHAR",
        452 => "CHAR",
        480 | 482 | 530 => "DOUBLE",
        496 => "INTEGER",
        500 => "SMALLINT",
        510 => "TIMESTAMP",
        520 => "BLOB",
        540 => "ARRAY",
        550 => "QUAD",
        560 => "TIME",
        570 => "DATE",
        580 => "BIGINT",
        32764 => "BOOLEAN",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses the php-src Firebird DSN keys and constructor options.
    #[test]
    fn parses_firebird_dsn() {
        let options = parse_dsn("firebird:host=db;port=3051;dbname=/data/app.fdb;charset=UTF8;role=ADMIN;dialect=3;user=user%3Bname;password=p%25w;transaction_isolation=1004;writable_transaction=0").unwrap();
        assert_eq!(options.host, "db");
        assert_eq!(options.port, 3051);
        assert_eq!(options.dbname, "/data/app.fdb");
        assert_eq!(options.user, "user;name");
        assert_eq!(options.password, "p%w");
        assert_eq!(options.isolation, READ_COMMITTED);
        assert!(!options.writable);
    }

    /// Accepts the legacy remote dbname syntax documented by PDO_FIREBIRD.
    #[test]
    fn parses_php_remote_dbname() {
        let options = parse_dsn(
            "firebird:dbname=db.example/3051:/data/app.fdb;charset=utf-8;user=test;password=secret",
        )
        .unwrap();
        assert_eq!(options.host, "db.example");
        assert_eq!(options.port, 3051);
        assert_eq!(options.dbname, "/data/app.fdb");
    }

    /// Accepts Firebird 3+'s URL-style IPv6 connection strings.
    #[test]
    fn parses_url_style_ipv6_dbname() {
        let options = parse_dsn("firebird:dbname=inet6://[::1]:3052/app.fdb").unwrap();
        assert_eq!(options.host, "::1");
        assert_eq!(options.port, 3052);
        assert_eq!(options.dbname, "app.fdb");
    }

    /// Rejects a transaction isolation constant outside php-src's supported set.
    #[test]
    fn rejects_invalid_isolation() {
        assert!(parse_dsn("firebird:dbname=test.fdb;transaction_isolation=9999").is_err());
    }

    /// Maps common Firebird errors into PDO SQLSTATE classes.
    #[test]
    fn maps_firebird_sqlstates() {
        assert_eq!(sqlstate_for_code(-803), "23000");
        assert_eq!(sqlstate_for_code(-204), "42S02");
        assert_eq!(sqlstate_for_code(-104), "42000");
        assert_eq!(sqlstate_for_code(-902), "08006");
    }

    /// Mirrors Firebird XSQLDA type names used by column metadata.
    #[test]
    fn maps_firebird_type_names() {
        assert_eq!(firebird_type_name(497), "INTEGER");
        assert_eq!(firebird_type_name(521), "BLOB");
        assert_eq!(firebird_type_name(32765), "BOOLEAN");
    }

    /// Ignores comments and literals when classifying statement keywords.
    #[test]
    fn scans_executable_sql_keywords() {
        assert_eq!(leading_keyword("/* hint */ -- line\n SELECT 1"), "SELECT");
        assert!(has_sql_keyword("INSERT INTO T VALUES (1)\nRETURNING ID", "RETURNING"));
        assert!(!has_sql_keyword("INSERT INTO T VALUES (' RETURNING ')", "RETURNING"));
    }

    /// Runs a direct Firebird round-trip when an explicit live DSN is provided.
    #[test]
    #[ignore]
    fn live_round_trip() {
        let dsn = std::env::var("ELEPHC_FIREBIRD_DSN")
            .expect("ELEPHC_FIREBIRD_DSN is required for the ignored live test");
        let mut connection = FirebirdConn::open(&dsn)
            .unwrap_or_else(|error| panic!("PDO_FIREBIRD connection failed: {error}"));
        let result = connection
            .execute("SELECT CAST(7 AS INTEGER) AS N, CAST('Ada' AS VARCHAR(10)) AS NAME FROM RDB$DATABASE", Vec::new())
            .unwrap_or_else(|error| panic!("PDO_FIREBIRD query failed: {error}"));
        assert_eq!(result.columns.len(), 2);
        assert!(matches!(result.rows[0][0], FirebirdCell::Int(7)));
        assert!(matches!(&result.rows[0][1], FirebirdCell::Bytes(value, false) if value == b"Ada"));
    }
}
