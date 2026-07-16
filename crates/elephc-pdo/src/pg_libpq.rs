//! Purpose:
//! PostgreSQL PDO backend using the same libpq client library as php-src. This
//! build-time-selected backend supplies GSSAPI and libpq-only connection options.
//!
//! Called from:
//! - `crate::lib` when `elephc-pdo` is built with `libpq-gss`.
//!
//! Key details:
//! - Explicit PDO DSN pairs are handed to `PQconnectdb`; libpq itself resolves
//!   services, passfiles, environment defaults, GSS/Kerberos authentication,
//!   encrypted keys, authentication policy, and replication startup parameters.
//! - Unbuffered execution uses `PQsend*` plus `PQsetSingleRowMode`, matching PHP
//!   8.5+ rather than emulating GSS on a second pure-Rust connection.

use std::collections::{HashMap, VecDeque};
use std::ffi::{c_char, c_void, CStr};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use libpq::result::ErrorField;
use libpq::{Connection, Format, PQResult, Status};

pub use crate::pg_native::Bind;
#[cfg(test)]
pub use crate::pg_native::parse_dsn;
#[cfg(test)]
pub use crate::pg_native::translate_placeholders;
#[cfg(test)]
pub(crate) use crate::pg_native::transaction_state_after_sql;
use crate::pg_native::{
    explicit_dsn_options, interpolate_emulated_sql, translate_placeholders_with_markers,
};

/// One decoded result value in the common bridge representation.
pub enum Cell {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

/// Metadata copied from a libpq result descriptor.
#[derive(Clone, Default)]
struct ColumnMeta {
    name: String,
    native_type: String,
    type_oid: i64,
    table_oid: i64,
    table_name: String,
    len: i64,
    precision: i64,
}

/// A live libpq connection with PDO-visible error and transaction bookkeeping.
pub struct PgConn {
    client: Connection,
    pub changes: i64,
    pub errmsg: String,
    pub errcode: i64,
    pub sqlstate: String,
    pub prefetch: bool,
    pub in_transaction: bool,
    generation: u64,
    statement_counter: u64,
    notices: Box<Mutex<VecDeque<String>>>,
}

/// A libpq-backed PDO statement, buffered or in single-row mode.
pub struct PgStmt {
    pub conn_id: i64,
    pub query_string: String,
    translated_sql: String,
    emulated: bool,
    markers: Vec<(usize, usize, usize)>,
    statement_name: Option<String>,
    pub sent_sql: String,
    pub named_map: HashMap<String, i64>,
    pub binds: Vec<Bind>,
    bound: Vec<bool>,
    columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
    pub cursor: isize,
    pub executed: bool,
    pub buffered: bool,
    simple_streaming: bool,
    streaming: bool,
    generation: u64,
}

/// Returns true for libpq result statuses representing successful commands.
fn result_ok(result: &PQResult) -> bool {
    matches!(
        result.status(),
        Status::CommandOk | Status::TuplesOk | Status::SingleTuple | Status::EmptyQuery
    )
}

/// Extracts SQLSTATE from a libpq result, falling back for client-side failures.
fn result_sqlstate(result: &PQResult) -> String {
    result
        .error_field(ErrorField::Sqlstate)
        .ok()
        .flatten()
        .unwrap_or("HY000")
        .to_string()
}

/// Converts a libpq result error into PDO connection error state.
fn record_result_error(conn: &mut PgConn, result: &PQResult) -> i64 {
    conn.sqlstate = result_sqlstate(result);
    conn.errmsg = result
        .error_message()
        .ok()
        .flatten()
        .unwrap_or_else(|| "PostgreSQL libpq operation failed".to_string());
    conn.errcode = result.status() as i64;
    -1
}

/// Formats a libpq integer server version as PostgreSQL's dotted version text.
fn format_server_version(version: i32) -> String {
    let major = version / 10_000;
    let minor = (version / 100) % 100;
    let patch = version % 100;
    if major >= 10 {
        format!("{major}.{patch}")
    } else {
        format!("{major}.{minor}.{patch}")
    }
}

/// Maps well-known PostgreSQL OIDs to php-src native type names.
fn native_type_name(oid: i64) -> String {
    match oid {
        16 => "bool",
        17 => "bytea",
        20 => "int8",
        21 => "int2",
        23 => "int4",
        25 => "text",
        26 => "oid",
        700 => "float4",
        701 => "float8",
        1042 => "bpchar",
        1043 => "varchar",
        1082 => "date",
        1083 => "time",
        1114 => "timestamp",
        1184 => "timestamptz",
        1700 => "numeric",
        2950 => "uuid",
        114 => "json",
        3802 => "jsonb",
        _ => "",
    }
    .to_string()
}

/// Decodes PostgreSQL's text-format bytea representation.
fn decode_bytea(value: &[u8]) -> Vec<u8> {
    let Some(hex) = value.strip_prefix(b"\\x") else {
        return value.to_vec();
    };
    hex.chunks_exact(2)
        .filter_map(|pair| std::str::from_utf8(pair).ok())
        .filter_map(|pair| u8::from_str_radix(pair, 16).ok())
        .collect()
}

/// Decodes one libpq result row according to each field OID.
fn decode_result_row(result: &PQResult, row: usize) -> Vec<Cell> {
    (0..result.nfields())
        .map(|column| {
            let Some(value) = result.value(row, column) else {
                return Cell::Null;
            };
            match result.field_type(column) as i64 {
                16 => Cell::Int(matches!(value, b"t" | b"true" | b"1") as i64),
                20 | 21 | 23 | 26 => std::str::from_utf8(value)
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .map(Cell::Int)
                    .unwrap_or_else(|| Cell::Text(String::from_utf8_lossy(value).into_owned())),
                700 | 701 | 1700 => std::str::from_utf8(value)
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .map(Cell::Float)
                    .unwrap_or_else(|| Cell::Text(String::from_utf8_lossy(value).into_owned())),
                17 => Cell::Bytes(decode_bytea(value)),
                _ => Cell::Text(String::from_utf8_lossy(value).into_owned()),
            }
        })
        .collect()
}

/// Copies result column descriptors and resolves source table names through libpq.
fn result_columns(
    client: &Connection,
    result: &PQResult,
    resolve_table_names: bool,
) -> Vec<ColumnMeta> {
    (0..result.nfields())
        .map(|column| {
            let table_oid = result.field_table(column).unwrap_or(0) as i64;
            let table_name = if table_oid == 0 || !resolve_table_names {
                String::new()
            } else {
                let lookup = client.exec(&format!(
                    "SELECT relname FROM pg_catalog.pg_class WHERE oid = {table_oid}"
                ));
                lookup
                    .value(0, 0)
                    .map(|value| String::from_utf8_lossy(value).into_owned())
                    .unwrap_or_default()
            };
            let type_oid = result.field_type(column) as i64;
            ColumnMeta {
                name: result.field_name(column).ok().flatten().unwrap_or_default(),
                native_type: native_type_name(type_oid),
                type_oid,
                table_oid,
                table_name,
                len: result.field_size(column).map(|size| size as i64).unwrap_or(-1),
                precision: result.field_mod(column).map(i64::from).unwrap_or(-1),
            }
        })
        .collect()
}

/// Converts one bound value into a libpq parameter buffer and format.
fn bind_bytes(bind: &Bind) -> (Option<Vec<u8>>, Format) {
    match bind {
        Bind::Null => (None, Format::Text),
        Bind::Int(value) => (
            Some(nul_terminated_text(value.to_string().into_bytes())),
            Format::Text,
        ),
        Bind::Float(value) => (
            Some(nul_terminated_text(value.to_string().into_bytes())),
            Format::Text,
        ),
        Bind::Text(value) => (
            Some(nul_terminated_text(value.as_bytes().to_vec())),
            Format::Text,
        ),
        Bind::Bytes(value) => (Some(value.clone()), Format::Binary),
    }
}

/// Appends the C terminator required by libpq for text-format parameter values.
fn nul_terminated_text(mut value: Vec<u8>) -> Vec<u8> {
    value.push(0);
    value
}

/// Renders explicit PDO DSN pairs as safely quoted libpq conninfo while leaving
/// service, passfile, and environment resolution to `PQconnectdb`.
fn libpq_conninfo(dsn: &str) -> Result<String, String> {
    let mut options = explicit_dsn_options(dsn)?;
    // php-src always supplies a bounded default when neither the DSN nor
    // PDO::ATTR_TIMEOUT selected one. Keep both PostgreSQL backends identical.
    options
        .entry("connect_timeout".to_string())
        .or_insert_with(|| "30".to_string());
    Ok(options
        .iter()
        .map(|(key, value)| {
            format!(
                "{}='{}'",
                key,
                value.replace('\\', "\\\\").replace('\'', "\\'")
            )
        })
        .collect::<Vec<_>>()
        .join(" "))
}

/// Buffers a libpq notice without re-entering compiled PHP from libpq's callback.
unsafe extern "C" fn notice_processor(argument: *mut c_void, message: *const c_char) {
    if argument.is_null() || message.is_null() {
        return;
    }
    let queue = unsafe { &*(argument as *const Mutex<VecDeque<String>>) };
    let message = unsafe { CStr::from_ptr(message) }
        .to_string_lossy()
        .trim()
        .to_string();
    if let Ok(mut queue) = queue.lock() {
        queue.push_back(message);
    }
}

impl PgConn {
    /// Opens the DSN through `PQconnectdb`, leaving every libpq-specific keyword intact.
    pub fn open(dsn: &str) -> Result<Self, String> {
        let conninfo = libpq_conninfo(dsn)?;
        let client = Connection::new(&conninfo).map_err(|error| error.to_string())?;
        let notices = Box::new(Mutex::new(VecDeque::new()));
        let notice_pointer = (&*notices) as *const Mutex<VecDeque<String>> as *mut c_void;
        unsafe {
            client.set_notice_processor(Some(notice_processor), notice_pointer);
        }
        Ok(Self {
            client,
            changes: 0,
            errmsg: String::new(),
            errcode: 0,
            sqlstate: "00000".to_string(),
            prefetch: true,
            in_transaction: false,
            generation: 0,
            statement_counter: 0,
            notices,
        })
    }

    /// Sets the default prefetch mode for future statements.
    pub fn set_prefetch(&mut self, prefetch: bool) -> i64 {
        self.prefetch = prefetch;
        1
    }

    /// Starts a new query generation after draining any prior async results.
    fn begin_query(&mut self) -> u64 {
        while self.client.result().is_some() {}
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }

    /// Records successful execution and transaction state.
    fn succeed(&mut self, sql: &str, changes: i64) {
        self.changes = changes;
        self.errcode = 0;
        self.errmsg.clear();
        self.sqlstate = "00000".to_string();
        let upper = sql.trim_start().to_ascii_uppercase();
        if upper.starts_with("BEGIN") || upper.starts_with("START TRANSACTION") {
            self.in_transaction = true;
        } else if upper.starts_with("COMMIT")
            || (upper.starts_with("ROLLBACK") && !upper.starts_with("ROLLBACK TO"))
        {
            self.in_transaction = false;
        }
    }

    /// Drains one notice buffered by libpq's notice processor.
    pub fn drain_notice(&self) -> String {
        self.notices
            .lock()
            .ok()
            .and_then(|mut notices| notices.pop_front())
            .unwrap_or_default()
    }

    /// Resets a persistent PostgreSQL session for PHP 8.6 semantics.
    pub fn discard_all(&mut self) {
        let _ = self.exec_simple("DISCARD ALL");
    }

    /// Executes SQL and returns affected rows or `-1`.
    pub fn exec(&mut self, sql: &str) -> i64 {
        self.exec_simple(sql)
    }

    /// Executes one simple-protocol command through libpq.
    pub fn exec_simple(&mut self, sql: &str) -> i64 {
        self.begin_query();
        let result = self.client.exec(sql);
        if !result_ok(&result) {
            return record_result_error(self, &result);
        }
        let changes = result.cmd_tuples().unwrap_or(0) as i64;
        self.succeed(sql, changes);
        changes
    }

    /// Returns the current or named PostgreSQL sequence value as an integer.
    pub fn last_insert_id(&mut self, name: Option<&str>) -> i64 {
        self.last_insert_id_text(name).parse().unwrap_or(0)
    }

    /// Returns the current or named PostgreSQL sequence value without truncation.
    pub fn last_insert_id_text(&mut self, name: Option<&str>) -> String {
        let query = name
            .map(|name| format!("SELECT currval('{}')", name.replace('\'', "''")))
            .unwrap_or_else(|| "SELECT lastval()".to_string());
        let result = self.client.exec(&query);
        result
            .value(0, 0)
            .map(|value| String::from_utf8_lossy(value).into_owned())
            .unwrap_or_default()
    }

    /// Returns the connected PostgreSQL server version.
    pub fn server_version(&mut self) -> String {
        format_server_version(self.client.server_version())
    }

    /// Returns the linked libpq client version.
    pub fn client_version(&self) -> String {
        format_server_version(libpq::version())
    }

    /// Returns a libpq-style connection status string.
    pub fn connection_status(&self) -> String {
        format!("{} via libpq", self.client.host().unwrap_or_default())
    }

    /// Reports whether libpq considers the connection unusable.
    pub fn is_closed(&self) -> bool {
        self.client.status() != libpq::connection::Status::Ok
    }

    /// Returns PDO's PostgreSQL server-information summary.
    pub fn server_info(&mut self) -> String {
        format!(
            "PID: {}; Client Encoding: {}",
            self.backend_pid(),
            self.client.parameter_status("client_encoding").unwrap_or_default()
        )
    }

    /// Returns the backend process identifier.
    pub fn backend_pid(&mut self) -> i64 {
        self.client.backend_pid() as i64
    }

    /// Creates a PostgreSQL large object and returns its OID.
    pub fn lob_create(&mut self) -> String {
        self.scalar_text("SELECT lo_create(0)")
    }

    /// Deletes a PostgreSQL large object.
    pub fn lob_unlink(&mut self, oid: &str) -> i64 {
        self.scalar_text(&format!("SELECT lo_unlink({})", oid.parse::<u32>().unwrap_or(0)))
            .parse()
            .unwrap_or(0)
    }

    /// Reads a complete PostgreSQL large object for the legacy ABI.
    pub fn lob_get(&mut self, oid: &str) -> Option<Vec<u8>> {
        let result = self.client.exec(&format!(
            "SELECT encode(lo_get({}), 'hex')",
            oid.parse::<u32>().ok()?
        ));
        result.value(0, 0).map(decode_hex)
    }

    /// Replaces a PostgreSQL large object from offset zero.
    pub fn lob_put(&mut self, oid: &str, data: &[u8]) -> i64 {
        let hex = hex_bytes(data);
        self.exec_simple(&format!(
            "SELECT lo_put({}, 0, decode('{hex}', 'hex'))",
            oid.parse::<u32>().unwrap_or(0)
        ));
        (self.errcode == 0) as i64
    }

    /// Returns a PostgreSQL large object's byte size.
    pub fn lob_size(&mut self, oid: &str) -> Option<i64> {
        self.scalar_text(&format!(
            "SELECT octet_length(lo_get({}))",
            oid.parse::<u32>().ok()?
        ))
        .parse()
        .ok()
    }

    /// Reads one bounded PostgreSQL large-object slice.
    pub fn lob_read_at(&mut self, oid: &str, offset: i64, length: i64) -> Option<Vec<u8>> {
        if offset < 0 || length < 0 {
            return None;
        }
        let result = self.client.exec(&format!(
            "SELECT encode(lo_get({}, {offset}, {length}), 'hex')",
            oid.parse::<u32>().ok()?
        ));
        result.value(0, 0).map(decode_hex)
    }

    /// Writes one bounded PostgreSQL large-object slice.
    pub fn lob_write_at(&mut self, oid: &str, offset: i64, data: &[u8]) -> i64 {
        if offset < 0 {
            return -1;
        }
        let hex = hex_bytes(data);
        let result = self.client.exec(&format!(
            "SELECT lo_put({}, {offset}, decode('{hex}', 'hex'))",
            oid.parse::<u32>().unwrap_or(0)
        ));
        if result_ok(&result) {
            data.len() as i64
        } else {
            record_result_error(self, &result)
        }
    }

    /// Streams bytes into COPY FROM STDIN through libpq.
    pub fn copy_in(&mut self, copy_sql: &str, data: &[u8]) -> i64 {
        let result = self.client.exec(copy_sql);
        if result.status() != Status::CopyIn {
            return record_result_error(self, &result);
        }
        if self.client.put_copy_data(data).is_err() || self.client.put_copy_end(None).is_err() {
            return -1;
        }
        let final_result = self.client.result();
        final_result
            .as_ref()
            .and_then(|result| result.cmd_tuples().ok())
            .unwrap_or(0) as i64
    }

    /// Collects COPY TO STDOUT bytes through libpq.
    pub fn copy_out(&mut self, copy_sql: &str) -> String {
        let result = self.client.exec(copy_sql);
        if result.status() != Status::CopyOut {
            record_result_error(self, &result);
            return String::new();
        }
        let mut output = Vec::new();
        while let Ok(chunk) = self.client.copy_data(false) {
            output.extend_from_slice(&chunk);
        }
        while self.client.result().is_some() {}
        String::from_utf8_lossy(&output).into_owned()
    }

    /// Polls libpq for one LISTEN/NOTIFY message until the requested deadline.
    pub fn get_notify(&mut self, timeout_ms: i64) -> String {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(0) as u64);
        loop {
            let _ = self.client.consume_input();
            if let Some(notification) = self.client.notifies() {
                return format!(
                    "{}\t{}\t{}",
                    notification.relname().unwrap_or_default(),
                    notification.be_pid(),
                    notification.extra().unwrap_or_default()
                );
            }
            if Instant::now() >= deadline {
                return String::new();
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Prepares a PDO statement and copies libpq's result descriptor.
    pub fn prepare(&mut self, sql: &str, emulated: bool) -> Result<PgStmt, String> {
        self.begin_query();
        let (translated, named_map, mixed, markers) = translate_placeholders_with_markers(sql);
        if mixed {
            self.sqlstate = "HY093".to_string();
            return Err("Invalid parameter number: mixed named and positional parameters".to_string());
        }
        let param_count = markers.iter().map(|marker| marker.2).max().unwrap_or(0);
        let mut statement_name = None;
        let mut columns = Vec::new();
        if !emulated {
            self.statement_counter = self.statement_counter.wrapping_add(1);
            let name = format!("elephc_pdo_{}", self.statement_counter);
            let prepared = self.client.prepare(Some(&name), &translated, &[]);
            if !result_ok(&prepared) {
                record_result_error(self, &prepared);
                return Err(self.errmsg.clone());
            }
            let description = self.client.describe_prepared(Some(&name));
            columns = result_columns(&self.client, &description, true);
            statement_name = Some(name);
        }
        self.errcode = 0;
        self.errmsg.clear();
        self.sqlstate = "00000".to_string();
        Ok(PgStmt {
            conn_id: 0,
            query_string: sql.to_string(),
            translated_sql: translated,
            emulated,
            markers,
            statement_name,
            sent_sql: String::new(),
            named_map,
            binds: vec![Bind::Null; param_count],
            bound: vec![false; param_count],
            columns,
            rows: Vec::new(),
            cursor: -1,
            executed: false,
            buffered: self.prefetch,
            simple_streaming: false,
            streaming: false,
            generation: 0,
        })
    }

    /// Executes a scalar query and returns its first textual field.
    fn scalar_text(&mut self, sql: &str) -> String {
        let result = self.client.exec(sql);
        if !result_ok(&result) {
            record_result_error(self, &result);
            return String::new();
        }
        result
            .value(0, 0)
            .map(|value| String::from_utf8_lossy(value).into_owned())
            .unwrap_or_default()
    }
}

impl PgStmt {
    /// Enables PHP 8.5+'s lazy simple-query behavior.
    pub fn enable_simple_streaming(&mut self) -> i64 {
        self.simple_streaming = true;
        1
    }

    /// Overrides statement buffering before execution.
    pub fn set_prefetch(&mut self, prefetch: bool) -> i64 {
        if self.executed {
            return 0;
        }
        self.buffered = prefetch;
        1
    }

    /// Resolves a named parameter to its one-based position.
    pub fn bind_parameter_index(&self, name: &str) -> i64 {
        self.named_map
            .get(name.strip_prefix(':').unwrap_or(name))
            .copied()
            .unwrap_or(0)
    }

    /// Stores one bound parameter.
    pub fn bind(&mut self, index: i64, value: Bind) -> i64 {
        if index < 1 || index as usize > self.binds.len() {
            return 0;
        }
        self.binds[index as usize - 1] = value;
        self.bound[index as usize - 1] = true;
        1
    }

    /// Resets cursor/result state while preserving bindings.
    pub fn reset(&mut self, conn: &mut PgConn) -> i64 {
        if self.streaming {
            while conn.client.result().is_some() {}
        }
        self.rows.clear();
        self.cursor = -1;
        self.executed = false;
        self.streaming = false;
        1
    }

    /// Clears every binding back to NULL/unbound.
    pub fn clear_bindings(&mut self) -> i64 {
        for (bind, bound) in self.binds.iter_mut().zip(&mut self.bound) {
            *bind = Bind::Null;
            *bound = false;
        }
        1
    }

    /// Executes the statement in buffered or libpq single-row mode.
    fn execute(&mut self, conn: &mut PgConn) -> Result<(), i64> {
        if self.bound.iter().any(|bound| !bound) {
            conn.sqlstate = "HY093".to_string();
            conn.errmsg = "Invalid parameter number".to_string();
            return Err(-1);
        }
        self.generation = conn.begin_query();
        let lazy = !self.buffered && (!self.emulated || self.simple_streaming);
        if self.emulated {
            let sql = interpolate_emulated_sql(&self.translated_sql, &self.markers, &self.binds)
                .map_err(|message| {
                    conn.errmsg = message;
                    conn.sqlstate = "HY093".to_string();
                    -1
                })?;
            self.sent_sql = sql.clone();
            if lazy {
                conn.client.send_query(&sql).map_err(|_| -1)?;
                conn.client.set_single_row_mode().map_err(|_| -1)?;
                conn.changes = 0;
                conn.errcode = 0;
                conn.errmsg.clear();
                conn.sqlstate = "00000".to_string();
                self.streaming = true;
                self.executed = true;
                return Ok(());
            }
            let result = conn.client.exec(&sql);
            return self.consume_buffered(conn, result);
        }
        let owned: Vec<(Option<Vec<u8>>, Format)> = self.binds.iter().map(bind_bytes).collect();
        let values: Vec<Option<&[u8]>> = owned
            .iter()
            .map(|(value, _)| value.as_deref())
            .collect();
        let formats: Vec<Format> = owned.iter().map(|(_, format)| *format).collect();
        let name = self.statement_name.as_deref();
        if lazy {
            conn.client
                .send_query_prepared(name, &values, &formats, Format::Text)
                .map_err(|_| -1)?;
            conn.client.set_single_row_mode().map_err(|_| -1)?;
            conn.changes = 0;
            conn.errcode = 0;
            conn.errmsg.clear();
            conn.sqlstate = "00000".to_string();
            self.streaming = true;
            self.executed = true;
            return Ok(());
        }
        let result = conn
            .client
            .exec_prepared(name, &values, &formats, Format::Text);
        self.consume_buffered(conn, result)
    }

    /// Copies a completed libpq result into statement-owned rows and metadata.
    fn consume_buffered(&mut self, conn: &mut PgConn, result: PQResult) -> Result<(), i64> {
        if !result_ok(&result) {
            return Err(record_result_error(conn, &result));
        }
        self.columns = result_columns(&conn.client, &result, true);
        self.rows = (0..result.ntuples())
            .map(|row| decode_result_row(&result, row))
            .collect();
        let changes = if self.rows.is_empty() {
            result.cmd_tuples().unwrap_or(0) as i64
        } else {
            self.rows.len() as i64
        };
        conn.succeed(&self.query_string, changes);
        self.executed = true;
        Ok(())
    }

    /// Advances a buffered or single-row-mode cursor.
    pub fn step(&mut self, conn: &mut PgConn) -> i64 {
        if self.executed && self.generation != conn.generation {
            return 0;
        }
        if !self.executed && self.execute(conn).is_err() {
            return -1;
        }
        if self.streaming {
            loop {
                let Some(result) = conn.client.result() else {
                    self.streaming = false;
                    conn.succeed(&self.query_string, conn.changes);
                    return 0;
                };
                if !result_ok(&result) {
                    self.streaming = false;
                    return record_result_error(conn, &result);
                }
                if result.status() == Status::SingleTuple && result.ntuples() == 1 {
                    if self.emulated || self.columns.is_empty() {
                        self.columns = result_columns(&conn.client, &result, false);
                    }
                    self.rows.clear();
                    self.rows.push(decode_result_row(&result, 0));
                    self.cursor = 0;
                    return 1;
                }
                if result.status() == Status::CommandOk {
                    conn.changes = result.cmd_tuples().unwrap_or(0) as i64;
                }
            }
        }
        self.cursor += 1;
        ((self.cursor as usize) < self.rows.len()) as i64
    }

    /// Applies PDO cursor orientation to buffered results; streaming remains forward-only.
    pub fn step_oriented(&mut self, conn: &mut PgConn, orientation: i64, offset: i64) -> i64 {
        if self.streaming || !self.executed {
            return self.step(conn);
        }
        let target = match orientation {
            0 => self.cursor + 1,
            1 => self.cursor - 1,
            2 => 0,
            3 => self.rows.len() as isize - 1,
            4 if offset > 0 => offset as isize - 1,
            4 if offset < 0 => self.rows.len() as isize + offset as isize,
            5 => self.cursor + offset as isize,
            _ => return 0,
        };
        if target < 0 || target as usize >= self.rows.len() {
            return 0;
        }
        self.cursor = target;
        1
    }

    /// Returns the current cell at a zero-based column index.
    fn cell(&self, index: i64) -> Option<&Cell> {
        self.rows
            .get(self.cursor.max(0) as usize)
            .and_then(|row| row.get(index as usize))
    }

    /// Returns the result column count.
    pub fn column_count(&self) -> i64 {
        if self.emulated && !self.executed && self.columns.is_empty() {
            1
        } else {
            self.columns.len() as i64
        }
    }

    /// Returns a result column name.
    pub fn column_name(&self, index: i64) -> String {
        self.columns.get(index as usize).map(|column| column.name.clone()).unwrap_or_default()
    }

    /// Returns a source table name.
    pub fn column_table_name(&self, index: i64) -> String {
        self.columns.get(index as usize).map(|column| column.table_name.clone()).unwrap_or_default()
    }

    /// Returns the bridge storage type code for the current value.
    pub fn column_type(&self, index: i64) -> i64 {
        match self.cell(index) {
            Some(Cell::Int(_)) => 1,
            Some(Cell::Float(_)) => 2,
            Some(Cell::Text(_)) => 3,
            Some(Cell::Bytes(_)) => 4,
            _ => 5,
        }
    }

    /// Estimates currently statement-owned result memory.
    pub fn result_memory_size(&self) -> Option<i64> {
        self.executed.then(|| {
            self.rows
                .iter()
                .flatten()
                .map(|cell| match cell {
                    Cell::Text(value) => value.capacity(),
                    Cell::Bytes(value) => value.capacity(),
                    _ => std::mem::size_of::<Cell>(),
                })
                .sum::<usize>() as i64
        })
    }

    /// Returns PostgreSQL's native type name.
    pub fn column_native_type(&self, index: i64) -> String {
        self.columns.get(index as usize).map(|column| column.native_type.clone()).unwrap_or_default()
    }

    /// Returns PostgreSQL's type OID.
    pub fn column_type_oid(&self, index: i64) -> i64 {
        self.columns.get(index as usize).map(|column| column.type_oid).unwrap_or(0)
    }

    /// Returns PostgreSQL's source table OID.
    pub fn column_table_oid(&self, index: i64) -> i64 {
        self.columns.get(index as usize).map(|column| column.table_oid).unwrap_or(0)
    }

    /// Returns libpq's PQfsize metadata value.
    pub fn column_len(&self, index: i64) -> i64 {
        self.columns.get(index as usize).map(|column| column.len).unwrap_or(-1)
    }

    /// Returns libpq's raw PQfmod metadata value.
    pub fn column_precision(&self, index: i64) -> i64 {
        self.columns.get(index as usize).map(|column| column.precision).unwrap_or(-1)
    }

    /// Returns the current value coerced to integer.
    pub fn column_int(&self, index: i64) -> i64 {
        match self.cell(index) {
            Some(Cell::Int(value)) => *value,
            Some(Cell::Float(value)) => *value as i64,
            Some(Cell::Text(value)) => value.parse().unwrap_or(0),
            _ => 0,
        }
    }

    /// Returns the current value coerced to float.
    pub fn column_double(&self, index: i64) -> f64 {
        match self.cell(index) {
            Some(Cell::Float(value)) => *value,
            Some(Cell::Int(value)) => *value as f64,
            Some(Cell::Text(value)) => value.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    /// Returns the current value as binary-safe bytes.
    pub fn column_data(&self, index: i64) -> Vec<u8> {
        match self.cell(index) {
            Some(Cell::Text(value)) => value.as_bytes().to_vec(),
            Some(Cell::Bytes(value)) => value.clone(),
            Some(Cell::Int(value)) => value.to_string().into_bytes(),
            Some(Cell::Float(value)) => value.to_string().into_bytes(),
            _ => Vec::new(),
        }
    }
}

/// Encodes bytes as lowercase hexadecimal SQL text.
fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Decodes lowercase/uppercase hexadecimal result bytes.
fn decode_hex(bytes: &[u8]) -> Vec<u8> {
    bytes
        .chunks_exact(2)
        .filter_map(|pair| std::str::from_utf8(pair).ok())
        .filter_map(|pair| u8::from_str_radix(pair, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for libpq-only connection-option forwarding.
    //!
    //! Called from:
    //! - `cargo test -p elephc-pdo --features libpq-gss`.
    //!
    //! Key details:
    //! - These tests require no server; live libpq execution is covered separately.

    use super::{bind_bytes, libpq_conninfo, Bind, Format};

    /// GSS, authentication policy, encrypted-key password, and replication reach libpq intact.
    #[test]
    fn libpq_only_options_are_forwarded() {
        let conninfo = libpq_conninfo(
            "pgsql:host=db;user=app;service=kerberos;passfile=/secure/pgpass;gssencmode=require;require_auth=gss;sslpassword=secret;replication=database",
        )
        .expect("libpq options render");
        assert!(conninfo.contains("gssencmode='require'"));
        assert!(conninfo.contains("require_auth='gss'"));
        assert!(conninfo.contains("sslpassword='secret'"));
        assert!(conninfo.contains("replication='database'"));
        assert!(conninfo.contains("service='kerberos'"));
        assert!(conninfo.contains("passfile='/secure/pgpass'"));
        assert!(conninfo.contains("connect_timeout='30'"));
    }

    /// Text-format parameters satisfy libpq's C-string contract while binary
    /// values retain their exact bytes, including any trailing zero.
    #[test]
    fn bind_buffers_use_the_required_libpq_termination() {
        let (text, text_format) = bind_bytes(&Bind::Text("Ada".to_string()));
        assert_eq!(text_format, Format::Text);
        assert_eq!(text.as_deref(), Some(b"Ada\0".as_slice()));

        let (binary, binary_format) = bind_bytes(&Bind::Bytes(vec![b'A', 0, b'B']));
        assert_eq!(binary_format, Format::Binary);
        assert_eq!(binary.as_deref(), Some([b'A', 0, b'B'].as_slice()));
    }
}
