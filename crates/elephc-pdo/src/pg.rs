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
//!   lazily on the first `step()`. The whole result set is materialized into
//!   typed `Cell` values, so the column accessors read from owned data and
//!   per-value NULL is reported through the SQLite-compatible type codes
//!   (1=int, 2=float, 3=text, 4=bytea/blob, 5=null).
//! - Parameter values are encoded according to the prepared statement's inferred
//!   parameter types, so an int bound where the column is `int4` is sent as a
//!   4-byte int, a text where the column is `int` is parsed, etc.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

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
    pub client: Client,
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
/// P1-d: every OTHER key is only forwarded when it is one tokio-postgres's own
/// `Config::from_str` parser recognizes — its accepted set is exactly: `user`,
/// `password`, `dbname`, `options`, `application_name`, `sslmode`,
/// `sslnegotiation`, `host`, `hostaddr`, `port`, `connect_timeout`,
/// `tcp_user_timeout`, `keepalives`, `keepalives_idle`, `keepalives_interval`,
/// `keepalives_retries`, `target_session_attrs`, `channel_binding`,
/// `load_balance_hosts` (`sslmode` is still stripped here, not forwarded — see
/// above). Any libpq key outside that set (e.g. `sslcrl`, `sslpassword`,
/// `sslsni`, `service`, `gssencmode`, `passfile`, `requiressl`,
/// `sslcompression`, `client_encoding`, or a typo) would otherwise make
/// `.parse::<Config>()` fail with a hard `UnknownOption` connect error even
/// though real libpq/PHP connects fine with it. Dropping it instead is a
/// deliberate graceful degradation: the DSN still connects, just without
/// whatever behavior that key would have configured (e.g. `client_encoding`'s
/// value would need a post-connect `SET client_encoding = ...` to have any
/// effect at all — not attempted here) — a silent no-op is preferable to a
/// connection that never happens.
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
pub fn parse_dsn(dsn: &str) -> Result<String, String> {
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
    let body = dsn
        .strip_prefix("pgsql:")
        .ok_or_else(|| "could not find driver (expected a pgsql: DSN)".to_string())?;
    let mut parts: Vec<String> = Vec::new();
    // F-PG-03: tracks whether the caller already bounded the connect (either
    // straight in the DSN or via `ATTR_TIMEOUT`, which the prelude folds into the
    // DSN under the very same key) — if so, that value wins over the 30 s default.
    let mut saw_connect_timeout = false;
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
        // The TLS keys are consumed by `parse_tls`/`open`, not by the libpq
        // connection string: tokio-postgres's parser rejects `sslrootcert`/
        // `sslcert`/`sslkey` and the `verify-ca`/`verify-full` sslmode values, so
        // forwarding any of them would make `.parse::<Config>()` fail.
        if matches!(key, "sslmode" | "sslrootcert" | "sslcert" | "sslkey") {
            continue;
        }
        // P1-d: silently drop any key tokio-postgres's parser does not accept,
        // rather than forwarding it and hard-failing the whole connection.
        if !ACCEPTED_KEYS.contains(&key) {
            continue;
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
        let value = if matches!(key, "user" | "password") {
            percent_decode_credential(value)
        } else {
            value.to_string()
        };
        // libpq connection strings quote values containing spaces/specials; a
        // simple single-quote wrap with backslash-escaping is sufficient here.
        let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
        parts.push(format!("{}='{}'", key, escaped));
    }
    // The emptiness check reads the caller's OWN keys, before the default timeout
    // is folded in: a DSN carrying nothing usable (`pgsql:`) must still be the
    // error it always was, not a connection string made non-empty by our default.
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
#[derive(Default)]
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
}

/// Extracts the TLS parameters from a `pgsql:` DSN (the keys [`parse_dsn`]
/// deliberately drops). Unknown keys are ignored; a DSN without the `pgsql:`
/// prefix yields the default (unset) parameters.
fn parse_tls(dsn: &str) -> PgTls {
    let mut tls = PgTls::default();
    let Some(body) = dsn.strip_prefix("pgsql:") else {
        return tls;
    };
    for pair in body.split(';') {
        let Some((key, value)) = pair.trim().split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "sslmode" => tls.mode = value.to_ascii_lowercase(),
            "sslrootcert" => tls.root_cert = Some(value),
            "sslcert" => tls.client_cert = Some(value),
            "sslkey" => tls.client_key = Some(value),
            _ => {}
        }
    }
    tls
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

    let builder = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_root_certificates(roots);

    let config = match (&tls.client_cert, &tls.client_key) {
        (Some(cert), Some(key)) => {
            let chain = load_certs(cert, "sslcert")?;
            let der = load_private_key(key)?;
            builder
                .with_client_auth_cert(chain, der)
                .map_err(|e| e.to_string())?
        }
        _ => builder.with_no_client_auth(),
    };
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
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
pub fn translate_placeholders(sql: &str) -> (String, HashMap<String, i64>, bool) {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut named: HashMap<String, i64> = HashMap::new();
    let mut next_index: i64 = 1;
    let mut i = 0;
    let mut in_string = false;
    // Whether the currently-open string honors backslash escapes (an
    // `E'...'`/`e'...'` string); irrelevant while `in_string` is false.
    let mut string_escapes = false;
    let mut saw_positional = false;
    let mut saw_named = false;
    while i < len {
        let c = bytes[i];
        if in_string {
            if string_escapes && c == b'\\' && i + 1 < len {
                // A backslash escapes the next character in an E-string
                // (which may itself be a multi-byte UTF-8 sequence): neither
                // participates in terminating the string. Copy the whole
                // escaped character via a slice rather than a per-byte `as
                // char` push (BUG 1) — pushing only the escaped byte's lead
                // byte would also leave its continuation bytes to be
                // re-visited at a non-char-boundary index on the next
                // iteration.
                let esc_len = utf8_len(bytes[i + 1]).min(len - i - 1);
                out.push('\\');
                out.push_str(&sql[i + 1..i + 1 + esc_len]);
                i += 1 + esc_len;
                continue;
            }
            let n = utf8_len(c).min(len - i);
            out.push_str(&sql[i..i + n]);
            if c == b'\'' {
                // Doubled '' is an escaped quote inside the literal.
                if i + 1 < len && bytes[i + 1] == b'\'' {
                    out.push('\'');
                    i += 2;
                    continue;
                }
                in_string = false;
            }
            i += n;
            continue;
        }
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
                // Block comment: verbatim to the matching (non-nested) `*/`, or
                // to EOF if unterminated.
                let start = i;
                let mut j = i + 2;
                while j + 1 < len && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                    j += 1;
                }
                let end = if j + 1 < len { j + 2 } else { len };
                out.push_str(&sql[start..end]);
                i = end;
            }
            b'"' => {
                // Double-quoted identifier: verbatim, with `""` as the doubled-
                // quote escape (no backslash escaping here).
                let start = i;
                let mut j = i + 1;
                loop {
                    if j >= len {
                        break;
                    }
                    if bytes[j] == b'"' {
                        if j + 1 < len && bytes[j + 1] == b'"' {
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
            b'\'' => {
                // A standalone `E`/`e` immediately before this quote (not part
                // of a longer identifier) makes this an escape-string.
                let is_e_prefixed = i > 0
                    && (bytes[i - 1] == b'E' || bytes[i - 1] == b'e')
                    && (i < 2 || !is_ident_byte(bytes[i - 2]));
                in_string = true;
                string_escapes = is_e_prefixed;
                out.push('\'');
                i += 1;
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
                            // Unterminated dollar-quote: consume verbatim to EOF.
                            out.push_str(&sql[i..len]);
                            i = len;
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
                out.push('$');
                out.push_str(&next_index.to_string());
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
                out.push('$');
                out.push_str(&index.to_string());
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
    (out, named, mixed)
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
        let conn_str = parse_dsn(dsn)?;
        let tls = parse_tls(dsn);
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
        let client = connect_tls(&mut config, &tls)?;
        Ok(PgConn {
            client,
            changes: 0,
            errmsg: String::new(),
            errcode: 0,
            sqlstate: "00000".to_string(),
            notices,
        })
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
        // execute() runs a single command; fall back to a multi-statement path for
        // scripts execute() rejects (it only accepts exactly one command).
        match self.client.execute(sql, &[]) {
            Ok(n) => {
                self.changes = n as i64;
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
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
                    rows as i64
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
                self.fail(e);
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
        match self.client.query_one("SHOW server_version", &[]) {
            Ok(row) => row.try_get::<_, String>(0).unwrap_or_default(),
            Err(_) => String::new(),
        }
    }

    /// Returns the PostgreSQL backend process id serving this connection
    /// (`SELECT pg_backend_pid()`), or 0 if the query fails. Backs
    /// `Pdo\Pgsql::getPid()`.
    pub fn backend_pid(&mut self) -> i64 {
        match self.client.query_one("SELECT pg_backend_pid()", &[]) {
            Ok(row) => row.try_get::<_, i32>(0).map(i64::from).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Creates a new empty large object and returns its OID as a decimal string
    /// (`SELECT lo_create(0)`), or an empty string on error. Backs
    /// `Pdo\Pgsql::lobCreate()`.
    pub fn lob_create(&mut self) -> String {
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
    /// standalone (no explicit transaction). Backs `Pdo\Pgsql::lobOpen()` (read-whole).
    pub fn lob_get(&mut self, oid: &str) -> Option<Vec<u8>> {
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

    /// Streams `data` into the server for a `COPY … FROM STDIN` statement (built by
    /// the prelude), returning the number of rows copied or -1 on error. Backs
    /// `Pdo\Pgsql::copyFromArray()` / `copyFromFile()`.
    pub fn copy_in(&mut self, copy_sql: &str, data: &[u8]) -> i64 {
        use std::io::Write;
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
    pub fn prepare(&mut self, sql: &str) -> Result<PgStmt, String> {
        let (translated, named_map, mixed) = translate_placeholders(sql);
        if mixed {
            return Err(self.fail_local(
                "HY093",
                "Invalid parameter number: mixed named and positional parameters",
            ));
        }
        match self.client.prepare(&translated) {
            Ok(statement) => {
                let col_names = statement
                    .columns()
                    .iter()
                    .map(|c| c.name().to_string())
                    .collect();
                let n_params = statement.params().len();
                self.errcode = 0;
                self.sqlstate = "00000".to_string();
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
                let msg = e.to_string();
                self.fail(e);
                Err(msg)
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
                    conn.sqlstate = "00000".to_string();
                    self.executed = true;
                    Ok(())
                }
                // `fail` records the SQLSTATE/message/native code and yields the `-1`
                // that `step()` propagates as the statement's error return.
                Err(e) => Err(conn.fail(e)),
            }
        } else {
            match conn.client.query(&self.statement, &refs) {
                Ok(rows) => {
                    self.rows = rows.iter().map(|r| decode_row(r)).collect();
                    conn.changes = self.rows.len() as i64;
                    conn.errcode = 0;
                    conn.sqlstate = "00000".to_string();
                    self.executed = true;
                    Ok(())
                }
                Err(e) => Err(conn.fail(e)),
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
            .columns()
            .get(i as usize)
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
            .columns()
            .get(i as usize)
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
            .columns()
            .get(i as usize)
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
            .columns()
            .get(i as usize)
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
            .columns()
            .get(i as usize)
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

    /// P1-d: an unrecognized-but-real libpq key (`sslcrl`) and a key
    /// tokio-postgres simply doesn't model (`client_encoding`) are dropped
    /// rather than forwarded, so the DSN still parses into a connection string
    /// instead of hard-failing with `UnknownOption`.
    #[test]
    fn parse_dsn_drops_unrecognized_libpq_keys() {
        let dsn = "pgsql:host=db.example.com;dbname=app;sslcrl=/x;client_encoding=UTF8";
        let conn_str = parse_dsn(dsn).expect("dsn parses despite the unrecognized keys");
        assert!(conn_str.contains("host='db.example.com'"));
        assert!(conn_str.contains("dbname='app'"));
        assert!(
            !conn_str.contains("sslcrl"),
            "sslcrl must not reach the libpq conn string: {conn_str}"
        );
        assert!(
            !conn_str.contains("client_encoding"),
            "client_encoding must not reach the libpq conn string: {conn_str}"
        );
        // The whole point: tokio-postgres's own parser must accept the result.
        conn_str
            .parse::<Config>()
            .expect("conn string with dropped keys must still parse");
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
            "pgsql:host=h;sslmode=VERIFY-FULL;sslrootcert=/ca.pem;sslcert=/c.pem;sslkey=/k.pem",
        );
        assert_eq!(tls.mode, "verify-full");
        assert_eq!(tls.root_cert.as_deref(), Some("/ca.pem"));
        assert_eq!(tls.client_cert.as_deref(), Some("/c.pem"));
        assert_eq!(tls.client_key.as_deref(), Some("/k.pem"));
    }

    /// A DSN without TLS keys yields the unset defaults (libpq/tokio-postgres both
    /// default to `prefer`, represented here by an empty mode).
    #[test]
    fn parse_tls_defaults_when_absent() {
        let tls = parse_tls("pgsql:host=h;dbname=d");
        assert!(tls.mode.is_empty());
        assert!(tls.root_cert.is_none());
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
}
