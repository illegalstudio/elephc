//! Purpose:
//! Exposes generated direct-AST DatePeriod checker metadata.
//! Keeps parser-backed source models available only to test oracles.
//!
//! Called from:
//! - crate::types::checker::driver through inject_builtin_date_period.
//!
//! Key details:
//! - DatePeriod remains an IteratorAggregate with independent getIterator snapshots.
//! - Production injection never parses embedded PHP source.

#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
use crate::names::Name;
#[cfg(test)]
use crate::parser::ast::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt,
    StaticReceiver, StmtKind, TypeExpr, Visibility,
};
#[cfg(test)]
use crate::types::traits::FlattenedClass;

#[allow(dead_code)]
#[cfg(test)]
mod bodies;
#[cfg(test)]
mod compliance_core;
#[cfg(test)]
pub(super) mod compliance_state;
