//! Purpose:
//! Shared checker contract for PHP's internal-array-pointer builtins
//! (`key`, `current`, `next`, `prev`, `reset`, `end`).
//!
//! Called from:
//! - The `check` hook of each of the six home files in `crate::builtins::array`.
//!
//! Key details:
//! - This module declares no builtin of its own; it only holds the validation the six
//!   home files share, so each of them keeps exactly one `builtin!` declaration.
//! - Read-only calls accept any array expression. By-reference seek calls accept writable
//!   places and call results, while rejecting non-referenceable values such as literals.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Validates one internal-array-pointer call and returns its `Mixed` result type.
///
/// The receiver must be array-typed. `Mixed` is allowed because heterogeneous arrays are
/// `Mixed` at compile time; the runtime helpers report `false`/`null` when a Mixed payload
/// turns out not to be a container. The four seek operations additionally require either a
/// writable place or a call result temporary; `key()` and `current()` accept ordinary array
/// expressions because they do not move the pointer.
///
/// The registry's `check_arity` has already enforced the single-argument arity, so
/// `cx.args[0]` is present whenever this runs.
pub fn check_array_pointer_call(
    cx: &mut BuiltinCheckCtx,
    name: &str,
) -> Result<PhpType, CompileError> {
    if matches!(name, "next" | "prev" | "reset" | "end")
        && !matches!(
            cx.args[0].kind,
            ExprKind::Variable(_)
                | ExprKind::PropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
                | ExprKind::ArrayAccess { .. }
                | ExprKind::FunctionCall { .. }
                | ExprKind::MethodCall { .. }
                | ExprKind::NullsafeMethodCall { .. }
                | ExprKind::NullsafeDynamicMethodCall { .. }
                | ExprKind::StaticMethodCall { .. }
                | ExprKind::ClosureCall { .. }
                | ExprKind::ExprCall { .. }
        )
    {
        return Err(CompileError::new(
            cx.args[0].span,
            &format!("{} parameter $array must be passed a variable", name),
        ));
    }
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !ty.is_php_array() && !may_hold_runtime_array(&ty) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() argument must be array", name),
        ));
    }
    Ok(PhpType::Mixed)
}

/// Returns whether a gradual value can carry an array accepted by the pointer runtime.
fn may_hold_runtime_array(ty: &PhpType) -> bool {
    match ty {
        // `Never` is the bottom element type a callback's dummy argument takes when the array it
        // walks has no precise element type (`array_map('current', $rows)`): it stands for
        // whatever the array holds, and the runtime helpers answer `false`/`null` for a payload
        // that is not a container.
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Never => true,
        PhpType::Union(members) => members.iter().any(may_hold_runtime_array),
        _ => false,
    }
}
