//! Purpose:
//! Implements PHP's variadic-argument introspection functions — `func_num_args()`,
//! `func_get_args()` and `func_get_arg($position)` — by desugaring them into ordinary
//! PHP the rest of the compiler already understands.
//!
//! Called from:
//! - `crate::pipeline::compile()`, right after `autoload::run` and before the AST optimizer.
//!
//! Key details:
//! - PHP lets any function be called with more positional arguments than it declares; the
//!   surplus is reachable only through these three functions. elephc models that by giving
//!   every function whose body uses one of them a hidden trailing variadic parameter
//!   `mixed ...$__elephc_func_args`, so the *existing* variadic call machinery (planner,
//!   EIR lowering, ABI) packs the surplus with no new ABI surface.
//! - Because the introspection calls are rewritten away here, no builtin registry entry
//!   exists for them. PHP accepts direct calls and literal `call_user_func*` forms, while
//!   rejecting a callback first stored in a variable or created with first-class syntax.
//! - The pass runs *after* name resolution and autoloading so autoloaded declarations are
//!   covered too. Call names are therefore matched on their unqualified last segment,
//!   case-insensitively, which accepts both the canonical `func_num_args` and the
//!   `Foo\func_num_args` a namespaced unqualified call resolves to when no such user
//!   function exists. A program that declares its own function with one of the three names
//!   disables the pass entirely (see `program_declares_introspection_name`).
//! - Supported scopes are functions, methods (instance and static) and closures/arrow
//!   functions. Source variadics are snapshotted on entry so later writes do not rewrite the
//!   argument history. Optional parameters carry internal count metadata so omitted defaults
//!   remain distinguishable from arguments the caller supplied.

mod build;
mod walk;

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{ClassMethod, Program, Stmt, StmtKind};
use crate::types::FunctionSig;

/// Name of the hidden variadic parameter that collects the surplus positional arguments.
///
/// Reserved: user code cannot declare `$__elephc_func_args` and reach this slot, and the
/// name never appears in a PHP-visible signature position because it is added after the
/// source declaration has been parsed.
pub(crate) const HIDDEN_ARGS_PARAM: &str = "__elephc_func_args";

/// Hidden regular parameter carrying the actual count when a source variadic owns the tail slot.
pub(crate) const HIDDEN_ARGC_PARAM: &str = "__elephc_func_argc";

/// Name of the hidden local that holds the evaluated `func_get_arg()` position when the
/// position expression is not already side-effect free, so it is evaluated exactly once
/// across the range checks and the indexed read.
const POSITION_TEMP: &str = "__elephc_func_arg_pos";

/// The three PHP argument-introspection functions this pass rewrites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntrospectionCall {
    /// `func_num_args()` — the number of arguments actually passed.
    NumArgs,
    /// `func_get_args()` — a fresh list of the arguments actually passed.
    GetArgs,
    /// `func_get_arg($position)` — one argument by zero-based position.
    GetArg,
}

impl IntrospectionCall {
    /// Returns the introspection call named by `name`, matching PHP's case-insensitive
    /// function names on the unqualified last segment so `func_num_args`,
    /// `\func_num_args` and the `Foo\func_num_args` produced by resolving an unqualified
    /// call inside a namespace all map to the same construct.
    fn from_name(name: &Name) -> Option<Self> {
        let segment = name.last_segment()?;
        Self::from_segment(segment)
    }

    /// Returns the introspection call spelled by a single unqualified identifier.
    fn from_segment(segment: &str) -> Option<Self> {
        if segment.eq_ignore_ascii_case("func_num_args") {
            Some(Self::NumArgs)
        } else if segment.eq_ignore_ascii_case("func_get_args") {
            Some(Self::GetArgs)
        } else if segment.eq_ignore_ascii_case("func_get_arg") {
            Some(Self::GetArg)
        } else {
            None
        }
    }

    /// Returns the canonical PHP spelling, used in every diagnostic this pass emits.
    fn php_name(self) -> &'static str {
        match self {
            Self::NumArgs => "func_num_args",
            Self::GetArgs => "func_get_args",
            Self::GetArg => "func_get_arg",
        }
    }

    /// Returns how many arguments PHP's signature accepts: none for `func_num_args()` and
    /// `func_get_args()`, exactly one (`$position`) for `func_get_arg()`.
    fn arity(self) -> usize {
        match self {
            Self::NumArgs | Self::GetArgs => 0,
            Self::GetArg => 1,
        }
    }
}

/// Rewrites every supported use of `func_num_args()`, `func_get_args()` and
/// `func_get_arg()` into plain PHP, adding the hidden variadic parameter to each function
/// scope that needed one.
///
/// Returns the rewritten program, or the combined diagnostics for every unsupported use.
/// A program that declares its own function named after one of the three constructs is
/// returned untouched, so the user's declaration keeps winning exactly as it does in PHP.
pub fn desugar(program: Program) -> Result<Program, CompileError> {
    let capture_all_frames = walk::program_uses_backtrace(&program);
    let rewrite_introspection = !program_declares_introspection_name(&program);
    if !rewrite_introspection && !capture_all_frames {
        return Ok(program);
    }
    let mut program = program;
    let mut rewriter = walk::Rewriter::new(capture_all_frames, rewrite_introspection);
    rewriter.walk_stmts(&mut program);
    match rewriter.into_errors() {
        errors if errors.is_empty() => Ok(program),
        errors => Err(CompileError::from_many(errors)),
    }
}

/// Returns whether `sig`'s variadic parameter is the hidden one this pass appends rather
/// than a variadic the source function declared.
///
/// The distinction matters at every call site: PHP accepts unknown *named* arguments for a
/// user-declared variadic (they become string-keyed variadic entries) but rejects them for
/// a function that declares no variadic at all — which, as far as the program is concerned,
/// is exactly what a scope carrying only the hidden parameter still is.
pub(crate) fn sig_collects_surplus_args(sig: &FunctionSig) -> bool {
    sig.variadic.as_deref() == Some(HIDDEN_ARGS_PARAM)
}

/// Returns whether the hidden collector must begin with the actual PHP argument count.
pub(crate) fn sig_collects_optional_arg_count(sig: &FunctionSig) -> bool {
    if !sig_collects_surplus_args(sig) {
        return false;
    }
    let regular = crate::types::call_args::regular_param_count(sig);
    sig.defaults
        .iter()
        .take(regular)
        .any(|default| default.is_some())
}

/// Returns whether a source-variadic signature carries the internal actual-count parameter.
pub(crate) fn sig_has_hidden_argc_param(sig: &FunctionSig) -> bool {
    sig.params
        .iter()
        .any(|(name, _)| name == HIDDEN_ARGC_PARAM)
}

/// Returns whether the program declares a function or method named after one of the three
/// introspection constructs.
///
/// PHP resolves an unqualified call inside a namespace to the namespaced function when one
/// exists, so such a program must keep its own definition. Detecting the name anywhere is
/// deliberately conservative: the cost of a false positive is that the introspection
/// constructs stay unsupported in that one program, which is exactly the behaviour before
/// this pass existed.
fn program_declares_introspection_name(program: &[Stmt]) -> bool {
    program.iter().any(stmt_declares_introspection_name)
}

/// Returns whether a statement — or any statement nested inside it — declares a function
/// or method whose name collides with one of the three introspection constructs.
fn stmt_declares_introspection_name(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, body, .. } => {
            declared_name_collides(name) || body.iter().any(stmt_declares_introspection_name)
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. } => methods.iter().any(method_declares_introspection_name),
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. } => body.iter().any(stmt_declares_introspection_name),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_declares_introspection_name)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body.iter().any(stmt_declares_introspection_name))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_declares_introspection_name))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_declares_introspection_name)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_declares_introspection_name))
        }
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_deref().is_some_and(stmt_declares_introspection_name)
                || update.as_deref().is_some_and(stmt_declares_introspection_name)
                || body.iter().any(stmt_declares_introspection_name)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body.iter().any(stmt_declares_introspection_name))
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_declares_introspection_name))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_declares_introspection_name)
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(stmt_declares_introspection_name))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_declares_introspection_name))
        }
        // Remaining statements cannot introduce a function declaration. A closure body can,
        // but a function declared inside a closure only becomes visible once the closure
        // runs, so it can never be the compile-time resolution target of a call this pass
        // rewrites.
        _ => false,
    }
}

/// Returns whether a class/trait/interface/enum method body declares a function whose name
/// collides with one of the three introspection constructs. The method's own name cannot
/// collide: methods are reached through `->`/`::` call syntax, never as free functions.
fn method_declares_introspection_name(method: &ClassMethod) -> bool {
    method.body.iter().any(stmt_declares_introspection_name)
}

/// Returns whether a declared function name collides with one of the three introspection
/// constructs, comparing the unqualified last segment case-insensitively.
fn declared_name_collides(name: &str) -> bool {
    let segment = name.rsplit('\\').next().unwrap_or(name);
    IntrospectionCall::from_segment(segment).is_some()
}
