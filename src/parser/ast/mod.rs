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
    CallableTarget, CastType, Expr, ExprKind, InstanceOfTarget, MagicConstant, StaticReceiver,
};
pub use ffi::{CType, ExternField, ExternParam, PackedField};
pub use operators::BinOp;
pub use oop::{
    Attribute, AttributeGroup, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl,
    TraitAdaptation, TraitUse, Visibility,
};
pub use stmt::{CatchClause, Program, Stmt, StmtKind, UseItem, UseKind};
pub use types::TypeExpr;
