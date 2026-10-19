//! Purpose:
//! Folds `function_exists('name')` for USER functions before type checking, so the PHP polyfill
//! idiom `if (! function_exists('f')) { function f() {} }` (framework `helpers.php` files)
//! materializes its declaration exactly when PHP would execute it.
//!
//! Called from:
//! - `crate::optimize::fold_constants_for_target()` (state setup),
//! - `crate::optimize::fold::expr` (the `function_exists` fold),
//! - `crate::optimize::control::fold` (`fold_block` / `target_dependent_condition`),
//! - `crate::optimize::target_guards::fold_callable_body()` (callable bodies are opaque).
//!
//! Key details:
//! - Before this pass the guard was never folded for a user name, the `if` survived, and the
//!   checker only registers TOP-LEVEL declarations, so the nested function silently vanished
//!   (`Undefined function: f`).
//! - Top-level code runs in program (= inclusion) order, so a literal `function_exists('f')`
//!   there is decided statically: true once a declaration of `f` is known (unconditional
//!   top-level declarations are hoisted, as PHP hoists them inside one file), false otherwise.
//!   When a `!function_exists('f')` guard folds to true, `fold_block` flattens its branch and
//!   [`record_materialized`] adds `f` to the inventory, so a later repetition of the same
//!   polyfill (Support vs Foundation helpers) folds to false and does not redeclare.
//! - Only names whose every nested declaration is such a CLEAN polyfill (directly in the
//!   then-branch of a top-level `if (!function_exists('<same name>'))` without elseif/else)
//!   are folded. Any other conditional declaration (inside a loop, a function body, another
//!   condition, an else branch...), an include-discovered function (`FunctionVariantGroup`
//!   / `FunctionVariantMark`, tracked at runtime in load order) or an extern makes the name OPAQUE and the call stays a
//!   runtime query. When top-level code calls `eval()` nothing folds to false.
//! - Inside callable bodies nothing is folded: a function body runs at an unknown time, when
//!   later polyfills may already exist; the runtime lowering answers from the final inventory.

use super::*;
use std::cell::Cell;

thread_local! {
    /// `None` outside the guarded fold; inside it, `Some(None)` when top-level code calls `eval()`
    /// and `Some(Some(opaque))` otherwise.
    static OPAQUE_USER_FUNCTIONS: RefCell<Option<Option<HashSet<String>>>> = const { RefCell::new(None) };
    static CALLABLE_DEPTH: Cell<usize> = const { Cell::new(0) };
    /// True while folding a block whose statements execute at top level, in program order
    /// (the program root and the bodies of `Synthetic`, `NamespaceBlock`, `IncludeOnceGuard`).
    static DECL_TOP: Cell<bool> = const { Cell::new(false) };
    /// True while folding a STATEMENT that itself sits in a [`DECL_TOP`] block.
    static STMT_TOP: Cell<bool> = const { Cell::new(false) };
}

/// Runs the target fold with the opaque-name inventory of `program` and top-level state set.
pub(super) fn with_user_guard_state<R>(program: Program, f: impl FnOnce(Program) -> R) -> R {
    // `eval()` can declare any function at runtime, so when top-level code calls it no literal
    // probe folds to false (the runtime lookup sees eval-declared functions).
    let inventory = (!top_level_code_calls_eval(&program)).then(|| {
        let mut opaque = HashSet::new();
        classify_block(&program, true, &mut opaque);
        opaque
    });
    let previous = OPAQUE_USER_FUNCTIONS.with(|slot| slot.replace(Some(inventory)));
    let previous_top = DECL_TOP.with(|slot| slot.replace(true));
    let previous_stmt = STMT_TOP.with(|slot| slot.replace(true));
    let previous_depth = CALLABLE_DEPTH.with(|slot| slot.replace(0));
    let result = f(program);
    CALLABLE_DEPTH.with(|slot| slot.set(previous_depth));
    STMT_TOP.with(|slot| slot.set(previous_stmt));
    DECL_TOP.with(|slot| slot.set(previous_top));
    OPAQUE_USER_FUNCTIONS.with(|slot| slot.replace(previous));
    result
}

/// Folds a callable body (function, method, closure) with user-name folding disabled.
pub(super) fn in_callable_body<R>(f: impl FnOnce() -> R) -> R {
    CALLABLE_DEPTH.with(|slot| slot.set(slot.get() + 1));
    let previous_top = DECL_TOP.with(|slot| slot.replace(false));
    let previous_stmt = STMT_TOP.with(|slot| slot.replace(false));
    let result = f();
    STMT_TOP.with(|slot| slot.set(previous_stmt));
    DECL_TOP.with(|slot| slot.set(previous_top));
    CALLABLE_DEPTH.with(|slot| slot.set(slot.get() - 1));
    result
}

/// Whether the statement being folded sits in a top-level (program-order) block.
pub(super) fn stmt_at_top() -> bool {
    STMT_TOP.with(Cell::get)
}

/// Folds one statement of a block: the statement is top-level iff its block is, and its own
/// nested blocks stay top-level only through declaration-transparent wrappers.
pub(super) fn fold_block_stmt<R>(transparent: bool, f: impl FnOnce() -> R) -> R {
    let here = DECL_TOP.with(Cell::get);
    let children = here && transparent;
    let previous_stmt = STMT_TOP.with(|slot| slot.replace(here));
    let previous_top = DECL_TOP.with(|slot| slot.replace(children));
    let result = f();
    DECL_TOP.with(|slot| slot.set(previous_top));
    STMT_TOP.with(|slot| slot.set(previous_stmt));
    result
}

/// Whether a statement's nested block keeps executing at its own (top-level) position.
pub(super) fn is_declaration_transparent(stmt: &Stmt) -> bool {
    matches!(
        stmt.kind,
        StmtKind::Synthetic(_) | StmtKind::NamespaceBlock { .. } | StmtKind::IncludeOnceGuard { .. }
    )
}

/// Adds declarations a folded top-level guard just materialized to the user-function inventory.
pub(super) fn record_materialized(stmts: &[Stmt]) {
    if !stmt_at_top() || active_fold_target().is_none() {
        return;
    }
    let mut found = HashSet::new();
    collect_top_level_user_functions_from_block(stmts, &mut found);
    if found.is_empty() {
        return;
    }
    ACTIVE_FOLD_USER_FUNCTIONS.with(|slot| {
        if let Some(functions) = slot.borrow_mut().as_mut() {
            functions.extend(found);
        }
    });
}

/// PHP's `function_exists()` needle: one optional leading `\`, nothing empty, no `\\` prefix.
fn needle(name: &str) -> Option<&str> {
    let bare = name.strip_prefix('\\').unwrap_or(name);
    (!bare.is_empty() && !bare.starts_with('\\')).then_some(bare)
}

/// Whether `name` is answered by the builtin/prelude fold rather than the user inventory.
fn builtin_like(bare: &str) -> bool {
    crate::builtins::registry::lookup(bare).is_some()
        || crate::name_resolver::is_global_date_procedural_alias(bare)
        || crate::stdlib_prelude::provides(bare)
        || crate::types::checker::builtins::canonical_builtin_function_name(bare).is_some()
}

/// Static value of a literal `function_exists(candidate)` for a USER function at this point of
/// top-level execution, or `None` when it must stay a runtime query (callable body, builtin
/// name, or a name with an opaque conditional declaration).
pub(in crate::optimize) fn user_function_exists_value(candidate: &str) -> Option<bool> {
    if active_fold_target().is_none() || CALLABLE_DEPTH.with(Cell::get) > 0 {
        return None;
    }
    let Some(bare) = needle(candidate) else {
        return Some(false);
    };
    if builtin_like(bare) {
        return None;
    }
    let key = php_symbol_key(bare);
    let opaque = OPAQUE_USER_FUNCTIONS.with(|slot| {
        slot.borrow().as_ref().map(|state| state.as_ref().map(|opaque| opaque.contains(&key)))
    })?;
    match opaque {
        // Top-level code calls eval: only a declaration already known to exist folds.
        None => active_fold_user_function_exists(bare).then_some(true),
        Some(true) => None,
        Some(false) => Some(active_fold_user_function_exists(bare)),
    }
}

/// Whether code that runs at top level (outside function, method and class bodies) calls `eval`.
///
/// An `eval` inside a callable body runs only when that callable is called; treating it as a
/// barrier would disable the fold for every program that merely contains one (Laravel's Carbon
/// mixins, psysh), and a body that evals a polyfill's name before the top-level guard runs is
/// not a pattern real code uses. Closures in top-level expressions still count.
fn top_level_code_calls_eval(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::EnumDecl { .. } => false,
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => top_level_code_calls_eval(body),
        _ => crate::opcache_prelude::detect::first_reference(
            std::slice::from_ref(stmt),
            crate::opcache_prelude::detect::Symbol::call_site("eval"),
        )
        .is_some(),
    })
}

/// Returns the literal name of a clean `!function_exists('name')` guard condition.
fn clean_guard_name(condition: &Expr) -> Option<&str> {
    let ExprKind::Not(inner) = &condition.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &inner.kind else {
        return None;
    };
    if !name
        .as_canonical()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("function_exists")
    {
        return None;
    }
    match args.as_slice() {
        [Expr { kind: ExprKind::StringLiteral(candidate), .. }] => needle(candidate),
        _ => None,
    }
}

/// Records every function declared at a non-top-level position other than a clean polyfill.
fn classify_block(stmts: &[Stmt], top: bool, opaque: &mut HashSet<String>) {
    for stmt in stmts {
        classify_stmt(stmt, top, opaque);
    }
}

fn classify_stmt(stmt: &Stmt, top: bool, opaque: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, body, .. } => {
            if !top {
                opaque.insert(php_symbol_key(name));
            }
            classify_block(body, false, opaque);
        }
        // Include-discovered functions keep a runtime load flag (`FunctionVariantGroup` and the
        // `FunctionVariantMark` left at the include site), which `function_exists` already
        // answers in load order.
        StmtKind::ExternFunctionDecl { name, .. }
        | StmtKind::FunctionVariantGroup { name, .. }
        | StmtKind::FunctionVariantMark { name, .. } => {
            opaque.insert(php_symbol_key(name));
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => classify_block(body, top, opaque),
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            let guard = (top && elseif_clauses.is_empty() && else_body.is_none())
                .then(|| clean_guard_name(condition))
                .flatten()
                .map(php_symbol_key);
            for inner in then_body {
                match (&inner.kind, &guard) {
                    (StmtKind::FunctionDecl { name, body, .. }, Some(guard))
                        if php_symbol_key(name) == *guard =>
                    {
                        classify_block(body, false, opaque);
                    }
                    _ => classify_stmt(inner, false, opaque),
                }
            }
            for (_, body) in elseif_clauses {
                classify_block(body, false, opaque);
            }
            if let Some(body) = else_body {
                classify_block(body, false, opaque);
            }
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            classify_block(then_body, false, opaque);
            if let Some(body) = else_body {
                classify_block(body, false, opaque);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => classify_block(body, false, opaque),
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                classify_block(body, false, opaque);
            }
            if let Some(body) = default {
                classify_block(body, false, opaque);
            }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            classify_block(try_body, false, opaque);
            for catch in catches {
                classify_block(&catch.body, false, opaque);
            }
            if let Some(body) = finally_body {
                classify_block(body, false, opaque);
            }
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. } => {
            for method in methods {
                classify_block(&method.body, false, opaque);
            }
        }
        _ => {}
    }
}
