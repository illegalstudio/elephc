//! Purpose:
//! Models PHP's `$GLOBALS` superglobal as a compile-time alias onto ordinary
//! global storage. A literal-key access `$GLOBALS['name']` is rewritten by the
//! parser into a single variable whose name is the *alias* produced by
//! [`alias_name`]; every later phase then treats it as an ordinary variable that
//! happens to live in `_eir_global_name` storage, exactly like a variable
//! imported with `global $name;`.
//!
//! Called from:
//! - `crate::parser::expr::prefix::parse_variable` (rejects a bare `$GLOBALS`),
//! - `crate::parser::expr::pratt` (rewrites `$GLOBALS['name']` to an alias),
//! - `crate::ast_usage` (collects the aliased global names of a program),
//! - `crate::ir_lower::context::LoweringContext` (routes an alias to global
//!   storage and interns the *target* name as the global symbol),
//! - `crate::ir_lower::function::collect_global_var_names` (keeps a top-level
//!   `$name` and a function's `$GLOBALS['name']` on the SAME storage),
//! - `crate::types::checker` (types an alias without an `Undefined variable`).
//!
//! Key details:
//! - The alias spelling `GLOBALS[name]` cannot collide with a real PHP variable:
//!   `[` is not valid in a PHP identifier, so no user program can declare it.
//!   That is what makes this sound where a naive `$GLOBALS['x']` -> `$x`
//!   desugaring is not — PHP keeps a function's local `$x` and `$GLOBALS['x']`
//!   strictly separate, and an injected `global $x;` would fuse them.
//! - Only keys that are valid PHP variable names are aliased. A READ of the whole
//!   array (`array_keys($GLOBALS)`) or through a computed key (`$GLOBALS[$name]`)
//!   becomes a call of [`SNAPSHOT_FUNCTION`], the closed world's set globals as an
//!   array (`crate::globals_snapshot_prelude`). Every write shape other than a
//!   literal key (computed key, whole-array assignment) is rejected LOUDLY at
//!   parse time rather than silently miscompiled.

/// Prefix of the internal alias spelling for a `$GLOBALS['name']` access.
const ALIAS_PREFIX: &str = "GLOBALS[";

/// Suffix of the internal alias spelling for a `$GLOBALS['name']` access.
const ALIAS_SUFFIX: &str = "]";

/// PHP's fatal for replacing the whole array (`$GLOBALS = [...]`), verbatim.
///
/// PHP 8.1+ raises this at runtime; an AOT compiler can only reject the program,
/// so the wording is reproduced exactly to keep the diagnostic recognizable.
pub const WHOLE_ARRAY_ASSIGN_MESSAGE: &str =
    "$GLOBALS can only be modified using the $GLOBALS[$name] = $value syntax";

/// Picks the refusal for a `$GLOBALS` occurrence from the token that follows it,
/// or returns `None` for the one supported shape (`$GLOBALS[`).
///
/// `$GLOBALS` reaches the parser down two independent routes — as an expression
/// operand and as the head of an assignment statement — and BOTH must refuse the
/// unsupported shapes. Keeping the choice here rather than duplicating it at each
/// site is deliberate: the statement route was originally missed, so
/// `$GLOBALS = [...]` compiled and ran on past the point where PHP raises a fatal,
/// which is exactly the silent divergence this module exists to prevent.
pub const fn unsupported_use_message(
    followed_by_bracket: bool,
    followed_by_assign: bool,
) -> Option<&'static str> {
    if followed_by_bracket {
        return None;
    }
    if followed_by_assign {
        return Some(WHOLE_ARRAY_ASSIGN_MESSAGE);
    }
    Some(BARE_USE_MESSAGE)
}

/// Diagnostic for a `$GLOBALS` use that is not a literal-key element access.
pub const BARE_USE_MESSAGE: &str = concat!(
    "Unsupported use of $GLOBALS: only literal-key element access ",
    "($GLOBALS['name']) is supported, not $GLOBALS as a whole array"
);

/// Diagnostic for `$GLOBALS[$expr]` with a key that is not a literal string.
pub const DYNAMIC_KEY_MESSAGE: &str = concat!(
    "Unsupported use of $GLOBALS: the key must be a literal string ",
    "($GLOBALS['name']), a computed key is not supported"
);

/// The function a whole-array READ of `$GLOBALS` calls (`array_keys($GLOBALS)`,
/// `$GLOBALS[$name]`): it answers the closed world's globals that are set, as an array
/// (`globals_snapshot_prelude`). A global holding null reads as absent.
pub const SNAPSHOT_FUNCTION: &str = "__elephc_globals_array";

/// `__elephc_globals_array()`.
pub fn snapshot_call(span: crate::span::Span) -> crate::parser::ast::Expr {
    crate::parser::ast::Expr::new(
        crate::parser::ast::ExprKind::FunctionCall {
            name: crate::names::Name::from_parts(
                crate::names::NameKind::FullyQualified,
                vec![SNAPSHOT_FUNCTION.to_string()],
            ),
            args: Vec::new(),
        },
        span,
    )
}

/// Whether the token after `$GLOBALS` (or `$GLOBALS[...]`) makes it a write target.
pub fn is_write_token(token: Option<&crate::lexer::Token>) -> bool {
    use crate::lexer::Token;
    matches!(
        token,
        Some(
            Token::Assign
                | Token::PlusAssign
                | Token::MinusAssign
                | Token::StarAssign
                | Token::StarStarAssign
                | Token::SlashAssign
                | Token::DotAssign
                | Token::PercentAssign
                | Token::AmpAssign
                | Token::PipeAssign
                | Token::CaretAssign
                | Token::LessLessAssign
                | Token::GreaterGreaterAssign
                | Token::QuestionQuestionAssign
                | Token::PlusPlus
                | Token::MinusMinus
        )
    )
}

/// Returns true when `key` is a name a PHP variable can actually have.
///
/// PHP's grammar for a variable name is `[a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*`.
/// A `$GLOBALS` key outside that set (`$GLOBALS['a b']`) names a global that no
/// `$name` syntax can reach, so it is refused rather than aliased.
pub fn is_php_variable_name(key: &str) -> bool {
    let mut bytes = key.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_alphabetic() || b == b'_' || b >= 0x80 => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80)
}

/// Builds the internal alias variable name for `$GLOBALS[key]`.
///
/// The caller must have checked [`is_php_variable_name`]; the spelling is only
/// unambiguous for keys that contain no `]`.
pub fn alias_name(key: &str) -> String {
    format!("{ALIAS_PREFIX}{key}{ALIAS_SUFFIX}")
}

/// Returns the aliased global name when `name` is a `$GLOBALS['…']` alias.
///
/// This is the inverse of [`alias_name`] and the single predicate every later
/// phase uses to recognize one: the type checker to type it, IR lowering to give
/// it global storage, and `ast_usage` to collect it.
pub fn alias_target(name: &str) -> Option<&str> {
    name.strip_prefix(ALIAS_PREFIX)?.strip_suffix(ALIAS_SUFFIX)
}

/// Returns true when `name` is a `$GLOBALS['…']` alias.
pub fn is_alias(name: &str) -> bool {
    alias_target(name).is_some()
}

/// Renders an alias back to its PHP spelling for diagnostics.
///
/// A raw alias (`GLOBALS[app]`) would leak an internal spelling into user-facing
/// errors, so any diagnostic naming a variable prints `$GLOBALS['app']` instead.
pub fn display_name(name: &str) -> String {
    match alias_target(name) {
        Some(target) => format!("$GLOBALS['{target}']"),
        None => format!("${name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an encoded global alias resolves back to its target and is recognized as an alias.
    #[test]
    fn alias_round_trips_through_target() {
        let alias = alias_name("app");
        assert_eq!(alias, "GLOBALS[app]");
        assert_eq!(alias_target(&alias), Some("app"));
        assert!(is_alias(&alias));
    }

    /// Verifies ordinary local and superglobal names are never mistaken for encoded aliases.
    #[test]
    fn ordinary_variable_names_are_not_aliases() {
        assert_eq!(alias_target("app"), None);
        assert_eq!(alias_target("GLOBALS"), None);
        assert!(!is_alias("_SERVER"));
    }

    /// Verifies the internal alias spelling cannot collide with a legal PHP variable name.
    #[test]
    fn alias_spelling_is_unreachable_for_php_source() {
        // `[` cannot occur in a PHP variable name, so no user program can
        // declare a local that collides with an alias.
        assert!(!is_php_variable_name("GLOBALS[app]"));
    }

    /// Verifies PHP variable-name validation accepts identifiers and rejects invalid spellings.
    #[test]
    fn php_variable_name_matches_php_grammar() {
        assert!(is_php_variable_name("app"));
        assert!(is_php_variable_name("_x9"));
        assert!(is_php_variable_name("__autoload_files"));
        assert!(!is_php_variable_name(""));
        assert!(!is_php_variable_name("9lives"));
        assert!(!is_php_variable_name("has space"));
        assert!(!is_php_variable_name("a-b"));
    }

    /// Verifies diagnostics render aliases with `$GLOBALS` syntax and locals with `$` syntax.
    #[test]
    fn display_name_uses_php_spelling() {
        assert_eq!(display_name(&alias_name("app")), "$GLOBALS['app']");
        assert_eq!(display_name("app"), "$app");
    }
}
