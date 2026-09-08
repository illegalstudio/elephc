//! Purpose:
//! Collects the PHP variable names that any function-like body in a program declares with
//! `global`, so the checker and EIR lowering answer "does program storage back this name?" from
//! ONE walk instead of two that can drift.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl` (once per check, before the first walk)
//! - `crate::ir_lower::function::lower_main` and the per-body lowering entry points
//!
//! Key details:
//! - `global $x;` inside a function/method/closure binds `$x` to the program-global cell the TOP
//!   LEVEL also writes through its own local slot. The checker must therefore not end a top-level
//!   binding of such a name (`unset` would leave the name unbound while another body still reaches
//!   the storage by name), and lowering must not abandon its slot. Both sides read this set.
//! - ONE walk, SHARED, so the checker can never approve a decision lowering refuses (or vice
//!   versa). It descends into STATEMENT bodies only: a `global` written inside a closure body, an
//!   assignment prelude or an enum method is invisible to both sides at once.
//! - That blind spot is deliberate, and BOTH directions of widening it were measured and rejected:
//!   - Widening the set LOWERING reads changes STORAGE CLASS. Moving a top-level name into the
//!     `_eir_global_*` symbol types it `Mixed`, and the array builtins have pre-existing
//!     `Mixed`-array backend gaps, so previously-correct programs broke:
//!     `$d = function () { global $a; }; $a = [3, 1, 2]; echo implode(",", $a);` crashed
//!     (PHP: `3,1,2`), and `array_sum`/`sort`/`usort`/`in_array`/`array_map`/`array_keys`/
//!     `array_reverse` on such a name became a hard `unsupported EIR backend feature: … for PHP
//!     type Mixed`.
//!   - Widening only the set the CHECKER's `unset`-kill veto reads has no such coupling, but it is
//!     not free either: it withholds the kill from every name a nested body merely MENTIONS, so
//!     `$a = $argc; unset($a); $f = function () { global $a; }; $a = "s";` — accepted in both modes
//!     today — became a permissive retype warning and a hard `cannot reassign` under
//!     `--strict-locals`. Its only purchase was turning an honest `Undefined variable` compile
//!     error into SILENT EMPTY output, because the nested body's write still went to storage main
//!     no longer reads. A compile error beats a silent wrong answer, so the veto reads this set.
//! - What both sides therefore keep: a `global` written inside a closure or an enum method does not
//!   reach main's storage, so that write is lost. PRE-EXISTING and tracked separately; its real fix
//!   is blocked on the `Mixed`-array backend gaps above, and closing those would let the wider walk
//!   be adopted on both sides at once.
//! - The walk is EXHAUSTIVE on `StmtKind` on purpose. Every arm that carries no statement body is
//!   listed as a deliberate no-op rather than swept into a catch-all, so a new variant that CAN
//!   hold a body has to be classified here instead of silently becoming an unnoticed blind spot —
//!   which is how the enum-method arm went unexamined until this campaign.
//! - `StmtKind::PackedClassDecl` is not one of those blind spots at all: a packed class declares
//!   typed FIELDS only, with no method bodies and no expressions, so there is nothing in it to walk.

use std::collections::HashSet;

use crate::parser::ast::{Stmt, StmtKind};

/// Collects the PHP variable names that any function-like STATEMENT body in `statements` declares
/// `global`.
///
/// The one set both EIR lowering and the checker's `unset`-kill veto read. It deliberately does NOT
/// see a `global` written inside a closure body, an assignment prelude or an enum method — see the
/// module preamble for the measurements behind that.
pub(crate) fn collect_global_var_names(statements: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_in_body(statements, &mut names);
    names
}

/// Recursively scans statement bodies for `global` declarations.
fn collect_in_body(statements: &[Stmt], names: &mut HashSet<String>) {
    for stmt in statements {
        collect_in_stmt(stmt, names);
    }
}

/// Scans one statement and every statement body nested inside it.
///
/// Exhaustive on `StmtKind`: a new statement that can carry a body has to be classified here rather
/// than defaulting to "declares nothing".
fn collect_in_stmt(stmt: &Stmt, names: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Global { vars } => {
            names.extend(vars.iter().cloned());
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            collect_in_body(then_body, names);
            for (_, body) in elseif_clauses {
                collect_in_body(body, names);
            }
            if let Some(body) = else_body {
                collect_in_body(body, names);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_in_body(then_body, names);
            if let Some(body) = else_body {
                collect_in_body(body, names);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::FunctionDecl { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => {
            collect_in_body(body, names);
        }
        StmtKind::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                collect_in_stmt(init, names);
            }
            if let Some(update) = update {
                collect_in_stmt(update, names);
            }
            collect_in_body(body, names);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                collect_in_body(body, names);
            }
            if let Some(body) = default {
                collect_in_body(body, names);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_in_body(try_body, names);
            for catch in catches {
                collect_in_body(&catch.body, names);
            }
            if let Some(body) = finally_body {
                collect_in_body(body, names);
            }
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. } => {
            for method in methods {
                collect_in_body(&method.body, names);
            }
        }
        // An enum's methods are NOT walked, and no arm below descends into an EXPRESSION (which is
        // where a closure literal's body and an assignment's synthesized prelude live). All three
        // are reached through the `_eir_global_*` symbol, so collecting a `global` from them would
        // move main's binding into program storage and hit the `Mixed`-array backend gaps the
        // module preamble measures. Listed explicitly rather than left to a catch-all so the
        // omission is a decision on the record: these are the blind spots investigated and
        // deliberately kept.
        StmtKind::EnumDecl { .. }
        // A packed class declares typed FIELDS only — no bodies, no expressions, nothing to walk.
        | StmtKind::PackedClassDecl { .. }
        // Externs are C declarations: types and names, never PHP statements.
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        // Statements whose only children are EXPRESSIONS. A `global` reaches one only through a
        // closure body or an assignment prelude, which is the blind spot above.
        | StmtKind::Echo(_)
        | StmtKind::Throw(_)
        | StmtKind::ExprStmt(_)
        | StmtKind::Assign { .. }
        | StmtKind::TypedAssign { .. }
        | StmtKind::ConstDecl { .. }
        | StmtKind::ListUnpack { .. }
        | StmtKind::ArrayPush { .. }
        | StmtKind::StaticPropertyAssign { .. }
        | StmtKind::StaticPropertyArrayPush { .. }
        | StmtKind::StaticPropertyArrayAssign { .. }
        | StmtKind::RefAssign { .. }
        | StmtKind::StaticVar { .. }
        | StmtKind::ArrayAssign { .. }
        | StmtKind::NestedArrayAssign { .. }
        | StmtKind::PropertyAssign { .. }
        | StmtKind::DynamicPropertyArrayPush { .. }
        | StmtKind::PropertyArrayPush { .. }
        | StmtKind::PropertyArrayAssign { .. }
        | StmtKind::Return(_)
        | StmtKind::Include { .. }
        // Leaves: no sub-statements and no sub-expressions.
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        // Variant groups/marks carry function NAMES; the bodies live in their own `FunctionDecl`s.
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. } => {}
    }
}
