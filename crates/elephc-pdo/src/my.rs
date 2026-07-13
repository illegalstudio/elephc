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
//!   lazily on the first `step()`. The whole result set is materialized into typed
//!   `Cell` values, so the column accessors read from owned data and per-value
//!   NULL is reported through the SQLite-compatible type codes (1=int, 2=float,
//!   3=text, 4=blob, 5=null).
//! - Bound values cross the wire as their native `mysql::Value` (ints, doubles,
//!   text bytes); the server coerces text to the column type, so — unlike the
//!   PostgreSQL driver — no per-parameter type inference is needed.

use std::collections::HashMap;
use std::time::Duration;

use mysql::consts::{CapabilityFlags, ColumnType};
use mysql::prelude::Queryable;
use mysql::{Column, Conn, OptsBuilder, Statement, Value};

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
    pub conn: Conn,
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
}

/// A live MySQL prepared statement and its lazily-materialized result.
pub struct MyStmt {
    pub conn_id: i64,
    statement: Statement,
    /// Maps a bare named placeholder (`name` from `:name`) to its 1-based slot.
    named_map: HashMap<String, i64>,
    /// For each `?` in source order, the 1-based bound slot that feeds it. Repeats
    /// for a reused named placeholder; `[1, 2, …]` for plain positional `?`.
    order: Vec<i64>,
    /// Bound values, indexed by 0-based slot (`slot 1` → index 0).
    binds: Vec<Bind>,
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
    /// Materialized rows; each row is a vector of decoded column cells.
    rows: Vec<Vec<Cell>>,
    /// Current 0-based row index; `-1` before the first `step()`.
    cursor: isize,
    /// Whether the query has been executed (results materialized) yet.
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
pub fn build_opts(dsn: &str, found_rows: bool) -> Result<(OptsBuilder, Option<String>), String> {
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
            "user" => user = Some(value),
            "password" => password = Some(value),
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
    if found_rows {
        opts = opts.additional_capabilities(CapabilityFlags::CLIENT_FOUND_ROWS);
    }
    // F-CORE-10: unconditional, so a DSN that names neither `connect_timeout` nor
    // (through the prelude) `ATTR_TIMEOUT` still inherits php-src's 30 s bound
    // instead of waiting out the OS TCP timeout. An explicit value — from either
    // seam, both of which land in `connect_timeout` above — wins over the default.
    let secs = connect_timeout.unwrap_or(DEFAULT_CONNECT_TIMEOUT_SECS);
    opts = opts.tcp_connect_timeout(Some(Duration::from_secs(secs)));
    Ok((opts, charset))
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

/// Scans a MySQL quoted region opened by `quote` (`'` or `"`) starting at
/// `start` (the index of the opening quote byte), returning the exclusive end
/// index just past the closing quote (or `len` if unterminated). Both quote
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
fn scan_my_string(bytes: &[u8], start: usize, quote: u8, no_backslash_escapes: bool) -> usize {
    let len = bytes.len();
    let mut j = start + 1;
    loop {
        if j >= len {
            return len;
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
            return j + 1;
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
            out.push_str(&sql[i..end]);
            i = end;
            continue;
        }
        match c {
            b'\'' | b'"' => {
                let end = scan_my_string(bytes, i, c, no_backslash_escapes);
                out.push_str(&sql[i..end]);
                i = end;
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

/// Applies the prelude's packed `Pdo\Mysql::ATTR_SSL_*` config to `opts`, enabling
/// rustls TLS for the connection. Only compiled with the opt-in `mysql-tls`
/// feature (the `mysql` crate's rustls backend pulls aws-lc-rs, which the default
/// build deliberately excludes). An empty config leaves `opts` untouched
/// (plaintext).
#[cfg(feature = "mysql-tls")]
fn apply_ssl_opts(opts: OptsBuilder, ssl_config: &str) -> Result<OptsBuilder, String> {
    if ssl_config.is_empty() {
        return Ok(opts);
    }
    install_crypto_provider();
    Ok(opts.ssl_opts(parse_ssl_config(ssl_config)))
}

/// The default (no `mysql-tls`) build has no MySQL TLS backend linked. Rather than
/// silently downgrade a program that asked for TLS to a plaintext connection, a
/// non-empty SSL config fails loudly; an empty config (no TLS requested) connects
/// normally.
#[cfg(not(feature = "mysql-tls"))]
fn apply_ssl_opts(opts: OptsBuilder, ssl_config: &str) -> Result<OptsBuilder, String> {
    if ssl_config.is_empty() {
        return Ok(opts);
    }
    Err("mysql TLS (Pdo\\Mysql::ATTR_SSL_*) was requested but requires the opt-in \
         `mysql-tls` feature, which was not compiled in (rebuild elephc-pdo with \
         --features mysql-tls)"
        .to_string())
}

/// Installs the ring `CryptoProvider` as the process default exactly once. The
/// `mysql` crate builds its rustls `ClientConfig` with the provider-less
/// `ClientConfig::builder()`, which panics when more than one provider is present
/// unless a process default is installed — and enabling `mysql-tls` brings in
/// aws-lc-rs alongside the ring provider that pg / elephc-tls already use. Pinning
/// ring keeps the whole process on a single, musl-friendly provider.
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
/// certificate and hostname validation via the crate's danger flags. Unknown keys
/// (e.g. the unsupported `MYSQL_ATTR_SSL_CAPATH`/`SSL_CIPHER`) are ignored.
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
    /// an empty string means no TLS. It is only honored when the opt-in
    /// `mysql-tls` feature is compiled in (see [`apply_ssl_opts`]).
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
    ) -> Result<MyConn, String> {
        let (mut opts, charset) = build_opts(dsn, found_rows)?;
        opts = apply_ssl_opts(opts, ssl_config)?;
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
        let conn = Conn::new(opts).map_err(|e| e.to_string())?;
        Ok(MyConn {
            conn,
            changes: 0,
            errmsg: String::new(),
            errcode: 0,
            sqlstate: "00000".to_string(),
            last_id: 0,
        })
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

    /// Runs a statement with no result rows (`PDO::exec`), returning the affected
    /// row count or `-1` on error.
    pub fn exec(&mut self, sql: &str) -> i64 {
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
        match outcome {
            Ok((affected, last)) => {
                self.note_last_id(last);
                self.changes = affected;
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
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
        match self.conn.query_drop(sql) {
            Ok(()) => 1,
            Err(e) => {
                self.sqlstate = err_sqlstate(&e);
                self.errmsg = e.to_string();
                self.errcode = err_code(&e);
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
        let (major, minor, patch) = self.conn.server_version();
        format!("{major}.{minor}.{patch}")
    }

    /// Returns the number of warnings raised by the last statement executed on this
    /// connection (`SELECT @@warning_count`), which PHP's
    /// `Pdo\Mysql::getWarningCount()` returns. `SELECT @@warning_count` does not
    /// itself clear the count and runs on a connection left clean by a preceding
    /// direct `exec()`/DML statement (no open result set), so it observes that
    /// statement's warnings. Divergence: an intervening prepared-statement
    /// `COM_STMT_CLOSE` — e.g. a `query()` result discarded before this call — resets
    /// the session count, so getWarningCount is reliable immediately after a direct
    /// exec()/DML statement (the pure-Rust client also does not surface the EOF-packet
    /// warnings of a SELECT). Backs `Pdo\Mysql::getWarningCount()`.
    pub fn warning_count(&mut self) -> i64 {
        match self.conn.query_first::<u64, _>("SELECT @@warning_count") {
            Ok(Some(n)) => n as i64,
            _ => 0,
        }
    }

    /// Returns whether the connection's current session has `NO_BACKSLASH_ESCAPES`
    /// active in its `sql_mode` (backslash is then a literal character in a string
    /// literal, so backslash-escaping a quoted value is unsafe there —
    /// `PDO::quote()`'s MySQL branch falls back to `''`-doubling only in that case,
    /// P1-f). The `mysql` crate already tracks this from the connection's session
    /// state (`Conn::no_backslash_escape`), so no extra query is needed here.
    pub fn no_backslash_escape(&self) -> bool {
        self.conn.no_backslash_escape()
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
    pub fn prepare(&mut self, sql: &str) -> Result<MyStmt, String> {
        let (translated, named_map, order, mixed) =
            translate_placeholders(sql, self.no_backslash_escape());
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
        match self.conn.prep(&translated) {
            Ok(statement) => {
                let col_names = statement
                    .columns()
                    .iter()
                    .map(|c| c.name_str().into_owned())
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
                // Distinct slots run 1..=N contiguously, so the highest slot in
                // `order` is the bound-value count.
                let n_binds = order.iter().copied().max().unwrap_or(0) as usize;
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                Ok(MyStmt {
                    conn_id: 0,
                    statement,
                    named_map,
                    order,
                    binds: vec![Bind::Null; n_binds],
                    col_names,
                    col_kinds,
                    col_types,
                    rows: Vec::new(),
                    cursor: -1,
                    executed: false,
                    is_call: sql_is_call_statement(sql),
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
        1
    }

    /// Resets the cursor and execution state, keeping the bound values.
    pub fn reset(&mut self) -> i64 {
        self.cursor = -1;
        self.executed = false;
        self.rows.clear();
        1
    }

    /// Clears all bound values back to NULL.
    pub fn clear_bindings(&mut self) -> i64 {
        for b in &mut self.binds {
            *b = Bind::Null;
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
                Bind::Bytes(b) => Value::Bytes(b.clone()),
            })
            .collect()
    }

    /// Executes the query (once) and materializes the result set into decoded
    /// cells. Records the affected row count and last insert id on the connection.
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
        let values = self.build_values();
        let statement = self.statement.clone();
        type ExecOutcome = (
            i64,
            Option<u64>,
            Vec<Vec<Cell>>,
            bool,
            Vec<String>,
            Vec<ColKind>,
            Vec<ColumnType>,
        );
        let outcome: Result<ExecOutcome, mysql::Error> = (|| {
            let mut res = conn.conn.exec_iter(&statement, values)?;
            let last = res.last_insert_id();
            let (is_select, col_names, col_kinds, col_types) = {
                let live = res.columns();
                let cols: &[Column] = live.as_ref();
                if cols.is_empty() {
                    (
                        false,
                        self.col_names.clone(),
                        self.col_kinds.clone(),
                        self.col_types.clone(),
                    )
                } else {
                    (
                        true,
                        cols.iter().map(|c| c.name_str().into_owned()).collect(),
                        cols.iter().map(ColKind::from_column).collect(),
                        // F-MY-08: refreshed with the rest, so a `CALL`'s
                        // `native_type`s come from its real (post-execution)
                        // columns rather than the empty prepare-time set.
                        cols.iter().map(|c| c.column_type()).collect(),
                    )
                }
            };
            let mut rows = Vec::new();
            if is_select {
                for row in res.by_ref() {
                    rows.push(decode_row(row?.unwrap(), &col_kinds));
                }
            }
            let affected = res.affected_rows() as i64;
            drop(res);
            Ok((
                affected, last, rows, is_select, col_names, col_kinds, col_types,
            ))
        })();
        match outcome {
            Ok((affected, last, rows, is_select, col_names, col_kinds, col_types)) => {
                conn.changes = if is_select {
                    rows.len() as i64
                } else {
                    affected
                };
                conn.note_last_id(last);
                conn.errcode = 0;
                conn.sqlstate = "00000".to_string();
                self.col_names = col_names;
                self.col_kinds = col_kinds;
                self.col_types = col_types;
                self.rows = rows;
                self.executed = true;
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

    /// Advances to the next row: `1` for a row, `0` when exhausted, `-1` on error.
    /// Executes lazily on the first call.
    pub fn step(&mut self, conn: &mut MyConn) -> i64 {
        if !self.executed {
            if let Err(code) = self.execute(conn) {
                return code;
            }
        }
        self.cursor += 1;
        if (self.cursor as usize) < self.rows.len() {
            1
        } else {
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
        if self.is_call && !self.executed && self.col_names.is_empty() {
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
            build_opts("mysql:host=localhost;dbname=testdb;connect_timeout=5", false).unwrap();
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
        let (opts, _charset) = build_opts("mysql:host=localhost;dbname=testdb", false).unwrap();
        let opts: mysql::Opts = opts.into();
        assert_eq!(
            opts.get_tcp_connect_timeout(),
            Some(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
        );
    }

    /// P2-3: a `charset=<name>` DSN key is captured (for `MyConn::open` to turn
    /// into a `SET NAMES <name>` init statement), validated to plain identifier
    /// characters so it cannot inject SQL into that generated statement.
    #[test]
    fn build_opts_captures_valid_charset() {
        let (_opts, charset) =
            build_opts("mysql:host=localhost;dbname=testdb;charset=utf8mb4", false).unwrap();
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
        )
        .unwrap();
        assert_eq!(charset, None);
    }

    /// A DSN with no `charset` key leaves it unset.
    #[test]
    fn build_opts_leaves_charset_unset_by_default() {
        let (_opts, charset) = build_opts("mysql:host=localhost;dbname=testdb", false).unwrap();
        assert_eq!(charset, None);
    }

    /// An empty SSL config is always a no-op (plaintext), regardless of feature.
    #[test]
    fn apply_ssl_opts_empty_is_noop() {
        assert!(apply_ssl_opts(OptsBuilder::new(), "").is_ok());
    }

    /// In the default build (no `mysql-tls`), a non-empty SSL config fails loudly
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
