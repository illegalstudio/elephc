//! Purpose:
//! Lowers PHP's `goto` statements and their target labels into structured control flow
//! (`while`, `switch`, `continue N`, `break N`) right after a file is parsed, so every later
//! pass keeps reading the statement-shaped control flow it was written for.
//!
//! Called from:
//! - `crate::source::finalize_physical_program()` for each physical source file.
//! - `crate::eval_aot::planning::parse_literal_fragment()` for literal `eval` fragments.
//! - The integration-test harnesses that mirror the per-file finalization steps.
//!
//! Key details:
//! - The type checker, the AST optimizer, termination analysis and EIR lowering are all
//!   structural walks without a control-flow graph. An arbitrary jump would be unsound in each of
//!   them, so the jump is removed here instead: after this pass no `StmtKind::Goto` exists.
//! - Each statement list that a jump lands in becomes a dispatch loop. A `goto` stores the
//!   label's number in a hidden state local and `continue`s to that loop, whose guarded
//!   segments skip ahead to the label. Leaving loops, `switch`es and `try` blocks on the way
//!   therefore runs exactly the cleanup a `continue N` runs, `finally` blocks included, as PHP
//!   does for `goto`.
//! - Labels are scoped per function, method, closure and file, never across them. The braced
//!   `namespace` blocks of a file share the file's scope, as in PHP. An included file is
//!   lowered on its own before include resolution splices it into the includer.
//! - PHP's own jump rules are diagnosed here with PHP's wording. A few shapes PHP accepts are
//!   refused with an explicit diagnostic: see `plan::Unsupported`.
//! - Every file is walked, with or without a `goto`: PHP rejects a duplicate label even when
//!   nothing jumps to it. A label no `goto` targets is simply dropped, so after this pass no
//!   `StmtKind::GotoLabel` exists either. Later passes still treat one as a no-op, for trees
//!   that were built without this pass.

mod analysis;
mod build;
mod plan;
mod rewrite;
#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};

use crate::errors::CompileError;
use crate::magic_constants::walker::{walk_program, Pass};
use crate::parser::ast::{ExprKind, MagicConstant, Program, Stmt, StmtKind};
use crate::span::Span;

/// Lowers every `goto` and label of one parsed file into structured control flow.
///
/// `scope_key` names the file (its path) and keeps the hidden state locals of different files
/// distinct, because an included file's top-level code shares its includer's variable scope.
///
/// Returns every jump diagnostic of the file at once when any jump or label is invalid.
pub fn lower_program(program: Program, scope_key: &str) -> Result<Program, CompileError> {
    let mut pass = GotoLowering::new(scope_key);
    let program = walk_program(program, &mut pass);
    let program = pass.lower_scope(program);
    let errors = pass.errors.into_inner();
    if errors.is_empty() {
        Ok(program)
    } else {
        Err(CompileError::from_many(errors))
    }
}

/// Returns whether `stmts` holds a `goto` or a label of this scope, at any statement depth.
///
/// Nested functions, classes and closures are separate label scopes and are not searched.
/// Braced `namespace` blocks belong to the file's scope and are.
pub(crate) fn contains_goto_or_label(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::Goto(_) | StmtKind::GotoLabel(_) => true,
        kind => plan::child_blocks(kind)
            .into_iter()
            .any(|(_, body)| contains_goto_or_label(body)),
    })
}

/// The walker pass that lowers each label scope of a file, innermost scopes first.
struct GotoLowering {
    /// Hash of the file's scope key, spelled into every hidden state local of the file.
    file_key: u64,
    /// Number of hidden state locals minted so far in this file.
    next_state_local: Cell<usize>,
    /// Jump diagnostics collected across every scope of the file.
    errors: RefCell<Vec<CompileError>>,
}

impl GotoLowering {
    /// Creates the pass for one file identified by `scope_key`.
    fn new(scope_key: &str) -> Self {
        // FNV-1a: stable for a path within and across runs, like the anonymous-class scope key.
        let file_key = scope_key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
        Self {
            file_key,
            next_state_local: Cell::new(0),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// Mints the storage name of one hidden jump-state local.
    ///
    /// The name carries the generated-local marker, so it cannot collide with a PHP variable and
    /// stays out of `get_defined_vars()`, `compact()` and eval scope synchronization.
    fn fresh_state_local(&self) -> String {
        let index = self.next_state_local.get();
        self.next_state_local.set(index + 1);
        crate::names::generated_local_name(&format!(
            "__elephc_goto_{:016x}_{}",
            self.file_key, index
        ))
    }

    /// Lowers one label scope: validates its jumps and rewrites them, or records diagnostics
    /// and returns the body unchanged when a jump is invalid.
    fn lower_scope(&self, body: Vec<Stmt>) -> Vec<Stmt> {
        if !contains_goto_or_label(&body) {
            return body;
        }
        let body = analysis::isolate_finally_of_labelled_tries(body);
        let plan = match analysis::plan_scope(&body) {
            Ok(plan) => plan,
            Err(errors) => {
                self.errors.borrow_mut().extend(errors);
                return body;
            }
        };
        let (body, errors) = rewrite::rewrite_scope(body, &plan, &|| self.fresh_state_local());
        self.errors.borrow_mut().extend(errors);
        body
    }
}

impl Pass for GotoLowering {
    /// Leaves magic constants as written: this pass only restructures statements.
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    /// Lowers the body of a function, method or closure.
    fn transform_scope_body(&self, body: Vec<Stmt>) -> Vec<Stmt> {
        self.lower_scope(body)
    }
}
