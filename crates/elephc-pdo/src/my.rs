//! Purpose:
//! The MySQL / MariaDB driver for the elephc PDO bridge. Connects with the
//! synchronous, pure-Rust `mysql` client (no system libmysqlclient), so compiled
//! PHP binaries stay standalone and talk to a running MySQL/MariaDB server over
//! the network.
//!
//! Called from:
//! - `crate::lib`'s `elephc_pdo_*` C-ABI functions, after matching the
//!   connection/statement's driver to `Conn::Mysql` / `Stmt::Mysql`.
//!
//! Key details:
//! - MySQL placeholders are positional `?`. PDO `:name` placeholders are rewritten
//!   to `?` at prepare time, with a per-`?` `order` recording which bound slot
//!   feeds it, so a `:name` used several times binds the same value to each `?`.
//!   A scanner skips `--`/`#`/`/* */` comments and `'…'`/`"…"`/`` `…` `` quoted
//!   regions (all with their driver-correct escape rules, string literals
//!   following the connection's live `NO_BACKSLASH_ESCAPES` `sql_mode`) so a
//!   `?`/`:name` inside any of those is never mistaken for a real placeholder,
//!   and the scanner's placeholder count always agrees with the server's own.
//!   PDO forbids mixing `?` and `:name` in one statement; `prepare()` rejects the
//!   mix with `HY093` before ever asking the server to prepare it.
//! - A statement is prepared server-side for column metadata, then executed
//!   lazily on the first `step()`. Buffered statements retain typed `Cell` rows;
//!   unbuffered statements move the client into a demand worker and retain only
//!   the current row, while preserving multiple result-set boundaries.
//! - Bound values cross the wire as their native `mysql::Value` (ints, doubles,
//!   text bytes); the server coerces text to the column type, so — unlike the
//!   PostgreSQL driver — no per-parameter type inference is needed.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Error as IoError, ErrorKind, Write};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use mysql::consts::{CapabilityFlags, ColumnFlags, ColumnType};
use mysql::prelude::Queryable;
use mysql::{Column, Conn, LocalInfileHandler, OptsBuilder, QueryResult, Statement, Value};

/// One materialized column value, already decoded to a PHP-friendly scalar.
pub enum Cell {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

/// A pending bound parameter value, converted to a `mysql::Value` at execute time.
#[derive(Clone)]
pub enum Bind {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    /// Text rendered with MySQL's national-character `N'…'` introducer on the
    /// emulated-prepare path (`PDO::PARAM_STR_NATL`). Native prepares send the
    /// same byte payload as ordinary text and let the server type the parameter.
    NationalText(String),
    /// Raw bytes, sent as-is (rather than through a lossy UTF-8 `String`) so a
    /// BLOB-style parameter round-trips embedded NUL bytes and arbitrary binary
    /// content unchanged.
    Bytes(Vec<u8>),
}

/// How a result column's MySQL type should render as text — the temporal types
/// need their own formatting; everything else decodes directly from the value.
/// `PartialEq`/`Debug` are only needed for the unit test asserting the BIT/
/// GEOMETRY classification below; deriving them unconditionally is simpler
/// than gating and costs nothing (both are trivial, non-`pub` derives).
#[derive(Clone, Copy, PartialEq, Debug)]
enum ColKind {
    Binary,
    Date,
    DateTime,
    Time,
    Other,
}

/// The `character_set` value MySQL uses for the special `binary` pseudo-collation
/// (collation id 63, `binary` charset): a `VARBINARY`/`BINARY` column always
/// reports this character set regardless of the connection's own charset, and it
/// is the ONLY signal that distinguishes those columns from `VARCHAR`/`CHAR` —
/// both pairs share the same wire `ColumnType` (`MYSQL_TYPE_VAR_STRING` /
/// `MYSQL_TYPE_STRING`).
const MYSQL_BINARY_CHARSET: u16 = 63;

/// The connect timeout applied when neither the DSN nor `PDO::ATTR_TIMEOUT`
/// supplies one (F-CORE-10). php-src's mysql handle factory reads
/// `connect_timeout = pdo_attr_lval(driver_options, PDO_ATTR_TIMEOUT, 30)`
/// (`mysql_driver.c:755`) and unconditionally feeds it to
/// `mysql_options(MYSQL_OPT_CONNECT_TIMEOUT, …)` (`mysql_driver.c:784`) — the 30 s
/// bound is ALWAYS in force, and `ATTR_TIMEOUT` only *changes* its value, never
/// removes it. Without this default, a plain `mysql:` connection to a black-holed
/// host fell back on the OS TCP timeout and could hang for minutes where real PHP
/// gives up after 30 s.
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 30;

/// Connection-time pdo_mysql options packed by the generated PDO prelude.
#[derive(Debug, Clone)]
struct MyDriverOptions {
    local_infile: bool,
    local_infile_directory: Option<PathBuf>,
    compress: bool,
    ignore_space: bool,
    multi_statements: bool,
    buffered_query: bool,
    ssl_ca_path: Option<PathBuf>,
}

impl Default for MyDriverOptions {
    /// Returns php-src/mysqlnd's default connection-option state.
    fn default() -> Self {
        Self {
            local_infile: false,
            local_infile_directory: None,
            compress: false,
            ignore_space: false,
            multi_statements: true,
            buffered_query: true,
            ssl_ca_path: None,
        }
    }
}

impl ColKind {
    /// Classifies a MySQL column into the text-rendering bucket the decoder
    /// needs (date-only, date+time, time-of-day, or value-driven).
    ///
    /// `MYSQL_TYPE_BIT` and `MYSQL_TYPE_GEOMETRY` (P0-D) are routed through
    /// `Binary` alongside the BLOB types: both carry arbitrary non-UTF-8 bytes
    /// (a `BIT(8)` column's high-bit values, WKB-encoded geometry), and without
    /// this, `decode_value` would run them through the lossy
    /// `String::from_utf8_lossy` path used for `Other`, corrupting the bytes
    /// into U+FFFD replacement characters. This matches php-src's mysqlnd,
    /// which returns both types as raw (binary-string) bytes.
    ///
    /// `VARBINARY`/`BINARY` (P1) arrive on the wire as the exact same
    /// `ColumnType` as `VARCHAR`/`CHAR` (`MYSQL_TYPE_VAR_STRING`/
    /// `MYSQL_TYPE_STRING` respectively) — `ColumnType` alone cannot tell them
    /// apart. The one distinguishing signal is the column's character set: a
    /// `VARBINARY`/`BINARY` column is always tagged with charset 63 (the
    /// `binary` collation), while a real `VARCHAR`/`CHAR` carries the
    /// connection's text charset (e.g. utf8mb4 = 45/46/224...). Without this,
    /// those columns fell to `Other` and every non-UTF-8 byte they held was
    /// silently replaced with U+FFFD by the lossy decode path — matching
    /// php-src's mysqlnd, which also keys off the charset-63 marker to return
    /// `VARBINARY`/`BINARY` as raw bytes.
    fn from_column(col: &Column) -> ColKind {
        match col.column_type() {
            ColumnType::MYSQL_TYPE_TINY_BLOB
            | ColumnType::MYSQL_TYPE_BLOB
            | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
            | ColumnType::MYSQL_TYPE_LONG_BLOB
            | ColumnType::MYSQL_TYPE_BIT
            | ColumnType::MYSQL_TYPE_GEOMETRY => ColKind::Binary,
            ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_STRING
                if col.character_set() == MYSQL_BINARY_CHARSET =>
            {
                ColKind::Binary
            }
            ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE => ColKind::Date,
            ColumnType::MYSQL_TYPE_DATETIME
            | ColumnType::MYSQL_TYPE_DATETIME2
            | ColumnType::MYSQL_TYPE_TIMESTAMP
            | ColumnType::MYSQL_TYPE_TIMESTAMP2 => ColKind::DateTime,
            ColumnType::MYSQL_TYPE_TIME | ColumnType::MYSQL_TYPE_TIME2 => ColKind::Time,
            _ => ColKind::Other,
        }
    }
}

/// Returns a MySQL result column's PDO-visible name, optionally prefixed with
/// the protocol table label for `PDO::ATTR_FETCH_TABLE_NAMES`.
fn column_display_name(column: &Column, fetch_table_names: bool) -> String {
    if fetch_table_names {
        format!("{}.{}", column.table_str(), column.name_str())
    } else {
        column.name_str().into_owned()
    }
}

/// MySQL's own name for a wire column type — the `native_type` key of
/// `PDOStatement::getColumnMeta()` (F-MY-08).
///
/// The strings are php-src's verbatim, produced by `type_to_name_native`
/// (`ext/pdo_mysql/mysql_statement.c:716-770`), whose
/// `PDO_MYSQL_NATIVE_TYPE_NAME(x)` macro stringifies the `MYSQL_TYPE_` suffix —
/// hence the ones no one would guess from the SQL keyword: an `INT` column is
/// `LONG`, a `TINYINT` is `TINY`, a `BIGINT` is `LONGLONG`, a `MEDIUMINT` is
/// `INT24`, a `VARCHAR` is `VAR_STRING`, a `CHAR` is `STRING`, and a modern
/// `DECIMAL` is `NEWDECIMAL` (`DECIMAL` is only the pre-5.0 legacy type).
///
/// Returns `""` for a type php-src's switch has no case for (its `default: return
/// NULL`, which makes `pdo_mysql_stmt_col_meta` OMIT the `native_type` key
/// entirely — `mysql_statement.c:812-815`). That covers the `mysql` crate's
/// `MYSQL_TYPE_VARCHAR` (a server-internal type never sent on the wire),
/// `MYSQL_TYPE_TIMESTAMP2`/`DATETIME2`/`TIME2` (likewise internal — the wire
/// carries the plain `TIMESTAMP`/`DATETIME`/`TIME` codes), `MYSQL_TYPE_TYPED_ARRAY`
/// (replication-only) and `MYSQL_TYPE_UNKNOWN`. php-src also names `VECTOR`
/// (MySQL 9), but the `mysql` crate's `ColumnType` has no such variant to match on.
/// The empty string is the bridge's neutral "no metadata" value, so the caller
/// treats those exactly as php-src does: no `native_type` at all.
fn native_type_name(t: ColumnType) -> &'static str {
    match t {
        ColumnType::MYSQL_TYPE_STRING => "STRING",
        ColumnType::MYSQL_TYPE_VAR_STRING => "VAR_STRING",
        ColumnType::MYSQL_TYPE_TINY => "TINY",
        ColumnType::MYSQL_TYPE_BIT => "BIT",
        ColumnType::MYSQL_TYPE_SHORT => "SHORT",
        ColumnType::MYSQL_TYPE_LONG => "LONG",
        ColumnType::MYSQL_TYPE_LONGLONG => "LONGLONG",
        ColumnType::MYSQL_TYPE_INT24 => "INT24",
        ColumnType::MYSQL_TYPE_FLOAT => "FLOAT",
        ColumnType::MYSQL_TYPE_DOUBLE => "DOUBLE",
        ColumnType::MYSQL_TYPE_DECIMAL => "DECIMAL",
        ColumnType::MYSQL_TYPE_NEWDECIMAL => "NEWDECIMAL",
        ColumnType::MYSQL_TYPE_GEOMETRY => "GEOMETRY",
        ColumnType::MYSQL_TYPE_TIMESTAMP => "TIMESTAMP",
        ColumnType::MYSQL_TYPE_YEAR => "YEAR",
        ColumnType::MYSQL_TYPE_SET => "SET",
        ColumnType::MYSQL_TYPE_ENUM => "ENUM",
        ColumnType::MYSQL_TYPE_DATE => "DATE",
        ColumnType::MYSQL_TYPE_NEWDATE => "NEWDATE",
        ColumnType::MYSQL_TYPE_JSON => "JSON",
        ColumnType::MYSQL_TYPE_TIME => "TIME",
        ColumnType::MYSQL_TYPE_DATETIME => "DATETIME",
        ColumnType::MYSQL_TYPE_TINY_BLOB => "TINY_BLOB",
        ColumnType::MYSQL_TYPE_MEDIUM_BLOB => "MEDIUM_BLOB",
        ColumnType::MYSQL_TYPE_LONG_BLOB => "LONG_BLOB",
        ColumnType::MYSQL_TYPE_BLOB => "BLOB",
        ColumnType::MYSQL_TYPE_NULL => "NULL",
        // php-src's `default: return NULL` — see the doc comment. A wildcard (not
        // the remaining variants spelled out) so a `mysql` crate bump that adds a
        // wire type keeps compiling into this same php-src-faithful "omit the key".
        _ => "",
    }
}

/// A live MySQL/MariaDB connection plus the last operation's bookkeeping that PDO
/// reads back (`rowCount`, `lastInsertId`, `errorCode`/`errorInfo`).
pub struct MyConn {
    conn: MyClientSlot,
    /// Transport description returned by `PDO::ATTR_CONNECTION_STATUS`, captured
    /// from the resolved connection options in php-src's `mysql_get_host_info()`
    /// shape (`"host via TCP/IP"` or `"Localhost via UNIX socket"`).
    pub host_info: String,
    pub changes: i64,
    pub errmsg: String,
    pub errcode: i64,
    /// 5-char SQLSTATE for the connection's last operation, taken from the ERR
    /// packet's SQLSTATE marker (`mysql::error::MySqlError::state`, which the
    /// client already parses from the wire protocol's `#`-prefixed field).
    /// "00000" on success; "HY000" for a transport/protocol error that carries no
    /// SQL error (not a `MySqlError`).
    pub sqlstate: String,
    /// The most recent non-zero AUTO_INCREMENT id, kept sticky across later
    /// non-INSERT statements (which would otherwise reset the protocol field) to
    /// match `PDO::lastInsertId()`. Stored as `u64` (P2-2's sibling gap): a
    /// `BIGINT UNSIGNED` AUTO_INCREMENT id can exceed `i64::MAX`, and casting at
    /// storage time would wrap it negative before either accessor ever runs.
    pub last_id: u64,
    /// Current session autocommit mode, kept in sync with `SET autocommit` for
    /// `PDO::ATTR_AUTOCOMMIT` reads and idempotent writes.
    pub autocommit: bool,
    /// Whether result column names are prefixed with their MySQL table name.
    pub fetch_table_names: bool,
    /// Default result buffering mode snapshotted by newly prepared statements.
    pub buffered_query: bool,
    /// Whether client-side execution accepts more than one SQL statement.
    pub multi_statements: bool,
    /// Whether an unbuffered statement still has unread rows.
    pub unbuffered_active: bool,
    /// Warning count from the final OK/EOF packet of the last completed operation.
    pub warning_count: u16,
    /// Best available live transaction state, updated after every successful
    /// bridge-owned command including raw `PDO::exec("BEGIN")` control SQL.
    pub in_transaction: bool,
    /// Handshake version cached while an unbuffered worker temporarily owns `Conn`.
    server_version: (u16, u16, u16),
    /// Session quoting mode cached before the client moves into a worker.
    no_backslash_escapes: bool,
    /// Worker currently owning the client for a demand-driven result stream.
    active_stream: Option<MyActiveStream>,
    /// Monotonic identity used to reject stale statement stream handles.
    next_stream_id: u64,
}

/// A live MySQL prepared statement and its buffered or demand-driven result.
pub struct MyStmt {
    pub conn_id: i64,
    /// Original SQL used for transaction-state bookkeeping and diagnostics.
    query_string: String,
    statement: Option<Statement>,
    /// Placeholder-translated SQL retained for the text-protocol emulated path.
    emulated_sql: Option<String>,
    /// Session quoting mode captured when the emulated statement is created.
    no_backslash_escape: bool,
    /// Most recent client-rendered SQL, exposed by `debugDumpParams()`.
    pub sent_sql: String,
    /// Maps a bare named placeholder (`name` from `:name`) to its 1-based slot.
    named_map: HashMap<String, i64>,
    /// For each `?` in source order, the 1-based bound slot that feeds it. Repeats
    /// for a reused named placeholder; `[1, 2, …]` for plain positional `?`.
    order: Vec<i64>,
    /// Bound values, indexed by 0-based slot (`slot 1` → index 0).
    binds: Vec<Bind>,
    /// Whether each slot was explicitly supplied for the current execution.
    bound: Vec<bool>,
    /// Result column names, available from the prepare (before execution).
    col_names: Vec<String>,
    /// Result column kinds, parallel to `col_names`, for temporal text rendering.
    col_kinds: Vec<ColKind>,
    /// Raw MySQL wire types, parallel to `col_names` (F-MY-08). Kept alongside the
    /// coarser `col_kinds` because `getColumnMeta`'s `native_type` reports the
    /// server's OWN type name (`VAR_STRING`, `NEWDECIMAL`, `LONGLONG`, …), a
    /// distinction `ColKind` deliberately collapses — every non-temporal,
    /// non-binary type lands in `ColKind::Other`. Refreshed from the live result
    /// on execute, like `col_names`/`col_kinds`, so a `CALL`'s columns (unknown at
    /// prepare time — see `execute`) get real names rather than none.
    col_types: Vec<ColumnType>,
    /// Native table label parallel to `col_names` for `getColumnMeta()`.
    col_tables: Vec<String>,
    /// Raw MySQL field flags parallel to `col_names`.
    col_flags: Vec<ColumnFlags>,
    /// Declared maximum byte lengths parallel to `col_names`.
    col_lengths: Vec<u32>,
    /// Native decimal precision markers parallel to `col_names`.
    col_precisions: Vec<u8>,
    /// Buffered rows, or the single active row for an unbuffered stream.
    rows: Vec<Vec<Cell>>,
    /// Result sets after the active one, retained in wire order for
    /// `PDOStatement::nextRowset()`.
    remaining_rowsets: Vec<MyRowset>,
    /// Current 0-based row index; `-1` before the first `step()`.
    cursor: isize,
    /// Whether the query has been executed yet.
    executed: bool,
    /// Whether the SQL text is a `CALL <procedure>(...)` invocation (P0-C). A
    /// stored procedure's real result shape (whether it `SELECT`s any rows at
    /// all, and how many columns) is only known once it actually runs —
    /// MySQL's `COM_STMT_PREPARE` always reports zero columns for a `CALL`,
    /// unlike a plain `SELECT`, whose column list is already known at prepare
    /// time. `column_count()` uses this flag to report a non-zero placeholder
    /// before execution instead of that genuine (but misleading) zero — see
    /// `column_count`'s doc comment for why that matters.
    is_call: bool,
    /// Snapshot of the connection's `ATTR_USE_BUFFERED_QUERY` mode.
    pub buffered: bool,
    /// Connection-owned demand stream used when buffering is disabled.
    stream_id: Option<u64>,
}

impl Drop for MyConn {
    /// Stops any active row worker before the owning PDO connection is released.
    fn drop(&mut self) {
        self.finish_active_stream();
    }
}

/// Keeps the MySQL client optional while an unbuffered worker owns it.
struct MyClientSlot(Option<Conn>);

impl Deref for MyClientSlot {
    type Target = Conn;

    /// Borrows the connected client outside a demand-driven result stream.
    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .expect("MySQL client is owned by an active unbuffered stream")
    }
}

impl DerefMut for MyClientSlot {
    /// Mutably borrows the connected client outside a demand-driven result stream.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
            .as_mut()
            .expect("MySQL client is owned by an active unbuffered stream")
    }
}

/// Commands sent to the MySQL worker.
enum MyStreamCommand {
    Next,
    Close,
}

/// Responses emitted by a MySQL row-stream worker.
enum MyStreamResponse {
    Rowset(MyRowset),
    Row(Vec<Cell>),
    RowsetEnd,
    Finished(Conn, u16),
    Failed(Conn, String, i64, String),
}

/// Connection-owned control plane for one active MySQL result stream.
struct MyActiveStream {
    id: u64,
    commands: mpsc::Sender<MyStreamCommand>,
    responses: mpsc::Receiver<MyStreamResponse>,
    worker: Option<JoinHandle<()>>,
}

/// One fully materialized MySQL protocol result set.
struct MyRowset {
    /// Server-reported affected rows for an OK-packet result.
    affected: i64,
    /// AUTO_INCREMENT id reported by this result set, when present.
    last_id: Option<u64>,
    /// Column names for row-returning result sets.
    col_names: Vec<String>,
    /// Decoding kinds parallel to `col_names`.
    col_kinds: Vec<ColKind>,
    /// Raw wire types parallel to `col_names`.
    col_types: Vec<ColumnType>,
    /// Native table labels parallel to `col_names`.
    col_tables: Vec<String>,
    /// Raw field flags parallel to `col_names`.
    col_flags: Vec<ColumnFlags>,
    /// Declared maximum byte lengths parallel to `col_names`.
    col_lengths: Vec<u32>,
    /// Native decimal precision markers parallel to `col_names`.
    col_precisions: Vec<u8>,
    /// Decoded rows for this result set.
    rows: Vec<Vec<Cell>>,
}

impl MyRowset {
    /// Returns PDO's row count for this result set: buffered row count for a
    /// SELECT-like set, otherwise the server's affected-row count.
    fn row_count(&self) -> i64 {
        if self.col_names.is_empty() {
            self.affected
        } else {
            self.rows.len() as i64
        }
    }
}

/// Extracts a MySQL server error code from a driver error, or `1` for transport /
/// protocol errors that carry no SQL error number.
fn err_code(e: &mysql::Error) -> i64 {
    match e {
        mysql::Error::MySqlError(me) => me.code as i64,
        _ => 1,
    }
}

/// Extracts the 5-char SQLSTATE from a driver error. The `mysql` crate already
/// parses the ERR packet's SQLSTATE marker (the `#` byte followed by 5 chars)
/// into `MySqlError::state`, so no manual wire-protocol parsing is needed here.
/// Falls back to the generic `HY000` for transport/protocol errors that carry no
/// SQL error (not a `MySqlError`).
fn err_sqlstate(e: &mysql::Error) -> String {
    match e {
        mysql::Error::MySqlError(me) => me.state.clone(),
        _ => "HY000".to_string(),
    }
}

/// Parses a PDO `mysql:` DSN (semicolon-separated `key=value` pairs) into the
/// `mysql` client's connection options, plus a validated `charset` value (P2-3,
/// second tuple element) for the caller to apply. Recognises `host`, `port`,
/// `dbname`, `unix_socket`, the credential keys the prelude folds in (`user`,
/// `password`), `connect_timeout` (P2-1: seconds, mapped to `tcp_connect_timeout`
/// — backs `PDO::ATTR_TIMEOUT`, which the prelude folds into the DSN alongside
/// the credentials since the option needs to take effect before the socket
/// connects), and `charset`; other unknown keys are accepted and ignored.
/// Returns an error for a DSN without the `mysql:` prefix.
///
/// `charset` has no direct `OptsBuilder` knob in the `mysql` crate, so it is
/// returned as data rather than applied here — `MyConn::open` turns it into a
/// `SET NAMES <charset>` statement alongside `ATTR_INIT_COMMAND` (P1-9) via
/// `OptsBuilder::init`. It is validated here to only contain the identifier
/// characters a real MySQL charset name uses (`[A-Za-z0-9_]`), so a stray value
/// cannot inject SQL into that generated statement; an invalid value is silently
/// dropped (documented best-effort, matching the surrounding DSN parsing style).
///
/// The connect timeout is ALWAYS applied (F-CORE-10), defaulting to
/// [`DEFAULT_CONNECT_TIMEOUT_SECS`] (30 s) for php-src parity — pdo_mysql's
/// `mysql_options(MYSQL_OPT_CONNECT_TIMEOUT, …)` is unconditional, with
/// `PDO::ATTR_TIMEOUT` only *overriding* the 30 s value rather than lifting the
/// bound. A `connect_timeout=` DSN key (the seam the prelude folds `ATTR_TIMEOUT`
/// into) therefore wins over the default whenever it parses. Deliberate divergence
/// from php-src: the bound is only enforced on the TCP path, since the `mysql`
/// crate consults `tcp_connect_timeout` in `connect_tcp` alone and a `unix_socket`
/// DSN connects locally (where a black-holed peer — the hang this guards against —
/// cannot arise).
///
/// `found_rows` is the connection's `Pdo\Mysql::ATTR_FOUND_ROWS` constructor
/// option (F-MY-06), threaded in from the open entrypoint rather than read from
/// the DSN: it is an attribute, not a DSN key, and it has to be known *before*
/// the handshake because it only exists as a capability bit negotiated there.
pub fn build_opts(
    dsn: &str,
    found_rows: bool,
    ignore_space: bool,
) -> Result<(OptsBuilder, Option<String>), String> {
    let body = dsn
        .strip_prefix("mysql:")
        .ok_or_else(|| "could not find driver (expected a mysql: DSN)".to_string())?;
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut dbname: Option<String> = None;
    let mut socket: Option<String> = None;
    let mut user: Option<String> = None;
    let mut password: Option<String> = None;
    let mut connect_timeout: Option<u64> = None;
    let mut charset: Option<String> = None;
    for pair in body.split(';') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "host" => host = Some(value),
            "port" => port = value.parse::<u16>().ok(),
            "dbname" => dbname = Some(value),
            "unix_socket" | "socket" => socket = Some(value),
            "user" => user = Some(percent_decode_credential(&value)),
            "password" => password = Some(percent_decode_credential(&value)),
            "connect_timeout" => connect_timeout = value.parse::<u64>().ok(),
            "charset" => {
                if !value.is_empty()
                    && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    charset = Some(value);
                }
            }
            // any other key is accepted for DSN compatibility but has no direct
            // option here.
            _ => {}
        }
    }
    let mut opts = OptsBuilder::new().user(user).pass(password).db_name(dbname);
    // F-MY-02: `unix_socket` only wins when the DSN names no host, or names exactly
    // `localhost`. php-src's handle factory takes the socket under precisely that
    // condition — `if (vars[0].optval && !strcmp("localhost", vars[0].optval))`
    // (`mysql_driver.c:940-946`), with the DSN parser defaulting an absent `host` to
    // `"localhost"` — so `mysql:host=127.0.0.1;unix_socket=/tmp/mysql.sock` is
    // TCP-only in real PHP, the socket key silently ignored. Preferring the socket
    // whenever it was present (as this did) connected such a DSN to a DIFFERENT
    // server than php-src would. The comparison is case-sensitive, matching the
    // `strcmp`; `127.0.0.1` is deliberately NOT localhost here, exactly as in
    // php-src (and in the mysql client itself, where the two are distinct).
    // Otherwise connect over TCP, defaulting the host so a `mysql:dbname=…` DSN
    // still reaches a local server.
    let host_is_localhost = host.as_deref().is_none_or(|h| h == "localhost");
    match socket.filter(|_| host_is_localhost) {
        Some(sock) => opts = opts.socket(Some(sock)),
        None => {
            opts = opts.ip_or_hostname(Some(host.unwrap_or_else(|| "localhost".to_string())));
            if let Some(p) = port {
                opts = opts.tcp_port(p);
            }
        }
    }
    // F-MY-06: `Pdo\Mysql::ATTR_FOUND_ROWS` ORs `CLIENT_FOUND_ROWS` into the connect
    // capabilities (php-src `mysql_driver.c:776-778`), which switches what the server
    // reports as the affected-row count of an UPDATE — and so `PDOStatement::
    // rowCount()` — from "rows actually CHANGED" to "rows MATCHED by the WHERE
    // clause". Without it there was no way to opt into the matched-rows semantics
    // apps commonly rely on. The `mysql` crate ORs `additional_capabilities` into the
    // handshake's client flags (`conn/mod.rs:739`), and its forbidden-flag filter
    // covers only the capabilities the connection manages itself (`CLIENT_SSL`,
    // `CLIENT_COMPRESS`, `CLIENT_PROTOCOL_41`, the MULTI_* pair, …) — never
    // `CLIENT_FOUND_ROWS` — so the bit does reach the server.
    let mut capabilities = CapabilityFlags::empty();
    if found_rows {
        capabilities.insert(CapabilityFlags::CLIENT_FOUND_ROWS);
    }
    if ignore_space {
        capabilities.insert(CapabilityFlags::CLIENT_IGNORE_SPACE);
    }
    if !capabilities.is_empty() {
        opts = opts.additional_capabilities(capabilities);
    }
    // F-CORE-10: unconditional, so a DSN that names neither `connect_timeout` nor
    // (through the prelude) `ATTR_TIMEOUT` still inherits php-src's 30 s bound
    // instead of waiting out the OS TCP timeout. An explicit value — from either
    // seam, both of which land in `connect_timeout` above — wins over the default.
    let secs = connect_timeout.unwrap_or(DEFAULT_CONNECT_TIMEOUT_SECS);
    opts = opts.tcp_connect_timeout(Some(Duration::from_secs(secs)));
    Ok((opts, charset))
}

/// Parses the percent-escaped MySQL driver-option string emitted by the PDO
/// prelude. Unsupported security options fail the connection explicitly instead
/// of being accepted into an inert attribute bag.
fn parse_driver_options(config: &str) -> Result<MyDriverOptions, String> {
    let mut options = MyDriverOptions::default();
    for pair in config.split(';').filter(|pair| !pair.is_empty()) {
        let Some((key, raw_value)) = pair.split_once('=') else {
            continue;
        };
        let value = percent_decode_credential(raw_value);
        match key {
            "local" => options.local_infile = value == "1",
            "dir" if !value.is_empty() => {
                options.local_infile_directory = Some(PathBuf::from(value))
            }
            "compress" => options.compress = value == "1",
            "ignore" => options.ignore_space = value == "1",
            "multi" => options.multi_statements = value != "0",
            "buffered" => options.buffered_query = value != "0",
            "capath" if !value.is_empty() => options.ssl_ca_path = Some(PathBuf::from(value)),
            "cipher" if !value.is_empty() => {
                return Err("Pdo\\Mysql::ATTR_SSL_CIPHER cannot be honored by the rustls MySQL client".to_string())
            }
            "serverkey" if !value.is_empty() => {
                return Err("Pdo\\Mysql::ATTR_SERVER_PUBLIC_KEY cannot be honored by the native MySQL client".to_string())
            }
            _ => {}
        }
    }
    if let Some(directory) = options.local_infile_directory.as_mut() {
        *directory = directory.canonicalize().map_err(|error| {
            format!(
                "Pdo\\Mysql::ATTR_LOCAL_INFILE_DIRECTORY '{}': {error}",
                directory.display()
            )
        })?;
        if !directory.is_dir() {
            return Err(format!(
                "Pdo\\Mysql::ATTR_LOCAL_INFILE_DIRECTORY '{}' is not a directory",
                directory.display()
            ));
        }
    }
    Ok(options)
}

/// Builds the local-infile callback installed on every MySQL connection. Disabled
/// connections always reject the server request. Enabled connections read the
/// requested file bytes, optionally requiring the canonical path to remain below
/// `allowed_directory`, and never acknowledge an empty synthetic upload on error.
fn local_infile_handler(
    enabled: bool,
    allowed_directory: Option<PathBuf>,
) -> LocalInfileHandler {
    LocalInfileHandler::new(move |file_name, writer| {
        if !enabled {
            return Err(IoError::new(
                ErrorKind::PermissionDenied,
                "LOAD DATA LOCAL INFILE is disabled",
            ));
        }
        let requested = String::from_utf8_lossy(file_name);
        let path = PathBuf::from(requested.as_ref());
        let absolute = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
        let canonical = absolute.canonicalize()?;
        if let Some(root) = &allowed_directory {
            if !canonical.starts_with(root) {
                return Err(IoError::new(
                    ErrorKind::PermissionDenied,
                    format!(
                        "LOCAL INFILE path '{}' is outside allowed directory '{}'",
                        canonical.display(),
                        root.display()
                    ),
                ));
            }
        }
        writer.write_all(&std::fs::read(canonical)?)
    })
}

/// Percent-decodes a `user=`/`password=` DSN value (F-CORE-02). The prelude
/// percent-encodes '%' and ';' on the credential it folds into the DSN — '%'
/// first, so the '%' introduced by encoding ';' as `%3B` is not itself
/// re-encoded — precisely so a ';' or '%' embedded in the username/password
/// survives `body.split(';')` above instead of truncating the credential.
/// This undoes that encoding; a value with no '%' is returned unchanged
/// (byte-identical) without allocating a new string. An invalid or truncated
/// escape (not two hex digits) is copied through verbatim rather than
/// rejected, since a bare '%' is legal in a value that predates this scheme.
fn percent_decode_credential(raw: &str) -> String {
    if !raw.contains('%') {
        return raw.to_string();
    }
    let b = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && b[i + 1].is_ascii_hexdigit()
            && b[i + 2].is_ascii_hexdigit()
        {
            let hi = (b[i + 1] as char).to_digit(16).unwrap() as u8;
            let lo = (b[i + 2] as char).to_digit(16).unwrap() as u8;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Returns whether `b` is an identifier byte (`[A-Za-z0-9_]`), used to read a
/// `:name` placeholder's name.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Returns the byte length of the UTF-8 sequence led by `b` (1 for ASCII, 2-4
/// for a multi-byte lead byte). `sql` is always valid UTF-8, so slicing
/// `&sql[i..i + utf8_len(bytes[i])]` lands on a valid char boundary at both
/// ends — used to copy a content byte (or run of one multi-byte codepoint)
/// through `out.push_str` instead of `out.push(b as char)`, which corrupts any
/// codepoint above U+007F: a `u8` cast to `char` treats each raw continuation
/// byte as its own Latin-1 codepoint and re-encodes it as 2 UTF-8 bytes,
/// doubling/mangling every multi-byte character embedded in the SQL text
/// (BUG 1).
fn utf8_len(b: u8) -> usize {
    if b & 0x80 == 0 {
        1
    } else if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else if b & 0xF8 == 0xF0 {
        4
    } else {
        // A stray continuation byte can't start a codepoint in valid UTF-8;
        // fall back to one byte so the scanner still makes forward progress.
        1
    }
}

/// Returns whether `bytes[i]` opens a MySQL comment and, if so, its exclusive end
/// index. The single definition of MySQL's three comment forms, shared by
/// `translate_placeholders` (which copies the region verbatim, never scanning it
/// for placeholders) and `sql_is_call_statement` (which skips it) so the two can
/// never drift apart:
/// - `--` line comment, but only when the second dash is followed by one of the
///   whitespace/control bytes `[ \t\v\f\r]` — the php-src `mysql_sql_parser.re`
///   COMMENTS rule. Without that trailing byte `a--b` is the arithmetic `a - -b`
///   (unlike PostgreSQL, where a bare `--` already comments);
/// - `#` line comment, with no trailing-whitespace requirement;
/// - `/* ... */` block comment, non-nested, running to EOF if unterminated.
///
/// A line comment ends *at* the newline (exclusive), which the caller then treats
/// as ordinary text/whitespace. An out-of-range `i` opens nothing (`None`), so a
/// caller scanning to EOF needs no separate bounds check.
fn scan_my_comment(bytes: &[u8], i: usize) -> Option<usize> {
    let len = bytes.len();
    match *bytes.get(i)? {
        b'-' if i + 2 < len
            && bytes[i + 1] == b'-'
            && matches!(bytes[i + 2], b' ' | b'\t' | b'\x0b' | b'\x0c' | b'\r') =>
        {
            let mut j = i + 2;
            while j < len && bytes[j] != b'\n' {
                j += 1;
            }
            Some(j)
        }
        b'#' => {
            let mut j = i + 1;
            while j < len && bytes[j] != b'\n' {
                j += 1;
            }
            Some(j)
        }
        b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
            let mut j = i + 2;
            while j + 1 < len && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                j += 1;
            }
            Some(if j + 1 < len { j + 2 } else { len })
        }
        _ => None,
    }
}

/// Returns whether `b` is whitespace to MySQL's lexer (`[ \t\n\v\f\r]`, the
/// `_MY_SPC` ctype class). Deliberately narrower than `str::trim_start`, which
/// also strips Unicode spaces (e.g. NBSP) that the server would reject as a
/// syntax error rather than skip.
fn is_my_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// Returns whether `sql` invokes a stored procedure (`CALL proc(...)`,
/// case-insensitive), ignoring any leading whitespace *and comments*. Used by
/// `prepare()` to set `MyStmt::is_call` — see `column_count`'s doc comment for why
/// a `CALL`'s prepare-time column count needs this special-casing. Requires a
/// non-identifier byte (or end of string) right after the keyword, so
/// `CALLBACK(...)` — a (nonsensical but not a stored-procedure-call) function-call
/// expression — is never mistaken for `CALL BACK(...)`.
///
/// The comment skipping is load-bearing, not cosmetic: the server ignores whatever
/// leads the statement, so `/* hint */ CALL p()` and `-- note\nCALL p()` are just as
/// much stored-procedure calls as a bare `CALL p()`. Testing only past the
/// whitespace left those mis-flagged as non-`CALL`, which handed the prelude the
/// genuine (but meaningless) prepare-time column count of `0` and so routed a
/// row-producing procedure into the no-result DML branch — silently discarding its
/// first row (the row-dropping bug `column_count` documents).
fn sql_is_call_statement(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    // Whitespace and the three comment forms can interleave in any order and
    // quantity ahead of the first keyword, so alternate between them until neither
    // consumes anything (a line comment stops at its newline, which the whitespace
    // pass then eats, so the loop always makes progress).
    loop {
        let start = i;
        while i < len && is_my_space(bytes[i]) {
            i += 1;
        }
        if let Some(end) = scan_my_comment(bytes, i) {
            i = end;
        }
        if i == start {
            break;
        }
    }
    let rest = &bytes[i..];
    if rest.len() < 4 || !rest[..4].eq_ignore_ascii_case(b"call") {
        return false;
    }
    rest.get(4).is_none_or(|&b| !is_ident_byte(b))
}

/// Returns whether `sql` contains a second non-empty statement after a real
/// semicolon separator. Quoted regions and MySQL comments are skipped with the
/// same escape rules as placeholder translation, so a semicolon inside data
/// never trips `ATTR_MULTI_STATEMENTS = false`.
fn sql_has_multiple_statements(sql: &str, no_backslash_escapes: bool) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0usize;
    let mut saw_statement = false;
    let mut completed_statement = false;
    while i < bytes.len() {
        if is_my_space(bytes[i]) {
            i += 1;
            continue;
        }
        if let Some(end) = scan_my_comment(bytes, i) {
            i = end;
            continue;
        }
        if bytes[i] == b';' {
            if saw_statement {
                completed_statement = true;
                saw_statement = false;
            }
            i += 1;
            continue;
        }
        if completed_statement {
            return true;
        }
        saw_statement = true;
        if matches!(bytes[i], b'\'' | b'"') {
            i = scan_my_string(bytes, i, bytes[i], no_backslash_escapes)
                .unwrap_or(bytes.len());
            continue;
        }
        if bytes[i] == b'`' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'`' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'`' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    false
}

/// Scans a MySQL quoted region opened by `quote` (`'` or `"`) starting at
/// `start` (the index of the opening quote byte), returning the exclusive end
/// index just past the closing quote, or `None` when it is unterminated. Both quote
/// styles are string literals in MySQL's default `sql_mode` and share the same
/// escaping: a doubled quote (`''`/`""`) is a literal quote, and a backslash
/// escapes the following byte unconditionally (so `\'`/`\"`/`\\` never
/// terminate or mis-parse the string).
///
/// F-MY-03: under the `NO_BACKSLASH_ESCAPES` `sql_mode` (`no_backslash_escapes`
/// true) the SERVER treats `\` as an ordinary byte inside a string literal —
/// doubling is then the ONLY escape. This is the same server-side fact
/// `PDO::quote()`'s MySQL branch already keys off (it falls back to `''`-doubling
/// there), and the scanner has to agree with it: assuming backslash-escaping in
/// that mode makes the scanner disagree with the server about where a literal
/// ENDS, so a `?`/`:name` the server sees as a real placeholder can be swallowed
/// as string content (e.g. `'a\' , ?` — the server closes the literal at the `'`
/// after the backslash, this scanner would not), yielding a bound-parameter count
/// that disagrees with the server's real placeholder count.
fn scan_my_string(
    bytes: &[u8],
    start: usize,
    quote: u8,
    no_backslash_escapes: bool,
) -> Option<usize> {
    let len = bytes.len();
    let mut j = start + 1;
    loop {
        if j >= len {
            return None;
        }
        let cj = bytes[j];
        if !no_backslash_escapes && cj == b'\\' && j + 1 < len {
            j += 2;
            continue;
        }
        if cj == quote {
            if j + 1 < len && bytes[j + 1] == quote {
                j += 2;
                continue;
            }
            return Some(j + 1);
        }
        j += 1;
    }
}

/// Translates PDO `?` and `:name` placeholders to MySQL's positional `?`,
/// returning the rewritten SQL, the bare-name → 1-based-slot map, a per-`?`
/// `order` (the slot each emitted `?` reads), and whether the SQL mixed a
/// positional `?` with a named `:name` (PDO forbids this combination;
/// `prepare()` checks the flag and raises `HY093` before ever reaching the
/// server).
///
/// The scanner tracks these mutually exclusive regions, copying each verbatim
/// (never scanning `?`/`:name` inside them) before resuming normal placeholder
/// scanning:
/// - `-- ...` and `# ...` line comments (to end of line or EOF);
/// - `/* ... */` block comments (non-nested, to the first `*/` or EOF);
/// - `'...'` and `"..."` string literals — both quote styles honor the doubled-
///   quote escape (`''`/`""`) and, unless `no_backslash_escapes` is set, backslash
///   escapes (`\'`, `\"`, `\\`, …), per MySQL's default `sql_mode`;
/// - `` `...` `` backtick-quoted identifiers, with `` `` `` as the doubled-quote
///   escape (no backslash escaping here).
///
/// `no_backslash_escapes` is the connection's LIVE `NO_BACKSLASH_ESCAPES`
/// `sql_mode` state (F-MY-03), threaded in from `MyConn::prepare` — see
/// [`scan_my_string`] for why a scanner that disagrees with the server about
/// backslash escaping also disagrees with it about the placeholder count. It only
/// affects the two string-literal forms: a backtick-quoted identifier never
/// honors backslash escapes in either mode, and a comment has no escapes at all.
///
/// A run of two or more `?` (e.g. `??`) is a single verbatim text token (php-src
/// treats it the same way) and allocates no slot; only a lone `?` is a real
/// positional placeholder. Symmetrically, a run of two or more `:` is left
/// untouched rather than read as a named placeholder.
///
/// A `:name` immediately preceded by an alphanumeric byte is NOT a named
/// placeholder (matching php-src's `pdo_sql_parser.re`, which skips the same
/// way).
pub fn translate_placeholders(
    sql: &str,
    no_backslash_escapes: bool,
) -> (String, HashMap<String, i64>, Vec<i64>, bool) {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut named: HashMap<String, i64> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();
    let mut next_slot: i64 = 1;
    let mut i = 0;
    let mut saw_positional = false;
    let mut saw_named = false;
    while i < len {
        let c = bytes[i];
        // All three comment forms (`-- `, `#`, `/* */`) are copied verbatim, so the
        // `?`/`:name` inside one is never mistaken for a placeholder. The rules live
        // in `scan_my_comment`, shared with `sql_is_call_statement` — a comment must
        // mean the same thing to both scanners.
        if let Some(end) = scan_my_comment(bytes, i) {
            if c == b'/'
                && i + 1 < len
                && bytes[i + 1] == b'*'
                && end == len
                && (len < 2 || &bytes[len - 2..] != b"*/")
            {
                // php-src's re2c scanner backtracks an unterminated block comment
                // to its one-byte fallback instead of swallowing the rest of the
                // statement. Copy only '/' so '*' and later placeholders are scanned.
                out.push('/');
                i += 1;
                continue;
            }
            out.push_str(&sql[i..end]);
            i = end;
            continue;
        }
        match c {
            b'\'' | b'"' => {
                if let Some(end) = scan_my_string(bytes, i, c, no_backslash_escapes) {
                    out.push_str(&sql[i..end]);
                    i = end;
                } else {
                    // Match php-src's scanner fallback for an unterminated quote:
                    // the opener is ordinary text and following placeholders remain visible.
                    out.push(c as char);
                    i += 1;
                }
            }
            b'`' => {
                // Backtick-quoted identifier: verbatim, with doubled `` `` ``
                // as the escape (no backslash escaping here).
                let start = i;
                let mut j = i + 1;
                loop {
                    if j >= len {
                        break;
                    }
                    if bytes[j] == b'`' {
                        if j + 1 < len && bytes[j + 1] == b'`' {
                            j += 2;
                            continue;
                        }
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                out.push_str(&sql[start..j]);
                i = j;
            }
            b'?' => {
                // A run of 2+ `?` is a single verbatim text token, no slot
                // allocated; a lone `?` is a fresh positional placeholder.
                let mut j = i + 1;
                while j < len && bytes[j] == b'?' {
                    j += 1;
                }
                if j - i >= 2 {
                    out.push_str(&sql[i..j]);
                    i = j;
                } else {
                    out.push('?');
                    order.push(next_slot);
                    next_slot += 1;
                    saw_positional = true;
                    i += 1;
                }
            }
            b':' => {
                // A run of 2+ `:` is a single verbatim text token, never a named
                // placeholder — php-src's `MULTICHAR = [:]{2,}` rule is greedy
                // (re2c's maximal munch swallows the whole contiguous run). The run
                // must be consumed WHOLE: taking colons two at a time leaves the
                // third one of an odd run (`:::c`) to be re-scanned as a fresh
                // `:c`, conjuring a named placeholder php-src never emits. Mirrors
                // the `?`-run handling above.
                let mut run_end = i + 1;
                while run_end < len && bytes[run_end] == b':' {
                    run_end += 1;
                }
                if run_end - i >= 2 {
                    out.push_str(&sql[i..run_end]);
                    i = run_end;
                    continue;
                }
                let start = i + 1;
                let mut j = start;
                while j < len && is_ident_byte(bytes[j]) {
                    j += 1;
                }
                if j == start {
                    // A bare colon (not a named placeholder); emit verbatim.
                    out.push(':');
                    i += 1;
                    continue;
                }
                // php-src's `pdo_sql_parser.re` only treats `:name` as a bind
                // placeholder when the byte immediately before the colon is
                // NOT alphanumeric (BUG 2). Emit the colon verbatim; the
                // identifier bytes are then re-scanned as ordinary text by
                // the default arm on the next iterations.
                if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
                    out.push(':');
                    i += 1;
                    continue;
                }
                let name = &sql[start..j];
                // Reused names share a slot; first sight allocates the next slot.
                let slot = *named.entry(name.to_string()).or_insert_with(|| {
                    let s = next_slot;
                    next_slot += 1;
                    s
                });
                out.push('?');
                order.push(slot);
                saw_named = true;
                i = j;
            }
            _ => {
                // Copy the whole codepoint via a slice (BUG 1): `c as char`
                // would corrupt any multi-byte UTF-8 character (e.g. an
                // embedded `'café'` byte outside a recognized quoted region —
                // the ordinary/unquoted path).
                let n = utf8_len(c).min(len - i);
                out.push_str(&sql[i..i + n]);
                i += n;
            }
        }
    }
    let mixed = saw_positional && saw_named;
    (out, named, order, mixed)
}

/// Replaces the translated statement's real `?` markers with safely quoted
/// MySQL literals while preserving markers inside comments and quoted regions.
fn interpolate_emulated_sql(
    sql: &str,
    values: &[Value],
    national: &[bool],
    no_backslash_escape: bool,
) -> Result<String, String> {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + values.len() * 8);
    let mut value_index = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(end) = scan_my_comment(bytes, i) {
            if bytes[i] == b'/'
                && i + 1 < bytes.len()
                && bytes[i + 1] == b'*'
                && end == bytes.len()
                && (bytes.len() < 2 || &bytes[bytes.len() - 2..] != b"*/")
            {
                out.push('/');
                i += 1;
                continue;
            }
            out.push_str(&sql[i..end]);
            i = end;
            continue;
        }
        match bytes[i] {
            quote @ (b'\'' | b'"') => {
                if let Some(end) = scan_my_string(bytes, i, quote, no_backslash_escape) {
                    out.push_str(&sql[i..end]);
                    i = end;
                } else {
                    out.push(quote as char);
                    i += 1;
                }
            }
            b'`' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'`' {
                        i += 1;
                        if i < bytes.len() && bytes[i] == b'`' {
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                out.push_str(&sql[start..i]);
            }
            b'?' => {
                let mut end = i + 1;
                while end < bytes.len() && bytes[end] == b'?' {
                    end += 1;
                }
                if end - i > 1 {
                    out.push_str(&sql[i..end]);
                    i = end;
                    continue;
                }
                let value = values.get(value_index).ok_or_else(|| {
                    "Invalid parameter number: number of bound variables does not match number of tokens"
                        .to_string()
                })?;
                if national.get(value_index).copied().unwrap_or(false) {
                    out.push('N');
                }
                out.push_str(&value.as_sql(no_backslash_escape));
                value_index += 1;
                i += 1;
            }
            _ => {
                let len = utf8_len(bytes[i]).min(bytes.len() - i);
                out.push_str(&sql[i..i + len]);
                i += len;
            }
        }
    }
    if value_index != values.len() {
        return Err(
            "Invalid parameter number: number of bound variables does not match number of tokens"
                .to_string(),
        );
    }
    Ok(out)
}

/// Applies the prelude's packed `Pdo\Mysql::ATTR_SSL_*` config to `opts`, enabling
/// rustls TLS for the connection. The default build enables mysql 28's ring-backed
/// `mysql-tls` feature. An empty config leaves `opts` untouched (plaintext).
#[cfg(feature = "mysql-tls")]
fn apply_ssl_opts(opts: OptsBuilder, ssl_config: &str) -> Result<OptsBuilder, String> {
    if ssl_config.is_empty() {
        return Ok(opts);
    }
    install_crypto_provider();
    Ok(opts.ssl_opts(parse_ssl_config(ssl_config)))
}

/// Owns a temporary PEM bundle assembled from MySQL's CA file/directory options.
///
/// The mysql crate reads the path while `Conn::new` builds its rustls connector,
/// so the bundle only needs to survive that call and is removed automatically
/// afterwards, including on connection failure.
struct TemporaryCaBundle {
    path: PathBuf,
}

impl Drop for TemporaryCaBundle {
    /// Removes the private bundle created for one connection attempt.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Combines `ATTR_SSL_CA` and every PEM certificate in `ATTR_SSL_CAPATH`.
///
/// libmysqlclient accepts a CA file and an OpenSSL hashed CA directory. Rustls
/// accepts a multi-certificate PEM file instead, so concatenating the directory's
/// certificates preserves the same trust-anchor semantics without weakening
/// verification. Returns the rewritten SSL config plus an owner for the temporary
/// bundle when a directory was requested.
fn normalize_ssl_ca_sources(
    ssl_config: &str,
    ca_path: Option<&std::path::Path>,
) -> Result<(String, Option<TemporaryCaBundle>), String> {
    let Some(ca_path) = ca_path else {
        return Ok((ssl_config.to_string(), None));
    };
    let canonical = ca_path.canonicalize().map_err(|error| {
        format!(
            "Pdo\\Mysql::ATTR_SSL_CAPATH '{}': {error}",
            ca_path.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "Pdo\\Mysql::ATTR_SSL_CAPATH '{}' is not a directory",
            canonical.display()
        ));
    }

    let mut pem = Vec::new();
    for pair in ssl_config.split(';').filter(|pair| !pair.is_empty()) {
        if let Some(("ca", value)) = pair.split_once('=') {
            let bytes = fs::read(value)
                .map_err(|error| format!("Pdo\\Mysql::ATTR_SSL_CA '{value}': {error}"))?;
            append_pem_certificates(&mut pem, &bytes);
        }
    }

    let mut entries = fs::read_dir(&canonical)
        .map_err(|error| format!("Pdo\\Mysql::ATTR_SSL_CAPATH '{}': {error}", canonical.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Pdo\\Mysql::ATTR_SSL_CAPATH '{}': {error}", canonical.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let metadata = entry.metadata().map_err(|error| {
            format!(
                "Pdo\\Mysql::ATTR_SSL_CAPATH '{}': {error}",
                entry.path().display()
            )
        })?;
        // OpenSSL-style CA directories commonly contain c_rehash symlinks; the
        // selected directory is trusted configuration, so follow them to files.
        if !metadata.is_file() {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(|error| {
            format!(
                "Pdo\\Mysql::ATTR_SSL_CAPATH '{}': {error}",
                entry.path().display()
            )
        })?;
        append_pem_certificates(&mut pem, &bytes);
    }
    if pem.is_empty() {
        return Err(format!(
            "Pdo\\Mysql::ATTR_SSL_CAPATH '{}' contains no PEM certificates",
            canonical.display()
        ));
    }

    static NEXT_BUNDLE_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_BUNDLE_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "elephc-pdo-mysql-ca-{}-{id}.pem",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("cannot create MySQL CA bundle '{}': {error}", path.display()))?;
    file.write_all(&pem)
        .map_err(|error| format!("cannot write MySQL CA bundle '{}': {error}", path.display()))?;

    let rewritten = ssl_config
        .split(';')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| !matches!(pair.split_once('='), Some(("ca", _))))
        .fold(format!("ca={};", path.display()), |mut config, pair| {
            config.push_str(pair);
            config.push(';');
            config
        });
    Ok((rewritten, Some(TemporaryCaBundle { path })))
}

/// Appends a PEM source when it contains at least one certificate block.
fn append_pem_certificates(output: &mut Vec<u8>, source: &[u8]) {
    if !source
        .windows(b"-----BEGIN CERTIFICATE-----".len())
        .any(|window| window == b"-----BEGIN CERTIFICATE-----")
    {
        return;
    }
    output.extend_from_slice(source);
    if !source.ends_with(b"\n") {
        output.push(b'\n');
    }
}

/// A custom build without `mysql-tls` has no MySQL TLS backend linked. Rather than
/// silently downgrade a program that asked for TLS to a plaintext connection, a
/// non-empty SSL config fails loudly; an empty config (no TLS requested) connects
/// normally.
#[cfg(not(feature = "mysql-tls"))]
fn apply_ssl_opts(opts: OptsBuilder, ssl_config: &str) -> Result<OptsBuilder, String> {
    if ssl_config.is_empty() {
        return Ok(opts);
    }
    Err("mysql TLS (Pdo\\Mysql::ATTR_SSL_*) was requested but requires the \
         `mysql-tls` feature, which was not compiled in (rebuild elephc-pdo with \
         --features mysql-tls)"
        .to_string())
}

/// Installs the ring `CryptoProvider` as the process default exactly once. The
/// `mysql` crate builds its rustls `ClientConfig` with the provider-less
/// `ClientConfig::builder()`, which panics when more than one provider is present
/// unless a process default is installed. mysql 28's `rustls-tls-ring` feature
/// uses the same provider as pg / elephc-tls; installing it explicitly keeps the
/// choice deterministic when the final binary links other rustls users.
#[cfg(feature = "mysql-tls")]
fn install_crypto_provider() {
    use std::sync::Once;
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // Ignored on the (harmless) race where another path already installed one.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Parses the prelude's packed SSL config (`ca=…;cert=…;key=…;verify=0|1`) into
/// mysql `SslOpts`. `ca` is `MYSQL_ATTR_SSL_CA` (a server CA bundle to trust on
/// top of the bundled webpki roots); `cert`+`key` are `MYSQL_ATTR_SSL_CERT`/
/// `SSL_KEY` (client-certificate mutual TLS, honored only when both are present);
/// `verify=0` is `MYSQL_ATTR_SSL_VERIFY_SERVER_CERT` set false, which disables
/// certificate and hostname validation via the crate's danger flags. Unsupported
/// security keys never reach this parser: `parse_driver_options` rejects them first.
#[cfg(feature = "mysql-tls")]
fn parse_ssl_config(ssl_config: &str) -> mysql::SslOpts {
    use mysql::{ClientIdentity, SslOpts};
    use std::path::PathBuf;

    let mut ca: Option<String> = None;
    let mut cert: Option<String> = None;
    let mut key: Option<String> = None;
    let mut verify = true;
    for pair in ssl_config.split(';') {
        let Some((k, v)) = pair.trim().split_once('=') else {
            continue;
        };
        let v = v.trim().to_string();
        match k.trim() {
            "ca" => ca = Some(v),
            "cert" => cert = Some(v),
            "key" => key = Some(v),
            "verify" => verify = v != "0",
            _ => {}
        }
    }

    let mut ssl = SslOpts::default();
    if let Some(ca) = ca {
        ssl = ssl.with_root_cert_path(Some(PathBuf::from(ca)));
    }
    if let (Some(cert), Some(key)) = (cert, key) {
        ssl = ssl.with_client_identity(Some(ClientIdentity::new(
            PathBuf::from(cert),
            PathBuf::from(key),
        )));
    }
    if !verify {
        ssl = ssl
            .with_danger_skip_domain_validation(true)
            .with_danger_accept_invalid_certs(true);
    }
    ssl
}

impl MyConn {
    /// Connects to MySQL/MariaDB for a `mysql:` DSN. `init_command` (P1-9), when
    /// non-empty, is one SQL statement run by the server immediately after
    /// authentication on every (re)connect — the bridge-level minimal wiring for
    /// `Pdo\Mysql::ATTR_INIT_COMMAND` (Doctrine/Laravel commonly set `SET NAMES
    /// utf8mb4` or a `sql_mode` here). It travels as its own parameter rather than
    /// a DSN `key=value` pair because the DSN parser splits on `;`, which a
    /// realistic init command (e.g. two statements) could contain. A DSN
    /// `charset=` key (P2-3) becomes its own `SET NAMES <charset>` statement, run
    /// before `init_command` so an explicit `ATTR_INIT_COMMAND` can still issue
    /// its own `SET NAMES`/`sql_mode` afterwards in the same session.
    ///
    /// `ssl_config` is the prelude's packed serialization of the
    /// `Pdo\Mysql::ATTR_SSL_*` constructor options (`ca=…;cert=…;key=…;verify=…`);
    /// an empty string means no TLS. It is honored when the default `mysql-tls`
    /// feature is compiled in (see [`apply_ssl_opts`]).
    ///
    /// `found_rows` is the `Pdo\Mysql::ATTR_FOUND_ROWS` constructor option
    /// (F-MY-06): it adds `CLIENT_FOUND_ROWS` to the capabilities negotiated in the
    /// handshake, so an UPDATE's `rowCount()` reports the rows its WHERE clause
    /// MATCHED rather than the rows it actually CHANGED. It can only be applied at
    /// connect time (see [`build_opts`]). Returns the connection or an error message
    /// for `last_open_error`.
    pub fn open(
        dsn: &str,
        init_command: &str,
        ssl_config: &str,
        found_rows: bool,
        driver_config: &str,
    ) -> Result<MyConn, String> {
        let driver_options = parse_driver_options(driver_config)?;
        let (mut opts, charset) =
            build_opts(dsn, found_rows, driver_options.ignore_space)?;
        let (ssl_config, _ca_bundle) =
            normalize_ssl_ca_sources(ssl_config, driver_options.ssl_ca_path.as_deref())?;
        opts = apply_ssl_opts(opts, &ssl_config)?;
        if driver_options.compress {
            opts = opts.compress(Some(mysql::Compression::default()));
        }
        opts = opts.local_infile_handler(Some(local_infile_handler(
            driver_options.local_infile,
            driver_options.local_infile_directory.clone(),
        )));
        let mut init_statements: Vec<String> = Vec::new();
        if let Some(cs) = charset {
            init_statements.push(format!("SET NAMES {cs}"));
        }
        if !init_command.is_empty() {
            init_statements.push(init_command.to_string());
        }
        if !init_statements.is_empty() {
            opts = opts.init(init_statements);
        }
        let resolved_opts: mysql::Opts = opts.clone().into();
        let host_info = match resolved_opts.get_socket() {
            Some(_) => "Localhost via UNIX socket".to_string(),
            None => format!("{} via TCP/IP", resolved_opts.get_ip_or_hostname()),
        };
        let conn = Conn::new(opts).map_err(|e| e.to_string())?;
        let server_version = conn.server_version();
        let no_backslash_escapes = conn.no_backslash_escape();
        Ok(MyConn {
            conn: MyClientSlot(Some(conn)),
            host_info,
            changes: 0,
            errmsg: String::new(),
            errcode: 0,
            sqlstate: "00000".to_string(),
            last_id: 0,
            autocommit: true,
            fetch_table_names: false,
            buffered_query: driver_options.buffered_query,
            multi_statements: driver_options.multi_statements,
            unbuffered_active: false,
            warning_count: 0,
            in_transaction: false,
            server_version,
            no_backslash_escapes,
            active_stream: None,
            next_stream_id: 0,
        })
    }

    /// Changes the default buffering mode used by statements prepared after this
    /// call, matching `Pdo\Mysql::ATTR_USE_BUFFERED_QUERY`'s connection attribute.
    pub fn set_buffered_query(&mut self, buffered: bool) -> i64 {
        self.buffered_query = buffered;
        1
    }

    /// Returns the current `ATTR_USE_BUFFERED_QUERY` default.
    pub fn buffered_query(&self) -> i64 {
        self.buffered_query as i64
    }

    /// Records mysqlnd's 2014/HY000 connection-busy diagnostic and returns false
    /// while an unbuffered statement still owns unread rows.
    fn ensure_not_busy(&mut self) -> bool {
        if !self.unbuffered_active {
            return true;
        }
        self.sqlstate = "HY000".to_string();
        self.errcode = 2014;
        self.errmsg = "Cannot execute queries while other unbuffered queries are active. Consider using PDOStatement::fetchAll(). Alternatively, if your code is only ever going to run against mysql, you may enable query buffering by setting the PDO::MYSQL_ATTR_USE_BUFFERED_QUERY attribute.".to_string();
        false
    }

    /// Restores a worker-owned client and refreshes connection properties that
    /// may have changed while its statement ran.
    fn restore_stream_client(&mut self, conn: Conn, warnings: u16) {
        self.server_version = conn.server_version();
        self.no_backslash_escapes = conn.no_backslash_escape();
        self.warning_count = warnings;
        self.conn.0 = Some(conn);
    }

    /// Stops the active worker and recovers its client for close/reset paths.
    fn finish_active_stream(&mut self) {
        let Some(mut active) = self.active_stream.take() else {
            return;
        };
        let _ = active.commands.send(MyStreamCommand::Close);
        while let Ok(response) = active.responses.recv() {
            match response {
                MyStreamResponse::Finished(conn, warnings) => {
                    self.restore_stream_client(conn, warnings);
                    break;
                }
                MyStreamResponse::Failed(conn, sqlstate, errcode, message) => {
                    let warnings = conn.warnings();
                    self.restore_stream_client(conn, warnings);
                    self.sqlstate = sqlstate;
                    self.errcode = errcode;
                    self.errmsg = message;
                    break;
                }
                MyStreamResponse::Rowset(_)
                | MyStreamResponse::Row(_)
                | MyStreamResponse::RowsetEnd => {}
            }
        }
        if let Some(worker) = active.worker.take() {
            let _ = worker.join();
        }
        self.unbuffered_active = false;
    }

    /// Finishes the active stream only when it belongs to `id`.
    fn finish_stream(&mut self, id: u64) {
        if self.active_stream.as_ref().map(|stream| stream.id) == Some(id) {
            self.finish_active_stream();
        }
    }

    /// Installs a newly spawned worker and waits for its first result-set metadata.
    fn activate_stream(
        &mut self,
        commands: mpsc::Sender<MyStreamCommand>,
        responses: mpsc::Receiver<MyStreamResponse>,
        worker: JoinHandle<()>,
    ) -> Result<(u64, MyRowset), i64> {
        self.next_stream_id = self.next_stream_id.wrapping_add(1).max(1);
        let id = self.next_stream_id;
        let mut active = MyActiveStream {
            id,
            commands,
            responses,
            worker: Some(worker),
        };
        match active.responses.recv() {
            Ok(MyStreamResponse::Rowset(rowset)) => {
                self.active_stream = Some(active);
                self.unbuffered_active = true;
                Ok((id, rowset))
            }
            Ok(MyStreamResponse::Finished(conn, warnings)) => {
                self.restore_stream_client(conn, warnings);
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                Err(-1)
            }
            Ok(MyStreamResponse::Failed(conn, sqlstate, errcode, message)) => {
                let warnings = conn.warnings();
                self.restore_stream_client(conn, warnings);
                self.sqlstate = sqlstate;
                self.errcode = errcode;
                self.errmsg = message;
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                Err(-1)
            }
            Ok(MyStreamResponse::Row(_) | MyStreamResponse::RowsetEnd) | Err(_) => {
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                self.sqlstate = "HY000".to_string();
                self.errcode = 1;
                self.errmsg = "MySQL unbuffered query worker terminated unexpectedly".to_string();
                Err(-1)
            }
        }
    }

    /// Starts an unbuffered prepared-statement worker.
    fn start_native_stream(
        &mut self,
        statement: Statement,
        values: Vec<Value>,
    ) -> Result<(u64, MyRowset), i64> {
        let Some(conn) = self.conn.0.take() else {
            return Err(-1);
        };
        let fetch_table_names = self.fetch_table_names;
        let (command_tx, command_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_mysql_native_stream(
                conn,
                statement,
                values,
                fetch_table_names,
                command_rx,
                response_tx,
            );
        });
        self.activate_stream(command_tx, response_rx, worker)
    }

    /// Starts an unbuffered text-protocol worker for emulated prepares.
    fn start_text_stream(&mut self, sql: String) -> Result<(u64, MyRowset), i64> {
        let Some(conn) = self.conn.0.take() else {
            return Err(-1);
        };
        let fetch_table_names = self.fetch_table_names;
        let (command_tx, command_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_mysql_text_stream(
                conn,
                sql,
                fetch_table_names,
                command_rx,
                response_tx,
            );
        });
        self.activate_stream(command_tx, response_rx, worker)
    }

    /// Requests one row from the active result set.
    fn next_stream_row(&mut self, id: u64) -> Result<Option<Vec<Cell>>, i64> {
        let Some(active) = self.active_stream.as_mut() else {
            return Ok(None);
        };
        if active.id != id {
            return Ok(None);
        }
        if active.commands.send(MyStreamCommand::Next).is_err() {
            return Err(-1);
        }
        match active.responses.recv() {
            Ok(MyStreamResponse::Row(row)) => Ok(Some(row)),
            Ok(MyStreamResponse::RowsetEnd) => Ok(None),
            Ok(MyStreamResponse::Failed(conn, sqlstate, errcode, message)) => {
                let warnings = conn.warnings();
                self.restore_stream_client(conn, warnings);
                self.sqlstate = sqlstate;
                self.errcode = errcode;
                self.errmsg = message;
                let mut active = self.active_stream.take().expect("active stream disappeared");
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                self.unbuffered_active = false;
                Err(-1)
            }
            Ok(MyStreamResponse::Finished(conn, warnings)) => {
                self.restore_stream_client(conn, warnings);
                let mut active = self.active_stream.take().expect("active stream disappeared");
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                self.unbuffered_active = false;
                Ok(None)
            }
            Ok(MyStreamResponse::Rowset(_)) | Err(_) => Err(-1),
        }
    }

    /// Activates the next protocol result set, or recovers the connection at EOF.
    fn next_stream_rowset(&mut self, id: u64) -> Result<Option<MyRowset>, i64> {
        let Some(active) = self.active_stream.as_mut() else {
            return Ok(None);
        };
        if active.id != id {
            return Ok(None);
        }
        match active.responses.recv() {
            Ok(MyStreamResponse::Rowset(rowset)) => Ok(Some(rowset)),
            Ok(MyStreamResponse::Finished(conn, warnings)) => {
                self.restore_stream_client(conn, warnings);
                let mut active = self.active_stream.take().expect("active stream disappeared");
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                self.unbuffered_active = false;
                Ok(None)
            }
            Ok(MyStreamResponse::Failed(conn, sqlstate, errcode, message)) => {
                let warnings = conn.warnings();
                self.restore_stream_client(conn, warnings);
                self.sqlstate = sqlstate;
                self.errcode = errcode;
                self.errmsg = message;
                let mut active = self.active_stream.take().expect("active stream disappeared");
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                self.unbuffered_active = false;
                Err(-1)
            }
            Ok(MyStreamResponse::Row(_) | MyStreamResponse::RowsetEnd) | Err(_) => Err(-1),
        }
    }

    /// Records the AUTO_INCREMENT id from the just-run statement when it is
    /// non-zero, so `lastInsertId()` survives an intervening non-INSERT query.
    /// Stored without a lossy cast (P2-2's sibling gap) so a `BIGINT UNSIGNED`
    /// AUTO_INCREMENT id above `i64::MAX` still round-trips through
    /// `last_insert_id_text`.
    fn note_last_id(&mut self, id: Option<u64>) {
        if let Some(id) = id {
            if id != 0 {
                self.last_id = id;
            }
        }
    }

    /// Updates transaction bookkeeping from a successfully executed SQL command.
    fn note_transaction_sql(&mut self, sql: &str) {
        self.in_transaction = transaction_state_after_sql(sql, self.in_transaction, self.autocommit);
    }

    /// Runs a statement with no result rows (`PDO::exec`), returning the affected
    /// row count or `-1` on error.
    pub fn exec(&mut self, sql: &str) -> i64 {
        if !self.ensure_not_busy() {
            return -1;
        }
        let no_backslash_escape = self.no_backslash_escape();
        if !self.multi_statements && sql_has_multiple_statements(sql, no_backslash_escape) {
            self.sqlstate = "42000".to_string();
            self.errcode = 1064;
            self.errmsg = "Multiple statements are disabled for this connection".to_string();
            return -1;
        }
        // Collect the outcome into owned values first so the `&mut self.conn`
        // borrow held by the query result ends before the connection bookkeeping
        // fields are written below.
        let outcome: Result<(i64, Option<u64>), mysql::Error> = match self.conn.query_iter(sql) {
            Ok(mut res) => {
                // P0-B: `last_insert_id`/`affected_rows` read the CURRENT result
                // set's OK packet, which only exists while the state machine is
                // still on that set. The first `.next()` call below on an
                // empty-result (DDL/DML) query advances the state straight to
                // `Done` (no OK packet), so `affected_rows()` read AFTER the
                // drain loop always returns 0. Capture both here, immediately
                // after the query succeeds and before draining, so the real
                // counts survive the state transition (verified live on
                // MariaDB 11: reading before the drain gives the correct count,
                // after gives 0).
                let last = res.last_insert_id();
                let affected = res.affected_rows() as i64;
                // Drain any result set (a SELECT run through exec() has rows; DDL
                // and DML have none) so the connection is ready for the next call.
                for row in res.by_ref() {
                    if row.is_err() {
                        break;
                    }
                }
                Ok((affected, last))
            }
            Err(e) => Err(e),
        };
        let warnings = self.conn.warnings();
        match outcome {
            Ok((affected, last)) => {
                self.note_last_id(last);
                self.changes = affected;
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                self.warning_count = warnings;
                self.note_transaction_sql(sql);
                affected
            }
            Err(e) => {
                self.sqlstate = err_sqlstate(&e);
                self.errmsg = e.to_string();
                self.errcode = err_code(&e);
                -1
            }
        }
    }

    /// Runs a bare transaction-control statement, returning `1`/`0`.
    pub fn exec_simple(&mut self, sql: &str) -> i64 {
        if !self.ensure_not_busy() {
            return 0;
        }
        match self.conn.query_drop(sql) {
            Ok(()) => {
                self.note_transaction_sql(sql);
                1
            }
            Err(e) => {
                self.sqlstate = err_sqlstate(&e);
                self.errmsg = e.to_string();
                self.errcode = err_code(&e);
                0
            }
        }
    }

    /// Enables or disables MySQL session autocommit. An unchanged value is a
    /// successful no-op; a server error leaves the stored state unchanged.
    pub fn set_autocommit(&mut self, enabled: bool) -> i64 {
        if !self.ensure_not_busy() {
            return 0;
        }
        if self.autocommit == enabled {
            return 1;
        }
        let sql = if enabled {
            "SET autocommit=1"
        } else {
            "SET autocommit=0"
        };
        match self.conn.query_drop(sql) {
            Ok(()) => {
                self.autocommit = enabled;
                if enabled {
                    self.in_transaction = false;
                }
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                1
            }
            Err(error) => {
                self.sqlstate = err_sqlstate(&error);
                self.errmsg = error.to_string();
                self.errcode = err_code(&error);
                0
            }
        }
    }

    /// Returns the last inserted AUTO_INCREMENT id. MySQL ignores the sequence
    /// name argument (it is a PostgreSQL/Oracle concept). Matches the bridge's
    /// `i64` ABI (`elephc_pdo_last_insert_id`): a `BIGINT UNSIGNED` id above
    /// `i64::MAX` still wraps through this accessor — `last_insert_id_text` is
    /// the precision-preserving one (mirroring PostgreSQL's own text accessor).
    pub fn last_insert_id(&self, _name: Option<&str>) -> i64 {
        self.last_id as i64
    }

    /// Like `last_insert_id`, but renders the id as decimal text without the
    /// lossy `i64` cast (P2-2's sibling gap), so a `BIGINT UNSIGNED`
    /// AUTO_INCREMENT id above `i64::MAX` round-trips exactly. MySQL ignores the
    /// sequence name argument, matching `last_insert_id`.
    pub fn last_insert_id_text(&self, _name: Option<&str>) -> String {
        self.last_id.to_string()
    }

    /// Returns the MySQL/MariaDB server's reported version (`MAJOR.MINOR.PATCH`),
    /// parsed from the handshake by the `mysql` client.
    pub fn server_version(&self) -> String {
        let (major, minor, patch) = self.server_version;
        format!("{major}.{minor}.{patch}")
    }

    /// Returns the pure-Rust MySQL client implementation and its pinned crate
    /// version. Unlike php-src there is no mysqlnd/libmysql client library in the
    /// standalone binary, so reporting the linked client crate is the truthful
    /// equivalent of `mysql_get_client_info()`.
    pub fn client_version(&self) -> String {
        "mysql 28.0.0".to_string()
    }

    /// Returns the connection transport description in the same shape as
    /// php-src's `mysql_get_host_info()` result.
    pub fn connection_status(&self) -> String {
        self.host_info.clone()
    }

    /// Pings an idle client; an active row worker already proves the connection
    /// is live enough to remain a valid persistent handle.
    pub fn is_alive(&mut self) -> bool {
        self.active_stream.is_some() || self.conn.ping().is_ok()
    }

    /// Updates the live `PDO::ATTR_FETCH_TABLE_NAMES` setting.
    pub fn set_fetch_table_names(&mut self, enabled: bool) {
        self.fetch_table_names = enabled;
    }

    /// Reconstructs MySQL's `COM_STATISTICS` text from live `SHOW STATUS` values.
    /// The Rust client does not expose the protocol command, but the same server
    /// counters are available without relying on fabricated constants.
    pub fn server_info(&mut self) -> String {
        if !self.ensure_not_busy() {
            return String::new();
        }
        let rows: Vec<(String, String)> = match self.conn.query("SHOW STATUS") {
            Ok(rows) => rows,
            Err(_) => return String::new(),
        };
        let values: HashMap<String, String> = rows.into_iter().collect();
        let value = |name: &str| values.get(name).map(String::as_str).unwrap_or("0");
        let uptime = value("Uptime").parse::<f64>().unwrap_or(0.0);
        let questions = value("Questions").parse::<f64>().unwrap_or(0.0);
        let queries_per_second = if uptime > 0.0 {
            questions / uptime
        } else {
            0.0
        };
        format!(
            "Uptime: {}  Threads: {}  Questions: {}  Slow queries: {}  Opens: {}  Flush tables: {}  Open tables: {}  Queries per second avg: {:.3}",
            value("Uptime"),
            value("Threads_connected"),
            value("Questions"),
            value("Slow_queries"),
            value("Opened_tables"),
            value("Flush_commands"),
            value("Open_tables"),
            queries_per_second,
        )
    }

    /// Returns the warning count captured from the final OK/EOF packet of the last
    /// completed operation, including SELECT and prepared-statement results.
    pub fn warning_count(&self) -> i64 {
        self.warning_count as i64
    }

    /// Returns whether the connection's current session has `NO_BACKSLASH_ESCAPES`
    /// active in its `sql_mode` (backslash is then a literal character in a string
    /// literal, so backslash-escaping a quoted value is unsafe there —
    /// `PDO::quote()`'s MySQL branch falls back to `''`-doubling only in that case,
    /// P1-f). The `mysql` crate already tracks this from the connection's session
    /// state (`Conn::no_backslash_escape`), so no extra query is needed here.
    pub fn no_backslash_escape(&self) -> bool {
        self.conn
            .0
            .as_ref()
            .map(Conn::no_backslash_escape)
            .unwrap_or(self.no_backslash_escapes)
    }

    /// Prepares a statement: translates placeholders and prepares it server-side
    /// for column metadata. Returns the statement or an error message. Rejects a
    /// SQL text that mixes a positional `?` with a named `:name` placeholder
    /// with `HY093` before ever asking the server to prepare it — PDO forbids
    /// combining the two styles in one statement, and MySQL's own placeholder
    /// syntax (a bare `?`) has no way to catch this itself.
    ///
    /// F-MY-03: the placeholder scan is handed this connection's LIVE
    /// `NO_BACKSLASH_ESCAPES` `sql_mode` (the same session state
    /// [`MyConn::no_backslash_escape`] reports to `PDO::quote()`), because that
    /// mode changes where the SERVER thinks a `'…'`/`"…"` literal ends — and a
    /// scanner that disagrees with the server about that disagrees with it about
    /// how many placeholders the statement has. This is the only place the flag can
    /// be read: `translate_placeholders` is a free function with no connection.
    pub fn prepare(&mut self, sql: &str, emulated: bool) -> Result<MyStmt, String> {
        if !self.ensure_not_busy() {
            return Err(self.errmsg.clone());
        }
        let no_backslash_escape = self.no_backslash_escape();
        if !self.multi_statements && sql_has_multiple_statements(sql, no_backslash_escape) {
            self.sqlstate = "42000".to_string();
            self.errcode = 1064;
            self.errmsg = "Multiple statements are disabled for this connection".to_string();
            return Err(self.errmsg.clone());
        }
        let (translated, named_map, order, mixed) =
            translate_placeholders(sql, no_backslash_escape);
        if mixed {
            // Nonzero native code like every other error path here, and `1`
            // specifically to match pg's identical HY093 branch (`pg.rs`,
            // `PgConn::prepare`): the same logical error must not report
            // `errorInfo()[1] == 0` on MySQL and `1` on PostgreSQL.
            self.errcode = 1;
            self.sqlstate = "HY093".to_string();
            self.errmsg =
                "Invalid parameter number: mixed named and positional parameters".to_string();
            return Err(self.errmsg.clone());
        }
        let n_binds = order.iter().copied().max().unwrap_or(0) as usize;
        if emulated {
            self.errcode = 0;
            self.sqlstate = "00000".to_string();
            return Ok(MyStmt {
                conn_id: 0,
                query_string: sql.to_string(),
                statement: None,
                emulated_sql: Some(translated),
                no_backslash_escape: self.no_backslash_escape(),
                sent_sql: String::new(),
                named_map,
                order,
                binds: vec![Bind::Null; n_binds],
                bound: vec![false; n_binds],
                col_names: Vec::new(),
                col_kinds: Vec::new(),
                col_types: Vec::new(),
                col_tables: Vec::new(),
                col_flags: Vec::new(),
                col_lengths: Vec::new(),
                col_precisions: Vec::new(),
                rows: Vec::new(),
                remaining_rowsets: Vec::new(),
                cursor: -1,
                executed: false,
                is_call: sql_is_call_statement(sql),
                buffered: self.buffered_query,
                stream_id: None,
            });
        }
        match self.conn.prep(&translated) {
            Ok(statement) => {
                let col_names = statement
                    .columns()
                    .iter()
                    .map(|column| column_display_name(column, self.fetch_table_names))
                    .collect();
                let col_kinds = statement
                    .columns()
                    .iter()
                    .map(ColKind::from_column)
                    .collect();
                // F-MY-08: the raw wire type, kept beside the coarser `ColKind` so
                // `getColumnMeta`'s `native_type` can report MySQL's own type name.
                let col_types = statement
                    .columns()
                    .iter()
                    .map(|c| c.column_type())
                    .collect();
                let col_tables = statement
                    .columns()
                    .iter()
                    .map(|column| column.table_str().into_owned())
                    .collect();
                let col_flags = statement
                    .columns()
                    .iter()
                    .map(Column::flags)
                    .collect();
                let col_lengths = statement
                    .columns()
                    .iter()
                    .map(Column::column_length)
                    .collect();
                let col_precisions = statement
                    .columns()
                    .iter()
                    .map(Column::decimals)
                    .collect();
                // Distinct slots run 1..=N contiguously, so the highest slot in
                // `order` is the bound-value count.
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                Ok(MyStmt {
                    conn_id: 0,
                    query_string: sql.to_string(),
                    statement: Some(statement),
                    emulated_sql: None,
                    no_backslash_escape: self.no_backslash_escape(),
                    sent_sql: String::new(),
                    named_map,
                    order,
                    binds: vec![Bind::Null; n_binds],
                    bound: vec![false; n_binds],
                    col_names,
                    col_kinds,
                    col_types,
                    col_tables,
                    col_flags,
                    col_lengths,
                    col_precisions,
                    rows: Vec::new(),
                    remaining_rowsets: Vec::new(),
                    cursor: -1,
                    executed: false,
                    is_call: sql_is_call_statement(sql),
                    buffered: self.buffered_query,
                    stream_id: None,
                })
            }
            Err(e) => {
                self.sqlstate = err_sqlstate(&e);
                self.errmsg = e.to_string();
                self.errcode = err_code(&e);
                Err(e.to_string())
            }
        }
    }
}

/// Derives MySQL's transaction state after one successful command. Explicit
/// control statements win; DDL implicitly commits; with autocommit disabled a
/// regular statement starts the session transaction.
pub(crate) fn transaction_state_after_sql(
    sql: &str,
    current: bool,
    autocommit: bool,
) -> bool {
    let normalized = sql.trim_start().to_ascii_uppercase();
    if normalized.starts_with("BEGIN") || normalized.starts_with("START TRANSACTION") {
        return true;
    }
    if normalized.starts_with("COMMIT") || normalized.starts_with("END") {
        return normalized.contains("AND CHAIN");
    }
    if normalized.starts_with("ROLLBACK") {
        if normalized.starts_with("ROLLBACK TO") {
            return current;
        }
        return normalized.contains("AND CHAIN");
    }
    if ["ALTER", "CREATE", "DROP", "GRANT", "LOCK", "RENAME", "REVOKE", "TRUNCATE", "UNLOCK"]
        .iter()
        .any(|keyword| normalized.starts_with(keyword))
    {
        return false;
    }
    if autocommit { current } else { true }
}

/// Renders a MySQL `DATE`/`DATETIME`/`TIMESTAMP` value as its canonical text:
/// date-only columns drop the time, others keep `H:M:S` (with a fractional part
/// only when present), matching the server's string output.
fn format_date(kind: ColKind, y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8, us: u32) -> String {
    if let ColKind::Date = kind {
        return format!("{:04}-{:02}-{:02}", y, mo, d);
    }
    let base = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, mi, s);
    if us > 0 {
        format!("{}.{:06}", base, us)
    } else {
        base
    }
}

/// Renders a MySQL `TIME` value as `[-]HH:MM:SS[.ffffff]`, where the hour field
/// rolls in any whole days (`TIME` spans beyond 24h), matching the server's text.
fn format_time(neg: bool, days: u32, h: u8, mi: u8, s: u8, us: u32) -> String {
    let hours = days * 24 + h as u32;
    let sign = if neg { "-" } else { "" };
    let base = format!("{}{:02}:{:02}:{:02}", sign, hours, mi, s);
    if us > 0 {
        format!("{}.{:06}", base, us)
    } else {
        base
    }
}

/// Decodes one `mysql::Value` (with its column kind, for temporal rendering) into
/// a PHP-friendly `Cell` scalar.
fn decode_value(v: Value, kind: ColKind) -> Cell {
    match v {
        Value::NULL => Cell::Null,
        Value::Int(i) => Cell::Int(i),
        // BIGINT UNSIGNED (P2-2): a value above `i64::MAX` would wrap negative
        // through a plain `as i64` cast, silently corrupting the column. PHP's
        // pdo_mysql/mysqlnd matches this numeric-string fallback for any integer
        // too large for a native `zend_long`.
        Value::UInt(u) => {
            if u > i64::MAX as u64 {
                Cell::Text(u.to_string())
            } else {
                Cell::Int(u as i64)
            }
        }
        Value::Float(f) => Cell::Float(f as f64),
        Value::Double(d) => Cell::Float(d),
        // Text, VARCHAR, DECIMAL, BLOB, etc. all arrive as raw bytes.
        Value::Bytes(b) => {
            if matches!(kind, ColKind::Binary) {
                Cell::Bytes(b)
            } else {
                Cell::Text(String::from_utf8_lossy(&b).into_owned())
            }
        }
        Value::Date(y, mo, d, h, mi, s, us) => {
            Cell::Text(format_date(kind, y, mo, d, h, mi, s, us))
        }
        Value::Time(neg, days, h, mi, s, us) => Cell::Text(format_time(neg, days, h, mi, s, us)),
    }
}

/// Decodes a whole result row's values into `Cell`s, pairing each with its
/// column's kind for temporal text rendering.
fn decode_row(values: Vec<Value>, kinds: &[ColKind]) -> Vec<Cell> {
    values
        .into_iter()
        .enumerate()
        .map(|(i, v)| decode_value(v, kinds.get(i).copied().unwrap_or(ColKind::Other)))
        .collect()
}

/// Builds an empty rowset carrying the current MySQL protocol set's metadata.
fn mysql_stream_rowset(
    columns: &[Column],
    affected: i64,
    last_id: Option<u64>,
    fetch_table_names: bool,
) -> MyRowset {
    MyRowset {
        affected,
        last_id,
        col_names: columns
            .iter()
            .map(|column| column_display_name(column, fetch_table_names))
            .collect(),
        col_kinds: columns.iter().map(ColKind::from_column).collect(),
        col_types: columns.iter().map(Column::column_type).collect(),
        col_tables: columns
            .iter()
            .map(|column| column.table_str().into_owned())
            .collect(),
        col_flags: columns.iter().map(Column::flags).collect(),
        col_lengths: columns.iter().map(Column::column_length).collect(),
        col_precisions: columns.iter().map(Column::decimals).collect(),
        rows: Vec::new(),
    }
}

/// Drives an already-started MySQL query one row per `Next` command, preserving
/// protocol result-set boundaries for `PDOStatement::nextRowset()`.
fn drive_mysql_stream<T: mysql::prelude::Protocol>(
    mut result: QueryResult<'_, '_, '_, T>,
    fetch_table_names: bool,
    commands: &mpsc::Receiver<MyStreamCommand>,
    responses: &mpsc::Sender<MyStreamResponse>,
) -> Result<(), mysql::Error> {
    while let Some(mut set) = result.iter() {
        let columns = set.columns();
        let columns: &[Column] = columns.as_ref();
        let kinds = columns.iter().map(ColKind::from_column).collect::<Vec<_>>();
        let rowset = mysql_stream_rowset(
            columns,
            set.affected_rows() as i64,
            set.last_insert_id(),
            fetch_table_names,
        );
        if responses.send(MyStreamResponse::Rowset(rowset)).is_err() {
            return Ok(());
        }
        while let Ok(command) = commands.recv() {
            match command {
                MyStreamCommand::Next => match set.next() {
                    Some(row) => {
                        let decoded = decode_row(row?.unwrap(), &kinds);
                        if responses.send(MyStreamResponse::Row(decoded)).is_err() {
                            return Ok(());
                        }
                    }
                    None => {
                        let _ = responses.send(MyStreamResponse::RowsetEnd);
                        break;
                    }
                },
                MyStreamCommand::Close => return Ok(()),
            }
        }
    }
    Ok(())
}

/// Runs a prepared MySQL query on an owned client and returns that client when
/// demand-driven iteration finishes.
fn run_mysql_native_stream(
    mut conn: Conn,
    statement: Statement,
    values: Vec<Value>,
    fetch_table_names: bool,
    commands: mpsc::Receiver<MyStreamCommand>,
    responses: mpsc::Sender<MyStreamResponse>,
) {
    let result = match conn.exec_iter(&statement, values) {
        Ok(result) => drive_mysql_stream(result, fetch_table_names, &commands, &responses),
        Err(error) => Err(error),
    };
    finish_mysql_stream_worker(conn, result, &responses);
}

/// Runs an emulated/text-protocol MySQL query on an owned client.
fn run_mysql_text_stream(
    mut conn: Conn,
    sql: String,
    fetch_table_names: bool,
    commands: mpsc::Receiver<MyStreamCommand>,
    responses: mpsc::Sender<MyStreamResponse>,
) {
    let result = match conn.query_iter(sql) {
        Ok(result) => drive_mysql_stream(result, fetch_table_names, &commands, &responses),
        Err(error) => Err(error),
    };
    finish_mysql_stream_worker(conn, result, &responses);
}

/// Sends a worker's recovered connection plus final warning/error state.
fn finish_mysql_stream_worker(
    conn: Conn,
    result: Result<(), mysql::Error>,
    responses: &mpsc::Sender<MyStreamResponse>,
) {
    match result {
        Ok(()) => {
            let warnings = conn.warnings();
            let _ = responses.send(MyStreamResponse::Finished(conn, warnings));
        }
        Err(error) => {
            let sqlstate = err_sqlstate(&error);
            let errcode = err_code(&error);
            let message = error.to_string();
            let _ = responses.send(MyStreamResponse::Failed(
                conn, sqlstate, errcode, message,
            ));
        }
    }
}

impl MyStmt {
    /// Resolves a named placeholder to its 1-based slot (0 if unknown). The
    /// leading colon is optional.
    pub fn bind_parameter_index(&self, name: &str) -> i64 {
        let bare = name.strip_prefix(':').unwrap_or(name);
        self.named_map.get(bare).copied().unwrap_or(0)
    }

    /// Stores a bound value at the 1-based slot `idx`. Returns `1`/`0`.
    pub fn bind(&mut self, idx: i64, value: Bind) -> i64 {
        if idx < 1 || (idx as usize) > self.binds.len() {
            return 0;
        }
        self.binds[(idx - 1) as usize] = value;
        self.bound[(idx - 1) as usize] = true;
        1
    }

    /// Resets the cursor and execution state, keeping the bound values.
    pub fn reset(&mut self, conn: &mut MyConn) -> i64 {
        if let Some(stream_id) = self.stream_id {
            conn.finish_stream(stream_id);
        }
        self.cursor = -1;
        self.executed = false;
        self.rows.clear();
        self.remaining_rowsets.clear();
        self.stream_id = None;
        1
    }

    /// Makes one materialized result set active and updates connection-level
    /// row-count/insert-id state to match it.
    fn install_rowset(&mut self, conn: &mut MyConn, rowset: MyRowset) {
        conn.changes = if !self.buffered && !rowset.col_names.is_empty() {
            0
        } else {
            rowset.row_count()
        };
        conn.note_last_id(rowset.last_id);
        self.col_names = rowset.col_names;
        self.col_kinds = rowset.col_kinds;
        self.col_types = rowset.col_types;
        self.col_tables = rowset.col_tables;
        self.col_flags = rowset.col_flags;
        self.col_lengths = rowset.col_lengths;
        self.col_precisions = rowset.col_precisions;
        self.rows = rowset.rows;
        self.cursor = -1;
        self.executed = true;
        conn.unbuffered_active = !self.buffered
            && (self.stream_id.is_some() || !self.rows.is_empty());
    }

    /// Advances to the next MySQL result set, discarding unread streamed rows as
    /// mysqlnd does. Returns `1` when a rowset became active and `0` at protocol EOF.
    pub fn next_rowset(&mut self, conn: &mut MyConn) -> i64 {
        if let Some(stream_id) = self.stream_id {
            if self.remaining_rowsets.is_empty() {
                loop {
                    match conn.next_stream_row(stream_id) {
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(code) => return code,
                    }
                }
                match conn.next_stream_rowset(stream_id) {
                    Ok(Some(rowset)) => self.remaining_rowsets.push(rowset),
                    Ok(None) => self.stream_id = None,
                    Err(code) => return code,
                }
            }
            self.rows.clear();
            self.cursor = -1;
        }
        if self.remaining_rowsets.is_empty() {
            if self.stream_id.is_none() {
                conn.unbuffered_active = false;
            }
            return 0;
        }
        let rowset = self.remaining_rowsets.remove(0);
        self.install_rowset(conn, rowset);
        1
    }

    /// Clears all bound values back to NULL.
    pub fn clear_bindings(&mut self) -> i64 {
        for (b, bound) in self.binds.iter_mut().zip(self.bound.iter_mut()) {
            *b = Bind::Null;
            *bound = false;
        }
        1
    }

    /// Builds the positional `mysql::Value` list (one per `?`, following `order`)
    /// for the prepared execution.
    fn build_values(&self) -> Vec<Value> {
        self.order
            .iter()
            .map(|&slot| match &self.binds[(slot - 1) as usize] {
                Bind::Null => Value::NULL,
                Bind::Int(v) => Value::Int(*v),
                Bind::Float(v) => Value::Double(*v),
                Bind::Text(s) => Value::Bytes(s.clone().into_bytes()),
                Bind::NationalText(s) => Value::Bytes(s.clone().into_bytes()),
                Bind::Bytes(b) => Value::Bytes(b.clone()),
            })
            .collect()
    }

    /// Builds one flag per emitted positional marker, identifying national-text
    /// values for MySQL's emulated `N'…'` literal syntax.
    fn build_national_flags(&self) -> Vec<bool> {
        self.order
            .iter()
            .map(|&slot| matches!(&self.binds[(slot - 1) as usize], Bind::NationalText(_)))
            .collect()
    }

    /// Executes the query once, either buffering decoded rows or starting the
    /// demand worker. Records row count and last insert id on the connection.
    ///
    /// P0-C: row materialization is decided from the EXECUTED result's live
    /// column metadata (`res.columns()`), not from `self.col_kinds` captured at
    /// PREPARE time. MySQL's `COM_STMT_PREPARE` reports zero columns for a
    /// `CALL proc()` statement — the result shape is only known once the
    /// procedure actually runs — so gating on the prepare-time count made
    /// `is_select` false for every `CALL`, silently dropping the procedure's
    /// rows off the wire. `res.columns()` must be read here, before the drain
    /// loop below: the crate's `QueryResult` is a state machine, and once the
    /// current result set's rows are fully consumed its `columns()` reports
    /// none, so metadata has to be captured while still on the just-executed
    /// set. When the live result has columns, `self.col_names`/`self.col_kinds`
    /// are refreshed from it too, so `columnCount()`/`getColumnMeta()` reflect
    /// the procedure's real output columns rather than the empty prepare-time
    /// set. A genuine non-SELECT (e.g. an `INSERT` via a prepared statement)
    /// still reports zero live columns, so it keeps taking the
    /// `affected_rows()` path below unchanged.
    ///
    /// This alone is not sufficient, though: the generated `PDOStatement::
    /// execute()` prelude reads `column_count()` BEFORE ever calling `step()`
    /// (and thus before this method has run even once) to decide whether the
    /// upcoming first `step()` is a throwaway "run the DML" call or a real
    /// "pre-fetch the first row" call whose result must be cached. A `CALL`'s
    /// prepare-time column count is genuinely `0` at that point (this method
    /// has not run yet to refresh it), so without `column_count()`'s own
    /// `is_call` placeholder (see its doc comment), the prelude picks the
    /// throwaway branch and discards this method's very first materialized row.
    fn execute(&mut self, conn: &mut MyConn) -> Result<(), i64> {
        if self.emulated_sql.is_some() {
            return self.execute_emulated(conn);
        }
        let values = self.build_values();
        let statement = self
            .statement
            .as_ref()
            .expect("native MySQL statement missing its prepared handle")
            .clone();
        if !self.buffered {
            let (stream_id, rowset) = conn.start_native_stream(statement, values)?;
            self.stream_id = Some(stream_id);
            self.remaining_rowsets.clear();
            self.install_rowset(conn, rowset);
            conn.note_transaction_sql(&self.query_string);
            return Ok(());
        }
        let outcome: Result<Vec<MyRowset>, mysql::Error> = (|| {
            let mut res = conn.conn.exec_iter(&statement, values)?;
            let mut rowsets = Vec::new();
            while let Some(mut set) = res.iter() {
                let last_id = set.last_insert_id();
                let affected = set.affected_rows() as i64;
                let live = set.columns();
                let cols: &[Column] = live.as_ref();
                let col_names = cols
                    .iter()
                    .map(|column| column_display_name(column, conn.fetch_table_names))
                    .collect::<Vec<_>>();
                let col_kinds = cols.iter().map(ColKind::from_column).collect::<Vec<_>>();
                let col_types = cols
                    .iter()
                    .map(|column| column.column_type())
                    .collect::<Vec<_>>();
                let col_tables = cols
                    .iter()
                    .map(|column| column.table_str().into_owned())
                    .collect::<Vec<_>>();
                let col_flags = cols.iter().map(Column::flags).collect::<Vec<_>>();
                let col_lengths = cols.iter().map(Column::column_length).collect::<Vec<_>>();
                let col_precisions = cols.iter().map(Column::decimals).collect::<Vec<_>>();
                let mut rows = Vec::new();
                for row in set.by_ref() {
                    rows.push(decode_row(row?.unwrap(), &col_kinds));
                }
                rowsets.push(MyRowset {
                    affected,
                    last_id,
                    col_names,
                    col_kinds,
                    col_types,
                    col_tables,
                    col_flags,
                    col_lengths,
                    col_precisions,
                    rows,
                });
            }
            Ok(rowsets)
        })();
        let warnings = conn.conn.warnings();
        match outcome {
            Ok(mut rowsets) => {
                conn.errcode = 0;
                conn.sqlstate = "00000".to_string();
                conn.warning_count = warnings;
                let first = if rowsets.is_empty() {
                    MyRowset {
                        affected: 0,
                        last_id: None,
                        col_names: self.col_names.clone(),
                        col_kinds: self.col_kinds.clone(),
                        col_types: self.col_types.clone(),
                        col_tables: self.col_tables.clone(),
                        col_flags: self.col_flags.clone(),
                        col_lengths: self.col_lengths.clone(),
                        col_precisions: self.col_precisions.clone(),
                        rows: Vec::new(),
                    }
                } else {
                    rowsets.remove(0)
                };
                self.remaining_rowsets = rowsets;
                self.install_rowset(conn, first);
                conn.note_transaction_sql(&self.query_string);
                Ok(())
            }
            Err(e) => {
                conn.sqlstate = err_sqlstate(&e);
                conn.errmsg = e.to_string();
                conn.errcode = err_code(&e);
                Err(-1)
            }
        }
    }

    /// Executes an emulated MySQL statement through the text protocol after
    /// client-side placeholder substitution and materializes its first result.
    fn execute_emulated(&mut self, conn: &mut MyConn) -> Result<(), i64> {
        if self.bound.iter().any(|bound| !bound) {
            conn.sqlstate = "HY093".to_string();
            conn.errcode = 1;
            conn.errmsg = "Invalid parameter number: number of bound variables does not match number of tokens".to_string();
            return Err(-1);
        }
        let values = self.build_values();
        let national = self.build_national_flags();
        let sql = match interpolate_emulated_sql(
            self.emulated_sql
                .as_deref()
                .expect("emulated MySQL statement missing SQL"),
            &values,
            &national,
            self.no_backslash_escape,
        ) {
            Ok(sql) => sql,
            Err(message) => {
                conn.sqlstate = "HY093".to_string();
                conn.errcode = 1;
                conn.errmsg = message;
                return Err(-1);
            }
        };
        self.sent_sql = sql.clone();
        if !self.buffered {
            let (stream_id, rowset) = conn.start_text_stream(sql)?;
            self.stream_id = Some(stream_id);
            self.remaining_rowsets.clear();
            self.install_rowset(conn, rowset);
            conn.note_transaction_sql(&self.query_string);
            return Ok(());
        }
        let outcome = (|| {
            let mut result = conn.conn.query_iter(sql)?;
            let mut rowsets = Vec::new();
            while let Some(mut set) = result.iter() {
                let last_id = set.last_insert_id();
                let affected = set.affected_rows() as i64;
                let columns = set.columns();
                let columns: &[Column] = columns.as_ref();
                let col_names = columns
                    .iter()
                    .map(|column| column_display_name(column, conn.fetch_table_names))
                    .collect::<Vec<_>>();
                let col_kinds = columns.iter().map(ColKind::from_column).collect::<Vec<_>>();
                let col_types = columns
                    .iter()
                    .map(|column| column.column_type())
                    .collect::<Vec<_>>();
                let col_tables = columns
                    .iter()
                    .map(|column| column.table_str().into_owned())
                    .collect::<Vec<_>>();
                let col_flags = columns.iter().map(Column::flags).collect::<Vec<_>>();
                let col_lengths = columns
                    .iter()
                    .map(Column::column_length)
                    .collect::<Vec<_>>();
                let col_precisions = columns.iter().map(Column::decimals).collect::<Vec<_>>();
                let mut rows = Vec::new();
                for row in set.by_ref() {
                    rows.push(decode_row(row?.unwrap(), &col_kinds));
                }
                rowsets.push(MyRowset {
                    affected,
                    last_id,
                    col_names,
                    col_kinds,
                    col_types,
                    col_tables,
                    col_flags,
                    col_lengths,
                    col_precisions,
                    rows,
                });
            }
            Ok::<_, mysql::Error>(rowsets)
        })();
        let warnings = conn.conn.warnings();
        match outcome {
            Ok(mut rowsets) => {
                conn.errcode = 0;
                conn.sqlstate = "00000".to_string();
                conn.warning_count = warnings;
                let first = if rowsets.is_empty() {
                    MyRowset {
                        affected: 0,
                        last_id: None,
                        col_names: Vec::new(),
                        col_kinds: Vec::new(),
                        col_types: Vec::new(),
                        col_tables: Vec::new(),
                        col_flags: Vec::new(),
                        col_lengths: Vec::new(),
                        col_precisions: Vec::new(),
                        rows: Vec::new(),
                    }
                } else {
                    rowsets.remove(0)
                };
                self.remaining_rowsets = rowsets;
                self.install_rowset(conn, first);
                conn.note_transaction_sql(&self.query_string);
                Ok(())
            }
            Err(error) => {
                conn.sqlstate = err_sqlstate(&error);
                conn.errmsg = error.to_string();
                conn.errcode = err_code(&error);
                Err(-1)
            }
        }
    }

    /// Advances to the next row: `1` for a row, `0` when exhausted, `-1` on error.
    /// Executes lazily on the first call.
    pub fn step(&mut self, conn: &mut MyConn) -> i64 {
        if !self.executed {
            if let Err(code) = self.execute(conn) {
                return code;
            }
        }
        if let Some(stream_id) = self.stream_id {
            return match conn.next_stream_row(stream_id) {
                Ok(Some(row)) => {
                    self.rows.clear();
                    self.rows.push(row);
                    self.cursor = 0;
                    1
                }
                Ok(None) => {
                    self.rows.clear();
                    self.cursor = 0;
                    match conn.next_stream_rowset(stream_id) {
                        Ok(Some(rowset)) => self.remaining_rowsets.push(rowset),
                        Ok(None) => self.stream_id = None,
                        Err(code) => return code,
                    }
                    0
                }
                Err(code) => code,
            };
        }
        self.cursor += 1;
        if (self.cursor as usize) < self.rows.len() {
            1
        } else {
            conn.unbuffered_active = false;
            0
        }
    }

    /// Returns the current cell at column `i`, if a row is active.
    fn cell(&self, i: i64) -> Option<&Cell> {
        if self.cursor < 0 {
            return None;
        }
        self.rows
            .get(self.cursor as usize)
            .and_then(|row| row.get(i as usize))
    }

    /// Number of result columns (available before execution).
    ///
    /// P0-C: for an unexecuted `CALL` (`self.is_call`, `self.col_names` still
    /// empty), this reports a placeholder `1` instead of the genuine prepare-time
    /// `0`. The generated `PDOStatement::execute()` prelude reads this count
    /// right before its first `step()` to decide which of two branches to take:
    /// a `0` means "no-result statement" (INSERT/UPDATE/DELETE/DDL) — it runs one
    /// throwaway `step()` and does not cache the result for the caller's first
    /// `fetch()`; a non-zero count means "SELECT-style" — it caches that same
    /// first `step()`'s row so no fetch ever skips it. A `CALL`'s real column
    /// count is not known until it actually runs (`execute()` below refreshes
    /// `col_names`/`col_kinds` from the live result), so reporting the genuine `0`
    /// here would misroute a row-producing `CALL` into the no-result branch,
    /// silently discarding the very first row it returns — the observed bug this
    /// hack fixes. Once executed, `col_names` reflects the real (possibly still
    /// zero, for a `CALL` with no internal `SELECT`) count and this reports it
    /// unconditionally — the placeholder only ever applies pre-execution.
    pub fn column_count(&self) -> i64 {
        if (self.is_call || self.emulated_sql.is_some())
            && !self.executed
            && self.col_names.is_empty()
        {
            1
        } else {
            self.col_names.len() as i64
        }
    }

    /// Name of result column `i` (0-based).
    pub fn column_name(&self, i: i64) -> String {
        self.col_names.get(i as usize).cloned().unwrap_or_default()
    }

    /// MySQL native type name of result column `i` (0-based) — the server's own
    /// name for the column's wire type (`LONG`, `VAR_STRING`, `NEWDECIMAL`, `BIT`,
    /// `JSON`, `TIMESTAMP`, …), exactly as php-src's `type_to_name_native` spells
    /// it (`ext/pdo_mysql/mysql_statement.c:716-770`; see [`native_type_name`]).
    /// Backs `getColumnMeta`'s `native_type` on a `mysql:` statement (F-MY-08),
    /// which until now fell through to the generic SQLite storage-class names
    /// ("integer"/"double"/"string") — the wrong vocabulary for this driver, and
    /// one that cannot distinguish a `VARCHAR` from a `BLOB` from a `DECIMAL`.
    ///
    /// Read from the column descriptor rather than a live cell (mirroring the pg
    /// accessor), so it reports the column's DECLARED type whether or not a row is
    /// active — a NULL value never degrades it to a runtime storage class.
    ///
    /// Empty string for an out-of-range index, and for the wire types php-src
    /// itself has no name for (where it omits the `native_type` key entirely):
    /// `""` is the neutral "no metadata / not this driver" value the bridge's
    /// dispatch already uses.
    pub fn column_native_type(&self, i: i64) -> String {
        if i < 0 {
            return String::new();
        }
        self.col_types
            .get(i as usize)
            .map(|&t| native_type_name(t).to_string())
            .unwrap_or_default()
    }

    /// Returns the server-provided table label for result column `i`.
    pub fn column_table_name(&self, i: i64) -> String {
        if i < 0 {
            return String::new();
        }
        self.col_tables.get(i as usize).cloned().unwrap_or_default()
    }

    /// Returns the raw MySQL `ColumnFlags` bits for result column `i`.
    pub fn column_flags(&self, i: i64) -> i64 {
        if i < 0 {
            return 0;
        }
        self.col_flags
            .get(i as usize)
            .map(|flags| i64::from(flags.bits()))
            .unwrap_or(0)
    }

    /// Returns MySQL's declared maximum column byte length.
    pub fn column_len(&self, i: i64) -> i64 {
        if i < 0 {
            return 0;
        }
        self.col_lengths
            .get(i as usize)
            .map(|length| i64::from(*length))
            .unwrap_or(0)
    }

    /// Returns MySQL's native decimals/precision marker for the column.
    pub fn column_precision(&self, i: i64) -> i64 {
        if i < 0 {
            return 0;
        }
        self.col_precisions
            .get(i as usize)
            .map(|precision| i64::from(*precision))
            .unwrap_or(0)
    }

    /// SQLite-compatible type code for the current row's column `i`:
    /// 1=int, 2=float, 3=text, 4=blob, 5=null.
    pub fn column_type(&self, i: i64) -> i64 {
        match self.cell(i) {
            Some(Cell::Int(_)) => 1,
            Some(Cell::Float(_)) => 2,
            Some(Cell::Text(_)) => 3,
            Some(Cell::Bytes(_)) => 4,
            _ => 5,
        }
    }

    /// Current row's column `i` as an integer.
    pub fn column_int(&self, i: i64) -> i64 {
        match self.cell(i) {
            Some(Cell::Int(v)) => *v,
            Some(Cell::Float(v)) => *v as i64,
            Some(Cell::Text(s)) => s.trim().parse().unwrap_or(0),
            Some(Cell::Bytes(b)) => String::from_utf8_lossy(b).trim().parse().unwrap_or(0),
            _ => 0,
        }
    }

    /// Current row's column `i` as a double.
    pub fn column_double(&self, i: i64) -> f64 {
        match self.cell(i) {
            Some(Cell::Float(v)) => *v,
            Some(Cell::Int(v)) => *v as f64,
            Some(Cell::Text(s)) => s.trim().parse().unwrap_or(0.0),
            Some(Cell::Bytes(b)) => String::from_utf8_lossy(b).trim().parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    /// Current row's column `i` as byte-counted PDO data.
    pub fn column_data(&self, i: i64) -> Vec<u8> {
        match self.cell(i) {
            Some(Cell::Bytes(b)) => b.clone(),
            Some(Cell::Text(s)) => s.as_bytes().to_vec(),
            Some(Cell::Int(v)) => v.to_string().into_bytes(),
            Some(Cell::Float(v)) => v.to_string().into_bytes(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Emulated interpolation skips quoted/comment markers and escapes a real
    /// placeholder value through mysql_common's protocol-aware literal renderer.
    #[test]
    fn emulated_interpolation_replaces_only_real_placeholders() {
        let (sql, _, order, mixed) = translate_placeholders(
            "SELECT '?', /* ? */ :first, :name, ??",
            false,
        );
        assert!(!mixed);
        assert_eq!(order, vec![1, 2]);
        let rendered = interpolate_emulated_sql(
            &sql,
            &[Value::Bytes(b"O'Reilly".to_vec()), Value::Int(7)],
            &[false, false],
            false,
        )
        .expect("emulated SQL renders");
        assert_eq!(rendered, "SELECT '?', /* ? */ 'O\\'Reilly', 7, ??");
    }

    /// National string parameters add the `N` introducer only to the matching
    /// emulated placeholder and leave ordinary strings unchanged.
    #[test]
    fn emulated_interpolation_marks_national_strings() {
        let rendered = interpolate_emulated_sql(
            "SELECT ?, ?",
            &[Value::Bytes(b"national".to_vec()), Value::Bytes(b"plain".to_vec())],
            &[true, false],
            false,
        )
        .expect("emulated national SQL renders");
        assert_eq!(rendered, "SELECT N'national', 'plain'");
    }

    /// Extracts the `Cell::Text` payload, or fails naming the wrong variant (no
    /// `Debug` derive on `Cell` elsewhere in the bridge, so this keeps the tests
    /// below from requiring one just for a panic message).
    fn expect_text(cell: Cell) -> String {
        match cell {
            Cell::Text(s) => s,
            Cell::Int(_) => panic!("expected Cell::Text, got Cell::Int"),
            Cell::Null => panic!("expected Cell::Text, got Cell::Null"),
            Cell::Float(_) => panic!("expected Cell::Text, got Cell::Float"),
            Cell::Bytes(_) => panic!("expected Cell::Text, got Cell::Bytes"),
        }
    }

    /// Extracts the `Cell::Int` payload, or fails naming the wrong variant.
    fn expect_int(cell: Cell) -> i64 {
        match cell {
            Cell::Int(v) => v,
            Cell::Text(_) => panic!("expected Cell::Int, got Cell::Text"),
            Cell::Null => panic!("expected Cell::Int, got Cell::Null"),
            Cell::Float(_) => panic!("expected Cell::Int, got Cell::Float"),
            Cell::Bytes(_) => panic!("expected Cell::Int, got Cell::Bytes"),
        }
    }

    /// P2-2: a `BIGINT UNSIGNED` value at or below `i64::MAX` decodes as a plain
    /// `Cell::Int` — the common case, unaffected by the overflow fix below.
    #[test]
    fn bigint_unsigned_within_i64_max_decodes_to_int() {
        let u = i64::MAX as u64;
        assert_eq!(expect_int(decode_value(Value::UInt(u), ColKind::Other)), i64::MAX);
    }

    /// P2-2 (mandatory unit test, no server needed): a `BIGINT UNSIGNED` value
    /// above `i64::MAX` must decode to the exact decimal numeric string rather
    /// than silently wrapping negative through an `as i64` cast.
    #[test]
    fn bigint_unsigned_above_i64_max_decodes_to_numeric_string() {
        let u = u64::MAX;
        assert_eq!(
            expect_text(decode_value(Value::UInt(u), ColKind::Other)),
            "18446744073709551615"
        );
        // The tightest regression check for the `>` boundary comparison: one
        // past `i64::MAX` must already take the text path.
        let boundary = i64::MAX as u64 + 1;
        assert_eq!(
            expect_text(decode_value(Value::UInt(boundary), ColKind::Other)),
            boundary.to_string()
        );
    }

    /// P0-D (mandatory unit test, no server needed): `BIT` and `GEOMETRY`
    /// columns must classify as `ColKind::Binary` so `decode_value` routes them
    /// through the byte-preserving `Cell::Bytes` path instead of the lossy
    /// `String::from_utf8_lossy` path used for `ColKind::Other` — otherwise a
    /// `BIT(8)` value like `0xFF` decodes as a 3-byte U+FFFD replacement
    /// character instead of the original byte. Neither type depends on the
    /// character set, so the default (0) is left as-is.
    #[test]
    fn bit_and_geometry_columns_classify_as_binary() {
        assert_eq!(
            ColKind::from_column(&Column::new(ColumnType::MYSQL_TYPE_BIT)),
            ColKind::Binary
        );
        assert_eq!(
            ColKind::from_column(&Column::new(ColumnType::MYSQL_TYPE_GEOMETRY)),
            ColKind::Binary
        );
    }

    /// P1 (mandatory unit test, no server needed): `VARBINARY`/`BINARY` columns
    /// arrive as `MYSQL_TYPE_VAR_STRING`/`MYSQL_TYPE_STRING` — the exact same
    /// `ColumnType` a `VARCHAR`/`CHAR` column uses — so only the charset-63
    /// (`binary` collation) marker tells them apart. A charset-63 `VAR_STRING`/
    /// `STRING` column must classify as `ColKind::Binary`; the same `ColumnType`
    /// under a real text charset (e.g. utf8mb4 = 45) must classify as `Other`, so
    /// a genuine `VARCHAR`/`CHAR` keeps decoding through the text path.
    #[test]
    fn varbinary_and_binary_columns_classify_by_charset_not_type() {
        let varbinary = Column::new(ColumnType::MYSQL_TYPE_VAR_STRING)
            .with_character_set(MYSQL_BINARY_CHARSET);
        assert_eq!(ColKind::from_column(&varbinary), ColKind::Binary);

        let binary =
            Column::new(ColumnType::MYSQL_TYPE_STRING).with_character_set(MYSQL_BINARY_CHARSET);
        assert_eq!(ColKind::from_column(&binary), ColKind::Binary);

        let varchar_utf8mb4 =
            Column::new(ColumnType::MYSQL_TYPE_VAR_STRING).with_character_set(45);
        assert_eq!(ColKind::from_column(&varchar_utf8mb4), ColKind::Other);
    }

    /// P0-C (mandatory unit test, no server needed): `sql_is_call_statement`
    /// recognizes `CALL` case-insensitively and with leading whitespace, but
    /// rejects a bare `CALLBACK(...)`-style identifier that merely starts with
    /// the same four letters, and any non-`CALL` statement.
    #[test]
    fn sql_is_call_statement_detects_call_only() {
        assert!(sql_is_call_statement("CALL my_call_sp()"));
        assert!(sql_is_call_statement("  call my_call_sp(?, ?)"));
        assert!(sql_is_call_statement("Call\tmy_call_sp()"));
        assert!(!sql_is_call_statement("CALLBACK()"));
        assert!(!sql_is_call_statement("SELECT 1"));
        assert!(!sql_is_call_statement("INSERT INTO t VALUES (1)"));
        assert!(!sql_is_call_statement(""));
    }

    /// F-MY-05 (re-opens P0-C for the commented variant): the server ignores
    /// whatever leads a statement, so a `CALL` behind an optimizer hint or a note
    /// is still a stored-procedure call. Recognizing only past the whitespace left
    /// those flagged non-`CALL`, which fed the prelude the genuine (but meaningless)
    /// prepare-time column count of `0` and routed a row-producing procedure into
    /// the no-result DML branch — dropping its first row. All three comment forms
    /// are covered, interleaved with whitespace and with each other.
    ///
    /// The negative half is the load-bearing one for the skipping logic itself: a
    /// leading comment must never make an ordinary statement LOOK like a `CALL`,
    /// whether the word appears inside the comment or not. And a bare `--` with no
    /// trailing whitespace is not a MySQL comment at all (it is the arithmetic
    /// `- -`, unlike PostgreSQL), so nothing behind it may be skipped. An
    /// unterminated block comment swallows the rest of the statement, leaving no
    /// keyword to test — also not a `CALL`.
    #[test]
    fn sql_is_call_statement_skips_leading_comments() {
        assert!(sql_is_call_statement("/* hint */ CALL p()"));
        assert!(sql_is_call_statement("-- note\nCALL p()"));
        assert!(sql_is_call_statement("# note\ncall p(?)"));
        assert!(sql_is_call_statement("  /* a */ -- b\n\t/* c */\nCALL p()"));
        assert!(!sql_is_call_statement("/* CALL */ SELECT 1"));
        assert!(!sql_is_call_statement("-- CALL p()\nSELECT 1"));
        assert!(!sql_is_call_statement("# CALL p()\nSELECT 1"));
        assert!(!sql_is_call_statement("--CALL p()"));
        assert!(!sql_is_call_statement("/* unterminated CALL p()"));
    }

    /// P2-1: a `connect_timeout=<secs>` DSN key (as the prelude folds in
    /// alongside `user=`/`password=` when `PDO::ATTR_TIMEOUT` is set) parses into
    /// the `mysql` client's `tcp_connect_timeout` option. Pure DSN-parsing logic,
    /// no server needed — `build_opts` never dials out. The explicit `5` also
    /// proves it WINS over F-CORE-10's 30 s default below.
    #[test]
    fn build_opts_maps_connect_timeout_dsn_key() {
        let (opts, _charset) =
            build_opts("mysql:host=localhost;dbname=testdb;connect_timeout=5", false, false).unwrap();
        let opts: mysql::Opts = opts.into();
        assert_eq!(opts.get_tcp_connect_timeout(), Some(Duration::from_secs(5)));
    }

    /// F-CORE-10: a DSN with no `connect_timeout` key (and no `PDO::ATTR_TIMEOUT`,
    /// which the prelude folds into that same key) still gets php-src's
    /// unconditional 30 s connect timeout — `mysql_driver.c:755,784` defaults
    /// `PDO_ATTR_TIMEOUT` to 30 and always passes it to
    /// `mysql_options(MYSQL_OPT_CONNECT_TIMEOUT, …)`. Leaving it unset (as this
    /// previously asserted) fell back on the OS TCP timeout, hanging a connection
    /// to a black-holed host far longer than real PHP does.
    #[test]
    fn build_opts_defaults_connect_timeout_to_30s() {
        let (opts, _charset) =
            build_opts("mysql:host=localhost;dbname=testdb", false, false).unwrap();
        let opts: mysql::Opts = opts.into();
        assert_eq!(
            opts.get_tcp_connect_timeout(),
            Some(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
        );
    }

    /// Packed PDO driver options preserve supported selections, including the
    /// CA-directory path, and reject security options the client cannot honor.
    #[test]
    fn driver_options_parse_supported_flags_and_security_paths() {
        let options = parse_driver_options(
            "local=1;compress=1;ignore=1;multi=0;buffered=0;capath=/tmp/mysql-ca;",
        )
        .expect("supported PDO MySQL options should parse");
        assert!(options.local_infile);
        assert!(options.compress);
        assert!(options.ignore_space);
        assert!(!options.multi_statements);
        assert!(!options.buffered_query);
        assert_eq!(options.ssl_ca_path, Some(PathBuf::from("/tmp/mysql-ca")));
        let error = parse_driver_options("serverkey=/tmp/key.pem;")
            .expect_err("an inert server public key must fail loudly");
        assert!(error.contains("ATTR_SERVER_PUBLIC_KEY"));
    }

    /// `ATTR_SSL_CAPATH` is translated from an OpenSSL-style CA directory into
    /// the multi-certificate PEM file accepted by the mysql crate's rustls path.
    #[test]
    fn ssl_capath_builds_a_sorted_temporary_pem_bundle() {
        static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "elephc-pdo-capath-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&dir).expect("create CA directory fixture");
        let direct = dir.join("direct.pem");
        fs::write(&direct, b"-----BEGIN CERTIFICATE-----\nDIRECT\n-----END CERTIFICATE-----\n")
            .expect("write direct CA fixture");
        fs::write(
            dir.join("02-second.pem"),
            b"-----BEGIN CERTIFICATE-----\nSECOND\n-----END CERTIFICATE-----\n",
        )
        .expect("write second directory CA fixture");
        fs::write(
            dir.join("01-first.crt"),
            b"-----BEGIN CERTIFICATE-----\nFIRST\n-----END CERTIFICATE-----\n",
        )
        .expect("write first directory CA fixture");
        fs::write(dir.join("README"), b"not a certificate")
            .expect("write ignored non-PEM fixture");

        let (config, bundle) = normalize_ssl_ca_sources(
            &format!("ca={};verify=1;", direct.display()),
            Some(&dir),
        )
        .expect("normalize CA sources");
        let bundle = bundle.expect("CAPATH must create a temporary bundle");
        let contents = fs::read_to_string(&bundle.path).expect("read temporary CA bundle");
        assert!(contents.contains("DIRECT"));
        let first = contents.find("FIRST").expect("first CA present");
        let second = contents.find("SECOND").expect("second CA present");
        assert!(first < second, "directory certificates must be deterministic");
        assert!(config.starts_with(&format!("ca={};", bundle.path.display())));
        assert!(config.ends_with("verify=1;"));

        drop(bundle);
        fs::remove_dir_all(&dir).expect("remove CA directory fixture");
    }

    /// `ATTR_IGNORE_SPACE` reaches the MySQL handshake capability only when
    /// requested, alongside but independently from `ATTR_FOUND_ROWS`.
    #[test]
    fn build_opts_sets_ignore_space_capability() {
        let (opts, _) =
            build_opts("mysql:host=localhost;dbname=testdb", false, true).unwrap();
        let opts: mysql::Opts = opts.into();
        assert!(
            opts.get_additional_capabilities()
                .contains(CapabilityFlags::CLIENT_IGNORE_SPACE)
        );
    }

    /// Multi-statement detection ignores semicolons inside every quoted/comment
    /// region and accepts one trailing separator, while finding real second SQL.
    #[test]
    fn multi_statement_detection_is_sql_aware() {
        assert!(!sql_has_multiple_statements("SELECT ';';", false));
        assert!(!sql_has_multiple_statements(
            "SELECT 1 /* ; SELECT 2 */; -- tail ;\n",
            false,
        ));
        assert!(sql_has_multiple_statements("SELECT 1; SELECT 2", false));
        assert!(sql_has_multiple_statements("SELECT `a;b`; CALL p()", false));
    }

    /// P2-3: a `charset=<name>` DSN key is captured (for `MyConn::open` to turn
    /// into a `SET NAMES <name>` init statement), validated to plain identifier
    /// characters so it cannot inject SQL into that generated statement.
    #[test]
    fn build_opts_captures_valid_charset() {
        let (_opts, charset) =
            build_opts(
                "mysql:host=localhost;dbname=testdb;charset=utf8mb4",
                false,
                false,
            )
            .unwrap();
        assert_eq!(charset.as_deref(), Some("utf8mb4"));
    }

    /// A `charset` value containing anything beyond `[A-Za-z0-9_]` (e.g. an
    /// attempted SQL-injection payload embedded in the DSN's `charset=` value —
    /// a `;` in the payload would already be defused by the DSN's own
    /// semicolon-segmented parsing, so this uses a quote/space payload that
    /// stays within the one `charset=` segment) is dropped rather than reaching
    /// the generated `SET NAMES` statement.
    #[test]
    fn build_opts_rejects_charset_with_unsafe_characters() {
        let (_opts, charset) = build_opts(
            "mysql:host=localhost;dbname=testdb;charset=utf8mb4' OR '1'='1",
            false,
            false,
        )
        .unwrap();
        assert_eq!(charset, None);
    }

    /// A DSN with no `charset` key leaves it unset.
    #[test]
    fn build_opts_leaves_charset_unset_by_default() {
        let (_opts, charset) =
            build_opts("mysql:host=localhost;dbname=testdb", false, false).unwrap();
        assert_eq!(charset, None);
    }

    /// F-CORE-02: the prelude percent-encodes a constructor-supplied password
    /// containing ';' and '%' (here `a;b%c` -> `a%3Bb%25c`) before folding it
    /// into the DSN, so it survives `body.split(';')` intact instead of
    /// truncating at the embedded ';'. `build_opts` must undo that encoding —
    /// `%3B` back to ';', `%25` back to '%' — landing on the original value.
    #[test]
    fn build_opts_percent_decodes_a_password_containing_semicolon_and_percent() {
        let (opts, _charset) = build_opts(
            "mysql:host=127.0.0.1;user=admin;password=a%3Bb%25c",
            false,
            false,
        )
        .unwrap();
        let opts: mysql::Opts = opts.into();
        assert_eq!(opts.get_user(), Some("admin"));
        assert_eq!(opts.get_pass(), Some("a;b%c"));
    }

    /// The percent-decoding above must be a no-op for a credential with no '%'
    /// byte at all — the common case — so it round-trips byte-identical.
    #[test]
    fn build_opts_leaves_a_plain_password_byte_identical() {
        let (opts, _charset) =
            build_opts(
                "mysql:host=127.0.0.1;user=admin;password=secret",
                false,
                false,
            )
            .unwrap();
        let opts: mysql::Opts = opts.into();
        assert_eq!(opts.get_pass(), Some("secret"));
    }

    /// An empty SSL config is always a no-op (plaintext), regardless of feature.
    #[test]
    fn apply_ssl_opts_empty_is_noop() {
        assert!(apply_ssl_opts(OptsBuilder::new(), "").is_ok());
    }

    /// In a custom build without `mysql-tls`, a non-empty SSL config fails loudly
    /// so a program that asked for TLS is not silently downgraded to plaintext.
    #[cfg(not(feature = "mysql-tls"))]
    #[test]
    fn apply_ssl_opts_requires_feature_when_configured() {
        // `OptsBuilder` has no `Debug`, so match rather than `unwrap_err`.
        match apply_ssl_opts(OptsBuilder::new(), "ca=/etc/ca.pem") {
            Ok(_) => panic!("expected an error when the mysql-tls feature is disabled"),
            Err(err) => assert!(err.contains("mysql-tls"), "unexpected error: {err}"),
        }
    }

    /// With `mysql-tls`, a packed config is parsed into `SslOpts` and attached
    /// without panicking (the ring provider installs on demand).
    #[cfg(feature = "mysql-tls")]
    #[test]
    fn apply_ssl_opts_builds_sslopts() {
        assert!(apply_ssl_opts(OptsBuilder::new(), "ca=/etc/ca.pem;verify=0").is_ok());
    }

    /// F-MY-08 (mandatory unit test, no server needed): the wire-type -> `native_type`
    /// mapping backing `getColumnMeta()`'s MySQL branch is php-src's `type_to_name_native`
    /// (`ext/pdo_mysql/mysql_statement.c:716-770`), whose `PDO_MYSQL_NATIVE_TYPE_NAME(x)`
    /// macro STRINGIFIES THE `MYSQL_TYPE_` SUFFIX. The names therefore do NOT match the SQL
    /// keyword a user wrote, and that is the whole point of pinning them: an `INT` column
    /// reports `LONG`, a `TINYINT` reports `TINY`, a `BIGINT` reports `LONGLONG`, a
    /// `MEDIUMINT` reports `INT24`, a `VARCHAR` reports `VAR_STRING`, a `CHAR` reports
    /// `STRING`, and a modern `DECIMAL` reports `NEWDECIMAL` (plain `DECIMAL` is only the
    /// pre-5.0 legacy type). A "helpful" mapping to the SQL spelling would be a divergence
    /// from real PDO dressed up as a courtesy.
    #[test]
    fn native_type_name_matches_php_src_type_to_name_native() {
        // The counter-intuitive ones — where the wire name and the SQL keyword differ.
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_LONG), "LONG"); // INT
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_TINY), "TINY"); // TINYINT
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_LONGLONG), "LONGLONG"); // BIGINT
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_INT24), "INT24"); // MEDIUMINT
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_VAR_STRING), "VAR_STRING"); // VARCHAR
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_STRING), "STRING"); // CHAR
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_NEWDECIMAL), "NEWDECIMAL"); // DECIMAL

        // The ones that do read as expected, spot-checked across the type families.
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_SHORT), "SHORT");
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_BIT), "BIT");
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_BLOB), "BLOB");
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_JSON), "JSON");
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_DATETIME), "DATETIME");
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_NULL), "NULL");
    }

    /// F-MY-08, the `default:` arm: php-src's switch has no case for a server-INTERNAL type
    /// and `return NULL`s, which makes `pdo_mysql_stmt_col_meta` OMIT the `native_type` key
    /// entirely (`mysql_statement.c:812-815`). The bridge's neutral stand-in for that is the
    /// EMPTY STRING, and the empty string is load-bearing downstream: the prelude's
    /// `getColumnMeta()` only OVERRIDES its derived storage-class `native_type` when this
    /// returns something non-empty, so `""` is exactly what keeps SQLite's metadata (where
    /// this is never called with a real MySQL type) byte-identical.
    ///
    /// `MYSQL_TYPE_TIMESTAMP2`/`DATETIME2`/`TIME2` are the internal ones worth naming: the
    /// wire carries the plain `TIMESTAMP`/`DATETIME`/`TIME` codes, so these must never reach
    /// a real column — and if one somehow does, omitting the key beats inventing a name.
    #[test]
    fn native_type_name_is_empty_for_types_php_src_has_no_case_for() {
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_TIMESTAMP2), "");
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_DATETIME2), "");
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_TIME2), "");
        assert_eq!(native_type_name(ColumnType::MYSQL_TYPE_UNKNOWN), "");
    }
}
