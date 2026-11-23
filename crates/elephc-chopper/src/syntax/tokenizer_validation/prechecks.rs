//! Purpose:
//! Runs the source-level prechecks that preserve Zend diagnostics for constructs the view erases.
//!
//! Called from:
//! - `crate::syntax::tokenizer_validation::validate()` before the parser view is built.
//!
//! Key details:
//! - Each check returns the first PHP-shaped error; the main parser never sees blanked regions.

use crate::scanner::{newline_count, operator, Operator, HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE};

use super::diagnostics::*;
use super::parser_view::{parser_view, ternary_colon_before_label};
use super::reference_scan::{
    is_reference_marker, is_write_continuation, reference_chain_end, reference_target_starts_at,
};
use super::*;

/// Token categories whose absence lets validation skip a precheck's full token scan.
///
/// Every precheck reacts only to a small set of token IDs. Recording those IDs once here
/// replaces a dozen full passes over a source that contains none of them.
#[derive(Clone, Copy, Default)]
pub(super) struct TokenPresence {
    /// A `declare` keyword is present.
    pub(super) declare: bool,
    /// A close tag and an `else`/`elseif` are both present.
    pub(super) close_tag_else: bool,
    /// `__halt_compiler` is present.
    pub(super) halt_compiler: bool,
    /// `new` is present.
    pub(super) new: bool,
    /// A reference-shaped ampersand token is present.
    pub(super) reference_ampersand: bool,
    /// `goto` is present.
    pub(super) goto: bool,
    /// A backtick delimiter is present.
    pub(super) backtick: bool,
    /// A `(void)` cast is present.
    pub(super) void_cast: bool,
    /// A `switch` opener, close tag and inline HTML are all present.
    pub(super) switch_html: bool,
    /// A bare `$` or a `${` interpolation opener is present.
    pub(super) dollar_brace: bool,
    /// A member access operator is present.
    pub(super) computed_member: bool,
    /// A bare `$` that may start a declaration is present.
    pub(super) declaration_variable: bool,
    /// A `{$...}` interpolation opener is present.
    pub(super) curly_open: bool,
    /// A variable inside a string context is present.
    pub(super) interpolation_offset: bool,
    /// `list` is present.
    pub(super) list: bool,
}

impl TokenPresence {
    /// Collects the trigger categories for every validation precheck in one forward pass.
    pub(super) fn scan<T: SourceToken>(tokens: &[T]) -> Self {
        let mut presence = Self::default();
        let mut saw_close_tag = false;
        let mut saw_else = false;
        let mut saw_switch = false;
        let mut saw_inline_html = false;
        let mut saw_dollar_brace = false;
        let mut saw_variable = false;
        let mut saw_string_context = false;
        for token in tokens {
            match token.id() {
                tok::T_DECLARE => presence.declare = true,
                tok::T_CLOSE_TAG => saw_close_tag = true,
                tok::T_ELSE | tok::T_ELSEIF => saw_else = true,
                tok::T_HALT_COMPILER => presence.halt_compiler = true,
                tok::T_NEW => presence.new = true,
                tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                | tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG => {
                    presence.reference_ampersand = true;
                }
                tok::T_GOTO => presence.goto = true,
                tok::T_VOID_CAST => presence.void_cast = true,
                tok::T_SWITCH => saw_switch = true,
                tok::T_INLINE_HTML => saw_inline_html = true,
                tok::T_DOLLAR_OPEN_CURLY_BRACES => saw_dollar_brace = true,
                tok::T_DOUBLE_COLON
                | tok::T_OBJECT_OPERATOR
                | tok::T_NULLSAFE_OBJECT_OPERATOR => presence.computed_member = true,
                tok::T_CURLY_OPEN => presence.curly_open = true,
                tok::T_LIST => presence.list = true,
                tok::T_VARIABLE => saw_variable = true,
                tok::T_ENCAPSED_AND_WHITESPACE | tok::T_START_HEREDOC => {
                    saw_string_context = true;
                }
                id if id == i64::from(b'`') => {
                    presence.backtick = true;
                    saw_string_context = true;
                }
                id if id == i64::from(b'$') => {
                    presence.declaration_variable = true;
                    saw_dollar_brace = true;
                }
                id if id == i64::from(b'"') => saw_string_context = true,
                _ => {}
            }
        }
        presence.close_tag_else = saw_close_tag && saw_else;
        presence.switch_html = saw_switch && saw_close_tag && saw_inline_html;
        presence.dollar_brace = saw_dollar_brace;
        presence.interpolation_offset = saw_variable && saw_string_context;
        presence
    }
}

/// Runs the source-level prechecks in diagnostic priority order.
///
/// Each check is skipped when its trigger category is absent from the source, so the
/// common valid source pays for one presence pass instead of one full scan per check.
pub(super) fn run_prechecks<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    presence: &TokenPresence,
    reference_delimiters: &OnceCell<Vec<Option<usize>>>,
    names: &mut Vec<usize>,
) -> Option<ValidationError> {
    if presence.close_tag_else {
        if let Some(error) = close_tag_else_error(source, tokens) {
            return Some(error);
        }
    }
    if presence.halt_compiler {
        if let Some(error) = halt_compiler_scope_error(source, tokens) {
            return Some(error);
        }
    }
    if presence.new {
        if let Some(error) = new_dynamic_static_error(source, tokens) {
            return Some(error);
        }
    }
    if presence.reference_ampersand {
        if let Some(error) = invalid_array_reference_error(source, tokens, reference_delimiters) {
            return Some(error);
        }
        if let Some(error) =
            invalid_reference_write_continuation_error(source, tokens, reference_delimiters)
        {
            return Some(error);
        }
    }
    if presence.goto {
        if let Some(error) = goto_statement_error(source, tokens) {
            return Some(error);
        }
    }
    if presence.backtick {
        if let Some(error) = unterminated_backtick_error(source, tokens) {
            return Some(error);
        }
    }
    if presence.void_cast {
        if let Some(error) = void_cast_expression_error(tokens) {
            return Some(error);
        }
    }
    if presence.switch_html {
        if let Some(error) = switch_case_html_error(source, tokens) {
            return Some(error);
        }
    }
    if presence.dollar_brace {
        if let Some(error) = dollar_brace_expression_error(source, tokens, names) {
            return Some(error);
        }
    }
    if presence.computed_member {
        if let Some(error) = computed_member_expression_error(source, tokens, names) {
            return Some(error);
        }
    }
    if presence.declaration_variable {
        if let Some(error) = declaration_variable_variable_error(source, tokens) {
            return Some(error);
        }
    }
    if presence.curly_open {
        if let Some(error) = complex_interpolation_error(source, tokens) {
            return Some(error);
        }
    }
    if presence.interpolation_offset {
        if let Some(error) = simple_interpolation_offset_error(source, tokens) {
            return Some(error);
        }
    }
    None
}

/// Rejects an unterminated backtick shell string with Zend's delimiter expectation.
#[inline(never)]
pub(crate) fn unterminated_backtick_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    let mut open = false;
    for token in tokens {
        if token.id() == i64::from(b'`') {
            open = !open;
        }
    }
    open.then(|| ValidationError {
        message: "syntax error, unexpected end of file, expecting \"`\"".into(),
        pos: source.len(),
        lexical: false,
    })
}

/// Reports PHP's diagnostics for malformed `goto` statements.
///
/// Valid `goto label;` is blanked from the parser view before parsing, so only invalid shapes
/// reach this scan. Zend reports the token after `goto` (or EOF), not `goto` itself.
#[inline(never)]
pub(crate) fn goto_statement_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    for index in 0..tokens.len() {
        if tokens[index].id() != tok::T_GOTO || !goto_is_statement(tokens, index) {
            continue;
        }
        let next = next_significant_index(tokens, index);
        let Some(next) = next else {
            return Some(end_of_file_expecting(source.len(), Some("\";\"")));
        };
        let id = tokens[next].id();
        if id == tok::T_STRING {
            let after = next_significant_index(tokens, next);
            let Some(after) = after else {
                return Some(end_of_file_expecting(source.len(), Some("\";\"")));
            };
            let after_id = tokens[after].id();
            if after_id == i64::from(b';') || after_id == tok::T_CLOSE_TAG {
                // Valid `goto label;`; the parser view removes it.
                continue;
            }
            return Some(unexpected_token_at(
                source,
                tokens[after].pos(),
                tokens[after].len(),
                Some("\";\""),
            ));
        }
        let noun = if id == tok::T_LNUMBER {
            "integer"
        } else if id == tok::T_VARIABLE {
            "variable"
        } else {
            "token"
        };
        return Some(unexpected_noun_at(
            source,
            tokens[next].pos(),
            tokens[next].len(),
            noun,
            Some("identifier"),
        ));
    }
    None
}

/// Returns whether a `goto` token is in statement position rather than a member name.
#[inline(never)]
pub(crate) fn goto_is_statement<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let Some(previous) = previous_significant_index(tokens, index) else {
        return true;
    };
    matches!(
        tokens[previous].id(),
        id if id == i64::from(b';')
            || id == i64::from(b'{')
            || id == i64::from(b'}')
            || id == i64::from(b':')
            || id == i64::from(b')')
            || id == tok::T_OPEN_TAG
            || id == tok::T_CLOSE_TAG
            || id == tok::T_ELSE
            || id == tok::T_DO
            || id == tok::T_TRY
            || id == tok::T_FINALLY
    )
}

/// Maps each paired delimiter to its partner for reference-chain and arrow-header scans.
///
/// The map follows the same generic nesting rule as the reference-target scan: any closing
/// delimiter closes the innermost opener, regardless of its spelling. A target group can then
/// be skipped in O(1) instead of rescanned once per enclosing reference marker.
pub(super) fn delimiter_pairs<T: SourceToken>(tokens: &[T]) -> Vec<Option<usize>> {
    let mut stack: Vec<usize> = Vec::new();
    let mut pairs = vec![None; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        let id = token.id();
        if id == i64::from(b'(')
            || id == i64::from(b'[')
            || id == i64::from(b'{')
            || id == tok::T_ATTRIBUTE
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            stack.push(index);
        } else if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') {
            if let Some(open) = stack.pop() {
                pairs[open] = Some(index);
                pairs[index] = Some(open);
            }
        }
    }
    pairs
}

/// Rejects a write operator after a reference target where PHP expects a value.
///
/// Assignment references and `foreach` value targets can be followed by ordinary operators
/// (`&$b + 1`, `&$b ? 1 : 2`), but never by an assignment or update (`&$b = 1`, `&$b++`).
/// The precheck runs on native tokens, so it covers both validation views.
#[inline(never)]
pub(crate) fn invalid_reference_write_continuation_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    reference_delimiters: &OnceCell<Vec<Option<usize>>>,
) -> Option<ValidationError> {
    let close_of = reference_delimiters.get_or_init(|| delimiter_pairs(tokens));
    let enclosing = enclosing_delimiters(tokens);
    for index in 0..tokens.len() {
        if !is_reference_marker(tokens[index].id()) {
            continue;
        }
        let Some(previous) = previous_significant_index(tokens, index) else {
            continue;
        };
        let previous_id = tokens[previous].id();
        let assignment = previous_id == i64::from(b'=');
        let foreach_value = previous_id == tok::T_AS;
        let use_capture = previous_id == i64::from(b'(')
            && enclosing[index] != u32::MAX
            && previous_significant_index(tokens, enclosing[index] as usize)
                .is_some_and(|before| tokens[before].id() == tok::T_USE);
        if !assignment && !foreach_value && !use_capture {
            continue;
        }
        if assignment
            && enclosing[index] != u32::MAX
            && paren_is_declaration_parameters(tokens, enclosing[index] as usize)
        {
            // A parameter default is an expression, so the ordinary parser reports the `&`.
            continue;
        }
        let Some(target) = next_significant_index(tokens, index) else {
            continue;
        };
        if !reference_target_starts_at(tokens, target) {
            continue;
        }
        let Some(end) = reference_chain_end(source, tokens, target, close_of) else {
            continue;
        };
        let Some(after) = next_significant_index(tokens, end) else {
            continue;
        };
        if !is_write_continuation(source, &tokens[after]) {
            continue;
        }
        let error = if assignment {
            token_error_at(source, tokens, tokens[after].pos())
        } else if foreach_value {
            reference_chain_error(source, tokens, after)
        } else {
            with_expectation(token_error_at(source, tokens, tokens[after].pos()), "\")\"")
        };
        return Some(error);
    }
    None
}

/// Rejects a malformed `array(&…)`, `list(&…)` or `[&…]` reference entry.
///
/// Zend consumes the marker and reports the token that follows it, so `array(&1)` is
/// `unexpected integer "1"`. A valid target must then stay a write-context variable chain:
/// `array(&$x + 1)` is `unexpected token "+", expecting "->" or "?->" or "["`. Removing the
/// marker before either check would hand the parser a valid plain array entry.
#[inline(never)]
pub(crate) fn invalid_array_reference_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    reference_delimiters: &OnceCell<Vec<Option<usize>>>,
) -> Option<ValidationError> {
    let has_ampersand = tokens.iter().any(|token| {
        token.id() == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
            || token.id() == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
    });
    if !has_ampersand {
        return None;
    }
    // Every nested target group is skipped through this map instead of rescanned, keeping
    // `array(&$a[array(&$b[0])])` linear in the number of tokens.
    let close_of = reference_delimiters.get_or_init(|| delimiter_pairs(tokens));
    // Every open delimiter with its closing token and whether it is an array/list entry list.
    let mut delimiters: Vec<(i64, bool)> = Vec::new();
    for index in 0..tokens.len() {
        let id = tokens[index].id();
        if id == i64::from(b'(') {
            let entry = previous_significant_index(tokens, index).is_some_and(|before| {
                matches!(tokens[before].id(), id if id == tok::T_ARRAY || id == tok::T_LIST)
            });
            delimiters.push((i64::from(b')'), entry));
            continue;
        }
        if id == i64::from(b'[') {
            delimiters.push((i64::from(b']'), bracket_opens_array_literal(tokens, index)));
            continue;
        }
        if id == tok::T_ATTRIBUTE {
            delimiters.push((i64::from(b']'), false));
            continue;
        }
        if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            delimiters.push((i64::from(b'}'), false));
            continue;
        }
        if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') {
            delimiters.pop();
            continue;
        }
        if id != tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
            && id != tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
        {
            continue;
        }
        let Some(previous) = previous_significant_index(tokens, index) else { continue; };
        if tokens[previous].id() != i64::from(b'(')
            && tokens[previous].id() != i64::from(b',')
            && tokens[previous].id() != i64::from(b'[')
            && tokens[previous].id() != tok::T_DOUBLE_ARROW
        {
            continue;
        }
        let Some((terminator, true)) = delimiters.last().copied() else { continue; };
        let Some(target) = next_significant_index(tokens, index) else {
            return Some(end_of_file_expecting(source.len(), None));
        };
        // The parser view checks these targets without hiding syntax errors inside them.
        if tokens[target].id() == i64::from(b'(') || tokens[target].id() == tok::T_READONLY {
            continue;
        }
        if !reference_target_starts_at(tokens, target) {
            return Some(token_error_at(source, tokens, tokens[target].pos()));
        }
        let mut depth = 0usize;
        let mut cursor = target;
        let mut previous_chain_id = 0i64;
        let mut first = true;
        while cursor < tokens.len() {
            let id = tokens[cursor].id();
            if is_trivia_token(id) {
                cursor += 1;
                continue;
            }
            if id == i64::from(b'(')
                || id == i64::from(b'[')
                || id == i64::from(b'{')
                || id == tok::T_ATTRIBUTE
                || id == tok::T_CURLY_OPEN
                || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
            {
                if let Some(close) = close_of[cursor] {
                    previous_chain_id = tokens[close].id();
                    first = false;
                    cursor = close + 1;
                    continue;
                }
                depth += 1;
            } else if id == i64::from(b')')
                || id == i64::from(b']')
                || id == i64::from(b'}')
            {
                if depth == 0 {
                    if id == terminator {
                        break;
                    }
                    return Some(reference_chain_error(source, tokens, cursor));
                }
                depth -= 1;
            } else if depth == 0 {
                if id == i64::from(b',') {
                    break;
                }
                let token = &tokens[cursor];
                let identifier = is_identifier_text(
                    &source[token.pos()..token.pos() + token.len()],
                );
                let member_name = identifier
                    && (first
                        || matches!(
                            previous_chain_id,
                            id if id == tok::T_DOUBLE_COLON
                                || id == tok::T_OBJECT_OPERATOR
                                || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                        ));
                if !reference_chain_base_token(id) && !member_name {
                    return Some(reference_chain_error(source, tokens, cursor));
                }
            }
            previous_chain_id = id;
            first = false;
            cursor += 1;
        }
        if cursor >= tokens.len() {
            return Some(end_of_file_expecting(source.len(), None));
        }
    }
    None
}

/// Returns whether `id` is a base token in a `&` reference target's variable chain.
fn reference_chain_base_token(id: i64) -> bool {
    id == tok::T_VARIABLE
        || id == tok::T_STRING
        || id == tok::T_IDENTIFIER
        || id == tok::T_NAME_FULLY_QUALIFIED
        || id == tok::T_NAME_QUALIFIED
        || id == tok::T_NAME_RELATIVE
        || id == tok::T_STATIC
        || id == i64::from(b'\\')
        || id == i64::from(b'$')
        || id == tok::T_DOUBLE_COLON
        || id == tok::T_OBJECT_OPERATOR
        || id == tok::T_NULLSAFE_OBJECT_OPERATOR
}

/// Zend's expected tokens when a reference target needs a variable dereference.
const REFERENCE_CONTINUATION: &str = "\"->\" or \"?->\" or \"[\"";

/// Builds Zend's reference-chain continuation diagnostic for one token.
fn reference_chain_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    index: usize,
) -> ValidationError {
    unexpected_token_at(
        source,
        tokens[index].pos(),
        tokens[index].len(),
        Some(REFERENCE_CONTINUATION),
    )
}

/// Checks the postfix chain of a grouped reference or a reference to a `readonly()` call.
///
/// A group or class constant needs another dereference. A call, index, object member or
/// static variable can finish the chain. The shared parser still validates the expressions
/// inside the skipped delimiters and the expression following a valid reference.
pub(super) fn normalized_reference_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    target: usize,
    close_of: &[Option<usize>],
    assignment: bool,
    end_pos: usize,
) -> Option<ReferenceError> {
    let target_pos = tokens[target].pos();
    let mut referenceable = tokens[target].id() == tok::T_READONLY;
    let open = if referenceable {
        let after = next_significant_index(tokens, target);
        let Some(open) = after.filter(|&open| tokens[open].id() == i64::from(b'(')) else {
            return Some(ReferenceError {
                diagnostic: reference_expectation_error(source, tokens, after, end_pos, "\"(\""),
                target_pos,
                readonly_without_call: true,
            });
        };
        open
    } else {
        target
    };
    let close = close_of[open].filter(|&close| tokens[close].id() == i64::from(b')'))?;
    let mut next = next_significant_index(tokens, close);
    while let Some(index) = next {
        let operator = tokens[index].id();
        let end = match operator {
            id if id == i64::from(b'(') || id == i64::from(b'[') => {
                referenceable = true;
                let expected = if id == i64::from(b'(') { b')' } else { b']' };
                close_of[index].filter(|&close| tokens[close].id() == i64::from(expected))?
            }
            tok::T_OBJECT_OPERATOR | tok::T_NULLSAFE_OBJECT_OPERATOR | tok::T_DOUBLE_COLON => {
                let mut member = next_significant_index(tokens, index)?;
                let computed_variable = tokens[member].id() == i64::from(b'$');
                while tokens[member].id() == i64::from(b'$') {
                    member = next_significant_index(tokens, member)?;
                }
                let token = &tokens[member];
                referenceable = operator != tok::T_DOUBLE_COLON
                    || computed_variable
                    || token.id() == tok::T_VARIABLE;
                if token.id() == i64::from(b'{') {
                    close_of[member].filter(|&close| tokens[close].id() == i64::from(b'}'))?
                } else if token.id() == tok::T_VARIABLE
                    || is_identifier_text(&source[token.pos()..token.pos() + token.len()])
                {
                    member
                } else {
                    return None;
                }
            }
            _ => break,
        };
        next = next_significant_index(tokens, end);
    }
    if !referenceable
        || (!assignment && next.is_some_and(|next| {
            let id = tokens[next].id();
            id != i64::from(b',') && id != i64::from(b']') && id != i64::from(b')')
        }))
    {
        return Some(ReferenceError {
            diagnostic: reference_expectation_error(
                source,
                tokens,
                next,
                end_pos,
                REFERENCE_CONTINUATION,
            ),
            target_pos,
            readonly_without_call: false,
        });
    }
    let next = next?;
    let token = &tokens[next];
    if assignment
        && invalid_reference_assignment_continuation(&source[token.pos()..token.pos() + token.len()])
    {
        return Some(ReferenceError {
            diagnostic: unexpected_token_at(source, token.pos(), token.len(), None),
            target_pos,
            readonly_without_call: false,
        });
    }
    None
}

/// Returns whether the token would consume the reference source as an assignment or update.
fn invalid_reference_assignment_continuation(source: &[u8]) -> bool {
    source == b"="
        || matches!(
            operator(source),
            Some((
                Operator::PlusAssign
                | Operator::MinusAssign
                | Operator::StarAssign
                | Operator::SlashAssign
                | Operator::DotAssign
                | Operator::PercentAssign
                | Operator::AmpAssign
                | Operator::PipeAssign
                | Operator::CaretAssign
                | Operator::LessLessAssign
                | Operator::GreaterGreaterAssign
                | Operator::QuestionQuestionAssign
                | Operator::StarStarAssign
                | Operator::PlusPlus
                | Operator::MinusMinus,
                _,
            ))
        )
}

/// Reports a missing reference continuation at the offending token or the end of the view.
fn reference_expectation_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    next: Option<usize>,
    end_pos: usize,
    expecting: &str,
) -> ValidationError {
    let Some(token) = next.map(|index| &tokens[index]) else {
        return end_of_file_expecting(end_pos, Some(expecting));
    };
    let error = if token.id() == tok::T_CLOSE_TAG {
        ValidationError {
            message: "syntax error, unexpected token \";\"".into(),
            pos: token.pos(),
            lexical: false,
        }
    } else {
        token_error_at(source, std::slice::from_ref(token), token.pos())
    };
    with_expectation(error, expecting)
}

/// Rejects `new $class::Name` and `new $class::{...}` class-name chains.
///
/// PHP only accepts `new $class::$property(...)`; the general expression parser is more
/// permissive, so the dynamic-class shapes are rejected explicitly.
#[inline(never)]
pub(crate) fn new_dynamic_static_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    for index in 0..tokens.len() {
        if tokens[index].id() != tok::T_NEW {
            continue;
        }
        let Some(name) = next_significant_index(tokens, index) else { continue; };
        if !matches!(
            tokens[name].id(),
            id if id == tok::T_VARIABLE || id == tok::T_STRING || id == tok::T_IDENTIFIER
        ) {
            continue;
        }
        let Some(colon) = next_significant_index(tokens, name) else { continue; };
        if tokens[colon].id() != tok::T_DOUBLE_COLON {
            continue;
        }
        let Some(member) = next_significant_index(tokens, colon) else { continue; };
        if tokens[member].id() == tok::T_VARIABLE {
            continue;
        }
        let noun = if tokens[member].id() == tok::T_STRING {
            "identifier"
        } else {
            "token"
        };
        return Some(unexpected_noun_at(
            source,
            tokens[member].pos(),
            tokens[member].len(),
            noun,
            Some("variable or \"$\""),
        ));
    }
    None
}

/// Returns Zend's warning for every `declare(encoding=...)` directive.
#[inline(never)]
pub(crate) fn declare_encoding_warnings<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_DECLARE {
            continue;
        }
        let Some(close) = complete_declare_encoding(source, tokens, index) else { continue; };
        warnings.push(ValidationWarning {
            message: "declare(encoding=...) ignored because Zend multibyte feature is turned off by settings".into(),
            // Zend reports this at the closing `)`: a multiline directive list prints the
            // line of that token, not the `declare` keyword.
            pos: tokens[close].pos(),
            // Zend reports this through php_error_docref(NULL, E_WARNING, ...), not as a
            // deprecation.
            severity: 2,
        });
    }
    warnings
}

/// Returns the closing `)` of a complete `declare(...)` list when it sets `encoding`.
///
/// Zend compiles a `declare` only after its whole directive list parses. A syntax error
/// inside the list therefore never reports the encoding warning, so the token scan has to
/// prove the list complete before claiming one.
pub(crate) fn complete_declare_encoding<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    declare: usize,
) -> Option<usize> {
    let open = next_significant_index(tokens, declare)?;
    if tokens[open].id() != i64::from(b'(') {
        return None;
    }
    let mut encoding = false;
    let mut openers: Vec<u8> = Vec::new();
    let mut expect_name = true;
    let mut has_equals = false;
    let mut has_value = false;
    let mut index = next_significant_index(tokens, open)?;
    loop {
        let id = tokens[index].id();
        if expect_name {
            if id != tok::T_STRING {
                return None;
            }
            let text = &source[tokens[index].pos()..tokens[index].pos() + tokens[index].len()];
            encoding |= text.eq_ignore_ascii_case(b"encoding");
            expect_name = false;
            has_equals = false;
            has_value = false;
        } else if !has_equals {
            if id != i64::from(b'=') {
                return None;
            }
            has_equals = true;
        } else if id == i64::from(b')') && openers.is_empty() {
            return (has_value && encoding).then_some(index);
        } else if id == i64::from(b',') && openers.is_empty() {
            if !has_value {
                return None;
            }
            expect_name = true;
        } else {
            if let Some(opener) = [b'(', b'[', b'{']
                .into_iter()
                .find(|opener| i64::from(*opener) == id)
            {
                openers.push(opener);
            } else if let Some(expected) = [b')', b']', b'}']
                .into_iter()
                .find(|closer| i64::from(*closer) == id)
            {
                let opener = match expected {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                if openers.pop() != Some(opener) {
                    return None;
                }
            }
            has_value = true;
        }
        index = next_significant_index(tokens, index)?;
    }
}

/// Validates the expression inside `${...}` before the parser view erases it.
///
/// Zend parses the contents as an expression (variable variables included), so `${1+}` and
/// `${}` are syntax errors even though the view rewrites the whole interpolation to `$x`.
pub(crate) fn dollar_brace_expression_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    names: &mut Vec<usize>,
) -> Option<ValidationError> {
    // Matching braces are computed once: a nested `${${...}}` chain otherwise rescans the
    // remaining source from every `$` and turns a linear parse into quadratic work.
    let close_of = brace_matches(tokens);
    // Only nested `${...}` bodies count toward the view cap: ordinary `{...}` blocks must
    // still have their interpolation checked (`256 × {` + `${1+}` is a syntax error).
    let mut dollar_closes: Vec<usize> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        while dollar_closes.last() == Some(&index) {
            dollar_closes.pop();
        }
        let opener_depth = dollar_closes.len();
        // Inside a string the lexer emits `${` as one T_DOLLAR_OPEN_CURLY_BRACES token; in
        // code it is a plain `$` followed by `{`, so the body starts after the brace.
        let (inner_start, first_inner, opener) = if token.id() == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            (token.pos() + token.len(), index + 1, index)
        } else if token.id() == i64::from(b'$') {
            let Some(brace) = next_significant_index(tokens, index) else {
                continue;
            };
            if tokens[brace].id() != i64::from(b'{') {
                continue;
            }
            (
                tokens[brace].pos() + tokens[brace].len(),
                brace + 1,
                brace,
            )
        } else {
            continue;
        };
        let cursor = close_of[opener];
        if cursor == NO_BRACE {
            return Some(ValidationError {
                message: "syntax error, unexpected end of file, expecting \"}\"".into(),
                pos: source.len(),
                lexical: false,
            });
        }
        let cursor = cursor as usize;
        // Bound the nested-view construction on pathological chains; the outermost 256
        // bodies are still validated, which covers every real source.
        if opener_depth >= MAX_DOLLAR_BRACE_NESTING {
            continue;
        }
        dollar_closes.push(cursor);
        let close_pos = tokens[cursor].pos();
        let close_len = tokens[cursor].len();
        let inner = &source[inner_start..close_pos];
        if inner.iter().all(u8::is_ascii_whitespace) {
            return Some(unexpected_token_at(source, close_pos, close_len, None));
        }
        // Zend's lexer emits T_STRING_VARNAME only for the simple `${name}` and
        // `${name[expr]}` forms inside strings. Re-parsing that text as an expression would
        // turn a keyword name such as `class` into the keyword token, so validate the shape
        // directly and parse only the offset expression.
        if tokens[first_inner].id() == tok::T_STRING_VARNAME {
            let name = &tokens[first_inner];
            let Some(after_name) = next_significant_index(tokens, first_inner) else {
                continue;
            };
            if after_name == cursor {
                continue;
            }
            if tokens[after_name].id() != i64::from(b'[') {
                return Some(unexpected_token_at(source, name.pos(), name.len(), None));
            }
            let Some(close_bracket) = matching_bracket_before(tokens, after_name, cursor) else {
                return Some(ValidationError {
                    message: "Unclosed '[' does not match '}'".into(),
                    pos: tokens[after_name].pos(),
                    lexical: false,
                });
            };
            if let Some(error) = expression_slice_error(
                source,
                tokens,
                after_name + 1,
                close_bracket,
                tokens[close_bracket].pos(),
                names,
            ) {
                return Some(error);
            }
            let after_bracket = next_significant_index(tokens, close_bracket);
            if after_bracket != Some(cursor) {
                let (pos, len) = after_bracket
                    .map(|index| (tokens[index].pos(), tokens[index].len()))
                    .unwrap_or((close_pos, close_len));
                return Some(unexpected_token_at(source, pos, len, Some("\"}\"")));
            }
            continue;
        }
        if let Some(error) =
            expression_slice_error(source, tokens, first_inner, cursor, close_pos, names)
        {
            return Some(error);
        }
    }
    None
}

/// Validates the expression inside a computed static member name, `C::{expr}`.
///
/// PHP accepts any expression there (`C::{'FO' . 'O'}`, `C::{strtoupper('foo')}`), but the
/// parser view only needs a member name, so the body is validated here and then rewritten to
/// an identifier. Returning the body to the view would leave `C:: FO . O`, which is not a
/// member expression.
pub(crate) fn computed_member_expression_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    names: &mut Vec<usize>,
) -> Option<ValidationError> {
    let close_of = brace_matches(tokens);
    let mut member_closes: Vec<usize> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        while member_closes.last() == Some(&index) {
            member_closes.pop();
        }
        let opener_depth = member_closes.len();
        if token.id() != tok::T_DOUBLE_COLON
            && token.id() != tok::T_OBJECT_OPERATOR
            && token.id() != tok::T_NULLSAFE_OBJECT_OPERATOR
        {
            continue;
        }
        let Some(brace) = next_significant_index(tokens, index) else {
            continue;
        };
        if tokens[brace].id() != i64::from(b'{') {
            continue;
        }
        let close = close_of[brace];
        if close == NO_BRACE {
            return Some(ValidationError {
                message: "syntax error, unexpected end of file, expecting \"}\"".into(),
                pos: source.len(),
                lexical: false,
            });
        }
        let close = close as usize;
        if opener_depth >= MAX_COMPUTED_MEMBER_NESTING {
            continue;
        }
        member_closes.push(close);
        let inner_start = tokens[brace].pos() + tokens[brace].len();
        let inner = &source[inner_start..tokens[close].pos()];
        if inner.iter().all(u8::is_ascii_whitespace) {
            return Some(unexpected_token_at(
                source,
                tokens[close].pos(),
                tokens[close].len(),
                None,
            ));
        }
        if let Some(error) = expression_slice_error(
            source,
            tokens,
            brace + 1,
            close,
            tokens[close].pos(),
            names,
        ) {
            return Some(error);
        }
    }
    None
}

/// Rejects variable variables in declaration name slots before the shared lexer sees them.
///
/// PHP reports `function f(int $$a)` at the first `$` with `expecting variable`. The parser
/// view keeps that `$` so the parser can reject it, but the view lexer has no variable-name
/// table and would fail first with an internal message. This scan produces Zend's diagnostic
/// from the native tokens, where the declaration context is still visible.
pub(crate) fn declaration_variable_variable_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    if !tokens.iter().any(|token| token.id() == i64::from(b'$')) {
        return None;
    }
    /// One open delimiter. `declaration_default` is `Some` only for a declaration parameter
    /// list; `previous_declaration` restores the enclosing declaration scope when it pops.
    struct Scope {
        declaration_default: Option<bool>,
        previous_declaration: Option<usize>,
    }
    let mut scopes: Vec<Scope> = Vec::new();
    let mut parens: Vec<usize> = Vec::new();
    let mut declaration_index: Option<usize> = None;
    let mut previous_significant = 0i64;
    for (index, token) in tokens.iter().enumerate() {
        let id = token.id();
        if id == i64::from(b'(') {
            let declaration = paren_is_declaration_parameters(tokens, index);
            let previous_declaration = declaration.then_some(declaration_index).flatten();
            scopes.push(Scope {
                declaration_default: declaration.then_some(false),
                previous_declaration,
            });
            if declaration {
                declaration_index = Some(scopes.len() - 1);
            }
            parens.push(index);
        } else if id == i64::from(b'[') || id == tok::T_ATTRIBUTE {
            scopes.push(Scope {
                declaration_default: None,
                previous_declaration: None,
            });
        } else if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            scopes.push(Scope {
                declaration_default: None,
                previous_declaration: None,
            });
        } else if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') {
            if id == i64::from(b')') {
                parens.pop();
            }
            if let Some(scope) = scopes.pop() {
                if scope.declaration_default.is_some() {
                    declaration_index = scope.previous_declaration;
                }
            }
        } else if id == i64::from(b'=') || id == i64::from(b',') {
            // Only the top level of the declaration list starts a parameter or its default;
            // a comma inside an array/call belongs to the default expression.
            if declaration_index.is_some_and(|declaration| declaration + 1 == scopes.len()) {
                if let Some(scope) = declaration_index.and_then(|declaration| scopes.get_mut(declaration)) {
                    scope.declaration_default = Some(id == i64::from(b'='));
                }
            }
        }
        if id == i64::from(b'$') {
            let default_started = declaration_index
                .and_then(|declaration| scopes.get(declaration))
                .is_some_and(|scope| scope.declaration_default == Some(true));
            let starts_variable = next_significant_index(tokens, index).is_some_and(|next| {
                let next = tokens[next].id();
                next == i64::from(b'$')
                    || next == i64::from(b'{')
                    || next == tok::T_VARIABLE
                    || next == tok::T_DOLLAR_OPEN_CURLY_BRACES
            });
            if !default_started
                && starts_variable
                && dollar_starts_declaration(
                    tokens,
                    index,
                    previous_significant,
                    parens.last().copied(),
                )
            {
                return Some(unexpected_token_at(
                    source,
                    token.pos(),
                    token.len(),
                    Some("variable"),
                ));
            }
        }
        if !is_trivia(id) && id != 0 {
            previous_significant = id;
        }
    }
    None
}

/// Validates the first token of every `{$...}` interpolation before the parser view copies it.
///
/// The shared lexer rejects `{$ b}` (and siblings) with an internal "Expected variable name"
/// message and a synthetic fragment position, so Zend's first-token diagnostic is built from the
/// native token stream here, where the real source position is known.
pub(crate) fn complex_interpolation_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    if !tokens.iter().any(|token| token.id() == tok::T_CURLY_OPEN) {
        return None;
    }
    let close_of = brace_matches(tokens);
    let nearest = enclosing_delimiters(tokens);
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_CURLY_OPEN {
            continue;
        }
        let first = next_significant_index(tokens, index);
        let close = close_of[index];
        if close == NO_BRACE {
            if let Some(error) = unterminated_interpolation_error(source, tokens, index, &nearest) {
                return Some(error);
            }
            continue;
        }
        let close = close as usize;
        let Some(first) = first else { continue; };
        let expectation = "variable or \"{\" or \"$\"";
        if first == close {
            return Some(unexpected_token_at(
                source,
                tokens[close].pos(),
                tokens[close].len(),
                Some(expectation),
            ));
        }
        let mut cursor = first;
        while tokens[cursor].id() == i64::from(b'$') {
            let Some(next) = next_significant_index(tokens, cursor) else { break; };
            if next >= close || tokens[next].id() != i64::from(b'$') {
                break;
            }
            cursor = next;
        }
        if tokens[cursor].id() == i64::from(b'$') {
            let next = next_significant_index(tokens, cursor);
            let valid = next.is_some_and(|next| {
                next != close
                    && matches!(
                        tokens[next].id(),
                        id if id == tok::T_VARIABLE
                            || id == i64::from(b'{')
                            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
                    )
            });
            if !valid {
                return Some(match next {
                    Some(next) if next != close => token_noun_error(
                        source,
                        tokens,
                        next,
                        expectation,
                    ),
                    _ => unexpected_token_at(
                        source,
                        tokens[close].pos(),
                        tokens[close].len(),
                        Some(expectation),
                    ),
                });
            }
        } else if !matches!(
            tokens[cursor].id(),
            id if id == tok::T_VARIABLE
                || id == i64::from(b'{')
                || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        ) {
            return Some(token_noun_error(source, tokens, cursor, expectation));
        }
        let open = nearest[close];
        if open != u32::MAX && open as usize >= first {
            let text = if tokens[open as usize].id() == i64::from(b'(') { "(" } else { "[" };
            return Some(ValidationError {
                message: format!("Unclosed '{text}' does not match '}}'"),
                pos: tokens[open as usize].pos(),
                lexical: false,
            });
        }
    }
    None
}

/// Reports the opening delimiter left unmatched before the closing quote of an interpolation.
pub(crate) fn unterminated_interpolation_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    open: usize,
    nearest: &[u32],
) -> Option<ValidationError> {
    let delimiter = tokens.iter().enumerate().skip(open + 1).find_map(|(index, token)| {
        let id = token.id();
        (id == i64::from(b'"') || id == i64::from(b'`') || id == tok::T_END_HEREDOC)
            .then_some(index)
    })?;
    let pair = nearest[delimiter];
    if pair != u32::MAX && pair as usize >= open + 1 {
        let text = if tokens[pair as usize].id() == i64::from(b'(') { "(" } else { "[" };
        return Some(ValidationError {
            message: format!("Unclosed '{text}'"),
            pos: tokens[pair as usize].pos(),
            lexical: false,
        });
    }
    let last = previous_significant_index(tokens, delimiter)?;
    if matches!(
        tokens[last].id(),
        id if id == tok::T_OBJECT_OPERATOR || id == tok::T_NULLSAFE_OBJECT_OPERATOR
    ) {
        return Some(delimiter_error(
            source,
            tokens,
            delimiter,
            "identifier or variable or \"{\" or \"$\"",
        ));
    }
    None
}

/// Rejects a simple `"$a[...]"` offset that the closing quote interrupts.
pub(crate) fn simple_interpolation_offset_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_VARIABLE {
            continue;
        }
        let previous = previous_significant_index(tokens, index);
        let in_string = previous.is_some_and(|previous| {
            matches!(
                tokens[previous].id(),
                id if id == i64::from(b'"')
                    || id == i64::from(b'`')
                    || id == tok::T_ENCAPSED_AND_WHITESPACE
                    || id == tok::T_START_HEREDOC
            )
        });
        if !in_string {
            continue;
        }
        let Some(open) = next_significant_index(tokens, index) else { continue; };
        if tokens[open].id() != i64::from(b'[') {
            continue;
        }
        // Stop at the first `]` (the simple offset cannot nest) or at the closing string
        // delimiter. Scanning to the delimiter first made a long string of `$a[0]` entries
        // quadratic.
        let mut close = None;
        let mut delimiter = None;
        for (index, token) in tokens.iter().enumerate().skip(open + 1) {
            let id = token.id();
            if id == i64::from(b']') {
                close = Some(index);
                break;
            }
            if id == i64::from(b'"') || id == i64::from(b'`') || id == tok::T_END_HEREDOC {
                delimiter = Some(index);
                break;
            }
        }
        if let Some(close) = close {
            // The offset is closed, but the simple syntax accepts exactly one key: a number,
            // a bareword, a variable, or a negated number.
            let Some(mut key) = next_significant_index(tokens, open) else { continue; };
            if key >= close {
                continue;
            }
            if tokens[key].id() == i64::from(b'-') {
                let Some(next) = next_significant_index(tokens, key) else { continue; };
                key = next;
            }
            let is_key = matches!(
                tokens[key].id(),
                id if id == tok::T_NUM_STRING
                    || id == tok::T_LNUMBER
                    || id == tok::T_DNUMBER
                    || id == tok::T_STRING
                    || id == tok::T_IDENTIFIER
                    || id == tok::T_VARIABLE
            );
            if !is_key {
                return Some(token_noun_error(source, tokens, key, "\"]\""));
            }
            if let Some(after) = next_significant_index(tokens, key) {
                if after != close {
                    return Some(token_noun_error(source, tokens, after, "\"]\""));
                }
            }
            continue;
        }
        let Some(delimiter) = delimiter else { continue; };
        let Some(first) = next_significant_index(tokens, open) else { continue; };
        if first >= delimiter {
            return Some(delimiter_error(
                source,
                tokens,
                delimiter,
                "\"-\" or identifier or variable or number",
            ));
        }
        let (after_key, negated) = if tokens[first].id() == i64::from(b'-') {
            match next_significant_index(tokens, first) {
                Some(next) if next < delimiter => (Some(next), true),
                _ => (None, true),
            }
        } else {
            (Some(first), false)
        };
        let Some(after_key) = after_key else {
            return Some(delimiter_error(source, tokens, delimiter, "number"));
        };
        let is_key = matches!(
            tokens[after_key].id(),
            id if id == tok::T_NUM_STRING
                || id == tok::T_LNUMBER
                || id == tok::T_DNUMBER
                || id == tok::T_STRING
                || id == tok::T_IDENTIFIER
                || id == tok::T_VARIABLE
        );
        if !is_key {
            return Some(token_noun_error(source, tokens, after_key, "\"]\""));
        }
        let after = next_significant_index(tokens, after_key);
        return Some(match after {
            Some(after) if after >= delimiter => {
                delimiter_error(source, tokens, delimiter, "\"]\"")
            }
            Some(after) => token_noun_error(source, tokens, after, "\"]\""),
            None if negated => delimiter_error(source, tokens, delimiter, "number"),
            None => delimiter_error(source, tokens, delimiter, "\"]\""),
        });
    }
    None
}

/// Precomputes the nearest open `(` or `[` before every token in one forward pass.
pub(crate) fn enclosing_delimiters<T: SourceToken>(tokens: &[T]) -> Vec<u32> {
    let mut stack: Vec<(usize, i64)> = Vec::new();
    let mut nearest = vec![u32::MAX; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        nearest[index] = stack.last().map_or(u32::MAX, |(open, _)| *open as u32);
        let id = token.id();
        if id == i64::from(b'(') || id == i64::from(b'[') {
            stack.push((index, id));
        } else if id == i64::from(b')') || id == i64::from(b']') {
            let expected = if id == i64::from(b')') { i64::from(b'(') } else { i64::from(b'[') };
            if stack.last().is_some_and(|(_, open)| *open == expected) {
                stack.pop();
            }
        }
    }
    nearest
}

/// Deepest nested `${...}` body the validator rebuilds before skipping the inner view.
pub(crate) const MAX_DOLLAR_BRACE_NESTING: usize = 256;

/// Deepest nested computed-member body the validator rebuilds before skipping the inner view.
pub(crate) const MAX_COMPUTED_MEMBER_NESTING: usize = 256;

/// Sentinel for a `{`-style opener with no matching `}`.
pub(crate) const NO_BRACE: u32 = u32::MAX;

/// Precomputes the matching `}` for every `{`-style opener in one pass.
pub(crate) fn brace_matches<T: SourceToken>(tokens: &[T]) -> Vec<u32> {
    let mut stack: Vec<usize> = Vec::new();
    let mut close = vec![NO_BRACE; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        let id = token.id();
        if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            stack.push(index);
        } else if id == i64::from(b'}') {
            if let Some(open) = stack.pop() {
                close[open] = u32::try_from(index).unwrap_or(NO_BRACE);
            }
        }
    }
    close
}

/// Finds the `}` matching an interpolation or computed-member body, counting nested scopes.
///
/// An interpolated string inside the body opens its own `{$...}`/`${...}` scope, which the
/// lexer emits as `T_CURLY_OPEN` / `T_DOLLAR_OPEN_CURLY_BRACES`, so those count as openers too.
pub(crate) fn matching_brace_end<T: SourceToken>(
    tokens: &[T],
    first_inner: usize,
    limit: usize,
) -> Option<usize> {
    let mut depth = 1usize;
    for index in first_inner..limit {
        let id = tokens[index].id();
        if id == i64::from(b'{')
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            depth += 1;
        } else if id == i64::from(b'}') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Validates that the tokens in `[first, end)` form exactly one expression.
///
/// The slice is normalized exactly like the enclosing parser view, so PHP-only variable
/// variables (`${$$a}`, nested `${...}`) stay valid while real syntax errors survive. The view
/// maps every byte back to its real source position; `fallback_pos` is where end-of-input
/// diagnostics land.
pub(crate) fn expression_slice_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    first: usize,
    end: usize,
    fallback_pos: usize,
    names: &mut Vec<usize>,
) -> Option<ValidationError> {
    let floor = tokens
        .get(first)
        .map_or(fallback_pos, |token| token.pos())
        .min(fallback_pos);
    let (view, map, reference_error) =
        parser_view(source, &tokens[first..end], fallback_pos, &OnceCell::new());
    let lines = line_starts(&view);
    let map_position = |view_pos: usize| {
        map.get(view_pos)
            .copied()
            .map(|pos| pos as usize)
            .unwrap_or(fallback_pos)
            .clamp(floor, fallback_pos)
    };
    let view_tokens = match crate::syntax::lexer::tokenize(&view) {
        Ok(view_tokens) => view_tokens,
        Err(error) => {
            let view_pos = view_offset(&view, &lines, error.span.line, error.span.col);
            let error = token_error_at(source, tokens, map_position(view_pos));
            return prefer_reference_error(Some(error), reference_error);
        }
    };
    let mut position = usize::from(
        view_tokens
            .first()
            .is_some_and(|(token, _)| *token == crate::syntax::lexer::Token::OpenTag),
    );
    let feedback = crate::syntax::parser::tokenizer_feedback::FeedbackGuard::start();
    let parsed = crate::syntax::parser::expr::parse_expr(&view_tokens, &mut position);
    let recorded = feedback.names();
    drop(feedback);
    let positions: Vec<(u32, u32)> = recorded
        .iter()
        .map(|(span, _)| (span.line, span.col))
        .collect();
    for offset in view_offsets(&view, &lines, &positions) {
        names.push(map_position(offset));
    }
    // Zend reads exactly one expression, so a leftover token is the syntax error: `${1; 2}`
    // reports the `;` and `${1 2}` reports the `2`.
    let error = match parsed {
        Ok(_) => match view_tokens.get(position) {
            Some((crate::syntax::lexer::Token::Eof, _)) | None => {
                return prefer_reference_error(None, reference_error);
            }
            Some((_, metadata)) => metadata.span,
        },
        Err(error) => error.span,
    };
    let view_pos = view_offset(&view, &lines, error.line, error.col);
    let error = token_error_at(source, tokens, map_position(view_pos));
    prefer_reference_error(Some(error), reference_error)
}

/// Finds the `)` matching the `(` at `open`, counting nested parentheses.
pub(crate) fn matching_paren_end<T: SourceToken>(tokens: &[T], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..tokens.len() {
        let id = tokens[index].id();
        if id == i64::from(b'(') {
            depth += 1;
        } else if id == i64::from(b')') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Finds the `]` matching the `[` at `open`, scanning no further than `limit`.
pub(crate) fn matching_bracket_before<T: SourceToken>(tokens: &[T], open: usize, limit: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..limit {
        let id = tokens[index].id();
        if id == i64::from(b'[') {
            depth += 1;
        } else if id == i64::from(b']') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// Rejects an `else`/`elseif` that follows a close tag outside a braced or alternative body.
///
/// PHP ends the whole `if` statement at `?>`, so `if (1) ?>x<?php else ...` is a parse error,
/// while `if (1): ?>x<?php else:` and `if (1) { ?>x<?php } else` are valid.
#[inline(never)]
pub(crate) fn close_tag_else_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    // One entry per open alternative-syntax construct: true for `if`, false for the others.
    let mut alternatives: Vec<bool> = Vec::new();
    for index in 0..tokens.len() {
        let id = tokens[index].id();
        let control_keyword = matches!(
            id,
            id if id == tok::T_IF
                || id == tok::T_WHILE
                || id == tok::T_FOR
                || id == tok::T_FOREACH
                || id == tok::T_SWITCH
                || id == tok::T_DECLARE
                || id == tok::T_ENDIF
                || id == tok::T_ENDWHILE
                || id == tok::T_ENDFOR
                || id == tok::T_ENDFOREACH
                || id == tok::T_ENDSWITCH
                || id == tok::T_ENDDECLARE
        );
        if control_keyword && control_keyword_is_name(tokens, index) {
            continue;
        }
        if id == tok::T_IF {
            if alternative_header_colon(tokens, index) {
                alternatives.push(true);
            }
            continue;
        }
        if matches!(
            id,
            id if id == tok::T_WHILE
                || id == tok::T_FOR
                || id == tok::T_FOREACH
                || id == tok::T_SWITCH
                || id == tok::T_DECLARE
        ) {
            if alternative_header_colon(tokens, index) {
                alternatives.push(false);
            }
            continue;
        }
        if matches!(
            id,
            id if id == tok::T_ENDIF
                || id == tok::T_ENDWHILE
                || id == tok::T_ENDFOR
                || id == tok::T_ENDFOREACH
                || id == tok::T_ENDSWITCH
                || id == tok::T_ENDDECLARE
        ) {
            alternatives.pop();
            continue;
        }
        if id != tok::T_CLOSE_TAG {
            continue;
        }
        let Some(next) = next_non_inline_html_index(tokens, index) else { continue; };
        let next_id = tokens[next].id();
        if next_id != tok::T_ELSE && next_id != tok::T_ELSEIF {
            continue;
        }
        if !alternatives.iter().any(|is_if| *is_if)
            || close_tag_after_empty_control_header(tokens, index)
        {
            return Some(unexpected_token_at(
                source,
                tokens[next].pos(),
                tokens[next].len(),
                Some("end of file"),
            ));
        }
        // A normal `else`/`elseif` cannot follow a close tag: the ordinary `if` it would bind
        // to ended at the tag. Only the alternative forms (`else:`, `elseif (...):`) are valid.
        if next_id == tok::T_ELSE {
            let after = next_significant_index(tokens, next).unwrap_or(next);
            if tokens[after].id() != i64::from(b':') {
                return Some(unexpected_token_at(
                    source,
                    tokens[after].pos(),
                    tokens[after].len(),
                    Some("\":\""),
                ));
            }
        } else if !alternative_header_colon(tokens, next) {
            return Some(unexpected_token_at(
                source,
                tokens[next].pos(),
                tokens[next].len(),
                Some("\":\""),
            ));
        }
    }
    None
}

/// Returns whether an alternative-syntax header (`if (...)`, `while (...)`, ...) ends in `:`.
pub(crate) fn alternative_header_colon<T: SourceToken>(tokens: &[T], keyword: usize) -> bool {
    alternative_header_colon_index(tokens, keyword).is_some()
}

/// Returns the `:` that closes an alternative-syntax header, when the header has one.
fn alternative_header_colon_index<T: SourceToken>(tokens: &[T], keyword: usize) -> Option<usize> {
    let open = next_significant_index(tokens, keyword)
        .filter(|&index| tokens[index].id() == i64::from(b'('))?;
    let mut depth = 0usize;
    let mut cursor = open;
    while cursor < tokens.len() {
        let id = tokens[cursor].id();
        if id == i64::from(b'(') {
            depth += 1;
        } else if id == i64::from(b')') {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        cursor += 1;
    }
    next_significant_index(tokens, cursor).filter(|&next| tokens[next].id() == i64::from(b':'))
}

/// Returns whether a control keyword is actually a member or declaration name.
pub(crate) fn control_keyword_is_name<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    previous_significant_index(tokens, index).is_some_and(|previous| {
        matches!(
            tokens[previous].id(),
            id if id == tok::T_DOUBLE_COLON
                || id == tok::T_OBJECT_OPERATOR
                || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                || id == tok::T_NS_SEPARATOR
                || id == i64::from(b'\\')
                || id == tok::T_FUNCTION
                || id == tok::T_CONST
                || id == tok::T_CASE
        )
    })
}

/// Returns whether the close tag immediately follows a control header that has no body yet.
///
/// `if ($b) ?>...<?php else` cannot attach the `else` to that ordinary `if`: PHP ends the
/// unbraced statement at the close tag. An outer alternative `if` must not rescue it.
pub(crate) fn close_tag_after_empty_control_header<T: SourceToken>(tokens: &[T], close: usize) -> bool {
    let Some(previous) = previous_significant_index(tokens, close) else {
        return false;
    };
    if tokens[previous].id() != i64::from(b')') {
        return false;
    }
    let mut depth = 1usize;
    let mut cursor = previous;
    while cursor > 0 {
        cursor -= 1;
        let id = tokens[cursor].id();
        if id == i64::from(b')') {
            depth += 1;
        } else if id == i64::from(b'(') {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
    }
    if tokens.get(cursor).map(|token| token.id()) != Some(i64::from(b'(')) {
        return false;
    }
    // Only an `if`/`elseif` header can own a following `else`; loops and `declare` accept the
    // close tag as their empty body and leave the `else` to the enclosing alternative `if`.
    previous_significant_index(tokens, cursor)
        .is_some_and(|before| matches!(tokens[before].id(), id if id == tok::T_IF || id == tok::T_ELSEIF))
}

/// Returns the next token after `index`, skipping inline HTML, open tags and trivia.
#[inline(never)]
pub(crate) fn next_non_inline_html_index<T: SourceToken>(tokens: &[T], index: usize) -> Option<usize> {
    let mut cursor = index + 1;
    while cursor < tokens.len() {
        let id = tokens[cursor].id();
        if id == tok::T_INLINE_HTML || id == tok::T_OPEN_TAG || is_trivia_token(id) {
            cursor += 1;
            continue;
        }
        return Some(cursor);
    }
    None
}

/// One open scope that changes how `__halt_compiler()` is diagnosed.
enum HaltScope {
    /// A function, method, control, hook or bare block body.
    Statement,
    /// A class-like body, trait adaptation block or property-hook container.
    ClassLike,
    /// A braced namespace. `line` is the one-based line of its `{`.
    Namespace { line: usize },
    /// `{$...}` or `${...}` interpolation. Its `}` must not close a real block.
    Interpolation,
    /// `if/while/for/foreach/declare (...):` until the matching `end*`.
    ///
    /// `switch (...):` is `Switch` instead: a halt before the first `case` is a parse error.
    Alternative,
    /// `match (...) {`. `open` is that `{`.
    Match { open: usize },
    /// `{` after `=>`. PHP rejects that brace, so a halt inside it is not this precheck's error.
    ArrowBlock,
    /// A computed member brace: `::{`, `->{` or `?->{`.
    Member,
    /// `switch (...) {` or `switch (...):`. `gate` is that `{` or `:`.
    ///
    /// A halt immediately after `gate` is a parse error. A halt after `case` or `default`
    /// is the outermost-scope CompileError. `alternative` selects the `endswitch` wording.
    Switch { gate: usize, alternative: bool },
}

/// Returns whether `{` opens a trait adaptation block (`use Trait {`, `use A, B {`).
///
/// A closure's `use ($a) {` stops at the parameter list and stays a statement body.
fn brace_is_trait_use<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    let mut cursor = open;
    let mut saw_name = false;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        if id == tok::T_USE {
            return saw_name;
        }
        if id == tok::T_STRING
            || id == tok::T_NAME_FULLY_QUALIFIED
            || id == tok::T_NAME_QUALIFIED
            || id == tok::T_NAME_RELATIVE
            || id == tok::T_NS_SEPARATOR
            || id == i64::from(b'\\')
        {
            saw_name = true;
            cursor = previous;
            continue;
        }
        if id == i64::from(b',') {
            cursor = previous;
            continue;
        }
        return false;
    }
    false
}

/// One delimiter step while a halt scan walks backward.
enum BackwardDelimiter {
    /// A closer, or the opener that matches one. The caller stays inside that group.
    Nested,
    /// `(`, `[` or `{` with no closer still open.
    Surface(i64),
}

/// Updates `depth` for one token in a backward delimiter walk.
///
/// `)`, `]` and `}` increase depth. `(`, `[` and `{` decrease it. `T_CURLY_OPEN` and
/// `T_DOLLAR_OPEN_CURLY_BRACES` decrease it only after a `}` raised it: those tokens open
/// an interpolation that ends with `}`. A real `{` at depth zero stays a block for the caller.
fn backward_delimiter(id: i64, depth: &mut i32) -> Option<BackwardDelimiter> {
    if id == i64::from(b')') || id == i64::from(b']') || id == i64::from(b'}') {
        *depth += 1;
        return Some(BackwardDelimiter::Nested);
    }
    if is_halt_brace_open(id) && id != i64::from(b'{') {
        if *depth > 0 {
            *depth -= 1;
            return Some(BackwardDelimiter::Nested);
        }
        return None;
    }
    if id == i64::from(b'(') || id == i64::from(b'[') || id == i64::from(b'{') {
        if *depth == 0 {
            return Some(BackwardDelimiter::Surface(id));
        }
        *depth -= 1;
        return Some(BackwardDelimiter::Nested);
    }
    None
}

/// Classifies a real `{` as a statement block, a class-like body or a namespace body.
///
/// The namespace line is filled by the caller from the brace token. A property-hook container
/// (`public $x {`) is not a statement list; the `get {` body inside it is. `match`, `switch`
/// and a computed member stop at their own header, so an enclosing function does not win.
/// Parentheses, nested braces and interpolation braces in that header are skipped, and
/// `Foo::class` is not a class declaration. `{` after `=>` is not a statement block either:
/// PHP reports that brace before the halt. A `}` at depth zero closes an earlier sibling
/// statement, so this brace does not inherit that sibling's class or switch.
fn halt_brace_kind<T: SourceToken>(tokens: &[T], open: usize) -> HaltScope {
    if let Some(previous) = previous_significant_index(tokens, open) {
        let id = tokens[previous].id();
        if id == tok::T_DOUBLE_COLON
            || id == tok::T_OBJECT_OPERATOR
            || id == tok::T_NULLSAFE_OBJECT_OPERATOR
        {
            return HaltScope::Member;
        }
        if id == tok::T_DOUBLE_ARROW {
            return HaltScope::ArrowBlock;
        }
    }
    if brace_is_trait_use(tokens, open)
        || previous_significant_index(tokens, open)
            .is_some_and(|previous| tokens[previous].id() == tok::T_VARIABLE)
    {
        return HaltScope::ClassLike;
    }
    let mut depth = 0i32;
    let mut cursor = open;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        // Crossing that sibling would label this block as the earlier class or switch.
        if depth == 0 && id == i64::from(b'}') {
            return HaltScope::Statement;
        }
        if let Some(delimiter) = backward_delimiter(id, &mut depth) {
            match delimiter {
                BackwardDelimiter::Nested => {
                    cursor = previous;
                    continue;
                }
                BackwardDelimiter::Surface(surface) if surface == i64::from(b'{') => {
                    return HaltScope::Statement;
                }
                BackwardDelimiter::Surface(_) => {}
            }
        } else if depth > 0 {
            cursor = previous;
            continue;
        }
        if id == tok::T_NAMESPACE {
            return HaltScope::Namespace { line: 0 };
        }
        if id == tok::T_CLASS || id == tok::T_INTERFACE || id == tok::T_TRAIT || id == tok::T_ENUM {
            if control_keyword_is_name(tokens, previous) {
                cursor = previous;
                continue;
            }
            return HaltScope::ClassLike;
        }
        if id == tok::T_MATCH && !control_keyword_is_name(tokens, previous) {
            return HaltScope::Match { open };
        }
        if id == tok::T_SWITCH && !control_keyword_is_name(tokens, previous) {
            return HaltScope::Switch {
                gate: open,
                alternative: false,
            };
        }
        if id == tok::T_FUNCTION
            || id == tok::T_FN
            || id == i64::from(b';')
            || id == i64::from(b'{')
        {
            return HaltScope::Statement;
        }
        cursor = previous;
    }
    HaltScope::Statement
}

/// Returns whether `index` opens or closes one alternative-syntax construct.
///
/// `1` pushes a scope, `-1` pops the nearest one, and `0` leaves the stack alone. `else` and
/// `elseif` stay inside the `if` they continue, so only the outer header is pushed.
fn alternative_scope_delta<T: SourceToken>(tokens: &[T], index: usize) -> i8 {
    let id = tokens[index].id();
    let header = id == tok::T_IF
        || id == tok::T_WHILE
        || id == tok::T_FOR
        || id == tok::T_FOREACH
        || id == tok::T_SWITCH
        || id == tok::T_DECLARE;
    let end = id == tok::T_ENDIF
        || id == tok::T_ENDWHILE
        || id == tok::T_ENDFOR
        || id == tok::T_ENDFOREACH
        || id == tok::T_ENDSWITCH
        || id == tok::T_ENDDECLARE;
    if (!header && !end) || control_keyword_is_name(tokens, index) {
        return 0;
    }
    if header && alternative_header_colon(tokens, index) {
        return 1;
    }
    if end {
        return -1;
    }
    0
}

/// Returns whether `__halt_compiler` sits where PHP's grammar accepts a statement.
///
/// A ternary colon is an expression boundary, so `$a ? 1 : __halt_compiler()` stays a parse
/// error even inside a function. A case, label or alternative-syntax colon starts a statement.
/// A semicolon inside parentheses belongs to a `for` header or a bad expression, and a `}`
/// that closes a match, closure, anonymous class or computed member does not end the statement.
fn halt_is_statement_start<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let Some(previous) = previous_significant_index(tokens, index) else {
        return true;
    };
    let id = tokens[previous].id();
    if id == i64::from(b';') {
        return !semicolon_inside_parentheses(tokens, previous);
    }
    if id == i64::from(b'}') {
        return brace_ends_statement(tokens, previous);
    }
    if id == i64::from(b'{') || id == tok::T_OPEN_TAG {
        return true;
    }
    id == i64::from(b':') && !ternary_colon_before_label(tokens, index)
}

/// Returns whether `semi` sits inside parentheses rather than between statements.
///
/// `for ($i = 0; __halt_compiler()` keeps that semicolon inside the header. An interpolation
/// `}` does not close it. `echo 1;` does not sit inside parentheses.
fn semicolon_inside_parentheses<T: SourceToken>(tokens: &[T], semi: usize) -> bool {
    let mut depth = 0i32;
    let mut cursor = semi;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        if let Some(delimiter) = backward_delimiter(id, &mut depth) {
            if let BackwardDelimiter::Surface(surface) = delimiter {
                return surface == i64::from(b'(');
            }
        } else if depth == 0
            && (id == i64::from(b';') || id == i64::from(b'{') || id == i64::from(b'}'))
        {
            return false;
        }
        cursor = previous;
    }
    false
}

/// Returns whether `}` finishes a statement, so the next token can be a new one.
///
/// A match arm list, a closure expression, an anonymous class and a computed member still
/// need a semicolon. A function, class, control or switch body does not.
fn brace_ends_statement<T: SourceToken>(tokens: &[T], close: usize) -> bool {
    let Some(open) = matching_open_brace(tokens, close) else {
        return true;
    };
    if tokens[open].id() != i64::from(b'{') {
        return false;
    }
    match halt_brace_kind(tokens, open) {
        HaltScope::Match { .. } | HaltScope::ArrowBlock | HaltScope::Member => false,
        HaltScope::ClassLike => !anonymous_class_brace(tokens, open),
        HaltScope::Statement => !closure_expression_brace(tokens, open),
        HaltScope::Namespace { .. }
        | HaltScope::Switch { .. }
        | HaltScope::Interpolation
        | HaltScope::Alternative => true,
    }
}

/// Returns the `{` or interpolation opener that `close` ends.
fn matching_open_brace<T: SourceToken>(tokens: &[T], close: usize) -> Option<usize> {
    if tokens.get(close).map(|token| token.id()) != Some(i64::from(b'}')) {
        return None;
    }
    let mut depth = 0i32;
    let mut cursor = close;
    while cursor > 0 {
        cursor -= 1;
        let id = tokens[cursor].id();
        if id == i64::from(b'}') {
            depth += 1;
            continue;
        }
        if is_halt_brace_open(id) {
            if depth == 0 {
                return Some(cursor);
            }
            depth -= 1;
        }
    }
    None
}

/// Returns whether `open` is the body of `new class`, including `extends` and arguments.
fn anonymous_class_brace<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    let mut depth = 0i32;
    let mut cursor = open;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        if let Some(delimiter) = backward_delimiter(id, &mut depth) {
            match delimiter {
                BackwardDelimiter::Surface(_) => return false,
                BackwardDelimiter::Nested => {
                    cursor = previous;
                    continue;
                }
            }
        }
        if depth > 0 {
            cursor = previous;
            continue;
        }
        if depth == 0 && id == tok::T_CLASS && !control_keyword_is_name(tokens, previous) {
            return previous_significant_index(tokens, previous)
                .is_some_and(|before| tokens[before].id() == tok::T_NEW);
        }
        if depth == 0 && (id == i64::from(b';') || id == i64::from(b'{') || id == tok::T_FUNCTION) {
            return false;
        }
        cursor = previous;
    }
    false
}

/// Returns whether `open` is a closure expression rather than a function declaration.
///
/// `function () use ($a) {}` and `static function () {}` after `=` stay expressions.
/// `function g() {}` and `label: function g() {}` end at the brace.
fn closure_expression_brace<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    let mut depth = 0i32;
    let mut cursor = open;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        if let Some(delimiter) = backward_delimiter(id, &mut depth) {
            match delimiter {
                BackwardDelimiter::Surface(_) => return false,
                BackwardDelimiter::Nested => {
                    cursor = previous;
                    continue;
                }
            }
        }
        if depth > 0 {
            cursor = previous;
            continue;
        }
        if depth == 0 && id == tok::T_FUNCTION {
            return !function_keyword_is_declaration(tokens, previous);
        }
        if depth == 0
            && (id == i64::from(b';')
                || id == i64::from(b'{')
                || id == tok::T_IF
                || id == tok::T_WHILE
                || id == tok::T_FOR
                || id == tok::T_FOREACH
                || id == tok::T_SWITCH)
        {
            return false;
        }
        cursor = previous;
    }
    false
}

/// Returns whether `function` at `index` declares a function or method.
///
/// A label colon does. A ternary colon does not: `$a ? function () {}` is an expression.
fn function_keyword_is_declaration<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let Some(previous) = previous_significant_index(tokens, index) else {
        return true;
    };
    if token_ends_statement(tokens, previous) {
        return true;
    }
    if is_declaration_modifier(tokens[previous].id()) {
        return modifier_starts_declaration(tokens, previous);
    }
    false
}

/// Returns whether a visibility, `static`, `abstract` or `final` starts a declaration.
fn modifier_starts_declaration<T: SourceToken>(tokens: &[T], modifier: usize) -> bool {
    let Some(previous) = previous_significant_index(tokens, modifier) else {
        return true;
    };
    if token_ends_statement(tokens, previous) || is_declaration_modifier(tokens[previous].id()) {
        return true;
    }
    false
}

/// Returns whether `index` is a semicolon, block edge, open tag or label colon.
fn token_ends_statement<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let id = tokens[index].id();
    if id == i64::from(b';') || id == i64::from(b'{') || id == i64::from(b'}') || id == tok::T_OPEN_TAG {
        return true;
    }
    id == i64::from(b':')
        && next_significant_index(tokens, index)
            .is_some_and(|next| !ternary_colon_before_label(tokens, next))
}

/// Returns whether `id` can introduce a method or a nested function declaration.
fn is_declaration_modifier(id: i64) -> bool {
    const T_FINAL: i64 = elephc_builtin_contract::token_const_id("T_FINAL");
    id == tok::T_PUBLIC
        || id == tok::T_PROTECTED
        || id == tok::T_PRIVATE
        || id == tok::T_STATIC
        || id == tok::T_ABSTRACT
        || id == T_FINAL
}

/// Returns the `for` header expectation when the halt starts one of its three clauses.
///
/// The init and condition clauses expect `;`. The update clause expects `)`. A semicolon
/// inside a call is not a `for` clause, so this returns none and the earlier error stands.
/// An interpolation brace inside a clause stays inside the header.
fn for_header_halt_expectation<T: SourceToken>(tokens: &[T], halt: usize) -> Option<&'static str> {
    let previous = previous_significant_index(tokens, halt)?;
    let previous_id = tokens[previous].id();
    if previous_id != i64::from(b';') && previous_id != i64::from(b'(') {
        return None;
    }
    let mut depth = 0i32;
    let mut semis = 0i32;
    let mut cursor = halt;
    while let Some(before) = previous_significant_index(tokens, cursor) {
        let id = tokens[before].id();
        if let Some(delimiter) = backward_delimiter(id, &mut depth) {
            if let BackwardDelimiter::Surface(surface) = delimiter {
                if surface != i64::from(b'(') {
                    return None;
                }
                let keyword = previous_significant_index(tokens, before)?;
                if tokens[keyword].id() != tok::T_FOR || control_keyword_is_name(tokens, keyword) {
                    return None;
                }
                return Some(if semis >= 2 { "\")\"" } else { "\";\"" });
            }
        } else if depth == 0 && id == i64::from(b';') {
            semis += 1;
        }
        cursor = before;
    }
    None
}

/// Returns whether the halt word is a member or namespace name rather than the construct.
fn halt_used_as_name<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    previous_significant_index(tokens, index).is_some_and(|previous| {
        matches!(
            tokens[previous].id(),
            token_id if token_id == tok::T_OBJECT_OPERATOR
                || token_id == tok::T_NULLSAFE_OBJECT_OPERATOR
                || token_id == i64::from(b'\\')
                || token_id == tok::T_NS_SEPARATOR
        )
    })
}

/// Builds PHP's `Unclosed '{'` diagnostic for a halt that ends a braced namespace early.
fn halt_namespace_unclosed(source: &[u8], pos: usize, brace_line: usize) -> ValidationError {
    let halt_line = 1 + newline_count(&source[..pos.min(source.len())]);
    let message = if halt_line == brace_line {
        "Unclosed '{'".to_string()
    } else {
        format!("Unclosed '{{' on line {brace_line}")
    };
    ValidationError { message, pos, lexical: false }
}

/// Returns the innermost scope that decides a halt, ignoring string interpolation.
fn innermost_halt_scope(scopes: &[HaltScope]) -> Option<&HaltScope> {
    scopes.iter().rev().find(|scope| !matches!(scope, HaltScope::Interpolation))
}

/// How a halt in a `match` arm list is reported.
enum MatchHalt {
    /// An earlier `{` after `=>` is already the parse error.
    Defer,
    /// Unexpected token with no expectation list.
    Generic,
    /// Unexpected token with PHP's expectation text.
    Expect(&'static str),
}

/// Classifies a halt inside one `match` arm list.
///
/// A finished arm expects `}`, with or without a separating comma. A finished condition
/// expects `=>`. `Foo::class`, operand-less `exit`/`die`/`yield` finish an operand.
/// `new class` still expects `{` through `extends` and `implements`. A halt after `=>`,
/// after an operator, or inside `()` or `[]` stays generic: call and array expectation
/// lists belong to the parser, and this precheck must not replace them with the arm-list `}`.
fn match_halt_decision<T: SourceToken>(tokens: &[T], open: usize, halt: usize) -> MatchHalt {
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut after_arrow = false;
    let mut comma_closed_arm = false;
    let mut deferred = false;
    let mut anon_header = false;
    let mut previous_significant: Option<i64> = None;
    for (offset, token) in tokens[open + 1..halt].iter().enumerate() {
        let token_index = open + 1 + offset;
        let id = token.id();
        if is_trivia_token(id) {
            continue;
        }
        if id == i64::from(b'(') {
            paren += 1;
        } else if id == i64::from(b')') {
            paren -= 1;
        } else if id == i64::from(b'[') {
            bracket += 1;
        } else if id == i64::from(b']') {
            bracket -= 1;
        } else if is_halt_brace_open(id) {
            if paren == 0 && bracket == 0 && brace == 0 {
                if id == i64::from(b'{') && previous_significant == Some(tok::T_DOUBLE_ARROW) {
                    deferred = true;
                }
                anon_header = false;
            }
            brace += 1;
        } else if id == i64::from(b'}') {
            brace -= 1;
        } else if paren == 0 && bracket == 0 && brace == 0 && id == tok::T_DOUBLE_ARROW {
            if !double_arrow_is_fn_arrow(tokens, token_index) {
                after_arrow = true;
                comma_closed_arm = false;
            }
        } else if paren == 0 && bracket == 0 && brace == 0 && id == i64::from(b',') {
            comma_closed_arm = after_arrow;
            if after_arrow {
                after_arrow = false;
            }
        } else if paren == 0 && bracket == 0 && brace == 0 && id == tok::T_CLASS {
            if previous_significant == Some(tok::T_NEW) {
                anon_header = true;
            }
        }
        previous_significant = Some(id);
    }
    if deferred {
        return MatchHalt::Defer;
    }
    if paren > 0 || bracket > 0 || brace > 0 {
        return MatchHalt::Generic;
    }
    if anon_header {
        return MatchHalt::Expect("\"{\"");
    }
    let Some(previous) = previous_significant_index(tokens, halt) else {
        return MatchHalt::Generic;
    };
    if previous == open {
        return MatchHalt::Expect("\"}\"");
    }
    let previous_id = tokens[previous].id();
    if previous_id == tok::T_DOUBLE_ARROW {
        return MatchHalt::Generic;
    }
    if previous_id == i64::from(b',') {
        return MatchHalt::Expect(if comma_closed_arm { "\"}\"" } else { "\"=>\"" });
    }
    if scoped_constant_name(tokens, previous) {
        return MatchHalt::Expect(if after_arrow { "\"}\"" } else { "\"=>\"" });
    }
    if is_postfix_operator(previous_id) && !match_operand_complete(tokens, previous) {
        if postfix_operator_follows_operand(tokens, previous) {
            return MatchHalt::Defer;
        }
    }
    if previous_id == tok::T_STATIC && !match_operand_complete(tokens, previous) {
        return MatchHalt::Expect("\"::\"");
    }
    if previous_id == tok::T_NEW {
        return MatchHalt::Expect("\"class\"");
    }
    if match_keyword_needs_paren(previous_id) {
        return MatchHalt::Expect("\"(\"");
    }
    if previous_id == tok::T_CLASS && !match_operand_complete(tokens, previous) {
        return MatchHalt::Defer;
    }
    if match_operand_complete(tokens, previous) {
        return MatchHalt::Expect(if after_arrow { "\"}\"" } else { "\"=>\"" });
    }
    MatchHalt::Generic
}

/// Returns whether a keyword before the halt still needs its `(`.
///
/// `empty`, `isset`, `eval`, `array`, `list`, `fn` and a nested `match` are not finished
/// operands. `unset` is reported on the keyword itself, so it stays out of this list.
fn match_keyword_needs_paren(id: i64) -> bool {
    const T_EMPTY: i64 = elephc_builtin_contract::token_const_id("T_EMPTY");
    const T_ISSET: i64 = elephc_builtin_contract::token_const_id("T_ISSET");
    const T_EVAL: i64 = elephc_builtin_contract::token_const_id("T_EVAL");
    const T_ARRAY: i64 = elephc_builtin_contract::token_const_id("T_ARRAY");
    id == T_EMPTY
        || id == T_ISSET
        || id == T_EVAL
        || id == T_ARRAY
        || id == tok::T_FN
        || id == tok::T_MATCH
        || id == tok::T_LIST
}

/// Returns whether the token at `index` finishes a match condition or arm operand.
///
/// `T_CLASS` counts only for `::class`. An anonymous `new class` is still open. Operand-less
/// `exit`, `die` and `yield` are finished. Operators and keywords that still take an operand
/// stay open, so the halt keeps the generic message instead of an arm-list expectation.
fn match_operand_complete<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let id = tokens[index].id();
    if is_postfix_operator(id) {
        return previous_significant_index(tokens, index)
            .is_some_and(|previous| postfix_lvalue(tokens, previous));
    }
    if id == tok::T_STATIC {
        const T_INSTANCEOF: i64 = elephc_builtin_contract::token_const_id("T_INSTANCEOF");
        return previous_significant_index(tokens, index).is_some_and(|previous| {
            let previous_id = tokens[previous].id();
            previous_id == tok::T_NEW || previous_id == T_INSTANCEOF
        });
    }
    if id == tok::T_CLASS {
        return previous_significant_index(tokens, index)
            .is_some_and(|previous| tokens[previous].id() == tok::T_DOUBLE_COLON);
    }
    if id == tok::T_EXIT || id == tok::T_YIELD {
        return true;
    }
    const T_DEFAULT: i64 = elephc_builtin_contract::token_const_id("T_DEFAULT");
    const T_FILE: i64 = elephc_builtin_contract::token_const_id("T_FILE");
    const T_LINE: i64 = elephc_builtin_contract::token_const_id("T_LINE");
    const T_DIR: i64 = elephc_builtin_contract::token_const_id("T_DIR");
    const T_CLASS_C: i64 = elephc_builtin_contract::token_const_id("T_CLASS_C");
    const T_FUNC_C: i64 = elephc_builtin_contract::token_const_id("T_FUNC_C");
    const T_METHOD_C: i64 = elephc_builtin_contract::token_const_id("T_METHOD_C");
    const T_NS_C: i64 = elephc_builtin_contract::token_const_id("T_NS_C");
    const T_TRAIT_C: i64 = elephc_builtin_contract::token_const_id("T_TRAIT_C");
    const T_PROPERTY_C: i64 = elephc_builtin_contract::token_const_id("T_PROPERTY_C");
    id == i64::from(b')')
        || id == i64::from(b']')
        || id == i64::from(b'}')
        || id == tok::T_LNUMBER
        || id == tok::T_DNUMBER
        || id == tok::T_STRING
        || id == tok::T_VARIABLE
        || id == tok::T_CONSTANT_ENCAPSED_STRING
        || id == tok::T_NAME_FULLY_QUALIFIED
        || id == tok::T_NAME_QUALIFIED
        || id == tok::T_NAME_RELATIVE
        || id == T_DEFAULT
        || id == T_FILE
        || id == T_LINE
        || id == T_DIR
        || id == T_CLASS_C
        || id == T_FUNC_C
        || id == T_METHOD_C
        || id == T_NS_C
        || id == T_TRAIT_C
        || id == T_PROPERTY_C
}

/// Reports a switch halt, or defers when a real `case` or `default` has not started.
///
/// Inline HTML and other tokens before the first label belong to `switch_case_html_error`
/// or the parser. A halt after a label is the outermost-scope `CompileError` when the call
/// is `()`. An argument, including a cast token in place of `(`, is a parse error there.
fn switch_halt_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    gate: usize,
    alternative: bool,
    index: usize,
) -> Option<ValidationError> {
    let at_gate = previous_significant_index(tokens, index) == Some(gate);
    if at_gate {
        let expecting = if alternative {
            "\"endswitch\" or \"case\" or \"default\""
        } else {
            "\"case\" or \"default\" or \"}\""
        };
        return Some(halt_parse_error(source, tokens, index, Some(expecting)));
    }
    if !switch_saw_label(tokens, gate, index) {
        return None;
    }
    if let Some(error) = halt_argument_error(source, tokens, index) {
        return Some(error);
    }
    Some(ValidationError {
        message: HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE.into(),
        pos: tokens[index].pos(),
        lexical: false,
    })
}

/// Returns the parse error for a malformed `__halt_compiler` argument.
///
/// `()` with whitespace or a comment is still an empty call and returns none. A token
/// after `(` is reported there. `(array)` is one cast token, so it is reported at that
/// token with `expecting "("`. A heredoc argument is reported on its first body line.
fn halt_argument_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    halt: usize,
) -> Option<ValidationError> {
    let next = next_significant_index(tokens, halt)?;
    if tokens[next].id() != i64::from(b'(') {
        if tokens[next].id() == tok::T_START_HEREDOC {
            return Some(halt_heredoc_argument_error(source, &tokens[next], "\"(\""));
        }
        return Some(token_noun_error(source, tokens, next, "\"(\""));
    }
    let inner = next_significant_index(tokens, next)?;
    if tokens[inner].id() == i64::from(b')') {
        return None;
    }
    if tokens[inner].id() == tok::T_START_HEREDOC {
        return Some(halt_heredoc_argument_error(source, &tokens[inner], "\")\""));
    }
    Some(token_noun_error(source, tokens, inner, "\")\""))
}

/// Builds the halt-argument diagnostic for a heredoc or nowdoc start.
///
/// PHP names both `heredoc start`. The header keeps a carriage return and drops one trailing
/// quote. The position is still the first body line, so it ties with the scanner inside an
/// open `{`.
fn halt_heredoc_argument_error<T: SourceToken>(
    source: &[u8],
    token: &T,
    expecting: &str,
) -> ValidationError {
    let text = &source[token.pos()..token.pos() + token.len()];
    let displayed = crate::scanner::heredoc_start_header(text);
    let line_header = text
        .split(|byte| *byte == b'\n' || *byte == b'\r')
        .next()
        .unwrap_or(text);
    let mut error_pos = token.pos() + line_header.len();
    if text.get(line_header.len()) == Some(&b'\r') && text.get(line_header.len() + 1) == Some(&b'\n')
    {
        error_pos += 2;
    } else if text.get(line_header.len()).is_some() {
        error_pos += 1;
    }
    ValidationError {
        message: format!(
            "syntax error, unexpected heredoc start \"{}\", expecting {expecting}",
            String::from_utf8_lossy(displayed)
        ),
        pos: error_pos.min(token.pos() + token.len()),
        lexical: false,
    }
}

/// Returns whether this switch already opened a `case` or `default` before the halt.
///
/// Labels inside a nested brace, call or array do not count. A `default` arm of a `match`
/// nested in the switch is not a switch label.
fn switch_saw_label<T: SourceToken>(tokens: &[T], gate: usize, halt: usize) -> bool {
    const T_DEFAULT: i64 = elephc_builtin_contract::token_const_id("T_DEFAULT");
    let mut depth = 0i32;
    for token in &tokens[gate + 1..halt] {
        let id = token.id();
        if is_trivia_token(id) {
            continue;
        }
        if is_halt_brace_open(id) || id == i64::from(b'(') || id == i64::from(b'[') {
            depth += 1;
            continue;
        }
        if id == i64::from(b'}') || id == i64::from(b')') || id == i64::from(b']') {
            depth -= 1;
            continue;
        }
        if depth == 0 && (id == tok::T_CASE || id == T_DEFAULT) {
            return true;
        }
    }
    false
}

/// Returns whether `id` is PHP's postfix `++` or `--` token.
fn is_postfix_operator(id: i64) -> bool {
    const T_INC: i64 = elephc_builtin_contract::token_const_id("T_INC");
    const T_DEC: i64 = elephc_builtin_contract::token_const_id("T_DEC");
    id == T_INC || id == T_DEC
}

/// Returns whether `++` or `--` follows an operand and is therefore not a prefix operator.
fn postfix_operator_follows_operand<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    previous_significant_index(tokens, index)
        .is_some_and(|previous| match_operand_complete(tokens, previous))
}

/// Returns whether `index` is a value PHP allows before postfix `++` or `--`.
///
/// A variable, an array close, or a property or static name qualifies. A parenthesis or a
/// literal does not: PHP reports the operator itself.
fn postfix_lvalue<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let id = tokens[index].id();
    if id == tok::T_VARIABLE || id == i64::from(b']') {
        return true;
    }
    let name = id == tok::T_STRING
        || id == tok::T_NAME_FULLY_QUALIFIED
        || id == tok::T_NAME_QUALIFIED
        || id == tok::T_NAME_RELATIVE;
    name && previous_significant_index(tokens, index).is_some_and(|previous| {
        let previous_id = tokens[previous].id();
        previous_id == tok::T_OBJECT_OPERATOR
            || previous_id == tok::T_NULLSAFE_OBJECT_OPERATOR
            || previous_id == tok::T_DOUBLE_COLON
    })
}

/// Returns whether `=>` at `index` belongs to an arrow function rather than a match arm.
///
/// `fn() => 1` and `fn(): int => 1` leave the match condition unfinished. The arm separator
/// before that arrow function stays in force. An interpolation in the parameter list stays
/// inside the arrow function.
fn double_arrow_is_fn_arrow<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    let mut depth = 0i32;
    let mut saw_paren = false;
    let mut cursor = index;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        if let Some(delimiter) = backward_delimiter(id, &mut depth) {
            match delimiter {
                BackwardDelimiter::Surface(_) => return false,
                BackwardDelimiter::Nested => {
                    if id == i64::from(b'(') && depth == 0 {
                        saw_paren = true;
                    }
                }
            }
        } else if depth == 0 && id == tok::T_FN {
            return saw_paren;
        } else if depth == 0
            && (id == i64::from(b';')
                || id == i64::from(b',')
                || id == tok::T_DOUBLE_ARROW
                || id == tok::T_MATCH)
        {
            return false;
        }
        cursor = previous;
    }
    false
}

/// Returns whether `index` is a name written after `::`, including semi-reserved keywords.
///
/// `C::match`, `C::new` and `C::static` are constant fetches. They finish a match operand.
fn scoped_constant_name<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    previous_significant_index(tokens, index)
        .is_some_and(|previous| tokens[previous].id() == tok::T_DOUBLE_COLON)
}

/// Builds the parse or compile diagnostic for one `__halt_compiler` token.
///
/// `None` means this halt is not a precheck error. That is the outermost scope and a `{`
/// after `=>`. A malformed argument, including a cast token in place of `(`, is a parse
/// error from `halt_argument_error`. PHP reports the brace for an arrow block, and a later
/// halt must not replace it.
/// A class body, match arm list, computed member and a switch with no `case` yet stay parse
/// errors even when a function around them is still open. The expectation text is PHP's for
/// that container.
fn halt_scope_failure<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    scopes: &[HaltScope],
    index: usize,
) -> Option<ValidationError> {
    let statement = halt_is_statement_start(tokens, index);
    let scope = innermost_halt_scope(scopes);
    if matches!(scope, Some(HaltScope::ArrowBlock)) {
        return None;
    }
    if matches!(
        scope,
        None | Some(HaltScope::Statement | HaltScope::Alternative)
    ) {
        if let Some(expecting) = for_header_halt_expectation(tokens, index) {
            return Some(halt_parse_error(source, tokens, index, Some(expecting)));
        }
    }
    if statement {
        match scope {
            None => return None,
            Some(HaltScope::Statement | HaltScope::Alternative) => {
                if let Some(error) = halt_argument_error(source, tokens, index) {
                    return Some(error);
                }
                return Some(ValidationError {
                    message: HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE.into(),
                    pos: tokens[index].pos(),
                    lexical: false,
                });
            }
            Some(HaltScope::Namespace { line }) => {
                if let Some(error) = halt_argument_error(source, tokens, index) {
                    return Some(error);
                }
                return Some(halt_namespace_unclosed(source, tokens[index].pos(), *line));
            }
            Some(HaltScope::Switch { gate, alternative }) => {
                return switch_halt_error(source, tokens, *gate, *alternative, index);
            }
            Some(
                HaltScope::ClassLike
                | HaltScope::Member
                | HaltScope::Interpolation
                | HaltScope::ArrowBlock
                | HaltScope::Match { .. },
            ) => {}
        }
    }
    if let Some(HaltScope::Switch { gate, .. }) = scope {
        if !switch_saw_label(tokens, *gate, index) {
            return None;
        }
    }
    if let Some(HaltScope::Match { open }) = scope {
        return match match_halt_decision(tokens, *open, index) {
            MatchHalt::Defer => None,
            MatchHalt::Generic => Some(halt_parse_error(source, tokens, index, None)),
            MatchHalt::Expect(expecting) => {
                Some(halt_parse_error(source, tokens, index, Some(expecting)))
            }
        };
    }
    Some(halt_parse_error(source, tokens, index, None))
}

/// Builds the unexpected-token diagnostic for a halt that is not a compile error.
///
/// The keyword spelling follows Zend, so `__HALT_COMPILER` is reported in lowercase. An
/// expectation is added only for the containers whose PHP message names one.
fn halt_parse_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    index: usize,
    expecting: Option<&str>,
) -> ValidationError {
    let Some(expecting) = expecting else {
        return token_at_index_error(source, tokens, index);
    };
    let token = &tokens[index];
    let text = &source[token.pos()..token.pos() + token.len()];
    let spelled = crate::scanner::keyword_diagnostic_spelling(token.id(), text)
        .unwrap_or_else(|| String::from_utf8_lossy(text).into_owned());
    ValidationError {
        message: format!("syntax error, unexpected token \"{spelled}\", expecting {expecting}"),
        pos: token.pos(),
        lexical: false,
    }
}

/// Rejects `__halt_compiler()` outside the outermost file scope.
///
/// The innermost non-interpolation scope decides. A statement inside a function, block or
/// alternative-syntax body is PHP's `CompileError`. A braced namespace halts before its `}`
/// and reports `Unclosed '{'`. A class body, match list, computed member, switch before its
/// first case, unbraced `if` and property initializer stay ordinary parse errors.
#[inline(never)]
pub(crate) fn halt_compiler_scope_error<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
) -> Option<ValidationError> {
    if !tokens.iter().any(|token| token.id() == tok::T_HALT_COMPILER) {
        return None;
    }
    let mut scopes: Vec<HaltScope> = Vec::new();
    for index in 0..tokens.len() {
        let id = tokens[index].id();
        if id == i64::from(b'{') {
            let mut scope = halt_brace_kind(tokens, index);
            if let HaltScope::Namespace { line } = &mut scope {
                *line = 1 + newline_count(&source[..tokens[index].pos().min(source.len())]);
            }
            scopes.push(scope);
            continue;
        }
        if id == tok::T_CURLY_OPEN || id == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            scopes.push(HaltScope::Interpolation);
            continue;
        }
        if id == i64::from(b'}') {
            scopes.pop();
            continue;
        }
        match alternative_scope_delta(tokens, index) {
            1 if id == tok::T_SWITCH => {
                if let Some(gate) = alternative_header_colon_index(tokens, index) {
                    scopes.push(HaltScope::Switch { gate, alternative: true });
                }
                continue;
            }
            1 => {
                scopes.push(HaltScope::Alternative);
                continue;
            }
            -1 if id == tok::T_ENDSWITCH => {
                if let Some(position) = scopes.iter().rposition(|scope| {
                    matches!(scope, HaltScope::Switch { alternative: true, .. })
                }) {
                    scopes.remove(position);
                }
                continue;
            }
            -1 => {
                if let Some(position) = scopes
                    .iter()
                    .rposition(|scope| matches!(scope, HaltScope::Alternative))
                {
                    scopes.remove(position);
                }
                continue;
            }
            _ => {}
        }
        if id != tok::T_HALT_COMPILER || halt_used_as_name(tokens, index) {
            continue;
        }
        if let Some(error) = halt_scope_failure(source, tokens, &scopes, index) {
            return Some(error);
        }
    }
    None
}

/// Rejects `list` where PHP's grammar reserves it, while keeping member/declaration uses.
///
/// elephc's lexer keeps `list` as an identifier, so the parser accepts it in expression and
/// declaration positions where reference PHP reports a ParseError. The allowed contexts below
/// are exactly the semi-reserved ones: member names, method/class-constant names, enum cases,
/// named arguments, trait adaptations, and the `list(...)` construct.
pub(crate) fn reserved_list_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    let list = tok::T_LIST;
    if !tokens.iter().any(|token| token.id() == list) {
        return None;
    }
    let enclosing = enclosing_braces(tokens);
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != list {
            continue;
        }
        let previous = previous_significant_index(tokens, index);
        let next = next_significant_index(tokens, index);
        let previous_id = previous.map_or(0, |index| tokens[index].id());
        let next_id = next.map_or(0, |index| tokens[index].id());
        let in_class = enclosing[index] != u32::MAX
            && enclosing_brace_is_class(tokens, enclosing[index] as usize);
        let allowed = if next_id == i64::from(b'(') {
            (previous_id != tok::T_FUNCTION || in_class) && previous_id != tok::T_NEW
        } else if previous_id == tok::T_FUNCTION {
            in_class
        } else if previous_id == tok::T_CONST {
            in_class
        } else if previous_id == tok::T_CASE {
            in_class
        } else if next_id == i64::from(b':') {
            true
        } else if [
            tok::T_OBJECT_OPERATOR,
            tok::T_NULLSAFE_OBJECT_OPERATOR,
            tok::T_DOUBLE_COLON,
        ]
        .contains(&previous_id)
        {
            true
        } else {
            next_id == tok::T_AS
        };
        if !allowed {
            return Some(token_at_index_error(source, tokens, index));
        }
    }
    None
}

/// Rejects a `(void)` cast outside a statement position, with PHP's own wording.
pub(crate) fn void_cast_expression_error<T: SourceToken>(tokens: &[T]) -> Option<ValidationError> {
    for index in 0..tokens.len() {
        if tokens[index].id() != tok::T_VOID_CAST {
            continue;
        }
        let previous = previous_significant_index(tokens, index)
            .map_or(0, |before| tokens[before].id());
        if void_cast_starts_statement(tokens, index, previous) {
            continue;
        }
        let expecting = if previous == tok::T_RETURN { ", expecting \";\"" } else { "" };
        return Some(ValidationError {
            message: format!("syntax error, unexpected token \"(void)\"{expecting}"),
            pos: tokens[index].pos(),
            lexical: false,
        });
    }
    None
}

/// Rejects inline HTML between a `switch` opener and its first case label.
///
/// The parser view blanks inline HTML, so `switch ($a): ?>\n    <?php case 1:` would otherwise
/// look like a close tag directly before `case`. PHP reports the HTML token itself.
pub(crate) fn switch_case_html_error<T: SourceToken>(source: &[u8], tokens: &[T]) -> Option<ValidationError> {
    for (index, token) in tokens.iter().enumerate() {
        if token.id() != tok::T_SWITCH {
            continue;
        }
        let Some(open) = next_significant_index(tokens, index)
            .filter(|&next| tokens[next].id() == i64::from(b'('))
        else {
            continue;
        };
        let mut depth = 0usize;
        let mut cursor = open;
        while cursor < tokens.len() {
            match tokens[cursor].id() {
                id if id == i64::from(b'(') => depth += 1,
                id if id == i64::from(b')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        let Some(opener) = next_significant_index(tokens, cursor) else {
            continue;
        };
        let alternative = tokens[opener].id() == i64::from(b':');
        if !alternative && tokens[opener].id() != i64::from(b'{') {
            continue;
        }
        let Some(close) = next_significant_index(tokens, opener) else {
            continue;
        };
        if tokens[close].id() != tok::T_CLOSE_TAG {
            continue;
        }
        let Some(html) = next_significant_index(tokens, close) else {
            continue;
        };
        if tokens[html].id() != tok::T_INLINE_HTML {
            continue;
        }
        let text = &source[tokens[html].pos()..tokens[html].pos() + tokens[html].len()];
        let expecting = if alternative {
            "\"endswitch\" or \"case\" or \"default\""
        } else {
            "\"case\" or \"default\" or \"}\""
        };
        return Some(ValidationError {
            message: format!(
                "syntax error, unexpected T_INLINE_HTML \"{}\", expecting {expecting}",
                String::from_utf8_lossy(text).replace('\n', " ").replace('\r', "")
            ),
            pos: tokens[html].pos(),
            lexical: false,
        });
    }
    None
}
