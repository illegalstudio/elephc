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
//!   at prepare time by a scanner that skips `--`/`/* */` comments, `'…'`
//!   (incl. `E'…'` backslash-escape strings) and `"…"` literals, `$tag$…$tag$`
//!   dollar-quoted strings, the `::type` cast operator, and the `??` jsonb
//!   operator escape, so a `?`/`:name` inside any of those is never mistaken
//!   for a real placeholder; the named map lets `bind_parameter_index(":name")`
//!   resolve. A SQL text mixing `?` and `:name` is rejected at `prepare()` with
//!   `HY093` (PDO forbids the combination).
//! - A statement is prepared server-side for column metadata, then executed
//!   lazily on the first `step()`. Buffered statements retain typed `Cell` rows;
//!   native prefetch-off statements move the client into a demand worker and
//!   retain only the current row.
//! - Parameter values are encoded according to the prepared statement's inferred
//!   parameter types, so an int bound where the column is `int4` is sent as a
//!   4-byte int, a text where the column is `int` is parsed, etc.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use postgres::types::{to_sql_checked, IsNull, Kind, ToSql, Type};
use postgres::{Client, Config, NoTls, Row, SimpleQueryMessage, Statement};

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
    /// Raw bytes, bound directly (bypassing the text re-encoding `Param::to_sql`
    /// otherwise does) so a BLOB-style parameter round-trips embedded NUL bytes
    /// and arbitrary binary content unchanged.
    Bytes(Vec<u8>),
}

/// A live PostgreSQL connection plus the last operation's bookkeeping that PDO
/// reads back (`rowCount`, `lastInsertId`, `errorCode`/`errorInfo`).
pub struct PgConn {
    client: PgClientSlot,
    pub changes: i64,
    pub errmsg: String,
    /// Native (driver-specific) error code for the connection's last operation, read
    /// back as `errorInfo()[1]`: `0` on success, [`PG_NATIVE_ERRCODE`] on failure.
    /// PostgreSQL has no integer error code — see that constant for the full rationale.
    pub errcode: i64,
    /// 5-char SQLSTATE for the connection's last operation, taken from the
    /// server's `ErrorResponse` (`tokio_postgres::error::Error::code`), which
    /// already parses the wire protocol's `SQLSTATE` field ('C' code). "00000" on
    /// success; "HY000" for an error that carries no SQLSTATE (a transport/
    /// connection failure rather than a server-reported error).
    pub sqlstate: String,
    /// Buffer of server NOTICE message texts captured during query execution,
    /// backing `Pdo\Pgsql::setNoticeCallback()`. The `postgres` client's connection
    /// task invokes the `Config::notice_callback` for every `AsyncMessage::Notice`;
    /// that closure pushes the message here, and the prelude drains this buffer after
    /// each `exec()`/`query()` and dispatches each message to the PHP callback. Shared
    /// (`Arc<Mutex>`) because the callback fires from the client's connection driver,
    /// which may run on a separate thread from the query call.
    pub notices: Arc<Mutex<VecDeque<String>>>,
    /// Default `PDO::ATTR_PREFETCH` state snapshotted by prepared statements.
    pub prefetch: bool,
    /// Monotonic query generation used to invalidate an older unbuffered cursor
    /// when PostgreSQL starts another query on the same connection.
    query_generation: u64,
    /// Demand-driven native row stream currently borrowing this connection.
    active_stream: Option<PgActiveStream>,
    /// Monotonic identity used to distinguish an invalidated older statement.
    next_stream_id: u64,
    /// Transaction state updated from every successful bridge-owned command.
    pub in_transaction: bool,
}

/// A live PostgreSQL prepared statement and its buffered or demand-driven result.
pub struct PgStmt {
    pub conn_id: i64,
    /// Original SQL retained for statement-level diagnostics.
    pub query_string: String,
    pub statement: Option<Statement>,
    /// SQL with PDO placeholders translated to PostgreSQL `$N` markers.
    emulated_sql: Option<String>,
    /// Generated marker byte ranges and their 1-based bind indexes.
    emulated_markers: Vec<(usize, usize, usize)>,
    /// Most recent client-rendered SQL, exposed by `debugDumpParams()`.
    pub sent_sql: String,
    /// Maps a bare named placeholder (`name` from `:name`) to its 1-based index.
    pub named_map: HashMap<String, i64>,
    /// Bound parameter values, indexed by 0-based position (`$1` → index 0).
    pub binds: Vec<Bind>,
    /// Whether each slot was explicitly supplied for the current execution.
    bound: Vec<bool>,
    /// Result column names, available from the prepare (before execution).
    pub col_names: Vec<String>,
    /// Source table names resolved from each column's PostgreSQL table OID.
    col_tables: Vec<String>,
    /// Buffered rows, or the single active row for a native unbuffered stream.
    pub rows: Vec<Vec<Cell>>,
    /// Current 0-based row index; `-1` before the first `step()`.
    pub cursor: isize,
    /// Whether the query has been executed yet.
    pub executed: bool,
    /// Whether this statement buffers its full result (`ATTR_PREFETCH != 0`).
    pub buffered: bool,
    /// Query generation assigned when an unbuffered execution starts.
    query_generation: u64,
    /// Connection-owned demand stream used when `ATTR_PREFETCH` disables buffering.
    stream_id: Option<u64>,
}

impl Drop for PgConn {
    /// Stops any active row worker before the owning PDO connection is released.
    fn drop(&mut self) {
        self.finish_active_stream();
    }
}

/// Keeps the synchronous client optional while a streaming worker temporarily owns it.
struct PgClientSlot(Option<Client>);

impl Deref for PgClientSlot {
    type Target = Client;

    /// Borrows the connected client; streaming callers recover it before other operations.
    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .expect("PostgreSQL client is owned by an active row stream")
    }
}

impl DerefMut for PgClientSlot {
    /// Mutably borrows the connected client outside an active row stream.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
            .as_mut()
            .expect("PostgreSQL client is owned by an active row stream")
    }
}

/// Commands sent to the worker so it reads at most one wire row per PDO fetch.
enum PgStreamCommand {
    Next,
    Close,
}

/// Results returned by a PostgreSQL row-stream worker.
enum PgStreamResponse {
    Started,
    Row(Vec<Cell>),
    Finished(Client),
    Failed(Client, String, String),
}

/// Connection-owned control plane for one active unbuffered statement.
struct PgActiveStream {
    id: u64,
    commands: mpsc::Sender<PgStreamCommand>,
    responses: mpsc::Receiver<PgStreamResponse>,
    worker: Option<JoinHandle<()>>,
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
        if let Bind::Bytes(b) = &self.bind {
            // Raw bytes bind directly regardless of the inferred parameter type
            // (calling `to_sql` rather than `to_sql_checked` skips the `accepts`
            // gate), so a BLOB parameter's embedded NUL / non-UTF-8 bytes reach
            // the server unchanged instead of going through the text re-encoding
            // below.
            return b.to_sql(ty, out);
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
            Bind::Bytes(_) => unreachable!("handled above"),
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
/// `password`, …) straight through — including `connect_timeout` (P2-1: the
/// prelude folds this in from `PDO::ATTR_TIMEOUT` alongside the credentials, and
/// libpq's own conninfo parser already understands the key, so no bridge-side
/// special-casing is needed here). The TLS keys (`sslmode`, `sslrootcert`,
/// `sslcert`, `sslkey`) are deliberately NOT forwarded: tokio-postgres's
/// connection-string parser only accepts `sslmode=disable|prefer|require` (it
/// rejects libpq's `verify-ca`/`verify-full`) and rejects the file-path keys
/// outright, so [`parse_tls`] extracts them and `open` applies them to the
/// `Config`/rustls connector instead. Returns an error for a DSN without the
/// `pgsql:` prefix.
///
/// [`resolve_dsn_options`] first expands libpq service/passfile/environment
/// sources and compatibility aliases. This function then forwards only keys
/// `postgres::Config` recognizes, while client encoding and rustls-specific TLS
/// controls are consumed separately. GSS, CRL, replication/authentication modes,
/// and typos remain explicit errors rather than silently ignored options.
///
/// F-PG-03 / F-CORE-10: when neither the DSN body nor the caller's
/// `PDO::ATTR_TIMEOUT` (which the prelude folds into the DSN as
/// `;connect_timeout=<secs>`, so both arrive here as the same key) supplies a
/// `connect_timeout`, one of 30 s is appended. php-src's pgsql handle factory
/// does the same (`pgsql_driver.c:1350,1373,1381` default `connect_timeout = 30`
/// and always append it to the conninfo), so every real-PHP pg connection is
/// bounded; without it the pure-Rust `postgres` client has no application-level
/// connect timeout and hangs for minutes on a black-holed host. php-src's *quirk*
/// of overwriting a DSN-supplied `connect_timeout=` with its own value is
/// deliberately NOT imitated: a value the DSN spells out wins, and the default
/// only fills the gap when nothing else did.
#[cfg(test)]
pub fn parse_dsn(dsn: &str) -> Result<String, String> {
    let options = resolve_dsn_options(dsn)?;
    parse_resolved_dsn(&options)
}

/// Resolves libpq-compatible service, environment, and password-file sources.
///
/// Precedence matches libpq: explicit PDO DSN values win over a selected service,
/// service values win over `PG*` environment defaults, and a password file is
/// consulted only when no password was supplied by a higher-priority source.
fn resolve_dsn_options(dsn: &str) -> Result<BTreeMap<String, String>, String> {
    let body = dsn
        .strip_prefix("pgsql:")
        .ok_or_else(|| "could not find driver (expected a pgsql: DSN)".to_string())?;
    let mut explicit = parse_option_pairs(body, "PostgreSQL DSN")?;
    for key in ["user", "password"] {
        if let Some(value) = explicit.get_mut(key) {
            *value = percent_decode_credential(value);
        }
    }

    let service_name = explicit
        .get("service")
        .cloned()
        .or_else(|| std::env::var("PGSERVICE").ok().filter(|value| !value.is_empty()));
    let service_file = explicit
        .get("servicefile")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PGSERVICEFILE").map(PathBuf::from))
        .or_else(default_service_file);

    let mut options = if let Some(service_name) = service_name {
        let path = service_file.ok_or_else(|| {
            format!("PostgreSQL service '{service_name}' requested but no service file is available")
        })?;
        load_service(&path, &service_name)?
    } else {
        BTreeMap::new()
    };

    for (key, environment) in pg_environment_keys() {
        if !options.contains_key(*key) {
            if let Ok(value) = std::env::var(environment) {
                if !value.is_empty() {
                    options.insert((*key).to_string(), value);
                }
            }
        }
    }
    if !options.contains_key("user") && !explicit.contains_key("user") {
        if let Some(user) = std::env::var("USER")
            .ok()
            .or_else(|| std::env::var("LOGNAME").ok())
            .filter(|value| !value.is_empty())
        {
            options.insert("user".to_string(), user);
        }
    }
    options.extend(explicit);
    options.remove("service");
    options.remove("servicefile");

    if !options.contains_key("application_name") {
        if let Some(value) = options.remove("fallback_application_name") {
            options.insert("application_name".to_string(), value);
        }
    } else {
        options.remove("fallback_application_name");
    }
    if !options.contains_key("sslmode") {
        if matches!(options.get("requiressl").map(String::as_str), Some("1")) {
            options.insert("sslmode".to_string(), "require".to_string());
        }
    }
    if let Some(value) = options.get("requiressl") {
        if !matches!(value.as_str(), "0" | "1") {
            return Err(format!("invalid PostgreSQL requiressl value '{value}': expected 0 or 1"));
        }
    }
    options.remove("requiressl");
    if let Some(value) = options.get("sslcompression") {
        if !matches!(value.as_str(), "0" | "1") {
            return Err(format!(
                "invalid PostgreSQL sslcompression value '{value}': expected 0 or 1"
            ));
        }
    }
    options.remove("sslcompression");
    apply_default_tls_files(&mut options);

    if !options.contains_key("password") {
        if let Some(path) = options
            .get("passfile")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("PGPASSFILE").map(PathBuf::from))
            .or_else(default_password_file)
        {
            if let Some(password) = password_from_file(&path, &options)? {
                options.insert("password".to_string(), password);
            }
        }
    }
    options.remove("passfile");
    Ok(options)
}

/// Applies libpq's conventional per-user certificate, key, root, and CRL paths
/// when the corresponding option was not supplied explicitly.
fn apply_default_tls_files(options: &mut BTreeMap<String, String>) {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let directory = home.join(".postgresql");
    for (key, filename) in [
        ("sslrootcert", "root.crt"),
        ("sslcert", "postgresql.crt"),
        ("sslkey", "postgresql.key"),
        ("sslcrl", "root.crl"),
    ] {
        if options.contains_key(key) {
            continue;
        }
        let path = directory.join(filename);
        if path.is_file() {
            options.insert(key.to_string(), path.display().to_string());
        }
    }
}

/// Parses semicolon-separated `key=value` options with last-value-wins semantics.
fn parse_option_pairs(body: &str, source: &str) -> Result<BTreeMap<String, String>, String> {
    let mut options = BTreeMap::new();
    for pair in body.split(';') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((key, value)) = pair.split_once('=') else {
            return Err(format!("invalid {source} option '{pair}': expected key=value"));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("invalid {source} option '{pair}': empty key"));
        }
        options.insert(key.to_ascii_lowercase(), value.trim().to_string());
    }
    Ok(options)
}

/// Returns the libpq environment variable corresponding to each connection key.
fn pg_environment_keys() -> &'static [(&'static str, &'static str)] {
    &[
        ("host", "PGHOST"),
        ("hostaddr", "PGHOSTADDR"),
        ("port", "PGPORT"),
        ("dbname", "PGDATABASE"),
        ("user", "PGUSER"),
        ("password", "PGPASSWORD"),
        ("application_name", "PGAPPNAME"),
        ("connect_timeout", "PGCONNECT_TIMEOUT"),
        ("client_encoding", "PGCLIENTENCODING"),
        ("options", "PGOPTIONS"),
        ("sslmode", "PGSSLMODE"),
        ("requiressl", "PGREQUIRESSL"),
        ("sslcompression", "PGSSLCOMPRESSION"),
        ("sslrootcert", "PGSSLROOTCERT"),
        ("sslcert", "PGSSLCERT"),
        ("sslkey", "PGSSLKEY"),
        ("sslcertmode", "PGSSLCERTMODE"),
        ("sslpassword", "PGSSLPASSWORD"),
        ("sslcrl", "PGSSLCRL"),
        ("sslcrldir", "PGSSLCRLDIR"),
        ("sslsni", "PGSSLSNI"),
        ("ssl_min_protocol_version", "PGSSLMINPROTOCOLVERSION"),
        ("ssl_max_protocol_version", "PGSSLMAXPROTOCOLVERSION"),
        ("sslnegotiation", "PGSSLNEGOTIATION"),
        ("gssencmode", "PGGSSENCMODE"),
        ("require_auth", "PGREQUIREAUTH"),
        ("passfile", "PGPASSFILE"),
        ("target_session_attrs", "PGTARGETSESSIONATTRS"),
        ("channel_binding", "PGCHANNELBINDING"),
        ("load_balance_hosts", "PGLOADBALANCEHOSTS"),
        ("tcp_user_timeout", "PGTCPUSER_TIMEOUT"),
    ]
}

/// Returns libpq's per-user service-file location for supported Unix targets.
fn default_service_file() -> Option<PathBuf> {
    let user_file = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".pg_service.conf"))
        .filter(|path| path.is_file());
    user_file.or_else(|| {
        std::env::var_os("PGSYSCONFDIR")
            .map(PathBuf::from)
            .map(|directory| directory.join("pg_service.conf"))
            .filter(|path| path.is_file())
    })
}

/// Loads one section from a libpq `pg_service.conf` file.
fn load_service(path: &Path, service_name: &str) -> Result<BTreeMap<String, String>, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("PostgreSQL service file '{}': {error}", path.display()))?;
    let mut selected = false;
    let mut found = false;
    let mut options = BTreeMap::new();
    for (line_number, raw) in contents.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            selected = line[1..line.len() - 1].trim() == service_name;
            found |= selected;
            continue;
        }
        if !selected {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "PostgreSQL service file '{}', line {}: expected key=value",
                path.display(),
                line_number + 1
            ));
        };
        options.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    if !found {
        return Err(format!(
            "PostgreSQL service '{service_name}' was not found in '{}'",
            path.display()
        ));
    }
    options.remove("service");
    options.remove("servicefile");
    Ok(options)
}

/// Returns libpq's per-user password-file location for supported Unix targets.
fn default_password_file() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".pgpass"))
        .filter(|path| path.is_file())
}

/// Finds the first matching `host:port:database:user:password` entry in `.pgpass`.
fn password_from_file(
    path: &Path,
    options: &BTreeMap<String, String>,
) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|error| format!("PostgreSQL passfile '{}': {error}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "PostgreSQL passfile '{}' must not be accessible by group or others",
                path.display()
            ));
        }
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("PostgreSQL passfile '{}': {error}", path.display()))?;
    let raw_host = options
        .get("host")
        .or_else(|| options.get("hostaddr"))
        .map(String::as_str)
        .unwrap_or("localhost");
    let host = if raw_host.starts_with('/') {
        "localhost"
    } else {
        raw_host
    };
    let port = options.get("port").map(String::as_str).unwrap_or("5432");
    let database = options
        .get("dbname")
        .or_else(|| options.get("user"))
        .map(String::as_str)
        .unwrap_or("");
    let user = options.get("user").map(String::as_str).unwrap_or("");
    if host.contains(',') || port.contains(',') {
        return Err(
            "PostgreSQL passfile with multiple hosts or ports cannot be represented by the native client"
                .to_string(),
        );
    }
    for line in contents.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = split_password_line(line)?;
        if password_field_matches(&fields[0], host)
            && password_field_matches(&fields[1], port)
            && password_field_matches(&fields[2], database)
            && password_field_matches(&fields[3], user)
        {
            return Ok(Some(fields[4].clone()));
        }
    }
    Ok(None)
}

/// Splits one `.pgpass` row while honoring backslash-escaped colons and slashes.
fn split_password_line(line: &str) -> Result<Vec<String>, String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            if !matches!(character, ':' | '\\') {
                fields.last_mut().unwrap().push('\\');
            }
            fields.last_mut().unwrap().push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == ':' && fields.len() < 5 {
            fields.push(String::new());
        } else {
            fields.last_mut().unwrap().push(character);
        }
    }
    if escaped {
        fields.last_mut().unwrap().push('\\');
    }
    if fields.len() != 5 {
        return Err("invalid PostgreSQL passfile entry: expected five colon-separated fields".to_string());
    }
    Ok(fields)
}

/// Applies `.pgpass` wildcard matching to one host, port, database, or user field.
fn password_field_matches(pattern: &str, value: &str) -> bool {
    pattern == "*" || pattern == value
}

/// Converts fully resolved options into the subset accepted by `postgres::Config`.
fn parse_resolved_dsn(options: &BTreeMap<String, String>) -> Result<String, String> {
    // php-src's `pgsql_driver.c:1350` default connect timeout, in seconds.
    const DEFAULT_CONNECT_TIMEOUT_SECS: u32 = 30;
    const ACCEPTED_KEYS: &[&str] = &[
        "user",
        "password",
        "dbname",
        "options",
        "application_name",
        "sslnegotiation",
        "host",
        "hostaddr",
        "port",
        "connect_timeout",
        "tcp_user_timeout",
        "keepalives",
        "keepalives_idle",
        "keepalives_interval",
        "keepalives_retries",
        "target_session_attrs",
        "channel_binding",
        "load_balance_hosts",
    ];
    let mut parts: Vec<String> = Vec::new();
    // F-PG-03: tracks whether the caller already bounded the connect (either
    // straight in the DSN or via `ATTR_TIMEOUT`, which the prelude folds into the
    // DSN under the very same key) — if so, that value wins over the 30 s default.
    let mut saw_connect_timeout = false;
    for (key, value) in options {
        // The TLS keys are consumed by `parse_tls`/`open`, not by the libpq
        // connection string: tokio-postgres's parser rejects `sslrootcert`/
        // `sslcert`/`sslkey` and the `verify-ca`/`verify-full` sslmode values, so
        // forwarding any of them would make `.parse::<Config>()` fail.
        if matches!(
            key.as_str(),
            "sslmode"
                | "sslrootcert"
                | "sslcert"
                | "sslkey"
                | "sslcertmode"
                | "sslcrl"
                | "sslcrldir"
                | "sslsni"
                | "ssl_min_protocol_version"
                | "ssl_max_protocol_version"
        ) {
            continue;
        }
        if key == "client_encoding" {
            validate_client_encoding(value)?;
            continue;
        }
        if key == "gssencmode" {
            if value == "disable" {
                continue;
            }
            return Err(format!(
                "unsupported PostgreSQL gssencmode '{value}': the native client has no GSSAPI transport"
            ));
        }
        if !ACCEPTED_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "unsupported PostgreSQL DSN option '{key}': elephc's native client cannot honor its libpq semantics"
            ));
        }
        if key == "connect_timeout" {
            saw_connect_timeout = true;
        }
        // F-CORE-02: the prelude percent-encodes '%' and ';' on a constructor-supplied
        // `user`/`password` value before folding it into the DSN, so it survives the
        // `body.split(';')` above intact instead of truncating at an embedded ';'.
        // Undo that encoding here — and only for these two keys, since every other
        // value is passed straight through byte-identical — before escaping it into
        // the libpq conninfo string below.
        // libpq connection strings quote values containing spaces/specials; a
        // simple single-quote wrap with backslash-escaping is sufficient here.
        let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
        parts.push(format!("{}='{}'", key, escaped));
    }
    // The resolver normally supplies libpq's OS-user default for a bare `pgsql:`.
    // Keep this guard for environments with neither explicit/default user nor any
    // other connection option; the timeout alone is not a usable identity.
    if parts.is_empty() {
        return Err("empty pgsql DSN".to_string());
    }
    // F-PG-03: bound an otherwise unbounded connect at php-src's 30 s (see the
    // doc comment) — only when the caller gave no `connect_timeout` of their own.
    if !saw_connect_timeout {
        parts.push(format!("connect_timeout='{}'", DEFAULT_CONNECT_TIMEOUT_SECS));
    }
    Ok(parts.join(" "))
}

/// Validates a PostgreSQL client-encoding identifier before it is embedded in a
/// post-connect `SET client_encoding` command.
fn validate_client_encoding(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!(
            "invalid PostgreSQL client_encoding '{value}': expected an encoding identifier"
        ));
    }
    Ok(())
}

/// Extracts the optional validated `client_encoding` DSN value.
#[cfg(test)]
fn client_encoding_from_dsn(dsn: &str) -> Result<Option<String>, String> {
    let options = resolve_dsn_options(dsn)?;
    client_encoding_from_options(&options)
}

/// Extracts and validates `client_encoding` from already resolved options.
fn client_encoding_from_options(
    options: &BTreeMap<String, String>,
) -> Result<Option<String>, String> {
    let Some(value) = options.get("client_encoding") else {
        return Ok(None);
    };
    validate_client_encoding(value)?;
    Ok(Some(value.clone()))
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

/// The PostgreSQL TLS parameters carried by a `pgsql:` DSN, extracted separately
/// from the libpq connection string (see [`parse_dsn`]). `mode` mirrors libpq's
/// `sslmode`; the three optional paths mirror libpq's `sslrootcert` (server CA
/// bundle), `sslcert`, and `sslkey` (client-certificate mutual TLS). The path
/// fields are only read when the `tls` feature is compiled in; a
/// `--no-default-features` build still parses them (so the DSN is accepted) but
/// leaves them unused.
#[cfg_attr(not(feature = "tls"), allow(dead_code))]
struct PgTls {
    /// Lowercased `sslmode` value; empty when the DSN omits it (libpq and
    /// tokio-postgres both default to `prefer`).
    mode: String,
    /// `sslrootcert`: a PEM CA bundle the server certificate is verified against.
    /// When absent, the bundled webpki-roots trust anchors are used.
    root_cert: Option<String>,
    /// `sslcert`: the client certificate chain PEM for mutual TLS.
    client_cert: Option<String>,
    /// `sslkey`: the client private-key PEM for mutual TLS.
    client_key: Option<String>,
    /// libpq's policy for presenting a client certificate (`allow|disable|require`).
    client_cert_mode: String,
    /// PEM certificate-revocation-list file.
    crl_file: Option<String>,
    /// Directory whose PEM CRLs are combined for rustls verification.
    crl_directory: Option<String>,
    /// Whether the TLS ClientHello carries the host name (libpq `sslsni`).
    server_name_indication: bool,
    /// Lowest TLS protocol version accepted by libpq-style configuration.
    min_protocol_version: Option<String>,
    /// Highest TLS protocol version accepted by libpq-style configuration.
    max_protocol_version: Option<String>,
}

impl Default for PgTls {
    /// Builds libpq-compatible TLS defaults (`sslmode=prefer`, SNI enabled).
    fn default() -> Self {
        Self {
            mode: String::new(),
            root_cert: None,
            client_cert: None,
            client_key: None,
            client_cert_mode: "allow".to_string(),
            crl_file: None,
            crl_directory: None,
            server_name_indication: true,
            min_protocol_version: None,
            max_protocol_version: None,
        }
    }
}

/// Extracts the TLS parameters from a `pgsql:` DSN (the keys [`parse_dsn`]
/// deliberately drops). Unknown keys are ignored; a DSN without the `pgsql:`
/// prefix yields the default (unset) parameters.
#[cfg(test)]
fn parse_tls(dsn: &str) -> Result<PgTls, String> {
    let options = resolve_dsn_options(dsn)?;
    parse_tls_options(&options)
}

/// Extracts and validates TLS settings from fully resolved connection options.
fn parse_tls_options(options: &BTreeMap<String, String>) -> Result<PgTls, String> {
    let mut tls = PgTls::default();
    if let Some(value) = options.get("sslmode") {
        tls.mode = value.to_ascii_lowercase();
    }
    if !matches!(
        tls.mode.as_str(),
        "" | "disable" | "allow" | "prefer" | "require" | "verify-ca" | "verify-full"
    ) {
        return Err(format!("invalid PostgreSQL sslmode '{}'", tls.mode));
    }
    tls.root_cert = options.get("sslrootcert").cloned();
    tls.client_cert = options.get("sslcert").cloned();
    tls.client_key = options.get("sslkey").cloned();
    tls.crl_file = options.get("sslcrl").cloned();
    tls.crl_directory = options.get("sslcrldir").cloned();
    if let Some(value) = options.get("sslcertmode") {
        tls.client_cert_mode = value.to_ascii_lowercase();
    }
    if !matches!(tls.client_cert_mode.as_str(), "allow" | "disable" | "require") {
        return Err(format!(
            "invalid PostgreSQL sslcertmode '{}'",
            tls.client_cert_mode
        ));
    }
    if tls.client_cert_mode == "require"
        && (tls.client_cert.is_none() || tls.client_key.is_none())
    {
        return Err("PostgreSQL sslcertmode=require needs both sslcert and sslkey".to_string());
    }
    if tls.client_cert_mode != "disable"
        && (tls.client_cert.is_some() != tls.client_key.is_some())
    {
        return Err("PostgreSQL client TLS authentication needs both sslcert and sslkey".to_string());
    }
    if let Some(value) = options.get("sslsni") {
        tls.server_name_indication = match value.as_str() {
            "1" => true,
            "0" => false,
            _ => {
                return Err(format!(
                    "invalid PostgreSQL sslsni value '{value}': expected 0 or 1"
                ))
            }
        };
    }
    tls.min_protocol_version = options.get("ssl_min_protocol_version").cloned();
    tls.max_protocol_version = options.get("ssl_max_protocol_version").cloned();
    validate_tls_protocol_range(&tls)?;
    Ok(tls)
}

/// Validates libpq TLS protocol bounds against rustls's TLS 1.2/1.3 support.
fn validate_tls_protocol_range(tls: &PgTls) -> Result<(), String> {
    let min = tls
        .min_protocol_version
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(tls_protocol_rank)
        .transpose()?;
    let max = tls
        .max_protocol_version
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(tls_protocol_rank)
        .transpose()?;
    if matches!(min, Some(0 | 1)) || matches!(max, Some(0 | 1)) {
        return Err("PostgreSQL TLS 1.0/1.1 cannot be honored by the rustls native client".to_string());
    }
    if matches!((min, max), (Some(min), Some(max)) if min > max) {
        return Err("PostgreSQL ssl_min_protocol_version exceeds ssl_max_protocol_version".to_string());
    }
    Ok(())
}

/// Maps a libpq TLS protocol spelling to an ordered version rank.
fn tls_protocol_rank(value: &str) -> Result<u8, String> {
    match value.to_ascii_uppercase().as_str() {
        "TLSV1" | "TLSV1.0" => Ok(0),
        "TLSV1.1" => Ok(1),
        "TLSV1.2" => Ok(2),
        "TLSV1.3" => Ok(3),
        _ => Err(format!("invalid PostgreSQL TLS protocol version '{value}'")),
    }
}

/// Applies the DSN's `sslmode` to `config` and opens the connection, using rustls
/// (ring provider) when TLS is requested. `disable` connects in plaintext;
/// `prefer` (the default) attempts TLS but allows a plaintext session;
/// `require`/`verify-ca`/`verify-full` demand TLS. The rustls verifier always
/// validates the server certificate against the trust anchors (a stricter, safer
/// default than libpq's bare `require`, which encrypts without verifying);
/// `verify-ca` and `verify-full` therefore behave identically here.
#[cfg(feature = "tls")]
fn connect_tls(config: &mut Config, tls: &PgTls) -> Result<Client, String> {
    use postgres::config::SslMode;
    if tls.mode == "disable" {
        config.ssl_mode(SslMode::Disable);
        return config.connect(NoTls).map_err(|e| e.to_string());
    }
    let require = matches!(tls.mode.as_str(), "require" | "verify-ca" | "verify-full");
    config.ssl_mode(if require {
        SslMode::Require
    } else {
        SslMode::Prefer
    });
    let connector = build_tls_connector(tls)?;
    config.connect(connector).map_err(|e| e.to_string())
}

/// The `--no-default-features` fallback: no TLS backend is linked, so a DSN that
/// *demands* TLS fails loudly rather than silently connecting in plaintext, while
/// `disable`/`prefer`/unset (which tolerate plaintext) still connect.
#[cfg(not(feature = "tls"))]
fn connect_tls(config: &mut Config, tls: &PgTls) -> Result<Client, String> {
    if matches!(tls.mode.as_str(), "require" | "verify-ca" | "verify-full") {
        return Err(format!(
            "pgsql sslmode={} requires TLS, which was not compiled in \
             (rebuild elephc-pdo with its default `tls` feature)",
            tls.mode
        ));
    }
    config.connect(NoTls).map_err(|e| e.to_string())
}

/// Builds a rustls `MakeRustlsConnect` for the pg connection. The `ClientConfig`
/// is built with an explicit ring `CryptoProvider` (`builder_with_provider`), so
/// it never depends on a process-global default provider. When `sslrootcert` is
/// given, only that PEM CA bundle is trusted; otherwise the bundled webpki-roots
/// anchors are used. `sslcert`+`sslkey` (both required together) enable
/// client-certificate mutual TLS.
#[cfg(feature = "tls")]
fn build_tls_connector(tls: &PgTls) -> Result<tokio_postgres_rustls::MakeRustlsConnect, String> {
    use rustls::RootCertStore;
    use std::sync::Arc;

    let mut roots = RootCertStore::empty();
    if let Some(ca) = &tls.root_cert {
        for cert in load_certs(ca, "sslrootcert")? {
            roots
                .add(cert)
                .map_err(|e| format!("sslrootcert {}: {}", ca, e))?;
        }
    } else {
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }

    let min_rank = tls
        .min_protocol_version
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(tls_protocol_rank)
        .transpose()?
        .unwrap_or(2);
    let max_rank = tls
        .max_protocol_version
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(tls_protocol_rank)
        .transpose()?
        .unwrap_or(3);
    let mut protocol_versions: Vec<&'static rustls::SupportedProtocolVersion> = Vec::new();
    if min_rank <= 3 && max_rank >= 3 {
        protocol_versions.push(&rustls::version::TLS13);
    }
    if min_rank <= 2 && max_rank >= 2 {
        protocol_versions.push(&rustls::version::TLS12);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier_roots = Arc::new(roots.clone());
    let builder = rustls::ClientConfig::builder_with_provider(Arc::clone(&provider))
    .with_protocol_versions(&protocol_versions)
    .map_err(|e| e.to_string())?
    .with_root_certificates(roots);

    let mut config = match (
        tls.client_cert_mode.as_str(),
        &tls.client_cert,
        &tls.client_key,
    ) {
        ("disable", _, _) => builder.with_no_client_auth(),
        (_, Some(cert), Some(key)) => {
            let chain = load_certs(cert, "sslcert")?;
            let der = load_private_key(key)?;
            builder
                .with_client_auth_cert(chain, der)
                .map_err(|e| e.to_string())?
        }
        _ => builder.with_no_client_auth(),
    };
    config.enable_sni = tls.server_name_indication;
    let crls = load_crls(tls)?;
    if !crls.is_empty() {
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            verifier_roots,
            provider,
        )
        .with_crls(crls)
        .build()
        .map_err(|error| format!("PostgreSQL TLS CRL configuration: {error}"))?;
        config
            .dangerous()
            .set_certificate_verifier(verifier);
    }
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
}

/// Loads CRLs from `sslcrl` and every regular file/symlink in `sslcrldir`.
#[cfg(feature = "tls")]
fn load_crls(
    tls: &PgTls,
) -> Result<Vec<rustls::pki_types::CertificateRevocationListDer<'static>>, String> {
    let requested = tls.crl_file.is_some() || tls.crl_directory.is_some();
    let mut paths = Vec::new();
    if let Some(path) = &tls.crl_file {
        paths.push(PathBuf::from(path));
    }
    if let Some(directory) = &tls.crl_directory {
        let mut entries = fs::read_dir(directory)
            .map_err(|error| format!("sslcrldir {directory}: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("sslcrldir {directory}: {error}"))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let metadata = entry
                .metadata()
                .map_err(|error| format!("sslcrldir '{}': {error}", entry.path().display()))?;
            if metadata.is_file() {
                paths.push(entry.path());
            }
        }
    }
    let mut output = Vec::new();
    for path in paths {
        let pem = fs::read(&path)
            .map_err(|error| format!("PostgreSQL CRL '{}': {error}", path.display()))?;
        let mut reader = &pem[..];
        for crl in rustls_pemfile::crls(&mut reader) {
            output.push(
                crl.map_err(|error| format!("PostgreSQL CRL '{}': {error}", path.display()))?,
            );
        }
    }
    if requested && output.is_empty() {
        return Err("PostgreSQL TLS CRL configuration contains no PEM CRLs".to_string());
    }
    Ok(output)
}

/// Reads a PEM file into a chain of DER certificates. `label` names the DSN key
/// for error messages (`sslrootcert` / `sslcert`).
#[cfg(feature = "tls")]
fn load_certs(
    path: &str,
    label: &str,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, String> {
    let pem = std::fs::read(path).map_err(|e| format!("{} {}: {}", label, path, e))?;
    let mut reader = &pem[..];
    let mut out = Vec::new();
    for cert in rustls_pemfile::certs(&mut reader) {
        out.push(cert.map_err(|e| format!("{} {}: {}", label, path, e))?);
    }
    if out.is_empty() {
        return Err(format!("{} {}: no certificates found", label, path));
    }
    Ok(out)
}

/// Reads the first PEM private key (PKCS#8, PKCS#1, or SEC1) from `sslkey`.
#[cfg(feature = "tls")]
fn load_private_key(path: &str) -> Result<rustls::pki_types::PrivateKeyDer<'static>, String> {
    let pem = std::fs::read(path).map_err(|e| format!("sslkey {}: {}", path, e))?;
    let mut reader = &pem[..];
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| format!("sslkey {}: {}", path, e))?
        .ok_or_else(|| format!("sslkey {}: no private key found", path))
}

/// Returns whether `b` is an identifier byte (`[A-Za-z0-9_]`), used both to
/// read a placeholder name and to test the "word boundary" before a possible
/// `E'...'`/`e'...'` escape-string prefix.
///
/// Deliberately ASCII-only: php-src's bind-name class really is
/// `BINDCHR = [:][a-zA-Z0-9_]+` (`pdo_sql_parser.re`), so a byte ≥ 0x80 ends a
/// `:name` rather than extending it. The dollar-quote *tag* classes are the wider
/// ones — see [`is_dolq_start`] / [`is_dolq_cont`], which must not be conflated
/// with this.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Returns whether `b` can OPEN a dollar-quote tag, per php-src's pgsql scanner
/// rule `DOLQ_START = [A-Za-z\200-\377_]` (`pgsql_sql_parser.re:32`). The
/// `\200-\377` half (every byte ≥ 0x80) is load-bearing, not decorative:
/// PostgreSQL's own lexer treats multibyte "letters" as identifier characters, so
/// `$café$ ... $café$` is a perfectly valid dollar-quoted string. Gating the tag
/// on `is_ascii_alphabetic()` left such a tag unrecognized, the quote never
/// opened, and the body fell through to the ordinary scanner — which then
/// rewrote any `?`/`:name` inside the *string literal* into a real bind
/// (F-PARSE-02).
fn is_dolq_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

/// Returns whether `b` can CONTINUE a dollar-quote tag, per php-src's
/// `DOLQ_CONT = [A-Za-z\200-\377_0-9]` (`pgsql_sql_parser.re:33`) — [`is_dolq_start`]
/// plus the digits. Every continuation byte of a multi-byte UTF-8 character is
/// itself ≥ 0x80, so a tag scan driven by this predicate always stops on a char
/// boundary and the tag can be sliced back out of the `&str` safely.
fn is_dolq_cont(b: u8) -> bool {
    is_dolq_start(b) || b.is_ascii_digit()
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

/// Scans a PostgreSQL single-quoted string or double-quoted identifier and returns
/// the exclusive end after its closing delimiter. `backslash_escapes` is enabled
/// only for a standalone `E'...'` prefix. `None` preserves php-src's scanner
/// backtracking contract for an unterminated region.
fn scan_pg_quoted_region(
    bytes: &[u8],
    start: usize,
    quote: u8,
    backslash_escapes: bool,
) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        if backslash_escapes && bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
                continue;
            }
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

/// Translates PDO `?` and `:name` placeholders to PostgreSQL `$N`, returning the
/// rewritten SQL, the bare-name → 1-based-index map, and whether the SQL mixed a
/// positional `?` with a named `:name` (PDO forbids this combination; `prepare()`
/// checks the flag and raises `HY093` before ever reaching the server).
///
/// The scanner tracks these mutually exclusive regions, copying each verbatim
/// (never scanning `?`/`:name` inside them) before resuming normal placeholder
/// scanning:
/// - `-- ...` line comments (to end of line or EOF);
/// - `/* ... */` block comments (non-nested, to the first `*/` or EOF);
/// - `'...'` single-quoted strings, with `''` as the doubled-quote escape and,
///   when the string is `E'...'`/`e'...'`-prefixed (a standalone `E`/`e` token,
///   not part of a preceding identifier), `\'`/`\\` backslash escapes active too
///   (a plain `'...'` string only recognizes the `''` doubling, per
///   `standard_conforming_strings`);
/// - `"..."` double-quoted identifiers, with `""` as the doubled-quote escape;
/// - `$tag$...$tag$` dollar-quoted strings (`tag` is empty or matches php-src's
///   `DOLQ_START DOLQ_CONT*` — see [`is_dolq_start`] / [`is_dolq_cont`], which
///   accept non-ASCII bytes, so `$café$...$café$` opens a quote like PostgreSQL's
///   own lexer — and must be followed by `$` to open; a `$` immediately followed
///   by a digit, e.g. a literal `$1` in the input, can never start a tag and is
///   emitted as a plain `$`).
///
/// A `??` (exactly two `?`) is PostgreSQL's jsonb `?`/`?|`/`?&` operator escape:
/// it collapses to a single literal `?` in the output and allocates no
/// placeholder slot. A lone `?` is a real positional placeholder. Symmetrically, a
/// run of two or more `:` — `::`, the cast operator, and any longer run — is a
/// single verbatim text token, never a named placeholder, and is consumed whole:
/// php-src's `MULTICHAR = [:]{2,}` is greedy, so eating colons pairwise would let
/// an odd run's last colon (`:::c`) be re-read as a phantom `:c` bind. `#` is not
/// a comment introducer in PostgreSQL.
///
/// A `:name` immediately preceded by an alphanumeric byte is NOT a named
/// placeholder (matching php-src's `pdo_sql_parser.re`, which skips the same
/// way), most importantly so an array slice like `data[1:5]` is left
/// untouched instead of misreading `:5` as a bind parameter.
fn translate_placeholders_with_markers(
    sql: &str,
) -> (
    String,
    HashMap<String, i64>,
    bool,
    Vec<(usize, usize, usize)>,
) {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut named: HashMap<String, i64> = HashMap::new();
    let mut next_index: i64 = 1;
    let mut i = 0;
    let mut saw_positional = false;
    let mut saw_named = false;
    let mut markers = Vec::new();
    while i < len {
        let c = bytes[i];
        match c {
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                // Line comment: verbatim to the end of the line (exclusive of
                // the newline itself, which the default arm then copies) or EOF.
                let start = i;
                let mut j = i + 2;
                while j < len && bytes[j] != b'\n' {
                    j += 1;
                }
                out.push_str(&sql[start..j]);
                i = j;
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                // Block comment: verbatim to the matching non-nested `*/`.
                // An unterminated opener backtracks to the one-byte fallback,
                // matching php-src's re2c scanner rather than swallowing EOF.
                let start = i;
                let mut j = i + 2;
                while j + 1 < len && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                    j += 1;
                }
                if j + 1 < len {
                    let end = j + 2;
                    out.push_str(&sql[start..end]);
                    i = end;
                } else {
                    out.push('/');
                    i += 1;
                }
            }
            b'"' => {
                if let Some(end) = scan_pg_quoted_region(bytes, i, b'"', false) {
                    out.push_str(&sql[i..end]);
                    i = end;
                } else {
                    out.push('"');
                    i += 1;
                }
            }
            b'\'' => {
                // A standalone `E`/`e` immediately before this quote (not part
                // of a longer identifier) makes this an escape-string.
                let is_e_prefixed = i > 0
                    && (bytes[i - 1] == b'E' || bytes[i - 1] == b'e')
                    && (i < 2 || !is_ident_byte(bytes[i - 2]));
                if let Some(end) = scan_pg_quoted_region(bytes, i, b'\'', is_e_prefixed) {
                    out.push_str(&sql[i..end]);
                    i = end;
                } else {
                    out.push('\'');
                    i += 1;
                }
            }
            b'$' => {
                // A `$` immediately followed by a digit can never open a
                // dollar-quote tag; emit it verbatim (e.g. a literal `$1`).
                if i + 1 < len && bytes[i + 1].is_ascii_digit() {
                    out.push('$');
                    i += 1;
                    continue;
                }
                let mut j = i + 1;
                if j < len && is_dolq_start(bytes[j]) {
                    j += 1;
                    while j < len && is_dolq_cont(bytes[j]) {
                        j += 1;
                    }
                }
                if j < len && bytes[j] == b'$' {
                    // `bytes[i+1..j]` is the (possibly empty) tag; the opening
                    // delimiter closes at `j` (its own `$`).
                    let tag = &sql[i + 1..j];
                    let delim = format!("${}$", tag);
                    let open_end = j + 1;
                    match sql[open_end..].find(&delim) {
                        Some(rel) => {
                            let close_end = open_end + rel + delim.len();
                            out.push_str(&sql[i..close_end]);
                            i = close_end;
                        }
                        None => {
                            // Unterminated dollar-quote: backtrack the opener and
                            // keep scanning its body for placeholders like php-src.
                            out.push('$');
                            i += 1;
                        }
                    }
                } else {
                    // Not a valid tag-open (e.g. a bare `$`); emit verbatim.
                    out.push('$');
                    i += 1;
                }
            }
            b'?' => {
                // `??` is the jsonb operator escape: a single literal `?`, no
                // placeholder slot allocated.
                if i + 1 < len && bytes[i + 1] == b'?' {
                    out.push('?');
                    i += 2;
                    continue;
                }
                let marker_start = out.len();
                out.push('$');
                out.push_str(&next_index.to_string());
                markers.push((marker_start, out.len(), next_index as usize));
                next_index += 1;
                saw_positional = true;
                i += 1;
            }
            b':' => {
                // A run of 2+ `:` (`::`, the cast operator, and any longer run) is a
                // single verbatim text token, never a named placeholder — php-src's
                // `MULTICHAR = [:]{2,}` rule (`pgsql_sql_parser.re:35`) is greedy
                // (re2c's maximal munch swallows the whole contiguous run). The run
                // must be consumed WHOLE: taking colons two at a time leaves the
                // third one of an odd run (`:::c`) to be re-scanned as a fresh `:c`,
                // conjuring a named placeholder php-src never emits.
                let mut run_end = i + 1;
                while run_end < len && bytes[run_end] == b':' {
                    run_end += 1;
                }
                if run_end - i >= 2 {
                    out.push_str(&sql[i..run_end]);
                    i = run_end;
                    continue;
                }
                // Read the placeholder name (identifier chars after the colon).
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
                // NOT alphanumeric (BUG 2). Without this, an array slice like
                // `data[1:5]` misreads `:5` as a named placeholder. Emit the
                // colon verbatim; the identifier bytes are then re-scanned as
                // ordinary text by the default arm on the next iterations.
                if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
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
                let marker_start = out.len();
                out.push('$');
                out.push_str(&index.to_string());
                markers.push((marker_start, out.len(), index as usize));
                saw_named = true;
                i = j;
            }
            _ => {
                // Copy the whole codepoint via a slice (BUG 1): `c as char`
                // would corrupt any multi-byte UTF-8 character (e.g. an
                // embedded `'café'`/`'Zürich'` byte outside a recognized
                // quoted region — the ordinary/unquoted path).
                let n = utf8_len(c).min(len - i);
                out.push_str(&sql[i..i + n]);
                i += n;
            }
        }
    }
    let mixed = saw_positional && saw_named;
    (out, named, mixed, markers)
}

/// Translates PDO placeholders to PostgreSQL `$N` markers for native prepares.
#[cfg(test)]
pub fn translate_placeholders(sql: &str) -> (String, HashMap<String, i64>, bool) {
    let (translated, named, mixed, _) = translate_placeholders_with_markers(sql);
    (translated, named, mixed)
}

/// Renders a PostgreSQL literal for one emulated-prepare bind without allowing
/// value bytes to alter the surrounding SQL syntax.
fn emulated_bind_literal(bind: &Bind) -> String {
    match bind {
        Bind::Null => "NULL".to_string(),
        Bind::Int(value) => value.to_string(),
        Bind::Float(value) if value.is_finite() => value.to_string(),
        Bind::Float(value) => format!("'{}'", value),
        Bind::Text(value) => format!("'{}'", value.replace('\'', "''")),
        Bind::Bytes(value) => {
            let hex = value
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            format!("decode('{hex}', 'hex')")
        }
    }
}

/// Substitutes only markers generated by the PDO scanner, leaving any source
/// `$1` token untouched even when the same statement also contains PDO binds.
fn interpolate_emulated_sql(
    sql: &str,
    markers: &[(usize, usize, usize)],
    binds: &[Bind],
) -> Result<String, String> {
    let mut out = String::with_capacity(sql.len() + binds.len() * 8);
    let mut cursor = 0usize;
    for &(start, end, bind_index) in markers {
        let bind = binds.get(bind_index.saturating_sub(1)).ok_or_else(|| {
            "Invalid parameter number: number of bound variables does not match number of tokens"
                .to_string()
        })?;
        out.push_str(&sql[cursor..start]);
        out.push_str(&emulated_bind_literal(bind));
        cursor = end;
    }
    out.push_str(&sql[cursor..]);
    Ok(out)
}

/// Extracts the 5-char SQLSTATE from a postgres driver error. `tokio_postgres`
/// (the `postgres` crate's async foundation) already parses the server's
/// `ErrorResponse` message and exposes its `SQLSTATE` field ('C' code) through
/// `Error::code()`, so no manual wire-protocol parsing is needed here. Errors
/// with no server-reported code (a connection/transport failure rather than a
/// query error) fall back to the generic `HY000`.
fn pg_sqlstate(e: &postgres::Error) -> String {
    e.code()
        .map(|c| c.code().to_string())
        .unwrap_or_else(|| "HY000".to_string())
}

/// Owns a PostgreSQL client while iterating a native query one requested row at
/// a time, returning the same client when the stream ends or is closed.
fn run_pg_stream_worker(
    mut client: Client,
    statement: Statement,
    params: Vec<Param>,
    commands: mpsc::Receiver<PgStreamCommand>,
    responses: mpsc::Sender<PgStreamResponse>,
) {
    use postgres::fallible_iterator::FallibleIterator;

    let result: Result<(), postgres::Error> = (|| {
        let refs: Vec<&(dyn ToSql + Sync)> = params
            .iter()
            .map(|param| param as &(dyn ToSql + Sync))
            .collect();
        let mut rows = client.query_raw(&statement, refs)?;
        if responses.send(PgStreamResponse::Started).is_err() {
            return Ok(());
        }
        while let Ok(command) = commands.recv() {
            match command {
                PgStreamCommand::Next => match rows.next()? {
                    Some(row) => {
                        if responses
                            .send(PgStreamResponse::Row(decode_row(&row)))
                            .is_err()
                        {
                            break;
                        }
                    }
                    None => break,
                },
                PgStreamCommand::Close => break,
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            let _ = responses.send(PgStreamResponse::Finished(client));
        }
        Err(error) => {
            let sqlstate = pg_sqlstate(&error);
            let message = error.to_string();
            let _ = responses.send(PgStreamResponse::Failed(client, sqlstate, message));
        }
    }
}

/// The "native" (driver-specific) error code this driver reports as PDO's
/// `errorInfo()[1]` for every PostgreSQL failure — a deliberate, documented
/// divergence from php-src rather than an oversight (D-07).
///
/// PostgreSQL has **no integer error code**. The wire protocol's `ErrorResponse`
/// message carries only string fields (severity, SQLSTATE, message, detail, hint,
/// position, …), and the SQLSTATE *is* the code — which PDO already surfaces as
/// `errorInfo()[0]` (see [`pg_sqlstate`]). Accordingly the `postgres` crate's
/// `Error`/`DbError` expose no integer at all: `DbError::code()` returns a
/// `SqlState` (the 5-char SQLSTATE string), and the only other numeric accessors
/// are the server's *source-file line* and the error's character *position* in the
/// query — neither is an error code.
///
/// What php-src's pdo_pgsql puts in `errorInfo()[1]` is not a server code either:
/// it is libpq's client-side `ExecStatusType` enum, i.e. `PQresultStatus()` of the
/// failed `PGresult` (almost always `PGRES_FATAL_ERROR`). elephc's driver is the
/// pure-Rust `postgres` client, which has no libpq and no `PGresult`, so that value
/// simply does not exist here and could only be fabricated.
///
/// This driver therefore reports a single non-zero "an error occurred" marker. Zero
/// is reserved for success: `errcode` doubles as the bridge's error flag (callers
/// such as `copy_out`'s empty-vs-failed disambiguation test `elephc_pdo_errcode()`
/// against 0), so the marker only has to be non-zero and stable. `1` also matches
/// the value `my.rs` uses for its driver-level `HY093` (mixed placeholder styles)
/// rejection, so the one error both drivers raise themselves reports the same
/// native code on both.
const PG_NATIVE_ERRCODE: i64 = 1;

impl PgConn {
    /// Connects to PostgreSQL for a `pgsql:` DSN. Returns the connection or an
    /// error message for `last_open_error`. The connection is built through a
    /// `Config` (rather than `Client::connect`) so a `notice_callback` can be
    /// installed that buffers every server NOTICE into `notices` for
    /// `Pdo\Pgsql::setNoticeCallback()`.
    pub fn open(dsn: &str) -> Result<PgConn, String> {
        let options = resolve_dsn_options(dsn)?;
        let conn_str = parse_resolved_dsn(&options)?;
        let client_encoding = client_encoding_from_options(&options)?;
        let tls = parse_tls_options(&options)?;
        let mut config: Config = conn_str.parse().map_err(|e: postgres::Error| e.to_string())?;
        let notices: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        let sink = Arc::clone(&notices);
        // The closure param is inferred as `tokio_postgres::error::DbError`; a NOTICE
        // carries its human-readable text in `message()`. Never blocks the driver: a
        // poisoned lock just drops the message.
        config.notice_callback(move |notice| {
            if let Ok(mut queue) = sink.lock() {
                queue.push_back(notice.message().to_string());
            }
        });
        // Applies `sslmode` and opens the connection over rustls (ring) when TLS is
        // requested, or plaintext otherwise (see `connect_tls`).
        let mut client = connect_tls(&mut config, &tls)?;
        if let Some(encoding) = client_encoding {
            client
                .batch_execute(&format!("SET client_encoding TO '{encoding}'"))
                .map_err(|error| error.to_string())?;
        }
        Ok(PgConn {
            client: PgClientSlot(Some(client)),
            changes: 0,
            errmsg: String::new(),
            errcode: 0,
            sqlstate: "00000".to_string(),
            notices,
            prefetch: true,
            query_generation: 0,
            active_stream: None,
            next_stream_id: 0,
            in_transaction: false,
        })
    }

    /// Sets the default PostgreSQL prefetch/buffering mode for future statements.
    pub fn set_prefetch(&mut self, prefetch: bool) -> i64 {
        self.prefetch = prefetch;
        1
    }

    /// Starts a new query generation and returns the generation an unbuffered
    /// statement must retain to remain readable.
    fn begin_query(&mut self) -> u64 {
        self.finish_active_stream();
        self.query_generation = self.query_generation.wrapping_add(1).max(1);
        self.query_generation
    }

    /// Stops and drains ownership from the active worker before another command.
    fn finish_active_stream(&mut self) {
        let Some(mut active) = self.active_stream.take() else {
            return;
        };
        let _ = active.commands.send(PgStreamCommand::Close);
        while let Ok(response) = active.responses.recv() {
            match response {
                PgStreamResponse::Finished(client) => {
                    self.client.0 = Some(client);
                    break;
                }
                PgStreamResponse::Failed(client, sqlstate, message) => {
                    self.client.0 = Some(client);
                    self.sqlstate = sqlstate;
                    self.errmsg = message;
                    self.errcode = PG_NATIVE_ERRCODE;
                    break;
                }
                PgStreamResponse::Started | PgStreamResponse::Row(_) => {}
            }
        }
        if let Some(worker) = active.worker.take() {
            let _ = worker.join();
        }
    }

    /// Finishes the active worker only when it belongs to `id`.
    fn finish_stream(&mut self, id: u64) {
        if self.active_stream.as_ref().map(|stream| stream.id) == Some(id) {
            self.finish_active_stream();
        }
    }

    /// Starts a demand-driven native query worker and returns its stream identity.
    fn start_stream(&mut self, statement: Statement, params: Vec<Param>) -> Result<u64, i64> {
        self.finish_active_stream();
        let Some(client) = self.client.0.take() else {
            self.errcode = PG_NATIVE_ERRCODE;
            self.sqlstate = "HY000".to_string();
            self.errmsg = "PostgreSQL connection is busy with an unbuffered query".to_string();
            return Err(-1);
        };
        let (command_tx, command_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_pg_stream_worker(client, statement, params, command_rx, response_tx);
        });
        self.next_stream_id = self.next_stream_id.wrapping_add(1).max(1);
        let id = self.next_stream_id;
        let mut active = PgActiveStream {
            id,
            commands: command_tx,
            responses: response_rx,
            worker: Some(worker),
        };
        match active.responses.recv() {
            Ok(PgStreamResponse::Started) => {
                self.active_stream = Some(active);
                Ok(id)
            }
            Ok(PgStreamResponse::Failed(client, sqlstate, message)) => {
                self.client.0 = Some(client);
                self.sqlstate = sqlstate;
                self.errmsg = message;
                self.errcode = PG_NATIVE_ERRCODE;
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                Err(-1)
            }
            Ok(PgStreamResponse::Finished(client)) => {
                self.client.0 = Some(client);
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                Err(-1)
            }
            Ok(PgStreamResponse::Row(_)) | Err(_) => {
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                self.errcode = PG_NATIVE_ERRCODE;
                self.sqlstate = "HY000".to_string();
                self.errmsg = "PostgreSQL unbuffered query worker terminated unexpectedly".to_string();
                Err(-1)
            }
        }
    }

    /// Requests one row from the active stream, recovering the client at EOF.
    fn next_stream_row(&mut self, id: u64) -> Result<Option<Vec<Cell>>, i64> {
        let Some(active) = self.active_stream.as_mut() else {
            return Ok(None);
        };
        if active.id != id {
            return Ok(None);
        }
        if active.commands.send(PgStreamCommand::Next).is_err() {
            self.sqlstate = "HY000".to_string();
            self.errmsg = "PostgreSQL row stream is unavailable".to_string();
            self.errcode = PG_NATIVE_ERRCODE;
            return Err(-1);
        }
        match active.responses.recv() {
            Ok(PgStreamResponse::Row(row)) => Ok(Some(row)),
            Ok(PgStreamResponse::Finished(client)) => {
                self.client.0 = Some(client);
                let mut active = self.active_stream.take().expect("active stream disappeared");
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                Ok(None)
            }
            Ok(PgStreamResponse::Failed(client, sqlstate, message)) => {
                self.client.0 = Some(client);
                self.sqlstate = sqlstate;
                self.errmsg = message;
                self.errcode = PG_NATIVE_ERRCODE;
                let mut active = self.active_stream.take().expect("active stream disappeared");
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
                Err(-1)
            }
            Ok(PgStreamResponse::Started) | Err(_) => {
                self.sqlstate = "HY000".to_string();
                self.errmsg = "PostgreSQL row stream returned an invalid response".to_string();
                self.errcode = PG_NATIVE_ERRCODE;
                Err(-1)
            }
        }
    }

    /// Removes and returns the oldest buffered server NOTICE message text, or an empty
    /// string when none is pending. Backs `Pdo\Pgsql::setNoticeCallback()`: the prelude
    /// drains this after each `exec()`/`query()` and dispatches each message to the
    /// registered PHP callback.
    pub fn drain_notice(&self) -> String {
        self.notices
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front())
            .unwrap_or_default()
    }

    /// Applies PHP 8.6's persistent-disconnect cleanup. PostgreSQL requires
    /// `DISCARD ALL` outside a transaction, so a standalone rollback is sent first.
    pub fn discard_all(&mut self) {
        self.begin_query();
        let _ = self.client.batch_execute("ROLLBACK");
        if let Err(error) = self.client.batch_execute("DISCARD ALL") {
            self.fail(error);
            return;
        }
        if let Ok(mut notices) = self.notices.lock() {
            notices.clear();
        }
        self.changes = 0;
        self.errmsg.clear();
        self.errcode = 0;
        self.sqlstate = "00000".to_string();
        self.prefetch = true;
        self.in_transaction = false;
        self.begin_query();
    }

    /// Updates transaction bookkeeping after one successful SQL command.
    fn note_transaction_sql(&mut self, sql: &str) {
        self.in_transaction = transaction_state_after_sql(sql, self.in_transaction);
    }

    /// Records a server/transport error: its SQLSTATE (`errorInfo()[0]`), its message
    /// (`errorInfo()[2]`) and the driver's single native error code
    /// ([`PG_NATIVE_ERRCODE`], `errorInfo()[1]` — PostgreSQL has no integer code, see
    /// the constant). Returns `-1`, the failure value of the row-count-returning
    /// entry points. Every error path of this driver funnels through here or through
    /// [`Self::fail_local`], so the native code is set in exactly those two places.
    fn fail(&mut self, e: postgres::Error) -> i64 {
        self.sqlstate = pg_sqlstate(&e);
        self.errmsg = e.to_string();
        self.errcode = PG_NATIVE_ERRCODE;
        -1
    }

    /// Records a failure the *driver itself* raises, with no `postgres::Error` behind
    /// it (the scanner's `HY093` rejection of a SQL text mixing `?` and `:name`), under
    /// the same native error code as a server error ([`PG_NATIVE_ERRCODE`]). Returns
    /// the recorded message, so a caller can `return Err(self.fail_local(…))`.
    fn fail_local(&mut self, sqlstate: &str, msg: &str) -> String {
        self.sqlstate = sqlstate.to_string();
        self.errmsg = msg.to_string();
        self.errcode = PG_NATIVE_ERRCODE;
        self.errmsg.clone()
    }

    /// Runs a statement with no result rows (`PDO::exec`), returning the affected
    /// row count or `-1`.
    pub fn exec(&mut self, sql: &str) -> i64 {
        self.begin_query();
        // execute() runs a single command; fall back to a multi-statement path for
        // scripts execute() rejects (it only accepts exactly one command).
        match self.client.execute(sql, &[]) {
            Ok(n) => {
                self.changes = n as i64;
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                self.note_transaction_sql(sql);
                n as i64
            }
            // P2-j: `simple_query` (not `batch_execute`) runs the whole script over
            // the simple query protocol and yields one `SimpleQueryMessage` per
            // statement, including a `CommandComplete(rows)` tag for each — mirroring
            // php-src's `PQexec`, which reports the LAST command's row count for a
            // multi-statement string. `batch_execute` discards those tags entirely
            // (always 0 affected), which is what this replaces.
            Err(_) => match self.client.simple_query(sql) {
                Ok(messages) => {
                    let rows = messages
                        .iter()
                        .rev()
                        .find_map(|m| match m {
                            SimpleQueryMessage::CommandComplete(n) => Some(*n),
                            _ => None,
                        })
                        .unwrap_or(0);
                    self.changes = rows as i64;
                    self.errcode = 0;
                    self.sqlstate = "00000".to_string();
                    self.note_transaction_sql(sql);
                    rows as i64
                }
                Err(e) => self.fail(e),
            },
        }
    }

    /// Runs a bare transaction-control statement, returning `1`/`0`.
    pub fn exec_simple(&mut self, sql: &str) -> i64 {
        self.begin_query();
        match self.client.batch_execute(sql) {
            Ok(()) => {
                self.note_transaction_sql(sql);
                1
            }
            Err(e) => {
                self.fail(e);
                0
            }
        }
    }

    /// Returns the last inserted id: `currval('name')` when a sequence name is
    /// given, else `lastval()` for the session. Returns `0` on error.
    pub fn last_insert_id(&mut self, name: Option<&str>) -> i64 {
        self.begin_query();
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

    /// Like `last_insert_id`, but returns the sequence value as PostgreSQL's text
    /// representation instead of parsing it as an `i64`: PostgreSQL sequences are
    /// `bigint` by default but a caller-chosen sequence can be any integer type,
    /// so a text round-trip avoids a lossy/failing numeric bridge. Empty string on
    /// error.
    ///
    /// F-CORE-18: an empty string is also the prelude's failure sentinel for
    /// `PDO::lastInsertId()` (`string|false`), so a server error (most commonly
    /// `lastval()`'s SQLSTATE 55000 when no sequence has been used yet in this
    /// session) records the connection's real SQLSTATE/message/native code via
    /// [`Self::fail`] before returning empty, instead of swallowing it — the
    /// prelude reads `elephc_pdo_sqlstate`/`elephc_pdo_errmsg` right after this
    /// call to decide between surfacing that error and a generic `IM001`.
    pub fn last_insert_id_text(&mut self, name: Option<&str>) -> String {
        self.begin_query();
        let sql = match name {
            Some(n) if !n.is_empty() => {
                format!("SELECT currval('{}')::text", n.replace('\'', "''"))
            }
            _ => "SELECT lastval()::text".to_string(),
        };
        match self.client.query_one(&sql, &[]) {
            Ok(row) => row.try_get::<_, String>(0).unwrap_or_default(),
            Err(e) => {
                self.fail(e);
                String::new()
            }
        }
    }

    /// Returns the PostgreSQL server's reported version string (`SHOW
    /// server_version`), or an empty string if the query fails.
    pub fn server_version(&mut self) -> String {
        self.begin_query();
        match self.client.query_one("SHOW server_version", &[]) {
            Ok(row) => row.try_get::<_, String>(0).unwrap_or_default(),
            Err(_) => String::new(),
        }
    }

    /// Returns the linked pure-Rust PostgreSQL client implementation and version,
    /// the standalone equivalent of php-src's compile-time libpq version.
    pub fn client_version(&self) -> String {
        "postgres 0.19.13".to_string()
    }

    /// Maps the synchronous client's live closed state to php-src's observable
    /// `PQstatus()` strings. A connected synchronous client is never exposed in
    /// one of libpq's asynchronous handshake states.
    pub fn connection_status(&self) -> String {
        if self.is_closed() {
            "Bad connection.".to_string()
        } else {
            "Connection OK; waiting to send.".to_string()
        }
    }

    /// Reports whether the underlying client is closed; an active stream owns a
    /// live client temporarily and therefore still counts as connected.
    pub fn is_closed(&self) -> bool {
        self.client
            .0
            .as_ref()
            .map(Client::is_closed)
            .unwrap_or(false)
    }

    /// Builds php-src's PostgreSQL server-information string from live backend
    /// and session parameters.
    pub fn server_info(&mut self) -> String {
        self.begin_query();
        let row = match self.client.query_one(
            "SELECT pg_backend_pid(), current_setting('client_encoding'), current_setting('is_superuser'), current_setting('session_authorization'), current_setting('DateStyle')",
            &[],
        ) {
            Ok(row) => row,
            Err(_) => return String::new(),
        };
        let pid = row.try_get::<_, i32>(0).unwrap_or(0);
        let client_encoding = row.try_get::<_, String>(1).unwrap_or_default();
        let is_superuser = row.try_get::<_, String>(2).unwrap_or_default();
        let session_authorization = row.try_get::<_, String>(3).unwrap_or_default();
        let date_style = row.try_get::<_, String>(4).unwrap_or_default();
        format!(
            "PID: {pid}; Client Encoding: {client_encoding}; Is Superuser: {is_superuser}; Session Authorization: {session_authorization}; Date Style: {date_style}"
        )
    }

    /// Returns the PostgreSQL backend process id serving this connection
    /// (`SELECT pg_backend_pid()`), or 0 if the query fails. Backs
    /// `Pdo\Pgsql::getPid()`.
    pub fn backend_pid(&mut self) -> i64 {
        self.begin_query();
        match self.client.query_one("SELECT pg_backend_pid()", &[]) {
            Ok(row) => row.try_get::<_, i32>(0).map(i64::from).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Creates a new empty large object and returns its OID as a decimal string
    /// (`SELECT lo_create(0)`), or an empty string on error. Backs
    /// `Pdo\Pgsql::lobCreate()`.
    pub fn lob_create(&mut self) -> String {
        self.begin_query();
        match self.client.query_one("SELECT lo_create(0)", &[]) {
            Ok(row) => row
                .try_get::<_, u32>(0)
                .map(|oid| oid.to_string())
                .unwrap_or_default(),
            Err(_) => String::new(),
        }
    }

    /// Deletes the large object named by `oid` (`SELECT lo_unlink(<oid>)`), returning
    /// 1 on success and 0 on a non-numeric OID or a server error. Backs
    /// `Pdo\Pgsql::lobUnlink()`.
    pub fn lob_unlink(&mut self, oid: &str) -> i64 {
        self.begin_query();
        let Ok(oid_num) = oid.parse::<u32>() else {
            return 0;
        };
        // oid_num is a validated integer, so inlining it is injection-safe.
        match self
            .client
            .query_one(&format!("SELECT lo_unlink({oid_num})"), &[])
        {
            Ok(_) => 1,
            Err(_) => 0,
        }
    }

    /// Reads a large object whole (`SELECT lo_get(<oid>)`), returning its raw bytes,
    /// or `None` on a non-numeric OID or a server error (e.g. no such object). Unlike
    /// the descriptor-based `lo_open`/`lo_read`/`lo_close` API, `lo_get` runs
    /// standalone (no explicit transaction). Retained for the pre-v45 bridge ABI;
    /// `Pdo\Pgsql::lobOpen()` now uses bounded reads.
    pub fn lob_get(&mut self, oid: &str) -> Option<Vec<u8>> {
        self.begin_query();
        let oid_num = oid.parse::<u32>().ok()?;
        // oid_num is a validated integer, so inlining it is injection-safe.
        match self
            .client
            .query_one(&format!("SELECT lo_get({oid_num})"), &[])
        {
            Ok(row) => match row.try_get::<_, Vec<u8>>(0) {
                Ok(bytes) => {
                    self.errcode = 0;
                    self.sqlstate = "00000".to_string();
                    Some(bytes)
                }
                Err(_) => None,
            },
            Err(e) => {
                self.fail(e);
                None
            }
        }
    }

    /// Writes a complete large-object value at offset zero for pre-v45 ABI callers.
    /// The current stream wrapper uses [`Self::lob_write_at`] for bounded patches.
    pub fn lob_put(&mut self, oid: &str, data: &[u8]) -> i64 {
        self.begin_query();
        let Ok(oid_num) = oid.parse::<u32>() else {
            return 0;
        };
        match self
            .client
            .query_one("SELECT lo_put($1, 0, $2)", &[&oid_num, &data])
        {
            Ok(_) => {
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                1
            }
            Err(e) => {
                self.fail(e);
                0
            }
        }
    }

    /// Returns the current byte length of a PostgreSQL large object without
    /// transferring its contents to the client, or `None` for an invalid/missing OID.
    pub fn lob_size(&mut self, oid: &str) -> Option<i64> {
        self.begin_query();
        let oid_num = oid.parse::<u32>().ok()?;
        let sql = "SELECT COALESCE(MAX(l.pageno::bigint * 2048 + octet_length(l.data)), 0)::bigint FROM pg_catalog.pg_largeobject_metadata m LEFT JOIN pg_catalog.pg_largeobject l ON l.loid = m.oid WHERE m.oid = $1 GROUP BY m.oid";
        match self.client.query_opt(sql, &[&oid_num]) {
            Ok(Some(row)) => {
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                row.try_get::<_, i64>(0).ok()
            }
            Ok(None) => None,
            Err(error) => {
                self.fail(error);
                None
            }
        }
    }

    /// Reads at most `length` bytes from a PostgreSQL large object at `offset`.
    /// The server returns only the requested slice, avoiding a whole-object snapshot.
    pub fn lob_read_at(&mut self, oid: &str, offset: i64, length: i64) -> Option<Vec<u8>> {
        self.begin_query();
        let oid_num = oid.parse::<u32>().ok()?;
        let length = i32::try_from(length).ok()?;
        if offset < 0 || length < 0 {
            return None;
        }
        match self
            .client
            .query_one("SELECT lo_get($1, $2, $3)", &[&oid_num, &offset, &length])
        {
            Ok(row) => {
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                row.try_get::<_, Vec<u8>>(0).ok()
            }
            Err(error) => {
                self.fail(error);
                None
            }
        }
    }

    /// Writes one byte slice to a PostgreSQL large object at `offset`, preserving
    /// the server's native sparse-extension and zero-fill behavior.
    pub fn lob_write_at(&mut self, oid: &str, offset: i64, data: &[u8]) -> i64 {
        self.begin_query();
        let Ok(oid_num) = oid.parse::<u32>() else {
            return -1;
        };
        if offset < 0 {
            return -1;
        }
        match self
            .client
            .query_one("SELECT lo_put($1, $2, $3)", &[&oid_num, &offset, &data])
        {
            Ok(_) => {
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                data.len() as i64
            }
            Err(error) => {
                self.fail(error);
                -1
            }
        }
    }

    /// Streams `data` into the server for a `COPY … FROM STDIN` statement (built by
    /// the prelude), returning the number of rows copied or -1 on error. Backs
    /// `Pdo\Pgsql::copyFromArray()` / `copyFromFile()`.
    pub fn copy_in(&mut self, copy_sql: &str, data: &[u8]) -> i64 {
        use std::io::Write;
        self.begin_query();
        // Run the whole COPY in a closure so the writer's borrow of `self.client`
        // ends before the connection bookkeeping fields (or `fail`) are written.
        let result: Result<u64, postgres::Error> = (|| {
            let mut writer = self.client.copy_in(copy_sql)?;
            // write_all's io::Error is not a postgres::Error; a write failure is
            // surfaced with the real server error by finish() below.
            let _ = writer.write_all(data);
            writer.finish()
        })();
        match result {
            Ok(rows) => {
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                rows as i64
            }
            Err(e) => self.fail(e),
        }
    }

    /// Runs a `COPY … TO STDOUT` statement (built by the prelude) and returns its raw
    /// text output (rows separated by newlines); also an empty string on error, same
    /// as for a genuinely empty COPY. Backs `Pdo\Pgsql::copyToArray()` / `copyToFile()`.
    ///
    /// P2-i: those two empty-string cases are told apart not by this return value
    /// but by `errcode`, which this method always resets to `0` on success (even an
    /// empty one) and sets non-zero via [`Self::fail`] on error — the prelude checks
    /// `elephc_pdo_errcode()` immediately after the call to distinguish "really
    /// empty" (returns `[]`) from "the COPY failed" (returns `false`), matching the
    /// stub's `array|false` contract for `copyToArray()`.
    pub fn copy_out(&mut self, copy_sql: &str) -> String {
        use std::io::Read;
        self.begin_query();
        // Run the COPY in a closure so the reader's borrow of `self.client` ends
        // before the connection bookkeeping fields (or `fail`) are written.
        let result: Result<Vec<u8>, postgres::Error> = (|| {
            let mut reader = self.client.copy_out(copy_sql)?;
            let mut buf = Vec::new();
            // read_to_end's io::Error is not a postgres::Error; a partial read still
            // returns whatever bytes arrived.
            let _ = reader.read_to_end(&mut buf);
            Ok(buf)
        })();
        match result {
            Ok(buf) => {
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                String::from_utf8_lossy(&buf).into_owned()
            }
            Err(e) => {
                self.fail(e);
                String::new()
            }
        }
    }

    /// Polls for a pending LISTEN/NOTIFY notification, returning it as a
    /// tab-separated `channel\tpid\tpayload` string, or an empty string if none
    /// arrives within `timeout_ms` (a zero/negative timeout polls once for an
    /// already-buffered notification). Backs `Pdo\Pgsql::getNotify()`; the prelude
    /// shapes the parts into the requested array form.
    pub fn get_notify(&mut self, timeout_ms: i64) -> String {
        use postgres::fallible_iterator::FallibleIterator;
        use std::time::Duration;
        self.begin_query();
        let timeout = Duration::from_millis(timeout_ms.max(0) as u64);
        let mut notifications = self.client.notifications();
        let next = if timeout.is_zero() {
            notifications.iter().next()
        } else {
            notifications.timeout_iter(timeout).next()
        };
        match next {
            Ok(Some(n)) => format!("{}\t{}\t{}", n.channel(), n.process_id(), n.payload()),
            _ => String::new(),
        }
    }

    /// Prepares a statement: translates placeholders and prepares it server-side
    /// for column metadata. Returns the statement or an error message. Rejects a
    /// SQL text that mixes a positional `?` with a named `:name` placeholder
    /// with `HY093` before ever asking the server to prepare it — PDO forbids
    /// combining the two styles in one statement, and the server has no notion
    /// of "named" placeholders to catch this itself.
    pub fn prepare(&mut self, sql: &str, emulated: bool) -> Result<PgStmt, String> {
        self.begin_query();
        let (translated, named_map, mixed, markers) = translate_placeholders_with_markers(sql);
        if mixed {
            return Err(self.fail_local(
                "HY093",
                "Invalid parameter number: mixed named and positional parameters",
            ));
        }
        if emulated {
            let n_params = markers
                .iter()
                .map(|(_, _, index)| *index)
                .max()
                .unwrap_or(0);
            self.errcode = 0;
            self.sqlstate = "00000".to_string();
            return Ok(PgStmt {
                conn_id: 0,
                query_string: sql.to_string(),
                statement: None,
                emulated_sql: Some(translated),
                emulated_markers: markers,
                sent_sql: String::new(),
                named_map,
                binds: vec![Bind::Null; n_params],
                bound: vec![false; n_params],
                col_names: Vec::new(),
                col_tables: Vec::new(),
                rows: Vec::new(),
                cursor: -1,
                executed: false,
                buffered: self.prefetch,
                query_generation: 0,
                stream_id: None,
            });
        }
        match self.client.prepare(&translated) {
            Ok(statement) => {
                let col_names = statement
                    .columns()
                    .iter()
                    .map(|c| c.name().to_string())
                    .collect();
                let n_params = statement.params().len();
                let col_tables = statement
                    .columns()
                    .iter()
                    .map(|column| {
                        let Some(oid) = column.table_oid() else {
                            return String::new();
                        };
                        self.client
                            .query_opt(
                                "SELECT relname FROM pg_catalog.pg_class WHERE oid = $1",
                                &[&oid],
                            )
                            .ok()
                            .flatten()
                            .and_then(|row| row.try_get::<_, String>(0).ok())
                            .unwrap_or_default()
                    })
                    .collect();
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
                Ok(PgStmt {
                    conn_id: 0,
                    query_string: sql.to_string(),
                    statement: Some(statement),
                    emulated_sql: None,
                    emulated_markers: Vec::new(),
                    sent_sql: String::new(),
                    named_map,
                    binds: vec![Bind::Null; n_params],
                    bound: vec![false; n_params],
                    col_names,
                    col_tables,
                    rows: Vec::new(),
                    cursor: -1,
                    executed: false,
                    buffered: self.prefetch,
                    query_generation: 0,
                    stream_id: None,
                })
            }
            Err(e) => {
                let msg = e.to_string();
                self.fail(e);
                Err(msg)
            }
        }
    }
}

/// Derives PostgreSQL transaction state after a successful transaction-control
/// command, preserving the current state for ordinary statements and savepoint rollback.
pub(crate) fn transaction_state_after_sql(sql: &str, current: bool) -> bool {
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
    current
}

impl PgStmt {
    /// Overrides this statement's buffering mode from prepare-time
    /// `PDO::ATTR_PREFETCH`, before its first execution.
    pub fn set_prefetch(&mut self, prefetch: bool) -> i64 {
        if self.executed {
            return 0;
        }
        self.buffered = prefetch;
        1
    }

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
        self.bound[(idx - 1) as usize] = true;
        1
    }

    /// Resets the cursor and execution state, keeping the bound values.
    pub fn reset(&mut self, conn: &mut PgConn) -> i64 {
        if let Some(stream_id) = self.stream_id {
            conn.finish_stream(stream_id);
        }
        self.cursor = -1;
        self.executed = false;
        self.rows.clear();
        self.stream_id = None;
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

    /// Executes the query once, buffering rows or starting a native demand stream.
    /// Sets `conn.changes` for non-result statements.
    fn execute(&mut self, conn: &mut PgConn) -> Result<(), i64> {
        self.query_generation = conn.begin_query();
        if self.emulated_sql.is_some() {
            return self.execute_emulated(conn);
        }
        let statement = self
            .statement
            .as_ref()
            .expect("native PostgreSQL statement missing its prepared handle");
        let param_types: Vec<Type> = statement.params().to_vec();
        let params: Vec<Param> = self
            .binds
            .iter()
            .zip(param_types.into_iter())
            .map(|(bind, ty)| Param {
                bind: bind.clone(),
                ty,
            })
            .collect();
        if !self.buffered && !statement.columns().is_empty() {
            let stream_id = conn.start_stream(statement.clone(), params)?;
            self.rows.clear();
            self.cursor = -1;
            self.stream_id = Some(stream_id);
            conn.changes = 0;
            conn.errcode = 0;
            conn.sqlstate = "00000".to_string();
            self.executed = true;
            conn.note_transaction_sql(&self.query_string);
            return Ok(());
        }
        let refs: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

        if statement.columns().is_empty() {
            // No result columns: a DML/DDL statement. Run it for the row count.
            match conn.client.execute(statement, &refs) {
                Ok(n) => {
                    conn.changes = n as i64;
                    conn.errcode = 0;
                    conn.sqlstate = "00000".to_string();
                    self.executed = true;
                    conn.note_transaction_sql(&self.query_string);
                    Ok(())
                }
                // `fail` records the SQLSTATE/message/native code and yields the `-1`
                // that `step()` propagates as the statement's error return.
                Err(e) => Err(conn.fail(e)),
            }
        } else {
            match conn.client.query(statement, &refs) {
                Ok(rows) => {
                    self.rows = rows.iter().map(|r| decode_row(r)).collect();
                    conn.changes = if self.buffered {
                        self.rows.len() as i64
                    } else {
                        0
                    };
                    conn.errcode = 0;
                    conn.sqlstate = "00000".to_string();
                    self.executed = true;
                    conn.note_transaction_sql(&self.query_string);
                    Ok(())
                }
                Err(e) => Err(conn.fail(e)),
            }
        }
    }

    /// Executes an emulated PostgreSQL statement through the simple-query
    /// protocol and materializes the final row-producing result set as text.
    fn execute_emulated(&mut self, conn: &mut PgConn) -> Result<(), i64> {
        if self.bound.iter().any(|bound| !bound) {
            conn.errcode = PG_NATIVE_ERRCODE;
            conn.sqlstate = "HY093".to_string();
            conn.errmsg = "Invalid parameter number: number of bound variables does not match number of tokens".to_string();
            return Err(-1);
        }
        let sql = match interpolate_emulated_sql(
            self.emulated_sql
                .as_deref()
                .expect("emulated PostgreSQL statement missing SQL"),
            &self.emulated_markers,
            &self.binds,
        ) {
            Ok(sql) => sql,
            Err(message) => {
                conn.errcode = PG_NATIVE_ERRCODE;
                conn.sqlstate = "HY093".to_string();
                conn.errmsg = message;
                return Err(-1);
            }
        };
        self.sent_sql = sql.clone();
        match conn.client.simple_query(&sql) {
            Ok(messages) => {
                self.rows.clear();
                self.col_names.clear();
                let mut changes = 0i64;
                for message in messages {
                    match message {
                        SimpleQueryMessage::RowDescription(columns) => {
                            self.rows.clear();
                            self.col_names = columns
                                .iter()
                                .map(|column| column.name().to_string())
                                .collect();
                            self.col_tables = vec![String::new(); columns.len()];
                        }
                        SimpleQueryMessage::Row(row) => {
                            let cells = (0..row.len())
                                .map(|index| match row.get(index) {
                                    Some(value) => Cell::Text(value.to_string()),
                                    None => Cell::Null,
                                })
                                .collect();
                            self.rows.push(cells);
                        }
                        SimpleQueryMessage::CommandComplete(count) => {
                            changes = count as i64;
                        }
                        _ => {}
                    }
                }
                conn.changes = if self.rows.is_empty() {
                    changes
                } else if !self.buffered {
                    0
                } else {
                    self.rows.len() as i64
                };
                conn.errcode = 0;
                conn.sqlstate = "00000".to_string();
                self.executed = true;
                conn.note_transaction_sql(&self.query_string);
                Ok(())
            }
            Err(error) => Err(conn.fail(error)),
        }
    }

    /// Advances to the next row: `1` for a row, `0` when exhausted, `-1` on
    /// error. Executes lazily on the first call.
    pub fn step(&mut self, conn: &mut PgConn) -> i64 {
        if self.executed
            && !self.buffered
            && self.query_generation != conn.query_generation
        {
            self.cursor = self.rows.len() as isize;
            return 0;
        }
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
                    self.stream_id = None;
                    0
                }
                Err(code) => code,
            };
        }
        self.cursor += 1;
        if (self.cursor as usize) < self.rows.len() {
            1
        } else {
            0
        }
    }

    /// Executes lazily and moves a materialized PostgreSQL result cursor according
    /// to PDO's scroll orientations. Absolute positions use PostgreSQL's one-based
    /// cursor convention; negative absolute positions count backward from the end.
    pub fn step_oriented(&mut self, conn: &mut PgConn, orientation: i64, offset: i64) -> i64 {
        if self.executed
            && !self.buffered
            && self.query_generation != conn.query_generation
        {
            self.cursor = self.rows.len() as isize;
            return 0;
        }
        if !self.executed {
            if let Err(code) = self.execute(conn) {
                return code;
            }
        }
        let len = self.rows.len() as i128;
        let current = self.cursor as i128;
        let target = match orientation {
            0 => current + 1,
            1 => current - 1,
            2 => 0,
            3 => len - 1,
            4 if offset > 0 => i128::from(offset) - 1,
            4 if offset < 0 => len + i128::from(offset),
            4 => -1,
            5 => current + i128::from(offset),
            _ => return 0,
        };
        if target < 0 {
            self.cursor = -1;
            return 0;
        }
        if target >= len {
            self.cursor = self.rows.len() as isize;
            return 0;
        }
        self.cursor = target as isize;
        1
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
        if self.emulated_sql.is_some() && !self.executed && self.col_names.is_empty() {
            1
        } else {
            self.col_names.len() as i64
        }
    }

    /// Name of result column `i` (0-based).
    pub fn column_name(&self, i: i64) -> String {
        self.col_names.get(i as usize).cloned().unwrap_or_default()
    }

    /// Returns the `pg_class.relname` resolved from the result column's table OID.
    pub fn column_table_name(&self, i: i64) -> String {
        if i < 0 {
            return String::new();
        }
        self.col_tables.get(i as usize).cloned().unwrap_or_default()
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

    /// Returns the bytes currently owned by this statement's materialized result,
    /// including row/cell storage and heap-backed text/byte payload capacities.
    /// `None` means the statement has not executed yet.
    pub fn result_memory_size(&self) -> Option<i64> {
        if !self.executed {
            return None;
        }
        let visible_rows: &[Vec<Cell>] = if self.buffered {
            &self.rows
        } else {
            self.rows
                .get(self.cursor.max(0) as usize)
                .map(std::slice::from_ref)
                .unwrap_or(&[])
        };
        let mut bytes = visible_rows.len() * std::mem::size_of::<Vec<Cell>>();
        for row in visible_rows {
            bytes = bytes.saturating_add(row.capacity() * std::mem::size_of::<Cell>());
            for cell in row {
                bytes = bytes.saturating_add(match cell {
                    Cell::Text(value) => value.capacity(),
                    Cell::Bytes(value) => value.capacity(),
                    Cell::Null | Cell::Int(_) | Cell::Float(_) => 0,
                });
            }
        }
        bytes = bytes.saturating_add(
            self.col_names
                .iter()
                .map(|name| name.capacity())
                .sum::<usize>(),
        );
        Some(i64::try_from(bytes).unwrap_or(i64::MAX))
    }

    /// PostgreSQL native type name of result column `i` (0-based) — the server's
    /// own `pg_type.typname` (`int4`, `bool`, `bytea`, `varchar`, …) that the
    /// driver resolved at prepare time off the retained `Statement`. Because it
    /// comes from the column descriptor rather than a live cell, it is available
    /// whether or not a row is active and reflects the column's DECLARED type
    /// instead of a NULL value's runtime storage class. Empty string for an
    /// out-of-range index. Backs `getColumnMeta`'s `native_type` on a `pgsql:`
    /// statement (P2-k).
    pub fn column_native_type(&self, i: i64) -> String {
        if i < 0 {
            return String::new();
        }
        self.statement
            .as_ref()
            .and_then(|statement| statement.columns().get(i as usize))
            .map(|c| c.type_().name().to_string())
            .unwrap_or_default()
    }

    /// PostgreSQL type OID of result column `i` (0-based) — the `PQftype` value
    /// carried by the column's `postgres::types::Type`. Backs `getColumnMeta`'s
    /// `pgsql:oid` key and, prelude-side, the PDO param-type derivation
    /// (BOOL→PARAM_BOOL, int-family→PARAM_INT, BYTEA→PARAM_LOB, else PARAM_STR).
    /// `0` (the invalid OID) for an out-of-range index. (P2-k)
    pub fn column_type_oid(&self, i: i64) -> i64 {
        if i < 0 {
            return 0;
        }
        self.statement
            .as_ref()
            .and_then(|statement| statement.columns().get(i as usize))
            .map(|c| i64::from(c.type_().oid()))
            .unwrap_or(0)
    }

    /// OID of the table result column `i` (0-based) was selected FROM, or `0`
    /// (`InvalidOid`) when the column is not a plain table column — an expression,
    /// a literal, an aggregate, a function result. Backs `getColumnMeta`'s
    /// `pgsql:table_oid` key, which php-src's `pgsql_stmt_get_column_meta`
    /// (`ext/pdo_pgsql/pgsql_statement.c`) emits UNCONDITIONALLY from `PQftable()`,
    /// including the `0` for an expression column (F-PG-01).
    ///
    /// Exact `PQftable()` parity, straight off the wire: the RowDescription message
    /// carries a per-field table OID, and tokio-postgres keeps it on `Column`
    /// (`Column::table_oid()`, statement.rs:104). It normalizes the wire's `0` to
    /// `None` (prepare.rs:100, `.filter(|n| *n != 0)`), so mapping `None` back to `0`
    /// here restores the server's value byte for byte. No catalog lookup and no
    /// per-fetch round trip are needed — contrary to the spec's premise, the pinned
    /// crate does surface this.
    ///
    /// `0` for an out-of-range index, which is also the neutral `InvalidOid`.
    pub fn column_table_oid(&self, i: i64) -> i64 {
        if i < 0 {
            return 0;
        }
        self.statement
            .as_ref()
            .and_then(|statement| statement.columns().get(i as usize))
            .and_then(|c| c.table_oid())
            .map(i64::from)
            .unwrap_or(0)
    }

    /// Byte width of result column `i`'s type (0-based): a positive fixed width
    /// (`int4` → 4, `timestamp` → 8, `uuid` → 16), `-1` for a variable-length
    /// (varlena) type (`text`, `varchar`, `numeric`, `bytea`, `json`, any array),
    /// or `-2` for a NUL-terminated C string (`cstring`, `unknown`). Backs
    /// `getColumnMeta`'s `len`, which php-src fills from `col->maxlen`, itself set
    /// straight from `PQfsize()` in `pgsql_stmt_describe`
    /// (`ext/pdo_pgsql/pgsql_statement.c:496`) (F-PG-02).
    ///
    /// ⚠ LIMITATION — this is DERIVED, not the value the server sent. `PQfsize()` is
    /// the RowDescription field's "data type size", and while postgres-protocol does
    /// parse it (`message/backend.rs:820`, exposed as `Field::type_size()`),
    /// tokio-postgres THROWS IT AWAY when it builds `Column` (prepare.rs:98-103 copies
    /// only name/table_oid/column_id/type_modifier/type) — there is no
    /// `Column::type_size()` to read. Reaching the real value would need either a
    /// crate fork or a `pg_type` catalog query, and the latter is impossible here
    /// anyway: this accessor takes `&self` with no `Client`, so it could only run at
    /// prepare time, adding a server round trip to EVERY prepare for a metadata field
    /// almost nothing reads.
    ///
    /// So the width is recomputed from the column's type instead — which is sound,
    /// because that is exactly what the server does: `PQfsize()` returns
    /// `pg_type.typlen`, a property of the TYPE, not of the column or the row (an
    /// `int4` column is 4 bytes wide in every table of every database). See
    /// [`type_len`] for the table and for the one case it cannot cover.
    ///
    /// `-1` for an out-of-range index (PostgreSQL's own "not a fixed width" value).
    pub fn column_len(&self, i: i64) -> i64 {
        if i < 0 {
            return -1;
        }
        self.statement
            .as_ref()
            .and_then(|statement| statement.columns().get(i as usize))
            .map(|c| type_len(c.type_()))
            .unwrap_or(-1)
    }

    /// Type modifier (`atttypmod`) of result column `i` (0-based), or `-1` when the
    /// type takes no modifier or the column carries none. Backs `getColumnMeta`'s
    /// `precision`, which php-src fills from `col->precision`, itself set straight
    /// from `PQfmod()` in `pgsql_stmt_describe`
    /// (`ext/pdo_pgsql/pgsql_statement.c:497`) (F-PG-02).
    ///
    /// Exact `PQfmod()` parity: the RowDescription carries the type modifier per
    /// field and tokio-postgres keeps it verbatim on `Column`
    /// (`Column::type_modifier()`, statement.rs:114) — no catalog lookup needed.
    ///
    /// The value is the RAW `atttypmod`, deliberately NOT decoded into a
    /// human-readable precision, because php-src does not decode it either — it
    /// copies `PQfmod()` through unchanged, so `VARCHAR(20)` reports 24 (the length
    /// plus `VARHDRSZ` = 4) and `NUMERIC(10,2)` reports 655366 (`((10 << 16) | 2) +
    /// 4`). Decoding it here would be a divergence from PHP dressed up as an
    /// improvement; a caller who wants the real precision must decode the modifier
    /// exactly as it would have to against real PDO.
    ///
    /// `-1` for an out-of-range index (PostgreSQL's own "no type modifier" value).
    pub fn column_precision(&self, i: i64) -> i64 {
        if i < 0 {
            return -1;
        }
        self.statement
            .as_ref()
            .and_then(|statement| statement.columns().get(i as usize))
            .map(|c| i64::from(c.type_modifier()))
            .unwrap_or(-1)
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

/// PostgreSQL's `pg_type.typlen` for `ty` — the byte width the server reports for a
/// column of this type in the RowDescription's "data type size" field, i.e. exactly
/// what `PQfsize()` hands back to php-src. Positive = a fixed width; `-1` = a
/// variable-length (varlena) type; `-2` = a NUL-terminated C string.
///
/// Recomputed from the type rather than read off the wire because tokio-postgres
/// discards the wire value (see [`PgStmt::column_len`]). That substitution is exact
/// for everything below: `typlen` is a column of `pg_type`, so it is a property of
/// the TYPE alone — the server looks it up by the very type OID the crate hands us.
/// The constants are transcribed from PostgreSQL's own catalog seed data,
/// `src/include/catalog/pg_type.dat`, not inferred.
///
/// Only the FIXED-width types are enumerated: `-1` is the fallback, and it is the
/// right answer for every varlena type (`text`, `varchar`, `bpchar`, `numeric`,
/// `bytea`, `json`/`jsonb`, `xml`, `bit`/`varbit`, `path`, `polygon`, `tsvector`,
/// `record`, and every array/range/multirange/composite type, all of which are
/// varlena by construction in PostgreSQL).
///
/// ⚠ The one case this cannot cover: a user-defined or extension type whose kind is
/// `Simple` and whose OID is therefore not one of the constants below reports `-1`.
/// That is correct for the varlena types extensions overwhelmingly define (`hstore`,
/// `citext`, `ltree`, PostGIS `geometry`, …) but WRONG for a fixed-width one, which
/// would report `-1` instead of its true width. `aclitem` is deliberately left out
/// for the same reason from the other direction: its width is not stable across
/// server versions (12 bytes until PostgreSQL 15, 16 from PostgreSQL 16, which
/// widened `AclMode` to 64 bits), so hardcoding either value would be a lie for half
/// the servers — it falls back to `-1`. `name` assumes the default `NAMEDATALEN` of
/// 64, which a server can be recompiled to change.
fn type_len(ty: &Type) -> i64 {
    // Two kinds have a width fixed by construction rather than by a catalog constant,
    // and their OIDs are assigned per-database so they can never match a constant
    // below. An enum is always stored as an OID (`DefineEnum` creates the type with
    // `sizeof(Oid)`), and a domain inherits its base type's width verbatim
    // (`DefineDomain` copies `typlen` from the base), so recursing yields the truth.
    // Arrays, ranges, multiranges and composites are always varlena — they need no arm,
    // the `-1` fallback already covers them. `Kind` is `#[non_exhaustive]`.
    match ty.kind() {
        Kind::Enum(_) => return 4,
        Kind::Domain(base) => return type_len(base),
        _ => {}
    }
    match *ty {
        // `bool` and `"char"` are single bytes.
        Type::BOOL | Type::CHAR => 1,
        Type::INT2 => 2,
        // The 4-byte types: the 32-bit numerics, `date` (a day count), and the whole
        // `reg*` family, every member of which is an OID under the hood.
        Type::INT4
        | Type::FLOAT4
        | Type::OID
        | Type::XID
        | Type::CID
        | Type::DATE
        | Type::REGPROC
        | Type::REGPROCEDURE
        | Type::REGOPER
        | Type::REGOPERATOR
        | Type::REGCLASS
        | Type::REGTYPE
        | Type::REGCONFIG
        | Type::REGDICTIONARY
        | Type::REGNAMESPACE
        | Type::REGROLE
        | Type::REGCOLLATION
        | Type::VOID => 4,
        // `tid` is a block number (4) plus an offset (2); `macaddr` is 6 raw bytes.
        Type::TID | Type::MACADDR => 6,
        // The 8-byte types: 64-bit numerics, `money` (an int64 of cents), and the
        // date/time types PostgreSQL stores as a 64-bit microsecond count.
        Type::INT8
        | Type::FLOAT8
        | Type::MONEY
        | Type::TIME
        | Type::TIMESTAMP
        | Type::TIMESTAMPTZ
        | Type::MACADDR8
        | Type::PG_LSN
        | Type::XID8 => 8,
        // `timetz` is a `time` (8) plus its UTC offset in seconds (4).
        Type::TIMETZ => 12,
        // `interval` is microseconds (8) + days (4) + months (4); `point` is two
        // float8 coordinates.
        Type::INTERVAL | Type::UUID | Type::POINT => 16,
        // `line` is the three float8 coefficients of `Ax + By + C = 0`; `circle` is a
        // centre `point` (16) plus a float8 radius.
        Type::LINE | Type::CIRCLE => 24,
        // Both are two `point`s: a segment's endpoints, a box's opposite corners.
        Type::LSEG | Type::BOX => 32,
        // `NAMEDATALEN`, the identifier type's fixed width.
        Type::NAME => 64,
        // The NUL-terminated C-string types. `unknown` is what an unresolved literal
        // types as, so it can genuinely surface as a result column.
        Type::CSTRING | Type::UNKNOWN => -2,
        // Every remaining type is variable-length. See the doc comment for the one
        // case this fallback gets wrong (a fixed-width user-defined type).
        _ => -1,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Emulated interpolation replaces only scanner-generated markers, quotes
    /// text and bytes, and preserves a source `$1` token byte-for-byte.
    #[test]
    fn emulated_interpolation_uses_scanner_marker_ranges() {
        let (sql, _, mixed, markers) =
            translate_placeholders_with_markers("SELECT '$1', $1, :first, :name");
        assert!(!mixed);
        let rendered = interpolate_emulated_sql(
            &sql,
            &markers,
            &[Bind::Text("O'Reilly".to_string()), Bind::Bytes(vec![0, 255])],
        )
        .expect("emulated SQL renders");
        assert_eq!(
            rendered,
            "SELECT '$1', $1, 'O''Reilly', decode('00ff', 'hex')"
        );
    }

    /// The TLS keys are consumed by `parse_tls`, not forwarded into the libpq
    /// connection string — tokio-postgres's parser rejects `sslrootcert` and the
    /// `verify-*` sslmode values, so leaking any of them would break `.parse()`.
    #[test]
    fn parse_dsn_strips_tls_keys() {
        let dsn = "pgsql:host=db.example.com;sslmode=require;sslrootcert=/etc/ca.pem;dbname=app";
        let conn_str = parse_dsn(dsn).expect("dsn parses");
        assert!(conn_str.contains("host='db.example.com'"));
        assert!(conn_str.contains("dbname='app'"));
        assert!(
            !conn_str.contains("sslmode"),
            "sslmode must not reach the libpq conn string: {conn_str}"
        );
        assert!(
            !conn_str.contains("sslrootcert"),
            "sslrootcert must not reach the libpq conn string: {conn_str}"
        );
    }

    /// A supported translated key is stripped from the native Config string,
    /// while an unsupported libpq key fails explicitly instead of disappearing.
    #[test]
    fn parse_dsn_translates_client_encoding_and_rejects_unsupported_keys() {
        let dsn = "pgsql:host=db.example.com;dbname=app;client_encoding=UTF8";
        let conn_str = parse_dsn(dsn).expect("translated DSN parses");
        assert!(conn_str.contains("host='db.example.com'"));
        assert!(conn_str.contains("dbname='app'"));
        assert!(
            !conn_str.contains("client_encoding"),
            "translated client_encoding must not reach Config: {conn_str}"
        );
        conn_str
            .parse::<Config>()
            .expect("conn string with translated key must parse");
        assert_eq!(
            client_encoding_from_dsn(dsn).expect("encoding parses"),
            Some("UTF8".to_string())
        );
        let error = parse_dsn("pgsql:host=db.example.com;replication=database")
            .expect_err("unsupported replication semantics must fail");
        assert!(error.contains("unsupported PostgreSQL DSN option 'replication'"));
    }

    /// Malformed options and unsafe client-encoding values fail during DSN parsing.
    #[test]
    fn parse_dsn_rejects_malformed_and_invalid_client_encoding() {
        assert!(parse_dsn("pgsql:host=localhost;broken").is_err());
        assert!(parse_dsn("pgsql:host=localhost;client_encoding=UTF8' RESET ALL")
            .is_err());
    }

    /// F-CORE-02: the prelude percent-encodes a constructor-supplied password
    /// containing ';' (here `a;b` -> `a%3Bb`) before folding it into the DSN, so
    /// it survives `body.split(';')` intact instead of truncating at the
    /// embedded ';'. `parse_dsn` must undo that encoding before the value
    /// reaches the libpq conninfo string, landing on the original `a;b` (quoted,
    /// with no `%3B` left in it).
    #[test]
    fn parse_dsn_percent_decodes_a_password_containing_semicolon() {
        let dsn = "pgsql:host=db.example.com;user=admin;password=a%3Bb";
        let conn_str = parse_dsn(dsn).expect("dsn parses");
        assert!(
            conn_str.contains("password='a;b'"),
            "expected the decoded password in: {conn_str}"
        );
        assert!(
            !conn_str.contains("%3B"),
            "the percent-escape must not reach the libpq conn string: {conn_str}"
        );
        // The whole point: tokio-postgres's own parser must accept the decoded value.
        conn_str
            .parse::<Config>()
            .expect("conn string with a decoded password must still parse");
    }

    /// `parse_tls` captures `sslmode` (lowercased) and the three file paths.
    #[test]
    fn parse_tls_captures_mode_and_paths() {
        let tls = parse_tls(
            "pgsql:host=h;sslmode=VERIFY-FULL;sslrootcert=/ca.pem;sslcert=/c.pem;sslkey=/k.pem;sslcrl=/root.crl;sslcrldir=/crls",
        )
        .expect("TLS options parse");
        assert_eq!(tls.mode, "verify-full");
        assert_eq!(tls.root_cert.as_deref(), Some("/ca.pem"));
        assert_eq!(tls.client_cert.as_deref(), Some("/c.pem"));
        assert_eq!(tls.client_key.as_deref(), Some("/k.pem"));
        assert_eq!(tls.crl_file.as_deref(), Some("/root.crl"));
        assert_eq!(tls.crl_directory.as_deref(), Some("/crls"));
        assert!(tls.server_name_indication);
    }

    /// A DSN without TLS keys yields the unset defaults (libpq/tokio-postgres both
    /// default to `prefer`, represented here by an empty mode).
    #[test]
    fn parse_tls_defaults_when_absent() {
        let tls = parse_tls("pgsql:host=h;dbname=d").expect("TLS defaults parse");
        assert!(tls.mode.is_empty());
        assert!(tls.root_cert.is_none());
        assert!(tls.server_name_indication);
    }

    /// A named libpq service contributes defaults while explicit DSN values win,
    /// including legacy `fallback_application_name` normalization.
    #[test]
    fn parse_dsn_resolves_service_file_with_explicit_precedence() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "elephc-pdo-pg-service-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&dir).expect("create service fixture directory");
        let service_file = dir.join("pg_service.conf");
        fs::write(
            &service_file,
            "[other]\nhost=ignored\n[app]\nhost=service-host\nport=5433\ndbname=service-db\nfallback_application_name=elephc\n",
        )
        .expect("write service fixture");

        let dsn = format!(
            "pgsql:service=app;servicefile={};host=explicit-host;user=alice",
            service_file.display()
        );
        let conn_str = parse_dsn(&dsn).expect("service DSN resolves");
        assert!(conn_str.contains("host='explicit-host'"));
        assert!(conn_str.contains("port='5433'"));
        assert!(conn_str.contains("dbname='service-db'"));
        assert!(conn_str.contains("application_name='elephc'"));
        assert!(!conn_str.contains("service="));

        fs::remove_dir_all(dir).expect("remove service fixture directory");
    }

    /// A secure `.pgpass` supplies the first wildcard-matching password and
    /// correctly unescapes colons and backslashes in its password field.
    #[test]
    fn parse_dsn_resolves_password_file() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "elephc-pdo-pg-passfile-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&dir).expect("create passfile fixture directory");
        let passfile = dir.join(".pgpass");
        fs::write(
            &passfile,
            "wrong:5432:app:alice:nope\nlocalhost:5432:app:alice:s3cr\\:et\\\\tail\n",
        )
        .expect("write passfile fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&passfile, fs::Permissions::from_mode(0o600))
                .expect("secure passfile permissions");
        }

        let dsn = format!(
            "pgsql:host=localhost;port=5432;dbname=app;user=alice;passfile={}",
            passfile.display()
        );
        let conn_str = parse_dsn(&dsn).expect("passfile DSN resolves");
        assert!(conn_str.contains("password='s3cr:et\\\\tail'"));
        assert!(!conn_str.contains("passfile"));

        fs::remove_dir_all(dir).expect("remove passfile fixture directory");
    }

    /// Legacy libpq TLS aliases and modern protocol/SNI controls are translated
    /// into the rustls configuration without leaking into `postgres::Config`.
    #[test]
    fn parse_tls_honors_legacy_alias_sni_and_protocol_bounds() {
        let tls = parse_tls(
            "pgsql:host=h;requiressl=1;sslcompression=0;sslcertmode=disable;sslsni=0;ssl_min_protocol_version=TLSv1.2;ssl_max_protocol_version=TLSv1.3",
        )
        .expect("extended TLS options parse");
        assert_eq!(tls.mode, "require");
        assert!(!tls.server_name_indication);
        assert_eq!(tls.client_cert_mode, "disable");
        assert_eq!(tls.min_protocol_version.as_deref(), Some("TLSv1.2"));
        assert_eq!(tls.max_protocol_version.as_deref(), Some("TLSv1.3"));
        assert!(parse_dsn(
            "pgsql:host=h;ssl_min_protocol_version=TLSv1.3;ssl_max_protocol_version=TLSv1.2"
        )
        .is_ok());
        assert!(parse_tls(
            "pgsql:host=h;ssl_min_protocol_version=TLSv1.3;ssl_max_protocol_version=TLSv1.2"
        )
        .is_err());
        assert!(parse_tls("pgsql:host=h;sslcertmode=require").is_err());
    }

    /// Building the rustls connector with the bundled webpki roots exercises the
    /// explicit ring `CryptoProvider` and the whole `ClientConfig` builder chain,
    /// catching a provider/API break without needing a live TLS server.
    #[cfg(feature = "tls")]
    #[test]
    fn build_tls_connector_with_webpki_roots_succeeds() {
        let tls = PgTls {
            mode: "require".to_string(),
            ..PgTls::default()
        };
        assert!(build_tls_connector(&tls).is_ok());
    }

    /// A missing custom `sslrootcert` file is a clear, labelled error — not a panic.
    #[cfg(feature = "tls")]
    #[test]
    fn build_tls_connector_missing_ca_errors() {
        let tls = PgTls {
            mode: "verify-full".to_string(),
            root_cert: Some("/nonexistent/elephc-does-not-exist-ca.pem".to_string()),
            ..PgTls::default()
        };
        // `MakeRustlsConnect` has no `Debug`, so match rather than `unwrap_err`.
        match build_tls_connector(&tls) {
            Ok(_) => panic!("expected an error for a missing sslrootcert file"),
            Err(err) => assert!(err.contains("sslrootcert"), "unexpected error: {err}"),
        }
    }

    /// An explicitly configured missing CRL path fails during connector creation.
    #[cfg(feature = "tls")]
    #[test]
    fn build_tls_connector_missing_crl_errors() {
        let tls = PgTls {
            mode: "verify-full".to_string(),
            crl_file: Some("/nonexistent/elephc-does-not-exist.crl".to_string()),
            ..PgTls::default()
        };
        match build_tls_connector(&tls) {
            Ok(_) => panic!("expected an error for a missing sslcrl file"),
            Err(error) => assert!(error.contains("CRL"), "unexpected error: {error}"),
        }
    }
}
