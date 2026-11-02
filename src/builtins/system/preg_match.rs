//! Purpose:
//! Home of the PHP `preg_match` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The third param `matches` is a by-reference array output and the optional flags and offset
//!   parameters follow PHP's public five-parameter signature.
//! - `lazy_check: true` suppresses the registry's default pre-inference loop so the hook
//!   can infer args[0] and args[1] (pattern and subject) while deliberately skipping
//!   inference of args[2] (`$matches`). `$matches` is a write-only output parameter;
//!   it is not declared before the call and inferring it would produce an
//!   "Undefined variable" error.
//! - `check` validates that args[2] (when present) is a `Variable` expression; passing
//!   a non-variable to the by-ref `$matches` param is a compile error.
//! - A literal pattern with the `u` modifier returns `int|false`: an invalid UTF-8 subject fails.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "preg_match",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::PregMatch),
        crate::builtins::semantics::BuiltinArgumentLowering::PositionalRegex,
    ),
}

/// Infers every input while leaving the by-reference capture destination write-only.
///
/// Infers args[0] (pattern) and args[1] (subject) to trigger type-environment side
/// effects, but deliberately skips inference of args[2] (`$matches`) because it is a
/// write-only output parameter that is undefined before the call. Shared call validation
/// owns l-value enforcement for the by-reference destination.
/// The result is PHP `int|false`: `1` or `0` for a completed search, and `false` when
/// the pattern is invalid or uses the removed `/e` modifier.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    if cx.args.len() >= 3
        && !cx
            .checker
            .is_by_ref_argument_lvalue(&cx.args[2], cx.env)?
    {
        return Err(CompileError::new(
            cx.args[2].span,
            "preg_match() parameter $matches must be passed a variable",
        ));
    }
    if cx.args.len() >= 4 {
        cx.checker.infer_type(&cx.args[3], cx.env)?;
    }
    if cx.args.len() >= 5 {
        cx.checker.infer_type(&cx.args[4], cx.env)?;
    }
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}

/// Returns whether a preg call with this pattern can report failure in its RESULT.
///
/// php returns false (or null for the replace family) when the pattern does not compile, and
/// when a match fails outright -- a subject that is not valid UTF-8 under `u`. A pattern that is
/// not a string literal (a variable, a constant, a concatenation) may not compile, so its call is
/// typed for the failure value; so is a literal carrying `u`. Any other literal keeps its narrow
/// result: the compiler has no PCRE of its own to reject an invalid literal with, so such a call
/// warns and returns the empty result at run time.
pub(crate) fn preg_pattern_can_fail(pattern: &crate::parser::ast::Expr) -> bool {
    match &pattern.kind {
        crate::parser::ast::ExprKind::StringLiteral(literal) => {
            literal_pattern_is_utf8(pattern) || literal_pattern_is_malformed(literal.as_bytes())
        }
        _ => true,
    }
}

/// Returns whether php rejects this literal pattern before compiling it: empty, an alphanumeric,
/// backslash or NUL delimiter, no closing delimiter, or an unknown modifier.
///
/// Mirrors the checks `__rt_preg_strip` makes at run time, including php's forward scan for the
/// closing delimiter. Such a call always fails, so it is typed for its failure value.
pub(crate) fn literal_pattern_is_malformed(pattern: &[u8]) -> bool {
    let start = pattern
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'));
    let Some(start) = start else {
        return true;
    };
    let pattern = &pattern[start..];
    let open = pattern[0];
    if open == 0 || open == b'\\' || open.is_ascii_alphanumeric() {
        return true;
    }
    let close = match open {
        b'(' => b')',
        b'{' => b'}',
        b'[' => b']',
        b'<' => b'>',
        other => other,
    };
    let mut depth = 1usize;
    let mut index = 1usize;
    let end = loop {
        let Some(&byte) = pattern.get(index) else {
            return true;
        };
        if byte == b'\\' {
            index += 2;
            continue;
        }
        if open == close {
            if byte == close {
                break index;
            }
        } else if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                break index;
            }
        }
        index += 1;
    };
    !pattern[end + 1..]
        .iter()
        .all(|modifier| b"imsuUAxDJnSXr \n\r".contains(modifier))
        || !literal_regex_body_is_balanced(&pattern[1..end])
}

/// Returns whether a regex body's unescaped groups and character classes all close.
///
/// PCRE2 rejects `/(/` when it COMPILES the pattern, past every delimiter check, so the call
/// still fails and must be typed for its failure value: `@preg_grep('/(/', $a)` answered an
/// empty array where php answers false. This is the cheap structural half of that compile, and
/// it only has to err one way: a body it wrongly calls unbalanced (a parenthesis inside an
/// `x`-mode comment, say) is merely typed `T|false`, which is always safe.
fn literal_regex_body_is_balanced(body: &[u8]) -> bool {
    let mut groups = 0usize;
    let mut index = 0usize;
    while index < body.len() {
        match body[index] {
            b'\\' => index += 2,
            b'[' => {
                // `]` right after `[` or `[^` is a literal member, not the class's end.
                index += 1;
                if body.get(index) == Some(&b'^') {
                    index += 1;
                }
                if body.get(index) == Some(&b']') {
                    index += 1;
                }
                loop {
                    match body.get(index) {
                        None => return false,
                        Some(b'\\') => index += 2,
                        Some(b']') => break,
                        Some(_) => index += 1,
                    }
                }
                index += 1;
            }
            b'(' => {
                groups += 1;
                index += 1;
            }
            b')' => {
                let Some(open) = groups.checked_sub(1) else {
                    return false;
                };
                groups = open;
                index += 1;
            }
            _ => index += 1,
        }
    }
    groups == 0
}

/// Returns whether a preg pattern is a string literal carrying the `u` modifier.
///
/// Under `u` a subject that is not valid UTF-8 makes the call fail, and php reports the failure
/// in the RESULT: `preg_match` returns false and `preg_replace` null. Symfony's console
/// `Helper::width()` tells a binary string apart by exactly that null. Only a literal pattern is
/// widened, so a call whose pattern cannot fail this way keeps its unboxed result.
pub(crate) fn literal_pattern_is_utf8(pattern: &crate::parser::ast::Expr) -> bool {
    let crate::parser::ast::ExprKind::StringLiteral(pattern) = &pattern.kind else {
        return false;
    };
    let pattern = pattern.trim_start();
    let Some(open) = pattern.chars().next() else {
        return false;
    };
    let close = match open {
        '(' => ')',
        '{' => '}',
        '[' => ']',
        '<' => '>',
        other => other,
    };
    pattern
        .rfind(close)
        .filter(|&end| end > 0)
        .is_some_and(|end| pattern[end + close.len_utf8()..].contains('u'))
}
