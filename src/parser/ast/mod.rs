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
    is_compound_assignment_self_read, CallableTarget, CastType, Expr, ExprKind, InstanceOfTarget,
    MagicConstant, StaticReceiver,
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
pub const NESTED_APPEND_TEMP_PREFIX: &str = "__elephc_napp_";
