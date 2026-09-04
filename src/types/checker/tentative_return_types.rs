//! Purpose:
//! Computes php's `Return type of X::m() should either be compatible with ...` deprecation for
//! classes that override a built-in method whose return type php declares TENTATIVELY.
//!
//! Called from:
//! - `crate::types::checker::driver`, once the class map is complete.
//!
//! Key details:
//! - php raises this while LINKING the class, so it fires before the script produces anything and
//!   whether or not the method is ever called. elephc's equivalent of "before any output" is the
//!   main prologue, which is where these land — the same place `$http_response_header`'s
//!   compile-time deprecation is emitted from.
//! - `php_user_filter` is the only built-in class in elephc's surface with tentative returns, and
//!   it has three: `filter(): int`, `onCreate(): bool` and `onClose(): void`. MEASURED on
//!   `php -n` 8.5.6 — a subclass declaring any of them WITHOUT a return type and WITHOUT
//!   `#[\ReturnTypeWillChange]` gets one notice per method, at the METHOD's line.
//! - php prints the child's parameter list exactly as declared, defaults included, and its own
//!   canonical spelling of the parent's. Where this module cannot render the child's list with
//!   certainty — a union (php reorders the members), a variadic, or a default that is not a plain
//!   literal — it emits NOTHING rather than a message that would differ from php's. Those shapes
//!   are recorded as a known gap; php also turns some of them into a fatal, which is a different
//!   diagnostic this does not attempt.

use std::collections::HashMap;

use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Expr, ExprKind, TypeExpr};
use crate::types::traits::FlattenedClass;

/// One built-in method whose return type php declares tentatively.
struct TentativeMethod {
    /// The method name, compared case-insensitively as php does.
    name: &'static str,
    /// php's own spelling of the parent declaration, parameters and return type included.
    parent_signature: &'static str,
}

/// `php_user_filter`'s three tentative returns, in php's declaration order.
const USER_FILTER_TENTATIVES: &[TentativeMethod] = &[
    TentativeMethod {
        name: "filter",
        parent_signature: "filter($in, $out, &$consumed, bool $closing): int",
    },
    TentativeMethod {
        name: "onCreate",
        parent_signature: "onCreate(): bool",
    },
    TentativeMethod {
        name: "onClose",
        parent_signature: "onClose(): void",
    },
];

/// Returns `(line, message)` for every tentative return a user class overrides untyped.
///
/// Sorted by line, which is the order php reports them in: it links each class as the compiler
/// reaches it, and each method in declaration order within the class.
pub(crate) fn tentative_return_deprecations(
    class_map: &HashMap<String, FlattenedClass>,
) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for class in class_map.values() {
        if !descends_from_user_filter(class, class_map) {
            continue;
        }
        for method in &class.methods {
            let Some(tentative) = USER_FILTER_TENTATIVES
                .iter()
                .find(|candidate| php_symbol_key(candidate.name) == php_symbol_key(&method.name))
            else {
                continue;
            };
            if method.return_type.is_some() || has_return_type_will_change(method) {
                continue;
            }
            let Some(params) = render_parameters(method) else {
                continue;
            };
            out.push((
                method.span.line,
                format!(
                    "Deprecated: Return type of {}::{}({}) should either be compatible with \
                     php_user_filter::{}, or the #[\\ReturnTypeWillChange] attribute should be \
                     used to temporarily suppress the notice\n",
                    class.name, method.name, params, tentative.parent_signature
                ),
            ));
        }
    }
    out.sort_by_key(|(line, _)| *line);
    out
}

/// Whether a class reaches `php_user_filter` through its parent chain.
///
/// A class that merely INHERITS an untyped override gets no notice: php raises it where the method
/// is DECLARED, so only the declaring class is walked here.
fn descends_from_user_filter(
    class: &FlattenedClass,
    class_map: &HashMap<String, FlattenedClass>,
) -> bool {
    let mut current = class;
    // The chain is finite and short; the bound only stops a cycle a malformed map could carry.
    for _ in 0..64 {
        let Some(parent) = current.extends.as_deref() else {
            return false;
        };
        let parent = parent.trim_start_matches('\\');
        if php_symbol_key(parent) == php_symbol_key("php_user_filter") {
            return true;
        }
        let Some(next) = class_map
            .values()
            .find(|candidate| php_symbol_key(&candidate.name) == php_symbol_key(parent))
        else {
            return false;
        };
        current = next;
    }
    false
}

/// Whether the method carries `#[\ReturnTypeWillChange]`, php's opt-out from this notice.
fn has_return_type_will_change(method: &ClassMethod) -> bool {
    method.attributes.iter().any(|group| {
        group.attributes.iter().any(|attribute| {
            php_symbol_key(attribute.name.to_string().trim_start_matches('\\'))
                == php_symbol_key("ReturnTypeWillChange")
        })
    })
}

/// Renders a method's parameter list the way php prints it, or `None` when it cannot be certain.
///
/// php prints what was DECLARED: the type if there is one, `&` for by-reference, and ` = <value>`
/// for a default. Returning `None` is deliberate — a message that differs from php's would be
/// worse than none, and the shapes declined here are the ones php renders differently from the
/// source (a union is reordered) or refuses outright (a variadic override is a fatal).
fn render_parameters(method: &ClassMethod) -> Option<String> {
    if method.variadic.is_some() {
        return None;
    }
    let mut rendered = Vec::with_capacity(method.params.len());
    for (name, type_expr, default, by_ref) in &method.params {
        let mut piece = String::new();
        if let Some(type_expr) = type_expr {
            piece.push_str(&render_type(type_expr)?);
            piece.push(' ');
        }
        if *by_ref {
            piece.push('&');
        }
        piece.push('$');
        piece.push_str(name);
        if let Some(default) = default {
            piece.push_str(" = ");
            piece.push_str(&render_default(default)?);
        }
        rendered.push(piece);
    }
    Some(rendered.join(", "))
}

/// Renders a declared type the way php prints it in this message.
fn render_type(type_expr: &TypeExpr) -> Option<String> {
    Some(match type_expr {
        TypeExpr::Int => "int".to_string(),
        TypeExpr::Float => "float".to_string(),
        TypeExpr::Bool => "bool".to_string(),
        TypeExpr::False => "false".to_string(),
        TypeExpr::Str => "string".to_string(),
        TypeExpr::Void => "void".to_string(),
        TypeExpr::Never => "never".to_string(),
        TypeExpr::Iterable => "iterable".to_string(),
        TypeExpr::Array(_) => "array".to_string(),
        TypeExpr::Named(name) => name.to_string().trim_start_matches('\\').to_string(),
        TypeExpr::Nullable(inner) => format!("?{}", render_type(inner)?),
        // php reorders union members into its own canonical order, which this cannot reproduce
        // from the source alone: `int|string` prints as `string|int`.
        _ => return None,
    })
}

/// Renders a default value the way php prints it, for the literals it can be certain of.
fn render_default(default: &Expr) -> Option<String> {
    Some(match &default.kind {
        ExprKind::Null => "null".to_string(),
        ExprKind::BoolLiteral(true) => "true".to_string(),
        ExprKind::BoolLiteral(false) => "false".to_string(),
        ExprKind::IntLiteral(value) => value.to_string(),
        ExprKind::StringLiteral(value) => format!("'{}'", value),
        ExprKind::ArrayLiteral(items) if items.is_empty() => "[]".to_string(),
        _ => return None,
    })
}
