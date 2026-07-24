//! Purpose:
//! Home of the PHP `proc_open` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, False)` to reflect PHP's false-on-failure.
//! - The public parameter order and names match PHP: command, descriptor spec,
//!   by-ref pipes, then optional cwd, environment and options.
//! - Windows accepts computed command arrays, cwd strings, environment maps, and
//!   PHP's five documented process options; the backend marshals their runtime
//!   storage before calling `CreateProcessW`.
//! - The Windows descriptor runtime supports pipes, sockets, files, redirects,
//!   null devices, and supplied stream resources while preserving sparse keys.
//! - `TypeSpec` cannot express the command string|array union or nullable array
//!   settings, so those parameters and the array parameters are declared Mixed
//!   and refined by `check`.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::platform::Platform;
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Low-bit mask carried beside a marshalled Windows environment block.
///
/// The remaining high bits hold the byte length of the optional environment
/// block. Keep this in one place so AST lowering, EIR lowering, and the native
/// runtime cannot disagree about the packed ABI.
pub(crate) const WINDOWS_PROC_OPTION_BITS: i64 = 5;

/// Bypasses `cmd.exe` when the command is a string.
pub(crate) const WINDOWS_PROC_BYPASS_SHELL: i64 = 1;
/// Temporarily suppresses Win32 critical-error dialogs around process creation.
pub(crate) const WINDOWS_PROC_SUPPRESS_ERRORS: i64 = 1 << 1;
/// Requests PHP's Windows blocking-pipe mode.
pub(crate) const WINDOWS_PROC_BLOCKING_PIPES: i64 = 1 << 2;
/// Requests `CREATE_NEW_PROCESS_GROUP`.
pub(crate) const WINDOWS_PROC_CREATE_PROCESS_GROUP: i64 = 1 << 3;
/// Requests `CREATE_NEW_CONSOLE`.
pub(crate) const WINDOWS_PROC_CREATE_NEW_CONSOLE: i64 = 1 << 4;

builtin! {
    name: "proc_open",
    area: Io,
    params: [
        command: Mixed,
        descriptor_spec: Mixed,
        ref pipes: Mixed,
        cwd: Mixed = DefaultSpec::Null,
        env_vars: Mixed = DefaultSpec::Null,
        options: Mixed = DefaultSpec::Null
    ],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(
            crate::ir::RuntimeFnId::ProcOpen,
        ),
        crate::builtins::semantics::BuiltinArgumentLowering::ProcOpen,
    ),
    summary: "Execute a command and open file pointers for process I/O.",
    php_manual: "function.proc-open",
}

/// Returns `Union(stream_resource, False)` for the proc_open result.
///
/// Windows accepts both string and scalar-array commands plus computed
/// associative arrays for environment/options. Other targets retain the existing
/// pipe runtime's narrower optional-setting support and diagnose Windows-only use.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let command_arg = argument_at(cx.args, 0, "command").expect("arity guarantees command");
    let command = cx.checker.infer_type(command_arg, cx.env)?;
    match command {
        PhpType::Str => {}
        ty if valid_command_array_type(&ty) => {
            if cx.checker.target_platform != Platform::Windows {
                return Err(CompileError::new(
                    cx.span,
                    "proc_open() array commands are currently supported only for the Windows target",
                ));
            }
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            return Err(CompileError::new(
                cx.span,
                "proc_open() command array must contain only scalar values",
            ));
        }
        _ => {
            return Err(CompileError::new(
                cx.span,
                "proc_open() first argument must be string or array",
            ));
        }
    }

    for (index, name) in [(1, "descriptor_spec"), (2, "pipes")] {
        let arg = argument_at(cx.args, index, name).expect("arity guarantees required argument");
        let ty = cx.checker.infer_type(arg, cx.env)?;
        if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            return Err(CompileError::new(
                cx.span,
                &format!("proc_open() ${name} argument must be array"),
            ));
        }
    }

    if let Some(arg) = argument_at(cx.args, 3, "cwd") {
        let ty = cx.checker.infer_type(arg, cx.env)?;
        if !matches!(&ty, PhpType::Void | PhpType::Never) {
            if cx.checker.target_platform != Platform::Windows {
                return Err(CompileError::new(
                    cx.span,
                    "proc_open() non-null $cwd is currently supported only for the Windows target",
                ));
            }
            if !matches!(ty, PhpType::Str) {
                return Err(CompileError::new(
                    cx.span,
                    "proc_open() $cwd must be string or null",
                ));
            }
        }
    }

    if let Some(arg) = argument_at(cx.args, 4, "env_vars") {
        let ty = cx.checker.infer_type(arg, cx.env)?;
        if !matches!(&ty, PhpType::Void | PhpType::Never) {
            if cx.checker.target_platform != Platform::Windows {
                return Err(CompileError::new(
                    cx.span,
                    "proc_open() non-null $env_vars is currently supported only for the Windows target",
                ));
            }
            if !valid_environment_type(&ty) {
                return Err(CompileError::new(
                    cx.span,
                    "proc_open() $env_vars must be null or an array of scalar values",
                ));
            }
        }
    }
    if let Some(arg) = argument_at(cx.args, 5, "options") {
        let ty = cx.checker.infer_type(arg, cx.env)?;
        if !matches!(&ty, PhpType::Void | PhpType::Never) {
            if cx.checker.target_platform != Platform::Windows {
                return Err(CompileError::new(
                    cx.span,
                    "proc_open() non-null $options is currently supported only for the Windows target",
                ));
            }
            if !valid_options_type(&ty) {
                return Err(CompileError::new(
                    cx.span,
                    "proc_open() $options must be an array",
                ));
            }
        }
    }

    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::False,
    ]))
}

/// Returns whether a command array can be converted with PHP scalar-to-string rules.
fn valid_command_array_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(value) => valid_proc_scalar_type(value),
        PhpType::AssocArray { key, value } => {
            matches!(**key, PhpType::Str | PhpType::Int | PhpType::Mixed)
                && valid_proc_scalar_type(value)
        }
        _ => false,
    }
}

/// Returns whether a value can be represented by the Windows process scalar converter.
fn valid_proc_scalar_type(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Str
            | PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Mixed
    )
}

/// Returns whether a checked Windows environment value can be marshalled by
/// the runtime scalar-to-string converter.
fn valid_environment_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(value) => valid_proc_scalar_type(value),
        PhpType::AssocArray { key, value } => {
            matches!(**key, PhpType::Str | PhpType::Int | PhpType::Mixed)
                && valid_proc_scalar_type(value)
        }
        _ => false,
    }
}

/// Returns whether a checked options array can carry PHP's Windows option
/// values at runtime.
///
/// php-src accepts truthy boolean and integer values for recognized keys, and
/// silently ignores unknown keys. The runtime mirrors that behavior; this
/// checker only rejects shapes that cannot be represented as an options array.
fn valid_options_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(value) => matches!(
            **value,
            PhpType::Bool | PhpType::False | PhpType::Int | PhpType::Mixed
        ),
        PhpType::AssocArray { key, value } => {
            matches!(**key, PhpType::Str | PhpType::Mixed)
                && matches!(
                    **value,
                    PhpType::Bool | PhpType::False | PhpType::Int | PhpType::Mixed
                )
        }
        _ => false,
    }
}

/// Returns one caller-visible argument by PHP parameter position, unwrapping
/// named arguments while retaining the positional-prefix rules checked earlier.
pub(crate) fn argument_at<'a>(args: &'a [Expr], index: usize, name: &str) -> Option<&'a Expr> {
    let mut positional = 0usize;
    for argument in args {
        match &argument.kind {
            ExprKind::NamedArg {
                name: argument_name,
                value,
            } if argument_name == name => return Some(value),
            ExprKind::NamedArg { .. } => {}
            _ if positional == index => return Some(argument),
            _ => positional += 1,
        }
    }
    None
}

/// Returns the literal command arguments after PHP scalar-to-string conversion
/// and ignores associative keys, as php-src's Windows command builder does.
pub(crate) fn static_command_array(expr: &Expr) -> Option<Vec<String>> {
    let values: Vec<String> = match &expr.kind {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .map(static_command_scalar)
            .collect::<Option<_>>()?,
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .map(|(_, value)| static_command_scalar(value))
            .collect::<Option<_>>()?,
        _ => return None,
    };
    if values.first().is_some_and(|value| !value.is_empty()) {
        Some(values)
    } else {
        None
    }
}

/// Converts one statically-known command value with PHP's scalar string rules.
fn static_command_scalar(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        ExprKind::IntLiteral(value) => Some(value.to_string()),
        ExprKind::FloatLiteral(value) => Some(value.to_string()),
        ExprKind::BoolLiteral(value) => Some(if *value { "1" } else { "" }.to_string()),
        ExprKind::Null => Some(String::new()),
        _ => None,
    }
}

/// Returns literal environment entries in php-src's key/value form.
///
/// `None` keys are numeric (or empty-string) hash keys, which php-src writes
/// as raw environment entries instead of synthesizing an `=` prefix.
pub(crate) fn static_environment(expr: &Expr) -> Option<Vec<(Option<String>, String)>> {
    let entries: Vec<(Option<String>, &Expr)> = match &expr.kind {
        ExprKind::ArrayLiteral(items) => items.iter().map(|value| (None, value)).collect(),
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .map(|(key, value)| {
                let key = match &key.kind {
                    ExprKind::StringLiteral(key) if !key.contains('\0') && !key.is_empty() => {
                        Some(key.clone())
                    }
                    ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) => None,
                    _ => return None,
                };
                Some((key, value))
            })
            .collect::<Option<_>>()?,
        _ => return None,
    };
    let mut environment = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        let value = static_command_scalar(value)?;
        if value.is_empty() {
            continue;
        }
        if value.contains('\0') {
            return None;
        }
        environment.push((key, value));
    }
    Some(environment)
}

/// Returns literal Windows option bits using php-src's truthiness rules.
///
/// Unknown option keys are intentionally ignored, matching `get_option()` in
/// php-src. Dynamic literal values return `None` so EIR codegen can dispatch to
/// the runtime options marshaller instead of guessing their value.
pub(crate) fn static_windows_options(expr: &Expr) -> Option<i64> {
    if matches!(&expr.kind, ExprKind::ArrayLiteral(items) if items.is_empty()) {
        return Some(0);
    }
    let ExprKind::ArrayLiteralAssoc(entries) = &expr.kind else {
        return None;
    };
    let mut flags = 0;
    for (key, value) in entries {
        let ExprKind::StringLiteral(key) = &key.kind else {
            return None;
        };
        let Some(bit) = windows_option_bit(key) else {
            continue;
        };
        let enabled = match &value.kind {
            ExprKind::BoolLiteral(value) => *value,
            ExprKind::IntLiteral(value) => *value != 0,
            _ => return None,
        };
        if enabled {
            flags |= bit;
        } else {
            flags &= !bit;
        }
    }
    Some(flags)
}

/// Maps one PHP Windows option name to its packed native ABI bit.
fn windows_option_bit(name: &str) -> Option<i64> {
    match name {
        "bypass_shell" => Some(WINDOWS_PROC_BYPASS_SHELL),
        "suppress_errors" => Some(WINDOWS_PROC_SUPPRESS_ERRORS),
        "blocking_pipes" => Some(WINDOWS_PROC_BLOCKING_PIPES),
        "create_process_group" => Some(WINDOWS_PROC_CREATE_PROCESS_GROUP),
        "create_new_console" => Some(WINDOWS_PROC_CREATE_NEW_CONSOLE),
        _ => None,
    }
}

/// Quotes one argument according to the Windows `CommandLineToArgvW` backslash
/// and quote rules used for direct (array-form) process creation.
fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty()
        && !argument
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return argument.to_string();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0usize;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }
        if character == '"' {
            quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
            quoted.push('"');
        } else {
            quoted.extend(std::iter::repeat_n('\\', backslashes));
            quoted.push(character);
        }
        backslashes = 0;
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

/// Returns whether a literal executable is handled by `cmd.exe` on Windows.
///
/// php-src resolves short and full Windows paths before comparing the final
/// component. The AOT compiler deliberately uses the bounded source spelling
/// here: it strips slash-separated path components and compares `cmd`,
/// `cmd.exe`, `.bat`, and `.cmd` case-insensitively. Runtime array commands use
/// the identical bounded rule, so static and computed commands cannot diverge.
fn is_windows_cmd_program(program: &str) -> bool {
    let basename = program.rsplit(['\\', '/']).next().unwrap_or(program);
    basename.eq_ignore_ascii_case("cmd")
        || basename.eq_ignore_ascii_case("cmd.exe")
        || basename
            .get(basename.len().saturating_sub(4)..)
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case(".bat") || extension.eq_ignore_ascii_case(".cmd")
            })
}

/// Quotes a command-processor argument using php-src's caret escaping rules.
///
/// Arguments for `cmd.exe` and batch files are always quoted. When they contain
/// a command-processor metacharacter, php-src also prefixes both surrounding
/// quotes and every metacharacter with `^`; retain the ordinary Windows
/// backslash-before-quote expansion underneath that extra protection.
fn quote_windows_cmd_argument(argument: &str) -> String {
    const SPECIAL: &str = "()!^\"<>&|%";
    let has_special = argument.chars().any(|character| SPECIAL.contains(character));
    let mut quoted = String::new();
    if has_special {
        quoted.push('^');
    }
    quoted.push('"');
    let mut backslashes = 0usize;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }
        if character == '"' {
            quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
        } else {
            quoted.extend(std::iter::repeat_n('\\', backslashes));
        }
        if has_special && SPECIAL.contains(character) {
            quoted.push('^');
        }
        quoted.push(character);
        backslashes = 0;
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    if has_special {
        quoted.push('^');
    }
    quoted.push('"');
    quoted
}

/// Builds the exact direct Windows command line for a validated literal argv.
pub(crate) fn static_windows_command_line(expr: &Expr) -> Option<String> {
    static_command_array(expr).map(|arguments| {
        let cmd_execution = is_windows_cmd_program(&arguments[0]);
        arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                if index != 0 && cmd_execution {
                    quote_windows_cmd_argument(argument)
                } else {
                    quote_windows_argument(argument)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// Encodes a validated environment as the double-NUL-terminated UTF-8 block
/// consumed by the Windows runtime before its counted UTF-16 conversion.
pub(crate) fn static_windows_environment_block(expr: &Expr) -> Option<String> {
    let environment = static_environment(expr)?;
    let mut block = String::new();
    for (name, value) in environment {
        if let Some(name) = name {
            block.push_str(&name);
            block.push('=');
        }
        block.push_str(&value);
        block.push('\0');
    }
    if block.is_empty() {
        block.push('\0');
    }
    block.push('\0');
    Some(block)
}

#[cfg(test)]
mod tests {
    use super::{
        is_windows_cmd_program, quote_windows_argument, quote_windows_cmd_argument,
        static_windows_command_line, static_windows_environment_block, static_windows_options,
        WINDOWS_PROC_BLOCKING_PIPES, WINDOWS_PROC_BYPASS_SHELL, WINDOWS_PROC_CREATE_NEW_CONSOLE,
        WINDOWS_PROC_SUPPRESS_ERRORS,
    };
    use crate::lexer::tokenize;
    use crate::parser::{
        ast::{ExprKind, StmtKind},
        parse,
    };

    /// Verifies Windows argv quoting preserves empty arguments, whitespace,
    /// embedded quotes, and trailing backslashes.
    #[test]
    fn quotes_windows_process_arguments() {
        assert_eq!(quote_windows_argument("plain"), "plain");
        assert_eq!(quote_windows_argument(""), "\"\"");
        assert_eq!(quote_windows_argument("two words"), "\"two words\"");
        assert_eq!(quote_windows_argument("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_windows_argument("C:\\Program Files\\"), "\"C:\\Program Files\\\\\"");
    }

    /// Verifies php-src's command-processor detection stays case-insensitive
    /// while remaining bounded to literal executable spellings.
    #[test]
    fn detects_cmd_and_batch_program_names() {
        assert!(is_windows_cmd_program("CMD.EXE"));
        assert!(is_windows_cmd_program(r"C:\\Windows\\System32\\cmd"));
        assert!(is_windows_cmd_program(r"tools\\BUILD.CmD"));
        assert!(is_windows_cmd_program("run.BAT"));
        assert!(!is_windows_cmd_program("cmd.exe.bak"));
        assert!(!is_windows_cmd_program("acmd.exe"));
        assert!(!is_windows_cmd_program("program.exe"));
    }

    /// Verifies cmd.exe arguments escape every php-src metacharacter without
    /// changing the ordinary executable quoting contract.
    #[test]
    fn quotes_windows_command_processor_arguments() {
        assert_eq!(quote_windows_cmd_argument("plain"), "\"plain\"");
        assert_eq!(quote_windows_cmd_argument("a&b"), "^\"a^&b^\"");
        assert_eq!(quote_windows_cmd_argument("a\"&b"), "^\"a\\^\"^&b^\"");
        assert_eq!(quote_windows_cmd_argument("a()!^\"<>&|%b"), "^\"a^(^)^!^^\\^\"^<^>^&^|^%b^\"");
    }

    /// Verifies literal argv arrays apply php-src's cmd.exe escaping only after
    /// the executable position, leaving ordinary executable arguments unchanged.
    #[test]
    fn static_command_lines_escape_only_command_processor_arguments() {
        let program = parse(&tokenize(
            r#"<?php proc_open(["CMD.EXE", "/c", "echo a&b"], [], $pipes);"#,
        )
        .expect("cmd fixture tokenizes"))
        .expect("cmd fixture parses");
        let command = match &program[0].kind {
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::FunctionCall { args, .. } => &args[0],
                _ => panic!("fixture must be a call"),
            },
            _ => panic!("fixture must be an expression statement"),
        };
        assert_eq!(
            static_windows_command_line(command),
            Some("CMD.EXE \"/c\" ^\"echo a^&b^\"".to_string())
        );

        let ordinary = parse(&tokenize(
            r#"<?php proc_open(["tool.exe", "/c", "echo a&b"], [], $pipes);"#,
        )
        .expect("ordinary fixture tokenizes"))
        .expect("ordinary fixture parses");
        let ordinary_command = match &ordinary[0].kind {
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::FunctionCall { args, .. } => &args[0],
                _ => panic!("fixture must be a call"),
            },
            _ => panic!("fixture must be an expression statement"),
        };
        assert_eq!(
            static_windows_command_line(ordinary_command),
            Some("tool.exe /c \"echo a&b\"".to_string())
        );
    }

    /// Verifies literal associative command arrays ignore keys and coerce scalar values.
    #[test]
    fn static_command_lines_ignore_keys_and_coerce_scalars() {
        let program = parse(&tokenize(
            r#"<?php proc_open(["program" => "tool.exe", 4 => 7, "empty" => false], [], $pipes);"#,
        )
        .expect("scalar argv fixture tokenizes"))
        .expect("scalar argv fixture parses");
        let command = match &program[0].kind {
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::FunctionCall { args, .. } => &args[0],
                _ => panic!("fixture must be a call"),
            },
            _ => panic!("fixture must be an expression statement"),
        };
        assert_eq!(
            static_windows_command_line(command),
            Some("tool.exe 7 \"\"".to_string())
        );
    }

    /// Verifies literal environment arrays preserve php-src raw numeric entries and omit empty values.
    #[test]
    fn static_environment_block_keeps_raw_numeric_entries() {
        let program = parse(&tokenize(
            r#"<?php proc_open("tool.exe", [], $pipes, null, [4 => "RAW=1", "=C:" => "C:\\tmp", "DROP" => false]);"#,
        )
        .expect("environment fixture tokenizes"))
        .expect("environment fixture parses");
        let environment = match &program[0].kind {
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::FunctionCall { args, .. } => &args[4],
                _ => panic!("fixture must be a call"),
            },
            _ => panic!("fixture must be an expression statement"),
        };
        assert_eq!(
            static_windows_environment_block(environment),
            Some("RAW=1\0=C:=C:\\tmp\0\0".to_string())
        );
    }

    /// Verifies literal Windows options preserve php-src's recognized-key and
    /// last-value semantics while ignoring unknown options.
    #[test]
    fn packs_all_supported_windows_options() {
        let program = parse(&tokenize(
            r#"<?php proc_open("x", [], $pipes, null, null, [
                "bypass_shell" => true,
                "suppress_errors" => 1,
                "blocking_pipes" => false,
                "create_new_console" => true,
                "unknown" => true,
                "bypass_shell" => false,
            ]);"#,
        )
        .expect("options fixture tokenizes"))
        .expect("options fixture parses");
        let options = match &program[0].kind {
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::FunctionCall { args, .. } => &args[5],
                _ => panic!("fixture must be a call"),
            },
            _ => panic!("fixture must be an expression statement"),
        };
        assert_eq!(
            static_windows_options(options),
            Some(WINDOWS_PROC_SUPPRESS_ERRORS | WINDOWS_PROC_CREATE_NEW_CONSOLE)
        );
        assert_eq!(WINDOWS_PROC_BYPASS_SHELL | WINDOWS_PROC_BLOCKING_PIPES, 5);
    }
}
