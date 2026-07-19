//! Purpose:
//! Optional PDO_OCI backend implemented through Oracle Instant Client and ODPI-C.
//!
//! Called from:
//! - `crate` connection/statement dispatch when the `oci` Cargo feature is selected.
//!
//! Key details:
//! - DSN, autocommit, prefetch, diagnostics, metadata, and session attributes mirror PDO_OCI 1.2.
//! - Oracle scalar results remain strings; LOB results are tagged for PHP stream materialization.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::Duration;

use oracle::sql_type::{Blob, OracleType, ToSql};
use oracle::{Connection, Error, Statement, StatementType, Version};

const ATTR_AUTOCOMMIT: i64 = 0;
const ATTR_PREFETCH: i64 = 1;
const OCI_ATTR_ACTION: i64 = 1000;
const OCI_ATTR_CLIENT_INFO: i64 = 1001;
const OCI_ATTR_CLIENT_IDENTIFIER: i64 = 1002;
const OCI_ATTR_MODULE: i64 = 1003;
const OCI_ATTR_CALL_TIMEOUT: i64 = 1004;
const DEFAULT_PREFETCH: u32 = 100;

/// One PDO-compatible Oracle error snapshot.
#[derive(Clone, Debug)]
struct ErrorState {
    sqlstate: String,
    native_code: i64,
    message: String,
}

impl Default for ErrorState {
    /// Returns PDO's no-error state.
    fn default() -> Self {
        Self {
            sqlstate: "00000".to_string(),
            native_code: 0,
            message: String::new(),
        }
    }
}

impl ErrorState {
    /// Converts an Oracle/ODPI diagnostic into PDO_OCI's SQLSTATE mapping.
    fn from_oracle(operation: &str, error: &Error) -> Self {
        let native_code = error.db_error().map_or(0, |error| i64::from(error.code()));
        Self {
            sqlstate: sqlstate_for_code(native_code).to_string(),
            native_code,
            message: format!("{operation}: {error}"),
        }
    }
}

/// Recovers PDO_OCI's SQLSTATE and native ORA code from a failed-open message.
pub(crate) fn open_diagnostic(message: &str) -> (&'static str, i64) {
    let native_code = message
        .match_indices("ORA-")
        .find_map(|(index, _)| {
            message
                .get(index + 4..index + 9)
                .and_then(|digits| digits.parse::<i64>().ok())
        })
        .unwrap_or(0);
    (sqlstate_for_code(native_code), native_code)
}

/// Maps Oracle native diagnostics through PDO_OCI's maintained SQLSTATE table.
fn sqlstate_for_code(native_code: i64) -> &'static str {
    match native_code {
        12154 => "42S02",
        22 | 378 | 602 | 603 | 604 | 609 | 1012 | 1033 | 1041 | 1043 | 1089 | 1090
        | 1092 | 3113 | 3114 | 3122 | 3135 | 12153 | 27146 | 28511 => "01002",
        _ => "HY000",
    }
}

/// Parsed PDO_OCI connection settings.
#[derive(Debug, Eq, PartialEq)]
struct OpenOptions {
    dbname: String,
    username: String,
    password: String,
    charset: Option<String>,
    auto_commit: bool,
}

/// Percent-decodes constructor credentials serialized by the PHP prelude.
fn decode_credential(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Parses php-src's `oci:dbname=...;charset=...` DSN and bridge-only fields.
fn parse_dsn(dsn: &str) -> Result<OpenOptions, String> {
    let body = dsn
        .strip_prefix("oci:")
        .ok_or_else(|| "could not find driver".to_string())?;
    let mut options = OpenOptions {
        dbname: String::new(),
        username: String::new(),
        password: String::new(),
        charset: None,
        auto_commit: true,
    };
    for field in body.split(';').filter(|field| !field.is_empty()) {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "dbname" => options.dbname = value.to_string(),
            "charset" => options.charset = Some(value.to_string()),
            "user" => options.username = decode_credential(value),
            "password" => options.password = decode_credential(value),
            "elephc_oci_autocommit" => options.auto_commit = value != "0",
            // Common constructor plumbing appends these for other network drivers.
            "connect_timeout" | "elephc_odbc_cursor_library" | "elephc_odbc_assume_utf8"
            | "elephc_odbc_autocommit" => {}
            _ => {}
        }
    }
    if let Some(charset) = options.charset.as_deref() {
        if !matches!(charset.to_ascii_uppercase().as_str(), "AL32UTF8" | "UTF8") {
            return Err(format!(
                "OCIEnvNlsCreate: character set {charset} is unavailable through the UTF-8 ODPI-C environment"
            ));
        }
    }
    Ok(options)
}

/// Live Oracle connection and PDO-visible state.
pub struct OciConn {
    connection: Connection,
    error: ErrorState,
    pub changes: i64,
    pub in_transaction: bool,
    auto_commit: bool,
    prefetch: u32,
}

impl OciConn {
    /// Opens an Oracle session using PDO_OCI DSN and credential precedence.
    pub fn open(dsn: &str) -> Result<Self, String> {
        let options = parse_dsn(dsn)?;
        let mut connection = Connection::connect(
            &options.username,
            &options.password,
            &options.dbname,
        )
        .map_err(|error| ErrorState::from_oracle("pdo_oci_handle_factory", &error).message)?;
        connection.set_autocommit(options.auto_commit);
        Ok(Self {
            connection,
            error: ErrorState::default(),
            changes: 0,
            in_transaction: false,
            auto_commit: options.auto_commit,
            prefetch: DEFAULT_PREFETCH,
        })
    }

    /// Performs PDO_OCI's OCIPing-equivalent persistent-connection probe.
    pub fn is_alive(&mut self) -> bool {
        match self.connection.ping() {
            Ok(()) => true,
            Err(error) if error.db_error().is_some_and(|error| error.code() == 1010) => true,
            Err(error) => {
                self.error = ErrorState::from_oracle("OCIPing", &error);
                false
            }
        }
    }

    /// Executes a non-SELECT statement and returns its affected-row count.
    pub fn exec(&mut self, sql: &str) -> i64 {
        match self.connection.execute(sql, &[]) {
            Ok(statement) => {
                self.changes = statement.row_count().unwrap_or(0) as i64;
                self.error = ErrorState::default();
                self.changes
            }
            Err(error) => {
                self.error = ErrorState::from_oracle("OCIStmtExecute", &error);
                self.changes = 0;
                -1
            }
        }
    }

    /// Starts PDO's tracked Oracle transaction without issuing SQL.
    pub fn begin(&mut self) -> bool {
        if self.in_transaction {
            return false;
        }
        self.connection.set_autocommit(false);
        self.in_transaction = true;
        true
    }

    /// Commits the active Oracle transaction and restores configured autocommit.
    pub fn commit(&mut self) -> bool {
        match self.connection.commit() {
            Ok(()) => {
                self.in_transaction = false;
                self.connection.set_autocommit(self.auto_commit);
                self.error = ErrorState::default();
                true
            }
            Err(error) => {
                self.error = ErrorState::from_oracle("OCITransCommit", &error);
                false
            }
        }
    }

    /// Rolls back the active Oracle transaction and restores configured autocommit.
    pub fn rollback(&mut self) -> bool {
        match self.connection.rollback() {
            Ok(()) => {
                self.in_transaction = false;
                self.connection.set_autocommit(self.auto_commit);
                self.error = ErrorState::default();
                true
            }
            Err(error) => {
                self.error = ErrorState::from_oracle("OCITransRollback", &error);
                false
            }
        }
    }

    /// Applies an integer-valued PDO_OCI connection attribute.
    pub fn set_attribute_int(&mut self, attribute: i64, value: i64) -> bool {
        match attribute {
            ATTR_AUTOCOMMIT => {
                if self.in_transaction && !self.commit() {
                    return false;
                }
                self.auto_commit = value != 0;
                self.connection.set_autocommit(self.auto_commit);
                true
            }
            ATTR_PREFETCH => {
                self.prefetch = sanitize_prefetch(value);
                true
            }
            OCI_ATTR_CALL_TIMEOUT => {
                let timeout = u64::from(value as u32);
                match self.connection.set_call_timeout(Some(Duration::from_millis(timeout))) {
                    Ok(()) => true,
                    Err(error) => {
                        self.error = ErrorState::from_oracle("OCIAttrSet: OCI_ATTR_CALL_TIMEOUT", &error);
                        false
                    }
                }
            }
            _ => false,
        }
    }

    /// Applies one string-valued PDO_OCI session attribute.
    pub fn set_attribute_text(&mut self, attribute: i64, value: &str) -> bool {
        let result = match attribute {
            OCI_ATTR_ACTION => self.connection.set_action(value),
            OCI_ATTR_CLIENT_INFO => self.connection.set_client_info(value),
            OCI_ATTR_CLIENT_IDENTIFIER => self.connection.set_client_identifier(value),
            OCI_ATTR_MODULE => self.connection.set_module(value),
            _ => return false,
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.error = ErrorState::from_oracle("OCIAttrSet", &error);
                false
            }
        }
    }

    /// Reads an integer-valued PDO_OCI connection attribute.
    pub fn attribute_int(&mut self, attribute: i64) -> Option<i64> {
        match attribute {
            ATTR_AUTOCOMMIT => Some(self.auto_commit as i64),
            ATTR_PREFETCH => Some(i64::from(self.prefetch)),
            OCI_ATTR_CALL_TIMEOUT => match self.connection.call_timeout() {
                Ok(timeout) => Some(timeout.map_or(0, |timeout| timeout.as_millis() as i64)),
                Err(error) => {
                    self.error = ErrorState::from_oracle("OCIAttrGet: OCI_ATTR_CALL_TIMEOUT", &error);
                    None
                }
            },
            _ => None,
        }
    }

    /// Returns the Oracle server version tuple.
    pub fn server_version(&mut self) -> String {
        match self.connection.server_version() {
            Ok((version, _)) => version.to_string(),
            Err(error) => {
                self.error = ErrorState::from_oracle("OCIServerRelease", &error);
                "<<Unknown>>".to_string()
            }
        }
    }

    /// Returns Oracle's server release banner.
    pub fn server_info(&mut self) -> String {
        match self.connection.server_version() {
            Ok((_, banner)) => banner,
            Err(error) => {
                self.error = ErrorState::from_oracle("OCIServerRelease", &error);
                "<<Unknown>>".to_string()
            }
        }
    }

    /// Returns the loaded Oracle Instant Client version.
    pub fn client_version(&self) -> String {
        Version::client().map_or_else(|_| String::new(), |version| version.to_string())
    }

    /// Returns the configured prepare-time prefetch row count.
    pub fn prefetch(&self) -> u32 {
        self.prefetch
    }

    /// Returns the current connection SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        &self.error.sqlstate
    }

    /// Returns the current native Oracle code.
    pub fn errcode(&self) -> i64 {
        self.error.native_code
    }

    /// Returns the current Oracle diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }
}

/// One bound PDO_OCI value; non-LOB values are intentionally sent as strings.
#[derive(Clone)]
enum OciBind {
    Unbound,
    Null,
    Text(String),
    Blob(Vec<u8>),
}

/// Metadata retained for one Oracle result column.
#[derive(Clone)]
struct OciColumn {
    name: String,
    oracle_type: OracleType,
    nullable: bool,
}

/// One materialized Oracle result cell.
#[derive(Clone)]
pub(crate) struct OciCell {
    pub(crate) data: Option<Vec<u8>>,
    pub(crate) lob: bool,
}

/// One PDO_OCI output-bind declaration retained until native execution.
#[derive(Clone)]
struct OciOutputSpec {
    lob: bool,
    max_length: u32,
}

/// Prepared PDO_OCI statement with buffered result rows.
pub struct OciStmt {
    pub conn_id: i64,
    native_sql: String,
    named_map: HashMap<String, i64>,
    binds: Vec<OciBind>,
    output_specs: Vec<Option<OciOutputSpec>>,
    output_values: Vec<Option<OciCell>>,
    columns: Vec<OciColumn>,
    rows: Vec<Vec<OciCell>>,
    cursor: isize,
    executed: bool,
    row_count: i64,
    prefetch: u32,
    pub sent_sql: String,
    error: ErrorState,
}

impl OciStmt {
    /// Validates and records a native Oracle statement and PDO placeholders.
    pub fn new(connection: &mut OciConn, conn_id: i64, sql: &str) -> Result<Self, String> {
        let (translated, named_map, order, mixed) = crate::my::translate_placeholders(sql, false);
        if mixed {
            return Err("Invalid parameter number: mixed named and positional parameters".to_string());
        }
        let native_sql = translate_oracle_placeholders(&translated, &order)?;
        if let Err(error) = connection.connection.statement(&native_sql).build() {
            connection.error = ErrorState::from_oracle("OCIStmtPrepare", &error);
            return Err(connection.error.message.clone());
        }
        let slots = order.iter().copied().max().unwrap_or(0).max(0) as usize;
        Ok(Self {
            conn_id,
            native_sql,
            named_map,
            binds: vec![OciBind::Unbound; slots],
            output_specs: vec![None; slots],
            output_values: vec![None; slots],
            columns: Vec::new(),
            rows: Vec::new(),
            cursor: -1,
            executed: false,
            row_count: 0,
            prefetch: connection.prefetch(),
            sent_sql: String::new(),
            error: ErrorState::default(),
        })
    }

    /// Resolves a named placeholder to its one-based PDO slot.
    pub fn parameter_index(&self, name: &str) -> i64 {
        self.named_map.get(name.trim_start_matches(':')).copied().unwrap_or(-1)
    }

    /// Stores one value in a one-based PDO bind slot.
    fn bind(&mut self, index: i64, value: OciBind) -> bool {
        let Some(slot) = usize::try_from(index).ok().and_then(|index| index.checked_sub(1)) else {
            return false;
        };
        let Some(target) = self.binds.get_mut(slot) else {
            return false;
        };
        *target = value;
        true
    }

    /// Binds an integer using PDO_OCI's string conversion.
    pub fn bind_int(&mut self, index: i64, value: i64) -> bool {
        self.bind(index, OciBind::Text(value.to_string()))
    }

    /// Binds a floating-point value using PDO_OCI's string conversion.
    pub fn bind_double(&mut self, index: i64, value: f64) -> bool {
        self.bind(index, OciBind::Text(value.to_string()))
    }

    /// Binds text bytes using the compiled PHP string encoding.
    pub fn bind_text(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.bind(index, OciBind::Text(String::from_utf8_lossy(&value).into_owned()))
    }

    /// Binds a temporary Oracle BLOB.
    pub fn bind_blob(&mut self, index: i64, value: Vec<u8>) -> bool {
        self.bind(index, OciBind::Blob(value))
    }

    /// Binds SQL NULL.
    pub fn bind_null(&mut self, index: i64) -> bool {
        self.bind(index, OciBind::Null)
    }

    /// Marks one bind as an OCI input/output value with PDO's buffer-size rules.
    pub fn bind_output(&mut self, index: i64, pdo_type: i64, max_length: i64) -> bool {
        let Some(slot) = usize::try_from(index).ok().and_then(|index| index.checked_sub(1)) else {
            return false;
        };
        let Some(target) = self.output_specs.get_mut(slot) else {
            return false;
        };
        let max_length = if max_length <= 0 {
            1332
        } else {
            u32::try_from(max_length).unwrap_or(u32::MAX)
        };
        *target = Some(OciOutputSpec {
            lob: (pdo_type & 0xFFFF) == 3,
            max_length,
        });
        true
    }

    /// Overrides the statement prefetch row count before execution.
    pub fn set_prefetch(&mut self, value: i64) -> i64 {
        if self.executed {
            return 0;
        }
        self.prefetch = sanitize_prefetch(value);
        1
    }

    /// Resets result/cursor state while preserving bound parameters.
    pub fn reset(&mut self) {
        self.columns.clear();
        self.rows.clear();
        self.cursor = -1;
        self.executed = false;
        self.row_count = 0;
        self.output_values.fill(None);
    }

    /// Clears result state and all bound values.
    pub fn clear_bindings(&mut self) {
        self.reset();
        self.binds.fill(OciBind::Unbound);
        self.output_specs.fill(None);
    }

    /// Reports whether the statement still needs native execution.
    pub fn needs_execute(&self) -> bool {
        !self.executed
    }

    /// Executes and fully materializes one Oracle result set.
    pub fn execute(&mut self, connection: &mut OciConn) -> Result<(), String> {
        if self.binds.iter().any(|value| matches!(value, OciBind::Unbound)) {
            self.error = ErrorState {
                sqlstate: "HY093".to_string(),
                native_code: 0,
                message: "Invalid parameter number: number of bound variables does not match number of tokens".to_string(),
            };
            return Err(self.error.message.clone());
        }
        let mut builder = connection.connection.statement(&self.native_sql);
        builder.prefetch_rows(self.prefetch);
        let mut statement = builder.build().map_err(|error| {
            self.error = ErrorState::from_oracle("OCIStmtPrepare", &error);
            self.error.message.clone()
        })?;
        bind_statement_values(
            &connection.connection,
            &mut statement,
            &self.binds,
            &self.output_specs,
        )
        .map_err(|message| {
            self.error = ErrorState {
                sqlstate: "HY000".to_string(),
                native_code: 0,
                message: format!("OCIBind: {message}"),
            };
            self.error.message.clone()
        })?;
        self.columns.clear();
        self.rows.clear();
        self.cursor = -1;
        let result = if statement.statement_type() == StatementType::Select {
            self.materialize_query(&mut statement, &[])
        } else {
            statement.execute(&[]).map(|()| {
                self.row_count = statement.row_count().unwrap_or(0) as i64;
            })
        };
        if let Err(error) = result {
            self.error = ErrorState::from_oracle("OCIStmtExecute", &error);
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        if let Err(message) = self.capture_output_values(&statement) {
            self.error = ErrorState {
                sqlstate: "HY000".to_string(),
                native_code: 0,
                message: format!("OCIBind output: {message}"),
            };
            connection.error = self.error.clone();
            return Err(self.error.message.clone());
        }
        connection.changes = self.row_count;
        connection.error = ErrorState::default();
        self.error = ErrorState::default();
        self.sent_sql.clear();
        self.executed = true;
        Ok(())
    }

    /// Copies native OCI output buffers into bridge-owned scalar or LOB bytes.
    fn capture_output_values(&mut self, statement: &Statement) -> Result<(), String> {
        let returning = matches!(
            statement.statement_type(),
            StatementType::Insert
                | StatementType::Update
                | StatementType::Delete
                | StatementType::Merge
        );
        for (slot, spec) in self.output_specs.iter().enumerate() {
            let Some(spec) = spec else {
                continue;
            };
            let index = slot + 1;
            let data = if spec.lob {
                read_output_blob(statement, index, returning)?
            } else {
                read_output_text(statement, index, returning)?.map(String::into_bytes)
            };
            self.output_values[slot] = Some(OciCell {
                data,
                lob: spec.lob,
            });
        }
        Ok(())
    }

    /// Returns one completed output bind for PHP-side reference synchronization.
    pub(crate) fn output_value(&self, index: i64) -> Option<&OciCell> {
        usize::try_from(index)
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|slot| self.output_values.get(slot))
            .and_then(Option::as_ref)
    }

    /// Buffers the query rows and php-src-compatible Oracle metadata.
    fn materialize_query(
        &mut self,
        statement: &mut Statement,
        parameters: &[&dyn ToSql],
    ) -> oracle::Result<()> {
        let rows = statement.query(parameters)?;
        self.columns = rows
            .column_info()
            .iter()
            .map(|column| OciColumn {
                name: column.name().to_string(),
                oracle_type: column.oracle_type().clone(),
                nullable: column.nullable(),
            })
            .collect();
        for result in rows {
            let row = result?;
            let mut output = Vec::with_capacity(self.columns.len());
            for (index, column) in self.columns.iter().enumerate() {
                let lob = matches!(
                    column.oracle_type,
                    OracleType::BLOB | OracleType::CLOB | OracleType::NCLOB | OracleType::BFILE
                );
                let data = match column.oracle_type {
                    OracleType::BLOB | OracleType::BFILE => row.get::<_, Option<Vec<u8>>>(index)?,
                    OracleType::Raw(_) | OracleType::LongRaw => row
                        .get::<_, Option<Vec<u8>>>(index)?
                        .map(|value| uppercase_hex(&value).into_bytes()),
                    _ => row.get::<_, Option<String>>(index)?.map(String::into_bytes),
                };
                output.push(OciCell { data, lob });
            }
            self.rows.push(output);
        }
        // PDO_OCI snapshots OCI_ATTR_ROW_COUNT at execute time, before any SELECT
        // fetch, so SELECT statements keep rowCount() at zero.
        self.row_count = 0;
        Ok(())
    }

    /// Advances to the next buffered result row.
    pub fn step(&mut self) -> i64 {
        let next = self.cursor + 1;
        if next < self.rows.len() as isize {
            self.cursor = next;
            1
        } else {
            0
        }
    }

    /// Selects a buffered row with PDO's scroll-orientation semantics.
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

    /// Returns the active Oracle column count.
    pub fn column_count(&self) -> i64 {
        self.columns.len() as i64
    }

    /// Returns one Oracle column name.
    pub fn column_name(&self, index: i64) -> String {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.columns.get(index))
            .map_or_else(String::new, |column| column.name.clone())
    }

    /// Returns the bridge cell tag: string, LOB stream, or NULL.
    pub fn column_type(&self, index: i64) -> i64 {
        match self.cell(index) {
            Some(OciCell { data: None, .. }) | None => 5,
            Some(OciCell { lob: true, .. }) => 4,
            Some(_) => 3,
        }
    }

    /// Parses the current Oracle string cell as integer.
    pub fn column_int(&self, index: i64) -> i64 {
        String::from_utf8_lossy(&self.column_data(index)).parse().unwrap_or(0)
    }

    /// Parses the current Oracle string cell as floating point.
    pub fn column_double(&self, index: i64) -> f64 {
        String::from_utf8_lossy(&self.column_data(index)).parse().unwrap_or(0.0)
    }

    /// Returns the exact bytes for the current Oracle cell.
    pub fn column_data(&self, index: i64) -> Vec<u8> {
        self.cell(index).and_then(|cell| cell.data.clone()).unwrap_or_default()
    }

    /// Returns php-src's OCI declared/native type name.
    pub fn column_native_type(&self, index: i64) -> String {
        self.column(index).map_or_else(String::new, |column| oracle_type_name(&column.oracle_type))
    }

    /// Returns PDO_PARAM_LOB for Oracle LOB columns and PDO_PARAM_STR otherwise.
    pub fn column_pdo_type(&self, index: i64) -> i64 {
        self.column(index).map_or(2, |column| {
            if matches!(
                column.oracle_type,
                OracleType::BLOB | OracleType::CLOB | OracleType::NCLOB | OracleType::BFILE
            ) {
                3
            } else {
                2
            }
        })
    }

    /// Returns the declared numeric scale or zero for non-NUMBER columns.
    pub fn column_scale(&self, index: i64) -> i64 {
        self.column(index).map_or(0, |column| match column.oracle_type {
            OracleType::Number(_, scale) => i64::from(scale),
            _ => 0,
        })
    }

    /// Returns nullable/blob metadata flags as bridge bits.
    pub fn column_flags(&self, index: i64) -> i64 {
        self.column(index).map_or(0, |column| {
            let mut flags = if column.nullable { 1 } else { 2 };
            if matches!(
                column.oracle_type,
                OracleType::BLOB | OracleType::CLOB | OracleType::NCLOB | OracleType::BFILE
            ) {
                flags |= 4;
            }
            flags
        })
    }

    /// Returns one stored Oracle column.
    fn column(&self, index: i64) -> Option<&OciColumn> {
        usize::try_from(index).ok().and_then(|index| self.columns.get(index))
    }

    /// Returns one current Oracle cell.
    fn cell(&self, index: i64) -> Option<&OciCell> {
        let row = usize::try_from(self.cursor).ok().and_then(|row| self.rows.get(row))?;
        usize::try_from(index).ok().and_then(|index| row.get(index))
    }

    /// Returns the statement SQLSTATE.
    pub fn sqlstate(&self) -> &str {
        &self.error.sqlstate
    }

    /// Returns the statement native Oracle code.
    pub fn errcode(&self) -> i64 {
        self.error.native_code
    }

    /// Returns the statement Oracle diagnostic text.
    pub fn errmsg(&self) -> &str {
        &self.error.message
    }
}

/// Rewrites normalized question-mark placeholders into repeatable Oracle names.
fn translate_oracle_placeholders(sql: &str, order: &[i64]) -> Result<String, String> {
    let mut translated = String::with_capacity(sql.len() + order.len() * 16);
    let mut occurrence = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            let Some(slot) = order.get(occurrence) else {
                return Err("Invalid parameter number".to_string());
            };
            translated.push_str(":elephc_pdo_");
            translated.push_str(&slot.to_string());
            occurrence += 1;
        } else {
            translated.push(ch);
        }
    }
    if occurrence != order.len() {
        return Err("Invalid parameter number".to_string());
    }
    Ok(translated)
}

/// Applies PDO_OCI's negative/overflow prefetch sanitization.
fn sanitize_prefetch(value: i64) -> u32 {
    if value < 0 {
        0
    } else if value > i64::from(u32::MAX / 1024) {
        DEFAULT_PREFETCH
    } else {
        value as u32
    }
}

/// Copies each retained PDO value into rust-oracle's native statement buffers.
fn bind_statement_values(
    connection: &Connection,
    statement: &mut Statement,
    binds: &[OciBind],
    output_specs: &[Option<OciOutputSpec>],
) -> Result<(), String> {
    for (slot, bind) in binds.iter().enumerate() {
        let index = slot + 1;
        match (bind, output_specs.get(slot).and_then(Option::as_ref)) {
            (OciBind::Unbound | OciBind::Null, Some(spec)) if spec.lob => statement
                .bind(index, &OracleType::BLOB)
                .map_err(|error| error.to_string())?,
            (OciBind::Blob(value), Some(spec)) if spec.lob => {
                let mut blob = Blob::new(connection).map_err(|error| error.to_string())?;
                blob.write_all(value).map_err(|error| error.to_string())?;
                statement
                    .bind(index, &(&blob, &OracleType::BLOB))
                    .map_err(|error| error.to_string())?;
            }
            (OciBind::Text(value), Some(spec)) if spec.lob => {
                let mut blob = Blob::new(connection).map_err(|error| error.to_string())?;
                blob.write_all(value.as_bytes()).map_err(|error| error.to_string())?;
                statement
                    .bind(index, &(&blob, &OracleType::BLOB))
                    .map_err(|error| error.to_string())?;
            }
            (OciBind::Unbound | OciBind::Null, Some(spec)) => statement
                .bind(index, &OracleType::Varchar2(spec.max_length))
                .map_err(|error| error.to_string())?,
            (OciBind::Text(value), Some(spec)) => statement
                .bind(index, &(value, &OracleType::Varchar2(spec.max_length)))
                .map_err(|error| error.to_string())?,
            (OciBind::Blob(value), Some(spec)) => {
                let value = String::from_utf8_lossy(value).into_owned();
                statement
                    .bind(index, &(&value, &OracleType::Varchar2(spec.max_length)))
                    .map_err(|error| error.to_string())?;
            }
            (OciBind::Unbound | OciBind::Null, None) => statement
                .bind(index, &Option::<String>::None)
                .map_err(|error| error.to_string())?,
            (OciBind::Text(value), None) => statement
                .bind(index, value)
                .map_err(|error| error.to_string())?,
            (OciBind::Blob(value), None) => {
                let mut blob = Blob::new(connection).map_err(|error| error.to_string())?;
                blob.write_all(value).map_err(|error| error.to_string())?;
                statement.bind(index, &blob).map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

/// Reads one scalar OCI output bind, including DML `RETURNING INTO` arrays.
fn read_output_text(
    statement: &Statement,
    index: usize,
    returning: bool,
) -> Result<Option<String>, String> {
    if returning {
        let values: Vec<Option<String>> = statement
            .returned_values(index)
            .map_err(|error| error.to_string())?;
        if let Some(value) = values.into_iter().last() {
            return Ok(value);
        }
    }
    statement.bind_value(index).map_err(|error| error.to_string())
}

/// Reads one OCI LOB output bind and drains the locator into owned bytes.
fn read_output_blob(
    statement: &Statement,
    index: usize,
    returning: bool,
) -> Result<Option<Vec<u8>>, String> {
    let mut blob = if returning {
        let values: Vec<Option<Blob>> = statement
            .returned_values(index)
            .map_err(|error| error.to_string())?;
        values.into_iter().last().flatten()
    } else {
        statement.bind_value(index).map_err(|error| error.to_string())?
    };
    let Some(blob) = blob.as_mut() else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    blob.read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    Ok(Some(bytes))
}

/// Maps rust-oracle's precise type enum to PDO_OCI's metadata spelling.
fn oracle_type_name(oracle_type: &OracleType) -> String {
    match oracle_type {
        OracleType::Timestamp(_) => "TIMESTAMP",
        OracleType::TimestampTZ(_) => "TIMESTAMP WITH TIMEZONE",
        OracleType::TimestampLTZ(_) => "TIMESTAMP WITH LOCAL TIMEZONE",
        OracleType::IntervalYM(_) => "INTERVAL YEAR TO MONTH",
        OracleType::IntervalDS(_, _) => "INTERVAL DAY TO SECOND",
        OracleType::Date => "DATE",
        OracleType::Float(_) => "FLOAT",
        OracleType::Number(_, _) | OracleType::Int64 => "NUMBER",
        OracleType::Long => "LONG",
        OracleType::Raw(_) => "RAW",
        OracleType::LongRaw => "LONG RAW",
        OracleType::NVarchar2(_) => "NVARCHAR2",
        OracleType::NChar(_) => "NCHAR",
        OracleType::Varchar2(_) => "VARCHAR2",
        OracleType::Char(_) => "CHAR",
        OracleType::BLOB => "BLOB",
        OracleType::NCLOB => "NCLOB",
        OracleType::CLOB => "CLOB",
        OracleType::BFILE => "BFILE",
        OracleType::Rowid => "ROWID",
        OracleType::BinaryFloat => "BINARY_FLOAT",
        OracleType::BinaryDouble => "BINARY_DOUBLE",
        OracleType::Json => "JSON",
        OracleType::Xml => "XML",
        _ => "UNKNOWN",
    }
    .to_string()
}

/// Renders Oracle RAW values through the SQLT_CHR hexadecimal conversion PDO_OCI uses.
fn uppercase_hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses OCI DSN credentials, charset, and constructor autocommit state.
    #[test]
    fn parses_oci_dsn() {
        let options = parse_dsn(
            "oci:dbname=//db.example:1521/app;charset=AL32UTF8;user=scott%3Badmin;password=t%25iger;elephc_oci_autocommit=0",
        )
        .unwrap();
        assert_eq!(options.dbname, "//db.example:1521/app");
        assert_eq!(options.username, "scott;admin");
        assert_eq!(options.password, "t%iger");
        assert_eq!(options.charset.as_deref(), Some("AL32UTF8"));
        assert!(!options.auto_commit);
    }

    /// Reuses one Oracle bind name for repeated named placeholders.
    #[test]
    fn translates_repeated_oracle_placeholders() {
        let sql = translate_oracle_placeholders("select ? + ? + ? from dual", &[1, 2, 1]).unwrap();
        assert_eq!(
            sql,
            "select :elephc_pdo_1 + :elephc_pdo_2 + :elephc_pdo_1 from dual"
        );
    }

    /// Rejects character sets ODPI-C cannot expose through its UTF-8 environment.
    #[test]
    fn rejects_non_utf8_charset() {
        assert!(parse_dsn("oci:dbname=db;charset=WE8MSWIN1252").is_err());
    }

    /// Recovers php-src's special OCI SQLSTATE mappings from open diagnostics.
    #[test]
    fn maps_open_diagnostics() {
        assert_eq!(open_diagnostic("ORA-12154: TNS could not resolve"), ("42S02", 12154));
        assert_eq!(open_diagnostic("ORA-03113: end-of-file"), ("01002", 3113));
        assert_eq!(open_diagnostic("DPI-1047: client library missing"), ("HY000", 0));
    }

    /// Matches PDO_OCI's uppercase SQLT_CHR rendering for RAW columns.
    #[test]
    fn renders_raw_values_as_uppercase_hex() {
        assert_eq!(uppercase_hex(&[0, 0xab, 0xff]), "00ABFF");
    }

    /// Mirrors PDO_OCI's zero/default behavior for invalid prefetch ranges.
    #[test]
    fn sanitizes_prefetch_ranges() {
        assert_eq!(sanitize_prefetch(-1), 0);
        assert_eq!(sanitize_prefetch(42), 42);
        assert_eq!(sanitize_prefetch(i64::from(u32::MAX / 1024) + 1), DEFAULT_PREFETCH);
    }

    /// Exercises the native Oracle client against the configured live database.
    #[test]
    #[ignore]
    fn live_oci_round_trip() {
        let dsn = std::env::var("ELEPHC_OCI_DSN")
            .expect("ELEPHC_OCI_DSN is required for the ignored PDO_OCI live test");
        let mut connection = OciConn::open(&dsn).expect("open Oracle test database");
        let _ = connection.exec(
            "BEGIN EXECUTE IMMEDIATE 'DROP TABLE ELEPHC_PDO_OCI_BRIDGE'; EXCEPTION WHEN OTHERS THEN NULL; END;",
        );
        assert_eq!(
            connection.exec(
                "CREATE TABLE ELEPHC_PDO_OCI_BRIDGE (ID NUMBER NOT NULL, NAME VARCHAR2(80), DATA BLOB)"
            ),
            0
        );

        let mut insert = OciStmt::new(
            &mut connection,
            1,
            "INSERT INTO ELEPHC_PDO_OCI_BRIDGE (ID, NAME, DATA) VALUES (:id, :name, :data)",
        )
        .unwrap();
        assert!(insert.bind_int(1, 7));
        assert!(insert.bind_text(2, "Éléphant".as_bytes().to_vec()));
        assert!(insert.bind_blob(3, b"A\0B".to_vec()));
        insert.execute(&mut connection).unwrap();
        assert_eq!(connection.changes, 1);

        let mut input_output = OciStmt::new(&mut connection, 1, "BEGIN :p := :p + 100; END;")
            .unwrap();
        assert!(input_output.bind_int(1, -1));
        assert!(input_output.bind_output(1, 1, 10));
        input_output.execute(&mut connection).unwrap();
        assert_eq!(input_output.output_value(1).unwrap().data.as_deref(), Some(b"99".as_slice()));

        let mut lob_output = OciStmt::new(
            &mut connection,
            1,
            "BEGIN SELECT DATA INTO :data FROM ELEPHC_PDO_OCI_BRIDGE WHERE ID = 7; END;",
        )
        .unwrap();
        assert!(lob_output.bind_null(1));
        assert!(lob_output.bind_output(1, 3, 0));
        lob_output.execute(&mut connection).unwrap();
        let output = lob_output.output_value(1).unwrap();
        assert!(output.lob);
        assert_eq!(output.data.as_deref(), Some(b"A\0B".as_slice()));

        let mut select = OciStmt::new(
            &mut connection,
            1,
            "SELECT ID, NAME, DATA FROM ELEPHC_PDO_OCI_BRIDGE ORDER BY ID",
        )
        .unwrap();
        select.execute(&mut connection).unwrap();
        assert_eq!(select.step(), 1);
        assert_eq!(select.column_data(0), b"7");
        assert_eq!(select.column_data(1), "Éléphant".as_bytes());
        assert_eq!(select.column_data(2), b"A\0B");
        assert_eq!(select.column_native_type(0), "NUMBER");
        assert_eq!(select.column_pdo_type(2), 3);

        assert!(connection.begin());
        assert_eq!(
            connection.exec(
                "INSERT INTO ELEPHC_PDO_OCI_BRIDGE (ID, NAME, DATA) VALUES (8, 'rollback', empty_blob())"
            ),
            1
        );
        assert!(connection.rollback());
        let mut count = OciStmt::new(
            &mut connection,
            1,
            "SELECT COUNT(*) FROM ELEPHC_PDO_OCI_BRIDGE",
        )
        .unwrap();
        count.execute(&mut connection).unwrap();
        assert_eq!(count.step(), 1);
        assert_eq!(count.column_data(0), b"1");
        assert_eq!(connection.exec("DROP TABLE ELEPHC_PDO_OCI_BRIDGE"), 0);
    }
}
