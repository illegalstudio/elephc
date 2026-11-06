//! Purpose:
//! Spells PHP type names consistently in parameter and property diagnostics.
//!
//! Called from:
//! - Selected enum argument guards and native typed-property write guards.
//!
//! Key details:
//! - Class names precede PHP's fixed builtin union order.
//! - Nullable single-member unions use ?T; physical array shapes share one PHP name.

use super::PhpType;

/// Spells a declared PHP type the way PHP spells it in a `TypeError`.
pub(crate) fn php_type_name(php_type: &PhpType) -> String {
    match php_type {
        PhpType::Int => "int".to_string(),
        PhpType::Float => "float".to_string(),
        PhpType::Str => "string".to_string(),
        PhpType::Bool => "bool".to_string(),
        PhpType::False => "false".to_string(),
        PhpType::Void | PhpType::Never => "null".to_string(),
        PhpType::Mixed => "mixed".to_string(),
        PhpType::Callable => "callable".to_string(),
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array".to_string(),
        PhpType::Iterable => "Traversable|array".to_string(),
        PhpType::Object(class_name) if class_name.is_empty() => "object".to_string(),
        PhpType::Object(class_name) => class_name.trim_start_matches('\\').to_string(),
        PhpType::Union(members) => php_union_type_name(members),
        other => format!("{:?}", other),
    }
}

/// The order php-src prints built-in union members in, after the class names.
///
/// `zend_type_to_string` walks a fixed type mask, so a union is NEVER printed in declaration
/// order: `int|string` prints as `string|int` and `null|int|string` as `string|int|null`. A
/// message built from declaration order disagrees with reference PHP for most unions, which is
/// why the spelling is normalized here instead of being joined as written.
const PHP_BUILTIN_TYPE_ORDER: [&str; 8] = [
    "object", "array", "string", "int", "float", "bool", "false", "true",
];

/// Spells a union the way PHP does, collapsing a nullable single type to `?T`.
pub(crate) fn php_union_type_name(members: &[PhpType]) -> String {
    let is_null = |member: &PhpType| matches!(member, PhpType::Void | PhpType::Never);
    let has_null = members.iter().any(is_null);
    let mut classes: Vec<String> = Vec::new();
    let mut builtins: Vec<String> = Vec::new();
    for member in members.iter().filter(|member| !is_null(member)) {
        for name in member_type_names(member) {
            let bucket = if PHP_BUILTIN_TYPE_ORDER.contains(&name.as_str()) {
                &mut builtins
            } else {
                &mut classes
            };
            if !bucket.contains(&name) {
                bucket.push(name);
            }
        }
    }
    builtins.sort_by_key(|name| {
        PHP_BUILTIN_TYPE_ORDER
            .iter()
            .position(|known| known == name)
            .unwrap_or(PHP_BUILTIN_TYPE_ORDER.len())
    });
    let mut named = classes;
    named.extend(builtins);
    if named.len() == 1 && has_null {
        return format!("?{}", named[0]);
    }
    if has_null {
        named.push("null".to_string());
    }
    named.join("|")
}

/// The individual names one declared union member contributes to the printed spelling.
///
/// `array` reaches codegen as the two-member union of its indexed and hash representations, so
/// both collapse to one `array`; `iterable` is stored as one member but PHP prints it as the
/// two types it stands for.
fn member_type_names(member: &PhpType) -> Vec<String> {
    match member {
        PhpType::Iterable => vec!["Traversable".to_string(), "array".to_string()],
        PhpType::Union(inner) => inner.iter().flat_map(member_type_names).collect(),
        other => vec![php_type_name(other)],
    }
}
