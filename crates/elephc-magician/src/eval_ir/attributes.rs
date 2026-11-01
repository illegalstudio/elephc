//! Purpose:
//! Defines eval attribute declarations and their argument metadata.
//!
//! Called from:
//! - Class-like/callable parsing, validation, context registration, and Reflection.
//!
//! Key details:
//! - Attribute arguments remain syntax values until explicitly materialized by the interpreter.
//! - An argument the parser cannot fold to a literal (a class constant, a concatenation) is kept
//!   as `EvalAttributeArg::Expr`; Reflection evaluates it to a literal before using it.

/// Literal attribute argument metadata retained by eval declarations.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum EvalAttributeArg {
    String(String),
    /// A PHP string attribute argument containing non-UTF-8 bytes.
    Bytes(Vec<u8>),
    Int(i64),
    Float(u64),
    Bool(bool),
    Null,
    Array(Vec<EvalAttributeArg>),
    Named {
        name: String,
        value: Box<EvalAttributeArg>,
    },
    IntKeyed {
        key: i64,
        value: Box<EvalAttributeArg>,
    },
    /// A constant expression the parser cannot fold (`Requirement::POSITIVE_INT`, a global
    /// constant, a concatenation). Reflection evaluates it when it materializes the attribute and
    /// continues with the literal it produced; nothing past that point sees this variant.
    Expr(Box<crate::eval_ir::EvalExpr>),
}

impl EvalAttributeArg {
    /// Returns the PHP named-argument key when this attribute arg is named.
    pub fn name(&self) -> Option<&str> {
        match self {
            EvalAttributeArg::Named { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Returns the PHP integer array key when this attribute arg is int-keyed.
    pub fn int_key(&self) -> Option<i64> {
        match self {
            EvalAttributeArg::IntKeyed { key, .. } => Some(*key),
            _ => None,
        }
    }

    /// Returns the scalar payload, unwrapping a named or int-keyed wrapper.
    pub fn value(&self) -> &EvalAttributeArg {
        match self {
            EvalAttributeArg::Named { value, .. } | EvalAttributeArg::IntKeyed { value, .. } => {
                value
            }
            _ => self,
        }
    }
}

/// Attribute metadata retained for eval class-like declarations.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EvalAttribute {
    name: String,
    args: Option<Vec<EvalAttributeArg>>,
}

impl EvalAttribute {
    /// Creates one eval attribute metadata entry.
    pub fn new(name: impl Into<String>, args: Option<Vec<EvalAttributeArg>>) -> Self {
        Self {
            name: name.into(),
            args,
        }
    }

    /// Returns the resolved PHP-visible attribute class name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns supported literal positional args, or `None` for unsupported metadata.
    pub fn args(&self) -> Option<&[EvalAttributeArg]> {
        self.args.as_deref()
    }
}


/// Renders one attribute exactly as PHP's `ReflectionAttribute::__toString()` does.
///
/// Symfony's `Config/Resource/ReflectionClassResource::generateSignature()` writes
/// `[$a->getName(), (string) $a]` for every attribute it tracks, and that signature decides whether
/// a config cache is fresh. With no `__toString` the cast raised `Object of class
/// ReflectionAttribute could not be converted to string` inside `isFresh()` for
/// `url_matching_routes.php` -- before routing -- so EVERY route died the moment the app grew a
/// controller carrying `#[Route]`.
///
/// The format, confirmed against `php -n` on 8.5. Both forms end with a newline:
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
pub fn render_reflection_attribute_string(name: &str, args: &[EvalAttributeArg]) -> String {
    if args.is_empty() {
        return format!("Attribute [ {} ]\n", name);
    }
    let mut out = format!(
        "Attribute [ {} ] {{\n  - Arguments [{}] {{\n",
        name,
        args.len()
    );
    for (index, arg) in args.iter().enumerate() {
        out.push_str(&format!("    Argument #{} [ ", index));
        // Only a NAMED argument prints a key. An int-keyed entry is an array-literal shape that
        // cannot reach an argument list, so it is left to the array rendering below.
        if let Some(key) = arg.name() {
            out.push_str(key);
            out.push_str(" = ");
        }
        render_attribute_arg_value(arg.value(), &mut out);
        out.push_str(" ]\n");
    }
    out.push_str("  }\n}\n");
    out
}

/// Appends one attribute-argument value in PHP's export spelling.
fn render_attribute_arg_value(value: &EvalAttributeArg, out: &mut String) {
    match value {
        EvalAttributeArg::Null => out.push_str("NULL"),
        EvalAttributeArg::Bool(true) => out.push_str("true"),
        EvalAttributeArg::Bool(false) => out.push_str("false"),
        EvalAttributeArg::Int(v) => out.push_str(&v.to_string()),
        // `{:?}` is the spelling that keeps PHP's trailing `.0` on a whole float (`1.0`, not `1`).
        EvalAttributeArg::Float(bits) => out.push_str(&format!("{:?}", f64::from_bits(*bits))),
        EvalAttributeArg::String(s) => render_attribute_arg_string(s, out),
        // Non-UTF-8 bytes print as their lossy text: the export is a diagnostic string.
        EvalAttributeArg::Bytes(bytes) => {
            render_attribute_arg_string(&String::from_utf8_lossy(bytes), out)
        }
        EvalAttributeArg::Array(elements) => {
            out.push('[');
            for (index, element) in elements.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                if let Some(key) = element.name() {
                    render_attribute_arg_string(key, out);
                    out.push_str(" => ");
                } else if let Some(key) = element.int_key() {
                    out.push_str(&key.to_string());
                    out.push_str(" => ");
                }
                render_attribute_arg_value(element.value(), out);
            }
            out.push(']');
        }
        // A wrapper only ever reaches here through `value()`, which unwraps it; matching it keeps
        // the arm total rather than relying on that.
        EvalAttributeArg::Named { value, .. } | EvalAttributeArg::IntKeyed { value, .. } => {
            render_attribute_arg_value(value, out)
        }
        // Reflection resolves expressions to literals before rendering; an unresolved one has no
        // value to print, which php would never show either.
        EvalAttributeArg::Expr(_) => out.push('?'),
    }
}

/// Appends a single-quoted string in PHP's AST-export escaping.
///
/// Confirmed against `php -n`: the backslash and the control characters are escaped, the single
/// quote is NOT (`q'uote` prints as `'q'uote'`), and neither is `$`. That is deliberately unlike
/// `var_export`, which escapes the quote instead.
fn render_attribute_arg_string(value: &str, out: &mut String) {
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
mod reflection_attribute_string_tests {
    use super::*;

    /// An attribute with no arguments is one line with no block, as `php -n` prints it.
    #[test]
    fn attribute_without_arguments_renders_one_line() {
        assert_eq!(
            render_reflection_attribute_string("R", &[]),
            "Attribute [ R ]\n"
        );
    }

    /// Every scalar spelling matches `php -n`, including `NULL` in caps and a whole float's `.0`.
    #[test]
    fn attribute_scalar_arguments_match_php_spelling() {
        let args = vec![
            EvalAttributeArg::String("s".to_string()),
            EvalAttributeArg::Int(3),
            EvalAttributeArg::Float(1.5f64.to_bits()),
            EvalAttributeArg::Float(1.0f64.to_bits()),
            EvalAttributeArg::Bool(true),
            EvalAttributeArg::Bool(false),
            EvalAttributeArg::Null,
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

    /// A named argument prints `name = value`, and arrays print inline with `=>` for keyed entries.
    #[test]
    fn attribute_named_and_array_arguments_match_php_spelling() {
        let args = vec![
            EvalAttributeArg::Named {
                name: "name".to_string(),
                value: Box::new(EvalAttributeArg::String("n".to_string())),
            },
            EvalAttributeArg::Array(vec![
                EvalAttributeArg::String("a".to_string()),
                EvalAttributeArg::String("b".to_string()),
            ]),
            EvalAttributeArg::Array(vec![EvalAttributeArg::Named {
                name: "k".to_string(),
                value: Box::new(EvalAttributeArg::Int(1)),
            }]),
            EvalAttributeArg::Array(Vec::new()),
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
    #[test]
    fn attribute_string_argument_escapes_only_what_php_escapes() {
        let args = vec![
            EvalAttributeArg::String("q'uote".to_string()),
            EvalAttributeArg::String("back\\slash".to_string()),
            EvalAttributeArg::String("nl\nhere".to_string()),
            EvalAttributeArg::String("dollar$x".to_string()),
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
