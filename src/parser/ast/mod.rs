//! Purpose:
//! Re-exports the AST node families used throughout the compiler frontend.
//! Keeps expressions, statements, operators, OOP declarations, FFI declarations, and types under one namespace.
//!
//! Called from:
//! - `crate::parser` construction code and every pass that walks or rewrites the AST.
//!
//! Key details:
//! - AST variants are cross-pass contracts; adding one requires auditing all walkers and lowerers.

mod expr;
mod ffi;
mod operators;
mod oop;
mod stmt;
mod types;

pub use expr::{
    is_compound_assignment_self_read, ArrayEntry, CallableTarget, CastType, Expr, ExprKind,
    InstanceOfTarget, MagicConstant, StaticReceiver,
};
pub use ffi::{CType, ExternField, ExternParam, PackedField};
pub use operators::BinOp;
pub use oop::{
    Attribute, AttributeGroup, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl,
    PropertyHooks, TraitAdaptation, TraitUse, Visibility,
};
pub use stmt::{CatchClause, Program, Stmt, StmtKind, UseItem, UseKind};
pub use types::TypeExpr;

/// Name prefix of the temporary a nested append (`$a[$k][] = $v`) reads its bucket into.
///
/// The parser mints it in `stmt::assign::postfix::lower_nested_append_assignment`; IR lowering
/// matches on it to recognize a nested-append `StmtKind::Synthetic` group and fuse it (see
/// `crate::ir_lower::stmt::nested_append`). It lives here, on the AST, because it is the shared
/// contract between those two — and it must not be reused by any other desugar, or that
/// recognizer would claim statements it does not own.
///
/// The minted name also carries `crate::names::GENERATED_LOCAL_MARKER` as a suffix, which is
/// what keeps it out of `get_defined_vars()` and eval scope synchronization; the prefix match
/// here is unaffected by that suffix.
pub const NESTED_APPEND_TEMP_PREFIX: &str = "__elephc_napp_";

/// Attribute the include rewrite puts on the `break` an included file's `return` becomes.
///
/// `resolver::engine_includes::confine_nested_returns` turns each `return` of an included file
/// into a `break` out to a `do { … } while (false)` wrapper. A `return` inside a `finally` is
/// legal PHP — it ends the file and discards any pending exception or return — but a `break` out
/// of a `finally` is not, and the checker refuses it. This marks the breaks that are really
/// returns, so the checker lets exactly those leave a `finally`.
///
/// UNFORGEABLE by construction: the parser rejects attributes on every non-declaration
/// statement, so no hand-written `break` can carry one. Lowering needs no change: a `finally`
/// body is duplicated at each exit and lowered after its frame is popped, so a jump out of it
/// skips the exceptional copy's rethrow — the same path a function's `return` in `finally`
/// already takes.
pub const INCLUDE_RETURN_BREAK_ATTRIBUTE: &str = "__elephc_include_return";
