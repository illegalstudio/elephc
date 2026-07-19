//! Purpose:
//! System CLI backend matching PDO_ODBC, PDO_INFORMIX, PDO_IBM, and PDO_SQLSRV.
//!
//! Called from:
//! - The PDO bridge root with the optional `odbc`, `informix`, `ibm`, or `sqlsrv` feature.
//!
//! Key details:
//! - Uses the ODBC 3 CLI ABI through `odbc-sys`, as the official drivers delegate to a driver manager.
//! - Materializes scalar result rows as text/null and preserves driver-specific LOB/type metadata.
//! - Keeps statement handles alive across `SQLMoreResults`, cursor-name, and scroll operations.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::{Mutex, OnceLock};

use odbc_sys::{
    AttrOdbcVersion, CDataType, CompletionType, ConnectionAttribute, Desc, DriverConnectOption,
    EnvironmentAttribute, FetchOrientation, FreeStmtOption, HDbc, HEnv, HStmt, Handle, HandleType,
    InfoType, NULL_DATA, Nullability, ParamType, SqlDataType, SqlReturn, SQLAllocHandle,
    SQLBindParameter, SQLCloseCursor, SQLColAttribute, SQLConnect, SQLDescribeCol, SQLDescribeParam, SQLDisconnect, SQLDriverConnect,
    SQLEndTran, SQLExecDirect, SQLExecute, SQLFetch, SQLFreeHandle, SQLFreeStmt,
    SQLGetData, SQLGetDiagRec, SQLGetInfo, SQLMoreResults, SQLNumParams, SQLNumResultCols,
    SQLPrepare, SQLPrepareW, SQLRowCount, SQLSetConnectAttr, SQLSetEnvAttr, SQLSetStmtAttr,
    SQLDrivers, SQLDriverConnectW, SQLExecDirectW, StatementAttribute,
};
#[cfg(feature = "sqlsrv")]
use odbc_sys::{HDesc, SQLColAttributeW, SQLDescribeColW, SQLGetStmtAttr};

const SQL_AUTOCOMMIT_OFF: isize = 0;
const SQL_AUTOCOMMIT_ON: isize = 1;
const SQL_CUR_USE_IF_NEEDED: i64 = 0;
const SQL_CUR_USE_ODBC: i64 = 1;
const SQL_CUR_USE_DRIVER: i64 = 2;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_ENCODING: i64 = 1000;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_QUERY_TIMEOUT: i64 = 1001;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_DIRECT_QUERY: i64 = 1002;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_CURSOR_SCROLL_TYPE: i64 = 1003;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_CLIENT_BUFFER_MAX_KB_SIZE: i64 = 1004;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_FETCHES_NUMERIC_TYPE: i64 = 1005;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_FETCHES_DATETIME_TYPE: i64 = 1006;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_FORMAT_DECIMALS: i64 = 1007;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_DECIMAL_PLACES: i64 = 1008;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ATTR_DATA_CLASSIFICATION: i64 = 1009;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ENCODING_DEFAULT: i64 = 1;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ENCODING_BINARY: i64 = 2;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ENCODING_SYSTEM: i64 = 3;
#[cfg(feature = "sqlsrv")]
const SQLSRV_ENCODING_UTF8: i64 = 65001;
#[cfg(feature = "sqlsrv")]
const SQL_COPT_SS_ACCESS_TOKEN: i32 = 1256;
#[cfg(feature = "sqlsrv")]
const SQL_COPT_SS_DATACLASSIFICATION_VERSION: i32 = 1400;
#[cfg(feature = "sqlsrv")]
const SQL_CA_SS_DATA_CLASSIFICATION: i16 = 1237;
#[cfg(feature = "sqlsrv")]
const SQL_CA_SS_DATA_CLASSIFICATION_VERSION: i16 = 1238;
#[cfg(feature = "informix")]
const SQL_INFX_ATTR_ODBC_TYPES_ONLY: i32 = 2257;
#[cfg(feature = "informix")]
const SQL_INFX_ATTR_LO_AUTOMATIC: i32 = 2262;
#[cfg(feature = "ibm")]
const PDO_IBM_ATTR_INFO_USERID: i32 = 1281;
#[cfg(feature = "ibm")]
const PDO_IBM_ATTR_INFO_ACCTSTR: i32 = 1282;
#[cfg(feature = "ibm")]
const PDO_IBM_ATTR_INFO_APPLNAME: i32 = 1283;
#[cfg(feature = "ibm")]
const PDO_IBM_ATTR_INFO_WRKSTNNAME: i32 = 1284;
#[cfg(feature = "ibm")]
const PDO_IBM_ATTR_USE_TRUSTED_CONTEXT: i32 = 2561;
#[cfg(feature = "ibm")]
const PDO_IBM_ATTR_TRUSTED_CONTEXT_USERID: i32 = 2562;
#[cfg(feature = "ibm")]
const PDO_IBM_ATTR_TRUSTED_CONTEXT_PASSWORD: i32 = 2563;
#[cfg(feature = "ibm")]
const SQL_IBM_ATTR_GET_GENERATED_VALUE: i32 = 2583;

unsafe extern "system" {
    /// Applies a driver-specific numeric connection attribute not modeled by `odbc-sys`.
    #[cfg(any(feature = "informix", feature = "ibm", feature = "sqlsrv"))]
    #[link_name = "SQLSetConnectAttr"]
    fn SQLSetConnectAttrRaw(
        connection_handle: HDbc,
        attribute: i32,
        value: *mut c_void,
        string_length: i32,
    ) -> SqlReturn;
    /// Reads a driver-specific connection attribute not modeled by `odbc-sys`.
    #[cfg(feature = "ibm")]
    #[link_name = "SQLGetConnectAttr"]
    fn SQLGetConnectAttrRaw(
        connection_handle: HDbc,
        attribute: i32,
        value: *mut c_void,
        buffer_length: i32,
        string_length: *mut i32,
    ) -> SqlReturn;
    /// Reads IBM's generated-value statement attribute not modeled by `odbc-sys`.
    #[cfg(feature = "ibm")]
    #[link_name = "SQLGetStmtAttr"]
    fn SQLGetStmtAttrRaw(
        statement_handle: HStmt,
        attribute: i32,
        value: *mut c_void,
        buffer_length: i32,
        string_length: *mut i32,
    ) -> SqlReturn;
    /// Reads a Microsoft implementation-row-descriptor field outside `odbc-sys`'s enum.
    #[cfg(feature = "sqlsrv")]
    #[link_name = "SQLGetDescFieldW"]
    fn SQLGetDescFieldWRaw(
        descriptor_handle: HDesc,
        record_number: i16,
        field_identifier: i16,
        value: *mut c_void,
        buffer_length: i32,
        string_length: *mut i32,
    ) -> SqlReturn;
    /// Assigns an ANSI cursor name to a prepared ODBC statement.
    fn SQLSetCursorName(
        statement_handle: HStmt,
        cursor_name: *const u8,
        name_length: i16,
    ) -> SqlReturn;
    /// Reads the ANSI cursor name assigned to a prepared ODBC statement.
    fn SQLGetCursorName(
        statement_handle: HStmt,
        cursor_name: *mut u8,
        buffer_length: i16,
        name_length: *mut i16,
    ) -> SqlReturn;
}

/// Selects the PDO extension semantics layered over the shared CLI ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CliFlavor {
    #[cfg(feature = "odbc")]
    Odbc,
    #[cfg(feature = "informix")]
    Informix,
    #[cfg(feature = "ibm")]
    Ibm,
    #[cfg(feature = "sqlsrv")]
    Sqlsrv,
}

/// Maps PDO_IBM's public sequential constants to IBM CLI's native attribute IDs.
#[cfg(feature = "ibm")]
fn ibm_native_connection_attribute(attribute: i32) -> Option<i32> {
    match attribute {
        PDO_IBM_ATTR_INFO_USERID => Some(1281),
        PDO_IBM_ATTR_INFO_ACCTSTR => Some(1284),
        PDO_IBM_ATTR_INFO_APPLNAME => Some(1283),
        PDO_IBM_ATTR_INFO_WRKSTNNAME => Some(1282),
        PDO_IBM_ATTR_USE_TRUSTED_CONTEXT => Some(2561),
        PDO_IBM_ATTR_TRUSTED_CONTEXT_USERID => Some(2562),
        PDO_IBM_ATTR_TRUSTED_CONTEXT_PASSWORD => Some(2563),
        _ => None,
    }
}

impl CliFlavor {
    /// Returns the exact PDO DSN prefix owned by this extension.
    fn dsn_prefix(self) -> &'static str {
        match self {
            #[cfg(feature = "odbc")]
            Self::Odbc => "odbc:",
            #[cfg(feature = "informix")]
            Self::Informix => "informix:",
            #[cfg(feature = "ibm")]
            Self::Ibm => "ibm:",
            #[cfg(feature = "sqlsrv")]
            Self::Sqlsrv => "sqlsrv:",
        }
    }

    /// Reports whether this flavor implements Microsoft's PDO_SQLSRV extension.
    fn is_sqlsrv(self) -> bool {
        #[cfg(feature = "sqlsrv")]
        if self == Self::Sqlsrv {
            return true;
        }
        false
    }
}

/// PDO-visible ODBC diagnostic record.
#[derive(Clone)]
struct ErrorState {
    sqlstate: String,
    native_code: i64,
    message: String,
}

impl Default for ErrorState {
    /// Creates the successful/no-error PDO state.
    fn default() -> Self {
        Self {
            sqlstate: "00000".to_string(),
            native_code: 0,
            message: String::new(),
        }
    }
}

/// Holds the diagnostic produced before a CLI connection handle enters the bridge table.
fn open_error_cell() -> &'static Mutex<ErrorState> {
    static ERROR: OnceLock<Mutex<ErrorState>> = OnceLock::new();
    ERROR.get_or_init(|| Mutex::new(ErrorState::default()))
}

/// Records one constructor failure for PDO's connection-level `errorInfo` fields.
fn remember_open_error(error: &ErrorState) {
    *open_error_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = error.clone();
}

/// Returns the SQLSTATE and native code captured by the latest failed CLI open.
pub(crate) fn open_diagnostic() -> (String, i64) {
    let error = open_error_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (error.sqlstate.clone(), error.native_code)
}

/// Reports whether an ODBC return code completed successfully.
fn succeeded(result: SqlReturn) -> bool {
    matches!(result, SqlReturn::SUCCESS | SqlReturn::SUCCESS_WITH_INFO)
}

/// Reads the first ODBC diagnostic record from a native handle.
fn diagnostic(handle_type: HandleType, handle: Handle, context: &str) -> ErrorState {
    let mut state = [0u8; 6];
    let mut native = 0i32;
    let mut message = [0u8; 1024];
    let mut length = 0i16;
    let result = unsafe {
        SQLGetDiagRec(
            handle_type,
            handle,
            1,
            state.as_mut_ptr(),
            &mut native,
            message.as_mut_ptr(),
            message.len() as i16,
            &mut length,
        )
    };
    if !succeeded(result) {
        return ErrorState {
            sqlstate: "HY000".to_string(),
            native_code: 0,
            message: context.to_string(),
        };
    }
    let state_len = state.iter().position(|byte| *byte == 0).unwrap_or(5);
    let message_len = usize::try_from(length).unwrap_or(0).min(message.len());
    ErrorState {
        sqlstate: String::from_utf8_lossy(&state[..state_len]).into_owned(),
        native_code: i64::from(native),
        message: format!("{context}: {}", String::from_utf8_lossy(&message[..message_len])),
    }
}

/// Percent-decodes constructor credentials folded into the internal bridge DSN.
fn decode_credential(value: &str) -> String {
    value
        .replace("%3B", ";")
        .replace("%3b", ";")
        .replace("%25", "%")
}

/// Quotes a constructor credential for an ODBC connection string.
fn quote_connection_value(value: &str) -> String {
    if value.starts_with('{') && value.ends_with('}') {
        value.to_string()
    } else if value.contains(';') || value.contains('}') {
        format!("{{{}}}", value.replace('}', "}}"))
    } else {
        value.to_string()
    }
}

/// Computes the stable token fingerprint PDO_SQLSRV adds to the pooling key.
#[cfg(feature = "sqlsrv")]
fn sqlsrv_token_fingerprint(token: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in token {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Builds Microsoft's aligned `ACCESSTOKEN` header plus zero-padded token bytes.
#[cfg(feature = "sqlsrv")]
fn sqlsrv_access_token_buffer(token: &[u8]) -> Vec<u32> {
    let payload_len = token.len().saturating_mul(2);
    let byte_len = 4usize.saturating_add(payload_len);
    let mut buffer = vec![0u32; byte_len.saturating_add(3) / 4];
    buffer[0] = u32::try_from(payload_len).unwrap_or(u32::MAX);
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), buffer.len() * 4)
    };
    for (index, byte) in token.iter().enumerate() {
        bytes[4 + index * 2] = *byte;
    }
    buffer
}

/// Reads unixODBC/iODBC's process-level `[ODBC] Pooling` switch.
#[cfg(feature = "sqlsrv")]
fn sqlsrv_driver_manager_pooling_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let file_name = std::env::var_os("ODBCINSTINI")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("odbcinst.ini"));
        let mut candidates = Vec::new();
        if file_name.is_absolute() {
            candidates.push(file_name.clone());
        }
        if let Some(directory) = std::env::var_os("ODBCSYSINI") {
            candidates.push(std::path::PathBuf::from(directory).join(&file_name));
        }
        candidates.extend([
            std::path::PathBuf::from("/etc").join(&file_name),
            std::path::PathBuf::from("/usr/local/etc").join(&file_name),
            std::path::PathBuf::from("/opt/homebrew/etc").join(&file_name),
        ]);
        for candidate in candidates {
            let Ok(contents) = std::fs::read_to_string(candidate) else {
                continue;
            };
            if let Some(enabled) = sqlsrv_pooling_from_ini(&contents) {
                return enabled;
            }
        }
        false
    })
}

/// Extracts `[ODBC] Pooling` from one driver-manager INI document.
#[cfg(feature = "sqlsrv")]
fn sqlsrv_pooling_from_ini(contents: &str) -> Option<bool> {
    let mut in_odbc = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_odbc = line[1..line.len() - 1].eq_ignore_ascii_case("odbc");
            continue;
        }
        if !in_odbc || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("pooling") {
            return Some(matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "yes" | "on" | "true"
            ));
        }
    }
    None
}

/// Returns the process-lifetime pooled SQLSRV ODBC environment.
#[cfg(feature = "sqlsrv")]
fn sqlsrv_pooled_environment() -> Result<Handle, String> {
    static ENVIRONMENT: OnceLock<Result<usize, String>> = OnceLock::new();
    match ENVIRONMENT.get_or_init(|| {
        let mut env = Handle::null();
        if !succeeded(unsafe { SQLAllocHandle(HandleType::Env, Handle::null(), &mut env) }) {
            return Err("SQLAllocHandle: pooled ENV failed".to_string());
        }
        let version = unsafe {
            SQLSetEnvAttr(
                env.as_henv(),
                EnvironmentAttribute::OdbcVersion,
                AttrOdbcVersion::Odbc3.into(),
                0,
            )
        };
        let pooling = unsafe {
            SQLSetEnvAttr(
                env.as_henv(),
                EnvironmentAttribute::ConnectionPooling,
                2isize as *mut c_void,
                odbc_sys::IS_UINTEGER,
            )
        };
        if !succeeded(version) || !succeeded(pooling) {
            unsafe { let _ = SQLFreeHandle(HandleType::Env, env); };
            return Err("SQLSetEnvAttr: pooled ODBC3 environment failed".to_string());
        }
        Ok(env.0 as usize)
    }) {
        Ok(pointer) => Ok(Handle(*pointer as *mut c_void)),
        Err(message) => Err(message.clone()),
    }
}

/// Frees a connection-private ODBC environment while retaining shared pooled ones.
fn free_environment_if_owned(env: Handle, owned: bool) {
    if owned {
        unsafe { let _ = SQLFreeHandle(HandleType::Env, env); };
    }
}

/// Allocates one connection-private ODBC 3 environment.
fn private_odbc_environment() -> Result<Handle, String> {
    let mut env = Handle::null();
    if !succeeded(unsafe { SQLAllocHandle(HandleType::Env, Handle::null(), &mut env) }) {
        return Err("SQLAllocHandle: ENV failed".to_string());
    }
    let version = unsafe {
        SQLSetEnvAttr(
            env.as_henv(),
            EnvironmentAttribute::OdbcVersion,
            AttrOdbcVersion::Odbc3.into(),
            0,
        )
    };
    if !succeeded(version) {
        free_environment_if_owned(env, true);
        return Err("SQLSetEnvAttr: ODBC3 failed".to_string());
    }
    Ok(env)
}

/// Selects SQLSRV's externally configured pooled environment or a private one.
#[cfg(feature = "sqlsrv")]
fn connection_environment(flavor: CliFlavor) -> Result<(Handle, bool), String> {
    if flavor.is_sqlsrv() && sqlsrv_driver_manager_pooling_enabled() {
        return sqlsrv_pooled_environment().map(|env| (env, false));
    }
    private_odbc_environment().map(|env| (env, true))
}

/// Selects a private environment when PDO_SQLSRV is not in this bridge build.
#[cfg(not(feature = "sqlsrv"))]
fn connection_environment(_flavor: CliFlavor) -> Result<(Handle, bool), String> {
    private_odbc_environment().map(|env| (env, true))
}

/// Parsed ODBC DSN and bridge-only constructor options.
struct OpenOptions {
    source: String,
    username: String,
    password: String,
    #[cfg(feature = "sqlsrv")]
    username_supplied: bool,
    #[cfg(feature = "sqlsrv")]
    password_supplied: bool,
    cursor_library: i64,
    assume_utf8: bool,
    auto_commit: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_access_token: Option<Vec<u8>>,
    #[cfg(feature = "ibm")]
    ibm_attributes: Vec<(i32, String)>,
}

/// Enumerates installed ODBC drivers and selects Microsoft's newest SQL Server driver.
fn sql_server_driver(env: HEnv) -> Option<String> {
    let mut direction = FetchOrientation::First;
    let mut candidates = Vec::new();
    loop {
        let mut description = [0u8; 256];
        let mut description_len = 0i16;
        let mut attributes = [0u8; 1024];
        let mut attributes_len = 0i16;
        let result = unsafe {
            SQLDrivers(
                env,
                direction,
                description.as_mut_ptr(),
                description.len() as i16,
                &mut description_len,
                attributes.as_mut_ptr(),
                attributes.len() as i16,
                &mut attributes_len,
            )
        };
        if result == SqlReturn::NO_DATA {
            break;
        }
        if !succeeded(result) {
            return None;
        }
        let length = usize::try_from(description_len).unwrap_or(0).min(description.len());
        let name = String::from_utf8_lossy(&description[..length]).into_owned();
        if name.to_ascii_lowercase().contains("sql server") {
            candidates.push(name);
        }
        direction = FetchOrientation::Next;
    }
    candidates.into_iter().max_by_key(|name| {
        let lower = name.to_ascii_lowercase();
        if lower.contains("odbc driver 18") {
            18
        } else if lower.contains("odbc driver 17") {
            17
        } else {
            0
        }
    })
}

/// Splits an ODBC connection string without treating semicolons inside braced values as separators.
fn split_connection_fields(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut fields = Vec::new();
    let mut start = 0usize;
    let mut braced = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'{' if !braced => braced = true,
            b'}' if braced => {
                if bytes.get(index + 1) == Some(&b'}') {
                    index += 1;
                } else {
                    braced = false;
                }
            }
            b';' if !braced => {
                fields.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    fields.push(&body[start..]);
    fields
}

/// Separates PDO_ODBC's DSN from bridge-only constructor fields.
fn parse_open_options(dsn: &str, flavor: CliFlavor) -> Result<OpenOptions, String> {
    let body = dsn
        .strip_prefix(flavor.dsn_prefix())
        .ok_or_else(|| "could not find driver".to_string())?;
    let mut source_parts = Vec::new();
    let mut username = String::new();
    let mut password = String::new();
    #[cfg(feature = "sqlsrv")]
    let mut username_supplied = false;
    #[cfg(feature = "sqlsrv")]
    let mut password_supplied = false;
    let mut cursor_library = SQL_CUR_USE_IF_NEEDED;
    let mut assume_utf8 = false;
    let mut auto_commit = true;
    #[cfg(feature = "ibm")]
    let mut ibm_attributes = Vec::new();
    #[cfg(feature = "sqlsrv")]
    let mut sqlsrv_access_token = None;
    for part in split_connection_fields(body) {
        let lower = part.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("user=") {
            let offset = part.len() - value.len();
            username = decode_credential(&part[offset..]);
            #[cfg(feature = "sqlsrv")]
            {
                username_supplied = true;
            }
        } else if let Some(value) = lower.strip_prefix("password=") {
            let offset = part.len() - value.len();
            password = decode_credential(&part[offset..]);
            #[cfg(feature = "sqlsrv")]
            {
                password_supplied = true;
            }
        } else if flavor.is_sqlsrv() && lower.starts_with("accesstoken=") {
            #[cfg(feature = "sqlsrv")]
            {
                let value = &part["accesstoken=".len()..];
                let value = value
                    .strip_prefix('{')
                    .and_then(|value| value.strip_suffix('}'))
                    .unwrap_or(value)
                    .replace("}}", "}");
                if value.is_empty() {
                    return Err("Access token must not be empty".to_string());
                }
                sqlsrv_access_token = Some(value.into_bytes());
            }
        } else if let Some(value) = lower.strip_prefix("elephc_odbc_cursor_library=") {
            cursor_library = value.parse().unwrap_or(SQL_CUR_USE_IF_NEEDED);
        } else if let Some(value) = lower.strip_prefix("elephc_odbc_assume_utf8=") {
            assume_utf8 = value != "0";
        } else if let Some(value) = lower.strip_prefix("elephc_odbc_autocommit=") {
            auto_commit = value != "0";
        } else if let Some(value) = lower.strip_prefix("elephc_ibm_attr_") {
            #[cfg(feature = "ibm")]
            if let Some((attribute, _)) = value.split_once('=') {
                let key_length = "elephc_ibm_attr_".len() + attribute.len() + 1;
                if let Ok(attribute) = attribute.parse() {
                    ibm_attributes.push((attribute, decode_credential(&part[key_length..])));
                }
            }
            #[cfg(not(feature = "ibm"))]
            let _ = value;
        } else if lower.starts_with("connect_timeout=") {
            // PDO_ODBC does not implement PDO::ATTR_TIMEOUT; the common prelude
            // folds it for network drivers, so discard it before DriverConnect.
        } else if flavor.is_sqlsrv() && lower.starts_with("connectionpooling=") {
            // On Unix-like targets current PDO_SQLSRV ignores this DSN option and
            // lets the ODBC manager's ODBCINST.INI Pooling setting select pooling.
        } else if !part.is_empty() {
            source_parts.push(part);
        }
    }
    if !matches!(cursor_library, SQL_CUR_USE_IF_NEEDED | SQL_CUR_USE_ODBC | SQL_CUR_USE_DRIVER) {
        return Err("Pdo\\Odbc::ATTR_USE_CURSOR_LIBRARY must be a valid SQL_USE_* value".to_string());
    }
    Ok(OpenOptions {
        source: source_parts.join(";"),
        username,
        password,
        #[cfg(feature = "sqlsrv")]
        username_supplied,
        #[cfg(feature = "sqlsrv")]
        password_supplied,
        cursor_library,
        assume_utf8,
        auto_commit,
        #[cfg(feature = "sqlsrv")]
        sqlsrv_access_token,
        #[cfg(feature = "ibm")]
        ibm_attributes,
    })
}

/// Live ODBC environment/connection pair and PDO state.
pub struct OdbcConn {
    env: HEnv,
    owns_env: bool,
    dbc: HDbc,
    error: ErrorState,
    pub changes: i64,
    pub in_transaction: bool,
    auto_commit: bool,
    assume_utf8: bool,
    flavor: CliFlavor,
    last_insert_id: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_encoding: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_query_timeout: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_direct_query: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_client_buffer_kb: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_fetch_numeric: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_fetch_datetime: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_format_decimals: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_decimal_places: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_default_str_param: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_emulate_prepares: bool,
    #[cfg(feature = "sqlsrv")]
    _sqlsrv_access_token: Option<Vec<u32>>,
}

// The bridge serializes access through its global connection-table mutex.
unsafe impl Send for OdbcConn {}

impl Drop for OdbcConn {
    /// Disconnects and frees the ODBC handles in dependency order.
    fn drop(&mut self) {
        unsafe {
            if !self.dbc.0.is_null() {
                if self.in_transaction || !self.auto_commit {
                    let _ = SQLEndTran(HandleType::Dbc, self.dbc.as_handle(), CompletionType::Rollback);
                }
                let _ = SQLDisconnect(self.dbc);
                let _ = SQLFreeHandle(HandleType::Dbc, self.dbc.as_handle());
            }
            if self.owns_env && !self.env.0.is_null() {
                let _ = SQLFreeHandle(HandleType::Env, self.env.as_handle());
            }
        }
    }
}

impl OdbcConn {
    /// Opens a PDO_ODBC named data source or direct connection string.
    #[cfg(feature = "odbc")]
    pub fn open_odbc(dsn: &str) -> Result<Self, String> {
        Self::open(dsn, CliFlavor::Odbc)
    }

    /// Opens a PDO_INFORMIX named data source or direct CLI connection string.
    #[cfg(feature = "informix")]
    pub fn open_informix(dsn: &str) -> Result<Self, String> {
        Self::open(dsn, CliFlavor::Informix)
    }

    /// Opens a PDO_IBM named data source or direct IBM CLI connection string.
    #[cfg(feature = "ibm")]
    pub fn open_ibm(dsn: &str) -> Result<Self, String> {
        Self::open(dsn, CliFlavor::Ibm)
    }

    /// Opens a PDO_SQLSRV DSN through Microsoft ODBC Driver 18 or 17.
    #[cfg(feature = "sqlsrv")]
    pub fn open_sqlsrv(dsn: &str) -> Result<Self, String> {
        Self::open(dsn, CliFlavor::Sqlsrv)
    }

    /// Opens either CLI flavor while retaining its distinct PDO identity.
    fn open(dsn: &str, flavor: CliFlavor) -> Result<Self, String> {
        remember_open_error(&ErrorState {
            sqlstate: "HY000".to_string(),
            native_code: 0,
            message: "CLI connection initialization failed".to_string(),
        });
        let mut options = parse_open_options(dsn, flavor)?;
        #[cfg(feature = "sqlsrv")]
        let sqlsrv_token = if flavor.is_sqlsrv() {
            options.sqlsrv_access_token.take()
        } else {
            None
        };
        #[cfg(feature = "sqlsrv")]
        if let Some(token) = sqlsrv_token.as_deref() {
            let conflicting_source_option = split_connection_fields(&options.source)
                .iter()
                .filter_map(|field| field.split_once('=').map(|(key, _)| key.trim()))
                .any(|key| {
                    key.eq_ignore_ascii_case("uid")
                        || key.eq_ignore_ascii_case("pwd")
                        || key.eq_ignore_ascii_case("authentication")
                });
            if options.username_supplied
                || options.password_supplied
                || conflicting_source_option
            {
                return Err(
                    "AccessToken cannot be combined with username, password, or Authentication"
                        .to_string(),
                );
            }
            let fingerprint = sqlsrv_token_fingerprint(token);
            if !options.source.is_empty() && !options.source.ends_with(';') {
                options.source.push(';');
            }
            options
                .source
                .push_str(&format!("APP={{MSPHPSQL AT-{fingerprint:016x}}}"));
        }
        #[cfg(feature = "sqlsrv")]
        let mut sqlsrv_access_token = sqlsrv_token
            .as_deref()
            .map(sqlsrv_access_token_buffer);
        let (env, owns_env) = connection_environment(flavor)?;
        let mut dbc = Handle::null();
        if flavor.is_sqlsrv()
            && !split_connection_fields(&options.source).iter().any(|field| {
                field
                    .split_once('=')
                    .is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case("driver"))
            })
        {
            let Some(driver) = sql_server_driver(env.as_henv()) else {
                free_environment_if_owned(env, owns_env);
                return Err("Microsoft ODBC Driver 18 or 17 for SQL Server is not installed".to_string());
            };
            options.source = format!("Driver={{{driver}}};{}", options.source);
        }
        let allocated_dbc = unsafe { SQLAllocHandle(HandleType::Dbc, env, &mut dbc) };
        if !succeeded(allocated_dbc) {
            free_environment_if_owned(env, owns_env);
            return Err("SQLAllocHandle: DBC failed".to_string());
        }
        let dbc_handle = dbc.as_hdbc();
        #[cfg(feature = "sqlsrv")]
        if let Some(token) = sqlsrv_access_token.as_mut() {
            let result = unsafe {
                SQLSetConnectAttrRaw(
                    dbc_handle,
                    SQL_COPT_SS_ACCESS_TOKEN,
                    token.as_mut_ptr().cast(),
                    odbc_sys::IS_POINTER,
                )
            };
            if !succeeded(result) {
                let error = diagnostic(
                    HandleType::Dbc,
                    dbc,
                    "SQLSetConnectAttr SQL_COPT_SS_ACCESS_TOKEN",
                );
                remember_open_error(&error);
                unsafe {
                    let _ = SQLFreeHandle(HandleType::Dbc, dbc);
                }
                free_environment_if_owned(env, owns_env);
                return Err(error.message);
            }
        }
        #[cfg(feature = "sqlsrv")]
        if flavor.is_sqlsrv() {
            let _ = unsafe {
                SQLSetConnectAttrRaw(
                    dbc_handle,
                    SQL_COPT_SS_DATACLASSIFICATION_VERSION,
                    2isize as *mut c_void,
                    odbc_sys::IS_POINTER,
                )
            };
        }
        let set_autocommit = unsafe {
            SQLSetConnectAttr(
                dbc_handle,
                ConnectionAttribute::AUTOCOMMIT,
                (if options.auto_commit { SQL_AUTOCOMMIT_ON } else { SQL_AUTOCOMMIT_OFF }) as *mut c_void,
                odbc_sys::IS_INTEGER,
            )
        };
        if !succeeded(set_autocommit) {
            let error = diagnostic(HandleType::Dbc, dbc, "SQLSetConnectAttr AUTOCOMMIT");
            remember_open_error(&error);
            unsafe {
                let _ = SQLFreeHandle(HandleType::Dbc, dbc);
            }
            free_environment_if_owned(env, owns_env);
            return Err(error.message);
        }
        #[cfg(feature = "ibm")]
        if flavor == CliFlavor::Ibm {
            for (attribute, value) in &options.ibm_attributes {
                let Some(native_attribute) = ibm_native_connection_attribute(*attribute) else {
                    continue;
                };
                let result = if *attribute == PDO_IBM_ATTR_USE_TRUSTED_CONTEXT {
                    let enabled = (value != "0") as isize;
                    unsafe {
                        SQLSetConnectAttrRaw(
                            dbc_handle,
                            native_attribute,
                            enabled as *mut c_void,
                            odbc_sys::IS_INTEGER,
                        )
                    }
                } else {
                    unsafe {
                        SQLSetConnectAttrRaw(
                            dbc_handle,
                            native_attribute,
                            value.as_ptr().cast_mut().cast(),
                            value.len() as i32,
                        )
                    }
                };
                if !succeeded(result) {
                    let error = diagnostic(HandleType::Dbc, dbc, "SQLSetConnectAttr IBM");
                    remember_open_error(&error);
                    unsafe {
                        let _ = SQLFreeHandle(HandleType::Dbc, dbc);
                    }
                    free_environment_if_owned(env, owns_env);
                    return Err(error.message);
                }
            }
        }
        let cursor_result = unsafe {
            SQLSetConnectAttr(
                dbc_handle,
                ConnectionAttribute::ODBC_CURSORS,
                options.cursor_library as isize as *mut c_void,
                odbc_sys::IS_INTEGER,
            )
        };
        if !succeeded(cursor_result) && options.cursor_library != SQL_CUR_USE_IF_NEEDED {
            let error = diagnostic(HandleType::Dbc, dbc, "SQLSetConnectAttr SQL_ODBC_CURSORS");
            remember_open_error(&error);
            unsafe {
                let _ = SQLFreeHandle(HandleType::Dbc, dbc);
            }
            free_environment_if_owned(env, owns_env);
            return Err(error.message);
        }

        let direct = options.source.contains('=');
        let connect_result = if direct {
            let mut source = options.source.trim_end_matches(';').to_string();
            let lower = source.to_ascii_lowercase();
            if !options.username.is_empty() && !lower.contains("uid=") {
                source.push_str(";UID=");
                source.push_str(&quote_connection_value(&options.username));
            }
            if !options.password.is_empty() && !lower.contains("pwd=") {
                source.push_str(";PWD=");
                source.push_str(&quote_connection_value(&options.password));
            }
            if flavor.is_sqlsrv() {
                let source = source.encode_utf16().collect::<Vec<_>>();
                let mut completed = [0u16; 1024];
                let mut completed_len = 0i16;
                unsafe {
                    SQLDriverConnectW(
                        dbc_handle,
                        ptr::null_mut(),
                        source.as_ptr(),
                        source.len() as i16,
                        completed.as_mut_ptr(),
                        completed.len() as i16,
                        &mut completed_len,
                        DriverConnectOption::NoPrompt,
                    )
                }
            } else {
                let mut completed = [0u8; 1024];
                let mut completed_len = 0i16;
                unsafe {
                    SQLDriverConnect(
                        dbc_handle,
                        ptr::null_mut(),
                        source.as_ptr(),
                        source.len() as i16,
                        completed.as_mut_ptr(),
                        completed.len() as i16,
                        &mut completed_len,
                        DriverConnectOption::NoPrompt,
                    )
                }
            }
        } else {
            unsafe {
                SQLConnect(
                    dbc_handle,
                    options.source.as_ptr(),
                    options.source.len() as i16,
                    options.username.as_ptr(),
                    options.username.len() as i16,
                    options.password.as_ptr(),
                    options.password.len() as i16,
                )
            }
        };
        if !succeeded(connect_result) {
            let error = diagnostic(
                HandleType::Dbc,
                dbc,
                if direct { "SQLDriverConnect" } else { "SQLConnect" },
            );
            remember_open_error(&error);
            unsafe {
                let _ = SQLFreeHandle(HandleType::Dbc, dbc);
            }
            free_environment_if_owned(env, owns_env);
            return Err(error.message);
        }
        #[cfg(feature = "informix")]
        if flavor == CliFlavor::Informix {
            for (attribute, context) in [
                (SQL_INFX_ATTR_LO_AUTOMATIC, "SQL_INFX_ATTR_LO_AUTOMATIC"),
                (SQL_INFX_ATTR_ODBC_TYPES_ONLY, "SQL_INFX_ATTR_ODBC_TYPES_ONLY"),
            ] {
                let result = unsafe {
                    SQLSetConnectAttrRaw(
                        dbc_handle,
                        attribute,
                        1isize as *mut c_void,
                        odbc_sys::IS_INTEGER,
                    )
                };
                if !succeeded(result) {
                    let error = diagnostic(HandleType::Dbc, dbc, context);
                    remember_open_error(&error);
                    unsafe {
                        let _ = SQLDisconnect(dbc_handle);
                        let _ = SQLFreeHandle(HandleType::Dbc, dbc);
                    }
                    free_environment_if_owned(env, owns_env);
                    return Err(error.message);
                }
            }
        }
        Ok(Self {
            env: env.as_henv(),
            owns_env,
            dbc: dbc_handle,
            error: ErrorState::default(),
            changes: 0,
            in_transaction: false,
            auto_commit: options.auto_commit,
            assume_utf8: options.assume_utf8 || flavor.is_sqlsrv(),
            flavor,
            last_insert_id: 0,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_encoding: SQLSRV_ENCODING_UTF8,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_query_timeout: 0,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_direct_query: false,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_client_buffer_kb: 10_240,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_fetch_numeric: false,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_fetch_datetime: false,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_format_decimals: false,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_decimal_places: -1,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_default_str_param: 0x2000_0000,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_emulate_prepares: false,
            #[cfg(feature = "sqlsrv")]
            _sqlsrv_access_token: sqlsrv_access_token,
        })
    }

    /// Returns the PDO registry identity selected when the CLI connection opened.
    pub(crate) fn driver_kind(&self) -> crate::driver::DriverKind {
        match self.flavor {
            #[cfg(feature = "odbc")]
            CliFlavor::Odbc => crate::driver::DriverKind::Odbc,
            #[cfg(feature = "informix")]
            CliFlavor::Informix => crate::driver::DriverKind::Informix,
            #[cfg(feature = "ibm")]
            CliFlavor::Ibm => crate::driver::DriverKind::Ibm,
            #[cfg(feature = "sqlsrv")]
            CliFlavor::Sqlsrv => crate::driver::DriverKind::Sqlsrv,
        }
    }

    /// Reports whether the driver manager considers the connection alive.
    pub fn is_alive(&mut self) -> bool {
        let mut dead = 0u32;
        let result = unsafe {
            odbc_sys::SQLGetConnectAttr(
                self.dbc,
                ConnectionAttribute::CONNECTION_DEAD,
                (&mut dead as *mut u32).cast(),
                0,
                ptr::null_mut(),
            )
        };
        if succeeded(result) && dead != 0 {
            return false;
        }
        let mut read_only = [0u8; 32];
        let mut length = 0i16;
        let fallback = unsafe {
            SQLGetInfo(
                self.dbc,
                InfoType::DataSourceReadOnly,
                read_only.as_mut_ptr().cast(),
                read_only.len() as i16,
                &mut length,
            )
        };
        succeeded(fallback) && length > 0
    }

    /// Executes one direct statement and returns its affected-row count.
    pub fn exec(&mut self, sql: &str) -> i64 {
        let mut statement = Handle::null();
        if !succeeded(unsafe { SQLAllocHandle(HandleType::Stmt, self.dbc.as_handle(), &mut statement) }) {
            self.error = diagnostic(HandleType::Dbc, self.dbc.as_handle(), "SQLAllocHandle: STMT");
            return -1;
        }
        let statement_handle = statement.as_hstmt();
        let result = if self.is_sqlsrv() {
            let sql = sql.encode_utf16().collect::<Vec<_>>();
            unsafe { SQLExecDirectW(statement_handle, sql.as_ptr(), sql.len() as i32) }
        } else {
            unsafe { SQLExecDirect(statement_handle, sql.as_ptr(), sql.len() as i32) }
        };
        if result == SqlReturn::NO_DATA {
            self.changes = 0;
        } else if !succeeded(result) {
            self.error = diagnostic(HandleType::Stmt, statement, "SQLExecDirect");
            unsafe { let _ = SQLFreeHandle(HandleType::Stmt, statement); };
            return -1;
        } else {
            let mut count = -1;
            if succeeded(unsafe { SQLRowCount(statement_handle, &mut count) }) {
                self.changes = count.max(0) as i64;
            }
        }
        self.error = ErrorState::default();
        if self.is_ibm() {
            self.refresh_ibm_ids_last_insert_id(statement_handle);
        }
        unsafe { let _ = SQLFreeHandle(HandleType::Stmt, statement); };
        if self.is_informix() && sql.trim_start().to_ascii_lowercase().starts_with("insert") {
            self.refresh_informix_last_insert_id();
        }
        self.changes
    }

    /// Reports whether this shared CLI handle belongs to PDO_INFORMIX.
    fn is_informix(&self) -> bool {
        #[cfg(feature = "informix")]
        if self.flavor == CliFlavor::Informix {
            return true;
        }
        false
    }

    /// Reports whether this shared CLI handle belongs to PDO_IBM.
    fn is_ibm(&self) -> bool {
        #[cfg(feature = "ibm")]
        if self.flavor == CliFlavor::Ibm {
            return true;
        }
        false
    }

    /// Reports whether this shared CLI handle belongs to PDO_ODBC itself.
    fn is_odbc(&self) -> bool {
        #[cfg(feature = "odbc")]
        if self.flavor == CliFlavor::Odbc {
            return true;
        }
        false
    }

    /// Reports whether this shared CLI handle belongs to PDO_SQLSRV.
    fn is_sqlsrv(&self) -> bool {
        self.flavor.is_sqlsrv()
    }

    /// Reads Informix's most recent SERIAL value without changing PDO error state.
    fn refresh_informix_last_insert_id(&mut self) {
        let mut raw = Handle::null();
        if !succeeded(unsafe { SQLAllocHandle(HandleType::Stmt, self.dbc.as_handle(), &mut raw) }) {
            self.last_insert_id = 0;
            return;
        }
        let statement = raw.as_hstmt();
        let sql = b"SELECT DBINFO('sqlca.sqlerrd1') FROM systables WHERE tabid = 1";
        let executed = unsafe { SQLExecDirect(statement, sql.as_ptr(), sql.len() as i32) };
        let fetched = succeeded(executed) && succeeded(unsafe { SQLFetch(statement) });
        let mut buffer = [0u8; 64];
        let mut indicator = 0isize;
        let read = fetched
            && succeeded(unsafe {
                SQLGetData(
                    statement,
                    1,
                    CDataType::Char,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as isize,
                    &mut indicator,
                )
            });
        self.last_insert_id = if read && indicator != NULL_DATA {
            let length = usize::try_from(indicator)
                .unwrap_or(0)
                .min(buffer.len().saturating_sub(1));
            String::from_utf8_lossy(&buffer[..length])
                .trim()
                .parse()
                .unwrap_or(0)
        } else {
            0
        };
        unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
    }

    /// Reads PDO_IBM's IDS generated-value statement attribute before handle release.
    fn refresh_ibm_ids_last_insert_id(&mut self, statement: HStmt) {
        #[cfg(feature = "ibm")]
        {
            if !self.is_ibm() || !self.server_info().starts_with("IDS") {
                return;
            }
            let mut buffer = [0u8; 64];
            let result = unsafe {
                SQLGetStmtAttrRaw(
                    statement,
                    SQL_IBM_ATTR_GET_GENERATED_VALUE,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as i32,
                    ptr::null_mut(),
                )
            };
            if succeeded(result) {
                let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
                let value = String::from_utf8_lossy(&buffer[..end]).trim().parse().unwrap_or(0);
                if value != 0 {
                    self.last_insert_id = value;
                }
            }
        }
        #[cfg(not(feature = "ibm"))]
        let _ = statement;
    }

    /// Returns the driver-specific current identity or named SQL Server sequence value.
    pub fn last_insert_id(&mut self, name: Option<&str>) -> String {
        if self.is_informix() {
            return self.last_insert_id.to_string();
        }
        if self.is_ibm() {
            let server = self.server_info();
            if server.starts_with("DB2") {
                return self
                    .query_scalar_text("SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1")
                    .unwrap_or_default();
            }
            return self.last_insert_id.to_string();
        }
        if self.is_sqlsrv() {
            let sql = name.filter(|name| !name.is_empty()).map_or_else(
                || "SELECT @@IDENTITY;".to_string(),
                |name| {
                    let name = name.replace('\'', "''");
                    format!("SELECT current_value FROM sys.sequences WHERE name=N'{name}'")
                },
            );
            return self.query_scalar_text(&sql).unwrap_or_default();
        }
        String::new()
    }

    /// Executes one scalar CLI query for driver helper hooks such as last-insert-id.
    fn query_scalar_text(&mut self, sql: &str) -> Option<String> {
        let mut raw = Handle::null();
        if !succeeded(unsafe { SQLAllocHandle(HandleType::Stmt, self.dbc.as_handle(), &mut raw) }) {
            self.error = diagnostic(HandleType::Dbc, self.dbc.as_handle(), "SQLAllocHandle: STMT");
            return None;
        }
        let statement = raw.as_hstmt();
        let executed = if self.is_sqlsrv() {
            let sql = sql.encode_utf16().collect::<Vec<_>>();
            unsafe { SQLExecDirectW(statement, sql.as_ptr(), sql.len() as i32) }
        } else {
            unsafe { SQLExecDirect(statement, sql.as_ptr(), sql.len() as i32) }
        };
        if !succeeded(executed) || !succeeded(unsafe { SQLFetch(statement) }) {
            self.error = diagnostic(HandleType::Stmt, raw, "SQLExecDirect");
            unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
            return None;
        }
        let mut buffer = [0u8; 128];
        let mut indicator = 0isize;
        let read = unsafe {
            SQLGetData(
                statement,
                1,
                CDataType::Char,
                buffer.as_mut_ptr().cast(),
                buffer.len() as isize,
                &mut indicator,
            )
        };
        unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
        if !succeeded(read) || indicator == NULL_DATA {
            return Some("0".to_string());
        }
        let length = usize::try_from(indicator)
            .unwrap_or(0)
            .min(buffer.len().saturating_sub(1));
        Some(String::from_utf8_lossy(&buffer[..length]).trim().to_string())
    }

    /// Starts a manual transaction by disabling native autocommit when needed.
    pub fn begin(&mut self) -> bool {
        if self.in_transaction {
            return false;
        }
        if self.auto_commit && !self.set_native_autocommit(false) {
            return false;
        }
        self.in_transaction = true;
        true
    }

    /// Commits the active transaction and restores configured autocommit.
    pub fn commit(&mut self) -> bool {
        self.end_transaction(CompletionType::Commit)
    }

    /// Rolls back the active transaction and restores configured autocommit.
    pub fn rollback(&mut self) -> bool {
        self.end_transaction(CompletionType::Rollback)
    }

    /// Completes one transaction through the driver manager.
    fn end_transaction(&mut self, completion: CompletionType) -> bool {
        let result = unsafe { SQLEndTran(HandleType::Dbc, self.dbc.as_handle(), completion) };
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Dbc, self.dbc.as_handle(), "SQLEndTran");
            return false;
        }
        self.in_transaction = false;
        !self.auto_commit || self.set_native_autocommit(true)
    }

    /// Changes the driver-manager autocommit attribute.
    fn set_native_autocommit(&mut self, enabled: bool) -> bool {
        let result = unsafe {
            SQLSetConnectAttr(
                self.dbc,
                ConnectionAttribute::AUTOCOMMIT,
                (if enabled { SQL_AUTOCOMMIT_ON } else { SQL_AUTOCOMMIT_OFF }) as *mut c_void,
                odbc_sys::IS_INTEGER,
            )
        };
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Dbc, self.dbc.as_handle(), "SQLSetConnectAttr AUTOCOMMIT");
            return false;
        }
        true
    }

    /// Updates PDO_ODBC's writable connection attributes.
    pub fn set_attribute(&mut self, attribute: i64, value: i64) -> bool {
        #[cfg(feature = "sqlsrv")]
        if self.is_sqlsrv() {
            return self.set_sqlsrv_attribute(attribute, value);
        }
        match attribute {
            0 if !self.in_transaction => {
                let enabled = value != 0;
                if enabled == self.auto_commit || self.set_native_autocommit(enabled) {
                    self.auto_commit = enabled;
                    true
                } else {
                    false
                }
            }
            1001 => {
                self.assume_utf8 = value != 0;
                true
            }
            _ => false,
        }
    }

    /// Reads PDO_ODBC's boolean connection attributes.
    pub fn attribute(&self, attribute: i64) -> Option<i64> {
        #[cfg(feature = "sqlsrv")]
        if self.is_sqlsrv() {
            return self.sqlsrv_attribute(attribute);
        }
        match attribute {
            0 => Some(self.auto_commit as i64),
            1001 => Some(self.assume_utf8 as i64),
            _ => None,
        }
    }

    /// Applies one PDO_SQLSRV connection attribute with upstream validation.
    #[cfg(feature = "sqlsrv")]
    fn set_sqlsrv_attribute(&mut self, attribute: i64, value: i64) -> bool {
        let accepted = match attribute {
            SQLSRV_ATTR_ENCODING => match value {
                SQLSRV_ENCODING_DEFAULT => {
                    self.sqlsrv_encoding = SQLSRV_ENCODING_UTF8;
                    true
                }
                SQLSRV_ENCODING_SYSTEM | SQLSRV_ENCODING_UTF8 => {
                    self.sqlsrv_encoding = value;
                    true
                }
                _ => false,
            },
            SQLSRV_ATTR_QUERY_TIMEOUT if value >= 0 => {
                self.sqlsrv_query_timeout = value;
                true
            }
            SQLSRV_ATTR_DIRECT_QUERY => {
                self.sqlsrv_direct_query = value != 0;
                true
            }
            SQLSRV_ATTR_CLIENT_BUFFER_MAX_KB_SIZE if value > 0 => {
                self.sqlsrv_client_buffer_kb = value;
                true
            }
            SQLSRV_ATTR_FETCHES_NUMERIC_TYPE => {
                self.sqlsrv_fetch_numeric = value != 0;
                true
            }
            SQLSRV_ATTR_FETCHES_DATETIME_TYPE => {
                self.sqlsrv_fetch_datetime = value != 0;
                true
            }
            SQLSRV_ATTR_FORMAT_DECIMALS => {
                self.sqlsrv_format_decimals = value != 0;
                true
            }
            SQLSRV_ATTR_DECIMAL_PLACES => {
                self.sqlsrv_decimal_places = if (0..=4).contains(&value) { value } else { -1 };
                true
            }
            17 => true,
            20 => {
                self.sqlsrv_emulate_prepares = value != 0;
                true
            }
            21 if matches!(value, 0x2000_0000 | 0x4000_0000) => {
                self.sqlsrv_default_str_param = value;
                true
            }
            _ => false,
        };
        if accepted {
            self.error = ErrorState::default();
        }
        accepted
    }

    /// Reads one PDO_SQLSRV connection attribute supported by the upstream hook.
    #[cfg(feature = "sqlsrv")]
    fn sqlsrv_attribute(&self, attribute: i64) -> Option<i64> {
        match attribute {
            SQLSRV_ATTR_ENCODING => Some(self.sqlsrv_encoding),
            SQLSRV_ATTR_QUERY_TIMEOUT => Some(self.sqlsrv_query_timeout),
            SQLSRV_ATTR_DIRECT_QUERY => Some(self.sqlsrv_direct_query as i64),
            SQLSRV_ATTR_CLIENT_BUFFER_MAX_KB_SIZE => Some(self.sqlsrv_client_buffer_kb),
            SQLSRV_ATTR_FETCHES_NUMERIC_TYPE => Some(self.sqlsrv_fetch_numeric as i64),
            SQLSRV_ATTR_FETCHES_DATETIME_TYPE => Some(self.sqlsrv_fetch_datetime as i64),
            SQLSRV_ATTR_FORMAT_DECIMALS => Some(self.sqlsrv_format_decimals as i64),
            SQLSRV_ATTR_DECIMAL_PLACES => Some(self.sqlsrv_decimal_places),
            20 => Some(self.sqlsrv_emulate_prepares as i64),
            21 => Some(self.sqlsrv_default_str_param),
            _ => None,
        }
    }

    /// Writes one PDO_IBM string-valued CLI connection attribute.
    #[cfg(feature = "ibm")]
    pub fn set_ibm_attribute_text(&mut self, attribute: i64, value: &str) -> bool {
        let Ok(attribute) = i32::try_from(attribute) else {
            return false;
        };
        if !matches!(
            attribute,
            PDO_IBM_ATTR_INFO_USERID
                | PDO_IBM_ATTR_INFO_ACCTSTR
                | PDO_IBM_ATTR_INFO_APPLNAME
                | PDO_IBM_ATTR_INFO_WRKSTNNAME
                | PDO_IBM_ATTR_TRUSTED_CONTEXT_USERID
                | PDO_IBM_ATTR_TRUSTED_CONTEXT_PASSWORD
        ) {
            return false;
        }
        let native_attribute = ibm_native_connection_attribute(attribute)
            .expect("validated PDO_IBM attribute must have a native CLI mapping");
        let result = unsafe {
            SQLSetConnectAttrRaw(
                self.dbc,
                native_attribute,
                value.as_ptr().cast_mut().cast(),
                value.len() as i32,
            )
        };
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Dbc, self.dbc.as_handle(), "SQLSetConnectAttr IBM");
            return false;
        }
        self.error = ErrorState::default();
        true
    }

    /// Reads one PDO_IBM string-valued CLI connection attribute.
    #[cfg(feature = "ibm")]
    pub fn ibm_attribute_text(&mut self, attribute: i64) -> Option<String> {
        let attribute = i32::try_from(attribute).ok()?;
        if !matches!(
            attribute,
            PDO_IBM_ATTR_INFO_USERID
                | PDO_IBM_ATTR_INFO_ACCTSTR
                | PDO_IBM_ATTR_INFO_APPLNAME
                | PDO_IBM_ATTR_INFO_WRKSTNNAME
                | PDO_IBM_ATTR_TRUSTED_CONTEXT_USERID
        ) {
            return None;
        }
        let native_attribute = ibm_native_connection_attribute(attribute)
            .expect("validated PDO_IBM attribute must have a native CLI mapping");
        let mut buffer = [0u8; 256];
        let mut length = 0i32;
        let result = unsafe {
            SQLGetConnectAttrRaw(
                self.dbc,
                native_attribute,
                buffer.as_mut_ptr().cast(),
                buffer.len() as i32,
                &mut length,
            )
        };
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Dbc, self.dbc.as_handle(), "SQLGetConnectAttr IBM");
            return None;
        }
        self.error = ErrorState::default();
        let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
        Some(String::from_utf8_lossy(&buffer[..length]).into_owned())
    }

    /// Reads PDO_IBM's trusted-context enablement flag.
    #[cfg(feature = "ibm")]
    pub fn ibm_attribute_int(&mut self, attribute: i64) -> Option<i64> {
        let attribute = i32::try_from(attribute).ok()?;
        if attribute != PDO_IBM_ATTR_USE_TRUSTED_CONTEXT {
            return None;
        }
        let native_attribute = ibm_native_connection_attribute(attribute)
            .expect("validated PDO_IBM attribute must have a native CLI mapping");
        let mut value = 0i32;
        let mut length = 0i32;
        let result = unsafe {
            SQLGetConnectAttrRaw(
                self.dbc,
                native_attribute,
                (&mut value as *mut i32).cast(),
                std::mem::size_of::<i32>() as i32,
                &mut length,
            )
        };
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Dbc, self.dbc.as_handle(), "SQLGetConnectAttr IBM");
            return None;
        }
        self.error = ErrorState::default();
        Some((value != 0) as i64)
    }

    /// Reads one textual SQLGetInfo field.
    pub fn info(&mut self, info_type: InfoType) -> String {
        let mut buffer = [0u8; 256];
        let mut length = 0i16;
        let result = unsafe {
            SQLGetInfo(
                self.dbc,
                info_type,
                buffer.as_mut_ptr().cast(),
                buffer.len() as i16,
                &mut length,
            )
        };
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Dbc, self.dbc.as_handle(), "SQLGetInfo");
            return String::new();
        }
        String::from_utf8_lossy(&buffer[..usize::try_from(length).unwrap_or(0).min(buffer.len())])
            .into_owned()
    }

    /// Returns the connected DBMS version exposed by `PDO::ATTR_SERVER_VERSION`.
    pub fn server_version(&mut self) -> String {
        self.info(InfoType::DbmsVer)
    }

    /// Returns the extension version string reported by the selected PDO driver.
    pub fn client_version(&self) -> String {
        match self.flavor {
            #[cfg(feature = "odbc")]
            CliFlavor::Odbc => "ODBC-unixODBC".to_string(),
            #[cfg(feature = "informix")]
            CliFlavor::Informix => "1.3.7".to_string(),
            #[cfg(feature = "ibm")]
            CliFlavor::Ibm => "1.7.0".to_string(),
            #[cfg(feature = "sqlsrv")]
            CliFlavor::Sqlsrv => "5.13.1".to_string(),
        }
    }

    /// Returns the connected DBMS name exposed by `PDO::ATTR_SERVER_INFO`.
    pub fn server_info(&mut self) -> String {
        self.info(InfoType::DbmsName)
    }

    /// Returns one PDO_SQLSRV client/server information array field.
    #[cfg(feature = "sqlsrv")]
    pub fn sqlsrv_info(&mut self, field: i64) -> String {
        if !self.is_sqlsrv() {
            return String::new();
        }
        match field {
            0 => self.query_scalar_text("SELECT DB_NAME()").unwrap_or_default(),
            1 => self.info(InfoType::DbmsVer),
            2 => self.info(InfoType::ServerName),
            3 => self.info(InfoType::DriverName),
            4 => self.info(InfoType::DriverOdbcVer),
            5 => self.info(InfoType::DriverVer),
            _ => String::new(),
        }
    }

    /// Returns the current connection SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        &self.error.sqlstate
    }

    /// Returns the current native ODBC error code.
    pub fn errcode(&self) -> i64 {
        self.error.native_code
    }

    /// Returns the current ODBC diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }
}

/// One bound ODBC input value.
#[derive(Clone)]
enum OdbcBind {
    Null,
    Int(i64),
    Double(f64),
    Text(Vec<u8>),
    Binary(Vec<u8>),
}

/// Selects PDO_SQLSRV's native floating-point ODBC types while preserving numeric descriptors.
#[cfg(feature = "sqlsrv")]
fn sqlsrv_double_parameter_types(
    flavor: CliFlavor,
    bind: &OdbcBind,
    described: SqlDataType,
) -> Option<(CDataType, SqlDataType)> {
    if !flavor.is_sqlsrv() || !matches!(bind, OdbcBind::Double(_)) {
        return None;
    }
    let sql_type = if matches!(
        described,
        SqlDataType::NUMERIC
            | SqlDataType::DECIMAL
            | SqlDataType::FLOAT
            | SqlDataType::REAL
            | SqlDataType::DOUBLE
    ) {
        described
    } else {
        SqlDataType::FLOAT
    };
    Some((CDataType::Double, sql_type))
}

/// Derives SQLSRV parameter defaults from the PHP value when Always Encrypted metadata is absent.
#[cfg(feature = "sqlsrv")]
fn sqlsrv_parameter_defaults(bind: &OdbcBind, encoding: i64) -> (SqlDataType, usize, i16) {
    let wide = encoding != SQLSRV_ENCODING_BINARY && encoding != SQLSRV_ENCODING_SYSTEM;
    match bind {
        OdbcBind::Null if encoding == SQLSRV_ENCODING_BINARY => {
            (SqlDataType::EXT_BINARY, 1, 0)
        }
        OdbcBind::Null => (SqlDataType::VARCHAR, 0, 0),
        OdbcBind::Int(value) if i32::try_from(*value).is_err() => {
            (SqlDataType::EXT_BIG_INT, 0, 0)
        }
        OdbcBind::Int(_) => (SqlDataType::INTEGER, 0, 0),
        OdbcBind::Double(_) => (SqlDataType::FLOAT, 0, 0),
        OdbcBind::Text(value) if wide => {
            let column_size = if value.len().saturating_mul(2) > 8000 { 0 } else { 4000 };
            (SqlDataType::EXT_W_VARCHAR, column_size, 0)
        }
        OdbcBind::Text(value) => {
            let column_size = if value.len() > 8000 { 0 } else { 8000 };
            (SqlDataType::VARCHAR, column_size, 0)
        }
        OdbcBind::Binary(value) => {
            let column_size = if value.len() > 8000 { 0 } else { 8000 };
            (SqlDataType::EXT_VAR_BINARY, column_size, 0)
        }
    }
}

/// Maps PDO_SQLSRV cursor constants to the native ODBC cursor used by Microsoft’s driver.
#[cfg(feature = "sqlsrv")]
fn sqlsrv_native_cursor_type(cursor_type: i64) -> Option<i64> {
    match cursor_type {
        1..=3 => Some(cursor_type),
        42 => Some(0),
        _ => None,
    }
}

/// Registration metadata for one CLI input/output parameter.
#[derive(Clone, Copy)]
struct OutputSpec {
    max_length: i64,
    input_output: bool,
    lob: bool,
}

/// Bounds an output buffer to PDO's declared maximum and returns its input length.
fn prepare_output_buffer(payload: &mut Vec<u8>, output: OutputSpec, precision: usize) -> usize {
    let input_length = payload.len();
    let capacity = if output.max_length > 0 {
        usize::try_from(output.max_length).unwrap_or(usize::MAX)
    } else {
        precision.max(1)
    };
    payload.truncate(capacity);
    let input_length = input_length.min(capacity);
    payload.resize(capacity, 0);
    input_length
}

/// Renders a SQLSRV emulated-prepare statement using T-SQL literals.
#[cfg(feature = "sqlsrv")]
fn interpolate_sqlsrv(
    sql: &str,
    order: &[i64],
    binds: &[OdbcBind],
    national_strings: bool,
) -> Result<String, String> {
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
        if matches!(ch, '\'' | '"' | '[') {
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
        render_sqlsrv_bind(
            &mut output,
            binds.get(slot).ok_or_else(|| "Invalid parameter number".to_string())?,
            national_strings,
        );
        marker += 1;
    }
    if marker != order.len() {
        return Err("Invalid parameter number".to_string());
    }
    Ok(output)
}

/// Appends one ODBC bind as the literal syntax used by PDO_SQLSRV emulation.
#[cfg(feature = "sqlsrv")]
fn render_sqlsrv_bind(output: &mut String, value: &OdbcBind, national_strings: bool) {
    match value {
        OdbcBind::Null => output.push_str("NULL"),
        OdbcBind::Int(value) => output.push_str(&value.to_string()),
        OdbcBind::Double(value) if value.is_finite() => output.push_str(&value.to_string()),
        OdbcBind::Double(_) => output.push_str("NULL"),
        OdbcBind::Text(bytes) => {
            if national_strings {
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
        OdbcBind::Binary(bytes) => {
            output.push_str("0x");
            for byte in bytes {
                use std::fmt::Write;
                let _ = write!(output, "{byte:02X}");
            }
        }
    }
}

/// Applies PDO_SQLSRV's decimal leading-zero and money scale formatting.
#[cfg(feature = "sqlsrv")]
fn format_sqlsrv_decimal(
    value: Option<Vec<u8>>,
    native_type: &str,
    format_decimals: bool,
    decimal_places: i64,
) -> Option<Vec<u8>> {
    let decimal_type = native_type.to_ascii_lowercase();
    let money = matches!(decimal_type.as_str(), "money" | "smallmoney");
    let decimal = money || matches!(decimal_type.as_str(), "decimal" | "numeric");
    if (!format_decimals || !decimal) && (decimal_places < 0 || !money) {
        return value;
    }
    let bytes = value?;
    let mut text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => return Some(error.into_bytes()),
    };
    if format_decimals {
        if text.starts_with('.') {
            text.insert(0, '0');
        } else if text.starts_with("-.") {
            text.insert(1, '0');
        }
    }
    if decimal_places >= 0
        && money
    {
        if let Ok(number) = text.parse::<f64>() {
            text = format!("{number:.precision$}", precision = decimal_places as usize);
        }
    }
    Some(text.into_bytes())
}

/// Reads one native-endian `u16` from Microsoft's classification blob.
#[cfg(feature = "sqlsrv")]
fn classification_u16(blob: &[u8], offset: &mut usize) -> Result<u16, String> {
    let end = offset.saturating_add(2);
    let bytes: [u8; 2] = blob
        .get(*offset..end)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| "Truncated SQL Server data-classification metadata".to_string())?;
    *offset = end;
    Ok(u16::from_ne_bytes(bytes))
}

/// Reads one native-endian `i32` from Microsoft's classification blob.
#[cfg(feature = "sqlsrv")]
fn classification_i32(blob: &[u8], offset: &mut usize) -> Result<i32, String> {
    let end = offset.saturating_add(4);
    let bytes: [u8; 4] = blob
        .get(*offset..end)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| "Truncated SQL Server data-classification metadata".to_string())?;
    *offset = end;
    Ok(i32::from_ne_bytes(bytes))
}

/// Reads one length-prefixed UTF-16 name or identifier from the classification blob.
#[cfg(feature = "sqlsrv")]
fn classification_utf16(blob: &[u8], offset: &mut usize) -> Result<String, String> {
    let units = usize::from(
        *blob
            .get(*offset)
            .ok_or_else(|| "Truncated SQL Server data-classification metadata".to_string())?,
    );
    *offset = offset.saturating_add(1);
    let byte_len = units.saturating_mul(2);
    let end = offset.saturating_add(byte_len);
    let bytes = blob
        .get(*offset..end)
        .ok_or_else(|| "Truncated SQL Server data-classification metadata".to_string())?;
    let utf16 = bytes
        .chunks_exact(2)
        .map(|unit| u16::from_ne_bytes([unit[0], unit[1]]))
        .collect::<Vec<_>>();
    *offset = end;
    Ok(String::from_utf16_lossy(&utf16))
}

/// Parses the ODBC Driver 17.2+ sensitivity-label blob into PDO-facing columns.
#[cfg(feature = "sqlsrv")]
fn parse_sqlsrv_classification_blob(
    blob: &[u8],
    rank_available: bool,
) -> Result<SqlsrvClassification, String> {
    let mut offset = 0usize;
    let label_count = usize::from(classification_u16(blob, &mut offset)?);
    let mut labels = Vec::with_capacity(label_count);
    for _ in 0..label_count {
        labels.push((
            classification_utf16(blob, &mut offset)?,
            classification_utf16(blob, &mut offset)?,
        ));
    }
    let info_count = usize::from(classification_u16(blob, &mut offset)?);
    let mut information_types = Vec::with_capacity(info_count);
    for _ in 0..info_count {
        information_types.push((
            classification_utf16(blob, &mut offset)?,
            classification_utf16(blob, &mut offset)?,
        ));
    }
    let query_rank = if rank_available {
        Some(classification_i32(blob, &mut offset)?)
    } else {
        None
    };
    let column_count = usize::from(classification_u16(blob, &mut offset)?);
    let mut columns = Vec::with_capacity(column_count);
    for _ in 0..column_count {
        let pair_count = usize::from(classification_u16(blob, &mut offset)?);
        let mut pairs = Vec::with_capacity(pair_count);
        for _ in 0..pair_count {
            let label_index = usize::from(classification_u16(blob, &mut offset)?);
            let information_index = usize::from(classification_u16(blob, &mut offset)?);
            let rank = if rank_available {
                Some(classification_i32(blob, &mut offset)?)
            } else {
                None
            };
            let (label_name, label_id) = labels
                .get(label_index)
                .cloned()
                .ok_or_else(|| "Invalid SQL Server sensitivity-label index".to_string())?;
            let (information_name, information_id) = information_types
                .get(information_index)
                .cloned()
                .ok_or_else(|| "Invalid SQL Server information-type index".to_string())?;
            pairs.push(SqlsrvClassificationPair {
                label_name,
                label_id,
                information_name,
                information_id,
                rank,
            });
        }
        columns.push(pairs);
    }
    if offset != blob.len() {
        return Err("Unexpected trailing SQL Server data-classification metadata".to_string());
    }
    Ok(SqlsrvClassification {
        query_rank,
        columns,
    })
}

/// Reads one text descriptor attribute without making unsupported metadata fatal.
fn column_text_attribute(statement: HStmt, column: u16, attribute: Desc) -> Option<String> {
    let mut buffer = [0u8; 256];
    let mut length = 0i16;
    let mut numeric = 0isize;
    let result = unsafe {
        SQLColAttribute(
            statement,
            column,
            attribute,
            buffer.as_mut_ptr().cast(),
            buffer.len() as i16,
            &mut length,
            &mut numeric,
        )
    };
    if !succeeded(result) {
        return None;
    }
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    Some(String::from_utf8_lossy(&buffer[..length]).into_owned())
}

/// Reads one UTF-16 descriptor attribute for PDO_SQLSRV Unicode metadata.
#[cfg(feature = "sqlsrv")]
fn column_text_attribute_w(statement: HStmt, column: u16, attribute: Desc) -> Option<String> {
    let mut buffer = [0u16; 256];
    let mut length = 0i16;
    let mut numeric = 0isize;
    let result = unsafe {
        SQLColAttributeW(
            statement,
            column,
            attribute,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * std::mem::size_of::<u16>()) as i16,
            &mut length,
            &mut numeric,
        )
    };
    if !succeeded(result) {
        return None;
    }
    let units = usize::try_from(length)
        .unwrap_or(0)
        .checked_div(std::mem::size_of::<u16>())
        .unwrap_or(0)
        .min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..units]))
}

/// Reads one numeric descriptor attribute without making unsupported metadata fatal.
fn column_numeric_attribute(statement: HStmt, column: u16, attribute: Desc) -> Option<isize> {
    let mut length = 0i16;
    let mut numeric = 0isize;
    let result = unsafe {
        SQLColAttribute(
            statement,
            column,
            attribute,
            ptr::null_mut(),
            0,
            &mut length,
            &mut numeric,
        )
    };
    succeeded(result).then_some(numeric)
}

/// Identifies the two Informix UDT names that PECL maps to `PDO::PARAM_LOB` metadata.
fn informix_metadata_is_lob(native_type: &str) -> bool {
    let native_type = native_type.to_ascii_uppercase();
    native_type == "BLOB"
        || native_type == "CLOB"
        || native_type.ends_with("_UDT_BLOB")
        || native_type.ends_with("_UDT_CLOB")
}

/// Reproduces PDO_IBM 1.7.0's metadata switch, including BOOLEAN/BIT fallthrough.
#[cfg(feature = "ibm")]
fn ibm_metadata_is_lob(data_type: i16) -> bool {
    matches!(data_type, -7 | 16 | -2 | -3 | -4 | -98 | -99 | -370)
}

/// Completed CLI output value copied before its native execution buffer expires.
#[derive(Clone)]
pub(crate) struct OdbcOutputValue {
    pub(crate) data: Option<Vec<u8>>,
    pub(crate) lob: bool,
    pub(crate) numeric: bool,
}

/// One materialized ODBC result column.
struct OdbcColumn {
    name: String,
    wide: bool,
    lob: bool,
    metadata_pdo_lob: bool,
    len: i64,
    precision: i64,
    scale: i64,
    table: String,
    native_type: String,
    flags: i64,
    #[cfg(feature = "sqlsrv")]
    data_type: i16,
}

/// One label/information-type pair returned for a classified result column.
#[cfg(feature = "sqlsrv")]
struct SqlsrvClassificationPair {
    label_name: String,
    label_id: String,
    information_name: String,
    information_id: String,
    rank: Option<i32>,
}

/// Parsed SQLSRV sensitivity metadata shared by `getColumnMeta()` calls.
#[cfg(feature = "sqlsrv")]
struct SqlsrvClassification {
    query_rank: Option<i32>,
    columns: Vec<Vec<SqlsrvClassificationPair>>,
}

/// Prepared ODBC statement retaining its native handle across result sets.
pub struct OdbcStmt {
    pub conn_id: i64,
    flavor: CliFlavor,
    stmt: HStmt,
    named_map: HashMap<String, i64>,
    order: Vec<i64>,
    binds: Vec<OdbcBind>,
    bound: Vec<bool>,
    indicators: Vec<odbc_sys::Len>,
    output_specs: Vec<Option<OutputSpec>>,
    output_values: Vec<Option<OdbcOutputValue>>,
    columns: Vec<OdbcColumn>,
    rows: Vec<Vec<Option<Vec<u8>>>>,
    cursor: isize,
    executed: bool,
    row_count: i64,
    assume_utf8: bool,
    pub sent_sql: String,
    error: ErrorState,
    is_insert: bool,
    #[cfg(feature = "sqlsrv")]
    translated_sql: String,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_direct_query: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_emulated: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_encoding: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_query_timeout: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_cursor_type: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_client_buffer_kb: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_fetch_numeric: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_fetch_datetime: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_format_decimals: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_decimal_places: i64,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_data_classification: bool,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_classification: Option<SqlsrvClassification>,
    #[cfg(feature = "sqlsrv")]
    sqlsrv_classification_error: Option<ErrorState>,
}

unsafe impl Send for OdbcStmt {}

impl Drop for OdbcStmt {
    /// Closes and frees the native statement handle.
    fn drop(&mut self) {
        unsafe {
            let _ = SQLCloseCursor(self.stmt);
            let _ = SQLFreeHandle(HandleType::Stmt, self.stmt.as_handle());
        }
    }
}

impl OdbcStmt {
    /// Allocates and prepares an ODBC statement with PDO placeholder normalization.
    pub fn new(
        connection: &mut OdbcConn,
        conn_id: i64,
        sql: &str,
        mode: i64,
    ) -> Result<Self, String> {
        let (translated, named_map, order, mixed) = crate::my::translate_pdo_placeholders(sql);
        if mixed {
            return Err("Invalid parameter number: mixed named and positional parameters".to_string());
        }
        let mut raw = Handle::null();
        if !succeeded(unsafe { SQLAllocHandle(HandleType::Stmt, connection.dbc.as_handle(), &mut raw) }) {
            connection.error = diagnostic(HandleType::Dbc, connection.dbc.as_handle(), "SQLAllocHandle: STMT");
            return Err(connection.error.message.clone());
        }
        let stmt = raw.as_hstmt();
        #[cfg(feature = "sqlsrv")]
        let sqlsrv_cursor_type = if connection.is_sqlsrv() {
            let requested = (mode >> 8) & 0xff;
            if requested != 0
                && ((mode & 2) == 0 || sqlsrv_native_cursor_type(requested).is_none())
            {
                let message = "An invalid SQLSRV cursor option was designated.".to_string();
                unsafe {
                    let _ = SQLFreeHandle(HandleType::Stmt, raw);
                }
                connection.error = ErrorState {
                    sqlstate: "IMSSP".to_string(),
                    native_code: 0,
                    message: message.clone(),
                };
                return Err(message);
            }
            if requested != 0 {
                requested
            } else if (mode & 2) != 0 {
                3
            } else {
                0
            }
        } else {
            0
        };
        #[cfg(feature = "sqlsrv")]
        let sqlsrv_native_cursor = connection
            .is_sqlsrv()
            .then(|| sqlsrv_native_cursor_type(sqlsrv_cursor_type))
            .flatten();
        #[cfg(not(feature = "sqlsrv"))]
        let sqlsrv_native_cursor: Option<i64> = None;
        if (mode & 2) != 0 {
            let (attribute, value) = if let Some(cursor_type) = sqlsrv_native_cursor {
                (StatementAttribute::CursorType, cursor_type as isize)
            } else {
                (StatementAttribute::CursorScrollable, 1isize)
            };
            let configured = unsafe {
                SQLSetStmtAttr(stmt, attribute, value as *mut c_void, 0)
            };
            if !succeeded(configured) {
                let error = diagnostic(HandleType::Stmt, raw, "SQLSetStmtAttr: scrollable cursor");
                unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
                connection.error = error.clone();
                return Err(error.message);
            }
        }
        #[cfg(feature = "sqlsrv")]
        let direct_sqlsrv = connection.is_sqlsrv()
            && (connection.sqlsrv_direct_query || (mode & 4) != 0);
        #[cfg(not(feature = "sqlsrv"))]
        let direct_sqlsrv = false;
        if !direct_sqlsrv {
            let prepared = if connection.is_sqlsrv() {
                let wide = translated.encode_utf16().collect::<Vec<_>>();
                unsafe { SQLPrepareW(stmt, wide.as_ptr(), wide.len() as i32) }
            } else {
                unsafe { SQLPrepare(stmt, translated.as_ptr(), translated.len() as i32) }
            };
            if !succeeded(prepared) {
                let error = diagnostic(HandleType::Stmt, raw, "SQLPrepare");
                unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
                connection.error = error.clone();
                return Err(error.message);
            }
        }
        let slots = order.iter().copied().max().unwrap_or(0).max(0) as usize;
        let mut native_params = 0i16;
        if !direct_sqlsrv
            && succeeded(unsafe { SQLNumParams(stmt, &mut native_params) })
            && native_params as usize != order.len()
        {
            unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
            return Err("Invalid parameter number: number of bound variables does not match number of tokens".to_string());
        }
        Ok(Self {
            conn_id,
            flavor: connection.flavor,
            stmt,
            named_map,
            order,
            binds: vec![OdbcBind::Null; slots],
            bound: vec![false; slots],
            indicators: vec![NULL_DATA; slots],
            output_specs: vec![None; slots],
            output_values: vec![None; slots],
            columns: Vec::new(),
            rows: Vec::new(),
            cursor: -1,
            executed: false,
            row_count: 0,
            assume_utf8: connection.assume_utf8,
            sent_sql: String::new(),
            error: ErrorState::default(),
            is_insert: translated
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("insert"),
            #[cfg(feature = "sqlsrv")]
            translated_sql: translated,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_direct_query: direct_sqlsrv,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_emulated: connection.sqlsrv_emulate_prepares || (mode & 1) != 0,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_encoding: SQLSRV_ENCODING_DEFAULT,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_query_timeout: connection.sqlsrv_query_timeout,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_cursor_type,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_client_buffer_kb: connection.sqlsrv_client_buffer_kb,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_fetch_numeric: connection.sqlsrv_fetch_numeric,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_fetch_datetime: connection.sqlsrv_fetch_datetime,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_format_decimals: connection.sqlsrv_format_decimals,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_decimal_places: connection.sqlsrv_decimal_places,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_data_classification: false,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_classification: None,
            #[cfg(feature = "sqlsrv")]
            sqlsrv_classification_error: None,
        })
    }

    /// Resolves a named placeholder to its one-based PDO slot.
    pub fn parameter_index(&self, name: &str) -> i64 {
        self.named_map.get(name.trim_start_matches(':')).copied().unwrap_or(-1)
    }

    /// Stores one bind value in a one-based slot.
    fn bind(&mut self, index: i64, value: OdbcBind) -> bool {
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

    /// Binds one integer value.
    pub fn bind_int(&mut self, index: i64, value: i64) -> bool {
        self.bind(index, OdbcBind::Int(value))
    }

    /// Binds one floating-point value.
    pub fn bind_double(&mut self, index: i64, value: f64) -> bool {
        self.bind(index, OdbcBind::Double(value))
    }

    /// Binds one text value.
    pub fn bind_text(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.bind(index, OdbcBind::Text(value))
    }

    /// Binds one binary value.
    pub fn bind_blob(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.bind(index, OdbcBind::Binary(value))
    }

    /// Binds SQL NULL.
    pub fn bind_null(&mut self, index: i64) -> bool {
        self.bind(index, OdbcBind::Null)
    }

    /// Registers an input/output buffer for a scalar CLI parameter.
    pub fn bind_output(&mut self, index: i64, pdo_type: i64, max_length: i64) -> i64 {
        let Some(slot) = usize::try_from(index).ok().and_then(|index| index.checked_sub(1)) else {
            return 0;
        };
        if slot >= self.output_specs.len() {
            return 0;
        }
        let base_type = pdo_type & 0xFFFF;
        // PDO_INFORMIX explicitly forces LOB parameters back to input-only.
        if base_type == 3 && !self.flavor.is_sqlsrv() {
            #[cfg(feature = "odbc")]
            if self.flavor == CliFlavor::Odbc {
                self.error = ErrorState {
                    sqlstate: "HY000".to_string(),
                    native_code: 0,
                    message: "Can't bind a lob for output".to_string(),
                };
                return -1;
            }
            self.output_specs[slot] = None;
            return 1;
        }
        self.output_specs[slot] = Some(OutputSpec {
            max_length,
            input_output: (pdo_type & 0x8000_0000) != 0,
            lob: base_type == 3,
        });
        1
    }

    /// Returns a completed scalar output parameter, if the slot was output-bound.
    pub fn output_value(&self, index: i64) -> Option<&OdbcOutputValue> {
        usize::try_from(index)
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|slot| self.output_values.get(slot))
            .and_then(Option::as_ref)
    }

    /// Resets execution/cursor state while preserving binds.
    pub fn reset(&mut self) {
        unsafe { let _ = SQLCloseCursor(self.stmt); };
        self.columns.clear();
        self.rows.clear();
        self.cursor = -1;
        self.executed = false;
        self.row_count = 0;
        self.output_values.fill(None);
    }

    /// Clears execution state and all bound values.
    pub fn clear_bindings(&mut self) {
        self.reset();
        self.binds.fill(OdbcBind::Null);
        self.bound.fill(false);
        self.output_specs.fill(None);
    }

    /// Reports whether the statement still needs execution.
    pub fn needs_execute(&self) -> bool {
        !self.executed
    }

    /// Binds all occurrences and executes the prepared native statement.
    pub fn execute(&mut self, connection: &mut OdbcConn) -> Result<(), String> {
        if self.bound.iter().any(|bound| !bound) {
            self.error = ErrorState {
                sqlstate: "HY093".to_string(),
                native_code: 0,
                message: "Invalid parameter number: number of bound variables does not match number of tokens".to_string(),
            };
            return Err(self.error.message.clone());
        }
        unsafe {
            let _ = SQLCloseCursor(self.stmt);
            let _ = SQLFreeStmt(self.stmt, FreeStmtOption::ResetParams);
        }
        #[cfg(feature = "sqlsrv")]
        if self.flavor.is_sqlsrv() && self.sqlsrv_emulated {
            return self.execute_sqlsrv_emulated(connection);
        }
        #[cfg(feature = "sqlsrv")]
        if self.flavor.is_sqlsrv() && self.sqlsrv_query_timeout > 0 {
            let timeout = self.sqlsrv_query_timeout as isize as *mut c_void;
            let configured = unsafe {
                SQLSetStmtAttr(self.stmt, StatementAttribute::QueryTimeout, timeout, 0)
            };
            if !succeeded(configured) {
                self.error = diagnostic(
                    HandleType::Stmt,
                    self.stmt.as_handle(),
                    "SQLSetStmtAttr: SQL_ATTR_QUERY_TIMEOUT",
                );
                connection.error = self.error.clone();
                return Err(self.error.message.clone());
            }
        }
        let mut payloads = Vec::with_capacity(self.order.len());
        let mut native_doubles = Vec::with_capacity(self.order.len());
        let mut descriptors = Vec::with_capacity(self.order.len());
        for (occurrence, slot) in self.order.iter().enumerate() {
            let slot = usize::try_from(*slot).ok().and_then(|slot| slot.checked_sub(1)).unwrap_or(0);
            #[cfg(feature = "sqlsrv")]
            let sqlsrv_defaults = self.flavor.is_sqlsrv().then(|| {
                sqlsrv_parameter_defaults(&self.binds[slot], self.sqlsrv_encoding)
            });
            #[cfg(not(feature = "sqlsrv"))]
            let sqlsrv_defaults: Option<(SqlDataType, usize, i16)> = None;
            let (fallback_type, initial_size, initial_scale) =
                sqlsrv_defaults.unwrap_or_else(|| match &self.binds[slot] {
                    OdbcBind::Int(_) => (SqlDataType::INTEGER, 4000, 5),
                    OdbcBind::Binary(value) => {
                        (SqlDataType::EXT_LONG_VAR_BINARY, value.len().max(4000), 5)
                    }
                    OdbcBind::Text(value) => {
                        (SqlDataType::EXT_LONG_VARCHAR, value.len().max(4000), 5)
                    }
                    _ => (SqlDataType::EXT_LONG_VARCHAR, 4000, 5),
                });
            let mut sql_type = fallback_type;
            let mut column_size = initial_size;
            let mut scale = initial_scale;
            if !self.flavor.is_sqlsrv() {
                let mut nullable = Nullability::NULLABLE;
                let described = unsafe {
                    SQLDescribeParam(
                        self.stmt,
                        occurrence as u16 + 1,
                        &mut sql_type,
                        &mut column_size,
                        &mut scale,
                        &mut nullable,
                    )
                };
                if !succeeded(described) {
                    sql_type = fallback_type;
                    column_size = initial_size;
                    scale = initial_scale;
                }
            }
            #[cfg(feature = "sqlsrv")]
            let native_double =
                sqlsrv_double_parameter_types(self.flavor, &self.binds[slot], sql_type);
            #[cfg(not(feature = "sqlsrv"))]
            let native_double: Option<(CDataType, SqlDataType)> = None;
            if let Some((_, native_sql_type)) = native_double {
                if native_sql_type != sql_type {
                    column_size = 0;
                    scale = 0;
                }
                sql_type = native_sql_type;
            }
            let wide = if self.flavor.is_sqlsrv() {
                #[cfg(feature = "sqlsrv")]
                {
                    self.sqlsrv_encoding != SQLSRV_ENCODING_BINARY
                        && self.sqlsrv_encoding != SQLSRV_ENCODING_SYSTEM
                }
                #[cfg(not(feature = "sqlsrv"))]
                false
            } else {
                self.assume_utf8
                    && matches!(
                        sql_type,
                        SqlDataType::EXT_W_CHAR
                            | SqlDataType::EXT_W_VARCHAR
                            | SqlDataType::EXT_W_LONG_VARCHAR
                    )
            };
            let (mut payload, c_type, indicator) = match (&self.binds[slot], native_double) {
                (OdbcBind::Double(_), Some((c_type, _))) => {
                    (Vec::new(), c_type, std::mem::size_of::<f64>() as isize)
                }
                (OdbcBind::Null, _) => (Vec::new(), CDataType::Char, NULL_DATA),
                (OdbcBind::Int(value), _) => {
                    let text = value.to_string();
                    (text.into_bytes(), CDataType::Char, 0)
                }
                (OdbcBind::Double(value), _) => {
                    let text = value.to_string();
                    (text.into_bytes(), CDataType::Char, 0)
                }
                (OdbcBind::Text(value), _) if wide => {
                    let payload = String::from_utf8(value.clone()).map_or_else(
                        |_| value.clone(),
                        |text| {
                            text.encode_utf16()
                                .flat_map(u16::to_ne_bytes)
                                .collect::<Vec<_>>()
                        },
                    );
                    (payload, CDataType::WChar, 0)
                }
                (OdbcBind::Text(value), _) => (value.clone(), CDataType::Char, 0),
                (OdbcBind::Binary(value), _) => (value.clone(), CDataType::Binary, 0),
            };
            let mut input_length = native_double
                .map_or(payload.len(), |_| std::mem::size_of::<f64>());
            if let Some(output) = self.output_specs[slot].filter(|_| native_double.is_none()) {
                input_length = prepare_output_buffer(&mut payload, output, column_size);
            }
            let indicator = if indicator == NULL_DATA {
                NULL_DATA
            } else {
                input_length as isize
            };
            payloads.push(payload);
            native_doubles.push(native_double.map(|_| match self.binds[slot] {
                OdbcBind::Double(value) => Box::new(value),
                _ => unreachable!("native double descriptors only accompany double binds"),
            }));
            descriptors.push((
                c_type,
                sql_type,
                column_size,
                scale,
                indicator,
                wide,
                native_double.is_some(),
            ));
        }
        self.indicators.clear();
        self.indicators.extend(descriptors.iter().map(|descriptor| descriptor.4));
        for (occurrence, (c_type, sql_type, column_size, scale, _, _, native_double)) in
            descriptors.iter().copied().enumerate()
        {
            let payload = &mut payloads[occurrence];
            let slot = usize::try_from(self.order[occurrence])
                .ok()
                .and_then(|slot| slot.checked_sub(1))
                .unwrap_or(0);
            let pointer = if native_double {
                native_doubles[occurrence]
                    .as_deref_mut()
                    .map_or(ptr::null_mut(), |value| (value as *mut f64).cast())
            } else if self.indicators[occurrence] == NULL_DATA
                && self.output_specs[slot].is_none()
            {
                ptr::null_mut()
            } else {
                payload.as_mut_ptr().cast()
            };
            let parameter_type = match self.output_specs[slot] {
                Some(output) if output.input_output => ParamType::InputOutput,
                Some(_) => ParamType::Output,
                None => ParamType::Input,
            };
            let result = unsafe {
                SQLBindParameter(
                    self.stmt,
                    occurrence as u16 + 1,
                    parameter_type,
                    c_type,
                    sql_type,
                    column_size,
                    scale,
                    pointer,
                    if native_double {
                        std::mem::size_of::<f64>() as isize
                    } else {
                        payload.len() as isize
                    },
                    &mut self.indicators[occurrence],
                )
            };
            if !succeeded(result) {
                self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLBindParameter");
                connection.error = self.error.clone();
                return Err(self.error.message.clone());
            }
        }
        #[cfg(feature = "sqlsrv")]
        let result = if self.flavor.is_sqlsrv() && self.sqlsrv_direct_query {
            let sql = self.translated_sql.encode_utf16().collect::<Vec<_>>();
            unsafe { SQLExecDirectW(self.stmt, sql.as_ptr(), sql.len() as i32) }
        } else {
            unsafe { SQLExecute(self.stmt) }
        };
        #[cfg(not(feature = "sqlsrv"))]
        let result = unsafe { SQLExecute(self.stmt) };
        if result != SqlReturn::NO_DATA && !succeeded(result) {
            self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLExecute");
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        let execution_info = if result == SqlReturn::SUCCESS_WITH_INFO && connection.is_odbc() {
            Some(diagnostic(
                HandleType::Stmt,
                self.stmt.as_handle(),
                "SQLExecute",
            ))
        } else {
            None
        };
        self.output_values.fill(None);
        for (occurrence, slot) in self.order.iter().copied().enumerate() {
            let Some(slot) = usize::try_from(slot).ok().and_then(|slot| slot.checked_sub(1)) else {
                continue;
            };
            if self.output_specs[slot].is_none() {
                continue;
            }
            let indicator = self.indicators[occurrence];
            let data = if indicator == NULL_DATA {
                None
            } else if descriptors[occurrence].6 {
                Some(
                    native_doubles[occurrence]
                        .as_deref()
                        .map_or(0.0, |value| *value)
                        .to_string()
                        .into_bytes(),
                )
            } else {
                let length = usize::try_from(indicator)
                    .unwrap_or(0)
                    .min(payloads[occurrence].len());
                let bytes = &payloads[occurrence][..length];
                if descriptors[occurrence].5 {
                    let units = bytes
                        .chunks_exact(2)
                        .map(|bytes| u16::from_ne_bytes([bytes[0], bytes[1]]));
                    Some(String::from_utf16_lossy(&units.collect::<Vec<_>>()).into_bytes())
                } else {
                    Some(bytes.to_vec())
                }
            };
            self.output_values[slot] = Some(OdbcOutputValue {
                data,
                lob: self.output_specs[slot].is_some_and(|output| output.lob),
                numeric: matches!(descriptors[occurrence].1.0, -7 | 4 | 5 | 16),
            });
        }
        self.sent_sql.clear();
        self.materialize_current_result(connection)?;
        if connection.is_ibm() {
            connection.refresh_ibm_ids_last_insert_id(self.stmt);
        }
        if self.is_insert && connection.is_informix() {
            connection.refresh_informix_last_insert_id();
        }
        self.executed = true;
        if let Some(warning) = execution_info {
            self.error = warning.clone();
            connection.error = warning;
        } else {
            self.error = ErrorState::default();
            connection.error = ErrorState::default();
        }
        Ok(())
    }

    /// Executes SQLSRV's client-side emulated-prepare path through `SQLExecDirectW`.
    #[cfg(feature = "sqlsrv")]
    fn execute_sqlsrv_emulated(&mut self, connection: &mut OdbcConn) -> Result<(), String> {
        if self.output_specs.iter().any(Option::is_some) {
            self.error = ErrorState {
                sqlstate: "IMSSP".to_string(),
                native_code: -82,
                message: "Output parameters are not supported with emulated prepares".to_string(),
            };
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        let national = connection.sqlsrv_default_str_param == 0x4000_0000
            || connection.sqlsrv_encoding == SQLSRV_ENCODING_UTF8;
        self.sent_sql = interpolate_sqlsrv(
            &self.translated_sql,
            &self.order,
            &self.binds,
            national,
        )?;
        if self.sqlsrv_query_timeout > 0 {
            let configured = unsafe {
                SQLSetStmtAttr(
                    self.stmt,
                    StatementAttribute::QueryTimeout,
                    self.sqlsrv_query_timeout as isize as *mut c_void,
                    0,
                )
            };
            if !succeeded(configured) {
                self.error = diagnostic(
                    HandleType::Stmt,
                    self.stmt.as_handle(),
                    "SQLSetStmtAttr: SQL_ATTR_QUERY_TIMEOUT",
                );
                connection.error = self.error.clone();
                return Err(self.error.message.clone());
            }
        }
        let sql = self.sent_sql.encode_utf16().collect::<Vec<_>>();
        let result = unsafe { SQLExecDirectW(self.stmt, sql.as_ptr(), sql.len() as i32) };
        if result != SqlReturn::NO_DATA && !succeeded(result) {
            self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLExecDirectW");
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        self.output_values.fill(None);
        self.materialize_current_result(connection)?;
        self.executed = true;
        self.error = ErrorState::default();
        connection.error = ErrorState::default();
        Ok(())
    }

    /// Describes and materializes the active native result set.
    fn materialize_current_result(&mut self, connection: &mut OdbcConn) -> Result<(), String> {
        self.columns.clear();
        self.rows.clear();
        self.cursor = -1;
        #[cfg(feature = "sqlsrv")]
        {
            self.sqlsrv_classification = None;
            self.sqlsrv_classification_error = None;
        }
        let sqlsrv = connection.is_sqlsrv();
        let mut count = 0i16;
        if !succeeded(unsafe { SQLNumResultCols(self.stmt, &mut count) }) {
            self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLNumResultCols");
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        if count == 0 {
            let mut row_count = -1;
            if succeeded(unsafe { SQLRowCount(self.stmt, &mut row_count) }) {
                self.row_count = if row_count < 0 { 0 } else { row_count as i64 };
            } else {
                self.row_count = 0;
            }
            connection.changes = self.row_count;
            return Ok(());
        }
        for index in 1..=count {
            let mut name = [0u8; 256];
            #[cfg(feature = "sqlsrv")]
            let mut wide_name = [0u16; 256];
            let mut name_len = 0i16;
            let mut data_type = SqlDataType::UNKNOWN_TYPE;
            let mut size = 0usize;
            let mut scale = 0i16;
            let mut nullable = Nullability::UNKNOWN;
            #[cfg(feature = "sqlsrv")]
            let result = if sqlsrv {
                unsafe {
                    SQLDescribeColW(
                        self.stmt,
                        index as u16,
                        wide_name.as_mut_ptr(),
                        wide_name.len() as i16,
                        &mut name_len,
                        &mut data_type,
                        &mut size,
                        &mut scale,
                        &mut nullable,
                    )
                }
            } else {
                unsafe {
                    SQLDescribeCol(
                        self.stmt,
                        index as u16,
                        name.as_mut_ptr(),
                        name.len() as i16,
                        &mut name_len,
                        &mut data_type,
                        &mut size,
                        &mut scale,
                        &mut nullable,
                    )
                }
            };
            #[cfg(not(feature = "sqlsrv"))]
            let result = unsafe {
                SQLDescribeCol(
                    self.stmt,
                    index as u16,
                    name.as_mut_ptr(),
                    name.len() as i16,
                    &mut name_len,
                    &mut data_type,
                    &mut size,
                    &mut scale,
                    &mut nullable,
                )
            };
            if !succeeded(result) {
                self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLDescribeCol");
                connection.error = self.error.clone();
                return Err(self.error.message.clone());
            }
            let informix = connection.is_informix();
            let ibm = connection.is_ibm();
            let native_type = if sqlsrv {
                #[cfg(feature = "sqlsrv")]
                {
                    column_text_attribute_w(self.stmt, index as u16, Desc::TypeName)
                        .unwrap_or_default()
                }
                #[cfg(not(feature = "sqlsrv"))]
                String::new()
            } else if informix || ibm {
                column_text_attribute(self.stmt, index as u16, Desc::TypeName).unwrap_or_default()
            } else {
                String::new()
            };
            let informix_lob = informix
                && (matches!(
                    data_type,
                    SqlDataType::EXT_LONG_VARCHAR
                        | SqlDataType::EXT_BINARY
                        | SqlDataType::EXT_VAR_BINARY
                        | SqlDataType::EXT_LONG_VAR_BINARY
                ) || data_type.0 == 17);
            let ibm_lob = ibm
                && matches!(data_type.0, -2 | -3 | -4 | -98 | -99 | -370);
            let lob = informix_lob || ibm_lob;
            let metadata_pdo_lob = (informix && informix_metadata_is_lob(&native_type))
                || (ibm && {
                    #[cfg(feature = "ibm")]
                    {
                        ibm_metadata_is_lob(data_type.0)
                    }
                    #[cfg(not(feature = "ibm"))]
                    false
                });
            let mut flags = 0;
            if nullable == Nullability::NO_NULLS {
                flags |= 1;
            }
            if (informix || ibm)
                && column_numeric_attribute(self.stmt, index as u16, Desc::Unsigned)
                    .is_some_and(|value| value != 0)
            {
                flags |= 2;
            }
            if (informix || ibm)
                && column_numeric_attribute(self.stmt, index as u16, Desc::AutoUniqueValue)
                    .is_some_and(|value| value != 0)
            {
                flags |= 4;
            }
            #[cfg(feature = "sqlsrv")]
            let column_name = if sqlsrv {
                String::from_utf16_lossy(
                    &wide_name[..usize::try_from(name_len)
                        .unwrap_or(0)
                        .min(wide_name.len())],
                )
            } else {
                String::from_utf8_lossy(
                    &name[..usize::try_from(name_len).unwrap_or(0).min(name.len())],
                )
                .into_owned()
            };
            #[cfg(not(feature = "sqlsrv"))]
            let column_name = String::from_utf8_lossy(
                &name[..usize::try_from(name_len).unwrap_or(0).min(name.len())],
            )
            .into_owned();
            self.columns.push(OdbcColumn {
                name: column_name,
                wide: if sqlsrv {
                    #[cfg(feature = "sqlsrv")]
                    {
                        self.sqlsrv_encoding != SQLSRV_ENCODING_BINARY
                            && self.sqlsrv_encoding != SQLSRV_ENCODING_SYSTEM
                            && !matches!(
                                data_type,
                                SqlDataType::EXT_BINARY
                                    | SqlDataType::EXT_VAR_BINARY
                                    | SqlDataType::EXT_LONG_VAR_BINARY
                            )
                    }
                    #[cfg(not(feature = "sqlsrv"))]
                    false
                } else {
                    self.assume_utf8
                        && matches!(
                            data_type,
                            SqlDataType::EXT_W_CHAR
                                | SqlDataType::EXT_W_VARCHAR
                                | SqlDataType::EXT_W_LONG_VARCHAR
                        )
                },
                lob,
                metadata_pdo_lob,
                len: i64::try_from(size).unwrap_or(i64::MAX),
                precision: i64::from(scale),
                scale: i64::from(scale),
                table: if informix || ibm || sqlsrv {
                    if sqlsrv {
                        #[cfg(feature = "sqlsrv")]
                        {
                            column_text_attribute_w(
                                self.stmt,
                                index as u16,
                                Desc::BaseTableName,
                            )
                            .unwrap_or_default()
                        }
                        #[cfg(not(feature = "sqlsrv"))]
                        String::new()
                    } else {
                        column_text_attribute(self.stmt, index as u16, Desc::BaseTableName)
                            .unwrap_or_default()
                    }
                } else {
                    String::new()
                },
                native_type,
                flags,
                #[cfg(feature = "sqlsrv")]
                data_type: data_type.0,
            });
        }
        #[cfg(feature = "sqlsrv")]
        let mut buffered_bytes = 0usize;
        loop {
            let fetched = unsafe { SQLFetch(self.stmt) };
            if fetched == SqlReturn::NO_DATA {
                break;
            }
            if !succeeded(fetched) {
                self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLFetch");
                connection.error = self.error.clone();
                return Err(self.error.message.clone());
            }
            let mut row = Vec::with_capacity(count as usize);
            for index in 1..=count {
                let wide = self.columns[index as usize - 1].wide;
                let value = self.read_column(index as u16, wide)?;
                #[cfg(feature = "sqlsrv")]
                let value = if sqlsrv {
                    let mut value = value;
                    value = format_sqlsrv_decimal(
                        value,
                        &self.columns[index as usize - 1].native_type,
                        self.sqlsrv_format_decimals,
                        self.sqlsrv_decimal_places,
                    );
                    if self.sqlsrv_cursor_type == 42 {
                        buffered_bytes = buffered_bytes
                            .saturating_add(value.as_ref().map_or(0, Vec::len));
                        let limit = usize::try_from(self.sqlsrv_client_buffer_kb)
                            .unwrap_or(usize::MAX)
                            .saturating_mul(1024);
                        if buffered_bytes > limit {
                            self.error = ErrorState {
                                sqlstate: "IMSSP".to_string(),
                                native_code: -59,
                                message: "Memory limit for buffered query exceeded".to_string(),
                            };
                            connection.error = self.error.clone();
                            return Err(self.error.message.clone());
                        }
                    }
                    value
                } else {
                    value
                };
                row.push(value);
            }
            self.rows.push(row);
        }
        let mut row_count = -1;
        if succeeded(unsafe { SQLRowCount(self.stmt, &mut row_count) }) {
            self.row_count = if row_count < 0 { 0 } else { row_count as i64 };
        } else {
            self.row_count = 0;
        }
        connection.changes = self.row_count;
        Ok(())
    }

    /// Reads an arbitrary-length current-row value as PDO_ODBC text.
    fn read_column(&mut self, column: u16, wide: bool) -> Result<Option<Vec<u8>>, String> {
        let mut value = Vec::new();
        loop {
            let mut chunk = [0u8; 8192];
            let mut indicator = 0isize;
            let result = unsafe {
                SQLGetData(
                    self.stmt,
                    column,
                    if wide { CDataType::WChar } else { CDataType::Char },
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as isize,
                    &mut indicator,
                )
            };
            if indicator == NULL_DATA {
                return Ok(None);
            }
            if result == SqlReturn::NO_DATA {
                break;
            }
            if !succeeded(result) {
                self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLGetData");
                return Err(self.error.message.clone());
            }
            let capacity = if wide { chunk.len() - 2 } else { chunk.len() - 1 };
            let payload = if result == SqlReturn::SUCCESS_WITH_INFO || indicator == odbc_sys::NO_TOTAL {
                capacity
            } else {
                let reported = usize::try_from(indicator).unwrap_or(0);
                if reported >= value.len() {
                    reported.saturating_sub(value.len()).min(capacity)
                } else {
                    reported.min(capacity)
                }
            };
            value.extend_from_slice(&chunk[..payload]);
            if result == SqlReturn::SUCCESS {
                break;
            }
        }
        if wide {
            let units = value
                .chunks_exact(2)
                .map(|bytes| u16::from_ne_bytes([bytes[0], bytes[1]]));
            return Ok(Some(String::from_utf16_lossy(&units.collect::<Vec<_>>()).into_bytes()));
        }
        Ok(Some(value))
    }

    /// Advances to the next buffered row.
    pub fn step(&mut self) -> i64 {
        let next = self.cursor + 1;
        if next < self.rows.len() as isize {
            self.cursor = next;
            1
        } else {
            0
        }
    }

    /// Selects a buffered row using PDO fetch orientation semantics.
    pub fn step_oriented(&mut self, orientation: i64, offset: i64) -> i64 {
        let target = match orientation {
            0 => self.cursor + 1,
            1 => self.cursor - 1,
            2 => 0,
            3 => self.rows.len() as isize - 1,
            4 => offset as isize,
            5 => self.cursor + offset as isize,
            _ => return 0,
        };
        if target < 0 || target >= self.rows.len() as isize {
            return 0;
        }
        self.cursor = target;
        1
    }

    /// Advances the native statement to its next result set.
    pub fn next_rowset(&mut self, connection: &mut OdbcConn) -> bool {
        let result = unsafe { SQLMoreResults(self.stmt) };
        if result == SqlReturn::NO_DATA {
            return false;
        }
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLMoreResults");
            connection.error = self.error.clone();
            return false;
        }
        self.materialize_current_result(connection).is_ok()
    }

    /// Sets the native ODBC cursor name.
    pub fn set_cursor_name(&mut self, name: &str) -> bool {
        let result = unsafe { SQLSetCursorName(self.stmt, name.as_ptr(), name.len() as i16) };
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLSetCursorName");
            return false;
        }
        true
    }

    /// Reads the native ODBC cursor name.
    pub fn cursor_name(&mut self) -> String {
        let mut buffer = [0u8; 256];
        let mut length = 0i16;
        let result = unsafe {
            SQLGetCursorName(self.stmt, buffer.as_mut_ptr(), buffer.len() as i16, &mut length)
        };
        if !succeeded(result) {
            self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLGetCursorName");
            return String::new();
        }
        String::from_utf8_lossy(&buffer[..usize::try_from(length).unwrap_or(0).min(buffer.len())]).into_owned()
    }

    /// Sets statement-level `ATTR_ASSUME_UTF8`; php-src stores it but returns false.
    pub fn set_assume_utf8(&mut self, enabled: bool) -> bool {
        self.assume_utf8 = enabled;
        false
    }

    /// Returns statement-level `ATTR_ASSUME_UTF8`; php-src reports false after filling the value.
    pub fn assume_utf8(&self) -> bool {
        false
    }

    /// Applies a PDO_SQLSRV statement attribute after PDO has created the handle.
    #[cfg(feature = "sqlsrv")]
    pub fn set_sqlsrv_attribute(&mut self, attribute: i64, value: i64) -> bool {
        if !self.flavor.is_sqlsrv() {
            return false;
        }
        let accepted = match attribute {
            SQLSRV_ATTR_ENCODING
                if matches!(
                    value,
                    SQLSRV_ENCODING_DEFAULT
                        | SQLSRV_ENCODING_BINARY
                        | SQLSRV_ENCODING_SYSTEM
                        | SQLSRV_ENCODING_UTF8
                ) =>
            {
                self.sqlsrv_encoding = value;
                true
            }
            SQLSRV_ATTR_QUERY_TIMEOUT if value >= 0 => {
                self.sqlsrv_query_timeout = value;
                true
            }
            SQLSRV_ATTR_CLIENT_BUFFER_MAX_KB_SIZE if value > 0 => {
                self.sqlsrv_client_buffer_kb = value;
                true
            }
            SQLSRV_ATTR_FETCHES_NUMERIC_TYPE => {
                self.sqlsrv_fetch_numeric = value != 0;
                true
            }
            SQLSRV_ATTR_FETCHES_DATETIME_TYPE => {
                self.sqlsrv_fetch_datetime = value != 0;
                true
            }
            SQLSRV_ATTR_FORMAT_DECIMALS => {
                self.sqlsrv_format_decimals = value != 0;
                true
            }
            SQLSRV_ATTR_DECIMAL_PLACES => {
                self.sqlsrv_decimal_places = if (0..=4).contains(&value) { value } else { -1 };
                true
            }
            SQLSRV_ATTR_DATA_CLASSIFICATION => {
                self.sqlsrv_data_classification = value != 0;
                self.sqlsrv_classification = None;
                self.sqlsrv_classification_error = None;
                true
            }
            _ => false,
        };
        if accepted {
            self.error = ErrorState::default();
        }
        accepted
    }

    /// Reads a PDO_SQLSRV statement attribute from its live statement state.
    #[cfg(feature = "sqlsrv")]
    pub fn sqlsrv_attribute(&self, attribute: i64) -> Option<i64> {
        if !self.flavor.is_sqlsrv() {
            return None;
        }
        match attribute {
            SQLSRV_ATTR_ENCODING => Some(self.sqlsrv_encoding),
            SQLSRV_ATTR_QUERY_TIMEOUT => Some(self.sqlsrv_query_timeout),
            SQLSRV_ATTR_DIRECT_QUERY => Some(self.sqlsrv_direct_query as i64),
            SQLSRV_ATTR_CURSOR_SCROLL_TYPE => Some(self.sqlsrv_cursor_type),
            SQLSRV_ATTR_CLIENT_BUFFER_MAX_KB_SIZE => Some(self.sqlsrv_client_buffer_kb),
            SQLSRV_ATTR_FETCHES_NUMERIC_TYPE => Some(self.sqlsrv_fetch_numeric as i64),
            SQLSRV_ATTR_FETCHES_DATETIME_TYPE => Some(self.sqlsrv_fetch_datetime as i64),
            SQLSRV_ATTR_FORMAT_DECIMALS => Some(self.sqlsrv_format_decimals as i64),
            SQLSRV_ATTR_DECIMAL_PLACES => Some(self.sqlsrv_decimal_places),
            SQLSRV_ATTR_DATA_CLASSIFICATION => Some(self.sqlsrv_data_classification as i64),
            10 => Some((self.sqlsrv_cursor_type != 0) as i64),
            _ => None,
        }
    }

    /// Applies SQLSRV prepare-only options before the statement is first executed.
    #[cfg(feature = "sqlsrv")]
    pub fn configure_sqlsrv_prepare_option(&mut self, attribute: i64, value: i64) -> bool {
        if !self.flavor.is_sqlsrv() {
            return false;
        }
        match attribute {
            SQLSRV_ATTR_DIRECT_QUERY => {
                self.sqlsrv_direct_query = value != 0;
                true
            }
            SQLSRV_ATTR_CURSOR_SCROLL_TYPE if sqlsrv_native_cursor_type(value).is_some() => {
                self.sqlsrv_cursor_type == value
            }
            _ => self.set_sqlsrv_attribute(attribute, value),
        }
    }

    /// Loads and parses Microsoft ODBC sensitivity metadata on first inspection.
    #[cfg(feature = "sqlsrv")]
    fn ensure_sqlsrv_classification(&mut self) -> bool {
        if !self.flavor.is_sqlsrv() || !self.sqlsrv_data_classification {
            return false;
        }
        if self.sqlsrv_classification.is_some() {
            return true;
        }
        if let Some(error) = self.sqlsrv_classification_error.clone() {
            self.error = error;
            return false;
        }
        if !self.executed {
            let error = ErrorState {
                sqlstate: "IMSSP".to_string(),
                native_code: -100,
                message: "Data classification metadata is unavailable before execution"
                    .to_string(),
            };
            self.error = error.clone();
            self.sqlsrv_classification_error = Some(error);
            return false;
        }
        let mut descriptor = HDesc::null();
        let descriptor_result = unsafe {
            SQLGetStmtAttr(
                self.stmt,
                StatementAttribute::ImpRowDesc,
                (&mut descriptor as *mut HDesc).cast(),
                odbc_sys::IS_POINTER,
                ptr::null_mut(),
            )
        };
        if !succeeded(descriptor_result) {
            let error = diagnostic(
                HandleType::Stmt,
                self.stmt.as_handle(),
                "SQLGetStmtAttr SQL_ATTR_IMP_ROW_DESC",
            );
            self.error = error.clone();
            self.sqlsrv_classification_error = Some(error);
            return false;
        }
        let mut required = 0i32;
        let length_result = unsafe {
            SQLGetDescFieldWRaw(
                descriptor,
                0,
                SQL_CA_SS_DATA_CLASSIFICATION,
                ptr::null_mut(),
                0,
                &mut required,
            )
        };
        if length_result != SqlReturn::SUCCESS || required <= 0 {
            let error = diagnostic(
                HandleType::Desc,
                descriptor.as_handle(),
                "SQLGetDescFieldW SQL_CA_SS_DATA_CLASSIFICATION",
            );
            self.error = error.clone();
            self.sqlsrv_classification_error = Some(error);
            return false;
        }
        let mut blob = vec![0u8; usize::try_from(required).unwrap_or(0)];
        let mut returned = 0i32;
        let data_result = unsafe {
            SQLGetDescFieldWRaw(
                descriptor,
                0,
                SQL_CA_SS_DATA_CLASSIFICATION,
                blob.as_mut_ptr().cast(),
                required,
                &mut returned,
            )
        };
        if data_result != SqlReturn::SUCCESS {
            let error = diagnostic(
                HandleType::Desc,
                descriptor.as_handle(),
                "SQLGetDescFieldW SQL_CA_SS_DATA_CLASSIFICATION",
            );
            self.error = error.clone();
            self.sqlsrv_classification_error = Some(error);
            return false;
        }
        blob.truncate(usize::try_from(returned).unwrap_or(blob.len()).min(blob.len()));
        let mut version = 0u32;
        let mut version_length = 0i32;
        let version_result = unsafe {
            SQLGetDescFieldWRaw(
                descriptor,
                0,
                SQL_CA_SS_DATA_CLASSIFICATION_VERSION,
                (&mut version as *mut u32).cast(),
                odbc_sys::IS_INTEGER,
                &mut version_length,
            )
        };
        match parse_sqlsrv_classification_blob(
            &blob,
            version_result == SqlReturn::SUCCESS && version >= 2,
        ) {
            Ok(classification) => {
                self.sqlsrv_classification = Some(classification);
                self.sqlsrv_classification_error = None;
                self.error = ErrorState::default();
                true
            }
            Err(message) => {
                let error = ErrorState {
                    sqlstate: "IMSSP".to_string(),
                    native_code: -101,
                    message,
                };
                self.error = error.clone();
                self.sqlsrv_classification_error = Some(error);
                false
            }
        }
    }

    /// Returns the number of sensitivity pairs for one result column, or `-1` on error.
    #[cfg(feature = "sqlsrv")]
    pub fn sqlsrv_classification_pair_count(&mut self, column: i64) -> i64 {
        if !self.ensure_sqlsrv_classification() {
            return -1;
        }
        usize::try_from(column)
            .ok()
            .and_then(|column| self.sqlsrv_classification.as_ref()?.columns.get(column))
            .map(|pairs| pairs.len() as i64)
            .unwrap_or(-1)
    }

    /// Returns one label/information-type string selected by PDO's metadata builder.
    #[cfg(feature = "sqlsrv")]
    pub fn sqlsrv_classification_text(
        &mut self,
        column: i64,
        pair: i64,
        field: i64,
    ) -> String {
        if !self.ensure_sqlsrv_classification() {
            return String::new();
        }
        let Some(pair) = usize::try_from(column)
            .ok()
            .and_then(|column| self.sqlsrv_classification.as_ref()?.columns.get(column))
            .and_then(|pairs| usize::try_from(pair).ok().and_then(|pair| pairs.get(pair)))
        else {
            return String::new();
        };
        match field {
            0 => pair.label_name.clone(),
            1 => pair.label_id.clone(),
            2 => pair.information_name.clone(),
            3 => pair.information_id.clone(),
            _ => String::new(),
        }
    }

    /// Returns one column sensitivity rank, or `-1` when the server omitted ranks.
    #[cfg(feature = "sqlsrv")]
    pub fn sqlsrv_classification_pair_rank(&mut self, column: i64, pair: i64) -> i64 {
        if !self.ensure_sqlsrv_classification() {
            return -1;
        }
        usize::try_from(column)
            .ok()
            .and_then(|column| self.sqlsrv_classification.as_ref()?.columns.get(column))
            .and_then(|pairs| usize::try_from(pair).ok().and_then(|pair| pairs.get(pair)))
            .and_then(|pair| pair.rank)
            .map(i64::from)
            .unwrap_or(-1)
    }

    /// Returns the result-set sensitivity rank, or `-1` when the server omitted it.
    #[cfg(feature = "sqlsrv")]
    pub fn sqlsrv_classification_query_rank(&mut self) -> i64 {
        if !self.ensure_sqlsrv_classification() {
            return -1;
        }
        self.sqlsrv_classification
            .as_ref()
            .and_then(|classification| classification.query_rank)
            .map(i64::from)
            .unwrap_or(-1)
    }

    /// Returns the active result column count.
    pub fn column_count(&self) -> i64 {
        self.columns.len() as i64
    }

    /// Returns one active result column name.
    pub fn column_name(&self, index: i64) -> String {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index)).map(|column| column.name.clone()).unwrap_or_default()
    }

    /// Returns PDO's common text/null storage-class tag, including Informix LOB streams.
    pub fn column_type(&self, index: i64) -> i64 {
        if self.cell(index).is_none_or(Option::is_none) {
            return 5;
        }
        #[cfg(feature = "sqlsrv")]
        if self.flavor.is_sqlsrv() && self.sqlsrv_fetch_numeric {
            let data_type = usize::try_from(index)
                .ok()
                .and_then(|index| self.columns.get(index))
                .map(|column| column.data_type)
                .unwrap_or_default();
            if matches!(data_type, -7 | -6 | 4 | 5) {
                return 1;
            }
            if matches!(data_type, 6 | 7 | 8) {
                return 2;
            }
        }
        if usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .is_some_and(|column| column.lob)
        {
            4
        } else {
            3
        }
    }

    /// Returns the driver-native result-column type name exposed by PDO_INFORMIX.
    pub fn column_native_type(&self, index: i64) -> String {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .map(|column| column.native_type.clone())
            .unwrap_or_default()
    }

    /// Returns the source table name exposed by PDO_INFORMIX when available.
    pub fn column_table_name(&self, index: i64) -> String {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .map(|column| column.table.clone())
            .unwrap_or_default()
    }

    /// Returns the SQL scale captured by `SQLDescribeCol` for PDO_INFORMIX metadata.
    pub fn column_scale(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .map(|column| column.scale)
            .unwrap_or_default()
    }

    /// Returns PDO core's common maximum column length captured by `SQLDescribeCol`.
    pub fn column_len(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .map(|column| column.len)
            .unwrap_or(-1)
    }

    /// Returns PDO core's common precision field, which CLI drivers fill from scale.
    pub fn column_precision(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .map(|column| column.precision)
            .unwrap_or_default()
    }

    /// Returns Informix not-null, unsigned, and auto-increment descriptor bits.
    pub fn column_flags(&self, index: i64) -> i64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .map(|column| column.flags)
            .unwrap_or_default()
    }

    /// Returns PDO_INFORMIX's metadata parameter type for the described column.
    pub fn column_pdo_type(&self, index: i64) -> i64 {
        if usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .is_some_and(|column| column.metadata_pdo_lob)
        {
            3
        } else {
            2
        }
    }

    /// Reports whether SQLSRV should materialize this temporal column as `DateTime`.
    #[cfg(feature = "sqlsrv")]
    pub fn column_is_datetime(&self, index: i64) -> bool {
        self.flavor.is_sqlsrv()
            && self.sqlsrv_fetch_datetime
            && usize::try_from(index)
                .ok()
                .and_then(|index| self.columns.get(index))
                .is_some_and(|column| matches!(column.data_type, 91 | 92 | 93 | -154 | -155))
    }

    /// Returns one current value parsed as integer.
    pub fn column_int(&self, index: i64) -> i64 {
        String::from_utf8_lossy(&self.column_data(index)).parse().unwrap_or(0)
    }

    /// Returns one current value parsed as floating point.
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

    /// Returns the statement SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        &self.error.sqlstate
    }

    /// Returns the statement native code.
    pub fn errcode(&self) -> i64 {
        self.error.native_code
    }

    /// Returns the statement diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appends one driver-format length-prefixed UTF-16 classification field.
    #[cfg(feature = "sqlsrv")]
    fn push_classification_text(blob: &mut Vec<u8>, value: &str) {
        let utf16 = value.encode_utf16().collect::<Vec<_>>();
        blob.push(utf16.len() as u8);
        for unit in utf16 {
            blob.extend_from_slice(&unit.to_ne_bytes());
        }
    }

    /// Parses a named DSN and bridge-only PDO constructor options.
    #[test]
    #[cfg(feature = "odbc")]
    fn parses_named_dsn_options() {
        let options = parse_open_options("odbc:inventory;user=user%3Bname;password=p%25w;elephc_odbc_cursor_library=2;elephc_odbc_assume_utf8=1", CliFlavor::Odbc).unwrap();
        assert_eq!(options.source, "inventory");
        assert_eq!(options.username, "user;name");
        assert_eq!(options.password, "p%w");
        assert_eq!(options.cursor_library, SQL_CUR_USE_DRIVER);
        assert!(options.assume_utf8);
    }

    /// Removes bridge-only options without modifying an ODBC connection string.
    #[test]
    #[cfg(feature = "odbc")]
    fn preserves_direct_connection_string() {
        let options = parse_open_options("odbc:Driver={SQLite3};Database=/tmp/test.db;user=me", CliFlavor::Odbc).unwrap();
        assert_eq!(options.source, "Driver={SQLite3};Database=/tmp/test.db");
        assert_eq!(options.username, "me");
    }

    /// Parses PDO_INFORMIX named DSNs and folded constructor credentials.
    #[test]
    #[cfg(feature = "informix")]
    fn parses_informix_named_dsn_options() {
        let options = parse_open_options(
            "informix:inventory;user=elephc;password=secret",
            CliFlavor::Informix,
        )
        .unwrap();
        assert_eq!(options.source, "inventory");
        assert_eq!(options.username, "elephc");
        assert_eq!(options.password, "secret");
    }

    /// Parses a PDO_IBM direct DSN and extracts constructor-only CLI attributes.
    #[test]
    #[cfg(feature = "ibm")]
    fn parses_ibm_direct_dsn_options() {
        let options = parse_open_options(
            "ibm:DATABASE=SAMPLE;HOSTNAME=db2;elephc_ibm_attr_1283=elephc%3Bapp;elephc_ibm_attr_2561=1",
            CliFlavor::Ibm,
        )
        .unwrap();
        assert_eq!(options.source, "DATABASE=SAMPLE;HOSTNAME=db2");
        assert_eq!(
            options.ibm_attributes,
            [(PDO_IBM_ATTR_INFO_APPLNAME, "elephc;app".to_string()), (PDO_IBM_ATTR_USE_TRUSTED_CONTEXT, "1".to_string())]
        );
    }

    /// Keeps public PDO constant ordering distinct from IBM CLI's account/workstation IDs.
    #[test]
    #[cfg(feature = "ibm")]
    fn maps_ibm_public_attributes_to_native_cli_ids() {
        assert_eq!(ibm_native_connection_attribute(PDO_IBM_ATTR_INFO_USERID), Some(1281));
        assert_eq!(ibm_native_connection_attribute(PDO_IBM_ATTR_INFO_ACCTSTR), Some(1284));
        assert_eq!(ibm_native_connection_attribute(PDO_IBM_ATTR_INFO_APPLNAME), Some(1283));
        assert_eq!(ibm_native_connection_attribute(PDO_IBM_ATTR_INFO_WRKSTNNAME), Some(1282));
    }

    /// Applies ODBC brace quoting to semicolons and closing braces.
    #[test]
    fn quotes_connection_values() {
        assert_eq!(quote_connection_value("plain"), "plain");
        assert_eq!(quote_connection_value("a;b}c"), "{a;b}}c}");
    }

    /// Preserves semicolons and escaped braces inside ODBC connection-string values.
    #[test]
    fn connection_field_split_respects_braced_values() {
        assert_eq!(
            split_connection_fields("Driver={A;B};PWD={x}};y};UID=user"),
            ["Driver={A;B}", "PWD={x}};y}", "UID=user"]
        );
    }

    /// Bounds oversized input/output values to the caller's declared max length.
    #[test]
    fn output_buffer_respects_declared_max_length() {
        let mut payload = vec![b'A'; 64];
        let input_length = prepare_output_buffer(
            &mut payload,
            OutputSpec {
                max_length: 4,
                input_output: true,
                lob: false,
            },
            4000,
        );
        assert_eq!(input_length, 4);
        assert_eq!(payload, b"AAAA");
    }

    /// Derives SQLSRV value types without trusting temporary-table text descriptors.
    #[test]
    #[cfg(feature = "sqlsrv")]
    fn maps_sqlsrv_values_to_native_odbc_types() {
        assert_eq!(
            sqlsrv_double_parameter_types(
                CliFlavor::Sqlsrv,
                &OdbcBind::Double(12.5),
                SqlDataType::EXT_LONG_VARCHAR,
            ),
            Some((CDataType::Double, SqlDataType::FLOAT))
        );
        assert_eq!(
            sqlsrv_double_parameter_types(
                CliFlavor::Sqlsrv,
                &OdbcBind::Double(12.5),
                SqlDataType::DECIMAL,
            ),
            Some((CDataType::Double, SqlDataType::DECIMAL))
        );
        assert_eq!(
            sqlsrv_double_parameter_types(
                CliFlavor::Sqlsrv,
                &OdbcBind::Text(b"12.5".to_vec()),
                SqlDataType::EXT_LONG_VARCHAR,
            ),
            None
        );
        assert_eq!(
            sqlsrv_parameter_defaults(
                &OdbcBind::Text(b"2026-07-17 12:34:56".to_vec()),
                SQLSRV_ENCODING_UTF8,
            ),
            (SqlDataType::EXT_W_VARCHAR, 4000, 0)
        );
        assert_eq!(
            sqlsrv_parameter_defaults(
                &OdbcBind::Text(b"2026-07-17 12:34:56".to_vec()),
                SQLSRV_ENCODING_SYSTEM,
            ),
            (SqlDataType::VARCHAR, 8000, 0)
        );
        assert_eq!(
            sqlsrv_parameter_defaults(&OdbcBind::Binary(vec![0, 255]), SQLSRV_ENCODING_BINARY),
            (SqlDataType::EXT_VAR_BINARY, 8000, 0)
        );
        assert_eq!(sqlsrv_native_cursor_type(3), Some(3));
        assert_eq!(sqlsrv_native_cursor_type(42), Some(0));
        assert_eq!(sqlsrv_native_cursor_type(99), None);
    }

    /// Recognizes both short and header-style Informix UDT names as metadata LOBs.
    #[test]
    fn recognizes_informix_metadata_lob_names() {
        assert!(informix_metadata_is_lob("BLOB"));
        assert!(informix_metadata_is_lob("sql_infx_udt_clob"));
        assert!(!informix_metadata_is_lob("LONG VARCHAR"));
    }

    /// Preserves PDO_IBM's BOOLEAN/BIT fallthrough and binary/LOB metadata mapping.
    #[test]
    #[cfg(feature = "ibm")]
    fn recognizes_ibm_metadata_lob_types() {
        assert!(ibm_metadata_is_lob(16));
        assert!(ibm_metadata_is_lob(-7));
        assert!(ibm_metadata_is_lob(-98));
        assert!(ibm_metadata_is_lob(-370));
        assert!(!ibm_metadata_is_lob(4));
        assert!(!ibm_metadata_is_lob(12));
    }

    /// Parses SQLSRV's direct DSN while separating folded PDO credentials.
    #[test]
    #[cfg(feature = "sqlsrv")]
    fn parses_sqlsrv_dsn_options() {
        let options = parse_open_options(
            "sqlsrv:Server=localhost,1433;Database=app;Encrypt=yes;user=sa;password=p%25w",
            CliFlavor::Sqlsrv,
        )
        .unwrap();
        assert_eq!(
            options.source,
            "Server=localhost,1433;Database=app;Encrypt=yes"
        );
        assert_eq!(options.username, "sa");
        assert_eq!(options.password, "p%w");
    }

    /// Extracts SQLSRV's access token instead of leaking it into the connection string.
    #[test]
    #[cfg(feature = "sqlsrv")]
    fn parses_sqlsrv_access_token() {
        let options = parse_open_options(
            "sqlsrv:Server=tcp:example.database.windows.net;AccessToken=abc.def;ConnectionPooling=yes",
            CliFlavor::Sqlsrv,
        )
        .unwrap();
        assert_eq!(options.source, "Server=tcp:example.database.windows.net");
        assert_eq!(options.sqlsrv_access_token.as_deref(), Some(b"abc.def".as_slice()));
        assert!(!options.username_supplied);
        assert!(!options.password_supplied);
    }

    /// Reads SQLSRV pooling only from the driver manager's `[ODBC]` section.
    #[test]
    #[cfg(feature = "sqlsrv")]
    fn parses_sqlsrv_driver_manager_pooling() {
        assert_eq!(
            sqlsrv_pooling_from_ini("[Other]\nPooling=No\n[ODBC]\nPooling = Yes\n"),
            Some(true)
        );
        assert_eq!(sqlsrv_pooling_from_ini("[ODBC]\nPooling=off\n"), Some(false));
        assert_eq!(sqlsrv_pooling_from_ini("[ODBC]\nTrace=No\n"), None);
    }

    /// Encodes Microsoft's access-token structure with a byte count and UCS-2 padding.
    #[test]
    #[cfg(feature = "sqlsrv")]
    fn builds_sqlsrv_access_token_buffer() {
        let buffer = sqlsrv_access_token_buffer(b"abc");
        let bytes = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), buffer.len() * 4)
        };
        assert_eq!(u32::from_ne_bytes(bytes[..4].try_into().unwrap()), 6);
        assert_eq!(&bytes[4..10], &[b'a', 0, b'b', 0, b'c', 0]);
        assert_eq!(sqlsrv_token_fingerprint(b"abc"), 0xe71f_a219_0541_574b);
    }

    /// Parses labels, information types, column ranks, and query rank from ODBC metadata.
    #[test]
    #[cfg(feature = "sqlsrv")]
    fn parses_sqlsrv_classification_metadata() {
        let mut blob = Vec::new();
        blob.extend_from_slice(&1u16.to_ne_bytes());
        push_classification_text(&mut blob, "Secret");
        push_classification_text(&mut blob, "L1");
        blob.extend_from_slice(&1u16.to_ne_bytes());
        push_classification_text(&mut blob, "PII");
        push_classification_text(&mut blob, "I1");
        blob.extend_from_slice(&2i32.to_ne_bytes());
        blob.extend_from_slice(&1u16.to_ne_bytes());
        blob.extend_from_slice(&1u16.to_ne_bytes());
        blob.extend_from_slice(&0u16.to_ne_bytes());
        blob.extend_from_slice(&0u16.to_ne_bytes());
        blob.extend_from_slice(&1i32.to_ne_bytes());

        let parsed = parse_sqlsrv_classification_blob(&blob, true).unwrap();
        assert_eq!(parsed.query_rank, Some(2));
        assert_eq!(parsed.columns.len(), 1);
        assert_eq!(parsed.columns[0].len(), 1);
        assert_eq!(parsed.columns[0][0].label_name, "Secret");
        assert_eq!(parsed.columns[0][0].label_id, "L1");
        assert_eq!(parsed.columns[0][0].information_name, "PII");
        assert_eq!(parsed.columns[0][0].information_id, "I1");
        assert_eq!(parsed.columns[0][0].rank, Some(1));
    }

    /// Quotes SQLSRV emulated values without replacing markers inside literals.
    #[test]
    #[cfg(feature = "sqlsrv")]
    fn interpolates_sqlsrv_emulated_statement() {
        let rendered = interpolate_sqlsrv(
            "SELECT '?' AS marker, ? AS text, ? AS payload",
            &[1, 2],
            &[OdbcBind::Text(b"O'Brien".to_vec()), OdbcBind::Binary(vec![0, 255])],
            true,
        )
        .unwrap();
        assert_eq!(
            rendered,
            "SELECT '?' AS marker, N'O''Brien' AS text, 0x00FF AS payload"
        );
    }

    /// Executes binds, typed text fetches, transactions, and multiple results against a live DSN.
    #[test]
    #[ignore]
    #[cfg(feature = "odbc")]
    fn live_odbc_round_trip() {
        let dsn = std::env::var("ELEPHC_ODBC_DSN")
            .expect("ELEPHC_ODBC_DSN is required for the ignored ODBC live test");
        let mut connection = OdbcConn::open_odbc(&dsn).expect("open live ODBC connection");
        assert!(connection.exec("CREATE TEMP TABLE elephc_odbc_bridge_test (id INTEGER, name VARCHAR(40))") >= 0);

        let mut insert = OdbcStmt::new(
            &mut connection,
            1,
            "INSERT INTO elephc_odbc_bridge_test (id, name) VALUES (:id, :name)",
            0,
        )
        .expect("prepare ODBC insert");
        assert!(insert.bind_int(insert.parameter_index("id"), 7));
        assert!(insert.bind_text(insert.parameter_index("name"), "Éléphant".as_bytes().to_vec()));
        insert.execute(&mut connection).expect("execute ODBC insert");
        assert_eq!(connection.changes, 1);

        let mut select = OdbcStmt::new(
            &mut connection,
            1,
            "SELECT id, name FROM elephc_odbc_bridge_test ORDER BY id",
            0,
        )
        .expect("prepare ODBC select");
        select.execute(&mut connection).expect("execute ODBC select");
        assert_eq!(select.step(), 1);
        assert_eq!(select.column_data(0), b"7");
        assert_eq!(select.column_data(1), "Éléphant".as_bytes());

        assert!(connection.begin());
        assert_eq!(
            connection.exec("INSERT INTO elephc_odbc_bridge_test (id, name) VALUES (8, 'rollback')"),
            1
        );
        assert!(connection.rollback());

        let mut count = OdbcStmt::new(
            &mut connection,
            1,
            "SELECT COUNT(*) FROM elephc_odbc_bridge_test",
            0,
        )
        .expect("prepare ODBC count");
        count.execute(&mut connection).expect("execute ODBC count");
        assert_eq!(count.step(), 1);
        assert_eq!(count.column_data(0), b"1");

        let mut rowsets = OdbcStmt::new(&mut connection, 1, "SELECT 1; SELECT 2", 0)
            .expect("prepare ODBC rowsets");
        rowsets.execute(&mut connection).expect("execute first ODBC rowset");
        assert_eq!(rowsets.step(), 1);
        assert_eq!(rowsets.column_data(0), b"1");
        assert!(rowsets.next_rowset(&mut connection));
        assert_eq!(rowsets.step(), 1);
        assert_eq!(rowsets.column_data(0), b"2");
    }

    /// Exercises SQLPrepareW, Unicode binds/fetches, numeric typing, and identity lookup live.
    #[test]
    #[ignore]
    #[cfg(feature = "sqlsrv")]
    fn live_sqlsrv_round_trip() {
        let dsn = std::env::var("ELEPHC_SQLSRV_DSN")
            .expect("ELEPHC_SQLSRV_DSN is required for the ignored SQLSRV live test");
        let mut connection = OdbcConn::open_sqlsrv(&dsn).expect("open live SQLSRV connection");
        assert!(connection.exec(
            "CREATE TABLE #elephc_sqlsrv_bridge (id INT IDENTITY(1,1), amount DECIMAL(10,2), happened DATETIME2, label NVARCHAR(40))"
        ) >= 0);

        let mut insert = OdbcStmt::new(
            &mut connection,
            1,
            "INSERT INTO #elephc_sqlsrv_bridge(amount, happened, label) VALUES (:amount, :happened, :label)",
            0,
        )
        .expect("prepare SQLSRV insert");
        assert!(insert.bind_double(insert.parameter_index("amount"), 12.5));
        assert!(insert.bind_text(
            insert.parameter_index("happened"),
            b"2026-07-17 12:34:56".to_vec(),
        ));
        assert!(insert.bind_text(
            insert.parameter_index("label"),
            "Éléphant".as_bytes().to_vec(),
        ));
        insert.execute(&mut connection).expect("execute SQLSRV insert");
        assert_eq!(connection.last_insert_id(None), "1");

        let mut select = OdbcStmt::new(
            &mut connection,
            1,
            "SELECT id, amount, happened, label AS [libellé] FROM #elephc_sqlsrv_bridge",
            2 | (42 << 8),
        )
        .expect("prepare SQLSRV select");
        assert!(select.configure_sqlsrv_prepare_option(
            SQLSRV_ATTR_CURSOR_SCROLL_TYPE,
            42,
        ));
        assert!(select.set_sqlsrv_attribute(SQLSRV_ATTR_FETCHES_NUMERIC_TYPE, 1));
        assert!(select.set_sqlsrv_attribute(SQLSRV_ATTR_FETCHES_DATETIME_TYPE, 1));
        select.execute(&mut connection).expect("execute SQLSRV select");
        assert_eq!(select.sqlsrv_attribute(SQLSRV_ATTR_CURSOR_SCROLL_TYPE), Some(42));
        assert_eq!(select.column_name(3), "libellé");
        assert_eq!(select.step(), 1);
        assert_eq!(select.column_type(0), 1);
        assert_eq!(select.column_data(0), b"1");
        assert_eq!(select.column_data(1), b"12.50");
        assert!(select.column_is_datetime(2));
        assert!(String::from_utf8_lossy(&select.column_data(2)).starts_with("2026-07-17 12:34:56"));
        assert_eq!(select.column_data(3), "Éléphant".as_bytes());
    }
}
