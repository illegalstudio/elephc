//! Purpose:
//! Type-checks the system PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::json_constants::JSON_INT_CONSTANTS;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

/// Type-checks a system builtin call by name, validating arity, argument types,
/// and return type. Returns `Ok(Some(PhpType))` for handled builtins, `Ok(None)`
/// for unknown system builtins, or an error for misuse.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "time" => {
            if !args.is_empty() {
                return Err(CompileError::new(span, "time() takes no arguments"));
            }
            Ok(Some(PhpType::Int))
        }
        "microtime" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "microtime() takes 0 or 1 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Float))
        }
        "sleep" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "sleep() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "usleep" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "usleep() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        "getenv" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "getenv() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "putenv" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "putenv() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "php_uname" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "php_uname() takes 0 or 1 arguments"));
            }
            if let Some(arg) = args.first() {
                let ty = checker.infer_type(arg, env)?;
                if ty != PhpType::Str {
                    return Err(CompileError::new(span, "php_uname() argument must be string"));
                }
            }
            Ok(Some(PhpType::Str))
        }
        "phpversion" => {
            if !args.is_empty() {
                return Err(CompileError::new(span, "phpversion() takes no arguments"));
            }
            Ok(Some(PhpType::Str))
        }
        "class_attribute_names" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "class_attribute_names() takes exactly 1 argument",
                ));
            }
            // Resolve at compile time: only string-literal class names are
            // supported in this iteration. Dynamic class names would require
            // a runtime name→class_id lookup table that elephc does not yet
            // expose.
            let arg_ty = checker.infer_type(&args[0], env)?;
            if !matches!(arg_ty, PhpType::Str) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_names() argument must be a string class name",
                ));
            }
            let ExprKind::StringLiteral(class_name) = &args[0].kind else {
                return Err(CompileError::new(
                    span,
                    "class_attribute_names() requires a string literal class name (dynamic lookup is not yet supported)",
                ));
            };
            if resolve_class_name(checker, class_name).is_none() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "class_attribute_names(): undefined class '{}'",
                        class_name
                    ),
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "class_attribute_args" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() takes exactly 2 arguments",
                ));
            }
            let class_arg_ty = checker.infer_type(&args[0], env)?;
            if !matches!(class_arg_ty, PhpType::Str) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() first argument must be a string class name",
                ));
            }
            let attr_arg_ty = checker.infer_type(&args[1], env)?;
            if !matches!(attr_arg_ty, PhpType::Str) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() second argument must be a string attribute name",
                ));
            }
            let ExprKind::StringLiteral(class_name) = &args[0].kind else {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() requires a string literal class name (dynamic lookup is not yet supported)",
                ));
            };
            if !matches!(args[1].kind, ExprKind::StringLiteral(_)) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() requires a string literal attribute name (dynamic lookup is not yet supported)",
                ));
            }
            if resolve_class_name(checker, class_name).is_none() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "class_attribute_args(): undefined class '{}'",
                        class_name
                    ),
                ));
            }
            let ExprKind::StringLiteral(attr_name) = &args[1].kind else {
                unreachable!("attribute argument literal checked above");
            };
            if class_attribute_args_unsupported(checker, class_name, attr_name) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args(): requested attribute uses argument metadata that is not supported yet",
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Mixed))))
        }
        "class_get_attributes" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "class_get_attributes() takes exactly 1 argument",
                ));
            }
            let arg_ty = checker.infer_type(&args[0], env)?;
            if !matches!(arg_ty, PhpType::Str) {
                return Err(CompileError::new(
                    span,
                    "class_get_attributes() argument must be a string class name",
                ));
            }
            let ExprKind::StringLiteral(class_name) = &args[0].kind else {
                return Err(CompileError::new(
                    span,
                    "class_get_attributes() requires a string literal class name (dynamic lookup is not yet supported)",
                ));
            };
            if resolve_class_name(checker, class_name).is_none() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "class_get_attributes(): undefined class '{}'",
                        class_name
                    ),
                ));
            }
            if class_get_attributes_unsupported(checker, class_name) {
                return Err(CompileError::new(
                    span,
                    "class_get_attributes(): class has attribute argument metadata that is not supported yet",
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Object(
                "ReflectionAttribute".to_string(),
            )))))
        }
        "exec" | "shell_exec" | "system" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "passthru" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "passthru() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        "define" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "define() takes exactly 2 arguments"));
            }
            let name_str = match &args[0].kind {
                ExprKind::StringLiteral(s) => s.clone(),
                _ => {
                    return Err(CompileError::new(
                        span,
                        "define() first argument must be a string literal",
                    ));
                }
            };
            let ty = checker.infer_type(&args[1], env)?;
            checker.constants.entry(name_str).or_insert(ty);
            Ok(Some(PhpType::Bool))
        }
        "date" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "date() takes 1 or 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "mktime" => {
            if args.len() != 6 {
                return Err(CompileError::new(span, "mktime() takes exactly 6 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "strtotime" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "strtotime() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "json_encode" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "json_encode() takes 1 to 3 arguments",
                ));
            }
            checker.infer_type(&args[0], env)?;
            for extra in &args[1..] {
                let ty = checker.infer_type(extra, env)?;
                if ty != PhpType::Int {
                    return Err(CompileError::new(
                        extra.span,
                        "json_encode() flags and depth must be integers",
                    ));
                }
            }
            Ok(Some(PhpType::Str))
        }
        "json_decode" => {
            if args.is_empty() || args.len() > 4 {
                return Err(CompileError::new(
                    span,
                    "json_decode() takes 1 to 4 arguments",
                ));
            }
            let json_ty = checker.infer_type(&args[0], env)?;
            if !is_json_string_arg_type(&json_ty) {
                return Err(CompileError::new(
                    args[0].span,
                    "json_decode() json argument must be string-compatible",
                ));
            }
            if let Some(assoc) = args.get(1) {
                let assoc_ty = checker.infer_type(assoc, env)?;
                if !is_json_associative_arg_type(&assoc_ty) {
                    return Err(CompileError::new(
                        assoc.span,
                        "json_decode() associative argument must be bool-compatible or null",
                    ));
                }
            }
            for extra in args.iter().skip(2) {
                let ty = checker.infer_type(extra, env)?;
                if ty != PhpType::Int {
                    return Err(CompileError::new(
                        extra.span,
                        "json_decode() depth and flags must be integers",
                    ));
                }
            }
            // Returns a structural Mixed: scalars (null/bool/int/float/string)
            // box natively; arrays and objects currently fall back to a
            // Mixed(string) wrapping the trimmed JSON slice (full structural
            // decode of containers is on the roadmap).
            Ok(Some(PhpType::Mixed))
        }
        "json_validate" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "json_validate() takes 1 to 3 arguments",
                ));
            }
            let json_ty = checker.infer_type(&args[0], env)?;
            if !is_json_string_arg_type(&json_ty) {
                return Err(CompileError::new(
                    args[0].span,
                    "json_validate() json argument must be string-compatible",
                ));
            }
            for extra in &args[1..] {
                let ty = checker.infer_type(extra, env)?;
                if ty != PhpType::Int {
                    return Err(CompileError::new(
                        extra.span,
                        "json_validate() depth and flags must be integers",
                    ));
                }
            }
            if let Some(flags) = args.get(2) {
                if let Some(value) = json_static_int_value(flags) {
                    const JSON_INVALID_UTF8_IGNORE: i64 = 1_048_576;
                    if value & !JSON_INVALID_UTF8_IGNORE != 0 {
                        return Err(CompileError::new(
                            flags.span,
                            "json_validate() flags must be 0 or JSON_INVALID_UTF8_IGNORE",
                        ));
                    }
                }
            }
            Ok(Some(PhpType::Bool))
        }
        "json_last_error" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "json_last_error() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Int))
        }
        "json_last_error_msg" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "json_last_error_msg() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Str))
        }
        "preg_match" | "preg_match_all" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "preg_replace" => {
            if args.len() != 3 {
                return Err(CompileError::new(
                    span,
                    "preg_replace() takes exactly 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "preg_split" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "preg_split() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        _ => Ok(None),
    }
}

/// Resolves a class name to its canonical key in the checker's class table.
/// Returns `Some(canonical_name)` if the class exists, `None` otherwise.
/// The lookup is case-insensitive per PHP rules.
fn resolve_class_name<'a>(checker: &'a Checker, class_name: &str) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    checker
        .classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

/// Returns `true` if `ty` is a valid type for the JSON string argument in
/// `json_decode` / `json_validate` / `json_encode` (scalar types and `Mixed`).
fn is_json_string_arg_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Str
        | PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::Void
        | PhpType::Mixed => true,
        PhpType::Union(types) => types.iter().all(is_json_string_arg_type),
        _ => false,
    }
}

/// Returns `true` if `ty` is a valid type for the associative argument in
/// `json_decode` (bool-compatible types plus `Mixed`).
fn is_json_associative_arg_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Void
        | PhpType::Mixed => true,
        PhpType::Union(types) => types.iter().all(is_json_associative_arg_type),
        _ => false,
    }
}

/// Attempts to evaluate an expression as a static integer at compile time.
/// Supports literals, known constants, negation, and bitwise ops.
/// Returns `Some(value)` if the expression is statically computable, `None` otherwise.
fn json_static_int_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => JSON_INT_CONSTANTS
            .iter()
            .find_map(|(constant, value)| (*constant == name.as_str()).then_some(*value)),
        ExprKind::Negate(inner) => json_static_int_value(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = json_static_int_value(left)?;
            let right = json_static_int_value(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns `true` if the named attribute on the class uses argument metadata
/// that the compiler does not yet support (i.e., `attribute_args` slot is `None`).
fn class_attribute_args_unsupported(checker: &Checker, class_name: &str, attr_name: &str) -> bool {
    let Some(resolved_class) = resolve_class_name(checker, class_name) else {
        return false;
    };
    let Some(class_info) = checker.classes.get(resolved_class) else {
        return false;
    };
    let attr_key = php_symbol_key(attr_name.trim_start_matches('\\'));
    class_info
        .attribute_names
        .iter()
        .enumerate()
        .find(|(_, name)| php_symbol_key(name.trim_start_matches('\\')) == attr_key)
        .is_some_and(|(idx, _)| !matches!(class_info.attribute_args.get(idx), Some(Some(_))))
}

/// Returns `true` if the class has any attribute whose argument metadata is not
/// fully supported (slot count mismatch or any `None` slot in `attribute_args`).
fn class_get_attributes_unsupported(checker: &Checker, class_name: &str) -> bool {
    let Some(resolved_class) = resolve_class_name(checker, class_name) else {
        return false;
    };
    checker.classes.get(resolved_class).is_some_and(|class_info| {
        class_info.attribute_names.len() != class_info.attribute_args.len()
            || class_info.attribute_args.iter().any(Option::is_none)
    })
}
