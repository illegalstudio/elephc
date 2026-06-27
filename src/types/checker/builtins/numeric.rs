//! Purpose:
//! Type-checks the numeric PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

/// Type-checks numeric and language-construct PHP builtins.
///
/// Validates argument count, argument types, and special cases (e.g., `buffer_free`
/// restriction on `$this`, locals-only) for the builtin functions in the numeric
/// family. Returns the inferred `PhpType` on success, or a `CompileError` on type/
/// arity mismatch.
///
/// ## Supported builtins
/// - Legacy scalar aliases not yet migrated into `src/builtins/`: `strval`,
///   `is_double`, `is_real`, `is_integer`, `is_long`
/// - Control: `exit`, `die`, `empty`
/// - Unset: `unset`
/// - Buffers: `buffer_len`, `buffer_free`
///
/// ## Arguments
/// - `checker`: mutable checker state for inference
/// - `name`: lowercase builtin name (case-insensitive lookup is handled by caller)
/// - `args`: parsed argument expressions
/// - `span`: source span for error reporting
/// - `env`: current type environment
///
/// ## Returns
/// `Ok(Some(PhpType))` with the inferred return type, `Ok(None)` for unknown builtins
/// (caller falls through), or `Err(CompileError)` on validation failure.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "exit" | "die" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "exit() takes 0 or 1 arguments"));
            }
            if let Some(arg) = args.first() {
                let ty = checker.infer_type(arg, env)?;
                if ty != PhpType::Int {
                    return Err(CompileError::new(span, "exit() argument must be integer"));
                }
            }
            Ok(Some(PhpType::Void))
        }
        "strval" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "strval() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "is_double" | "is_real" | "is_integer" | "is_long" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "method_exists" | "property_exists" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Bool))
        }
        "empty" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "empty() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "unset" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "unset() takes at least 1 argument"));
            }
            for arg in args {
                check_unset_arg(checker, arg, env)?;
            }
            Ok(Some(PhpType::Void))
        }
        "buffer_len" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "buffer_len() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Buffer(_)) {
                return Err(CompileError::new(
                    span,
                    "buffer_len() argument must be buffer<T>",
                ));
            }
            Ok(Some(PhpType::Int))
        }
        "buffer_free" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "buffer_free() takes exactly 1 argument"));
            }
            match &args[0].kind {
                ExprKind::Variable(name) => {
                    if checker.current_class.is_some() && name == "this" {
                        return Err(CompileError::new(span, "buffer_free() cannot free $this"));
                    }
                    if checker.active_ref_params.contains(name)
                        || checker.active_globals.contains(name)
                        || checker.active_statics.contains(name)
                    {
                        return Err(CompileError::new(
                            span,
                            "buffer_free() argument must be a local variable",
                        ));
                    }
                }
                _ => {
                    let ty = checker.infer_type(&args[0], env)?;
                    if !matches!(ty, PhpType::Buffer(_)) {
                        return Err(CompileError::new(
                            span,
                            "buffer_free() argument must be buffer<T>",
                        ));
                    }
                    return Err(CompileError::new(
                        span,
                        "buffer_free() argument must be a local variable",
                    ));
                }
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Buffer(_)) {
                return Err(CompileError::new(
                    span,
                    "buffer_free() argument must be buffer<T>",
                ));
            }
            Ok(Some(PhpType::Void))
        }
        _ => Ok(None),
    }
}

/// Type-checks one `unset()` operand while preserving PHP's non-reading property semantics.
fn check_unset_arg(checker: &mut Checker, arg: &Expr, env: &TypeEnv) -> Result<(), CompileError> {
    if let ExprKind::PropertyAccess { object, property }
    | ExprKind::NullsafePropertyAccess { object, property } = &arg.kind
    {
        let object_ty = checker.infer_type(object, env)?;
        if unset_object_property_probe_is_valid(checker, &object_ty, property, arg)? {
            return Ok(());
        }
    }
    checker.infer_type(arg, env).map(|_| ())
}

/// Returns true when `unset($object->property)` can be checked without reading the property.
fn unset_object_property_probe_is_valid(
    checker: &Checker,
    object_ty: &PhpType,
    property: &str,
    arg: &Expr,
) -> Result<bool, CompileError> {
    match object_ty {
        PhpType::Object(class_name) => {
            unset_property_probe_is_valid_on_class(checker, class_name, property, arg)
        }
        PhpType::Mixed => Ok(true),
        PhpType::Union(members) => {
            if let Some(class_name) = checker.union_single_object_class(object_ty) {
                unset_property_probe_is_valid_on_class(checker, &class_name, property, arg)
            } else {
                Ok(members.iter().any(|member| matches!(member, PhpType::Mixed)))
            }
        }
        _ => Ok(false),
    }
}

/// Checks one known receiver class for PHP `unset($object->property)` magic/no-op legality.
fn unset_property_probe_is_valid_on_class(
    checker: &Checker,
    class_name: &str,
    property: &str,
    arg: &Expr,
) -> Result<bool, CompileError> {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return Ok(true);
    }
    let Some(class_info) = checker.classes.get(class_name) else {
        return Ok(false);
    };
    if let Some(visibility) = class_info.property_visibilities.get(property) {
        let declaring_class = class_info
            .property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name);
        if !checker.can_access_member(declaring_class, visibility) {
            if class_info.methods.contains_key("__unset") {
                return Ok(true);
            }
            return Err(CompileError::new(
                arg.span,
                &format!(
                    "Cannot access {} property: {}::{}",
                    Checker::visibility_label(visibility),
                    class_name,
                    property
                ),
            ));
        }
        return Ok(false);
    }
    Ok(true)
}

/// Returns the most precise supported result type for `abs($value)`.
fn abs_result_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Float => PhpType::Float,
        PhpType::Mixed => PhpType::Mixed,
        PhpType::Union(members) if members.iter().any(|member| *member == PhpType::Float) => {
            PhpType::Mixed
        }
        PhpType::Union(members) if members.iter().any(|member| *member == PhpType::Mixed) => {
            PhpType::Mixed
        }
        _ => PhpType::Int,
    }
}
