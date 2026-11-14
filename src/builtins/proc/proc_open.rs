//! Purpose:
//! Home of the PHP `proc_open` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - `lazy_check` leaves `$pipes` uninferred: it is a write-only output that may not
//!   exist before the call, and must be a plain variable the lowering can store into.
//! - The checker-facing result is `resource|false`; the backend value is a boxed cell.
//! - Spawning, descriptor validation and php-src's diagnostics live in the prelude's
//!   `__elephc_proc_open`.

use crate::builtins::semantics::{BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinResultOwnership};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "proc_open",
    check: check,
    lazy_check: true,
    semantics: super::semantics(
        super::mixed_result,
        BuiltinResultOwnership::Fresh,
        BuiltinArgumentLowering::WriteOnlyMixedOutputs(&[2]),
        BuiltinCallablePolicy::StaticOnly("proc_open() writes its $pipes output into a caller variable"),
    ),
}

/// The parameter names in declaration order, for positional arguments.
const PARAMETERS: &[&str] = &["command", "descriptor_spec", "pipes", "cwd", "env_vars", "options"];

/// Infers every argument except `$pipes`, which must be a plain variable, then answers
/// `resource|false`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::builtins::spec::reject_if_process_spawn_forbidden(cx)?;
    for (index, arg) in cx.args.iter().enumerate() {
        let (parameter, value) = match &arg.kind {
            ExprKind::NamedArg { name, value } => (Some(php_symbol_key(name)), value.as_ref()),
            _ => (PARAMETERS.get(index).map(|name| (*name).to_string()), arg),
        };
        if parameter.as_deref() == Some("pipes") {
            require_variable(value)?;
            continue;
        }
        cx.checker.infer_type(value, cx.env)?;
    }
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::Resource(Some("process".to_string())),
        PhpType::False,
    ]))
}

/// Rejects a `$pipes` argument the lowering could not write back into.
fn require_variable(value: &Expr) -> Result<(), CompileError> {
    if matches!(value.kind, ExprKind::Variable(_)) {
        return Ok(());
    }
    Err(CompileError::new(value.span, "proc_open() parameter $pipes must be passed a variable"))
}
