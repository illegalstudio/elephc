//! Purpose:
//! Home of the PHP `class_alias` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook accepts calls whose class strings are statically resolvable after
//!   name resolution. Dynamic class strings are accepted in closed-world builds only, where
//!   the backend registers them in the run-time alias table.
//!   Everything else is rejected with a message naming the accepted forms (`UNSUPPORTED_CALL_SHAPE`).
//! - Arguments are pre-inferred by the registry common path before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "class_alias",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ClassAlias,
    ),
}

/// Rejects any direct `class_alias()` call that reaches the type checker.
///
/// AOT compilation resolves `class_alias()` at the top-level statement stage only, and only when
/// both class names are compile-time constants. A call in another context, or one whose names
/// are only known at run time (a variable, `$object::class`), is not supported and is rejected
/// here with a message naming the forms that are accepted.
///
/// A call whose class names the compiler resolved statically (`resolved_class_alias_args`) is
/// accepted and answers `Bool`; a runtime-dependent one is rejected.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some((_, alias)) = crate::autoload::resolved_class_alias_args(cx.args) {
        // An interface's alias is synthesized as an interface (`autoload::alias`).
        if cx.checker.classes.contains_key(&alias) || cx.checker.interfaces.contains_key(&alias) {
            return Ok(PhpType::Bool);
        }
    }
    // A closed world compiles every class, so a run-time alias only names one of them: the
    // backend records it in the table the by-name lookups resolve through.
    if crate::closed_world::is_enabled() {
        return Ok(PhpType::Bool);
    }
    Err(CompileError::new(cx.span, UNSUPPORTED_CALL_SHAPE))
}

/// The diagnostic for a `class_alias()` call the top-level alias collector could not turn into a
/// declaration. It lists the accepted class-name forms so a rejected call points at a fix.
const UNSUPPORTED_CALL_SHAPE: &str = "class_alias() is only supported as a top-level statement \
    with compile-time-constant class names (string literals, Name::class, or a concatenation of them)";
