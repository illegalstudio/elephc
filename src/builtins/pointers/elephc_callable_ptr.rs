//! Purpose:
//! Home of the internal `__elephc_callable_ptr` builtin: it reinterprets a
//! closure / first-class callable value as the raw pointer to its 64-byte callable
//! descriptor. This is the PHP-prelude half of the PDO Tier-D "decompose-at-PHP"
//! callback design: a `callable` is broken into (descriptor pointer, adapter
//! address) so that no bridge extern ever declares a `callable` parameter.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//! - The PDO prelude driver methods (`Pdo\Sqlite::createCollation`, and later
//!   `createFunction` / `createAggregate`).
//!
//! Key details:
//! - `internal: true` keeps it out of PHP-visible catalogs and the parity gate while
//!   remaining callable through `registry::is_supported`.
//! - `check` returns `PhpType::Pointer(None)`; the runtime value of a closure /
//!   first-class callable already IS its descriptor pointer, so lowering is a bare
//!   identity load guarded against string / array callables (whose value is a PHP
//!   string, not a descriptor).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "__elephc_callable_ptr",
    area: Internal,
    params: [value: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Reinterprets a closure / first-class callable as its raw descriptor pointer.",
    internal: true
}

/// Infers the argument type and returns `PhpType::Pointer(None)`.
///
/// The static callable kind (closure / first-class vs string / array) is not carried
/// by `PhpType::Callable`, so the string / array rejection happens at lowering where
/// the value's codegen type is available. The registry's `check_arity` enforces the
/// single-argument arity.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Pointer(None))
}

/// Lowers a `__elephc_callable_ptr` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_elephc_callable_ptr(ctx, inst)
}
