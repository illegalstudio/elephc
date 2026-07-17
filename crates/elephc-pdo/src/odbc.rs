//! Purpose:
//! System CLI backend matching php-src's PDO_ODBC and PECL PDO_INFORMIX behavior.
//!
//! Called from:
//! - The PDO bridge root when built with the optional `odbc` or `informix` feature.
//!
//! Key details:
//! - Uses the ODBC 3 CLI ABI through `odbc-sys`, like both PHP extensions delegate to a driver manager.
//! - Materializes scalar result rows as text/null and preserves Informix LOB stream metadata.
//! - Keeps statement handles alive across `SQLMoreResults`, cursor-name, and scroll operations.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::{Mutex, OnceLock};

use odbc_sys::{
    AttrOdbcVersion, CDataType, CompletionType, ConnectionAttribute, Desc, DriverConnectOption,
    EnvironmentAttribute, FreeStmtOption, HDbc, HEnv, HStmt, Handle, HandleType,
    InfoType, NULL_DATA, Nullability, ParamType, SqlDataType, SqlReturn, SQLAllocHandle,
    SQLBindParameter, SQLCloseCursor, SQLColAttribute, SQLConnect, SQLDescribeCol, SQLDescribeParam, SQLDisconnect, SQLDriverConnect,
    SQLEndTran, SQLExecDirect, SQLExecute, SQLFetch, SQLFreeHandle, SQLFreeStmt,
    SQLGetData, SQLGetDiagRec, SQLGetInfo, SQLMoreResults, SQLNumParams, SQLNumResultCols,
    SQLPrepare, SQLRowCount, SQLSetConnectAttr, SQLSetEnvAttr, SQLSetStmtAttr, StatementAttribute,
};

const SQL_AUTOCOMMIT_OFF: isize = 0;
const SQL_AUTOCOMMIT_ON: isize = 1;
const SQL_CUR_USE_IF_NEEDED: i64 = 0;
const SQL_CUR_USE_ODBC: i64 = 1;
const SQL_CUR_USE_DRIVER: i64 = 2;
const SQL_INFX_ATTR_ODBC_TYPES_ONLY: i32 = 2257;
const SQL_INFX_ATTR_LO_AUTOMATIC: i32 = 2262;

unsafe extern "system" {
    /// Applies a driver-specific numeric connection attribute not modeled by `odbc-sys`.
    #[link_name = "SQLSetConnectAttr"]
    fn SQLSetConnectAttrRaw(
        connection_handle: HDbc,
        attribute: i32,
        value: *mut c_void,
        string_length: i32,
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
}

impl CliFlavor {
    /// Returns the exact PDO DSN prefix owned by this extension.
    fn dsn_prefix(self) -> &'static str {
        match self {
            #[cfg(feature = "odbc")]
            Self::Odbc => "odbc:",
            #[cfg(feature = "informix")]
            Self::Informix => "informix:",
        }
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

/// Parsed ODBC DSN and bridge-only constructor options.
struct OpenOptions {
    source: String,
    username: String,
    password: String,
    cursor_library: i64,
    assume_utf8: bool,
    auto_commit: bool,
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
    let mut cursor_library = SQL_CUR_USE_IF_NEEDED;
    let mut assume_utf8 = false;
    let mut auto_commit = true;
    for part in split_connection_fields(body) {
        let lower = part.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("user=") {
            let offset = part.len() - value.len();
            username = decode_credential(&part[offset..]);
        } else if let Some(value) = lower.strip_prefix("password=") {
            let offset = part.len() - value.len();
            password = decode_credential(&part[offset..]);
        } else if let Some(value) = lower.strip_prefix("elephc_odbc_cursor_library=") {
            cursor_library = value.parse().unwrap_or(SQL_CUR_USE_IF_NEEDED);
        } else if let Some(value) = lower.strip_prefix("elephc_odbc_assume_utf8=") {
            assume_utf8 = value != "0";
        } else if let Some(value) = lower.strip_prefix("elephc_odbc_autocommit=") {
            auto_commit = value != "0";
        } else if lower.starts_with("connect_timeout=") {
            // PDO_ODBC does not implement PDO::ATTR_TIMEOUT; the common prelude
            // folds it for network drivers, so discard it before DriverConnect.
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
        cursor_library,
        assume_utf8,
        auto_commit,
    })
}

/// Live ODBC environment/connection pair and PDO state.
pub struct OdbcConn {
    env: HEnv,
    dbc: HDbc,
    error: ErrorState,
    pub changes: i64,
    pub in_transaction: bool,
    auto_commit: bool,
    assume_utf8: bool,
    flavor: CliFlavor,
    last_insert_id: i64,
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
            if !self.env.0.is_null() {
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

    /// Opens either CLI flavor while retaining its distinct PDO identity.
    fn open(dsn: &str, flavor: CliFlavor) -> Result<Self, String> {
        remember_open_error(&ErrorState {
            sqlstate: "HY000".to_string(),
            native_code: 0,
            message: "CLI connection initialization failed".to_string(),
        });
        let options = parse_open_options(dsn, flavor)?;
        let mut env = Handle::null();
        let mut dbc = Handle::null();
        let allocated_env = unsafe { SQLAllocHandle(HandleType::Env, Handle::null(), &mut env) };
        if !succeeded(allocated_env) {
            return Err("SQLAllocHandle: ENV failed".to_string());
        }
        let set_version = unsafe {
            SQLSetEnvAttr(
                env.as_henv(),
                EnvironmentAttribute::OdbcVersion,
                AttrOdbcVersion::Odbc3.into(),
                0,
            )
        };
        if !succeeded(set_version) {
            unsafe { let _ = SQLFreeHandle(HandleType::Env, env); };
            return Err("SQLSetEnvAttr: ODBC3 failed".to_string());
        }
        let allocated_dbc = unsafe { SQLAllocHandle(HandleType::Dbc, env, &mut dbc) };
        if !succeeded(allocated_dbc) {
            unsafe { let _ = SQLFreeHandle(HandleType::Env, env); };
            return Err("SQLAllocHandle: DBC failed".to_string());
        }
        let dbc_handle = dbc.as_hdbc();
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
                let _ = SQLFreeHandle(HandleType::Env, env);
            }
            return Err(error.message);
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
                let _ = SQLFreeHandle(HandleType::Env, env);
            }
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
                let _ = SQLFreeHandle(HandleType::Env, env);
            }
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
                        let _ = SQLFreeHandle(HandleType::Env, env);
                    }
                    return Err(error.message);
                }
            }
        }
        Ok(Self {
            env: env.as_henv(),
            dbc: dbc_handle,
            error: ErrorState::default(),
            changes: 0,
            in_transaction: false,
            auto_commit: options.auto_commit,
            assume_utf8: options.assume_utf8,
            flavor,
            last_insert_id: 0,
        })
    }

    /// Returns the PDO registry identity selected when the CLI connection opened.
    pub(crate) fn driver_kind(&self) -> crate::driver::DriverKind {
        match self.flavor {
            #[cfg(feature = "odbc")]
            CliFlavor::Odbc => crate::driver::DriverKind::Odbc,
            #[cfg(feature = "informix")]
            CliFlavor::Informix => crate::driver::DriverKind::Informix,
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
        let result = unsafe { SQLExecDirect(statement_handle, sql.as_ptr(), sql.len() as i32) };
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

    /// Returns the most recent Informix SERIAL value as PDO text.
    pub fn last_insert_id(&self) -> String {
        if self.is_informix() {
            self.last_insert_id.to_string()
        } else {
            String::new()
        }
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
        match attribute {
            0 => Some(self.auto_commit as i64),
            1001 => Some(self.assume_utf8 as i64),
            _ => None,
        }
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
        }
    }

    /// Returns the connected DBMS name exposed by `PDO::ATTR_SERVER_INFO`.
    pub fn server_info(&mut self) -> String {
        self.info(InfoType::DbmsName)
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

/// Registration metadata for one CLI input/output parameter.
#[derive(Clone, Copy)]
struct OutputSpec {
    max_length: i64,
    input_output: bool,
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

/// Completed CLI output value copied before its native execution buffer expires.
#[derive(Clone)]
pub(crate) struct OdbcOutputValue {
    pub(crate) data: Option<Vec<u8>>,
    pub(crate) lob: bool,
}

/// One materialized ODBC result column.
struct OdbcColumn {
    name: String,
    wide: bool,
    lob: bool,
    metadata_lob: bool,
    scale: i64,
    table: String,
    native_type: String,
    flags: i64,
}

/// Prepared ODBC statement retaining its native handle across result sets.
pub struct OdbcStmt {
    pub conn_id: i64,
    #[cfg(feature = "odbc")]
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
        scrollable: bool,
    ) -> Result<Self, String> {
        let (translated, named_map, order, mixed) = crate::my::translate_placeholders(sql, false);
        if mixed {
            return Err("Invalid parameter number: mixed named and positional parameters".to_string());
        }
        let mut raw = Handle::null();
        if !succeeded(unsafe { SQLAllocHandle(HandleType::Stmt, connection.dbc.as_handle(), &mut raw) }) {
            connection.error = diagnostic(HandleType::Dbc, connection.dbc.as_handle(), "SQLAllocHandle: STMT");
            return Err(connection.error.message.clone());
        }
        let stmt = raw.as_hstmt();
        if scrollable {
            let configured = unsafe {
                SQLSetStmtAttr(
                    stmt,
                    StatementAttribute::CursorScrollable,
                    1isize as *mut c_void,
                    0,
                )
            };
            if !succeeded(configured) {
                let error = diagnostic(HandleType::Stmt, raw, "SQLSetStmtAttr: SQL_ATTR_CURSOR_SCROLLABLE");
                unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
                connection.error = error.clone();
                return Err(error.message);
            }
        }
        let prepared = unsafe { SQLPrepare(stmt, translated.as_ptr(), translated.len() as i32) };
        if !succeeded(prepared) {
            let error = diagnostic(HandleType::Stmt, raw, "SQLPrepare");
            unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
            connection.error = error.clone();
            return Err(error.message);
        }
        let slots = order.iter().copied().max().unwrap_or(0).max(0) as usize;
        let mut native_params = 0i16;
        if succeeded(unsafe { SQLNumParams(stmt, &mut native_params) }) && native_params as usize != order.len() {
            unsafe { let _ = SQLFreeHandle(HandleType::Stmt, raw); };
            return Err("Invalid parameter number: number of bound variables does not match number of tokens".to_string());
        }
        Ok(Self {
            conn_id,
            #[cfg(feature = "odbc")]
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
        if base_type == 3 {
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
        let mut payloads = Vec::with_capacity(self.order.len());
        let mut descriptors = Vec::with_capacity(self.order.len());
        for (occurrence, slot) in self.order.iter().enumerate() {
            let slot = usize::try_from(*slot).ok().and_then(|slot| slot.checked_sub(1)).unwrap_or(0);
            let fallback_type = match &self.binds[slot] {
                OdbcBind::Int(_) => SqlDataType::INTEGER,
                OdbcBind::Binary(_) => SqlDataType::EXT_LONG_VAR_BINARY,
                _ => SqlDataType::EXT_LONG_VARCHAR,
            };
            let mut sql_type = fallback_type;
            let mut column_size = 4000usize;
            let mut scale = 5i16;
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
                column_size = match &self.binds[slot] {
                    OdbcBind::Text(value) | OdbcBind::Binary(value) => value.len().max(4000),
                    _ => 4000,
                };
                scale = 5;
            }
            let wide = self.assume_utf8
                && matches!(
                    sql_type,
                    SqlDataType::EXT_W_CHAR
                        | SqlDataType::EXT_W_VARCHAR
                        | SqlDataType::EXT_W_LONG_VARCHAR
                );
            let (mut payload, c_type, indicator) = match &self.binds[slot] {
                OdbcBind::Null => (Vec::new(), CDataType::Char, NULL_DATA),
                OdbcBind::Int(value) => {
                    let text = value.to_string();
                    (text.into_bytes(), CDataType::Char, 0)
                }
                OdbcBind::Double(value) => {
                    let text = value.to_string();
                    (text.into_bytes(), CDataType::Char, 0)
                }
                OdbcBind::Text(value) if wide => {
                    let payload = String::from_utf8(value.clone()).map_or_else(
                        |_| value.clone(),
                        |text| {
                            text.encode_utf16()
                                .flat_map(u16::to_ne_bytes)
                                .collect::<Vec<_>>()
                        },
                    );
                    (payload, CDataType::Binary, 0)
                }
                OdbcBind::Text(value) => (value.clone(), CDataType::Char, 0),
                OdbcBind::Binary(value) => (value.clone(), CDataType::Binary, 0),
            };
            let mut input_length = payload.len();
            if let Some(output) = self.output_specs[slot] {
                input_length = prepare_output_buffer(&mut payload, output, column_size);
            }
            let indicator = if indicator == NULL_DATA {
                NULL_DATA
            } else {
                input_length as isize
            };
            payloads.push(payload);
            descriptors.push((c_type, sql_type, column_size, scale, indicator, wide));
        }
        self.indicators.clear();
        self.indicators.extend(descriptors.iter().map(|descriptor| descriptor.4));
        for (occurrence, (c_type, sql_type, column_size, scale, _, _)) in
            descriptors.iter().copied().enumerate()
        {
            let payload = &mut payloads[occurrence];
            let slot = usize::try_from(self.order[occurrence])
                .ok()
                .and_then(|slot| slot.checked_sub(1))
                .unwrap_or(0);
            let pointer = if self.indicators[occurrence] == NULL_DATA
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
                    payload.len() as isize,
                    &mut self.indicators[occurrence],
                )
            };
            if !succeeded(result) {
                self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLBindParameter");
                connection.error = self.error.clone();
                return Err(self.error.message.clone());
            }
        }
        let result = unsafe { SQLExecute(self.stmt) };
        if result != SqlReturn::NO_DATA && !succeeded(result) {
            self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLExecute");
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        let execution_info = if result == SqlReturn::SUCCESS_WITH_INFO
            && !connection.is_informix()
        {
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
            self.output_values[slot] = Some(OdbcOutputValue { data, lob: false });
        }
        self.sent_sql.clear();
        self.materialize_current_result(connection)?;
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

    /// Describes and materializes the active native result set.
    fn materialize_current_result(&mut self, connection: &mut OdbcConn) -> Result<(), String> {
        self.columns.clear();
        self.rows.clear();
        self.cursor = -1;
        let mut count = 0i16;
        if !succeeded(unsafe { SQLNumResultCols(self.stmt, &mut count) }) {
            self.error = diagnostic(HandleType::Stmt, self.stmt.as_handle(), "SQLNumResultCols");
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        for index in 1..=count {
            let mut name = [0u8; 256];
            let mut name_len = 0i16;
            let mut data_type = SqlDataType::UNKNOWN_TYPE;
            let mut size = 0usize;
            let mut scale = 0i16;
            let mut nullable = Nullability::UNKNOWN;
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
            let native_type = if informix {
                column_text_attribute(self.stmt, index as u16, Desc::TypeName).unwrap_or_default()
            } else {
                String::new()
            };
            let lob = informix
                && (matches!(
                    data_type,
                    SqlDataType::EXT_LONG_VARCHAR
                        | SqlDataType::EXT_BINARY
                        | SqlDataType::EXT_VAR_BINARY
                        | SqlDataType::EXT_LONG_VAR_BINARY
                ) || data_type.0 == 17);
            let metadata_lob = informix && informix_metadata_is_lob(&native_type);
            let mut flags = 0;
            if nullable == Nullability::NO_NULLS {
                flags |= 1;
            }
            if informix
                && column_numeric_attribute(self.stmt, index as u16, Desc::Unsigned)
                    .is_some_and(|value| value != 0)
            {
                flags |= 2;
            }
            if informix
                && column_numeric_attribute(self.stmt, index as u16, Desc::AutoUniqueValue)
                    .is_some_and(|value| value != 0)
            {
                flags |= 4;
            }
            self.columns.push(OdbcColumn {
                name: String::from_utf8_lossy(&name[..usize::try_from(name_len).unwrap_or(0).min(name.len())]).into_owned(),
                wide: self.assume_utf8
                    && matches!(
                        data_type,
                        SqlDataType::EXT_W_CHAR
                            | SqlDataType::EXT_W_VARCHAR
                            | SqlDataType::EXT_W_LONG_VARCHAR
                    ),
                lob,
                metadata_lob,
                scale: i64::from(scale),
                table: if informix {
                    column_text_attribute(self.stmt, index as u16, Desc::BaseTableName)
                        .unwrap_or_default()
                } else {
                    String::new()
                },
                native_type,
                flags,
            });
        }
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
                row.push(self.read_column(index as u16, wide)?);
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
                    if wide { CDataType::Binary } else { CDataType::Char },
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
            let capacity = if wide { chunk.len() } else { chunk.len() - 1 };
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
            .is_some_and(|column| column.metadata_lob)
        {
            3
        } else {
            2
        }
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
            },
            4000,
        );
        assert_eq!(input_length, 4);
        assert_eq!(payload, b"AAAA");
    }

    /// Recognizes both short and header-style Informix UDT names as metadata LOBs.
    #[test]
    fn recognizes_informix_metadata_lob_names() {
        assert!(informix_metadata_is_lob("BLOB"));
        assert!(informix_metadata_is_lob("sql_infx_udt_clob"));
        assert!(!informix_metadata_is_lob("LONG VARCHAR"));
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
            false,
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
            false,
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
            false,
        )
        .expect("prepare ODBC count");
        count.execute(&mut connection).expect("execute ODBC count");
        assert_eq!(count.step(), 1);
        assert_eq!(count.column_data(0), b"1");

        let mut rowsets = OdbcStmt::new(&mut connection, 1, "SELECT 1; SELECT 2", false)
            .expect("prepare ODBC rowsets");
        rowsets.execute(&mut connection).expect("execute first ODBC rowset");
        assert_eq!(rowsets.step(), 1);
        assert_eq!(rowsets.column_data(0), b"1");
        assert!(rowsets.next_rowset(&mut connection));
        assert_eq!(rowsets.step(), 1);
        assert_eq!(rowsets.column_data(0), b"2");
    }
}
