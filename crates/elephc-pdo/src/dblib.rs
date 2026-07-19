//! Purpose:
//! FreeTDS DB-Library backend matching php-src's `pdo_dblib` driver.
//!
//! Called from:
//! - The bridge root when built with the optional `dblib` feature.
//!
//! Key details:
//! - Calls the same `libsybdb` client API as PHP instead of reimplementing TDS.
//! - Materializes DB-Library rowsets so bridge handles remain independent and safe.
//! - PDO placeholders are emulated client-side because DB-Library has no prepare API.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_uchar, CStr, CString};
use std::ptr;
use std::sync::{Mutex, Once, OnceLock};

const SUCCEED: c_int = 1;
const FAIL: c_int = 0;
const NO_MORE_ROWS: c_int = -2;
const NO_MORE_RESULTS: c_int = 2;
const INT_CANCEL: c_int = 2;

const DBSETUSER: c_int = 2;
const DBSETPWD: c_int = 3;
const DBSETAPP: c_int = 5;
const DBSETCHARSET: c_int = 10;
const DBSETDBNAME: c_int = 14;
const DBTEXTSIZE: c_int = 17;
const DBQUOTEDIDENT: c_int = 35;

const SYBIMAGE: c_int = 34;
const SYBTEXT: c_int = 35;
const SYBUNIQUE: c_int = 36;
const SYBVARBINARY: c_int = 37;
const SYBINTN: c_int = 38;
const SYBVARCHAR: c_int = 39;
const SYBMSDATE: c_int = 40;
const SYBMSTIME: c_int = 41;
const SYBMSDATETIME2: c_int = 42;
const SYBMSDATETIMEOFFSET: c_int = 43;
const SYBBINARY: c_int = 45;
const SYBCHAR: c_int = 47;
const SYBINT1: c_int = 48;
const SYBBIT: c_int = 50;
const SYBINT2: c_int = 52;
const SYBINT4: c_int = 56;
const SYBDATETIME4: c_int = 58;
const SYBREAL: c_int = 59;
const SYBMONEY: c_int = 60;
const SYBDATETIME: c_int = 61;
const SYBFLT8: c_int = 62;
const SYBNTEXT: c_int = 99;
const SYBBITN: c_int = 104;
const SYBDECIMAL: c_int = 106;
const SYBNUMERIC: c_int = 108;
const SYBFLTN: c_int = 109;
const SYBMONEYN: c_int = 110;
const SYBINT8: c_int = 127;
const SYBNVARCHAR: c_int = 103;
const SYBMONEY4: c_int = 122;

/// FreeTDS's Sybase-layout broken-down date record used by `dbdatecrack`.
#[repr(C)]
struct DbDateRec2 {
    dateyear: c_int,
    quarter: c_int,
    datemonth: c_int,
    datedmonth: c_int,
    datedyear: c_int,
    week: c_int,
    datedweek: c_int,
    datehour: c_int,
    dateminute: c_int,
    datesecond: c_int,
    datensecond: c_int,
    datetzone: c_int,
}

/// FreeTDS's two-word classic datetime representation accepted by `dbdatecrack`.
#[repr(C)]
struct DbDateTime {
    days: c_int,
    time: c_int,
}

#[repr(C)]
struct DbProcess {
    _private: [u8; 0],
}

#[repr(C)]
struct LoginRecord {
    _private: [u8; 0],
}

/// FreeTDS precision/scale descriptor returned for one result column.
#[repr(C)]
struct DbTypeInfo {
    precision: c_int,
    scale: c_int,
}

type ErrorHandler = unsafe extern "C" fn(
    *mut DbProcess,
    c_int,
    c_int,
    c_int,
    *mut c_char,
    *mut c_char,
) -> c_int;

type MessageHandler = unsafe extern "C" fn(
    *mut DbProcess,
    c_int,
    c_int,
    c_int,
    *mut c_char,
    *mut c_char,
    *mut c_char,
    c_int,
) -> c_int;

#[link(name = "sybdb")]
extern "C" {
    fn dbinit() -> c_int;
    fn dblogin() -> *mut LoginRecord;
    fn dbloginfree(login: *mut LoginRecord);
    fn dbsetlname(login: *mut LoginRecord, value: *const c_char, which: c_int) -> c_int;
    fn dbsetlversion(login: *mut LoginRecord, version: c_uchar) -> c_int;
    fn dbsetlogintime(seconds: c_int) -> c_int;
    fn dbsettime(seconds: c_int) -> c_int;
    fn dbopen(login: *mut LoginRecord, server: *const c_char) -> *mut DbProcess;
    fn dbclose(process: *mut DbProcess);
    fn dbdead(process: *mut DbProcess) -> c_int;
    fn dbcmd(process: *mut DbProcess, sql: *const c_char) -> c_int;
    fn dbsqlexec(process: *mut DbProcess) -> c_int;
    fn dbresults(process: *mut DbProcess) -> c_int;
    fn dbnextrow(process: *mut DbProcess) -> c_int;
    fn dbnumcols(process: *mut DbProcess) -> c_int;
    fn dbcolname(process: *mut DbProcess, column: c_int) -> *mut c_char;
    fn dbcoltype(process: *mut DbProcess, column: c_int) -> c_int;
    fn dbcollen(process: *mut DbProcess, column: c_int) -> c_int;
    fn dbcolsource(process: *mut DbProcess, column: c_int) -> *mut c_char;
    fn dbcoltypeinfo(process: *mut DbProcess, column: c_int) -> *mut DbTypeInfo;
    fn dbcolutype(process: *mut DbProcess, column: c_int) -> c_int;
    fn dbdata(process: *mut DbProcess, column: c_int) -> *mut c_uchar;
    fn dbdatlen(process: *mut DbProcess, column: c_int) -> c_int;
    fn dbcount(process: *mut DbProcess) -> c_int;
    fn dbcancel(process: *mut DbProcess) -> c_int;
    fn dbsetopt(
        process: *mut DbProcess,
        option: c_int,
        char_parameter: *const c_char,
        int_parameter: c_int,
    ) -> c_int;
    fn dbconvert(
        process: *mut DbProcess,
        source_type: c_int,
        source: *const c_uchar,
        source_len: c_int,
        dest_type: c_int,
        dest: *mut c_uchar,
        dest_len: c_int,
    ) -> c_int;
    fn dbdatecrack(
        process: *mut DbProcess,
        record: *mut DbDateRec2,
        datetime: *mut DbDateTime,
    ) -> c_int;
    fn dbsetuserdata(process: *mut DbProcess, data: *mut c_uchar);
    fn dbgetuserdata(process: *mut DbProcess) -> *mut c_uchar;
    fn dberrhandle(handler: Option<ErrorHandler>) -> Option<ErrorHandler>;
    fn dbmsghandle(handler: Option<MessageHandler>) -> Option<MessageHandler>;
    fn dbversion() -> *const c_char;
    fn dbtds(process: *mut DbProcess) -> c_int;
}

/// Native DB-Library diagnostic state for one connection/statement operation.
#[derive(Clone, Default)]
struct ErrorState {
    sqlstate: String,
    native_code: i64,
    message: String,
    os_code: i64,
    severity: i64,
    os_message: String,
}

/// Last diagnostic raised before DB-Library has produced a connection handle.
fn open_error() -> &'static Mutex<ErrorState> {
    static ERROR: OnceLock<Mutex<ErrorState>> = OnceLock::new();
    ERROR.get_or_init(|| Mutex::new(ErrorState::default()))
}

/// Converts a possibly-null DB-Library C string into an owned Rust string.
unsafe fn owned_cstr(value: *const c_char) -> String {
    if value.is_null() {
        String::new()
    } else {
        CStr::from_ptr(value).to_string_lossy().into_owned()
    }
}

/// Maps DB-Library client codes to php-src's PDO_DBLIB SQLSTATE classes.
fn sqlstate_for_db_error(db_error: c_int) -> &'static str {
    match db_error {
        20017 | 20002 => "01002",
        20010 => "HY001",
        20014 => "28000",
        _ => "HY000",
    }
}

/// Receives DB-Library client errors and stores the SQLSTATE/native diagnostic.
unsafe extern "C" fn error_handler(
    process: *mut DbProcess,
    severity: c_int,
    db_error: c_int,
    os_error: c_int,
    db_message: *mut c_char,
    os_message: *mut c_char,
) -> c_int {
    let sqlstate = sqlstate_for_db_error(db_error);
    let mut message = owned_cstr(db_message);
    let os = owned_cstr(os_message);
    if !os.is_empty() {
        if !message.is_empty() {
            message.push_str(": ");
        }
        message.push_str(&os);
    }
    if !process.is_null() {
        let state = dbgetuserdata(process) as *mut ErrorState;
        if !state.is_null() {
            (*state).sqlstate = sqlstate.to_string();
            (*state).native_code = i64::from(db_error);
            (*state).message = message;
            (*state).os_code = i64::from(os_error);
            (*state).severity = i64::from(severity);
            (*state).os_message = os;
            return INT_CANCEL;
        }
    }
    if let Ok(mut state) = open_error().lock() {
        state.sqlstate = sqlstate.to_string();
        state.native_code = i64::from(db_error);
        state.message = message;
        state.os_code = i64::from(os_error);
        state.severity = i64::from(severity);
        state.os_message = os;
    }
    INT_CANCEL
}

/// Receives server messages and retains messages with a non-zero severity.
unsafe extern "C" fn message_handler(
    process: *mut DbProcess,
    message_number: c_int,
    _message_state: c_int,
    severity: c_int,
    message: *mut c_char,
    _server: *mut c_char,
    _procedure: *mut c_char,
    _line: c_int,
) -> c_int {
    if severity == 0 || process.is_null() {
        return 0;
    }
    let state = dbgetuserdata(process) as *mut ErrorState;
    if !state.is_null() {
        let state = &mut *state;
        let _ = message_number;
        state.message = owned_cstr(message);
        if state.sqlstate.is_empty() {
            state.sqlstate = "HY000".to_string();
        }
    }
    0
}

/// Initializes FreeTDS and installs process-global DB-Library callbacks once.
fn initialize() -> Result<(), String> {
    static INIT: Once = Once::new();
    static RESULT: OnceLock<Result<(), String>> = OnceLock::new();
    INIT.call_once(|| unsafe {
        let result = if dbinit() == FAIL {
            Err("PDO_DBLIB: dbinit() failed".to_string())
        } else {
            dberrhandle(Some(error_handler));
            dbmsghandle(Some(message_handler));
            Ok(())
        };
        let _ = RESULT.set(result);
    });
    RESULT
        .get()
        .cloned()
        .unwrap_or_else(|| Err("PDO_DBLIB: initialization failed".to_string()))
}

/// Parsed DBLIB DSN fields with php-src-compatible defaults.
struct DsnOptions {
    host: String,
    port: Option<i32>,
    dbname: Option<String>,
    user: Option<String>,
    password: Option<String>,
    charset: Option<String>,
    appname: String,
    version: Option<String>,
    connection_timeout: i32,
    query_timeout: i32,
    stringify_uniqueidentifier: bool,
    skip_empty_rowsets: bool,
    datetime_convert: bool,
}

/// Decodes the narrow percent encoding used when constructor credentials are folded into a DSN.
fn percent_decode_credential(raw: &str) -> String {
    raw.replace("%3B", ";")
        .replace("%3b", ";")
        .replace("%25", "%")
}

/// Parses one integer-like constructor flag stored in the internal DBLIB DSN.
fn dsn_bool(values: &HashMap<String, String>, key: &str) -> bool {
    values
        .get(key)
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|value| value != 0)
}

/// Parses the semicolon-separated `dblib:` DSN accepted by PDO_DBLIB.
fn parse_dsn(dsn: &str) -> Result<DsnOptions, String> {
    let body = dsn
        .strip_prefix("dblib:")
        .ok_or_else(|| "could not find driver".to_string())?;
    let mut values = HashMap::new();
    for pair in body.split(';').filter(|pair| !pair.is_empty()) {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        values.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let timeout = values
        .get("timeout")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(30);
    let port = values.get("port").and_then(|value| value.parse::<i32>().ok());
    Ok(DsnOptions {
        host: values
            .remove("host")
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        port,
        dbname: values.remove("dbname"),
        user: values
            .remove("user")
            .map(|value| percent_decode_credential(&value)),
        password: values
            .remove("password")
            .map(|value| percent_decode_credential(&value)),
        charset: values.remove("charset"),
        appname: values
            .remove("appname")
            .unwrap_or_else(|| "PHP FreeTDS".to_string()),
        version: values.remove("version"),
        connection_timeout: values
            .get("connection_timeout")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(timeout),
        query_timeout: values
            .get("query_timeout")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(timeout),
        stringify_uniqueidentifier: dsn_bool(&values, "stringify_uniqueidentifier"),
        skip_empty_rowsets: dsn_bool(&values, "skip_empty_rowsets"),
        datetime_convert: dsn_bool(&values, "datetime_convert"),
    })
}

/// Builds the server name passed to `dbopen`, using FreeTDS's documented port override syntax.
fn server_name(options: &DsnOptions) -> String {
    match options.port {
        Some(port) => format!("{}:{}", options.host, port),
        None => options.host.clone(),
    }
}

/// Maps php-src's accepted PDO_DBLIB DSN version spellings to FreeTDS constants.
fn tds_login_version(value: &str) -> Option<c_uchar> {
    match value {
        "4.2" => Some(3),
        "4.6" => Some(1),
        "5.0" | "6.0" | "7.0" => Some(4),
        "7.1" => Some(5),
        "7.2" | "8.0" => Some(6),
        "7.3" => Some(7),
        "7.4" => Some(8),
        "10.0" => Some(2),
        "auto" => Some(0),
        _ => None,
    }
}

/// Sets one string-valued DB-Library login property when a value is present.
unsafe fn set_login_string(
    login: *mut LoginRecord,
    value: Option<&str>,
    property: c_int,
) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let value = CString::new(value).map_err(|_| "PDO_DBLIB: DSN contains NUL".to_string())?;
    if dbsetlname(login, value.as_ptr(), property) == FAIL {
        Err("PDO_DBLIB: failed to set login property".to_string())
    } else {
        Ok(())
    }
}

/// Live FreeTDS DB-Library connection.
pub struct DblibConn {
    link: *mut DbProcess,
    error: Box<ErrorState>,
    pub changes: i64,
    pub in_transaction: bool,
    stringify_uniqueidentifier: bool,
    skip_empty_rowsets: bool,
    datetime_convert: bool,
}

// DB-Library handles are used only while the bridge connection-table mutex is held.
unsafe impl Send for DblibConn {}

impl Drop for DblibConn {
    /// Cancels pending results and closes the native DBPROCESS.
    fn drop(&mut self) {
        unsafe {
            if !self.link.is_null() {
                dbcancel(self.link);
                dbclose(self.link);
            }
        }
    }
}

impl DblibConn {
    /// Opens a FreeTDS connection from a PDO `dblib:` DSN.
    pub fn open(dsn: &str) -> Result<Self, String> {
        initialize()?;
        let options = parse_dsn(dsn)?;
        if let Ok(mut error) = open_error().lock() {
            *error = ErrorState::default();
        }
        unsafe {
            dbsetlogintime(options.connection_timeout.max(0));
            dbsettime(options.query_timeout.max(0));
            let login = dblogin();
            if login.is_null() {
                return Err("PDO_DBLIB: dblogin() failed".to_string());
            }
            let configured = set_login_string(login, options.user.as_deref(), DBSETUSER)
                .and_then(|_| set_login_string(login, options.password.as_deref(), DBSETPWD))
                .and_then(|_| set_login_string(login, Some(&options.appname), DBSETAPP))
                .and_then(|_| set_login_string(login, options.charset.as_deref(), DBSETCHARSET))
                .and_then(|_| set_login_string(login, options.dbname.as_deref(), DBSETDBNAME));
            if let Some(version) = options.version.as_deref() {
                let Some(version) = tds_login_version(version) else {
                    dbloginfree(login);
                    return Err("PDO_DBLIB: Invalid version specified in connection string.".to_string());
                };
                if configured.is_ok() && dbsetlversion(login, version) == FAIL {
                    dbloginfree(login);
                    return Err("PDO_DBLIB: Failed to set version specified in connection string.".to_string());
                }
            }
            if let Err(error) = configured {
                dbloginfree(login);
                return Err(error);
            }
            let host = CString::new(server_name(&options))
                .map_err(|_| "PDO_DBLIB: host contains NUL".to_string())?;
            let link = dbopen(login, host.as_ptr());
            dbloginfree(login);
            if link.is_null() {
                let message = open_error()
                    .lock()
                    .ok()
                    .map(|error| error.message.clone())
                    .filter(|message| !message.is_empty())
                    .unwrap_or_else(|| "PDO_DBLIB: unable to connect".to_string());
                return Err(message);
            }
            let max_text = b"2147483647\0";
            let quoted_identifiers = b"1\0";
            dbsetopt(link, DBTEXTSIZE, max_text.as_ptr().cast::<c_char>(), -1);
            dbsetopt(
                link,
                DBQUOTEDIDENT,
                quoted_identifiers.as_ptr().cast::<c_char>(),
                -1,
            );
            let mut error = Box::new(ErrorState::default());
            dbsetuserdata(link, (&mut *error as *mut ErrorState).cast::<c_uchar>());
            Ok(Self {
                link,
                error,
                changes: 0,
                in_transaction: false,
                stringify_uniqueidentifier: options.stringify_uniqueidentifier,
                skip_empty_rowsets: options.skip_empty_rowsets,
                datetime_convert: options.datetime_convert,
            })
        }
    }

    /// Reports whether FreeTDS considers this connection dead.
    pub fn is_alive(&self) -> bool {
        unsafe { dbdead(self.link) == 0 }
    }

    /// Clears the connection diagnostic before a new operation.
    fn clear_error(&mut self) {
        *self.error = ErrorState::default();
        unsafe { dbsetuserdata(self.link, (&mut *self.error as *mut ErrorState).cast::<c_uchar>()) };
    }

    /// Returns the connection's current five-character SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        if self.error.sqlstate.is_empty() {
            "00000"
        } else {
            &self.error.sqlstate
        }
    }

    /// Returns the current DB-Library/server native error number.
    pub fn errcode(&self) -> i64 {
        self.error.native_code
    }

    /// Returns the current DB-Library/server diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }

    /// Returns the DB-Library operating-system error code for extended errorInfo.
    pub fn os_errcode(&self) -> i64 {
        self.error.os_code
    }

    /// Returns the DB-Library error severity for extended errorInfo.
    pub fn severity(&self) -> i64 {
        self.error.severity
    }

    /// Returns the DB-Library operating-system diagnostic text.
    pub fn os_errmsg(&self) -> &str {
        &self.error.os_message
    }

    /// Records a bridge-generated error on the connection for PDO diagnostics.
    pub fn set_error(&mut self, sqlstate: &str, native_code: i64, message: String) {
        *self.error = ErrorState {
            sqlstate: sqlstate.to_string(),
            native_code,
            message,
            ..ErrorState::default()
        };
    }

    /// Applies a writable PDO_DBLIB connection attribute.
    pub fn set_attribute(&mut self, attribute: i64, value: i64) -> bool {
        match attribute {
            2 | 1001 => unsafe { dbsettime(value as c_int) == SUCCEED },
            1002 => {
                self.stringify_uniqueidentifier = value != 0;
                true
            }
            1005 => {
                self.skip_empty_rowsets = value != 0;
                true
            }
            1006 => {
                self.datetime_convert = value != 0;
                true
            }
            _ => false,
        }
    }

    /// Reads a boolean PDO_DBLIB attribute, or `None` for a non-readable one.
    pub fn attribute_bool(&self, attribute: i64) -> Option<bool> {
        match attribute {
            1002 => Some(self.stringify_uniqueidentifier),
            1005 => Some(self.skip_empty_rowsets),
            1006 => Some(self.datetime_convert),
            _ => None,
        }
    }

    /// Executes SQL and materializes every DB-Library result set.
    pub fn execute(&mut self, sql: &str) -> Result<Vec<DblibRowset>, String> {
        self.clear_error();
        unsafe { dbcancel(self.link) };
        let sql = CString::new(sql).map_err(|_| "PDO_DBLIB: SQL contains NUL".to_string())?;
        if unsafe { dbcmd(self.link, sql.as_ptr()) } == FAIL
            || unsafe { dbsqlexec(self.link) } == FAIL
        {
            return Err(self.operation_error("PDO_DBLIB: query execution failed"));
        }
        let rowsets = unsafe { self.collect_rowsets()? };
        self.changes = rowsets.first().map_or(0, |rowset| rowset.row_count);
        Ok(rowsets)
    }

    /// Executes one transaction-control command and updates local PDO state.
    pub fn transaction(&mut self, sql: &str, active_after: bool) -> bool {
        match self.execute(sql) {
            Ok(_) => {
                self.in_transaction = active_after;
                true
            }
            Err(_) => false,
        }
    }

    /// Returns the FreeTDS library version string.
    pub fn client_version(&self) -> String {
        unsafe { owned_cstr(dbversion()) }
    }

    /// Returns the negotiated TDS protocol version.
    pub fn tds_version(&self) -> &'static str {
        match unsafe { dbtds(self.link) } {
            1 => "2.0",
            2 => "3.4",
            3 => "4.0",
            4 => "4.2",
            5 => "4.6",
            6 => "4.9.5",
            7 => "5.0",
            8 => "7.0",
            9 => "7.1",
            10 => "7.2",
            11 => "7.3",
            12 => "7.4",
            _ => "",
        }
    }

    /// Builds a stable error string when the native callback supplied no text.
    fn operation_error(&self, fallback: &str) -> String {
        if self.error.message.is_empty() {
            fallback.to_string()
        } else {
            self.error.message.clone()
        }
    }

    /// Drains all DB-Library results into bridge-owned rowsets.
    unsafe fn collect_rowsets(&mut self) -> Result<Vec<DblibRowset>, String> {
        let mut rowsets = Vec::new();
        let mut computed_column_count = 0usize;
        loop {
            match dbresults(self.link) {
                NO_MORE_RESULTS => break,
                FAIL => return Err(self.operation_error("PDO_DBLIB: dbresults() returned FAIL")),
                SUCCEED => {
                    let column_count = dbnumcols(self.link).max(0) as usize;
                    let columns = (0..column_count)
                        .map(|index| {
                            read_column(self.link, index + 1, &mut computed_column_count)
                        })
                        .collect();
                    let mut rows = Vec::new();
                    loop {
                        match dbnextrow(self.link) {
                            NO_MORE_ROWS => break,
                            FAIL => {
                                return Err(self.operation_error(
                                    "PDO_DBLIB: dbnextrow() returned FAIL",
                                ))
                            }
                            _ => {
                                rows.push(
                                    (0..column_count)
                                        .map(|index| {
                                            read_cell(
                                                self.link,
                                                index + 1,
                                                self.stringify_uniqueidentifier,
                                                self.datetime_convert,
                                            )
                                        })
                                        .collect(),
                                );
                            }
                        }
                    }
                    if !self.skip_empty_rowsets || column_count > 0 {
                        rowsets.push(DblibRowset {
                            columns,
                            rows,
                            row_count: i64::from(dbcount(self.link)),
                        });
                    }
                }
                _ => return Err("PDO_DBLIB: unexpected dbresults() status".to_string()),
            }
        }
        Ok(rowsets)
    }
}

/// One materialized result column.
#[derive(Clone)]
pub struct DblibColumn {
    pub name: String,
    pub native_type: c_int,
    pub max_len: i64,
    pub precision: i64,
    pub scale: i64,
    pub source: String,
    pub user_type: i64,
}

/// One materialized DB-Library cell in the bridge's common PDO value shape.
#[derive(Clone)]
pub enum DblibCell {
    Null,
    Int(i64),
    Float(f64),
    Bytes(Vec<u8>, bool),
}

/// One materialized DB-Library rowset.
pub struct DblibRowset {
    pub columns: Vec<DblibColumn>,
    pub rows: Vec<Vec<DblibCell>>,
    pub row_count: i64,
}

/// Reads metadata for one one-based DB-Library column.
unsafe fn read_column(
    process: *mut DbProcess,
    column: usize,
    computed_column_count: &mut usize,
) -> DblibColumn {
    let raw_name = dbcolname(process, column as c_int);
    let name = if raw_name.is_null() || *raw_name == 0 {
        let name = if *computed_column_count == 0 {
            "computed".to_string()
        } else {
            format!("computed{}", *computed_column_count)
        };
        *computed_column_count += 1;
        name
    } else {
        owned_cstr(raw_name)
    };
    let type_info = dbcoltypeinfo(process, column as c_int);
    let (precision, scale) = if type_info.is_null() {
        (0, 0)
    } else {
        (i64::from((*type_info).precision), i64::from((*type_info).scale))
    };
    DblibColumn {
        name,
        native_type: dbcoltype(process, column as c_int),
        max_len: i64::from(dbcollen(process, column as c_int)),
        precision,
        scale,
        source: owned_cstr(dbcolsource(process, column as c_int)),
        user_type: i64::from(dbcolutype(process, column as c_int)),
    }
}

/// Reads and converts one one-based DB-Library cell.
unsafe fn read_cell(
    process: *mut DbProcess,
    column: usize,
    stringify_uniqueidentifier: bool,
    datetime_convert: bool,
) -> DblibCell {
    let data = dbdata(process, column as c_int);
    let len = dbdatlen(process, column as c_int);
    if data.is_null() && len == 0 {
        return DblibCell::Null;
    }
    let native_type = dbcoltype(process, column as c_int);
    match native_type {
        SYBINT1 | SYBBIT => DblibCell::Int(i64::from(ptr::read_unaligned(data))),
        SYBINT2 => DblibCell::Int(i64::from(ptr::read_unaligned(data.cast::<i16>()))),
        SYBINT4 => DblibCell::Int(i64::from(ptr::read_unaligned(data.cast::<i32>()))),
        SYBINT8 => DblibCell::Int(ptr::read_unaligned(data.cast::<i64>())),
        SYBREAL => DblibCell::Float(f64::from(ptr::read_unaligned(data.cast::<f32>()))),
        SYBFLT8 => DblibCell::Float(ptr::read_unaligned(data.cast::<f64>())),
        SYBDECIMAL | SYBNUMERIC | SYBMONEY | SYBMONEY4 | SYBMONEYN => {
            let mut value = 0.0f64;
            let converted = dbconvert(
                ptr::null_mut(),
                native_type,
                data,
                len,
                SYBFLT8,
                (&mut value as *mut f64).cast::<c_uchar>(),
                std::mem::size_of::<f64>() as c_int,
            );
            if converted < 0 {
                converted_text(process, native_type, data, len)
            } else {
                DblibCell::Float(value)
            }
        }
        SYBUNIQUE if stringify_uniqueidentifier => {
            converted_uniqueidentifier(native_type, data, len)
        }
        SYBBINARY | SYBVARBINARY | SYBIMAGE | SYBUNIQUE => {
            DblibCell::Bytes(std::slice::from_raw_parts(data, len.max(0) as usize).to_vec(), true)
        }
        SYBCHAR | SYBVARCHAR | SYBTEXT | SYBNTEXT | SYBNVARCHAR => {
            DblibCell::Bytes(std::slice::from_raw_parts(data, len.max(0) as usize).to_vec(), false)
        }
        SYBDATETIME | SYBDATETIME4 | SYBMSDATETIME2 if !datetime_convert => {
            cracked_datetime(process, native_type, data)
        }
        SYBDATETIME | SYBDATETIME4 | SYBMSDATE | SYBMSTIME | SYBMSDATETIME2
        | SYBMSDATETIMEOFFSET | SYBINTN | SYBFLTN | SYBBITN => {
            converted_text(process, native_type, data, len)
        }
        _ => converted_text(process, native_type, data, len),
    }
}

/// Converts a SQL Server uniqueidentifier to php-src's uppercase 36-byte text form.
unsafe fn converted_uniqueidentifier(
    native_type: c_int,
    data: *const c_uchar,
    len: c_int,
) -> DblibCell {
    let mut buffer = vec![0u8; 37];
    let converted = dbconvert(
        ptr::null_mut(),
        native_type,
        data,
        len,
        SYBCHAR,
        buffer.as_mut_ptr(),
        36,
    );
    if converted <= 0 {
        return DblibCell::Bytes(Vec::new(), false);
    }
    buffer.truncate(converted as usize);
    buffer.make_ascii_uppercase();
    DblibCell::Bytes(buffer, false)
}

/// Formats DBLIB datetime values using php-src's fixed second-resolution representation.
unsafe fn cracked_datetime(
    process: *mut DbProcess,
    native_type: c_int,
    data: *const c_uchar,
) -> DblibCell {
    let mut datetime = std::mem::MaybeUninit::<DbDateTime>::zeroed();
    if dbconvert(
        process,
        native_type,
        data,
        -1,
        SYBDATETIME,
        datetime.as_mut_ptr().cast::<c_uchar>(),
        -1,
    ) <= 0
    {
        return DblibCell::Bytes(Vec::new(), false);
    }
    let mut record = std::mem::MaybeUninit::<DbDateRec2>::zeroed();
    if dbdatecrack(process, record.as_mut_ptr(), datetime.as_mut_ptr()) != SUCCEED {
        return DblibCell::Bytes(Vec::new(), false);
    }
    let record = record.assume_init();
    DblibCell::Bytes(
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            record.dateyear,
            record.datemonth + 1,
            record.datedmonth,
            record.datehour,
            record.dateminute,
            record.datesecond
        )
        .into_bytes(),
        false,
    )
}

/// Converts an arbitrary DB-Library value through FreeTDS's own SQLCHAR converter.
unsafe fn converted_text(
    process: *mut DbProcess,
    native_type: c_int,
    data: *const c_uchar,
    len: c_int,
) -> DblibCell {
    let mut buffer = vec![0u8; (len.max(32) as usize).saturating_mul(2).saturating_add(64)];
    let converted = dbconvert(
        process,
        native_type,
        data,
        len,
        SYBCHAR,
        buffer.as_mut_ptr(),
        buffer.len() as c_int,
    );
    if converted <= 0 {
        DblibCell::Bytes(Vec::new(), false)
    } else {
        buffer.truncate(converted as usize);
        while buffer.last() == Some(&b' ') {
            buffer.pop();
        }
        DblibCell::Bytes(buffer, false)
    }
}

/// Bound value used by DBLIB's mandatory emulated-prepare path.
#[derive(Clone, Default)]
enum BindValue {
    #[default]
    Null,
    Int(i64),
    Float(f64),
    Text(Vec<u8>, bool),
    Blob(Vec<u8>),
}

/// Live DBLIB statement with bridge-owned bindings and materialized rowsets.
pub struct DblibStmt {
    pub conn_id: i64,
    translated_sql: String,
    named_map: HashMap<String, i64>,
    order: Vec<i64>,
    binds: Vec<BindValue>,
    bound: Vec<bool>,
    rowsets: Vec<DblibRowset>,
    rowset_index: usize,
    cursor: isize,
    executed: bool,
    pub sent_sql: String,
    error: ErrorState,
}

impl DblibStmt {
    /// Creates an emulated DBLIB statement and records PDO placeholder ordering.
    pub fn new(conn_id: i64, sql: &str) -> Result<Self, String> {
        let (translated_sql, named_map, order, mixed) =
            crate::my::translate_pdo_placeholders(sql);
        if mixed {
            return Err("Invalid parameter number: mixed named and positional parameters".to_string());
        }
        let slots = order.iter().copied().max().unwrap_or(0).max(0) as usize;
        Ok(Self {
            conn_id,
            translated_sql,
            named_map,
            order,
            binds: vec![BindValue::Null; slots],
            bound: vec![false; slots],
            rowsets: Vec::new(),
            rowset_index: 0,
            cursor: -1,
            executed: false,
            sent_sql: String::new(),
            error: ErrorState::default(),
        })
    }

    /// Resolves a named PDO placeholder to its one-based bind slot.
    pub fn parameter_index(&self, name: &str) -> i64 {
        self.named_map
            .get(name.trim_start_matches(':'))
            .copied()
            .unwrap_or(-1)
    }

    /// Stores an integer bind in a one-based slot.
    pub fn bind_int(&mut self, index: i64, value: i64) -> bool {
        self.set_bind(index, BindValue::Int(value))
    }

    /// Stores a floating-point bind in a one-based slot.
    pub fn bind_double(&mut self, index: i64, value: f64) -> bool {
        self.set_bind(index, BindValue::Float(value))
    }

    /// Stores a text bind, optionally using DBLIB's national-string `N` prefix.
    pub fn bind_text(&mut self, index: i64, value: Vec<u8>, national: bool) -> bool {
        self.set_bind(index, BindValue::Text(value, national))
    }

    /// Stores a binary bind rendered as a T-SQL hexadecimal literal.
    pub fn bind_blob(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.set_bind(index, BindValue::Blob(value))
    }

    /// Stores a SQL NULL bind in a one-based slot.
    pub fn bind_null(&mut self, index: i64) -> bool {
        self.set_bind(index, BindValue::Null)
    }

    /// Updates one bind slot and records that the caller explicitly supplied it.
    fn set_bind(&mut self, index: i64, value: BindValue) -> bool {
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

    /// Clears buffered execution state while retaining parameter bindings.
    pub fn reset(&mut self) {
        self.rowsets.clear();
        self.rowset_index = 0;
        self.cursor = -1;
        self.executed = false;
        self.sent_sql.clear();
        self.error = ErrorState::default();
    }

    /// Clears every parameter binding and buffered execution state.
    pub fn clear_bindings(&mut self) {
        self.reset();
        self.bound.fill(false);
        self.binds.fill(BindValue::Null);
    }

    /// Renders the emulated SQL and executes it through the owning connection.
    pub fn execute(&mut self, conn: &mut DblibConn) -> Result<(), String> {
        if self.bound.iter().any(|bound| !bound) {
            self.error.sqlstate = "HY093".to_string();
            self.error.message =
                "Invalid parameter number: number of bound variables does not match number of tokens"
                    .to_string();
            return Err(self.error.message.clone());
        }
        self.sent_sql = match interpolate(&self.translated_sql, &self.order, &self.binds) {
            Ok(sql) => sql,
            Err(message) => {
                self.error.sqlstate = "HY093".to_string();
                self.error.message = message.clone();
                return Err(message);
            }
        };
        match conn.execute(&self.sent_sql) {
            Ok(rowsets) => {
                self.rowsets = rowsets;
                self.rowset_index = 0;
                self.cursor = -1;
                self.executed = true;
                self.error = ErrorState::default();
                Ok(())
            }
            Err(message) => {
                self.error = conn.error.as_ref().clone();
                self.error.message = message.clone();
                Err(message)
            }
        }
    }

    /// Returns whether the statement has no materialized execution yet.
    pub fn needs_execute(&self) -> bool {
        !self.executed
    }

    /// Advances to the next row of the current materialized rowset.
    pub fn step(&mut self) -> i64 {
        let Some(rowset) = self.rowsets.get(self.rowset_index) else {
            return 0;
        };
        let next = self.cursor + 1;
        if next < rowset.rows.len() as isize {
            self.cursor = next;
            1
        } else {
            0
        }
    }

    /// Advances to the next result set, resetting its row cursor.
    pub fn next_rowset(&mut self) -> bool {
        if self.rowset_index + 1 >= self.rowsets.len() {
            return false;
        }
        self.rowset_index += 1;
        self.cursor = -1;
        true
    }

    /// Returns the row count reported for the active materialized rowset.
    pub fn current_row_count(&self) -> i64 {
        self.rowset().map_or(0, |rowset| rowset.row_count)
    }

    /// Returns the active result set, if execution produced one.
    fn rowset(&self) -> Option<&DblibRowset> {
        self.rowsets.get(self.rowset_index)
    }

    /// Returns the active row, if `step()` positioned the cursor on one.
    fn row(&self) -> Option<&[DblibCell]> {
        usize::try_from(self.cursor)
            .ok()
            .and_then(|index| self.rowset()?.rows.get(index))
            .map(Vec::as_slice)
    }

    /// Returns the current row's cell at `index`.
    pub fn cell(&self, index: usize) -> Option<&DblibCell> {
        self.row()?.get(index)
    }

    /// Returns the bridge's common PDO storage-class code for one current cell.
    pub fn column_type(&self, index: i64) -> i64 {
        let Ok(index) = usize::try_from(index) else {
            return 5;
        };
        match self.cell(index) {
            Some(DblibCell::Int(_)) => 1,
            Some(DblibCell::Float(_)) => 2,
            Some(DblibCell::Bytes(_, true)) => 4,
            Some(DblibCell::Bytes(_, false)) => 3,
            Some(DblibCell::Null) | None => 5,
        }
    }

    /// Returns one current cell as an integer.
    pub fn column_int(&self, index: i64) -> i64 {
        let Ok(index) = usize::try_from(index) else {
            return 0;
        };
        match self.cell(index) {
            Some(DblibCell::Int(value)) => *value,
            Some(DblibCell::Float(value)) => *value as i64,
            Some(DblibCell::Bytes(value, _)) => String::from_utf8_lossy(value).parse().unwrap_or(0),
            Some(DblibCell::Null) | None => 0,
        }
    }

    /// Returns one current cell as a floating-point value.
    pub fn column_double(&self, index: i64) -> f64 {
        let Ok(index) = usize::try_from(index) else {
            return 0.0;
        };
        match self.cell(index) {
            Some(DblibCell::Int(value)) => *value as f64,
            Some(DblibCell::Float(value)) => *value,
            Some(DblibCell::Bytes(value, _)) => String::from_utf8_lossy(value).parse().unwrap_or(0.0),
            Some(DblibCell::Null) | None => 0.0,
        }
    }

    /// Returns one current cell as its PDO byte payload.
    pub fn column_data(&self, index: i64) -> Vec<u8> {
        let Ok(index) = usize::try_from(index) else {
            return Vec::new();
        };
        match self.cell(index) {
            Some(DblibCell::Int(value)) => value.to_string().into_bytes(),
            Some(DblibCell::Float(value)) => value.to_string().into_bytes(),
            Some(DblibCell::Bytes(value, _)) => value.clone(),
            Some(DblibCell::Null) | None => Vec::new(),
        }
    }

    /// Returns one current result column's name.
    pub fn column_name(&self, index: i64) -> String {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.column(index))
            .map(|column| column.name.clone())
            .unwrap_or_default()
    }

    /// Returns one current result column's native FreeTDS type name.
    pub fn column_native_type(&self, index: i64) -> String {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.column(index))
            .map(|column| native_type_name(column.native_type).to_string())
            .unwrap_or_default()
    }

    /// Returns one current result column's declared maximum byte length.
    pub fn column_len(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.column(index))
            .map_or(-1, |column| column.max_len)
    }

    /// Returns one current result column's DB-Library precision.
    pub fn column_precision(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.column(index))
            .map_or(0, |column| column.precision)
    }

    /// Returns one current result column's DB-Library scale.
    pub fn column_scale(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.column(index))
            .map_or(0, |column| column.scale)
    }

    /// Returns one current result column's source expression/table label.
    pub fn column_source(&self, index: i64) -> String {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.column(index))
            .map(|column| column.source.clone())
            .unwrap_or_default()
    }

    /// Returns one current result column's DB-Library native type identifier.
    pub fn column_native_type_id(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.column(index))
            .map_or(0, |column| i64::from(column.native_type))
    }

    /// Returns one current result column's server user-type identifier.
    pub fn column_user_type_id(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.column(index))
            .map_or(0, |column| column.user_type)
    }

    /// Returns the current result set's column at `index`.
    pub fn column(&self, index: usize) -> Option<&DblibColumn> {
        self.rowset()?.columns.get(index)
    }

    /// Returns the current result set's column count.
    pub fn column_count(&self) -> i64 {
        self.rowset().map_or(0, |rowset| rowset.columns.len() as i64)
    }

    /// Returns the statement SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        if self.error.sqlstate.is_empty() {
            "00000"
        } else {
            &self.error.sqlstate
        }
    }

    /// Returns the statement native error code.
    pub fn errcode(&self) -> i64 {
        self.error.native_code
    }

    /// Returns the statement diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }

    /// Returns the statement's DB-Library operating-system error code.
    pub fn os_errcode(&self) -> i64 {
        self.error.os_code
    }

    /// Returns the statement's DB-Library error severity.
    pub fn severity(&self) -> i64 {
        self.error.severity
    }

    /// Returns the statement's DB-Library operating-system diagnostic text.
    pub fn os_errmsg(&self) -> &str {
        &self.error.os_message
    }
}

/// Interpolates one translated DBLIB statement with safely quoted T-SQL values.
fn interpolate(sql: &str, order: &[i64], binds: &[BindValue]) -> Result<String, String> {
    let mut output = String::with_capacity(sql.len() + binds.len() * 8);
    let mut marker = 0usize;
    let mut chars = sql.chars().peekable();
    let mut quote = None;
    while let Some(ch) = chars.next() {
        if let Some(active) = quote {
            output.push(ch);
            if ch == active {
                if chars.peek() == Some(&active) {
                    output.push(chars.next().unwrap_or(active));
                } else {
                    quote = None;
                }
            }
            continue;
        }
        if ch == '\'' || ch == '"' || ch == '[' {
            quote = Some(if ch == '[' { ']' } else { ch });
            output.push(ch);
            continue;
        }
        if ch != '?' {
            output.push(ch);
            continue;
        }
        let slot = order
            .get(marker)
            .copied()
            .and_then(|slot| usize::try_from(slot).ok())
            .and_then(|slot| slot.checked_sub(1))
            .ok_or_else(|| "Invalid parameter number".to_string())?;
        render_bind(&mut output, binds.get(slot).ok_or_else(|| "Invalid parameter number".to_string())?);
        marker += 1;
    }
    if marker != order.len() {
        return Err("Invalid parameter number".to_string());
    }
    Ok(output)
}

/// Appends one bound value as a T-SQL literal.
fn render_bind(output: &mut String, value: &BindValue) {
    match value {
        BindValue::Null => output.push_str("NULL"),
        BindValue::Int(value) => output.push_str(&value.to_string()),
        BindValue::Float(value) if value.is_finite() => output.push_str(&value.to_string()),
        BindValue::Float(_) => output.push_str("NULL"),
        BindValue::Text(bytes, national) => {
            if *national {
                output.push('N');
            }
            output.push('\'');
            for ch in String::from_utf8_lossy(bytes).chars() {
                if ch == '\'' {
                    output.push('\'');
                }
                output.push(ch);
            }
            output.push('\'');
        }
        BindValue::Blob(bytes) => {
            output.push_str("0x");
            for byte in bytes {
                use std::fmt::Write;
                let _ = write!(output, "{byte:02X}");
            }
        }
    }
}

/// Maps a FreeTDS native type ID to php-src's PDO_DBLIB metadata spelling.
pub fn native_type_name(native_type: c_int) -> &'static str {
    match native_type {
        31 => "nvarchar",
        34 => "image",
        35 => "text",
        36 => "uniqueidentifier",
        37 => "varbinary",
        38 | 127 => "bigint",
        39 | 167 => "varchar",
        40 => "date",
        41 => "time",
        42 => "datetime2",
        43 => "datetimeoffset",
        45 | 173 => "binary",
        47 | 175 => "char",
        48 => "tinyint",
        50 | 104 => "bit",
        52 => "smallint",
        55 | 106 => "decimal",
        56 => "int",
        58 => "smalldatetime",
        59 => "real",
        60 => "money",
        61 => "datetime",
        62 => "float",
        63 | 108 => "numeric",
        98 => "sql_variant",
        99 => "ntext",
        122 => "smallmoney",
        165 => "varbinary",
        189 => "timestamp",
        231 => "nvarchar",
        239 => "nchar",
        240 => "geometry",
        241 => "xml",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses the PDO_DBLIB DSN defaults and explicit client options.
    #[test]
    fn parses_dblib_dsn() {
        let options = parse_dsn(
            "dblib:host=db;port=1433;dbname=app;user=user%3Bname;password=p%25w;charset=UTF-8;version=7.4;timeout=4;stringify_uniqueidentifier=1;skip_empty_rowsets=1;datetime_convert=1",
        )
        .unwrap();
        assert_eq!(options.host, "db");
        assert_eq!(options.port, Some(1433));
        assert_eq!(options.dbname.as_deref(), Some("app"));
        assert_eq!(options.user.as_deref(), Some("user;name"));
        assert_eq!(options.password.as_deref(), Some("p%w"));
        assert_eq!(options.version.as_deref(), Some("7.4"));
        assert_eq!(options.connection_timeout, 4);
        assert_eq!(options.query_timeout, 4);
        assert!(options.stringify_uniqueidentifier);
        assert!(options.skip_empty_rowsets);
        assert!(options.datetime_convert);
        assert_eq!(tds_login_version("7.4"), Some(8));
        assert_eq!(tds_login_version("unsupported"), None);
        assert_eq!(server_name(&options), "db:1433");
    }

    /// Leaves FreeTDS aliases untouched when no explicit PDO port extension is present.
    #[test]
    fn preserves_dblib_server_alias_without_port() {
        let options = parse_dsn("dblib:host=production-alias").unwrap();
        assert_eq!(server_name(&options), "production-alias");
    }

    /// Interpolates reused named placeholders with SQL-safe DBLIB literals.
    #[test]
    fn interpolates_dblib_bindings() {
        let (sql, _, order, _) =
            crate::my::translate_pdo_placeholders("SELECT :name, :name, ?");
        let rendered = interpolate(
            &sql,
            &order,
            &[
                BindValue::Text(b"O'Brien".to_vec(), true),
                BindValue::Blob(vec![0, 255]),
            ],
        )
        .unwrap();
        assert_eq!(rendered, "SELECT N'O''Brien', N'O''Brien', 0x00FF");
    }

    /// Rejects mixed placeholder styles before DB-Library sees the statement.
    #[test]
    fn rejects_mixed_placeholder_styles() {
        let result = DblibStmt::new(1, "SELECT ? AS positional, :named AS named");
        assert!(matches!(result, Err(message) if message.contains("mixed named and positional")));
    }

    /// Mirrors php-src's native PDO_DBLIB type metadata names.
    #[test]
    fn maps_native_type_names() {
        assert_eq!(native_type_name(56), "int");
        assert_eq!(native_type_name(231), "nvarchar");
        assert_eq!(native_type_name(36), "uniqueidentifier");
    }

    /// Mirrors php-src's four DB-Library client-error SQLSTATE classifications.
    #[test]
    fn maps_dblib_client_error_sqlstates() {
        assert_eq!(sqlstate_for_db_error(20017), "01002");
        assert_eq!(sqlstate_for_db_error(20002), "01002");
        assert_eq!(sqlstate_for_db_error(20010), "HY001");
        assert_eq!(sqlstate_for_db_error(20014), "28000");
        assert_eq!(sqlstate_for_db_error(20018), "HY000");
    }

    /// Runs a direct FreeTDS round-trip when an explicit live DSN is provided.
    #[test]
    #[ignore]
    fn live_round_trip() {
        let dsn = std::env::var("ELEPHC_DBLIB_DSN")
            .expect("ELEPHC_DBLIB_DSN is required for the ignored live test");
        let mut connection = DblibConn::open(&dsn).unwrap_or_else(|error| {
            panic!("PDO_DBLIB connection failed: {error}")
        });
        let sets = connection
            .execute("SELECT CAST(7 AS INT) AS n, CAST('Ada' AS VARCHAR(10)) AS name")
            .unwrap_or_else(|error| panic!("PDO_DBLIB query failed: {error}"));
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].columns[0].name, "n");
        assert_eq!(sets[0].columns[0].native_type, SYBINT4);
        assert_eq!(sets[0].columns[0].max_len, 4);
        assert!(matches!(sets[0].rows[0][0], DblibCell::Int(7)));
        assert!(matches!(&sets[0].rows[0][1], DblibCell::Bytes(value, false) if value == b"Ada"));

        assert!(connection.set_attribute(1002, 1));
        let typed = connection
            .execute("SELECT CAST('00112233-4455-6677-8899-AABBCCDDEEFF' AS uniqueidentifier), CAST('2024-02-03T04:05:06' AS datetime2)")
            .unwrap_or_else(|error| panic!("PDO_DBLIB typed query failed: {error}"));
        assert!(matches!(&typed[0].rows[0][0], DblibCell::Bytes(value, false) if value == b"00112233-4455-6677-8899-AABBCCDDEEFF"));
        assert!(matches!(&typed[0].rows[0][1], DblibCell::Bytes(value, false) if value == b"2024-02-03 04:05:06"));
    }

    /// Opens the live DBLIB fixture through the public C ABI used by compiled PHP.
    #[test]
    #[ignore]
    fn live_c_abi_open() {
        let dsn = CString::new(
            std::env::var("ELEPHC_DBLIB_DSN")
                .expect("ELEPHC_DBLIB_DSN is required for the ignored live test"),
        )
        .unwrap();
        let empty = CString::new("").unwrap();
        let connection = unsafe {
            crate::elephc_pdo_open_persistent(
                dsn.as_ptr(),
                0,
                0,
                empty.as_ptr(),
                empty.as_ptr(),
                0,
                empty.as_ptr(),
                empty.as_ptr(),
            )
        };
        assert!(connection > 0, "C ABI DBLIB open returned {connection}");
        assert_eq!(
            unsafe { CStr::from_ptr(crate::elephc_pdo_driver_name(connection)) }
                .to_string_lossy(),
            "dblib"
        );
        crate::elephc_pdo_close(connection);
    }
}
