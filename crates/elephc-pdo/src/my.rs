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
//!   feeds it, so a `:name` used several times binds the same value to each `?`
//!   (PHP cannot mix `?` and `:name` in one statement, so the two cases never
//!   interleave).
//! - A statement is prepared server-side for column metadata, then executed
//!   lazily on the first `step()`. The whole result set is materialized into typed
//!   `Cell` values, so the column accessors read from owned data and per-value
//!   NULL is reported through the SQLite-compatible type codes (1=int, 2=float,
//!   3=text, 5=null).
//! - Bound values cross the wire as their native `mysql::Value` (ints, doubles,
//!   text bytes); the server coerces text to the column type, so — unlike the
//!   PostgreSQL driver — no per-parameter type inference is needed.

use std::collections::HashMap;

use mysql::consts::ColumnType;
use mysql::prelude::Queryable;
use mysql::{Conn, OptsBuilder, Statement, Value};

/// One materialized column value, already decoded to a PHP-friendly scalar.
pub enum Cell {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
}

/// A pending bound parameter value, converted to a `mysql::Value` at execute time.
#[derive(Clone)]
pub enum Bind {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
}

/// How a result column's MySQL type should render as text — the temporal types
/// need their own formatting; everything else decodes directly from the value.
#[derive(Clone, Copy)]
enum ColKind {
    Date,
    DateTime,
    Time,
    Other,
}

impl ColKind {
    /// Classifies a MySQL column type into the text-rendering bucket the decoder
    /// needs (date-only, date+time, time-of-day, or value-driven).
    fn from_column_type(ct: ColumnType) -> ColKind {
        match ct {
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

/// A live MySQL/MariaDB connection plus the last operation's bookkeeping that PDO
/// reads back (`rowCount`, `lastInsertId`, `errorCode`/`errorInfo`).
pub struct MyConn {
    pub conn: Conn,
    pub changes: i64,
    pub errmsg: String,
    pub errcode: i64,
    /// The most recent non-zero AUTO_INCREMENT id, kept sticky across later
    /// non-INSERT statements (which would otherwise reset the protocol field) to
    /// match `PDO::lastInsertId()`.
    pub last_id: i64,
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
    /// Materialized rows; each row is a vector of decoded column cells.
    rows: Vec<Vec<Cell>>,
    /// Current 0-based row index; `-1` before the first `step()`.
    cursor: isize,
    /// Whether the query has been executed (results materialized) yet.
    executed: bool,
}

/// Extracts a MySQL server error code from a driver error, or `1` for transport /
/// protocol errors that carry no SQL error number.
fn err_code(e: &mysql::Error) -> i64 {
    match e {
        mysql::Error::MySqlError(me) => me.code as i64,
        _ => 1,
    }
}

/// Parses a PDO `mysql:` DSN (semicolon-separated `key=value` pairs) into the
/// `mysql` client's connection options. Recognises `host`, `port`, `dbname`,
/// `unix_socket`, and the credential keys the prelude folds in (`user`,
/// `password`); unknown keys (e.g. `charset`) are accepted and ignored. Returns an
/// error for a DSN without the `mysql:` prefix.
pub fn build_opts(dsn: &str) -> Result<OptsBuilder, String> {
    let body = dsn
        .strip_prefix("mysql:")
        .ok_or_else(|| "could not find driver (expected a mysql: DSN)".to_string())?;
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut dbname: Option<String> = None;
    let mut socket: Option<String> = None;
    let mut user: Option<String> = None;
    let mut password: Option<String> = None;
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
            // charset and any other key are accepted for DSN compatibility but
            // have no direct option here (modern MariaDB defaults to utf8mb4).
            _ => {}
        }
    }
    let mut opts = OptsBuilder::new()
        .user(user)
        .pass(password)
        .db_name(dbname);
    // A unix socket DSN connects locally; otherwise connect over TCP (defaulting
    // the host so a `mysql:dbname=…` DSN still reaches a local server).
    if let Some(sock) = socket {
        opts = opts.socket(Some(sock));
    } else {
        opts = opts.ip_or_hostname(Some(host.unwrap_or_else(|| "localhost".to_string())));
        if let Some(p) = port {
            opts = opts.tcp_port(p);
        }
    }
    Ok(opts)
}

/// Translates PDO `?` and `:name` placeholders to MySQL's positional `?`,
/// returning the rewritten SQL, the bare-name → 1-based-slot map, and a per-`?`
/// `order` (the slot each emitted `?` reads). Single-quoted string literals are
/// passed through untouched, and a `::` sequence is not mistaken for a named
/// placeholder.
pub fn translate_placeholders(sql: &str) -> (String, HashMap<String, i64>, Vec<i64>) {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut named: HashMap<String, i64> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();
    let mut next_slot: i64 = 1;
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_string {
            out.push(c);
            if c == '\'' {
                // Doubled '' is an escaped quote inside the literal.
                if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    out.push('\'');
                    i += 2;
                    continue;
                }
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' => {
                in_string = true;
                out.push(c);
                i += 1;
            }
            '?' => {
                // Each positional placeholder is its own fresh slot.
                out.push('?');
                order.push(next_slot);
                next_slot += 1;
                i += 1;
            }
            ':' => {
                // `::` is not a named placeholder; emit verbatim.
                if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                    out.push_str("::");
                    i += 2;
                    continue;
                }
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() {
                    let nc = bytes[j] as char;
                    if nc.is_ascii_alphanumeric() || nc == '_' {
                        j += 1;
                    } else {
                        break;
                    }
                }
                if j == start {
                    // A bare colon (not a named placeholder); emit verbatim.
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
                i = j;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, named, order)
}

impl MyConn {
    /// Connects to MySQL/MariaDB for a `mysql:` DSN. Returns the connection or an
    /// error message for `last_open_error`.
    pub fn open(dsn: &str) -> Result<MyConn, String> {
        let opts = build_opts(dsn)?;
        let conn = Conn::new(opts).map_err(|e| e.to_string())?;
        Ok(MyConn {
            conn,
            changes: 0,
            errmsg: String::new(),
            errcode: 0,
            last_id: 0,
        })
    }

    /// Records the AUTO_INCREMENT id from the just-run statement when it is
    /// non-zero, so `lastInsertId()` survives an intervening non-INSERT query.
    fn note_last_id(&mut self, id: Option<u64>) {
        if let Some(id) = id {
            if id != 0 {
                self.last_id = id as i64;
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
                let last = res.last_insert_id();
                // Drain any result set (a SELECT run through exec() has rows; DDL
                // and DML have none) so the connection is ready for the next call.
                for row in res.by_ref() {
                    if row.is_err() {
                        break;
                    }
                }
                let affected = res.affected_rows() as i64;
                Ok((affected, last))
            }
            Err(e) => Err(e),
        };
        match outcome {
            Ok((affected, last)) => {
                self.note_last_id(last);
                self.changes = affected;
                self.errcode = 0;
                affected
            }
            Err(e) => {
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
                self.errmsg = e.to_string();
                self.errcode = err_code(&e);
                0
            }
        }
    }

    /// Returns the last inserted AUTO_INCREMENT id. MySQL ignores the sequence
    /// name argument (it is a PostgreSQL/Oracle concept).
    pub fn last_insert_id(&self, _name: Option<&str>) -> i64 {
        self.last_id
    }

    /// Prepares a statement: translates placeholders and prepares it server-side
    /// for column metadata. Returns the statement or an error message.
    pub fn prepare(&mut self, sql: &str) -> Result<MyStmt, String> {
        let (translated, named_map, order) = translate_placeholders(sql);
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
                    .map(|c| ColKind::from_column_type(c.column_type()))
                    .collect();
                // Distinct slots run 1..=N contiguously, so the highest slot in
                // `order` is the bound-value count.
                let n_binds = order.iter().copied().max().unwrap_or(0) as usize;
                Ok(MyStmt {
                    conn_id: 0,
                    statement,
                    named_map,
                    order,
                    binds: vec![Bind::Null; n_binds],
                    col_names,
                    col_kinds,
                    rows: Vec::new(),
                    cursor: -1,
                    executed: false,
                })
            }
            Err(e) => {
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
fn format_date(
    kind: ColKind,
    y: u16,
    mo: u8,
    d: u8,
    h: u8,
    mi: u8,
    s: u8,
    us: u32,
) -> String {
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
        Value::UInt(u) => Cell::Int(u as i64),
        Value::Float(f) => Cell::Float(f as f64),
        Value::Double(d) => Cell::Float(d),
        // Text, VARCHAR, DECIMAL, BLOB, etc. all arrive as raw bytes.
        Value::Bytes(b) => Cell::Text(String::from_utf8_lossy(&b).into_owned()),
        Value::Date(y, mo, d, h, mi, s, us) => Cell::Text(format_date(kind, y, mo, d, h, mi, s, us)),
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
            })
            .collect()
    }

    /// Executes the query (once) and materializes the result set into decoded
    /// cells. Records the affected row count and last insert id on the connection.
    fn execute(&mut self, conn: &mut MyConn) -> Result<(), i64> {
        let values = self.build_values();
        let statement = self.statement.clone();
        // A statement with result columns is a SELECT-style query; otherwise it is
        // DML/DDL whose affected-row count and insert id we record.
        let is_select = !self.col_kinds.is_empty();
        let col_kinds = self.col_kinds.clone();
        let outcome: Result<(i64, Option<u64>, Vec<Vec<Cell>>), mysql::Error> = (|| {
            let mut res = conn.conn.exec_iter(&statement, values)?;
            let last = res.last_insert_id();
            let mut rows = Vec::new();
            if is_select {
                for row in res.by_ref() {
                    rows.push(decode_row(row?.unwrap(), &col_kinds));
                }
            }
            let affected = res.affected_rows() as i64;
            drop(res);
            Ok((affected, last, rows))
        })();
        match outcome {
            Ok((affected, last, rows)) => {
                conn.changes = if is_select { rows.len() as i64 } else { affected };
                conn.note_last_id(last);
                conn.errcode = 0;
                self.rows = rows;
                self.executed = true;
                Ok(())
            }
            Err(e) => {
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
    pub fn column_count(&self) -> i64 {
        self.col_names.len() as i64
    }

    /// Name of result column `i` (0-based).
    pub fn column_name(&self, i: i64) -> String {
        self.col_names.get(i as usize).cloned().unwrap_or_default()
    }

    /// SQLite-compatible type code for the current row's column `i`:
    /// 1=int, 2=float, 3=text, 5=null.
    pub fn column_type(&self, i: i64) -> i64 {
        match self.cell(i) {
            Some(Cell::Int(_)) => 1,
            Some(Cell::Float(_)) => 2,
            Some(Cell::Text(_)) => 3,
            _ => 5,
        }
    }

    /// Current row's column `i` as an integer.
    pub fn column_int(&self, i: i64) -> i64 {
        match self.cell(i) {
            Some(Cell::Int(v)) => *v,
            Some(Cell::Float(v)) => *v as i64,
            Some(Cell::Text(s)) => s.trim().parse().unwrap_or(0),
            _ => 0,
        }
    }

    /// Current row's column `i` as a double.
    pub fn column_double(&self, i: i64) -> f64 {
        match self.cell(i) {
            Some(Cell::Float(v)) => *v,
            Some(Cell::Int(v)) => *v as f64,
            Some(Cell::Text(s)) => s.trim().parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    /// Current row's column `i` as text.
    pub fn column_text(&self, i: i64) -> String {
        match self.cell(i) {
            Some(Cell::Text(s)) => s.clone(),
            Some(Cell::Int(v)) => v.to_string(),
            Some(Cell::Float(v)) => v.to_string(),
            _ => String::new(),
        }
    }
}
