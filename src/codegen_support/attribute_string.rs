//! Purpose:
//! Renders `ReflectionAttribute::__toString()` the way PHP's own AST export does.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::attributes` when it materializes a reflection
//!   attribute object at compile time, to fill the precomputed `__string` slot.
//!
//! Key details:
//! - An attribute's arguments are literals fixed at their declaration site, so the whole rendering
//!   is a compile-time constant: nothing is formatted at run time.
//! - The eval bridge materializes the same objects at RUN time and carries its own copy of this
//!   format in `elephc_magician::eval_ir::attributes`; the two crates are independent (magician is
//!   only a dev-dependency here), so the format is duplicated and both copies are pinned by tests
//!   asserting the same `php -n` golden strings.

use crate::types::{AttrArgEntry, AttrArgValue, AttrKey};

/// Renders one attribute exactly as PHP's `ReflectionAttribute::__toString()` does.
///
/// Symfony's `ReflectionClassResource::generateSignature()` writes `[$a->getName(), (string) $a]`
/// for every attribute it tracks, and that signature decides whether a config cache is fresh. With
/// no `__toString` the cast raised `Object of class ReflectionAttribute could not be converted to
/// string`, which killed EVERY request the moment the app grew an attribute-annotated controller.
///
/// The format, confirmed against `php -n` on 8.5:
///
/// ```text
/// Attribute [ R ]                       // no arguments: one line, no block
/// Attribute [ R ] {                     // with arguments
///   - Arguments [2] {
///     Argument #0 [ 'positional' ]
///     Argument #1 [ named = 3 ]
///   }
/// }
/// ```
///
/// Both forms end with a newline.
pub(crate) fn render_reflection_attribute_string(name: &str, args: &[AttrArgEntry]) -> String {
    if args.is_empty() {
        return format!("Attribute [ {} ]\n", name);
    }
    let mut out = format!(
        "Attribute [ {} ] {{\n  - Arguments [{}] {{\n",
        name,
        args.len()
    );
    for (index, entry) in args.iter().enumerate() {
        out.push_str(&format!("    Argument #{} [ ", index));
        // A NAMED argument prints its name; an explicit integer key is an array-literal shape that
        // cannot reach an argument list, so only the string key is a name here.
        if let Some(AttrKey::Str(key)) = &entry.key {
            out.push_str(key);
            out.push_str(" = ");
        }
        render_value(&entry.value, &mut out);
        out.push_str(" ]\n");
    }
    out.push_str("  }\n}\n");
    out
}

/// Appends one attribute-argument value in PHP's export spelling.
///
/// Known divergence: PHP folds a CLASS constant to its value (`#[A(K::C)]` prints `'kc'`) while
/// leaving an enum case and a userland global constant as written (`E::A`, `G`). elephc carries
/// both class constants and enum cases as [`AttrArgValue::ScopedConst`] with no value attached, so
/// both print in the written form. The rendering is only ever hashed, never parsed back.
fn render_value(value: &AttrArgValue, out: &mut String) {
    match value {
        AttrArgValue::Null => out.push_str("NULL"),
        AttrArgValue::Bool(true) => out.push_str("true"),
        AttrArgValue::Bool(false) => out.push_str("false"),
        AttrArgValue::Int(v) => out.push_str(&v.to_string()),
        // `{:?}` is the spelling that keeps PHP's trailing `.0` on a whole float (`1.0`, not `1`).
        AttrArgValue::Float(bits) => out.push_str(&format!("{:?}", f64::from_bits(*bits))),
        AttrArgValue::Str(s) => render_string(s, out),
        AttrArgValue::ConstRef(name) => out.push_str(name),
        AttrArgValue::ScopedConst(owner, member) => {
            out.push_str(owner);
            out.push_str("::");
            out.push_str(member);
        }
        AttrArgValue::Array(entries) => {
            out.push('[');
            for (index, entry) in entries.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                match &entry.key {
                    Some(AttrKey::Str(key)) => {
                        render_string(key, out);
                        out.push_str(" => ");
                    }
                    Some(AttrKey::Int(key)) => {
                        out.push_str(&key.to_string());
                        out.push_str(" => ");
                    }
                    None => {}
                }
                render_value(&entry.value, out);
            }
            out.push(']');
        }
    }
}

/// Appends a single-quoted string in PHP's AST-export escaping.
///
/// Confirmed against `php -n`: the backslash and the control characters are escaped, the single
/// quote is NOT (`q'uote` prints as `'q'uote'`), and neither is `$`. That is deliberately unlike
/// `var_export`, which escapes the quote and leaves the backslash pairs alone.
fn render_string(value: &str, out: &mut String) {
    out.push('\'');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            '\x0c' => out.push_str("\\f"),
            '\x1b' => out.push_str("\\e"),
            other => out.push(other),
        }
    }
    out.push('\'');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a positional argument entry.
    fn positional(value: AttrArgValue) -> AttrArgEntry {
        AttrArgEntry { key: None, value }
    }

    /// An attribute with no arguments is one line with no block, as `php -n` prints it.
    #[test]
    fn test_attribute_without_arguments_renders_one_line() {
        assert_eq!(
            render_reflection_attribute_string("R", &[]),
            "Attribute [ R ]\n"
        );
    }

    /// Every scalar spelling matches `php -n`, including `NULL` in caps and a whole float's `.0`.
    #[test]
    fn test_attribute_scalar_arguments_match_php_spelling() {
        let args = vec![
            positional(AttrArgValue::Str("s".to_string())),
            positional(AttrArgValue::Int(3)),
            positional(AttrArgValue::Float(1.5f64.to_bits())),
            positional(AttrArgValue::Float(1.0f64.to_bits())),
            positional(AttrArgValue::Bool(true)),
            positional(AttrArgValue::Bool(false)),
            positional(AttrArgValue::Null),
        ];
        assert_eq!(
            render_reflection_attribute_string("R", &args),
            concat!(
                "Attribute [ R ] {\n",
                "  - Arguments [7] {\n",
                "    Argument #0 [ 's' ]\n",
                "    Argument #1 [ 3 ]\n",
                "    Argument #2 [ 1.5 ]\n",
                "    Argument #3 [ 1.0 ]\n",
                "    Argument #4 [ true ]\n",
                "    Argument #5 [ false ]\n",
                "    Argument #6 [ NULL ]\n",
                "  }\n}\n",
            )
        );
    }

    /// A named argument prints `name = value`, and arrays print inline with `=>` for string keys.
    #[test]
    fn test_attribute_named_and_array_arguments_match_php_spelling() {
        let args = vec![
            AttrArgEntry {
                key: Some(AttrKey::Str("name".to_string())),
                value: AttrArgValue::Str("n".to_string()),
            },
            positional(AttrArgValue::Array(vec![
                positional(AttrArgValue::Str("a".to_string())),
                positional(AttrArgValue::Str("b".to_string())),
            ])),
            positional(AttrArgValue::Array(vec![AttrArgEntry {
                key: Some(AttrKey::Str("k".to_string())),
                value: AttrArgValue::Int(1),
            }])),
            positional(AttrArgValue::Array(Vec::new())),
        ];
        assert_eq!(
            render_reflection_attribute_string("R", &args),
            concat!(
                "Attribute [ R ] {\n",
                "  - Arguments [4] {\n",
                "    Argument #0 [ name = 'n' ]\n",
                "    Argument #1 [ ['a', 'b'] ]\n",
                "    Argument #2 [ ['k' => 1] ]\n",
                "    Argument #3 [ [] ]\n",
                "  }\n}\n",
            )
        );
    }

    /// The backslash and control characters are escaped; the single quote and `$` are not.
    ///
    /// `php -n` prints `back\slash` as `'back\\slash'` and `q'uote` as `'q'uote'`.
    #[test]
    fn test_attribute_string_argument_escapes_only_what_php_escapes() {
        let args = vec![
            positional(AttrArgValue::Str("q'uote".to_string())),
            positional(AttrArgValue::Str("back\\slash".to_string())),
            positional(AttrArgValue::Str("nl\nhere".to_string())),
            positional(AttrArgValue::Str("dollar$x".to_string())),
        ];
        assert_eq!(
            render_reflection_attribute_string("R", &args),
            concat!(
                "Attribute [ R ] {\n",
                "  - Arguments [4] {\n",
                "    Argument #0 [ 'q'uote' ]\n",
                "    Argument #1 [ 'back\\\\slash' ]\n",
                "    Argument #2 [ 'nl\\nhere' ]\n",
                "    Argument #3 [ 'dollar$x' ]\n",
                "  }\n}\n",
            )
        );
    }
}
