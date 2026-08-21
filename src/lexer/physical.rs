//! Purpose:
//! Locates PHP's terminal `__HALT_COMPILER()` directive in raw physical-file bytes.
//! Decodes only the executable prefix so an arbitrary binary payload remains opaque.
//!
//! Called from:
//! - `crate::lexer::tokenize_bytes_with_mode()` for entry, include, and autoload files.
//!
//! Key details:
//! - Candidate validation reuses the PHP lexer, so strings/comments cannot masquerade as directives.
//! - The stored offset is byte-based and includes a close-tag terminator plus its absorbed newline.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::source::SourceMode;
use crate::span::Span;

/// Byte positions for one validated halt directive.
#[derive(Debug, Clone, Copy)]
pub(super) struct HaltDirective {
    /// Keyword start relative to the BOM-stripped source slice scanned by `Cursor`.
    pub(super) scan_start: usize,
    /// Physical-file byte offset immediately after PHP's statement terminator.
    pub(super) raw_offset: usize,
}

/// Tokenizes a physical byte stream while keeping a valid post-halt suffix opaque.
pub(super) fn tokenize_physical_bytes(
    source: &[u8],
    mode: SourceMode,
) -> Result<Vec<super::SpannedToken>, CompileError> {
    let bom_len = usize::from(source.starts_with(&[0xef, 0xbb, 0xbf])) * 3;
    let raw_halt = find_halt_directive(source, mode);
    let code_end = raw_halt.map_or(source.len(), |(_, end)| end);
    let code = std::str::from_utf8(&source[..code_end]).map_err(|error| {
        CompileError::new(
            span_for_byte_offset(source, error.valid_up_to()),
            "PHP source before __HALT_COMPILER() must be valid UTF-8",
        )
    })?;
    let directive = raw_halt.map(|(start, end)| HaltDirective {
        scan_start: start.saturating_sub(bom_len),
        raw_offset: end,
    });
    super::scan::scan_tokens(code, mode, directive)
}

/// Finds the first lexically valid direct halt statement and its PHP byte offset.
fn find_halt_directive(source: &[u8], mode: SourceMode) -> Option<(usize, usize)> {
    const KEYWORD: &[u8] = b"__halt_compiler";
    let mut search_from = 0;
    while search_from + KEYWORD.len() <= source.len() {
        let relative = source[search_from..]
            .windows(KEYWORD.len())
            .position(|window| window.eq_ignore_ascii_case(KEYWORD))?;
        let start = search_from + relative;
        search_from = start + 1;
        if !has_identifier_boundaries(source, start, KEYWORD.len()) {
            continue;
        }
        let Some(end) = halt_statement_end(source, start + KEYWORD.len()) else {
            continue;
        };
        let Ok(prefix) = std::str::from_utf8(&source[..end]) else {
            continue;
        };
        let Ok(tokens) = super::scan::scan_tokens(prefix, mode, None) else {
            continue;
        };
        if tokens_end_in_direct_halt(&tokens) {
            return Some((start, end));
        }
    }
    None
}

/// Checks PHP identifier boundaries around an ASCII halt keyword candidate.
fn has_identifier_boundaries(source: &[u8], start: usize, len: usize) -> bool {
    let identifier_byte = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80;
    !start.checked_sub(1).is_some_and(|index| identifier_byte(source[index]))
        && !source
            .get(start + len)
            .copied()
            .is_some_and(identifier_byte)
}

/// Parses trivia plus `()` and either `;` or PHP's closing-tag semicolon token.
fn halt_statement_end(source: &[u8], mut pos: usize) -> Option<usize> {
    pos = skip_php_trivia(source, pos);
    (source.get(pos) == Some(&b'(')).then_some(())?;
    pos += 1;
    pos = skip_php_trivia(source, pos);
    (source.get(pos) == Some(&b')')).then_some(())?;
    pos += 1;
    pos = skip_php_trivia(source, pos);
    if source.get(pos) == Some(&b';') {
        return Some(pos + 1);
    }
    if source.get(pos..pos + 2) == Some(b"?>") {
        pos += 2;
        if source.get(pos..pos + 2) == Some(b"\r\n") {
            return Some(pos + 2);
        }
        if source
            .get(pos)
            .is_some_and(|byte| matches!(*byte, b'\n' | b'\r'))
        {
            return Some(pos + 1);
        }
        return Some(pos);
    }
    None
}

/// Skips whitespace and PHP comments between halt-directive grammar tokens.
fn skip_php_trivia(source: &[u8], mut pos: usize) -> usize {
    loop {
        while source.get(pos).is_some_and(|byte| is_php_whitespace(*byte)) {
            pos += 1;
        }
        if source.get(pos..pos + 2) == Some(b"//") {
            pos += 2;
            while source
                .get(pos)
                .is_some_and(|byte| !matches!(*byte, b'\n' | b'\r'))
                && source.get(pos..pos + 2) != Some(b"?>")
            {
                pos += 1;
            }
            continue;
        }
        if source.get(pos) == Some(&b'#') && source.get(pos..pos + 2) != Some(b"#[") {
            pos += 1;
            while source
                .get(pos)
                .is_some_and(|byte| !matches!(*byte, b'\n' | b'\r'))
                && source.get(pos..pos + 2) != Some(b"?>")
            {
                pos += 1;
            }
            continue;
        }
        if source.get(pos..pos + 2) == Some(b"/*") {
            let tail = &source[pos + 2..];
            let Some(relative_end) = tail.windows(2).position(|window| window == b"*/") else {
                return source.len();
            };
            pos += 2 + relative_end + 2;
            continue;
        }
        return pos;
    }
}

/// Returns whether one raw byte belongs to PHP's scanner-level whitespace class.
fn is_php_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t')
}

/// Confirms the candidate is a direct statement rather than `\`, `->`, or `::` call syntax.
fn tokens_end_in_direct_halt(tokens: &[super::SpannedToken]) -> bool {
    let Some((prefix, tail)) = tokens.split_last() else {
        return false;
    };
    if prefix.0 != Token::Eof || tail.len() < 4 {
        return false;
    }
    let suffix = &tail[tail.len() - 4..];
    if !matches!(&suffix[0].0, Token::HaltCompiler(_))
        || suffix[1].0 != Token::LParen
        || suffix[2].0 != Token::RParen
        || suffix[3].0 != Token::Semicolon
    {
        return false;
    }
    !tail
        .get(tail.len().saturating_sub(5))
        .is_some_and(|(token, _)| {
            matches!(
                token,
                Token::Backslash | Token::Arrow | Token::QuestionArrow | Token::DoubleColon
            )
        })
}

/// Computes a one-based source span for a raw byte position used in UTF-8 diagnostics.
fn span_for_byte_offset(source: &[u8], offset: usize) -> Span {
    let mut line = 1_u32;
    let mut col = 1_u32;
    for byte in &source[..offset.min(source.len())] {
        if *byte == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    Span::new(line, col)
}
