//! Purpose:
//! Eval registry entry and implementation for `phpinfo`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Mirrors the native lowering's compile-time module table. The `$flags` argument is
//!   evaluated for side effects; the current table is emitted for every value.

use super::*;

eval_builtin! {
    contract: "phpinfo",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `phpinfo($flags?)` and echoes the module table.
pub(in crate::interpreter) fn eval_builtin_phpinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_phpinfo_result(values),
        [flags] => {
            let _ = eval_expr(flags, context, scope, values)?;
            eval_phpinfo_result(values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Echoes the compile-time module table and returns PHP's boolean success value.
pub(in crate::interpreter) fn eval_phpinfo_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let version = crate::eval_php_profile::eval_php_version_string().to_string();
    let directives = vec![
        ("display_errors".to_string(), "On".to_string()),
        ("error_reporting".to_string(), "30719".to_string()),
        ("html_errors".to_string(), "Off".to_string()),
        ("memory_limit".to_string(), "128M".to_string()),
        ("max_execution_time".to_string(), "0".to_string()),
        ("precision".to_string(), "14".to_string()),
        ("serialize_precision".to_string(), "-1".to_string()),
        ("variables_order".to_string(), "GPCS".to_string()),
        ("date.timezone".to_string(), "UTC".to_string()),
        ("pcre.backtrack_limit".to_string(), "1000000".to_string()),
        ("pcre.recursion_limit".to_string(), "100000".to_string()),
        ("open_basedir".to_string(), String::new()),
    ];
    let extensions = super::get_loaded_extensions::eval_loaded_extension_names();
    let environment = vec![
        ("ELEPHC_PHP_VERSION".to_string(), version.clone()),
        ("ELEPHC_SAPI".to_string(), "eval".to_string()),
    ];
    let text = elephc_builtin_contract::phpinfo::render(
        &elephc_builtin_contract::phpinfo::PhpinfoTable {
            php_version: &version,
            system: "eval",
            server_api: "Command Line Interface",
            extensions: &extensions,
            directives: &directives,
            environment: &environment,
        },
    );
    let output = values.string_bytes_value(text.as_bytes())?;
    values.echo(output)?;
    values.bool_value(true)
}
