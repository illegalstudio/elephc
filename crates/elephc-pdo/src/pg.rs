//! Purpose:
//! The PostgreSQL driver for the elephc PDO bridge. Connects with the pure-Rust
//! synchronous `postgres` client (no system libpq), so compiled PHP binaries
//! stay standalone and talk to a running PostgreSQL server over the network.
//!
//! Called from:
//! - `crate::lib`'s `elephc_pdo_*` C-ABI functions, after matching the
//!   connection/statement's driver to `Conn::Postgres` / `Stmt::Postgres`.
//!
//! Key details:
//! - PDO `?` / `:name` placeholders are translated to PostgreSQL's `$1, $2, …`
//!   at prepare time (respecting `'…'` string literals and the `::type` cast
//!   operator); the named map lets `bind_parameter_index(":name")` resolve.
//! - A statement is prepared server-side for column metadata, then executed
//!   lazily on the first `step()`. The whole result set is materialized into
//!   typed `Cell` values, so the column accessors read from owned data and
//!   per-value NULL is reported through the SQLite-compatible type codes
//!   (1=int, 2=float, 3=text, 4=bytea/blob, 5=null).
//! - Parameter values are encoded according to the prepared statement's inferred
//!   parameter types, so an int bound where the column is `int4` is sent as a
//!   4-byte int, a text where the column is `int` is parsed, etc.

use std::collections::HashMap;

use postgres::types::{to_sql_checked, IsNull, ToSql, Type};
use postgres::{Client, NoTls, Row, Statement};

/// One materialized column value, already decoded to a PHP-friendly scalar.
pub enum Cell {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

/// A pending bound parameter value (before it is encoded for the inferred
/// PostgreSQL parameter type at execute time).
#[derive(Clone)]
pub enum Bind {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
}

/// A live PostgreSQL connection plus the last operation's bookkeeping that PDO
/// reads back (`rowCount`, `lastInsertId`, `errorCode`/`errorInfo`).
pub struct PgConn {
    pub client: Client,
    pub changes: i64,
    pub errmsg: String,
    pub errcode: i64,
}

/// A live PostgreSQL prepared statement and its lazily-materialized result.
pub struct PgStmt {
    pub conn_id: i64,
    pub statement: Statement,
    /// Maps a bare named placeholder (`name` from `:name`) to its 1-based index.
    pub named_map: HashMap<String, i64>,
    /// Bound parameter values, indexed by 0-based position (`$1` → index 0).
    pub binds: Vec<Bind>,
    /// Result column names, available from the prepare (before execution).
    pub col_names: Vec<String>,
    /// Materialized rows; each row is a vector of decoded column cells.
    pub rows: Vec<Vec<Cell>>,
    /// Current 0-based row index; `-1` before the first `step()`.
    pub cursor: isize,
    /// Whether the query has been executed (results materialized) yet.
    pub executed: bool,
}

/// Encodes a pending `Bind` according to the inferred PostgreSQL parameter type,
/// so the value crosses the wire in the format the server expects.
struct Param {
    bind: Bind,
    ty: Type,
}

impl std::fmt::Debug for Param {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Param({:?})", self.ty)
    }
}

impl ToSql for Param {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut postgres::types::private::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        if let Bind::Null = self.bind {
            return Ok(IsNull::Yes);
        }
        // PDO/PHP sends parameters as text and lets the server coerce them to the
        // column type. We replicate that: take the bound value's canonical string
        // form and re-encode it for the parameter type the prepared statement
        // inferred. A value that cannot parse into the target type surfaces as a
        // query error (an unparseable timestamp, etc.).
        let s = match &self.bind {
            Bind::Int(v) => v.to_string(),
            Bind::Float(v) => v.to_string(),
            Bind::Text(t) => t.clone(),
            Bind::Null => unreachable!(),
        };
        let st = s.trim();
        match *ty {
            Type::BOOL => matches!(st, "1" | "t" | "true" | "TRUE" | "on").to_sql(ty, out),
            Type::INT2 => st.parse::<i16>()?.to_sql(ty, out),
            Type::INT4 => st.parse::<i32>()?.to_sql(ty, out),
            Type::INT8 | Type::OID => st.parse::<i64>()?.to_sql(ty, out),
            Type::FLOAT4 => st.parse::<f32>()?.to_sql(ty, out),
            Type::FLOAT8 => st.parse::<f64>()?.to_sql(ty, out),
            Type::NUMERIC => st.parse::<rust_decimal::Decimal>()?.to_sql(ty, out),
            Type::DATE => st.parse::<chrono::NaiveDate>()?.to_sql(ty, out),
            Type::TIME => st.parse::<chrono::NaiveTime>()?.to_sql(ty, out),
            Type::TIMESTAMP => parse_naive_datetime(st)?.to_sql(ty, out),
            Type::TIMESTAMPTZ => parse_datetime_utc(st)?.to_sql(ty, out),
            Type::UUID => st.parse::<uuid::Uuid>()?.to_sql(ty, out),
            Type::JSON | Type::JSONB => {
                serde_json::from_str::<serde_json::Value>(&s)?.to_sql(ty, out)
            }
            // Text family and anything else: send the text and let the server
            // parse it (the `accepts` override below allows the unknown type).
            _ => s.to_sql(ty, out),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        // The value is re-encoded for whatever type the prepared statement
        // inferred for this parameter, so accept every target type.
        true
    }

    to_sql_checked!();
}

/// Parses a `timestamp` text value (`YYYY-MM-DD HH:MM:SS[.ffffff]`, with a space
/// or `T` separator) into a `NaiveDateTime` for binding.
fn parse_naive_datetime(s: &str) -> Result<chrono::NaiveDateTime, chrono::ParseError> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f"))
}

/// Parses a `timestamptz` text value into a UTC `DateTime` for binding. Accepts a
/// trailing offset (`+00`, `+00:00`, `Z`); a value with no offset is taken as UTC.
fn parse_datetime_utc(
    s: &str,
) -> Result<chrono::DateTime<chrono::Utc>, Box<dyn std::error::Error + Sync + Send>> {
    use chrono::TimeZone;
    if let Ok(dt) = chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%#z") {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(dt) = chrono::DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%#z") {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    let naive = parse_naive_datetime(s)?;
    Ok(chrono::Utc.from_utc_datetime(&naive))
}

/// Parses a PDO `pgsql:` DSN (semicolon-separated `key=value` pairs) into a
/// libpq-style connection string the `postgres` client accepts. Recognises the
/// PDO key `dbname` as-is and passes other keys (`host`, `port`, `user`,
/// `password`, `sslmode`, …) straight through. Returns an error for a DSN
/// without the `pgsql:` prefix.
pub fn parse_dsn(dsn: &str) -> Result<String, String> {
    let body = dsn
        .strip_prefix("pgsql:")
        .ok_or_else(|| "could not find driver (expected a pgsql: DSN)".to_string())?;
    let mut parts: Vec<String> = Vec::new();
    for pair in body.split(';') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        // libpq connection strings quote values containing spaces/specials; a
        // simple single-quote wrap with backslash-escaping is sufficient here.
        let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
        parts.push(format!("{}='{}'", key, escaped));
    }
    if parts.is_empty() {
        return Err("empty pgsql DSN".to_string());
    }
    Ok(parts.join(" "))
}

/// Translates PDO `?` and `:name` placeholders to PostgreSQL `$N`, returning the
/// rewritten SQL and the bare-name → 1-based-index map. Single-quoted string
/// literals are passed through untouched, and the `::type` cast operator is not
/// mistaken for a named placeholder.
pub fn translate_placeholders(sql: &str) -> (String, HashMap<String, i64>) {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut named: HashMap<String, i64> = HashMap::new();
    let mut next_index: i64 = 1;
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
                out.push('$');
                out.push_str(&next_index.to_string());
                next_index += 1;
                i += 1;
            }
            ':' => {
                // `::` is the cast operator, not a named placeholder.
                if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                    out.push_str("::");
                    i += 2;
                    continue;
                }
                // Read the placeholder name (identifier chars after the colon).
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
                let index = *named.entry(name.to_string()).or_insert_with(|| {
                    let idx = next_index;
                    next_index += 1;
                    idx
                });
                out.push('$');
                out.push_str(&index.to_string());
                i = j;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, named)
}

impl PgConn {
    /// Connects to PostgreSQL for a `pgsql:` DSN. Returns the connection or an
    /// error message for `last_open_error`.
    pub fn open(dsn: &str) -> Result<PgConn, String> {
        let conn_str = parse_dsn(dsn)?;
        let client = Client::connect(&conn_str, NoTls).map_err(|e| e.to_string())?;
        Ok(PgConn {
            client,
            changes: 0,
            errmsg: String::new(),
            errcode: 0,
        })
    }

    /// Records an error message + a generic non-zero code, returning `-1`.
    fn fail(&mut self, e: postgres::Error) -> i64 {
        self.errmsg = e.to_string();
        self.errcode = 1;
        -1
    }

    /// Runs a statement with no result rows (`PDO::exec`), returning the affected
    /// row count or `-1`.
    pub fn exec(&mut self, sql: &str) -> i64 {
        // execute() runs a single command; fall back to batch_execute for
        // multi-statement scripts (returning 0 affected, as PHP does for those).
        match self.client.execute(sql, &[]) {
            Ok(n) => {
                self.changes = n as i64;
                self.errcode = 0;
                n as i64
            }
            Err(_) => match self.client.batch_execute(sql) {
                Ok(()) => {
                    self.changes = 0;
                    self.errcode = 0;
                    0
                }
                Err(e) => self.fail(e),
            },
        }
    }

    /// Runs a bare transaction-control statement, returning `1`/`0`.
    pub fn exec_simple(&mut self, sql: &str) -> i64 {
        match self.client.batch_execute(sql) {
            Ok(()) => 1,
            Err(e) => {
                self.errmsg = e.to_string();
                self.errcode = 1;
                0
            }
        }
    }

    /// Returns the last inserted id: `currval('name')` when a sequence name is
    /// given, else `lastval()` for the session. Returns `0` on error.
    pub fn last_insert_id(&mut self, name: Option<&str>) -> i64 {
        let sql = match name {
            Some(n) if !n.is_empty() => {
                format!("SELECT currval('{}')", n.replace('\'', "''"))
            }
            _ => "SELECT lastval()".to_string(),
        };
        match self.client.query_one(&sql, &[]) {
            Ok(row) => row.try_get::<_, i64>(0).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Prepares a statement: translates placeholders and prepares it server-side
    /// for column metadata. Returns the statement or an error message.
    pub fn prepare(&mut self, sql: &str) -> Result<PgStmt, String> {
        let (translated, named_map) = translate_placeholders(sql);
        match self.client.prepare(&translated) {
            Ok(statement) => {
                let col_names = statement
                    .columns()
                    .iter()
                    .map(|c| c.name().to_string())
                    .collect();
                let n_params = statement.params().len();
                Ok(PgStmt {
                    conn_id: 0,
                    statement,
                    named_map,
                    binds: vec![Bind::Null; n_params],
                    col_names,
                    rows: Vec::new(),
                    cursor: -1,
                    executed: false,
                })
            }
            Err(e) => {
                self.errmsg = e.to_string();
                self.errcode = 1;
                Err(e.to_string())
            }
        }
    }
}

impl PgStmt {
    /// Resolves a named placeholder to its 1-based index (0 if unknown). The
    /// leading colon is optional.
    pub fn bind_parameter_index(&self, name: &str) -> i64 {
        let bare = name.strip_prefix(':').unwrap_or(name);
        self.named_map.get(bare).copied().unwrap_or(0)
    }

    /// Stores a bound value at the 1-based placeholder `idx`. Returns `1`/`0`.
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

    /// Executes the query (once) and materializes the result set into decoded
    /// cells. Sets `conn.changes` for non-result statements.
    fn execute(&mut self, conn: &mut PgConn) -> Result<(), i64> {
        let param_types: Vec<Type> = self.statement.params().to_vec();
        let params: Vec<Param> = self
            .binds
            .iter()
            .zip(param_types.into_iter())
            .map(|(bind, ty)| Param {
                bind: bind.clone(),
                ty,
            })
            .collect();
        let refs: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

        if self.statement.columns().is_empty() {
            // No result columns: a DML/DDL statement. Run it for the row count.
            match conn.client.execute(&self.statement, &refs) {
                Ok(n) => {
                    conn.changes = n as i64;
                    conn.errcode = 0;
                    self.executed = true;
                    Ok(())
                }
                Err(e) => {
                    conn.errmsg = e.to_string();
                    conn.errcode = 1;
                    Err(-1)
                }
            }
        } else {
            match conn.client.query(&self.statement, &refs) {
                Ok(rows) => {
                    self.rows = rows.iter().map(|r| decode_row(r)).collect();
                    conn.changes = self.rows.len() as i64;
                    conn.errcode = 0;
                    self.executed = true;
                    Ok(())
                }
                Err(e) => {
                    conn.errmsg = e.to_string();
                    conn.errcode = 1;
                    Err(-1)
                }
            }
        }
    }

    /// Advances to the next row: `1` for a row, `0` when exhausted, `-1` on
    /// error. Executes lazily on the first call.
    pub fn step(&mut self, conn: &mut PgConn) -> i64 {
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
    /// 1=int, 2=float, 3=text, 4=bytea/blob, 5=null.
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

    /// Current row's column `i` as text.
    pub fn column_text(&self, i: i64) -> String {
        match self.cell(i) {
            Some(Cell::Text(s)) => s.clone(),
            Some(Cell::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
            Some(Cell::Int(v)) => v.to_string(),
            Some(Cell::Float(v)) => v.to_string(),
            _ => String::new(),
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

/// Decodes a result row's columns into PHP-friendly `Cell` scalars, mapping each
/// PostgreSQL type to int/float/text and NULLs to `Cell::Null`. Types without a
/// direct scalar decoding (e.g. arrays) fall back to a text attempt, then null.
fn decode_row(row: &Row) -> Vec<Cell> {
    (0..row.len())
        .map(|i| {
            let ty = row.columns()[i].type_();
            match *ty {
                Type::BOOL => row
                    .get::<_, Option<bool>>(i)
                    .map(|b| Cell::Int(b as i64))
                    .unwrap_or(Cell::Null),
                Type::INT2 => row
                    .get::<_, Option<i16>>(i)
                    .map(|v| Cell::Int(v as i64))
                    .unwrap_or(Cell::Null),
                Type::INT4 => row
                    .get::<_, Option<i32>>(i)
                    .map(|v| Cell::Int(v as i64))
                    .unwrap_or(Cell::Null),
                Type::INT8 => row
                    .get::<_, Option<i64>>(i)
                    .map(Cell::Int)
                    .unwrap_or(Cell::Null),
                Type::OID => row
                    .get::<_, Option<u32>>(i)
                    .map(|v| Cell::Int(v as i64))
                    .unwrap_or(Cell::Null),
                Type::FLOAT4 => row
                    .get::<_, Option<f32>>(i)
                    .map(|v| Cell::Float(v as f64))
                    .unwrap_or(Cell::Null),
                Type::FLOAT8 => row
                    .get::<_, Option<f64>>(i)
                    .map(Cell::Float)
                    .unwrap_or(Cell::Null),
                Type::TEXT
                | Type::VARCHAR
                | Type::BPCHAR
                | Type::NAME
                | Type::CHAR
                | Type::UNKNOWN => row
                    .get::<_, Option<String>>(i)
                    .map(Cell::Text)
                    .unwrap_or(Cell::Null),
                Type::BYTEA => row
                    .get::<_, Option<Vec<u8>>>(i)
                    .map(Cell::Bytes)
                    .unwrap_or(Cell::Null),
                // numeric/decimal: returned as a string to preserve precision,
                // matching PHP's PDO_pgsql.
                Type::NUMERIC => row
                    .get::<_, Option<rust_decimal::Decimal>>(i)
                    .map(|d| Cell::Text(d.to_string()))
                    .unwrap_or(Cell::Null),
                // Date/time/timestamp: formatted as PostgreSQL's text output.
                Type::DATE => row
                    .get::<_, Option<chrono::NaiveDate>>(i)
                    .map(|d| Cell::Text(d.format("%Y-%m-%d").to_string()))
                    .unwrap_or(Cell::Null),
                Type::TIME => row
                    .get::<_, Option<chrono::NaiveTime>>(i)
                    .map(|t| Cell::Text(t.format("%H:%M:%S%.f").to_string()))
                    .unwrap_or(Cell::Null),
                Type::TIMESTAMP => row
                    .get::<_, Option<chrono::NaiveDateTime>>(i)
                    .map(|t| Cell::Text(t.format("%Y-%m-%d %H:%M:%S%.f").to_string()))
                    .unwrap_or(Cell::Null),
                Type::TIMESTAMPTZ => row
                    .get::<_, Option<chrono::DateTime<chrono::Utc>>>(i)
                    .map(|t| Cell::Text(t.format("%Y-%m-%d %H:%M:%S%.f+00").to_string()))
                    .unwrap_or(Cell::Null),
                // json/jsonb: the JSON value re-serialized as a compact string.
                Type::JSON | Type::JSONB => row
                    .get::<_, Option<serde_json::Value>>(i)
                    .map(|v| Cell::Text(v.to_string()))
                    .unwrap_or(Cell::Null),
                Type::UUID => row
                    .get::<_, Option<uuid::Uuid>>(i)
                    .map(|u| Cell::Text(u.to_string()))
                    .unwrap_or(Cell::Null),
                // Any other type (arrays, bytea, network types, …): best-effort
                // text read, else null. Read these with an explicit `::text` cast.
                _ => match row.try_get::<_, Option<String>>(i) {
                    Ok(Some(s)) => Cell::Text(s),
                    _ => Cell::Null,
                },
            }
        })
        .collect()
}
