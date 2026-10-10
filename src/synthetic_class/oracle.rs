//! Purpose:
//! Compares generated assignment local identities independently of source coordinates.
//!
//! Called from:
//! - PDO and MySQLi built-versus-parsed declaration oracles.
//!
//! Key details:
//! - Only compiler-reserved assignment names are alpha-renamed in debug renderings.
//! - Repeated names retain their identity, so mismatched alias relationships still fail.

use std::collections::HashMap;

/// Canonicalizes generated assignment names by first occurrence, preserving all other text.
pub(crate) fn assignment_names(rendered: &str) -> String {
    let prefix = "__elephc_assign_expr_";
    let mut names = HashMap::new();
    let mut cleaned = String::with_capacity(rendered.len());
    let mut rest = rendered;
    while let Some(at) = rest.find(prefix) {
        let context = &rest[..at];
        let is_local_name = context.ends_with("Variable(\"") || context.ends_with("Assign { name: \"");
        cleaned.push_str(context);
        rest = &rest[at..];
        let end = rest.find('"').unwrap_or(rest.len());
        let name = &rest[..end];
        let suffix = name[prefix.len()..].strip_suffix(crate::names::GENERATED_LOCAL_MARKER);
        if is_local_name && suffix.is_some_and(|suffix| !suffix.is_empty()
            && suffix.bytes().all(|byte| byte.is_ascii_digit() || byte == b'_'))
        {
            let next = names.len();
            let id = *names.entry(name.to_string()).or_insert(next);
            cleaned.push_str(&format!("{prefix}canonical_{id}{}", crate::names::GENERATED_LOCAL_MARKER));
        } else {
            cleaned.push_str(name);
        }
        rest = &rest[end..];
    }
    cleaned.push_str(rest);
    cleaned
}

/// Changing only generated source coordinates preserves the same alias graph.
#[test]
fn assignment_oracle_preserves_repeated_identity() {
    let parsed = r#"Variable("__elephc_assign_expr_12_4_0#gen") Variable("__elephc_assign_expr_12_4_1#gen") Variable("__elephc_assign_expr_12_4_0#gen")"#;
    let built = r#"Variable("__elephc_assign_expr_0_17_0#gen") Variable("__elephc_assign_expr_0_17_1#gen") Variable("__elephc_assign_expr_0_17_0#gen")"#;
    let broken = r#"Variable("__elephc_assign_expr_0_17_0#gen") Variable("__elephc_assign_expr_0_17_1#gen") Variable("__elephc_assign_expr_0_17_1#gen")"#;
    assert_eq!(assignment_names(parsed), assignment_names(built));
    assert_ne!(assignment_names(parsed), assignment_names(broken));
}

/// User names and AST structure are not erased by generated-name comparison.
#[test]
fn assignment_oracle_preserves_user_names_and_structure() {
    let rendered = r#"Variable("__elephc_assign_expr_12_4_0") Assignment(result_target: None)"#;
    assert_eq!(assignment_names(rendered), rendered);
    let literal = r#"StringLiteral("__elephc_assign_expr_12_4_0#gen")"#;
    assert_eq!(assignment_names(literal), literal);
    let class = r#"ClassDecl { name: "__elephc_assign_expr_12_4_0#gen" }"#;
    assert_eq!(assignment_names(class), class);
    assert_ne!(assignment_names("Assignment(result_target: None)"),
        assignment_names("Assignment(result_target: Some(Variable(\"value\")))"));
}

/// Built effectful assignments capture the RHS once and use distinct generated frame locals.
#[test]
fn built_assignment_captures_once_with_unique_names() {
    use super::{e_assign, e_call, e_dyn_prop, e_var};
    use crate::parser::ast::{ExprKind, StmtKind};
    let build = || e_assign(e_dyn_prop(e_var("object"), e_var("name")), e_call("rhs", vec![]));
    let first = build();
    let second = build();
    let name = |expr: &crate::parser::ast::Expr| {
        let ExprKind::Assignment { value, result_target, prelude, .. } = &expr.kind else {
            panic!("expected assignment");
        };
        assert_eq!(value.kind, result_target.as_ref().expect("result capture").kind);
        let ExprKind::Variable(name) = &value.kind else { panic!("expected captured value"); };
        assert_eq!(prelude.iter().filter(|stmt| matches!(
            &stmt.kind, StmtKind::Assign { value, .. } if matches!(value.kind, ExprKind::FunctionCall { .. })
        )).count(), 1);
        name.clone()
    };
    assert_ne!(name(&first), name(&second));
}
