//! Purpose:
//! Identifies names reserved by the runtime PHP profile.
//!
//! Called from:
//! - The scanner adapter and AST-to-EvalIR conversion.
//!
//! Key details:
//! - Class-like names reserve type atoms in addition to PHP keywords.

/// Compares a source identifier to a PHP keyword using ASCII case-insensitive rules.
pub(super) fn ident_eq(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

/// Returns true when PHP forbids a name for class, interface, trait, or enum declarations.
pub(crate) fn is_reserved_class_like_name(name: &str) -> bool {
    [
        "__halt_compiler",
        "abstract",
        "and",
        "array",
        "as",
        "break",
        "callable",
        "case",
        "catch",
        "class",
        "clone",
        "const",
        "continue",
        "declare",
        "default",
        "die",
        "do",
        "echo",
        "else",
        "elseif",
        "empty",
        "enddeclare",
        "endfor",
        "endforeach",
        "endif",
        "endswitch",
        "endwhile",
        "eval",
        "exit",
        "extends",
        "false",
        "final",
        "finally",
        "fn",
        "for",
        "foreach",
        "function",
        "global",
        "goto",
        "if",
        "implements",
        "include",
        "include_once",
        "instanceof",
        "insteadof",
        "interface",
        "isset",
        "list",
        "match",
        "namespace",
        "new",
        "null",
        "or",
        "print",
        "private",
        "protected",
        "public",
        "readonly",
        "require",
        "require_once",
        "return",
        "static",
        "switch",
        "throw",
        "trait",
        "true",
        "try",
        "unset",
        "use",
        "var",
        "while",
        "xor",
        "yield",
        "bool",
        "int",
        "float",
        "string",
        "object",
        "mixed",
        "never",
        "void",
        "iterable",
        "self",
        "parent",
    ]
    .iter()
    .any(|keyword| ident_eq(name, keyword))
}

/// Returns true when PHP's grammar keeps a word as a keyword, so it cannot name a jump label.
///
/// Narrower than `is_reserved_class_like_name`: the scalar type names and `true`, `false`,
/// `null`, `self` and `parent` are refused only as class-like names. To the grammar they are
/// ordinary identifiers, and `goto int; int:` is valid PHP.
pub(crate) fn is_reserved_keyword(name: &str) -> bool {
    is_reserved_class_like_name(name)
        && ![
            "bool", "int", "float", "string", "object", "mixed", "never", "void", "iterable",
            "self", "parent", "true", "false", "null",
        ]
        .iter()
        .any(|word| ident_eq(name, word))
}

/// Returns the first namespace segment and the optional remaining suffix.
pub(super) fn split_first_name_segment(name: &str) -> (&str, Option<&str>) {
    name.split_once('\\')
        .map_or((name, None), |(first, tail)| (first, Some(tail)))
}
