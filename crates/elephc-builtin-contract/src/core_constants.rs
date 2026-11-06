//! Purpose:
//! Shares Elephc's build and target environment constants between native and eval backends.
//!
//! Called from:
//! - Compiler constant prescan and Magician's predefined-constant evaluator.
//!
//! Key details:
//! - Standalone programs have no PHP installation, ini directories, PEAR, or Zend VM.
//! - PHP_BINARY is materialized by the running program, outside this static value table.

use crate::ConstValue;

/// Native default-value ABI kind that resolves PHP_BINARY in the running program.
pub const EXECUTABLE_PATH_DEFAULT_KIND: u64 = 6;

/// Returns target/build Core metadata without embedding the compiler host's installation paths.
pub fn core_environment_constant(name: &str, apple: bool) -> Option<ConstValue> {
    Some(match name {
        "DEFAULT_INCLUDE_PATH" => ConstValue::Str("."),
        "PEAR_EXTENSION_DIR" | "PEAR_INSTALL_DIR" | "PHP_BINDIR" | "PHP_CONFIG_FILE_PATH"
        | "PHP_CONFIG_FILE_SCAN_DIR" | "PHP_DATADIR" | "PHP_EXTENSION_DIR" | "PHP_LIBDIR"
        | "PHP_LOCALSTATEDIR" | "PHP_MANDIR" | "PHP_PREFIX" | "PHP_SBINDIR" | "PHP_SYSCONFDIR" => ConstValue::Str(""),
        "PHP_BUILD_DATE" => ConstValue::Str(env!("ELEPHC_CORE_BUILD_DATE")),
        "PHP_BUILD_PROVIDER" => ConstValue::Str("elephc"),
        "PHP_CLI_PROCESS_TITLE" => ConstValue::Bool(!apple),
        "PHP_FD_SETSIZE" => ConstValue::Int(1024),
        "PHP_MAXPATHLEN" => ConstValue::Int(if apple { 1024 } else { 4096 }),
        "PHP_SHLIB_SUFFIX" => ConstValue::Str(if apple { "dylib" } else { "so" }),
        "ZEND_VM_KIND" => ConstValue::Str("ZEND_VM_KIND_NONE"),
        _ => return None,
    })
}
