//! Purpose:
//! Computes the debugger's read-only structural projection of a user
//! `__debugInfo()` body without executing it.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_debug_class_markers` while emitting the
//!   `@class` and `@debuginfo` debug metadata markers.
//!
//! Key details:
//! - Only a single `return [...]` of `$this->property` reads, scalar literals,
//!   and literal-only nested arrays is accepted. Any other shape returns `None`
//!   and the class keeps the `runtime` marker plus its declared-property view.
//! - The projection is bounded (array depth 3, 16 children per array, 256 string
//!   bytes) so a large literal cannot bloat the `.edbg.json` sidecar.
//! - Rendered text mirrors the debugger's PHP-style scalar and array formatting;
//!   strings are escaped exactly once here because the value is display-only.

use std::collections::HashMap;

use crate::parser::ast::{ClassMethod, Expr, ExprKind, StmtKind};
use crate::types::ClassInfo;

/// Maximum array children kept in one literal projection array.
const MAX_ARRAY_CHILDREN: usize = 16;
/// Maximum nested literal-array depth kept in one projection.
const MAX_ARRAY_DEPTH: usize = 3;
/// Maximum string bytes kept in one projected literal string.
const MAX_STRING_BYTES: usize = 256;

/// One projected `__debugInfo()` entry.
pub(crate) struct DebugProjectionEntry {
    /// Array key exposed by `__debugInfo()`.
    pub(crate) key: String,
    /// Property read or static literal displayed under that key.
    pub(crate) value: DebugProjectionValue,
}

/// One safely projected `__debugInfo()` value.
pub(crate) enum DebugProjectionValue {
    /// Read the named declared property from the object's live slot.
    Property(String),
    /// Display a compile-time literal without reading or running anything.
    Literal {
        /// PHP-style display text (`int(3)`, `array(2) {…}`, …).
        text: String,
        /// PHP type name for DAP's `type` field.
        php_type: String,
    },
}

/// Returns the safe debugger projection of a class's `__debugInfo()` body.
///
/// A `None` result means the body is not statically safe to project; the caller
/// emits the `runtime` marker instead and the debugger never invokes the method.
pub(crate) fn debug_projection(
    class_info: &ClassInfo,
    classes: &HashMap<String, ClassInfo>,
) -> Option<Vec<DebugProjectionEntry>> {
    let method = debug_info_method(class_info, classes)?;
    debug_projection_for_method(method, class_info)
}

/// Resolves the `__debugInfo()` declaration visible on `class_info`, including an
/// inherited one, by following the implementation-owner table.
pub(crate) fn debug_info_method<'a>(
    class_info: &'a ClassInfo,
    classes: &'a HashMap<String, ClassInfo>,
) -> Option<&'a ClassMethod> {
    let key = crate::names::php_symbol_key("__debugInfo");
    let owner = class_info
        .method_impl_classes
        .get(&key)
        .and_then(|owner| classes.get(owner))
        .unwrap_or(class_info);
    owner
        .method_decls
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case("__debugInfo"))
}

/// Projects one resolved `__debugInfo()` declaration against the object's layout.
pub(crate) fn debug_projection_for_method(
    method: &ClassMethod,
    class_info: &ClassInfo,
) -> Option<Vec<DebugProjectionEntry>> {
    if method.is_static || !method.has_body {
        return None;
    }
    let [only_stmt] = method.body.as_slice() else {
        return None;
    };
    let StmtKind::Return(Some(returned)) = &only_stmt.kind else {
        return None;
    };
    let pairs = match &returned.kind {
        ExprKind::ArrayLiteralAssoc(pairs) => pairs,
        // `return [];` is an exact zero-entry projection.
        ExprKind::ArrayLiteral(items) if items.is_empty() => return Some(Vec::new()),
        _ => return None,
    };
    let mut entries = Vec::with_capacity(pairs.len());
    let mut keys = std::collections::HashSet::new();
    for (key_expr, value_expr) in pairs {
        let key = projected_key(key_expr)?;
        if !keys.insert(key.clone()) {
            // PHP would apply last-wins; refusing keeps the projection exact
            // instead of guessing which occurrence a reader expects.
            return None;
        }
        let value = projected_entry_value(value_expr, class_info)?;
        entries.push(DebugProjectionEntry { key, value });
    }
    Some(entries)
}

/// Returns the string key of one projected entry.
fn projected_key(expr: &Expr) -> Option<String> {
    let key = match &expr.kind {
        ExprKind::StringLiteral(key) => key.clone(),
        ExprKind::IntLiteral(key) => key.to_string(),
        _ => return None,
    };
    // NUL is php-src's visibility-mangling marker; the debugger's projection does
    // not reproduce that spelling.
    (!key.contains('\0')).then_some(key)
}

/// Projects one top-level entry value, allowing a live property read.
fn projected_entry_value(expr: &Expr, class_info: &ClassInfo) -> Option<DebugProjectionValue> {
    if let Some(property) = this_property(expr) {
        return class_info
            .properties
            .iter()
            .any(|(name, _)| name == property)
            .then(|| DebugProjectionValue::Property(property.to_string()));
    }
    let literal = literal_value(expr, 0)?;
    Some(DebugProjectionValue::Literal {
        text: literal.text,
        php_type: literal.php_type,
    })
}

/// One rendered literal value.
struct RenderedLiteral {
    /// PHP-style display text.
    text: String,
    /// PHP type name.
    php_type: String,
}

/// Renders one literal value, recursing only through literal-only arrays.
fn literal_value(expr: &Expr, depth: usize) -> Option<RenderedLiteral> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(RenderedLiteral {
            text: format!("int({value})"),
            php_type: "int".to_string(),
        }),
        ExprKind::FloatLiteral(value) => Some(RenderedLiteral {
            text: format!("float({value})"),
            php_type: "float".to_string(),
        }),
        ExprKind::BoolLiteral(value) => Some(RenderedLiteral {
            text: format!("bool({})", if *value { "true" } else { "false" }),
            php_type: "bool".to_string(),
        }),
        ExprKind::Null => Some(RenderedLiteral {
            text: "null".to_string(),
            php_type: "null".to_string(),
        }),
        ExprKind::StringLiteral(value) => {
            if value.len() > MAX_STRING_BYTES {
                return None;
            }
            Some(RenderedLiteral {
                text: format!(
                    "string({}) \"{}\"",
                    value.len(),
                    escape_php_string(value)
                ),
                php_type: "string".to_string(),
            })
        }
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg().map(|value| RenderedLiteral {
                text: format!("int({value})"),
                php_type: "int".to_string(),
            }),
            ExprKind::FloatLiteral(value) => Some(RenderedLiteral {
                text: format!("float({})", -*value),
                php_type: "float".to_string(),
            }),
            _ => None,
        },
        ExprKind::ArrayLiteral(items) => {
            let entries: Vec<(String, &Expr)> = items
                .iter()
                .enumerate()
                .map(|(index, item)| (index.to_string(), item))
                .collect();
            render_array(entries, depth)
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let mut entries = Vec::with_capacity(pairs.len());
            let mut keys = std::collections::HashSet::new();
            for (key_expr, value_expr) in pairs {
                let key = projected_key(key_expr)?;
                if !keys.insert(key.clone()) {
                    return None;
                }
                entries.push((key, value_expr));
            }
            render_array(entries, depth)
        }
        _ => None,
    }
}

/// Renders one literal-only array, bounded by depth and child count.
fn render_array(entries: Vec<(String, &Expr)>, depth: usize) -> Option<RenderedLiteral> {
    if depth >= MAX_ARRAY_DEPTH || entries.len() > MAX_ARRAY_CHILDREN {
        return None;
    }
    if entries.is_empty() {
        return Some(RenderedLiteral {
            text: "array(0)".to_string(),
            php_type: "array".to_string(),
        });
    }
    let mut text = format!("array({}) {{", entries.len());
    for (index, (key, value_expr)) in entries.into_iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        // PHP normalizes canonical decimal strings to integer array keys;
        // everything else (leading zeros, "+1", non-numeric text) stays a string.
        if key.parse::<i64>().is_ok_and(|value| value.to_string() == key) {
            text.push_str(&format!("[{key}] => "));
        } else {
            text.push_str(&format!("[\"{}\"] => ", escape_php_string(&key)));
        }
        let value = literal_value(value_expr, depth + 1)?;
        text.push_str(&value.text);
    }
    text.push('}');
    Some(RenderedLiteral {
        text,
        php_type: "array".to_string(),
    })
}

/// Returns the property name when `expr` is a direct `$this->property` read.
fn this_property(expr: &Expr) -> Option<&str> {
    let ExprKind::PropertyAccess { object, property } = &expr.kind else {
        return None;
    };
    match &object.kind {
        ExprKind::This => Some(property),
        ExprKind::Variable(receiver) if receiver == "this" => Some(property),
        _ => None,
    }
}

/// Escapes one string the way the debugger's PHP-style value formatter does.
fn escape_php_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            ch if ch.is_control() => out.push_str(&format!("\\x{:02x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one checked class map from a source program, for projection tests.
    fn checked_classes(source: &str) -> std::collections::HashMap<String, crate::types::ClassInfo> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize projection fixture");
        let program = crate::parser::parse(&tokens).expect("parse projection fixture");
        crate::types::check(&program)
            .expect("check projection fixture")
            .classes
    }

    /// Returns the `Probe` class and the full class map.
    fn probe(
        source: &str,
    ) -> (
        crate::types::ClassInfo,
        std::collections::HashMap<String, crate::types::ClassInfo>,
    ) {
        let classes = checked_classes(source);
        let class = classes.get("Probe").cloned().expect("Probe class");
        (class, classes)
    }

    /// Verifies property reads, scalar literals and nested literal arrays project,
    /// while computed values stay on the runtime fallback.
    #[test]
    fn projects_only_safe_debug_info_values() {
        let (safe, classes) = probe(
            "<?php class Probe { public int $id = 1; public string $name = 'x'; \
             public function __debugInfo(): array { return [\
             'id' => $this->id, 'name' => $this->name, \
             'version' => '1.0', 'flags' => [1, 2, 3], \
             'meta' => ['stable' => true, 'label' => 'ok'], \
             ]; } }",
        );
        let projection = debug_projection(&safe, &classes).expect("safe projection");
        assert_eq!(projection.len(), 5);
        assert!(matches!(
            &projection[0].value,
            DebugProjectionValue::Property(ref property) if property == "id"
        ));
        assert!(matches!(
            &projection[2].value,
            DebugProjectionValue::Literal { ref text, .. } if text == "string(3) \"1.0\""
        ));
        assert!(matches!(
            &projection[3].value,
            DebugProjectionValue::Literal { ref text, .. }
                if text == "array(3) {[0] => int(1), [1] => int(2), [2] => int(3)}"
        ));
        assert!(matches!(
            &projection[4].value,
            DebugProjectionValue::Literal { ref text, .. }
                if text == "array(2) {[\"stable\"] => bool(true), [\"label\"] => string(2) \"ok\"}"
        ));

        let (computed, computed_classes) = probe(
            "<?php class Probe { public int $id = 1; \
             public function __debugInfo(): array { return ['n' => $this->id + 1]; } }",
        );
        assert!(debug_projection(&computed, &computed_classes).is_none());
    }

    /// Verifies an inherited `__debugInfo()` keeps its projection on the child
    /// class and still resolves property reads against the child layout.
    #[test]
    fn projects_inherited_debug_info_on_child_layout() {
        let classes = checked_classes(
            "<?php class ParentInfo { public int $id = 1; \
             public function __debugInfo(): array { return ['id' => $this->id, 'v' => 'p']; } } \
             class ChildInfo extends ParentInfo { public int $x = 2; }",
        );
        let child = classes.get("ChildInfo").expect("ChildInfo class");
        let projection = debug_projection(child, &classes).expect("inherited projection");
        assert_eq!(projection.len(), 2);
        assert!(matches!(
            &projection[0].value,
            DebugProjectionValue::Property(property) if property == "id"
        ));
        assert!(matches!(
            &projection[1].value,
            DebugProjectionValue::Literal { text, .. } if text == "string(1) \"p\""
        ));
    }

    /// Verifies oversized literal arrays and strings are refused instead of
    /// bloating the debug sidecar or silently displaying a truncated array.
    #[test]
    fn refuses_unbounded_literal_projections() {
        let (wide, wide_classes) = probe(
            "<?php class Probe { public function __debugInfo(): array { return [\
             'x' => [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17]]; } }",
        );
        assert!(debug_projection(&wide, &wide_classes).is_none());
        let (deep, deep_classes) = probe(
            "<?php class Probe { public function __debugInfo(): array { return [\
             'x' => [[[[1]]]]]; } }",
        );
        assert!(debug_projection(&deep, &deep_classes).is_none());
        let long = format!(
            "<?php class Probe {{ public function __debugInfo(): array {{ return [\
             'x' => '{}']; }} }}",
            "x".repeat(MAX_STRING_BYTES + 1)
        );
        let (long_string, long_classes) = probe(&long);
        assert!(debug_projection(&long_string, &long_classes).is_none());
    }
}
