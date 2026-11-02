//! Purpose:
//! Proves that one already parsed numeric eval fragment cannot consult native metadata.
//!
//! Called from:
//! - Generated eval code after flushing caller scope and before interpreter execution.
//!
//! Key details:
//! - Undefined reads, reference/global/typed bindings and nonnumeric cells fail closed.
//! - Only straight-line numeric stores, expressions and returns qualify.
//! - No PHP runs while proving: runtime hooks only inspect existing cell tags.

use crate::abi::{ElephcEvalContext, ElephcEvalScope, ABI_VERSION};
use crate::eval_ir::{EvalBinOp, EvalConst, EvalExpr, EvalProgram, EvalStmt};
use crate::value::RuntimeCellHandle;
use crate::interpreter::{EVAL_TAG_BOOL, EVAL_TAG_FLOAT, EVAL_TAG_INT, EVAL_TAG_NULL, EVAL_TAG_STRING};
use std::collections::HashSet;

/// Returns one when metadata is already complete or the next fragment cannot need it.
///
/// # Safety
/// Handles must be null or live current-thread handles; source bytes must be readable
/// for `code_len` bytes. Scope entries must retain valid runtime cells throughout the call.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_can_defer_native_catalog(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { can_defer_inner(ctx, scope, code_ptr, code_len) })
        .unwrap_or(0)
}

/// Returns one when synchronizing old scope cells cannot invoke PHP without native metadata.
///
/// # Safety
/// All non-null handles must be live and current-thread; scope cells must remain valid.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_can_flush_without_native_catalog(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    global_scope: *mut ElephcEvalScope,
) -> i32 {
    std::panic::catch_unwind(|| {
        let Some(context) = (unsafe { ctx.as_ref() }) else { return 0; };
        if context.abi_version() != ABI_VERSION { return 0; }
        if context.native_catalog_is_ready() { return 1; }
        if !context.native_catalog_deferral_budget_remaining() { return 0; }
        if context.eval_scope_sync_requires_full() { return 0; }
        let (Some(scope), Some(globals)) = (unsafe { scope.as_ref() }, unsafe { global_scope.as_ref() })
            else { return 0; };
        #[cfg(not(test))]
        {
            use crate::interpreter::RuntimeValueOps;
            let mut values = crate::runtime_hooks::ElephcRuntimeOps::with_context(ctx);
            i32::from(trivial_scope_replacement(scope, |cell| values.type_tag(cell).ok())
                && trivial_scope_replacement(globals, |cell| values.type_tag(cell).ok()))
        }
        #[cfg(test)]
        {
            i32::from(trivial_scope_replacement(scope, |_| None)
                && trivial_scope_replacement(globals, |_| None))
        }
    }).unwrap_or(0)
}

/// Rejects existing cells or bindings whose replacement can invoke user code or type checks.
fn trivial_scope_replacement(
    scope: &ElephcEvalScope,
    mut tag: impl FnMut(RuntimeCellHandle) -> Option<u64>,
) -> bool {
    scope.entries_iter().all(|(name, entry)| {
        if !entry.flags().is_visible() { return true; }
        !entry.flags().by_ref && scope.reference_target(name).is_none()
            && !scope.is_global_alias(name) && scope.local_type(name).is_none()
            && !entry.cell().is_null()
            && matches!(tag(entry.cell()), Some(EVAL_TAG_INT | EVAL_TAG_STRING | EVAL_TAG_FLOAT
                | EVAL_TAG_BOOL | EVAL_TAG_NULL))
    })
}

/// Validates handles before inspecting the cached syntax and numeric scope cells.
///
/// # Safety
/// Mirrors the exported wrapper's handle and source lifetime requirements.
unsafe fn can_defer_inner(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
) -> i32 {
    let Some(context) = (unsafe { ctx.as_ref() }) else { return 0; };
    if context.abi_version() != ABI_VERSION { return 0; }
    if context.native_catalog_is_ready() { return 1; }
    if !context.native_catalog_deferral_budget_remaining() { return 0; }
    let Some(scope) = (unsafe { scope.as_ref() }) else { return 0; };
    let Ok(len) = usize::try_from(code_len) else { return 0; };
    if context.eval_scope_sync_requires_full() || (len != 0 && code_ptr.is_null())
        || len > 64 * 1024 { return 0; }
    let code = if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(code_ptr, len) } };
    let Ok(program) = crate::parse_cache::parse_fragment_cached(code) else { return 0; };
    #[cfg(not(test))]
    {
        use crate::interpreter::RuntimeValueOps;
        let mut values = crate::runtime_hooks::ElephcRuntimeOps::with_context(ctx);
        let globals = context.global_scope_ptr().and_then(|ptr| unsafe { ptr.as_ref() });
        i32::from(metadata_free_fragment(&program, scope, globals, |cell| values.type_tag(cell).ok()))
    }
    #[cfg(test)]
    {
        // Unit-test binaries do not link the generated runtime cell-tag wrapper.
        let _ = (program, scope);
        0
    }
}

/// Requires every retained scope cell to remain callback-free even during context destruction.
pub(in crate::ffi) fn metadata_free_fragment(
    program: &EvalProgram,
    scope: &ElephcEvalScope,
    globals: Option<&ElephcEvalScope>,
    mut tag: impl FnMut(RuntimeCellHandle) -> Option<u64>,
) -> bool {
    trivial_scope_replacement(scope, &mut tag)
        && globals.is_none_or(|scope| trivial_scope_replacement(scope, &mut tag))
        && numeric_fragment(program, scope, tag)
}

/// Checks syntax and current scope without evaluating any expression or mutating PHP state.
fn numeric_fragment(
    program: &EvalProgram,
    scope: &ElephcEvalScope,
    tag: impl FnMut(RuntimeCellHandle) -> Option<u64>,
) -> bool {
    if program.native_generic_syntax() || !program.cli_superglobals().is_empty() { return false; }
    let mut proof = NumericProof { scope, tag, assigned: HashSet::new(), remaining: 256 };
    program.statements().iter().all(|statement| match statement {
        EvalStmt::StoreVar { name, value } => {
            if !proof.binding(name, true) || !proof.expression(value, 0) { return false; }
            proof.assigned.insert(name.clone());
            true
        }
        EvalStmt::Expr(value) | EvalStmt::Return(Some(value)) => proof.expression(value, 0),
        EvalStmt::Return(None) => true,
        _ => false,
    })
}

/// Tracks names assigned numeric values earlier in the same straight-line fragment.
struct NumericProof<'a, F> {
    scope: &'a ElephcEvalScope,
    tag: F,
    assigned: HashSet<String>,
    remaining: usize,
}

impl<F: FnMut(RuntimeCellHandle) -> Option<u64>> NumericProof<'_, F> {
    /// Requires ordinary untyped local storage and an existing numeric cell before reading or replacing it.
    fn binding(&mut self, name: &str, allow_missing: bool) -> bool {
        if matches!(name, "this" | "GLOBALS" | "_SESSION")
            || crate::eval_ir::EVAL_CLI_POPULATED_SUPERGLOBALS.contains(&name)
            || self.scope.is_global_alias(name) || self.scope.reference_target(name).is_some()
            || self.scope.local_type(name).is_some() { return false; }
        if self.assigned.contains(name) { return true; }
        let Some(entry) = self.scope.entry(name) else { return allow_missing; };
        if entry.flags().by_ref { return false; }
        if !entry.flags().is_visible() { return allow_missing; }
        !entry.cell().is_null() && matches!((self.tag)(entry.cell()), Some(EVAL_TAG_INT | EVAL_TAG_FLOAT))
    }

    /// Allows only numeric constants, initialized numeric reads and add/subtract/multiply.
    fn expression(&mut self, value: &EvalExpr, depth: usize) -> bool {
        if self.remaining == 0 || depth > 32 { return false; }
        self.remaining -= 1;
        match value {
            EvalExpr::Const(EvalConst::Int(_) | EvalConst::Float(_)) => true,
            EvalExpr::LoadVar(name) => self.binding(name, false),
            EvalExpr::Binary { op: EvalBinOp::Add | EvalBinOp::Sub | EvalBinOp::Mul, left, right } =>
                self.expression(left, depth + 1) && self.expression(right, depth + 1),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::ScopeCellOwnership;

    /// Builds a fake visible scalar entry; tag inspection remains a test closure, never a dereference.
    fn scope() -> ElephcEvalScope {
        let mut scope = ElephcEvalScope::new();
        scope.set("sum", RuntimeCellHandle::from_raw(1usize as *mut _), ScopeCellOwnership::Borrowed);
        scope
    }

    /// Parses each candidate exactly as the production parse-cache entry would.
    fn accepts(code: &str, scope: &ElephcEvalScope, tag: Option<u64>) -> bool {
        let Ok(program) = crate::parser::parse_fragment(code.as_bytes()) else { return false; };
        numeric_fragment(&program, scope, |_| tag)
    }

    /// Numeric reads, promotion-capable operations and fresh locals need no symbol lookup.
    #[test]
    fn deferred_native_catalog_accepts_only_numeric_straight_line_code() {
        let scope = scope();
        for code in ["$sum=$sum+3;", "$sum+=3;", "$sum++;", "$sum=$sum*2-1;return $sum;",
            "$new=2;$sum=$new+3;", "return 1.5;", ""] {
            assert!(accepts(code, &scope, Some(EVAL_TAG_INT)), "{code}");
            assert!(accepts(code, &scope, Some(EVAL_TAG_FLOAT)), "{code}");
        }
        for tag in [1, 3, 4, 5, 6, 7, 8, 9, 10, 11] {
            assert!(!accepts("$sum=$sum+3;", &scope, Some(tag)), "tag {tag}");
            assert!(!accepts("$sum=3;", &scope, Some(tag)), "release tag {tag}");
        }
        assert!(!accepts("$sum=$sum+3;", &scope, None));
    }

    /// Warnings, callbacks, references, symbols, types and nonnumeric operators request the full catalog.
    #[test]
    fn deferred_native_catalog_rejects_every_metadata_or_warning_boundary() {
        let mut scope = scope();
        for code in ["$sum=$missing+3;", "$sum=strlen('a');", "$sum=PHP_INT_MAX;",
            "$sum=1/0;", "$sum='1'+3;", "$sum=true;", "$sum=null;",
            "throw new Exception();", "eval('1;');", "$sum=$this;", "$GLOBALS=3;",
            "if(true){$sum=3;}", "function f(){}", "$r=&$sum;"] {
            assert!(!accepts(code, &scope, Some(EVAL_TAG_INT)), "{code}");
        }
        scope.set_reference("alias", "sum", RuntimeCellHandle::from_raw(1usize as *mut _), ScopeCellOwnership::Borrowed);
        assert!(!accepts("$sum+=3;", &scope, Some(0)));
        let mut scope = super::tests::scope();
        scope.mark_global_alias("sum");
        assert!(!accepts("$sum+=3;", &scope, Some(0)));
        let mut scope = super::tests::scope();
        scope.declare_local_type("sum".into(), crate::eval_ir::EvalParameterType::new(
            vec![crate::eval_ir::EvalParameterTypeVariant::Int], false));
        assert!(!accepts("$sum+=3;", &scope, Some(0)));
    }

    /// Dropping arrays, objects, resources or indirect bindings can invoke PHP before eval starts.
    #[test]
    fn deferred_native_catalog_rejects_nontrivial_old_scope_cells_before_flush() {
        let mut scope = scope();
        for tag in [EVAL_TAG_INT, EVAL_TAG_STRING, EVAL_TAG_FLOAT, EVAL_TAG_BOOL, EVAL_TAG_NULL] {
            assert!(trivial_scope_replacement(&scope, |_| Some(tag)), "trivial tag {tag}");
        }
        for tag in [4, 5, 6, 7, 9, 10, 11] {
            assert!(!trivial_scope_replacement(&scope, |_| Some(tag)), "callback tag {tag}");
        }
        assert!(!trivial_scope_replacement(&scope, |_| None));
        scope.set_reference("alias", "sum", RuntimeCellHandle::from_raw(1usize as *mut _), ScopeCellOwnership::Borrowed);
        assert!(!trivial_scope_replacement(&scope, |_| Some(EVAL_TAG_INT)));
    }

    /// Unused objects must not survive in a metadata-free context and call back during its epilogue.
    #[test]
    fn deferred_native_catalog_rejects_unused_callback_owners() {
        let mut scope = scope();
        scope.set("unused", RuntimeCellHandle::from_raw(2usize as *mut _), ScopeCellOwnership::Borrowed);
        let program = crate::parser::parse_fragment(b"$sum+=3;").unwrap();
        let tag = |cell: RuntimeCellHandle| Some(if cell.as_ptr() as usize == 1 { EVAL_TAG_INT } else { 6 });
        assert!(numeric_fragment(&program, &scope, tag));
        assert!(!metadata_free_fragment(&program, &scope, None, tag));
        let globals = scope;
        let scope = super::tests::scope();
        assert!(!metadata_free_fragment(&program, &scope, Some(&globals), tag));
    }

    /// Proof work stays bounded even for a large or deeply nested parsed fragment.
    #[test]
    fn deferred_native_catalog_bounds_proof_work() {
        let scope = scope();
        let number = EvalExpr::Const(EvalConst::Int(1));
        let program = EvalProgram::new(0, vec![EvalStmt::Expr(number.clone()); 257]);
        assert!(!numeric_fragment(&program, &scope, |_| Some(0)));
        let mut expression = number.clone();
        for _ in 0..34 {
            expression = EvalExpr::Binary { op: EvalBinOp::Add,
                left: Box::new(expression), right: Box::new(number.clone()) };
        }
        let program = EvalProgram::new(0, vec![EvalStmt::Expr(expression)]);
        assert!(!numeric_fragment(&program, &scope, |_| Some(0)));
    }
}
