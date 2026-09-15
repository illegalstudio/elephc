//! Purpose:
//! Synthesizes the PHP statement body of one `clone()` property-override applicator, so every
//! override write travels the ORDINARY property-assignment lowering (typed weak coercion, set
//! hooks, `__set`, dynamic-property storage, ownership) instead of a second, weaker path.
//!
//! Called from:
//! - `super::lower_clone_override_applicators()` through `crate::ir_lower::function`.
//!
//! Key details:
//! - The loop is a plain `foreach` over the runtime override array, so the engine inherits PHP's
//!   array iteration order, arbitrary key types and the loop's own container/key/value ownership
//!   on both the normal and the throwing path. The first failing arm throws out of the loop and
//!   the remaining keys are never applied, which is php's documented stop-at-first-error rule.
//! - Keys are stringified exactly once with `(string)`, which is what turns the integer key `7`
//!   into the property name `"7"`.
//! - A NUL-prefixed name is refused before any arm runs, matching php-src's mangled-name guard.
//! - An override entry that still belongs to a PHP reference set is refused right there, in
//!   iteration order, by the internal `__elephc_clone_override_reference_guard` builtin. php
//!   applies every earlier entry first and only throws once iteration REACHES the referenced
//!   one, so the refusal cannot be hoisted to a whole-array pre-scan without dropping writes
//!   php had already performed.
//! - A scope-selected ancestor private slot is written through `super::scoped_setters`, whose
//!   `$this` is typed with that ancestor, rather than through this body's own receiver.

use crate::names::Name;
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

use super::arms::OverrideArm;

/// Applicator parameter holding the clone, typed with the clone's own class.
pub(super) const THIS_PARAM: &str = "this";
/// Applicator parameter holding the runtime override array.
pub(super) const OVERRIDES_PARAM: &str = "__elephc_clone_overrides";
/// Loop key local.
const KEY_LOCAL: &str = "__elephc_clone_key";
/// Loop value local.
const VALUE_LOCAL: &str = "__elephc_clone_value";
/// Stringified property name local.
const NAME_LOCAL: &str = "__elephc_clone_name";
/// Internal builtin refusing one override entry that is still a PHP reference.
const REFERENCE_GUARD: &str = "__elephc_clone_override_reference_guard";

/// One declared property name plus the arm it resolves to and the symbol that arm calls.
pub(super) struct ResolvedArm {
    /// Property name the key must match for this arm to run.
    pub(super) property: String,
    /// What php does with a write to that name from this applicator's scope.
    pub(super) arm: OverrideArm,
    /// Scoped setter helper this arm calls, when the scope owns the slot.
    pub(super) scoped_helper: Option<String>,
}

/// Builds the complete applicator body for one `(runtime class, invocation scope)` pair.
pub(super) fn build(
    class_name: &str,
    arms: &[ResolvedArm],
    unknown: &OverrideArm,
) -> Vec<Stmt> {
    let span = Span::dummy();
    let mut body = vec![
        stmt(
            StmtKind::Assign {
                name: NAME_LOCAL.to_string(),
                value: expr(ExprKind::Cast {
                    target: CastType::String,
                    expr: Box::new(variable(KEY_LOCAL)),
                }),
            },
            span,
        ),
        nul_name_guard(),
        reference_override_guard(),
    ];
    body.push(name_dispatch_chain(class_name, arms, unknown));
    vec![stmt(
        StmtKind::Foreach {
            array: variable(OVERRIDES_PARAM),
            key_var: Some(KEY_LOCAL.to_string()),
            value_var: VALUE_LOCAL.to_string(),
            value_by_ref: false,
            body,
        },
        span,
    )]
}

/// Builds php-src's refusal for a property name whose first byte is NUL.
fn nul_name_guard() -> Stmt {
    stmt(
        StmtKind::If {
            condition: expr(ExprKind::FunctionCall {
                name: Name::unqualified("str_starts_with"),
                args: vec![variable(NAME_LOCAL), string("\0")],
            }),
            then_body: vec![throw_error(string(
                "Cannot access property starting with \"\\0\"",
            ))],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        Span::dummy(),
    )
}

/// Builds php's refusal for an entry whose value still belongs to a PHP reference set.
///
/// The loop's own value local travels with the call: a by-value `foreach` retains the entry's
/// boxed Mixed cell, and the guard has to discount that borrow rather than read it as a second
/// owner of the reference cell.
fn reference_override_guard() -> Stmt {
    stmt(
        StmtKind::ExprStmt(expr(ExprKind::FunctionCall {
            name: Name::unqualified(REFERENCE_GUARD),
            args: vec![
                variable(OVERRIDES_PARAM),
                variable(NAME_LOCAL),
                variable(VALUE_LOCAL),
            ],
        })),
        Span::dummy(),
    )
}

/// Builds the `if ($name === "p") { … } elseif … else { … }` chain over every declared name.
fn name_dispatch_chain(
    class_name: &str,
    arms: &[ResolvedArm],
    unknown: &OverrideArm,
) -> Stmt {
    let fallback = unknown_name_statements(class_name, unknown);
    let Some((first, rest)) = arms.split_first() else {
        return stmt(StmtKind::Synthetic(fallback), Span::dummy());
    };
    stmt(
        StmtKind::If {
            condition: name_matches(&first.property),
            then_body: arm_statements(class_name, first),
            elseif_clauses: rest
                .iter()
                .map(|arm| (name_matches(&arm.property), arm_statements(class_name, arm)))
                .collect(),
            else_body: Some(fallback),
        },
        Span::dummy(),
    )
}

/// Builds `$name === "<property>"`.
fn name_matches(property: &str) -> Expr {
    expr(ExprKind::BinaryOp {
        left: Box::new(variable(NAME_LOCAL)),
        op: BinOp::StrictEq,
        right: Box::new(string(property)),
    })
}

/// Builds the statements one resolved arm runs for a matched, statically known property name.
fn arm_statements(class_name: &str, resolved: &ResolvedArm) -> Vec<Stmt> {
    match &resolved.arm {
        OverrideArm::AssignThis => vec![property_assign(&resolved.property)],
        // `super::resolve_scoped_helper` refuses the build when the helper is missing, so this
        // arm always has one. Writing the clone's own visible slot instead would silently hit a
        // DIFFERENT property: the child's shadowing private slot, never the ancestor's.
        OverrideArm::AssignScoped { .. } => vec![scoped_setter_call(
            resolved
                .scoped_helper
                .as_deref()
                .expect("an AssignScoped arm carries its scoped setter helper"),
        )],
        OverrideArm::MagicSet => vec![magic_set_call()],
        OverrideArm::DynamicAssign => vec![dynamic_property_assign()],
        OverrideArm::Deny(message) => vec![throw_error(string(message))],
        OverrideArm::DenyDynamicCreation => {
            vec![throw_error(string(&format!(
                "Cannot create dynamic property {}::${}",
                class_name, resolved.property
            )))]
        }
    }
}

/// Builds what a key matching NO declared property name does on this class.
///
/// The name is only known at run time here, so the refusal message is composed from the class
/// name and the loop's own stringified key instead of a baked-in literal.
fn unknown_name_statements(class_name: &str, unknown: &OverrideArm) -> Vec<Stmt> {
    match unknown {
        OverrideArm::MagicSet => vec![magic_set_call()],
        OverrideArm::DynamicAssign => vec![dynamic_property_assign()],
        OverrideArm::Deny(message) => vec![throw_error(string(message))],
        // A declared-slot arm can never be the answer for an unmatched runtime name.
        OverrideArm::AssignThis
        | OverrideArm::AssignScoped { .. }
        | OverrideArm::DenyDynamicCreation => vec![throw_error(concat(
            string(&format!("Cannot create dynamic property {}::$", class_name)),
            variable(NAME_LOCAL),
        ))],
    }
}

/// Builds `$this->property = $value;` for a statically known, accessible slot.
fn property_assign(property: &str) -> Stmt {
    stmt(
        StmtKind::PropertyAssign {
            object: Box::new(expr(ExprKind::This)),
            property: property.to_string(),
            value: variable(VALUE_LOCAL),
        },
        Span::dummy(),
    )
}

/// Builds `_clone_set_<class>_<slot>($this, $value);` for a scope-selected ancestor slot.
fn scoped_setter_call(helper: &str) -> Stmt {
    stmt(
        StmtKind::ExprStmt(expr(ExprKind::FunctionCall {
            name: Name::unqualified(helper),
            args: vec![expr(ExprKind::This), variable(VALUE_LOCAL)],
        })),
        Span::dummy(),
    )
}

/// Builds `$this->__set($name, $value);`.
fn magic_set_call() -> Stmt {
    stmt(
        StmtKind::ExprStmt(expr(ExprKind::MethodCall {
            object: Box::new(expr(ExprKind::This)),
            method: "__set".to_string(),
            args: vec![variable(NAME_LOCAL), variable(VALUE_LOCAL)],
        })),
        Span::dummy(),
    )
}

/// Builds `$this->{$name} = $value;` for receivers with dynamic-property storage.
fn dynamic_property_assign() -> Stmt {
    stmt(
        StmtKind::ExprStmt(expr(ExprKind::Assignment {
            target: Box::new(expr(ExprKind::DynamicPropertyAccess {
                object: Box::new(expr(ExprKind::This)),
                property: Box::new(variable(NAME_LOCAL)),
            })),
            value: Box::new(variable(VALUE_LOCAL)),
            result_target: None,
            prelude: Vec::new(),
            conditional_value_temp: None,
        })),
        Span::dummy(),
    )
}

/// Builds `throw new Error(<message expression>);`.
fn throw_error(message: Expr) -> Stmt {
    stmt(
        StmtKind::Throw(expr(ExprKind::NewObject {
            class_name: Name::unqualified("Error"),
            args: vec![message],
        })),
        Span::dummy(),
    )
}

/// Builds `<left> . <right>`.
fn concat(left: Expr, right: Expr) -> Expr {
    expr(ExprKind::BinaryOp {
        left: Box::new(left),
        op: BinOp::Concat,
        right: Box::new(right),
    })
}

/// Builds a variable expression with the shared synthetic span.
fn variable(name: &str) -> Expr {
    expr(ExprKind::Variable(name.to_string()))
}

/// Builds a string literal expression with the shared synthetic span.
fn string(value: &str) -> Expr {
    expr(ExprKind::StringLiteral(value.to_string()))
}

/// Wraps an expression kind with the shared synthetic span.
fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::dummy())
}

/// Wraps a statement kind with an explicit span.
fn stmt(kind: StmtKind, span: Span) -> Stmt {
    Stmt::new(kind, span)
}
