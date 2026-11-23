//! Purpose:
//! Builds the byte-preserving parser view and normalises PHP-only constructs for the AST parser.
//!
//! Called from:
//! - `crate::syntax::tokenizer_validation::validate()` after the lexical prechecks.
//!
//! Key details:
//! - Every view byte keeps a source offset so diagnostics and identifier feedback stay exact.
//! - Inline HTML, tags, casts and interpolation are rewritten without changing line numbers.

use super::*;

/// Builds the PHP-only parser view, its source map and any error in a normalized reference.
pub(super) fn parser_view<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    end_pos: usize,
    reference_delimiters: &OnceCell<Vec<Option<usize>>>,
) -> (String, Vec<u32>, Option<ReferenceError>) {
    let mut view = Vec::with_capacity(source.len() + 8);
    let mut map = Vec::with_capacity(source.len() + 8);
    append_mapped(&mut view, &mut map, b"<?php ", 0);
    let mut index = 0;
    let mut previous_significant = 0i64;
    // Track enclosing delimiters in one pass so a by-reference `&` never rescans the prefix.
    let mut brackets: Vec<usize> = Vec::new();
    let mut parens: Vec<usize> = Vec::new();
    let mut paren_defaults: Vec<Option<(bool, Option<usize>)>> = Vec::new();
    let mut declaration_default_index: Option<usize> = None;
    let mut braces: Vec<usize> = Vec::new();
    // Whether each open `(` is a control header (`if`, `while`, ...) whose closing `)` can be
    // followed by a single-statement body, and which closing tokens proved that.
    let mut control_parens: Vec<bool> = Vec::new();
    let mut control_close = vec![false; tokens.len()];
    let mut reference_error: Option<ReferenceError> = None;
    while index < tokens.len() {
        let token = &tokens[index];
        match token.id() {
            id if id == i64::from(b'(') => {
                parens.push(index);
                control_parens.push(is_control_header_keyword(previous_significant));
                let declaration = paren_is_declaration_parameters(tokens, index);
                let previous = declaration.then_some(declaration_default_index).flatten();
                paren_defaults.push(declaration.then_some((false, previous)));
                if declaration {
                    declaration_default_index = Some(paren_defaults.len() - 1);
                }
            }
            id if id == i64::from(b')') => {
                parens.pop();
                control_close[index] = control_parens.pop().unwrap_or(false);
                if let Some(Some((_, previous))) = paren_defaults.pop() {
                    declaration_default_index = previous;
                }
            }
            id if id == i64::from(b'=') || id == i64::from(b',') => {
                if let Some(declaration) = declaration_default_index {
                    let open = parens[declaration];
                    if declaration + 1 == paren_defaults.len()
                        && brackets.last().is_none_or(|bracket| *bracket < open)
                        && braces.last().is_none_or(|brace| *brace < open)
                    {
                        if let Some(Some((started, _))) = paren_defaults.get_mut(declaration) {
                            *started = id == i64::from(b'=');
                        }
                    }
                }
            }
            id if id == i64::from(b'[') || id == tok::T_ATTRIBUTE => brackets.push(index),
            id if id == i64::from(b']') => {
                brackets.pop();
            }
            id if id == i64::from(b'{') => braces.push(index),
            id if id == i64::from(b'}') => {
                braces.pop();
            }
            _ => {}
        }
        let text = &source[token.pos()..token.pos() + token.len()];
        if is_reference_ampersand(
            tokens,
            index,
            previous_significant,
            brackets.last().copied(),
            parens.last().copied(),
            braces.last().copied(),
            |arrow| {
                if arrow.is_some_and(|arrow| {
                    arrow_starts_function_body(
                        tokens,
                        arrow,
                        reference_delimiters.get_or_init(|| delimiter_pairs(tokens)),
                    )
                }) {
                    return false;
                }
                let Some(target) = next_significant_index(tokens, index) else { return false; };
                let target_id = tokens[target].id();
                if target_id != i64::from(b'(') && target_id != tok::T_READONLY {
                    return reference_target_starts_at(tokens, target);
                }
                if reference_error.as_ref().is_none_or(|error| token.pos() < error.diagnostic.pos) {
                    let error = normalized_reference_error(
                        source,
                        tokens,
                        target,
                        reference_delimiters.get_or_init(|| delimiter_pairs(tokens)),
                        previous_significant == i64::from(b'='),
                        end_pos,
                    );
                    if let Some(error) = error {
                        if reference_error.as_ref().is_none_or(|previous| {
                            error.diagnostic.pos < previous.diagnostic.pos
                        }) {
                            reference_error = Some(error);
                        }
                    }
                }
                // Parse the ordinary expression to keep earlier syntax errors and identifier
                // feedback. The reference-specific error retains its own source position.
                true
            },
        ) {
            append_mapped(&mut view, &mut map, b" ", token.pos());
            index += 1;
            continue;
        }
        // Zend emits T_GOTO for the keyword, but PHP also allows `goto` as a member,
        // constant, enum-case and named-argument name. A real goto statement is the
        // shape `goto <label> ;` (a close tag also terminates it).
        if token.id() == tok::T_GOTO {
            let terminator = next_significant_index(tokens, index)
                .filter(|&label| tokens[label].id() == tok::T_STRING)
                .and_then(|label| next_significant_index(tokens, label))
                .filter(|&end| {
                    tokens[end].id() == i64::from(b';') || tokens[end].id() == tok::T_CLOSE_TAG
                });
            if let Some(terminator) = terminator {
                // Blank only the statement itself, stopping at its own terminator. Scanning
                // ahead to the next `;` would swallow a close tag and every later token up to
                // an unrelated semicolon, hiding syntax errors that Zend reports.
                for (offset, skipped) in tokens[index..=terminator].iter().enumerate() {
                    let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                    if index + offset == terminator {
                        // Keep an empty statement so an unbraced control body does not vanish:
                        // `if ($a) goto done; else ...` must still see a statement before `else`.
                        let mut normalized = text.to_vec();
                        for byte in &mut normalized {
                            if !matches!(*byte, b'\n' | b'\r') {
                                *byte = b' ';
                            }
                        }
                        if let Some(first) = normalized.first_mut() {
                            *first = b';';
                        }
                        append_mapped(&mut view, &mut map, &normalized, skipped.pos());
                    } else {
                        append_blank(&mut view, &mut map, text, skipped.pos());
                    }
                }
                index = terminator + 1;
                previous_significant = tokens[terminator].id();
                continue;
            }
        }
        let previous_closes_control_header = previous_significant == i64::from(b')')
            && previous_significant_index(tokens, index)
                .is_some_and(|previous| control_close.get(previous).copied().unwrap_or(false));
        if is_label_declaration(
            tokens,
            index,
            previous_significant,
            previous_closes_control_header,
        ) {
            // Keep an empty statement where the label was. Erasing it entirely makes
            // `if ($a) done: else other:` look like `if ($a) else`, a parse error.
            let text = &source[token.pos()..token.pos() + token.len()];
            let mut normalized = text.to_vec();
            for byte in &mut normalized {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
            if let Some(first) = normalized.first_mut() {
                *first = b';';
            }
            append_mapped(&mut view, &mut map, &normalized, token.pos());
            let colon = next_significant_index(tokens, index).expect("label colon follows label");
            append_blank(&mut view, &mut map, b":", tokens[colon].pos());
            index = colon + 1;
            continue;
        }
        let declaration_default_started = declaration_default_index
            .and_then(|declaration| paren_defaults.get(declaration))
            .is_some_and(|state| matches!(state, Some((true, _))));
        if token.id() == i64::from(b'$')
            && next_significant_index(tokens, index).is_some_and(|next| {
                let id = tokens[next].id();
                id == i64::from(b'$')
                    || id == tok::T_VARIABLE
                    || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
            })
            && (declaration_default_started
                || !dollar_starts_declaration(
                    tokens,
                    index,
                    previous_significant,
                    parens.last().copied(),
                ))
        {
            append_blank(&mut view, &mut map, b"$", token.pos());
            index += 1;
            continue;
        }
        if token.id() == i64::from(b'$')
            && next_significant_index(tokens, index).is_some_and(|next| tokens[next].id() == i64::from(b'{'))
            && (declaration_default_started
                || !dollar_starts_declaration(
                    tokens,
                    index,
                    previous_significant,
                    parens.last().copied(),
                ))
        {
            append_mapped(&mut view, &mut map, b"$x", token.pos());
            let brace = next_significant_index(tokens, index).unwrap();
            append_blank(&mut view, &mut map, b"{", tokens[brace].pos());
            let mut end = brace + 1;
            let mut depth = 1usize;
            while end < tokens.len() {
                let id = tokens[end].id();
                if id == i64::from(b'{')
                    || id == tok::T_CURLY_OPEN
                    || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
                {
                    depth += 1;
                } else if id == i64::from(b'}') {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                end += 1;
            }
            for skipped in &tokens[brace + 1..end.min(tokens.len())] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        if token.id() == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            append_mapped(&mut view, &mut map, b"$x", token.pos());
            let mut end = index + 1;
            let mut depth = 1usize;
            while end < tokens.len() && depth > 0 {
                let id = tokens[end].id();
                if id == i64::from(b'{')
                    || id == tok::T_CURLY_OPEN
                    || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
                {
                    depth += 1;
                } else if id == i64::from(b'}') {
                    depth -= 1;
                }
                if depth == 0 {
                    break;
                }
                end += 1;
            }
            for skipped in &tokens[index + 1..end.min(tokens.len())] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        // A first-class callable is `name(...)`: the ellipsis sits directly inside an argument
        // list. `f($a...)`, `f([]...)` and a declaration's `function f(...)` are ParseErrors, so
        // every other shape keeps its marker.
        if token.id() == tok::T_ELLIPSIS
            && ellipsis_is_first_class_callable(source, tokens, index)
        {
            append_blank(&mut view, &mut map, b"...", token.pos());
            index += 1;
            continue;
        }
        if token.id() == i64::from(b'{')
            && previous_significant == tok::T_DOUBLE_COLON
        {
            // `C::{expr}` computes a member name. The body was validated before the view was
            // built; the view only needs a member identifier, so keep a short one and blank
            // the braces and expression.
            append_mapped(&mut view, &mut map, b"x", token.pos());
            let end =
                matching_brace_end(tokens, index + 1, tokens.len()).unwrap_or(tokens.len());
            for skipped in &tokens[index + 1..end] {
                let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
                append_blank(&mut view, &mut map, text, skipped.pos());
            }
            if end < tokens.len() {
                append_blank(&mut view, &mut map, b"}", tokens[end].pos());
            }
            index = end.saturating_add(1);
            continue;
        }
        if token.id() == i64::from(b'`') {
            index = append_backtick_string(&mut view, &mut map, source, tokens, index);
            previous_significant = token.id();
            index += 1;
            continue;
        }
        if token.id() == tok::T_OPEN_TAG || token.id() == tok::T_INLINE_HTML {
            append_blank(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_OPEN_TAG_WITH_ECHO {
            append_mapped(&mut view, &mut map, b"echo ", token.pos());
        } else if token.id() == tok::T_CLOSE_TAG {
            append_mapped(&mut view, &mut map, b";", token.pos());
            append_blank(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_HALT_COMPILER
            && !halt_compiler_is_name(tokens, index)
        {
            index = skip_halt_compiler(&mut view, &mut map, source, tokens, index);
        } else if matches!(token.id(), tok::T_UNSET_CAST | tok::T_STRING_CAST)
            || (token.id() == tok::T_VOID_CAST
                && void_cast_starts_statement(tokens, index, previous_significant))
        {
            append_safe_cast(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_CONSTANT_ENCAPSED_STRING {
            append_string(&mut view, &mut map, text, token.pos());
        } else if token.id() == tok::T_DNUMBER && text.ends_with(b".") {
            let mut normalized = text.to_vec();
            if let Some(last) = normalized.last_mut() {
                *last = b' ';
            }
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_START_HEREDOC
            && text.len() >= 4
            && matches!(text[0], b'b' | b'B')
            && &text[1..4] == b"<<<"
        {
            let mut normalized = text.to_vec();
            normalized[0] = b' ';
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_NUM_STRING && !text.iter().all(u8::is_ascii_digit) {
            let normalized = vec![b'0'; text.len()];
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if token.id() == tok::T_STRING && text.eq_ignore_ascii_case(b"enum") {
            append_mapped(&mut view, &mut map, b"zenum", token.pos());
        } else if token.id() == tok::T_STRING
            && [
                b"packed".as_slice(),
                b"extern",
                b"ifdef",
                b"self",
                b"parent",
                b"true",
                b"false",
                b"null",
            ]
            .iter()
            .any(|word| text.eq_ignore_ascii_case(word))
        {
            // PHP lexes elephc's `packed`/`extern`/`ifdef` and the PHP names `self`, `parent`,
            // `true`, `false`, `null` as plain T_STRING names. The view must not let the
            // compiler lexer reserve them, or `packed()`, `self;` and `true()` would fail while
            // the compiler-only declarations below still must be rejected.
            let mut normalized = b"z".to_vec();
            normalized.extend_from_slice(text);
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else if text.len() >= 2
            && matches!(text[0], b'b' | b'B')
            && matches!(text[1], b'\'' | b'"')
        {
            let mut normalized = text.to_vec();
            normalized[0] = b' ';
            append_mapped(&mut view, &mut map, &normalized, token.pos());
        } else {
            append_mapped(&mut view, &mut map, text, token.pos());
        }
        if !is_trivia_token(token.id()) {
            previous_significant = token.id();
        }
        index += 1;
    }
    normalize_parse_newlines(&mut view);
    (sanitize_view(view), map, reference_error)
}

/// Replaces each invalid UTF-8 byte with one ASCII placeholder, preserving byte offsets.
pub(crate) fn sanitize_view(view: Vec<u8>) -> String {
    match String::from_utf8(view) {
        Ok(text) => return text,
        Err(error) => return sanitize_invalid_utf8(error.into_bytes()),
    }
}

/// Replaces invalid UTF-8 bytes with ASCII placeholders while preserving byte offsets.
pub(crate) fn sanitize_invalid_utf8(view: Vec<u8>) -> String {
    let mut out = String::with_capacity(view.len());
    let mut offset = 0;
    while offset < view.len() {
        match std::str::from_utf8(&view[offset..]) {
            Ok(valid) => {
                out.push_str(valid);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                out.push_str(std::str::from_utf8(&view[offset..offset + valid])
                    .expect("valid_up_to always ends on a UTF-8 boundary"));
                let invalid = error.error_len().unwrap_or(view.len() - offset - valid);
                for _ in 0..invalid {
                    // PHP identifiers accept bytes >= 0x80, so an underscore keeps the whole
                    // span one identifier and cannot turn a byte name into a reserved word
                    // (`\xffor` must not become `xor`).
                    out.push('_');
                }
                offset += valid + invalid;
            }
        }
    }
    out
}

/// Returns whether an ordinary reference target can start at `next`.
///
/// PHP allows `$a = &$b`, `$a = &$b[0]`, and `$a = &foo()`, but not `$a = &1` or
/// `$a = &new C`. Keeping the ampersand in the view for the latter lets the parser reject it.
pub(crate) fn reference_target_starts_at<T: SourceToken>(tokens: &[T], next: usize) -> bool {
    match tokens[next].id() {
        id if id == tok::T_VARIABLE => true,
        // `$$name` / `${$name}`: the variable variable's name is computed at run time.
        id if id == i64::from(b'$') => true,
        id if id == tok::T_STATIC => next_significant_index(tokens, next)
            .is_some_and(|after| tokens[after].id() == tok::T_DOUBLE_COLON),
        id if id == tok::T_STRING || id == tok::T_IDENTIFIER => {
            next_significant_index(tokens, next).is_some_and(|after| {
                matches!(
                    tokens[after].id(),
                    id if id == i64::from(b'(')
                        || id == i64::from(b'[')
                        || id == i64::from(b'\\')
                        || id == tok::T_DOUBLE_COLON
                        || id == tok::T_OBJECT_OPERATOR
                        || id == tok::T_NULLSAFE_OBJECT_OPERATOR
                )
            })
        }
        // The native lexer folds `\foo`, `Ns\foo` and `namespace\foo` into one name token.
        id if id == tok::T_NAME_FULLY_QUALIFIED
            || id == tok::T_NAME_QUALIFIED
            || id == tok::T_NAME_RELATIVE =>
        {
            next_significant_index(tokens, next).is_some_and(|after| {
                matches!(
                    tokens[after].id(),
                    id if id == i64::from(b'(') || id == tok::T_DOUBLE_COLON
                )
            })
        }
        _ => false,
    }
}

/// Returns whether the token at `index` is a statement label (`name:`).
pub(crate) fn is_label_declaration<T: SourceToken>(
    tokens: &[T],
    index: usize,
    previous_significant: i64,
    previous_closes_control_header: bool,
) -> bool {
    // Check the current token first: a comment or operator run must not trigger the lookahead
    // scan merely to discover it can never be a label.
    if tokens[index].id() != tok::T_STRING {
        return false;
    }
    let Some(next) = next_significant_index(tokens, index) else {
        return false;
    };
    if tokens[next].id() != i64::from(b':') {
        return false;
    }
    let previous = previous_significant_index(tokens, index);
    if previous_significant == i64::from(b'{')
        && previous.is_some_and(|brace| enclosing_brace_is_class(tokens, brace))
    {
        return false;
    }
    if previous_significant == i64::from(b':') && ternary_colon_before_label(tokens, index) {
        return false;
    }
    if previous_significant != 0
        && previous_significant != i64::from(b';')
        && previous_significant != i64::from(b'{')
        && previous_significant != i64::from(b'}')
        && previous_significant != i64::from(b')')
        && previous_significant != i64::from(b':')
        && previous_significant != tok::T_OPEN_TAG
        && previous_significant != tok::T_CLOSE_TAG
        && previous_significant != tok::T_INLINE_HTML
        && previous_significant != tok::T_ELSE
        && previous_significant != tok::T_DO
        && previous_significant != tok::T_TRY
        && previous_significant != tok::T_FINALLY
    {
        return false;
    }
    // A `)` only starts a single-statement body when it closes a control header such as
    // `if (...)`; `echo (1) label:` is not a label at all.
    if previous_significant == i64::from(b')') && !previous_closes_control_header {
        return false;
    }
    true
}

/// Returns whether one token id opens a control structure whose body is a single statement.
fn is_control_header_keyword(id: i64) -> bool {
    id == tok::T_IF
        || id == tok::T_ELSEIF
        || id == tok::T_WHILE
        || id == tok::T_FOR
        || id == tok::T_FOREACH
        || id == tok::T_DECLARE
}

/// Returns whether the `:` before a label candidate closes a ternary expression.
///
/// `case $a ? B : C:` is a case label, not `C:` as a goto label; counting unmatched `?`
/// tokens back to the statement start distinguishes the ternary colon.
pub(crate) fn ternary_colon_before_label<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    // `index` names the label candidate; the colon under test sits immediately before it.
    // Start the scan before that colon, otherwise its own `:` cancels the nearest `?`.
    let Some(colon) = previous_significant_index(tokens, index) else {
        return false;
    };
    let mut depth = 0isize;
    let mut cursor = colon;
    while cursor > 0 {
        cursor -= 1;
        match tokens[cursor].id() {
            id if id == i64::from(b'?') => depth += 1,
            id if id == i64::from(b':') => depth -= 1,
            id if id == i64::from(b';')
                || id == i64::from(b'{')
                || id == i64::from(b'}')
                || id == tok::T_OPEN_TAG
                || id == tok::T_CLOSE_TAG =>
            {
                break;
            }
            _ => {}
        }
        if depth > 0 {
            return true;
        }
    }
    false
}

/// Precomputes the nearest open brace for every token in one forward pass.
///
/// `u32::MAX` means "no enclosing brace". The engine caps source length at `u32::MAX`, so a
/// token index always fits.
pub(crate) fn enclosing_braces<T: SourceToken>(tokens: &[T]) -> Vec<u32> {
    let mut stack: Vec<u32> = Vec::new();
    let mut enclosing = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        enclosing.push(stack.last().copied().unwrap_or(u32::MAX));
        if token.id() == i64::from(b'{') {
            stack.push(u32::try_from(index).unwrap_or(u32::MAX));
        } else if token.id() == i64::from(b'}') {
            stack.pop();
        }
    }
    enclosing
}

/// Returns whether one precomputed enclosing brace opens a class-like body.
pub(crate) fn enclosing_brace_is_class<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    let mut cursor = open;
    while cursor > 0 {
        cursor -= 1;
        let id = tokens[cursor].id();
        if [
            tok::T_CLASS,
            tok::T_INTERFACE,
            tok::T_TRAIT,
            tok::T_ENUM,
        ]
        .contains(&id)
        {
            return true;
        }
        if [tok::T_FUNCTION, tok::T_FN].contains(&id)
            || [i64::from(b';'), i64::from(b'{'), i64::from(b'}')].contains(&id)
        {
            return false;
        }
    }
    false
}

/// Converts a lone CR into LF so elephc's line-based parser sees Zend's line breaks.
pub(crate) fn normalize_parse_newlines(view: &mut [u8]) {
    for index in 0..view.len() {
        if view[index] == b'\r' && view.get(index + 1) != Some(&b'\n') {
            view[index] = b'\n';
        }
    }
}

/// Returns whether one ampersand is a PHP reference marker the parser view should remove.
///
/// Declarations, captures, foreach and reference assignments or array entries accept a marker.
/// The view keeps the ampersand in `foo(&$a)` for the parser to reject, and in `($a & $b)`
/// for the parser to interpret as the bitwise operator.
pub(crate) fn is_reference_ampersand<T: SourceToken>(
    tokens: &[T],
    index: usize,
    previous_significant: i64,
    enclosing_bracket: Option<usize>,
    enclosing_paren: Option<usize>,
    enclosing_brace: Option<usize>,
    reference_value: impl FnOnce(Option<usize>) -> bool,
) -> bool {
    if !is_reference_marker(tokens[index].id()) {
        return false;
    }
    if previous_significant == i64::from(b'=') {
        // A parameter DEFAULT takes an expression, so `function f($a = &$b) {}` is a PHP
        // ParseError even though `$a = &$b;` is a reference assignment.
        if enclosing_paren.is_some_and(|open| paren_is_declaration_parameters(tokens, open)) {
            return false;
        }
        return reference_value(None);
    }
    if [tok::T_FUNCTION, tok::T_AS, tok::T_YIELD].contains(&previous_significant) {
        return true;
    }
    // `key => &$value` is an array entry. An arrow-function body or a match arm is not
    // (`fn() => &$x` is a PHP ParseError), so its marker must stay for the parser to reject.
    if previous_significant == tok::T_DOUBLE_ARROW {
        let Some(arrow) = previous_significant_index(tokens, index) else { return false; };
        return arrow_pair_is_array_value(
            tokens,
            arrow,
            enclosing_bracket,
            enclosing_paren,
            enclosing_brace,
        ) && reference_value(Some(arrow));
    }
    if previous_significant != i64::from(b'(') && previous_significant != i64::from(b',')
        && previous_significant != i64::from(b'[')
    {
        return false;
    }
    // Array entries accept a reference marker (`[&$x, &$y]`), index expressions never do
    // (`$a[&$b]` is a PHP ParseError). Destructuring and `foreach` targets are array entries.
    if previous_significant == i64::from(b'[') {
        if let Some(open) = enclosing_bracket {
            if bracket_opens_array_literal(tokens, open) {
                return reference_value(None);
            }
        }
        return false;
    }
    if previous_significant == i64::from(b',') {
        if let Some(open) = enclosing_bracket {
            if bracket_opens_array_literal(tokens, open)
                && delimiter_is_outside(open, enclosing_paren)
                && delimiter_is_outside(open, enclosing_brace)
            {
                return reference_value(None);
            }
        }
    }
    let Some(open) = enclosing_paren else {
        return false;
    };
    let before = previous_significant_index(tokens, open);
    let Some(before) = before else {
        return false;
    };
    // `array(&$x)` and `list(&$x)` accept a reference marker only when a reference target
    // follows; removing the marker before `array(&1)` would hand the parser a valid `array(1)`
    // and silently approve invalid PHP. Declaration and capture lists keep their marker
    // unconditionally because the parser still validates the following parameter or capture.
    if [tok::T_ARRAY, tok::T_LIST].contains(&tokens[before].id()) {
        return reference_value(None);
    }
    if [tok::T_FUNCTION, tok::T_FN, tok::T_USE].contains(&tokens[before].id()) {
        return true;
    }
    // A named function or method declaration: `function foo(&$x)`.
    if tokens[before].id() == tok::T_STRING
        || tokens[before].id() == tok::T_IDENTIFIER
    {
        return previous_significant_index(tokens, before)
            .is_some_and(|prior| tokens[prior].id() == tok::T_FUNCTION);
    }
    false
}

/// Identifies either Zend token form of an ampersand.
fn is_reference_marker(id: i64) -> bool {
    matches!(id, tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG | tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG)
}

/// Distinguishes a `fn(...) =>` inside an array from the array's key/value separator.
///
/// Only the adjacent header is inspected. A parenthesized return type is skipped through
/// the delimiter map, so nested groups do not make repeated arrow checks rescan the body.
fn arrow_starts_function_body<T: SourceToken>(
    tokens: &[T],
    arrow: usize,
    pairs: &[Option<usize>],
) -> bool {
    let Some(mut cursor) = previous_significant_index(tokens, arrow) else { return false; };
    if ends_arrow_parameter_list(tokens, cursor, pairs) {
        return true;
    }
    loop {
        let id = tokens[cursor].id();
        if id == i64::from(b':') {
            return previous_significant_index(tokens, cursor)
                .is_some_and(|close| ends_arrow_parameter_list(tokens, close, pairs));
        }
        if id == i64::from(b')') {
            let Some(open) = pairs[cursor].filter(|&open| tokens[open].id() == i64::from(b'(')) else {
                return false;
            };
            cursor = open;
        } else if !is_declaration_type_token(id)
            && !is_reference_marker(id)
            && id != i64::from(b'?')
            && id != i64::from(b'|')
        {
            return false;
        }
        let Some(before) = previous_significant_index(tokens, cursor) else { return false; };
        cursor = before;
    }
}

/// Checks the parameter close and the optional return-reference marker before its opener.
fn ends_arrow_parameter_list<T: SourceToken>(
    tokens: &[T],
    close: usize,
    pairs: &[Option<usize>],
) -> bool {
    if tokens[close].id() != i64::from(b')') {
        return false;
    }
    let Some(open) = pairs[close].filter(|&open| tokens[open].id() == i64::from(b'(')) else {
        return false;
    };
    let Some(mut keyword) = previous_significant_index(tokens, open) else { return false; };
    if is_reference_marker(tokens[keyword].id()) {
        let Some(before) = previous_significant_index(tokens, keyword) else { return false; };
        keyword = before;
    }
    tokens[keyword].id() == tok::T_FN && previous_significant_index(tokens, keyword).is_none_or(|before| {
        !matches!(
            tokens[before].id(),
            tok::T_DOUBLE_COLON | tok::T_OBJECT_OPERATOR | tok::T_NULLSAFE_OBJECT_OPERATOR
        )
    })
}

/// Returns whether the ellipsis at `index` is a first-class callable marker.
///
/// The marker only appears as `callee(...)`. A declaration parameter list — `function (...)`,
/// `function f(...)`, `fn(...)`, `fn&(...)` — looks identical, so the callee tokens before the
/// opening parenthesis decide: a `function`/`fn` keyword means the ellipsis must stay and the
/// parser rejects the missing parameter name exactly as PHP does.
pub(crate) fn ellipsis_is_first_class_callable<T: SourceToken>(
    source: &[u8],
    tokens: &[T],
    index: usize,
) -> bool {
    let Some(open) = previous_significant_index(tokens, index) else {
        return false;
    };
    if tokens[open].id() != i64::from(b'(')
        || !next_significant_index(tokens, index)
            .is_some_and(|next| tokens[next].id() == i64::from(b')'))
    {
        return false;
    }
    let mut cursor = open;
    while let Some(before) = previous_significant_index(tokens, cursor) {
        match tokens[before].id() {
            id if id == tok::T_FUNCTION || id == tok::T_FN => return false,
            // A named declaration puts the name (and an optional reference marker) between the
            // keyword and the parenthesis: `function &f(...)`.
            id if id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                || id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG =>
            {
                cursor = before;
            }
            _ => {
                // Semi-reserved method names (`match`, `readonly`, `list`, ...) lex as their
                // keyword token; any identifier-shaped spelling is still a declaration name.
                let token = &tokens[before];
                let text = &source[token.pos()..token.pos() + token.len()];
                if !is_identifier_text(text) {
                    break;
                }
                cursor = before;
            }
        }
    }
    true
}

/// Returns whether a token text is a PHP identifier spelling.
pub(crate) fn is_identifier_text(text: &[u8]) -> bool {
    text.first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_' || *byte >= 0x80)
        && text.iter().all(|byte| {
            byte.is_ascii_alphanumeric() || *byte == b'_' || *byte >= 0x80
        })
}

/// Returns whether one `[` opens an array literal rather than a subscript.
///
/// `$a[0]` and `Foo::BAR[0]` index an expression that ends right before the bracket; `[&$x]`,
/// `f([&$x])` and `[$x, &$y] = $arr` open a new literal. The distinction decides whether a `&`
/// inside the bracket is a reference marker or an invalid operator.
pub(crate) fn bracket_opens_array_literal<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    if tokens[open].id() == tok::T_ATTRIBUTE {
        return false;
    }
    previous_significant_index(tokens, open).is_none_or(|before| {
        !matches!(
            tokens[before].id(),
            id if id == tok::T_VARIABLE
                || id == tok::T_STRING
                || id == tok::T_NAME_FULLY_QUALIFIED
                || id == tok::T_NAME_QUALIFIED
                || id == tok::T_NAME_RELATIVE
                || id == tok::T_LNUMBER
                || id == tok::T_DNUMBER
                || id == tok::T_CONSTANT_ENCAPSED_STRING
                || id == i64::from(b')')
                || id == i64::from(b']')
        )
    })
}

/// Returns whether a `key => value` pair sits inside an array literal, `array(...)`,
/// `list(...)`, or the `as key => value` target of a `foreach` header.
///
/// The arrow has to sit directly in the opener's pair list: an arrow nested inside a closure,
/// `match` arm, or nested array is that construct's own pair and does not accept a reference
/// here (`array(1 => match($x) { 2 => &$v })` is a PHP ParseError).
pub(crate) fn arrow_pair_is_array_value<T: SourceToken>(
    tokens: &[T],
    arrow: usize,
    enclosing_bracket: Option<usize>,
    enclosing_paren: Option<usize>,
    enclosing_brace: Option<usize>,
) -> bool {
    if let Some(open) = enclosing_bracket {
        if bracket_opens_array_literal(tokens, open)
            && delimiter_is_outside(open, enclosing_paren)
            && delimiter_is_outside(open, enclosing_brace)
        {
            return true;
        }
    }
    let Some(open) = enclosing_paren else { return false; };
    if !delimiter_is_outside(open, enclosing_bracket)
        || !delimiter_is_outside(open, enclosing_brace)
    {
        return false;
    }
    let Some(before) = previous_significant_index(tokens, open) else { return false; };
    match tokens[before].id() {
        id if id == tok::T_ARRAY || id == tok::T_LIST => true,
        id if id == tok::T_FOREACH => list_contains_at_level(tokens, open, arrow, Some(tok::T_AS)),
        _ => false,
    }
}

/// Returns whether an inner delimiter opened before `open`, so it encloses rather than nests.
pub(crate) fn delimiter_is_outside(open: usize, inner: Option<usize>) -> bool {
    inner.is_none_or(|inner| inner < open)
}

/// Returns whether `end` sits directly in `open`'s list, optionally after a top-level `target`.
///
/// Nesting counts every delimiter kind so an arrow inside a closure, attribute, or nested array
/// does not qualify as a direct pair of the enclosing list.
pub(crate) fn list_contains_at_level<T: SourceToken>(
    tokens: &[T],
    open: usize,
    end: usize,
    target: Option<i64>,
) -> bool {
    let mut depth = 0usize;
    for index in open + 1..end {
        let id = tokens[index].id();
        if id == i64::from(b'(')
            || id == i64::from(b'[')
            || id == i64::from(b'{')
            || id == tok::T_ATTRIBUTE
            || id == tok::T_CURLY_OPEN
            || id == tok::T_DOLLAR_OPEN_CURLY_BRACES
        {
            depth += 1;
        } else if id == i64::from(b')')
            || id == i64::from(b']')
            || id == i64::from(b'}')
        {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && target.is_some_and(|target| id == target) {
            return true;
        }
    }
    depth == 0
}

/// Returns whether a `(void)` cast stands where a STATEMENT may begin.
///
/// PHP accepts `(void)$x;` only as a statement: `$a = (void)$x;` and `foo((void)$x);` are parse
/// errors. The other casts keep the rewrite everywhere.
pub(crate) fn void_cast_starts_statement<T: SourceToken>(
    tokens: &[T],
    index: usize,
    previous_significant: i64,
) -> bool {
    match previous_significant {
        id if id == i64::from(b';')
            || id == i64::from(b'{')
            || id == i64::from(b'}')
            || id == i64::from(b':')
            || id == tok::T_OPEN_TAG
            || id == tok::T_CLOSE_TAG
            || id == tok::T_DO
            || id == tok::T_ELSE =>
        {
            true
        }
        id if id == i64::from(b')') => {
            let Some(close) = previous_significant_index(tokens, index) else {
                return false;
            };
            let mut depth = 1usize;
            let mut cursor = close;
            while cursor > 0 {
                cursor -= 1;
                match tokens[cursor].id() {
                    id if id == i64::from(b')') => depth += 1,
                    id if id == i64::from(b'(') => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            previous_significant_index(tokens, cursor).is_some_and(|before| {
                matches!(
                    tokens[before].id(),
                    id if id == tok::T_IF
                        || id == tok::T_ELSEIF
                        || id == tok::T_WHILE
                        || id == tok::T_FOR
                        || id == tok::T_FOREACH
                )
            })
        }
        _ => false,
    }
}

/// Returns whether one `(` opens a declaration parameter list.
pub(crate) fn paren_is_declaration_parameters<T: SourceToken>(tokens: &[T], open: usize) -> bool {
    let Some(before) = previous_significant_index(tokens, open) else { return false; };
    // An anonymous declaration has no name between the keyword and the list:
    // `function (...)`, `fn(...)`, and their `static` forms.
    if tokens[before].id() == tok::T_FUNCTION || tokens[before].id() == tok::T_FN {
        return true;
    }
    let Some(prior) = previous_significant_index(tokens, before) else { return false; };
    let prior_id = tokens[prior].id();
    if prior_id == tok::T_FUNCTION || prior_id == tok::T_FN {
        // The name may be any semi-reserved spelling (`readonly`, `list`, ...).
        return true;
    }
    // `function &f(...)`: skip the optional return-reference marker before the name.
    matches!(
        prior_id,
        id if id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
            || id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
    ) && previous_significant_index(tokens, prior)
        .is_some_and(|function| {
            matches!(
                tokens[function].id(),
                id if id == tok::T_FUNCTION || id == tok::T_FN
            )
        })
}

/// Returns whether a `$` opens a declaration name slot instead of a variable variable.
///
/// PHP only allows variable variables in expressions: `static $$a;`, `function f($$a)`,
/// `public $$a;`, `final $$a`, `private(set) $$a`, `catch (E $$e)` and `use ($$a)` are syntax
/// errors, so their `$` must stay in the view and let the parser reject it. A parameter
/// DEFAULT (`function f($x = $$a)`) is an expression again. `abstract` and `var` stay off this
/// fast path: PHP reports the modifier itself, not the `$`.
pub(crate) fn dollar_starts_declaration<T: SourceToken>(
    tokens: &[T],
    index: usize,
    previous_significant: i64,
    enclosing_paren: Option<usize>,
) -> bool {
    if matches!(
        previous_significant,
        id if id == tok::T_STATIC
            || id == tok::T_PUBLIC
            || id == tok::T_PROTECTED
            || id == tok::T_PRIVATE
            || id == tok::T_READONLY
            || id == tok::T_FINAL
            || id == tok::T_PUBLIC_SET
            || id == tok::T_PROTECTED_SET
            || id == tok::T_PRIVATE_SET
    ) {
        return true;
    }
    let Some(open) = enclosing_paren else {
        // A typed property has no enclosing parameter list: walk back over the type to the
        // declaration modifier, stopping at a statement/expression boundary.
        return property_modifier_before(tokens, index);
    };
    let before = previous_significant_index(tokens, open).map(|index| tokens[index].id());
    if before == Some(tok::T_CATCH) {
        return true;
    }
    let declaration = before == Some(tok::T_USE) || paren_is_declaration_parameters(tokens, open);
    if !declaration {
        return false;
    }
    // Track the optional type before the name: `function f(int $$a)`, `?Foo $$a`,
    // `A|B $$a`, DNF types and variadics all put a type-shaped token before the `$`.
    is_declaration_type_token(previous_significant)
        || matches!(
            previous_significant,
            id if id == i64::from(b'(')
                || id == i64::from(b',')
                || id == i64::from(b'&')
                || id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                || id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
                || id == tok::T_ELLIPSIS
        )
}

/// Returns whether a token can end a declaration's type before its variable.
pub(crate) fn is_declaration_type_token(id: i64) -> bool {
    matches!(
        id,
        id if id == tok::T_STRING
            || id == tok::T_IDENTIFIER
            || id == tok::T_NAME_FULLY_QUALIFIED
            || id == tok::T_NAME_QUALIFIED
            || id == tok::T_NAME_RELATIVE
            || id == tok::T_ARRAY
            || id == tok::T_CALLABLE
            || id == tok::T_STATIC
            || id == tok::T_NS_SEPARATOR
            || id == i64::from(b'\\')
            || id == i64::from(b')')
    )
}

/// Returns whether a token immediately before a `$` can end a declaration's type/modifier.
///
/// Anything else (an operator, a literal, a variable, a control keyword) means the `$` starts
/// an expression, so the declaration scan can stop without walking the statement prefix.
fn declaration_name_predecessor(id: i64) -> bool {
    is_declaration_type_token(id)
        || matches!(
            id,
            id if id == tok::T_PUBLIC
                || id == tok::T_PROTECTED
                || id == tok::T_PRIVATE
                || id == tok::T_READONLY
                || id == tok::T_STATIC
                || id == tok::T_FINAL
                || id == tok::T_ABSTRACT
                || id == tok::T_VAR
                || id == tok::T_PUBLIC_SET
                || id == tok::T_PROTECTED_SET
                || id == tok::T_PRIVATE_SET
                || id == i64::from(b'?')
                || id == i64::from(b'|')
                || id == i64::from(b'&')
                || id == i64::from(b'<')
                || id == i64::from(b'>')
                || id == i64::from(b']')
                || id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                || id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
                || id == tok::T_ELLIPSIS
        )
}

/// Returns whether a token can appear between a property modifier and its variable.
fn is_property_type_token(id: i64) -> bool {
    (is_declaration_type_token(id) && id != tok::T_STATIC)
        || id == i64::from(b'?')
        || id == i64::from(b'|')
        || id == i64::from(b'&')
        || id == i64::from(b'(')
        || id == i64::from(b')')
        || id == tok::T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
        || id == tok::T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
}

/// Returns whether a token is a property modifier that can introduce `$$name`.
///
/// Asymmetric visibility, `final`, `abstract` and `var` are modifiers too. They do not appear
/// in an expression before `$$name`, so `static::$$a` and `instanceof static ? $$a` stay
/// expressions.
fn is_property_modifier(id: i64) -> bool {
    id == tok::T_PUBLIC
        || id == tok::T_PROTECTED
        || id == tok::T_PRIVATE
        || id == tok::T_STATIC
        || id == tok::T_READONLY
        || id == tok::T_FINAL
        || id == tok::T_ABSTRACT
        || id == tok::T_VAR
        || id == tok::T_PUBLIC_SET
        || id == tok::T_PROTECTED_SET
        || id == tok::T_PRIVATE_SET
}

/// Returns whether the token before a property modifier starts a declaration.
///
/// `instanceof static` is an expression. `public static`, `{ static` and `#[Attr] public`
/// are declarations. Another modifier, including `final`, `var` and `private(set)`, also
/// starts the declaration.
fn property_declaration_boundary(id: Option<i64>) -> bool {
    let Some(id) = id else {
        return true;
    };
    id == i64::from(b';')
        || id == i64::from(b'{')
        || id == i64::from(b'}')
        || id == i64::from(b']')
        || id == tok::T_OPEN_TAG
        || is_property_modifier(id)
}

/// Returns whether a declaration modifier precedes `index` across a property type only.
///
/// `static::$$a` stops at `::`. A modifier counts only when every token between it and `$`
/// is a property type and the token before the modifier is a declaration boundary, so
/// `$o instanceof static ? $$a` stays an expression. `static $$a`, `public static ?int $$a`
/// and `public private(set) int $$a` still count as declarations.
pub(crate) fn property_modifier_before<T: SourceToken>(tokens: &[T], index: usize) -> bool {
    // An operator, literal, variable or keyword immediately before the `$` proves this is an
    // expression. Rejecting it in O(1) keeps repeated `$$a` terms from walking the statement.
    let Some(immediate) = previous_significant_index(tokens, index) else {
        return false;
    };
    let immediate_id = tokens[immediate].id();
    if immediate_id == tok::T_DOUBLE_COLON || !declaration_name_predecessor(immediate_id) {
        return false;
    }
    let mut cursor = index;
    while let Some(previous) = previous_significant_index(tokens, cursor) {
        let id = tokens[previous].id();
        if id == tok::T_DOUBLE_COLON {
            return false;
        }
        if is_property_modifier(id) {
            let before = previous_significant_index(tokens, previous).map(|index| tokens[index].id());
            return property_declaration_boundary(before);
        }
        if is_property_type_token(id) {
            cursor = previous;
            continue;
        }
        return false;
    }
    false
}

/// Returns whether a token is trivia for previous-significant-token tracking.
pub(crate) fn is_trivia_token(id: i64) -> bool {
    id == tok::T_WHITESPACE || id == tok::T_COMMENT || id == tok::T_DOC_COMMENT
}

/// Appends a backtick expression as a plain double-quoted string for parser validation.
pub(crate) fn append_backtick_string<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    open: usize,
) -> usize {
    append_backtick_string_nested(view, map, source, tokens, open, 0)
}

/// Appends one backtick expression at `depth`, bounding nested shell-string recursion.
pub(crate) fn append_backtick_string_nested<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    open: usize,
    depth: usize,
) -> usize {
    append_mapped(view, map, b"\"", tokens[open].pos());
    let Some(close) = append_backtick_body(view, map, source, tokens, open, depth) else {
        return open;
    };
    append_mapped(view, map, b"\"", tokens[close].pos());
    close
}

/// Copies one backtick body as a double-quoted string body.
///
/// Only a backtick at interpolation depth zero closes this string. A nested shell string inside
/// `{$...}` is normalized the same way instead of ending the outer one, and `{$expr}` regions
/// otherwise keep their quotes and escapes verbatim.
pub(crate) fn append_backtick_body<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    open: usize,
    depth: usize,
) -> Option<usize> {
    // An escaped `$` (`\$`) must not start interpolation once the backslash is blanked.
    let mut backslashes = 0usize;
    let mut expression_depth = 0usize;
    let mut index = open + 1;
    while index < tokens.len() {
        let token = &tokens[index];
        let id = token.id();
        if expression_depth == 0 && id == i64::from(b'`') {
            return Some(index);
        }
        if id == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            // `${expr}` is rewritten exactly like the main view does: emit a plain variable and
            // blank the braced expression, so a variable variable (`${$name}`) or any other
            // expression inside it never reaches the shared lexer's string scanner.
            append_mapped(view, map, b"$x", token.pos());
            backslashes = 0;
            index = blank_braced_expression(view, map, source, tokens, index);
            continue;
        }
        if id == tok::T_CURLY_OPEN {
            expression_depth += 1;
        }
        if expression_depth > 0 && id == i64::from(b'`') {
            // Nested shell strings are rewritten recursively. Past this depth the marker is
            // blanked instead: a pathological source must not exhaust the native stack, and the
            // surrounding expression stays grammar-valid for the parser.
            if depth >= MAX_SHELL_NESTING {
                let text = &source[token.pos()..token.pos() + token.len()];
                append_blank(view, map, text, token.pos());
            } else {
                index = append_backtick_string_nested(view, map, source, tokens, index, depth + 1);
            }
            backslashes = 0;
        } else if expression_depth > 0 {
            if id == i64::from(b'$')
                && next_significant_index(tokens, index).is_some_and(|next| tokens[next].id() == i64::from(b'{'))
            {
                // `${expr}` written inside an interpolation expression (e.g. `{${$name}}`):
                // emit a plain variable and blank the braced region like the main view does.
                let brace = next_significant_index(tokens, index).expect("brace follows dollar");
                append_mapped(view, map, b"$x", token.pos());
                append_blank(view, map, b"{", tokens[brace].pos());
                index = blank_braced_expression(view, map, source, tokens, brace);
                continue;
            }
            if id == i64::from(b'$')
                && next_significant_index(tokens, index).is_some_and(|next| {
                    let next_id = tokens[next].id();
                    next_id == i64::from(b'$')
                        || next_id == tok::T_VARIABLE
                        || next_id == tok::T_DOLLAR_OPEN_CURLY_BRACES
                })
            {
                // `{$$name}` is a variable variable inside interpolation. The closed-world parser
                // has no variable-name table, so blank the marker like the main view does.
                append_blank(view, map, b"$", token.pos());
            } else {
                let text = &source[token.pos()..token.pos() + token.len()];
                for (offset, byte) in text.iter().copied().enumerate() {
                    view.push(byte);
                    map.push(u32::try_from(token.pos() + offset).unwrap_or(u32::MAX));
                }
            }
            backslashes = 0;
        } else {
            let text = &source[token.pos()..token.pos() + token.len()];
            for (offset, byte) in text.iter().copied().enumerate() {
                let escaped_dollar = byte == b'$' && backslashes % 2 == 1;
                view.push(if matches!(byte, b'\n' | b'\r') {
                    byte
                } else if matches!(byte, b'"' | b'\\') {
                    b' '
                } else if escaped_dollar {
                    b' '
                } else {
                    byte
                });
                map.push(u32::try_from(token.pos() + offset).unwrap_or(u32::MAX));
                if byte == b'\\' {
                    backslashes += 1;
                } else {
                    backslashes = 0;
                }
            }
        }
        if expression_depth > 0 {
            if id == i64::from(b'{') {
                expression_depth += 1;
            } else if id == i64::from(b'}') {
                expression_depth -= 1;
            }
        }
        index += 1;
    }
    None
}

/// Deepest nested backtick rewrite the validation view performs before blanking the marker.
pub(crate) const MAX_SHELL_NESTING: usize = 256;

/// Appends a valid cast in the same number of view bytes, preserving offsets.
pub(crate) fn append_safe_cast(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    let safe = b"(int)";
    for (offset, byte) in text.iter().copied().enumerate() {
        view.push(if offset < safe.len() {
            safe[offset]
        } else if matches!(byte, b'\n' | b'\r') {
            byte
        } else {
            b' '
        });
        map.push(u32::try_from(source_pos + offset).unwrap_or(u32::MAX));
    }
}

/// Appends a string literal with any binary prefix blanked for parser validation.
pub(crate) fn append_string(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    if text.len() >= 2 && matches!(text[0], b'b' | b'B') && matches!(text[1], b'\'' | b'"') {
        let mut normalized = text.to_vec();
        normalized[0] = b' ';
        append_mapped(view, map, &normalized, source_pos);
    } else {
        append_mapped(view, map, text, source_pos);
    }
}

/// Blanks one token while preserving CR/LF bytes for line numbering.
pub(crate) fn append_blank(view: &mut Vec<u8>, map: &mut Vec<u32>, text: &[u8], source_pos: usize) {
    for (offset, byte) in text.iter().copied().enumerate() {
        view.push(if matches!(byte, b'\n' | b'\r') { byte } else { b' ' });
        map.push(u32::try_from(source_pos + offset).unwrap_or(u32::MAX));
    }
}

/// Blanks the tokens of one braced interpolation expression through its closing `}`.
///
/// `open_brace` is the token that owns the opener (`${` or a plain `{`). Returns the index just
/// past the closing brace so the caller can resume with the rest of the shell-string body.
pub(crate) fn blank_braced_expression<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    open_brace: usize,
) -> usize {
    let mut end = open_brace + 1;
    let mut depth = 1usize;
    while end < tokens.len() && depth > 0 {
        let id = tokens[end].id();
        if id == i64::from(b'{') || id == tok::T_CURLY_OPEN || id == tok::T_DOLLAR_OPEN_CURLY_BRACES {
            depth += 1;
        } else if id == i64::from(b'}') {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        end += 1;
    }
    for skipped in &tokens[open_brace + 1..end.min(tokens.len())] {
        let text = &source[skipped.pos()..skipped.pos() + skipped.len()];
        append_blank(view, map, text, skipped.pos());
    }
    if end < tokens.len() {
        append_blank(view, map, b"}", tokens[end].pos());
    }
    end.saturating_add(1)
}

/// Appends bytes mapped to one source position.
pub(crate) fn append_mapped(view: &mut Vec<u8>, map: &mut Vec<u32>, bytes: &[u8], source_pos: usize) {
    view.extend_from_slice(bytes);
    map.resize(map.len() + bytes.len(), u32::try_from(source_pos).unwrap_or(u32::MAX));
}

/// Blanks a valid `__halt_compiler ( ) ;` header and returns the last consumed token index.
pub(crate) fn skip_halt_compiler<T: SourceToken>(
    view: &mut Vec<u8>,
    map: &mut Vec<u32>,
    source: &[u8],
    tokens: &[T],
    start: usize,
) -> usize {
    let mut index = start;
    let mut significant = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let text = &source[token.pos()..token.pos() + token.len()];
        append_blank(view, map, text, token.pos());
        if !is_trivia(token.id()) {
            significant += 1;
            if significant == 3 {
                return index;
            }
        }
        index += 1;
    }
    tokens.len().saturating_sub(1)
}
