//! Purpose:
//! Lowers parsed list destructuring patterns into ordinary assignment statements.
//! Creates temporary value access expressions for positional and keyed destructuring entries.
//!
//! Called from:
//! - `crate::parser::stmt::assign::list::parse_list_unpack()`.
//!
//! Key details:
//! - Lowering preserves assignment target spans and skips invalid append targets that cannot be represented safely.

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

use super::{ListEntry, ListPattern, ListTarget};

/// Lowers a `ListPattern` into either a `ListUnpack` statement (when all entries are simple
/// positional local variables) or a sequence of ordinary `Assign` statements using temporary
/// variables to anchor each list element access.
///
/// Returns a single `ListUnpack` stmt when the pattern contains only `$var` targets in
/// positional order; otherwise emits a chain of assignments through synthetic temporaries.
pub(super) fn lower_list_unpack(pattern: ListPattern, value: Expr, span: Span) -> Stmt {
    if let Some(vars) = simple_local_positional_vars(&pattern) {
        return Stmt::new(StmtKind::ListUnpack { vars, value }, span);
    }

    let mut lowerer = ListLowerer::new(span);
    let source = lowerer.bind_temp(value);
    lowerer.lower_pattern(&pattern, source);
    Stmt::new(StmtKind::Synthetic(lowerer.stmts), span)
}

/// Returns `Some(vars)` if every entry in the pattern is a bare `$variable` with no key and
/// no nesting. In that case the pattern can be handled by the simpler `ListUnpack` stmt form.
/// Returns `None` if any entry involves a computed key, a non-variable target, a nested
/// pattern, or an append target.
fn simple_local_positional_vars(pattern: &ListPattern) -> Option<Vec<String>> {
    let mut vars = Vec::new();
    for entry in &pattern.entries {
        match entry {
            ListEntry::Target {
                key: None,
                target: ListTarget::Expr(expr),
            } => match &expr.kind {
                ExprKind::Variable(name) => vars.push(name.clone()),
                _ => return None,
            },
            _ => return None,
        }
    }
    Some(vars)
}

/// Stateful lowerer that accumulates `Stmt`s as it walks a `ListPattern`, generating
/// temporary variable bindings to anchor intermediate values.
struct ListLowerer {
    /// Span used for all emitted statements and expressions.
    span: Span,
    /// Counter for generating unique temporary variable names.
    next_temp: usize,
    /// Statements emitted during lowering, in order.
    stmts: Vec<Stmt>,
}

impl ListLowerer {
    /// Initialises the lowerer with the span to use for all emitted statements and a
    /// zero-initialized temp counter.
    fn new(span: Span) -> Self {
        Self {
            span,
            next_temp: 0,
            stmts: Vec::new(),
        }
    }

    /// Binds `value` to a fresh temporary variable by emitting an `Assign` statement, then
    /// returns an expression referring to that temporary. The caller uses the returned
    /// expression as the source for subsequent `lower_pattern` or `lower_target` calls.
    fn bind_temp(&mut self, value: Expr) -> Expr {
        let name = self.next_temp_name();
        self.stmts.push(Stmt::new(
            StmtKind::Assign {
                name: name.clone(),
                value,
            },
            self.span,
        ));
        Expr::new(ExprKind::Variable(name), self.span)
    }

    /// Walks each entry of `pattern` in order. For each entry with no explicit key, emits an
    /// integer literal key based on its position. Creates an `ArrayAccess` expression from
    /// `source` and the (positional or keyed) key, then delegates to `lower_target`.
    fn lower_pattern(&mut self, pattern: &ListPattern, source: Expr) {
        for (index, entry) in pattern.entries.iter().enumerate() {
            let ListEntry::Target { key, target } = entry else {
                continue;
            };
            let key_expr = key.clone().unwrap_or_else(|| Expr::int_lit(index as i64));
            let value = Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(source.clone()),
                    index: Box::new(key_expr),
                },
                self.span,
            );
            self.lower_target(target, value);
        }
    }

    /// Dispatches on the variant of `target`:
    /// - `Nested`: binds the value to a temp then recursively lowers the nested pattern.
    /// - `Expr`: produces an ordinary assignment via `lower_assignment_target`, if the target
    ///   expression kind is supported.
    /// - `Append`: produces an array-push statement via `lower_append_target`, if supported.
    /// Skips targets that cannot be safely represented, adding nothing to `stmts` in that case.
    fn lower_target(&mut self, target: &ListTarget, value: Expr) {
        match target {
            ListTarget::Nested(pattern) => {
                let nested_source = self.bind_temp(value);
                self.lower_pattern(pattern, nested_source);
            }
            ListTarget::Expr(expr) => {
                if let Some(stmt) = lower_assignment_target(expr.clone(), value, self.span) {
                    self.stmts.push(stmt);
                }
            }
            ListTarget::Append(base) => {
                if let Some(stmt) = lower_append_target(base.clone(), value, self.span) {
                    self.stmts.push(stmt);
                }
            }
        }
    }

    /// Returns a unique temporary variable name using the source span line/column and an
    /// incrementing counter. The format is `__elephc_list_{line}_{col}_{N}`.
    fn next_temp_name(&mut self) -> String {
        let name = format!(
            "__elephc_list_{}_{}_{}",
            self.span.line, self.span.col, self.next_temp
        );
        self.next_temp += 1;
        name
    }
}

/// Converts a bare expression `target` and a value into an assignment statement kind,
/// mapping supported expression forms to the corresponding `StmtKind` variant:
/// - `Variable(name)` → `Assign`
/// - `ArrayAccess` on a `Variable` → `ArrayAssign`
/// - `ArrayAccess` on a `PropertyAccess` → `PropertyArrayAssign`
/// - `ArrayAccess` on a `StaticPropertyAccess` → `StaticPropertyArrayAssign`
/// - `PropertyAccess` → `PropertyAssign`
/// - `StaticPropertyAccess` → `StaticPropertyAssign`
///
/// Returns `None` if the target expression kind is not a supported lvalue form.
fn lower_assignment_target(target: Expr, value: Expr, span: Span) -> Option<Stmt> {
    let kind = match target.kind {
        ExprKind::Variable(name) => StmtKind::Assign { name, value },
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => StmtKind::ArrayAssign {
                array,
                index: *index,
                value,
            },
            ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                object,
                property,
                index: *index,
                value,
            },
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                }
            }
            _ => return None,
        },
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
            object,
            property,
            value,
        },
        ExprKind::StaticPropertyAccess { receiver, property } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        },
        _ => return None,
    };
    Some(Stmt::new(kind, span))
}

/// Converts a `base` expression and a `value` into an array-push statement kind, mapping
/// supported receiver forms to the corresponding `StmtKind` variant:
/// - `Variable(array)` → `ArrayPush`
/// - `PropertyAccess` → `PropertyArrayPush`
/// - `StaticPropertyAccess` → `StaticPropertyArrayPush`
///
/// Returns `None` if the base expression kind does not support append semantics.
fn lower_append_target(base: Expr, value: Expr, span: Span) -> Option<Stmt> {
    let kind = match base.kind {
        ExprKind::Variable(array) => StmtKind::ArrayPush { array, value },
        ExprKind::PropertyAccess { object, property } => {
            StmtKind::PropertyArrayPush {
                object,
                property,
                value,
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            }
        }
        _ => return None,
    };
    Some(Stmt::new(kind, span))
}
