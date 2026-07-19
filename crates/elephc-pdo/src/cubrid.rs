//! Purpose:
//! CUBRID CCI backend matching the official external PDO_CUBRID extension.
//!
//! Called from:
//! - The PDO bridge root when built with the optional `cubrid` feature.
//!
//! Key details:
//! - Loads the same `libcascci` client used upstream at runtime, keeping builds SDK-independent.
//! - Owns every CCI connection/request handle and copies transient CCI result metadata immediately.
//! - Materializes result rows so PDO's forward and scroll fetch orientations share one safe path.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_long, c_uchar, c_ulong, c_void, CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::OnceLock;

use libloading::Library;

const CCI_ER_DBMS: i32 = -20_001;
const CCI_ER_NO_MORE_DATA: i32 = -20_005;
const CAS_ER_NOT_IMPLEMENTED: i32 = -10_100;
const CAS_ER_NO_MORE_RESULT_SET: i32 = -10_022;
const CCI_EXEC_QUERY_ALL: c_char = 0x02;
const CCI_TRAN_COMMIT: c_char = 1;
const CCI_TRAN_ROLLBACK: c_char = 2;
const CCI_CURSOR_CURRENT: i32 = 1;
const CCI_A_TYPE_STR: i32 = 1;
const CCI_A_TYPE_INT: i32 = 2;
const CCI_A_TYPE_DOUBLE: i32 = 4;
const CCI_A_TYPE_BIT: i32 = 5;
const CCI_A_TYPE_SET: i32 = 7;
const CCI_A_TYPE_BIGINT: i32 = 8;
const CCI_A_TYPE_BLOB: i32 = 9;
const CCI_A_TYPE_CLOB: i32 = 10;
const CCI_U_TYPE_NULL: i32 = 0;
const CCI_U_TYPE_STRING: i32 = 2;
const CCI_U_TYPE_INT: i32 = 8;
const CCI_U_TYPE_DOUBLE: i32 = 12;
const CCI_U_TYPE_BIGINT: i32 = 21;
const CCI_U_TYPE_BLOB: i32 = 23;
const CCI_U_TYPE_CLOB: i32 = 24;
const CCI_U_TYPE_BIT: i32 = 5;
const CCI_U_TYPE_VARBIT: i32 = 6;
const CCI_U_TYPE_SET: i32 = 16;
const CUBRID_STMT_INSERT: i32 = 20;
const CUBRID_STMT_SELECT: i32 = 21;
const CUBRID_STMT_UPDATE: i32 = 22;
const CUBRID_STMT_DELETE: i32 = 23;

static OPEN_ERROR_CODE: AtomicI64 = AtomicI64::new(0);

#[repr(C)]
#[derive(Clone)]
struct NativeError {
    err_code: c_int,
    err_msg: [c_char; 1024],
}

impl Default for NativeError {
    /// Creates an empty CCI error buffer for one native call.
    fn default() -> Self {
        Self {
            err_code: 0,
            err_msg: [0; 1024],
        }
    }
}

#[repr(C)]
struct NativeColumn {
    ext_type: c_uchar,
    is_non_null: c_char,
    scale: i16,
    precision: c_int,
    col_name: *mut c_char,
    real_attr: *mut c_char,
    class_name: *mut c_char,
    default_value: *mut c_char,
    is_auto_increment: c_char,
    is_unique_key: c_char,
    is_primary_key: c_char,
    is_foreign_key: c_char,
    is_reverse_index: c_char,
    is_reverse_unique: c_char,
    is_shared: c_char,
    charset: c_int,
}

#[repr(C)]
struct NativeBit {
    size: c_int,
    buffer: *mut c_char,
}

type InitFn = unsafe extern "C" fn();
type EndFn = unsafe extern "C" fn();
type VersionFn = unsafe extern "C" fn(*mut c_int, *mut c_int, *mut c_int) -> c_int;
type ConnectFn = unsafe extern "C" fn(*mut c_char, *mut c_char, *mut c_char, *mut NativeError) -> c_int;
type DisconnectFn = unsafe extern "C" fn(c_int, *mut NativeError) -> c_int;
type EndTranFn = unsafe extern "C" fn(c_int, c_char, *mut NativeError) -> c_int;
type PrepareFn = unsafe extern "C" fn(c_int, *const c_char, c_char, *mut NativeError) -> c_int;
type BindFn = unsafe extern "C" fn(c_int, c_int, c_int, *mut c_void, c_int, c_char) -> c_int;
type ExecuteFn = unsafe extern "C" fn(c_int, c_char, c_int, *mut NativeError) -> c_int;
type ResultInfoFn = unsafe extern "C" fn(c_int, *mut c_int, *mut c_int) -> *mut NativeColumn;
type CloseRequestFn = unsafe extern "C" fn(c_int) -> c_int;
type CursorFn = unsafe extern "C" fn(c_int, c_int, c_int, *mut NativeError) -> c_int;
type FetchFn = unsafe extern "C" fn(c_int, *mut NativeError) -> c_int;
type FetchBufferClearFn = unsafe extern "C" fn(c_int) -> c_int;
type GetDataFn = unsafe extern "C" fn(c_int, c_int, c_int, *mut c_void, *mut c_int) -> c_int;
type NextResultFn = unsafe extern "C" fn(c_int, *mut NativeError) -> c_int;
type SchemaFn = unsafe extern "C" fn(c_int, c_int, *mut c_char, *mut c_char, c_char, *mut NativeError) -> c_int;
type GetDbVersionFn = unsafe extern "C" fn(c_int, *mut c_char, c_int) -> c_int;
type GetAutocommitFn = unsafe extern "C" fn(c_int) -> c_int;
type SetAutocommitFn = unsafe extern "C" fn(c_int, c_int) -> c_int;
type GetDbParameterFn = unsafe extern "C" fn(c_int, c_int, *mut c_void, *mut NativeError) -> c_int;
type SetIsolationLevelFn = unsafe extern "C" fn(c_int, c_int, *mut NativeError) -> c_int;
type SetLockTimeoutFn = unsafe extern "C" fn(c_int, c_int, *mut NativeError) -> c_int;
type SetQueryTimeoutFn = unsafe extern "C" fn(c_int, c_int) -> c_int;
type EscapeStringFn = unsafe extern "C" fn(
    c_int,
    *mut c_char,
    *const c_char,
    c_ulong,
    *mut NativeError,
) -> c_long;
type LastInsertIdFn = unsafe extern "C" fn(c_int, *mut c_void, *mut NativeError) -> c_int;
type BlobNewFn = unsafe extern "C" fn(c_int, *mut *mut c_void, *mut NativeError) -> c_int;
type BlobSizeFn = unsafe extern "C" fn(*mut c_void) -> i64;
type BlobWriteFn = unsafe extern "C" fn(c_int, *mut c_void, i64, c_int, *const c_char, *mut NativeError) -> c_int;
type BlobReadFn = unsafe extern "C" fn(c_int, *mut c_void, i64, c_int, *mut c_char, *mut NativeError) -> c_int;
type BlobFreeFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type SetMakeFn = unsafe extern "C" fn(*mut *mut c_void, c_int, c_int, *mut c_void, *mut c_int) -> c_int;
type SetFreeFn = unsafe extern "C" fn(*mut c_void);

struct CciApi {
    _library: Library,
    init: InitFn,
    end: EndFn,
    version: VersionFn,
    connect: ConnectFn,
    disconnect: DisconnectFn,
    end_tran: EndTranFn,
    prepare: PrepareFn,
    bind: BindFn,
    execute: ExecuteFn,
    result_info: ResultInfoFn,
    close_request: CloseRequestFn,
    cursor: CursorFn,
    fetch: FetchFn,
    fetch_buffer_clear: FetchBufferClearFn,
    get_data: GetDataFn,
    next_result: NextResultFn,
    schema: SchemaFn,
    get_db_version: GetDbVersionFn,
    get_autocommit: GetAutocommitFn,
    set_autocommit: SetAutocommitFn,
    get_db_parameter: GetDbParameterFn,
    set_isolation_level: SetIsolationLevelFn,
    set_lock_timeout: SetLockTimeoutFn,
    set_query_timeout: SetQueryTimeoutFn,
    escape_string: EscapeStringFn,
    last_insert_id: LastInsertIdFn,
    blob_new: BlobNewFn,
    blob_size: BlobSizeFn,
    blob_write: BlobWriteFn,
    blob_read: BlobReadFn,
    blob_free: BlobFreeFn,
    clob_new: BlobNewFn,
    clob_size: BlobSizeFn,
    clob_write: BlobWriteFn,
    clob_read: BlobReadFn,
    clob_free: BlobFreeFn,
    set_make: SetMakeFn,
    set_free: SetFreeFn,
}

/// Copies one function pointer out of a loaded CCI library.
unsafe fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, String> {
    library
        .get::<T>(name)
        .map(|symbol| *symbol)
        .map_err(|error| format!("missing {}: {error}", String::from_utf8_lossy(name)))
}

impl CciApi {
    /// Loads CUBRID CCI from an explicit override or the platform's conventional names.
    fn load() -> Result<Self, String> {
        let mut candidates = Vec::new();
        if let Some(path) = std::env::var_os("CUBRID_CCI_LIBRARY") {
            candidates.push(path);
        }
        #[cfg(target_os = "macos")]
        candidates.extend(["libcascci.dylib".into(), "libcascci.so".into()]);
        #[cfg(target_os = "linux")]
        candidates.extend(["libcascci.so".into(), "libcascci.so.11".into()]);
        #[cfg(target_os = "windows")]
        candidates.push("cascci.dll".into());
        let mut failures = Vec::new();
        for candidate in candidates {
            let library = match unsafe { Library::new(&candidate) } {
                Ok(library) => library,
                Err(error) => {
                    failures.push(format!("{}: {error}", candidate.to_string_lossy()));
                    continue;
                }
            };
            let loaded = unsafe {
                Ok::<_, String>(Self {
                    init: symbol(&library, b"cci_init\0")?,
                    end: symbol(&library, b"cci_end\0")?,
                    version: symbol(&library, b"cci_get_version\0")?,
                    connect: symbol(&library, b"cci_connect_with_url_ex\0")?,
                    disconnect: symbol(&library, b"cci_disconnect\0")?,
                    end_tran: symbol(&library, b"cci_end_tran\0")?,
                    prepare: symbol(&library, b"cci_prepare\0")?,
                    bind: symbol(&library, b"cci_bind_param\0")?,
                    execute: symbol(&library, b"cci_execute\0")?,
                    result_info: symbol(&library, b"cci_get_result_info\0")?,
                    close_request: symbol(&library, b"cci_close_req_handle\0")?,
                    cursor: symbol(&library, b"cci_cursor\0")?,
                    fetch: symbol(&library, b"cci_fetch\0")?,
                    fetch_buffer_clear: symbol(&library, b"cci_fetch_buffer_clear\0")?,
                    get_data: symbol(&library, b"cci_get_data\0")?,
                    next_result: symbol(&library, b"cci_next_result\0")?,
                    schema: symbol(&library, b"cci_schema_info\0")?,
                    get_db_version: symbol(&library, b"cci_get_db_version\0")?,
                    get_autocommit: symbol(&library, b"cci_get_autocommit\0")?,
                    set_autocommit: symbol(&library, b"cci_set_autocommit\0")?,
                    get_db_parameter: symbol(&library, b"cci_get_db_parameter\0")?,
                    set_isolation_level: symbol(&library, b"cci_set_isolation_level\0")?,
                    set_lock_timeout: symbol(&library, b"cci_set_lock_timeout\0")?,
                    set_query_timeout: symbol(&library, b"cci_set_query_timeout\0")?,
                    escape_string: symbol(&library, b"cci_escape_string\0")?,
                    last_insert_id: symbol(&library, b"cci_get_last_insert_id\0")?,
                    blob_new: symbol(&library, b"cci_blob_new\0")?,
                    blob_size: symbol(&library, b"cci_blob_size\0")?,
                    blob_write: symbol(&library, b"cci_blob_write\0")?,
                    blob_read: symbol(&library, b"cci_blob_read\0")?,
                    blob_free: symbol(&library, b"cci_blob_free\0")?,
                    clob_new: symbol(&library, b"cci_clob_new\0")?,
                    clob_size: symbol(&library, b"cci_clob_size\0")?,
                    clob_write: symbol(&library, b"cci_clob_write\0")?,
                    clob_read: symbol(&library, b"cci_clob_read\0")?,
                    clob_free: symbol(&library, b"cci_clob_free\0")?,
                    set_make: symbol(&library, b"cci_set_make\0")?,
                    set_free: symbol(&library, b"cci_set_free\0")?,
                    _library: library,
                })
            };
            match loaded {
                Ok(api) => {
                    unsafe { (api.init)() };
                    return Ok(api);
                }
                Err(error) => failures.push(error),
            }
        }
        Err(format!(
            "CUBRID CCI client library was not found ({})",
            failures.join("; ")
        ))
    }
}

impl Drop for CciApi {
    /// Shuts CCI down before unloading its shared library at process termination.
    fn drop(&mut self) {
        unsafe { (self.end)() };
    }
}

/// Returns the process-wide dynamically loaded CCI API.
fn api() -> Result<&'static CciApi, String> {
    static API: OnceLock<Result<CciApi, String>> = OnceLock::new();
    API.get_or_init(CciApi::load).as_ref().map_err(Clone::clone)
}

/// Copies a nullable C string owned by CCI into Rust storage.
fn copy_c_string(pointer: *const c_char) -> String {
    if pointer.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
    }
}

/// Converts arbitrary text to a C-compatible string without panicking on NUL bytes.
fn c_string(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "CUBRID value contains a NUL byte".to_string())
}

/// Percent-decodes the credential encoding used by the PDO prelude.
fn decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let pair = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(decoded) = pair.and_then(|pair| u8::from_str_radix(pair, 16).ok()) {
                output.push(decoded);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

struct Dsn {
    url: String,
    user: String,
    password: String,
}

/// Converts PDO_CUBRID's semicolon DSN into the official CCI connection URL.
fn parse_dsn(dsn: &str) -> Result<Dsn, String> {
    let body = dsn
        .strip_prefix("cubrid:")
        .ok_or_else(|| "Invalid CUBRID data source name".to_string())?;
    let mut values = HashMap::new();
    let mut extras = Vec::new();
    for part in body.split(';').filter(|part| !part.is_empty()) {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| "Invalid CUBRID connection string".to_string())?;
        let key_lower = key.trim().to_ascii_lowercase();
        let value = if matches!(key_lower.as_str(), "user" | "password") {
            decode_component(value.trim())
        } else {
            value.trim().to_string()
        };
        if matches!(key_lower.as_str(), "host" | "port" | "dbname" | "user" | "password") {
            values.insert(key_lower, value);
        } else {
            extras.push((key.to_string(), value));
        }
    }
    let host = values.remove("host").unwrap_or_else(|| "localhost".to_string());
    let port = values.remove("port").unwrap_or_else(|| "55300".to_string());
    let dbname = values.remove("dbname").unwrap_or_else(|| "demodb".to_string());
    let user = values.remove("user").unwrap_or_else(|| "public".to_string());
    let password = values.remove("password").unwrap_or_default();
    let mut url = format!("cci:CUBRID:{host}:{port}:{dbname}:{user}:{password}:");
    for (index, (key, value)) in extras.into_iter().enumerate() {
        url.push(if index == 0 { '?' } else { '&' });
        url.push_str(&key);
        url.push('=');
        url.push_str(&value);
    }
    Ok(Dsn { url, user, password })
}

#[derive(Clone)]
struct ErrorState {
    sqlstate: String,
    code: i64,
    message: String,
}

impl Default for ErrorState {
    /// Creates PDO's successful no-error state.
    fn default() -> Self {
        Self {
            sqlstate: "00000".to_string(),
            code: 0,
            message: String::new(),
        }
    }
}

/// Maps one CCI result/error buffer to PDO_CUBRID's HY000 diagnostic shape.
fn native_error(result: i32, native: &NativeError) -> ErrorState {
    let (code, message) = if result == CCI_ER_DBMS {
        (native.err_code as i64, format!("DBMS, {}", copy_c_string(native.err_msg.as_ptr())))
    } else {
        let message = copy_c_string(native.err_msg.as_ptr());
        let facility = if result > -10_200 {
            "CAS"
        } else if result > -20_100 {
            "CCI"
        } else if result > -31_000 {
            "CLIENT"
        } else {
            "UNKNOWN"
        };
        let message = if message.is_empty() { format!("{facility}, CCI error {result}") } else { format!("{facility}, {message}") };
        (result as i64, message)
    };
    ErrorState {
        sqlstate: "HY000".to_string(),
        code,
        message,
    }
}

/// Returns the SQLSTATE and native code captured by the latest failed CUBRID open.
pub fn open_diagnostic() -> (&'static str, i64) {
    ("HY000", OPEN_ERROR_CODE.load(Ordering::Relaxed))
}

/// Records a constructor failure for PDOException and returns its display text.
fn record_open_error(error: &ErrorState) -> String {
    OPEN_ERROR_CODE.store(error.code, Ordering::Relaxed);
    error.message.clone()
}

/// Decodes CCI's `TCCT TTTT` extended-type representation into its scalar domain.
fn collection_domain(ext_type: u8) -> u8 {
    ((ext_type & 0x80) >> 2) | (ext_type & 0x1f)
}

/// Resolves PDO_CUBRID's case-insensitive driver-option type names to CCI domains.
fn named_type(name: &str) -> Option<i32> {
    match name.to_ascii_uppercase().as_str() {
        "NULL" => Some(0),
        "CHAR" => Some(1),
        "STRING" => Some(2),
        "NCHAR" => Some(3),
        "VARNCHAR" => Some(4),
        "BIT" => Some(5),
        "VARBIT" => Some(6),
        "NUMERIC" | "NUMBER" => Some(7),
        "INT" => Some(8),
        "SHORT" => Some(9),
        "MONETARY" => Some(10),
        "FLOAT" => Some(11),
        "DOUBLE" => Some(12),
        "DATE" => Some(13),
        "TIME" => Some(14),
        "TIMESTAMP" => Some(15),
        "SET" => Some(16),
        "MULTISET" => Some(17),
        "SEQUENCE" => Some(18),
        "OBJECT" => Some(19),
        "RESULTSET" => Some(20),
        "BIGINT" => Some(21),
        "DATETIME" => Some(22),
        "BLOB" => Some(23),
        "CLOB" => Some(24),
        "ENUM" => Some(25),
        _ => None,
    }
}

/// Decodes the byte-length framing emitted by the PHP prelude for one CUBRID set.
fn decode_set(input: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut cursor = input.iter().position(|byte| *byte == b':')?;
    let count = std::str::from_utf8(&input[..cursor]).ok()?.parse::<usize>().ok()?;
    cursor += 1;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let separator = input[cursor..].iter().position(|byte| *byte == b':')? + cursor;
        let length = std::str::from_utf8(&input[cursor..separator]).ok()?.parse::<usize>().ok()?;
        cursor = separator + 1;
        let end = cursor.checked_add(length)?;
        values.push(input.get(cursor..end)?.to_vec());
        cursor = end;
    }
    (cursor == input.len()).then_some(values)
}

/// Returns the CUBRID native type spelling used by PDO_CUBRID metadata.
fn native_type_name(ext_type: u8, precision: i32, scale: i16) -> String {
    let domain = collection_domain(ext_type);
    let base = match domain {
        0 => "unknown".to_string(),
        1 => format!("char({precision})"),
        2 => format!("varchar({precision})"),
        3 => format!("nchar({precision})"),
        4 => format!("varnchar({precision})"),
        5 => "bit".to_string(),
        6 => format!("varbit({precision})"),
        7 => format!("numeric({precision},{scale})"),
        8 => "integer".to_string(),
        9 => "smallint".to_string(),
        10 => "monetary".to_string(),
        11 => "float".to_string(),
        12 => "double".to_string(),
        13 => "date".to_string(),
        14 => "time".to_string(),
        15 => "timestamp".to_string(),
        16 => "set".to_string(),
        17 => "multiset".to_string(),
        18 => "sequence".to_string(),
        19 => "object".to_string(),
        20 => "[unknown]".to_string(),
        21 => "bigint".to_string(),
        22 => "datetime".to_string(),
        23 => "blob".to_string(),
        24 => "clob".to_string(),
        25 => "enum".to_string(),
        _ => "[unknown]".to_string(),
    };
    match ext_type & 0x60 {
        0x20 => format!("set({base})"),
        0x40 => format!("multiset({base})"),
        0x60 => format!("sequence({base})"),
        _ => base,
    }
}

/// Owns a live CUBRID CCI connection and its PDO-visible state.
pub struct CubridConn {
    handle: i32,
    auto_commit: bool,
    configured_autocommit: bool,
    query_timeout: i64,
    pub in_transaction: bool,
    pub changes: i64,
    error: ErrorState,
}

unsafe impl Send for CubridConn {}

impl Drop for CubridConn {
    /// Disconnects the native CCI session.
    fn drop(&mut self) {
        if let Ok(api) = api() {
            let mut error = NativeError::default();
            unsafe { (api.disconnect)(self.handle, &mut error) };
        }
    }
}

impl CubridConn {
    /// Opens a PDO_CUBRID DSN through the process CCI client.
    pub fn open(dsn: &str) -> Result<Self, String> {
        OPEN_ERROR_CODE.store(0, Ordering::Relaxed);
        let api = api()?;
        let dsn = parse_dsn(dsn).map_err(|message| {
            OPEN_ERROR_CODE.store(-30_019, Ordering::Relaxed);
            message
        })?;
        let url = c_string(&dsn.url)?;
        let user = c_string(&dsn.user)?;
        let password = c_string(&dsn.password)?;
        let mut error = NativeError::default();
        let handle = unsafe {
            (api.connect)(
                url.as_ptr().cast_mut(),
                user.as_ptr().cast_mut(),
                password.as_ptr().cast_mut(),
                &mut error,
            )
        };
        if handle < 0 {
            return Err(record_open_error(&native_error(handle, &error)));
        }
        let auto_commit = unsafe { (api.get_autocommit)(handle) };
        if auto_commit < 0 {
            let mut disconnect_error = NativeError::default();
            unsafe { (api.disconnect)(handle, &mut disconnect_error) };
            OPEN_ERROR_CODE.store(auto_commit as i64, Ordering::Relaxed);
            return Err(format!("CCI, CCI error {auto_commit}"));
        }
        let mut connection = Self {
            handle,
            auto_commit: auto_commit != 0,
            configured_autocommit: auto_commit != 0,
            query_timeout: -1,
            in_transaction: false,
            changes: 0,
            error: ErrorState::default(),
        };
        for parameter in [1, 2] {
            let mut value = 0i32;
            let mut error = NativeError::default();
            let result = unsafe {
                (api.get_db_parameter)(
                    handle,
                    parameter,
                    (&mut value as *mut i32).cast(),
                    &mut error,
                )
            };
            if result < 0 && result != CAS_ER_NOT_IMPLEMENTED {
                connection.error = native_error(result, &error);
                return Err(record_open_error(&connection.error));
            }
        }
        if !connection.commit_initial_transaction() {
            return Err(record_open_error(&connection.error));
        }
        Ok(connection)
    }

    /// Commits CCI's initial connection transaction like the official extension factory.
    fn commit_initial_transaction(&mut self) -> bool {
        let mut error = NativeError::default();
        let result = unsafe { (api().expect("CCI loaded").end_tran)(self.handle, CCI_TRAN_COMMIT, &mut error) };
        if result < 0 {
            self.error = native_error(result, &error);
            false
        } else {
            true
        }
    }

    /// Executes one SQL statement and records the affected-row count.
    pub fn exec(&mut self, sql: &str) -> Result<i64, String> {
        let mut statement = CubridStmt::new(self, 0, sql)?;
        statement.execute(self)?;
        self.changes = statement.row_count;
        Ok(self.changes)
    }

    /// Probes the connection with the same `db_root` scalar query used upstream.
    pub fn is_alive(&mut self) -> bool {
        self.exec("select 1+1 from db_root").is_ok()
    }

    /// Starts a transaction while retaining the configured post-transaction autocommit mode.
    pub fn begin(&mut self) -> bool {
        if self.in_transaction {
            return false;
        }
        if self.configured_autocommit {
            if !self.set_native_autocommit(false) {
                return false;
            }
            self.auto_commit = false;
        } else if !self.end_native_transaction(CCI_TRAN_COMMIT) {
            return false;
        }
        self.in_transaction = true;
        true
    }

    /// Commits the current CCI transaction.
    pub fn commit(&mut self) -> bool {
        self.end_transaction(CCI_TRAN_COMMIT)
    }

    /// Rolls back the current CCI transaction.
    pub fn rollback(&mut self) -> bool {
        self.end_transaction(CCI_TRAN_ROLLBACK)
    }

    /// Ends a native transaction and restores configured autocommit.
    fn end_transaction(&mut self, kind: c_char) -> bool {
        if !self.end_native_transaction(kind) {
            return false;
        }
        self.in_transaction = false;
        if self.configured_autocommit {
            if !self.set_native_autocommit(true) {
                return false;
            }
            self.auto_commit = true;
        }
        true
    }

    /// Ends one native CCI transaction without changing PDO's configured state.
    fn end_native_transaction(&mut self, kind: c_char) -> bool {
        let mut error = NativeError::default();
        let result = unsafe { (api().expect("CCI loaded").end_tran)(self.handle, kind, &mut error) };
        if result < 0 {
            self.error = native_error(result, &error);
            return false;
        }
        true
    }

    /// Changes native CCI autocommit and records a diagnostic on failure.
    fn set_native_autocommit(&mut self, enabled: bool) -> bool {
        let result = unsafe { (api().expect("CCI loaded").set_autocommit)(self.handle, enabled as i32) };
        if result < 0 {
            self.error = ErrorState {
                sqlstate: "HY000".to_string(),
                code: result as i64,
                message: format!("CCI, CCI error {result}"),
            };
            false
        } else {
            true
        }
    }

    /// Changes PDO_CUBRID's live autocommit setting outside a transaction.
    pub fn set_autocommit(&mut self, enabled: bool) -> bool {
        if self.auto_commit == enabled {
            return true;
        }
        if !self.auto_commit && !self.end_native_transaction(CCI_TRAN_COMMIT) {
            return false;
        }
        if !self.set_native_autocommit(enabled) {
            return false;
        }
        self.configured_autocommit = enabled;
        self.auto_commit = enabled;
        self.in_transaction = false;
        true
    }

    /// Writes isolation-level or lock-timeout CCI database parameters.
    pub fn set_attribute(&mut self, attribute: i64, value: i64) -> bool {
        match attribute {
            0 => return self.set_autocommit(value != 0),
            2 => {
                if value == 0 || (value < 0 && value != -1) {
                    return false;
                }
                self.query_timeout = value;
                return true;
            }
            1000 | 1001 => {}
            _ => return false,
        }
        let mut error = NativeError::default();
        let result = unsafe {
            if attribute == 1000 {
                (api().expect("CCI loaded").set_isolation_level)(self.handle, value as i32, &mut error)
            } else {
                (api().expect("CCI loaded").set_lock_timeout)(self.handle, value as i32, &mut error)
            }
        };
        if result < 0 {
            self.error = native_error(result, &error);
            false
        } else {
            true
        }
    }

    /// Reads autocommit and PDO_CUBRID's three connection attributes.
    pub fn attribute(&mut self, attribute: i64) -> Option<i64> {
        if attribute == 0 {
            return Some(self.auto_commit as i64);
        }
        if attribute == 2 {
            return Some(self.query_timeout);
        }
        let parameter = match attribute {
            1000 => 1,
            1001 => 2,
            1002 => 3,
            _ => return None,
        };
        let mut value = 0i32;
        let mut error = NativeError::default();
        let result = unsafe {
            (api().expect("CCI loaded").get_db_parameter)(
                self.handle,
                parameter,
                (&mut value as *mut i32).cast(),
                &mut error,
            )
        };
        if result < 0 {
            if attribute == 1002 {
                return Some(0);
            }
            self.error = native_error(result, &error);
            None
        } else {
            Some(value as i64)
        }
    }

    /// Returns CCI's client version.
    pub fn client_version(&self) -> String {
        let mut major = 0;
        let mut minor = 0;
        let mut patch = 0;
        unsafe { (api().expect("CCI loaded").version)(&mut major, &mut minor, &mut patch) };
        format!("{major}.{minor}.{patch}")
    }

    /// Returns the connected CUBRID server version.
    pub fn server_version(&mut self) -> String {
        let mut buffer: [c_char; 128] = [0; 128];
        let result = unsafe {
            (api().expect("CCI loaded").get_db_version)(
                self.handle,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            )
        };
        if result < 0 {
            String::new()
        } else {
            copy_c_string(buffer.as_ptr())
        }
    }

    /// Returns PDO_CUBRID's textual last inserted identifier.
    pub fn last_insert_id(&mut self) -> String {
        let mut pointer: *mut c_char = ptr::null_mut();
        let mut error = NativeError::default();
        let result = unsafe {
            (api().expect("CCI loaded").last_insert_id)(
                self.handle,
                (&mut pointer as *mut *mut c_char).cast(),
                &mut error,
            )
        };
        if result < 0 {
            self.error = native_error(result, &error);
            String::new()
        } else {
            copy_c_string(pointer)
        }
    }

    /// Escapes exact bytes through CCI's connection-aware PDO_CUBRID quoter.
    pub fn quote(&mut self, input: &[u8]) -> Result<Vec<u8>, String> {
        let mut output = vec![0u8; input.len().saturating_mul(2).saturating_add(18)];
        let mut error = NativeError::default();
        let result = unsafe {
            (api()?.escape_string)(
                self.handle,
                output.as_mut_ptr().cast(),
                input.as_ptr().cast(),
                input.len() as c_ulong,
                &mut error,
            )
        };
        if result < 0 {
            self.error = native_error(result as i32, &error);
            return Err(self.error.message.clone());
        }
        output.truncate(result as usize);
        Ok(output)
    }

    /// Returns the current connection SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        &self.error.sqlstate
    }

    /// Returns the current connection native code.
    pub fn errcode(&self) -> i64 {
        self.error.code
    }

    /// Returns the current connection diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }
}

#[derive(Clone)]
enum BindValue {
    Null,
    Int(i32),
    BigInt(i64),
    Double(f64),
    Text(Vec<u8>),
    Lob(Vec<u8>, i32),
    TypedText(Vec<u8>, i32),
    Bit(Vec<u8>, i32),
    Set(Vec<Vec<u8>>, i32),
}

#[derive(Clone)]
struct Column {
    name: String,
    table: String,
    default_value: String,
    native_type: String,
    ext_type: u8,
    precision: i64,
    scale: i64,
    flags: i64,
}

/// Owns one prepared CCI request and its materialized result sets.
pub struct CubridStmt {
    pub conn_id: i64,
    request: i32,
    named_map: HashMap<String, i64>,
    order: Vec<i64>,
    binds: Vec<BindValue>,
    bound: Vec<bool>,
    columns: Vec<Column>,
    rows: Vec<Vec<Option<Vec<u8>>>>,
    cursor: isize,
    executed: bool,
    row_count: i64,
    error: ErrorState,
}

unsafe impl Send for CubridStmt {}

impl Drop for CubridStmt {
    /// Closes the native CCI request handle.
    fn drop(&mut self) {
        if self.request > 0 {
            if let Ok(api) = api() {
                unsafe { (api.close_request)(self.request) };
            }
        }
    }
}

impl CubridStmt {
    /// Prepares SQL with PDO named-placeholder normalization.
    pub fn new(connection: &mut CubridConn, conn_id: i64, sql: &str) -> Result<Self, String> {
        let (translated, named_map, order, mixed) = crate::my::translate_placeholders(sql, false);
        if mixed {
            return Err("Invalid parameter number: mixed named and positional parameters".to_string());
        }
        let sql = c_string(&translated)?;
        let mut error = NativeError::default();
        let request = unsafe {
            (api()?.prepare)(connection.handle, sql.as_ptr(), 0, &mut error)
        };
        if request < 0 {
            connection.error = native_error(request, &error);
            return Err(connection.error.message.clone());
        }
        if connection.query_timeout != -1 && connection.query_timeout != 0 {
            let timeout_ms = connection.query_timeout.saturating_mul(1000);
            let timeout_ms = i32::try_from(timeout_ms).unwrap_or(i32::MAX);
            let result = unsafe { (api()?.set_query_timeout)(request, timeout_ms) };
            if result < 0 {
                unsafe { (api()?.close_request)(request) };
                connection.error = ErrorState {
                    sqlstate: "HY000".to_string(),
                    code: result as i64,
                    message: format!("CCI, CCI query-timeout error {result}"),
                };
                return Err(connection.error.message.clone());
            }
        }
        let slots = order.iter().copied().max().unwrap_or(0).max(0) as usize;
        Ok(Self {
            conn_id,
            request,
            named_map,
            order,
            binds: vec![BindValue::Null; slots],
            bound: vec![false; slots],
            columns: Vec::new(),
            rows: Vec::new(),
            cursor: -1,
            executed: false,
            row_count: 0,
            error: ErrorState::default(),
        })
    }

    /// Wraps a CCI schema-information request as an already executed statement.
    pub fn schema(
        connection: &mut CubridConn,
        conn_id: i64,
        schema_type: i64,
        class_name: &str,
        attribute_name: &str,
    ) -> Result<Self, String> {
        let class_name = (!class_name.is_empty())
            .then(|| c_string(class_name))
            .transpose()?;
        let attribute_name = (!attribute_name.is_empty())
            .then(|| c_string(attribute_name))
            .transpose()?;
        let flag = match schema_type {
            1 | 2 => 1,
            4 | 5 | 20 => 2,
            _ => 0,
        };
        let mut error = NativeError::default();
        let request = unsafe {
            (api()?.schema)(
                connection.handle,
                schema_type as i32,
                class_name
                    .as_ref()
                    .map_or(ptr::null_mut(), |value| value.as_ptr().cast_mut()),
                attribute_name
                    .as_ref()
                    .map_or(ptr::null_mut(), |value| value.as_ptr().cast_mut()),
                flag,
                &mut error,
            )
        };
        if request < 0 {
            connection.error = native_error(request, &error);
            return Err(connection.error.message.clone());
        }
        let mut statement = Self {
            conn_id,
            request,
            named_map: HashMap::new(),
            order: Vec::new(),
            binds: Vec::new(),
            bound: Vec::new(),
            columns: Vec::new(),
            rows: Vec::new(),
            cursor: -1,
            executed: true,
            row_count: 0,
            error: ErrorState::default(),
        };
        statement.materialize(connection)?;
        Ok(statement)
    }

    /// Resolves one named placeholder to its 1-based bind slot.
    pub fn parameter_index(&self, name: &str) -> i64 {
        self.named_map
            .get(name.trim_start_matches(':'))
            .copied()
            .unwrap_or(0)
    }

    /// Stores one bind value after validating its 1-based slot.
    fn bind(&mut self, index: i64, value: BindValue) -> bool {
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

    /// Binds an integer using CCI's native integer widths.
    pub fn bind_int(&mut self, index: i64, value: i64) -> bool {
        if let Ok(value) = i32::try_from(value) {
            self.bind(index, BindValue::Int(value))
        } else {
            self.bind(index, BindValue::BigInt(value))
        }
    }

    /// Binds a floating-point value.
    pub fn bind_double(&mut self, index: i64, value: f64) -> bool {
        self.bind(index, BindValue::Double(value))
    }

    /// Binds exact text bytes.
    pub fn bind_text(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.bind(index, BindValue::Text(value))
    }

    /// Binds exact BLOB bytes through CCI's LOB API.
    pub fn bind_blob(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.bind(index, BindValue::Lob(value, CCI_U_TYPE_BLOB))
    }

    /// Stores a PDO_CUBRID driver-option scalar or collection bind.
    pub fn bind_typed(
        &mut self,
        index: i64,
        value: Vec<u8>,
        type_name: &str,
        is_set: bool,
        pdo_type: i64,
    ) -> bool {
        let mut domain = if type_name.is_empty() {
            CCI_U_TYPE_STRING
        } else if let Some(domain) = named_type(type_name) {
            domain
        } else {
            self.error = ErrorState {
                sqlstate: "HY000".to_string(),
                code: -30_008,
                message: "CLIENT, Not supported type".to_string(),
            };
            return false;
        };
        if domain == 25 && !is_set {
            domain = match pdo_type {
                0 => CCI_U_TYPE_NULL,
                1 => CCI_U_TYPE_INT,
                3 => CCI_U_TYPE_BLOB,
                _ => CCI_U_TYPE_STRING,
            };
        }
        let binding = if is_set {
            let Some(values) = decode_set(&value) else {
                return false;
            };
            let element_type = if domain == CCI_U_TYPE_BIT || domain == CCI_U_TYPE_VARBIT {
                domain
            } else {
                CCI_U_TYPE_STRING
            };
            BindValue::Set(values, element_type)
        } else {
            match domain {
                CCI_U_TYPE_NULL => BindValue::Null,
                CCI_U_TYPE_BLOB | CCI_U_TYPE_CLOB => BindValue::Lob(value, domain),
                CCI_U_TYPE_BIT | CCI_U_TYPE_VARBIT => BindValue::Bit(value, domain),
                _ => BindValue::TypedText(value, domain),
            }
        };
        self.bind(index, binding)
    }

    /// Binds SQL NULL.
    pub fn bind_null(&mut self, index: i64) -> bool {
        self.bind(index, BindValue::Null)
    }

    /// Clears cursor/result state while retaining native preparation and binds.
    pub fn reset(&mut self) {
        if self.executed {
            if let Ok(api) = api() {
                unsafe { (api.fetch_buffer_clear)(self.request) };
            }
        }
        self.columns.clear();
        self.rows.clear();
        self.cursor = -1;
        self.executed = false;
        self.row_count = 0;
    }

    /// Clears all values as PDO does before `execute(array)` replaces them.
    pub fn clear_bindings(&mut self) {
        self.reset();
        self.binds.fill(BindValue::Null);
        self.bound.fill(false);
    }

    /// Reports whether this request has not yet run since reset.
    pub fn needs_execute(&self) -> bool {
        !self.executed
    }

    /// Binds every occurrence, executes CCI, and materializes the active result.
    pub fn execute(&mut self, connection: &mut CubridConn) -> Result<(), String> {
        let cleared = unsafe { (api()?.fetch_buffer_clear)(self.request) };
        if cleared < 0 {
            self.error = ErrorState {
                sqlstate: "HY000".to_string(),
                code: cleared as i64,
                message: format!("CCI, CCI fetch-buffer error {cleared}"),
            };
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        if self.bound.iter().any(|bound| !bound) {
            self.error = ErrorState {
                sqlstate: "HY000".to_string(),
                code: -30_017,
                message: "CLIENT, Param not bind".to_string(),
            };
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        let api = api()?;
        let mut lob_handles = Vec::new();
        for (occurrence, slot) in self.order.iter().enumerate() {
            let slot = (*slot as usize).saturating_sub(1);
            let result = match &mut self.binds[slot] {
                BindValue::Null => unsafe {
                    (api.bind)(self.request, occurrence as i32 + 1, CCI_A_TYPE_STR, ptr::null_mut(), CCI_U_TYPE_NULL, 0)
                },
                BindValue::Int(value) => unsafe {
                    (api.bind)(self.request, occurrence as i32 + 1, CCI_A_TYPE_INT, (value as *mut i32).cast(), CCI_U_TYPE_INT, 0)
                },
                BindValue::BigInt(value) => unsafe {
                    (api.bind)(self.request, occurrence as i32 + 1, CCI_A_TYPE_BIGINT, (value as *mut i64).cast(), CCI_U_TYPE_BIGINT, 0)
                },
                BindValue::Double(value) => unsafe {
                    (api.bind)(self.request, occurrence as i32 + 1, CCI_A_TYPE_DOUBLE, (value as *mut f64).cast(), CCI_U_TYPE_DOUBLE, 0)
                },
                BindValue::Text(value) => {
                    value.push(0);
                    let result = unsafe {
                        (api.bind)(self.request, occurrence as i32 + 1, CCI_A_TYPE_STR, value.as_mut_ptr().cast(), CCI_U_TYPE_STRING, 0)
                    };
                    value.pop();
                    result
                }
                BindValue::TypedText(value, domain) => {
                    value.push(0);
                    let result = unsafe {
                        (api.bind)(self.request, occurrence as i32 + 1, CCI_A_TYPE_STR, value.as_mut_ptr().cast(), *domain, 0)
                    };
                    value.pop();
                    result
                }
                BindValue::Bit(value, domain) => {
                    let mut bit = NativeBit {
                        size: value.len() as i32,
                        buffer: value.as_mut_ptr().cast(),
                    };
                    unsafe {
                        (api.bind)(self.request, occurrence as i32 + 1, CCI_A_TYPE_BIT, (&mut bit as *mut NativeBit).cast(), *domain, 0)
                    }
                }
                BindValue::Lob(value, domain) => {
                    let mut lob = ptr::null_mut();
                    let mut error = NativeError::default();
                    let created = unsafe {
                        if *domain == CCI_U_TYPE_BLOB {
                            (api.blob_new)(connection.handle, &mut lob, &mut error)
                        } else {
                            (api.clob_new)(connection.handle, &mut lob, &mut error)
                        }
                    };
                    if created < 0 {
                        self.error = native_error(created, &error);
                        connection.error = self.error.clone();
                        return Err(self.error.message.clone());
                    }
                    let written = unsafe { if *domain == CCI_U_TYPE_BLOB {
                        (api.blob_write)(connection.handle, lob, 0, value.len() as i32, value.as_ptr().cast(), &mut error)
                    } else {
                        (api.clob_write)(connection.handle, lob, 0, value.len() as i32, value.as_ptr().cast(), &mut error)
                    } };
                    if written < 0 {
                        unsafe { if *domain == CCI_U_TYPE_BLOB { (api.blob_free)(lob) } else { (api.clob_free)(lob) } };
                        self.error = native_error(written, &error);
                        connection.error = self.error.clone();
                        return Err(self.error.message.clone());
                    }
                    lob_handles.push((lob, *domain));
                    unsafe {
                        let a_type = if *domain == CCI_U_TYPE_BLOB { CCI_A_TYPE_BLOB } else { CCI_A_TYPE_CLOB };
                        (api.bind)(self.request, occurrence as i32 + 1, a_type, lob, *domain, 1)
                    }
                }
                BindValue::Set(values, element_type) => {
                    let mut set = ptr::null_mut();
                    let mut indicators = values.iter().map(|value| i32::from(value == b"NULL")).collect::<Vec<_>>();
                    let result = if *element_type == CCI_U_TYPE_BIT || *element_type == CCI_U_TYPE_VARBIT {
                        let mut bits = values.iter_mut().map(|value| NativeBit {
                            size: value.len() as i32,
                            buffer: value.as_mut_ptr().cast(),
                        }).collect::<Vec<_>>();
                        unsafe { (api.set_make)(&mut set, *element_type, bits.len() as i32, bits.as_mut_ptr().cast(), indicators.as_mut_ptr()) }
                    } else {
                        let mut strings = values.iter_mut().map(|value| {
                            if let Some(nul) = value.iter().position(|byte| *byte == 0) {
                                value.truncate(nul);
                            }
                            value.push(0);
                            value.as_mut_ptr().cast::<c_char>()
                        }).collect::<Vec<_>>();
                        unsafe { (api.set_make)(&mut set, CCI_U_TYPE_STRING, strings.len() as i32, strings.as_mut_ptr().cast(), indicators.as_mut_ptr()) }
                    };
                    if result < 0 {
                        result
                    } else {
                        let bound = unsafe { (api.bind)(self.request, occurrence as i32 + 1, CCI_A_TYPE_SET, set, CCI_U_TYPE_SET, 0) };
                        unsafe { (api.set_free)(set) };
                        bound
                    }
                }
            };
            if result < 0 {
                self.error = ErrorState {
                    sqlstate: "HY000".to_string(),
                    code: result as i64,
                    message: format!("CCI, CCI bind error {result}"),
                };
                connection.error = self.error.clone();
                for (lob, domain) in lob_handles {
                    unsafe { if domain == CCI_U_TYPE_BLOB { (api.blob_free)(lob) } else { (api.clob_free)(lob) } };
                }
                return Err(self.error.message.clone());
            }
        }
        let mut error = NativeError::default();
        let result = unsafe { (api.execute)(self.request, CCI_EXEC_QUERY_ALL, 0, &mut error) };
        for (lob, domain) in lob_handles {
            unsafe { if domain == CCI_U_TYPE_BLOB { (api.blob_free)(lob) } else { (api.clob_free)(lob) } };
        }
        if result < 0 {
            self.error = native_error(result, &error);
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        self.executed = true;
        self.row_count = result as i64;
        connection.changes = self.row_count;
        self.materialize(connection)
    }

    /// Copies active CCI metadata and all rows before the next native result replaces them.
    fn materialize(&mut self, connection: &mut CubridConn) -> Result<(), String> {
        self.columns.clear();
        self.rows.clear();
        self.cursor = -1;
        let api = api()?;
        let mut statement_type = -1;
        let mut count = 0;
        let metadata = unsafe { (api.result_info)(self.request, &mut statement_type, &mut count) };
        if count > 0 && metadata.is_null() {
            self.error = ErrorState {
                sqlstate: "HY000".to_string(),
                code: -30_003,
                message: "CLIENT, Cannot get column info".to_string(),
            };
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        for index in 0..count.max(0) as usize {
            let native = unsafe { &*metadata.add(index) };
            let mut flags = 0i64;
            flags |= i64::from(native.is_non_null != 0);
            flags |= i64::from(native.is_auto_increment != 0) << 1;
            flags |= i64::from(native.is_unique_key != 0) << 2;
            flags |= i64::from(native.is_primary_key != 0) << 3;
            flags |= i64::from(native.is_foreign_key != 0) << 4;
            flags |= i64::from(native.is_reverse_index != 0) << 5;
            flags |= i64::from(native.is_reverse_unique != 0) << 6;
            self.columns.push(Column {
                name: copy_c_string(native.col_name),
                table: copy_c_string(native.class_name),
                default_value: copy_c_string(native.default_value),
                native_type: native_type_name(native.ext_type, native.precision, native.scale),
                ext_type: native.ext_type,
                precision: native.precision as i64,
                scale: native.scale as i64,
                flags,
            });
        }
        if statement_type == CUBRID_STMT_INSERT
            || statement_type == CUBRID_STMT_UPDATE
            || statement_type == CUBRID_STMT_DELETE
        {
            connection.changes = self.row_count;
        }
        if statement_type == CUBRID_STMT_SELECT || count > 0 {
            loop {
                let mut error = NativeError::default();
                let positioned = unsafe { (api.cursor)(self.request, 1, CCI_CURSOR_CURRENT, &mut error) };
                if positioned == CCI_ER_NO_MORE_DATA {
                    break;
                }
                if positioned < 0 {
                    self.error = native_error(positioned, &error);
                    connection.error = self.error.clone();
                    return Err(self.error.message.clone());
                }
                let fetched = unsafe { (api.fetch)(self.request, &mut error) };
                if fetched < 0 {
                    self.error = native_error(fetched, &error);
                    connection.error = self.error.clone();
                    return Err(self.error.message.clone());
                }
                let mut row = Vec::with_capacity(self.columns.len());
                for (index, column) in self.columns.iter().enumerate() {
                    row.push(Self::read_column(api, connection.handle, self.request, index, column)?);
                }
                self.rows.push(row);
            }
            self.row_count = self.rows.len() as i64;
        }
        Ok(())
    }

    /// Copies one scalar or LOB result cell out of CCI-owned memory.
    fn read_column(
        api: &CciApi,
        connection: i32,
        request: i32,
        index: usize,
        column: &Column,
    ) -> Result<Option<Vec<u8>>, String> {
        let domain = collection_domain(column.ext_type);
        if domain == CCI_U_TYPE_BLOB as u8 || domain == CCI_U_TYPE_CLOB as u8 {
            let mut lob: *mut c_void = ptr::null_mut();
            let mut indicator = 0;
            let a_type = if domain == CCI_U_TYPE_BLOB as u8 { CCI_A_TYPE_BLOB } else { CCI_A_TYPE_CLOB };
            let result = unsafe {
                (api.get_data)(request, index as i32 + 1, a_type, (&mut lob as *mut *mut c_void).cast(), &mut indicator)
            };
            if result < 0 {
                return Err(format!("CCI, CCI get-data error {result}"));
            }
            if indicator < 0 || lob.is_null() {
                return Ok(None);
            }
            let size = unsafe { if domain == CCI_U_TYPE_BLOB as u8 { (api.blob_size)(lob) } else { (api.clob_size)(lob) } };
            let mut output = vec![0u8; usize::try_from(size.max(0)).unwrap_or(0)];
            if output.is_empty() {
                unsafe { if domain == CCI_U_TYPE_BLOB as u8 { (api.blob_free)(lob) } else { (api.clob_free)(lob) } };
                return Ok(Some(output));
            }
            let mut error = NativeError::default();
            let read = unsafe {
                if domain == CCI_U_TYPE_BLOB as u8 {
                    (api.blob_read)(connection, lob, 0, output.len() as i32, output.as_mut_ptr().cast(), &mut error)
                } else {
                    (api.clob_read)(connection, lob, 0, output.len() as i32, output.as_mut_ptr().cast(), &mut error)
                }
            };
            unsafe { if domain == CCI_U_TYPE_BLOB as u8 { (api.blob_free)(lob) } else { (api.clob_free)(lob) } };
            if read < 0 {
                return Err(native_error(read, &error).message);
            }
            output.truncate(usize::try_from(read).unwrap_or(0).min(output.len()));
            return Ok(Some(output));
        }
        let mut pointer: *mut c_char = ptr::null_mut();
        let mut indicator = 0;
        let result = unsafe {
            (api.get_data)(request, index as i32 + 1, CCI_A_TYPE_STR, (&mut pointer as *mut *mut c_char).cast(), &mut indicator)
        };
        if result < 0 {
            return Err(format!("CCI, CCI get-data error {result}"));
        }
        if indicator < 0 || pointer.is_null() {
            Ok(None)
        } else {
            Ok(Some(unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), indicator as usize) }.to_vec()))
        }
    }

    /// Advances to the next materialized row.
    pub fn step(&mut self) -> i64 {
        let next = self.cursor + 1;
        if next < self.rows.len() as isize {
            self.cursor = next;
            1
        } else {
            0
        }
    }

    /// Selects one materialized row using PDO fetch-orientation constants.
    pub fn step_oriented(&mut self, orientation: i64, offset: i64) -> i64 {
        let target = match orientation {
            0 => self.cursor + 1,
            1 => self.cursor - 1,
            2 => 0,
            3 => self.rows.len() as isize - 1,
            4 if offset > 0 => match isize::try_from(offset - 1) {
                Ok(offset) => offset,
                Err(_) => return 0,
            },
            4 => return 0,
            5 => self.cursor + offset as isize,
            _ => return 0,
        };
        if target < 0 || target >= self.rows.len() as isize {
            0
        } else {
            self.cursor = target;
            1
        }
    }

    /// Advances to and materializes CCI's next result set.
    pub fn next_rowset(&mut self, connection: &mut CubridConn) -> bool {
        let mut error = NativeError::default();
        let result = unsafe { (api().expect("CCI loaded").next_result)(self.request, &mut error) };
        if result == CAS_ER_NO_MORE_RESULT_SET {
            return false;
        }
        if result < 0 {
            self.error = native_error(result, &error);
            connection.error = self.error.clone();
            return false;
        }
        self.row_count = result as i64;
        self.materialize(connection).is_ok()
    }

    /// Returns the affected/result row count reported by CCI.
    pub fn row_count(&self) -> i64 {
        self.row_count
    }

    /// Returns the active result column count.
    pub fn column_count(&self) -> i64 {
        self.columns.len() as i64
    }

    /// Returns one active result column name.
    pub fn column_name(&self, index: i64) -> String {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index)).map(|column| column.name.clone()).unwrap_or_default()
    }

    /// Returns PDO's text, LOB, or NULL storage tag for one current cell.
    pub fn column_type(&self, index: i64) -> i64 {
        if self.cell(index).is_none_or(Option::is_none) {
            return 5;
        }
        let domain = usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .map(|column| collection_domain(column.ext_type))
            .unwrap_or_default();
        if domain == CCI_U_TYPE_BLOB as u8 || domain == CCI_U_TYPE_CLOB as u8 { 4 } else { 3 }
    }

    /// Returns one current value parsed as an integer.
    pub fn column_int(&self, index: i64) -> i64 {
        String::from_utf8_lossy(&self.column_data(index)).parse().unwrap_or(0)
    }

    /// Returns one current value parsed as a double.
    pub fn column_double(&self, index: i64) -> f64 {
        String::from_utf8_lossy(&self.column_data(index)).parse().unwrap_or(0.0)
    }

    /// Returns one current value's exact bytes.
    pub fn column_data(&self, index: i64) -> Vec<u8> {
        self.cell(index).and_then(Option::as_ref).cloned().unwrap_or_default()
    }

    /// Returns one current row cell.
    fn cell(&self, index: i64) -> Option<&Option<Vec<u8>>> {
        let row = usize::try_from(self.cursor).ok().and_then(|row| self.rows.get(row))?;
        usize::try_from(index).ok().and_then(|index| row.get(index))
    }

    /// Returns the native PDO_CUBRID type string.
    pub fn column_native_type(&self, index: i64) -> String {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index)).map(|column| column.native_type.clone()).unwrap_or_default()
    }

    /// Returns the source class/table name.
    pub fn column_table_name(&self, index: i64) -> String {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index)).map(|column| column.table.clone()).unwrap_or_default()
    }

    /// Returns the PDO_CUBRID metadata default value.
    pub fn column_default(&self, index: i64) -> String {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index)).map(|column| column.default_value.clone()).unwrap_or_default()
    }

    /// Returns the declared CUBRID precision.
    pub fn column_precision(&self, index: i64) -> i64 {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index)).map(|column| column.precision).unwrap_or_default()
    }

    /// Returns the declared CUBRID scale.
    pub fn column_scale(&self, index: i64) -> i64 {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index)).map(|column| column.scale).unwrap_or_default()
    }

    /// Returns packed not-null/key/index metadata flags.
    pub fn column_flags(&self, index: i64) -> i64 {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index)).map(|column| column.flags).unwrap_or_default()
    }

    /// Returns the statement SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        &self.error.sqlstate
    }

    /// Returns the statement native code.
    pub fn errcode(&self) -> i64 {
        self.error.code
    }

    /// Returns the statement diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses defaults, credentials, and pass-through CCI URL options.
    #[test]
    fn parses_cubrid_dsn() {
        let dsn = parse_dsn("cubrid:host=db;port=33000;dbname=app;user=scott;password=t%3Bger;althosts=db2%25").unwrap();
        assert_eq!(dsn.user, "scott");
        assert_eq!(dsn.password, "t;ger");
        assert_eq!(dsn.url, "cci:CUBRID:db:33000:app:scott:t;ger:?althosts=db2%25");
    }

    /// Matches upstream's native metadata spelling for representative types.
    #[test]
    fn formats_native_types() {
        assert_eq!(native_type_name(2, 40, 0), "varchar(40)");
        assert_eq!(native_type_name(7, 10, 2), "numeric(10,2)");
        assert_eq!(native_type_name(23, 0, 0), "blob");
        assert_eq!(native_type_name(0x22, 40, 0), "set(varchar(40))");
        assert_eq!(native_type_name(0x82, 0, 0), "[unknown]");
        assert_eq!(named_type("unknown"), None);
    }

    /// Decodes empty and byte-containing collection frames without delimiter ambiguity.
    #[test]
    fn decodes_set_frames() {
        assert_eq!(decode_set(b"0:").unwrap(), Vec::<Vec<u8>>::new());
        assert_eq!(decode_set(b"3:1:a0:3:b:c").unwrap(), vec![b"a".to_vec(), Vec::new(), b"b:c".to_vec()]);
        assert!(decode_set(b"1:4:abc").is_none());
    }

    /// Preserves invalid percent sequences while decoding valid credentials.
    #[test]
    fn decodes_credentials_losslessly() {
        assert_eq!(decode_component("a%20b%ZZ"), "a b%ZZ");
    }

}
