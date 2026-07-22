//! Purpose:
//! Type-checks compiler-resident PHP language constructs with lazy or l-value operands.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - These constructs cannot use ordinary eager registry call normalization.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

/// Type-checks compiler-resident PHP language constructs.
///
/// Validates arity, eager subexpressions, and non-reading property probes while
/// preserving the lazy semantics that prevent ordinary registry normalization.
///
/// ## Supported builtins
/// - Control: `exit`, `die`, `empty`
/// - Property/local probes: `isset`, `unset`
///
/// ## Arguments
/// - `checker`: mutable checker state for inference
/// - `name`: lowercase builtin name (case-insensitive lookup is handled by caller)
/// - `args`: parsed argument expressions
/// - `span`: source span for error reporting
/// - `env`: current type environment
///
/// ## Returns
/// Returns the inferred type or a source diagnostic for an invalid construct.
pub(super) fn check(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
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
            Ok(PhpType::Void)
        }
        "empty" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "empty() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(PhpType::Bool)
        }
        "unset" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "unset() takes at least 1 argument"));
            }
            for arg in args {
                check_unset_arg(checker, arg, env)?;
            }
            Ok(PhpType::Void)
        }
        "isset" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "isset() takes at least 1 argument"));
            }
            for arg in args {
                check_isset_arg(checker, arg, env)?;
            }
            Ok(PhpType::Bool)
        }
        _ => unreachable!("non-language construct reached language construct checker"),
    }
}

/// Type-checks one `isset()` operand without forcing an observable property read.
fn check_isset_arg(checker: &mut Checker, arg: &Expr, env: &TypeEnv) -> Result<(), CompileError> {
    if let ExprKind::PropertyAccess { object, .. }
    | ExprKind::NullsafePropertyAccess { object, .. } = &arg.kind
    {
        let object_ty = checker.infer_type(object, env)?;
        if isset_object_receiver_type(checker, &object_ty) {
            return Ok(());
        }
    }
    checker.infer_type(arg, env).map(|_| ())
}

/// Returns whether an `isset()` receiver can use non-reading property semantics.
fn isset_object_receiver_type(checker: &Checker, ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) | PhpType::Mixed => true,
        PhpType::Union(members) => {
            checker.union_single_object_class(ty).is_some()
                || members.iter().any(|member| matches!(member, PhpType::Mixed))
        }
        _ => false,
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
