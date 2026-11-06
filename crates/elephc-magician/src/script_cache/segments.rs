//! Purpose:
//! Parses an included PHP file, inline HTML and every code block together, into the one
//! program the interpreter replays. This is the unit the runtime script cache stores: doing
//! it once per file replaces the per-include `<?php`/`?>` scan and parse.
//!
//! Called from:
//! - `crate::script_cache::store` when filling an entry.
//! - `crate::interpreter::include_exec` for the uncached path.
//!
//! Key details:
//! - ONE PROGRAM PER FILE, as php-src compiles a whole script. Each block used to be parsed
//!   on its own, so a block spanning the tags failed: the most common template shape,
//!   `<?php foreach ($rows as $r) { ?><li><?= $r ?></li><?php } ?>`, was a parse error where
//!   reference PHP 8.5.10 runs it (MEASURED). The file is now cut at its tags ([`split_script`],
//!   the same scan as before) and the pieces are joined into one token stream: a code block is
//!   lexed where it sits, inline HTML becomes `echo "<bytes>";`, `?>` becomes `;` and `<?=`
//!   becomes `echo`, which are exactly the tokens the eval lexer produces for a `?>` in eval'd
//!   code (`crate::lexer::inline_html`).
//! - Inline HTML is carried as BYTES, not lexed: an included template may hold any encoding
//!   outside its PHP blocks, and the eval lexer only reads UTF-8.
//! - A PARSE ERROR ANYWHERE FAILS THE WHOLE FILE before any of it runs, as in PHP. Replay used
//!   to print the blocks ahead of the broken one first.
//! - Each block is lexed at its own line, and `?>` swallows the one newline after it, as the
//!   Zend scanner does: `__LINE__` restarted at 1 in every block, and the newline was printed.
//! - `<?php` opens code only when followed by whitespace or the end of the file (`<?phpX` is
//!   HTML), and `<?=` opens an echo; the short `<?` stays HTML (`short_open_tag` is off in the
//!   production php.ini). The eval lexer applies the same rules.
//! - [`ParseMode`] is load-bearing, not a tuning knob. With the script cache OFF this parse
//!   runs on EVERY include, so it must go through a byte-keyed parse memo; dropping the memo
//!   regressed a 63 KiB include from 1.58 ms to 5.38 ms. With the cache ON the script entry IS
//!   the memo, and memoizing it a second time would only pin another copy.

use crate::errors::EvalParseError;
use crate::eval_ir::EvalProgram;
use crate::lexer::{tokenize_at_line, Token, TokenKind};
use crate::parse_cache::parse_script_cached;
use crate::parser;
use elephc_builtin_contract::string_literal::{push_escaped_byte, push_literal_char};
use std::sync::Arc;

/// Whether a script parse should go through the byte-keyed parse memo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParseMode {
    /// The caller keeps the result: parse directly, memoizing nothing.
    Fresh,
    /// The caller discards the result: reuse the byte-keyed script parse memo.
    Memoized,
}

/// What replaying an included file runs.
///
/// Serializable as one unit: this is what the on-disk file cache stores, so a warm start
/// replays exactly what an in-memory hit would.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub(crate) enum ScriptSegment {
    /// The whole file, parsed, ready to execute.
    Code(Arc<EvalProgram>),
    /// A file that failed to parse, raised when an include reaches it.
    ParseError(EvalParseError),
}

impl ScriptSegment {
    /// Returns the byte footprint this segment contributes to the cache budget.
    ///
    /// A parsed file is charged its source length, which is the same accounting
    /// `opcache_get_status()` already applies to the compile-time manifest entries.
    pub(crate) fn memory_footprint(&self) -> usize {
        match self {
            Self::Code(program) => program.source_len(),
            Self::ParseError(_) => 0,
        }
    }
}

/// Parses an included PHP file into the segment list the script cache stores: one segment.
pub(crate) fn segment_script(bytes: &[u8], mode: ParseMode) -> Vec<ScriptSegment> {
    let parsed = match mode {
        ParseMode::Fresh => parse_script(bytes).map(Arc::new),
        ParseMode::Memoized => parse_script_cached(bytes),
    };
    vec![match parsed {
        Ok(program) => ScriptSegment::Code(program),
        Err(error) => ScriptSegment::ParseError(error),
    }]
}

/// Parses a whole PHP file, which starts in HTML mode, into one program.
pub(crate) fn parse_script(bytes: &[u8]) -> Result<EvalProgram, EvalParseError> {
    let mut tokens = Vec::new();
    let mut line: i64 = 1;
    let mut counted = 0;
    for piece in split_script(bytes) {
        line += newlines(&bytes[counted..piece.start()]);
        counted = piece.start();
        match piece {
            ScriptPiece::Html { bytes: html, .. } => push_inline_html(&mut tokens, html, line),
            ScriptPiece::Code { code, echo, closed, .. } => {
                if echo {
                    tokens.push(Token::new(TokenKind::Ident("echo".to_string()), line));
                }
                let source = std::str::from_utf8(code).map_err(|_| EvalParseError::InvalidUtf8)?;
                let mut block = tokenize_at_line(source, line)?;
                block.pop(); // this block's end-of-input sentinel: the file goes on
                tokens.extend(block);
                line += newlines(code);
                counted += code.len();
                if closed {
                    // `?>` ends a statement, as a `;` does.
                    tokens.push(Token::new(TokenKind::Semicolon, line));
                }
            }
        }
    }
    line += newlines(&bytes[counted..]);
    tokens.push(Token::new(TokenKind::Eof, line));
    parser::parse_tokens(tokens, bytes.len())
}

/// One piece of an included file, as its tags cut it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptPiece<'a> {
    /// Bytes outside any PHP block, echoed verbatim.
    Html { start: usize, bytes: &'a [u8] },
    /// The code between an open tag and its `?>` (or the end of the file).
    Code {
        start: usize,
        code: &'a [u8],
        /// Opened by `<?=`, so the block's expression is echoed.
        echo: bool,
        /// Ended by `?>` rather than by the end of the file.
        closed: bool,
    },
}

impl ScriptPiece<'_> {
    /// Returns the piece's byte offset in the file.
    fn start(&self) -> usize {
        match self {
            Self::Html { start, .. } | Self::Code { start, .. } => *start,
        }
    }
}

/// Cuts a PHP file at its open and close tags, dropping the tags and the newline a `?>`
/// swallows. Empty HTML runs are not pieces.
fn split_script(bytes: &[u8]) -> Vec<ScriptPiece<'_>> {
    let mut pieces = Vec::new();
    let mut cursor = 0;
    while let Some(tag) = find_open_tag(bytes, cursor) {
        push_html(&mut pieces, bytes, cursor, tag.start);
        let close = find_php_close_tag(bytes, tag.code_start);
        let code_end = close.unwrap_or(bytes.len());
        pieces.push(ScriptPiece::Code {
            start: tag.code_start,
            code: &bytes[tag.code_start..code_end],
            echo: tag.echo,
            closed: close.is_some(),
        });
        let Some(close) = close else {
            return pieces;
        };
        cursor = close + 2 + swallowed_newline_len(&bytes[close + 2..]);
    }
    push_html(&mut pieces, bytes, cursor, bytes.len());
    pieces
}

/// Appends `bytes[start..end]` as an HTML piece unless it is empty.
fn push_html<'a>(pieces: &mut Vec<ScriptPiece<'a>>, bytes: &'a [u8], start: usize, end: usize) {
    if start < end {
        pieces.push(ScriptPiece::Html { start, bytes: &bytes[start..end] });
    }
}

/// Returns how many bytes of newline a `?>` swallows from `rest`: one `\n`, `\r\n` or `\r`.
fn swallowed_newline_len(rest: &[u8]) -> usize {
    match rest {
        [b'\r', b'\n', ..] => 2,
        [b'\n' | b'\r', ..] => 1,
        _ => 0,
    }
}

/// Counts the `\n` bytes in `bytes`, the line breaks the eval lexer counts.
fn newlines(bytes: &[u8]) -> i64 {
    bytes.iter().filter(|&&byte| byte == b'\n').count() as i64
}

/// Emits `echo "<html>";` for an inline HTML run, keeping its exact bytes.
fn push_inline_html(tokens: &mut Vec<Token>, html: &[u8], line: i64) {
    let mut text = String::with_capacity(html.len());
    for chunk in html.utf8_chunks() {
        for ch in chunk.valid().chars() {
            push_literal_char(ch, &mut text);
        }
        for &byte in chunk.invalid() {
            push_escaped_byte(byte, &mut text);
        }
    }
    tokens.push(Token::new(TokenKind::Ident("echo".to_string()), line));
    tokens.push(Token::new(TokenKind::String(text), line));
    tokens.push(Token::new(TokenKind::Semicolon, line));
}

/// Where an open tag sits and where its code starts.
struct OpenTag {
    start: usize,
    code_start: usize,
    echo: bool,
}

/// Whether `byte` may follow `<?php` for the tag to open code: the Zend scanner accepts a
/// space, a tab, or a line break, and nothing else — not the other Unicode spaces.
///
/// The eval lexer (`crate::lexer::inline_html`) opens tags by the same rule, so a file and an
/// `eval()` string agree on where code starts.
pub(crate) fn is_open_tag_separator(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// Finds the next `<?php` (followed by whitespace or the end of the file) or `<?=` tag.
fn find_open_tag(bytes: &[u8], from: usize) -> Option<OpenTag> {
    let mut at = from;
    while let Some(start) = find_from(bytes, at, b"<?") {
        let after = &bytes[start + 2..];
        if after.first() == Some(&b'=') {
            return Some(OpenTag { start, code_start: start + 3, echo: true });
        }
        if after.len() >= 3
            && after[..3].eq_ignore_ascii_case(b"php")
            && after.get(3).copied().is_none_or(is_open_tag_separator)
        {
            return Some(OpenTag { start, code_start: start + 5, echo: false });
        }
        at = start + 2;
    }
    None
}


/// Finds the `?>` that ENDS the code block starting at `start`, as PHP's lexer does.
///
/// A `?>` inside a single-quoted, double-quoted or backtick string, a `/* */` comment, or a
/// heredoc / nowdoc body is part of that token and does not close the block. One inside a `//`
/// or `#` line comment DOES — PHP's own rule, and why a line comment is scanned for it rather
/// than skipped. A plain byte search split `<?php echo "?>";` at the quote and handed the parser
/// ` echo "` — an unterminated string — so a valid file failed to include and
/// `opcache_compile_file()` refused it. MEASURED: reference prints `a?>b c?>d done` and compiles
/// the file; elephc raised a parse error. The same naive search predates this module (it lived
/// in `include_exec`), so every include was affected, not only the cache.
///
/// An unterminated string or comment runs to the end of the file, as in PHP: there is no
/// closing tag, and the parser reports whatever is wrong with the code.
pub(crate) fn find_php_close_tag(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'?' if bytes.get(i + 1) == Some(&b'>') => return Some(i),
            b'\'' | b'"' | b'`' => match skip_quoted(bytes, i) {
                QuotedScan::Closed(end) => i = end,
                QuotedScan::PhpCloseTag(close) => return Some(close),
                QuotedScan::Unterminated => return None,
            },
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = find_from(bytes, i + 2, b"*/")? + 2;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                if let Some(close) = scan_line_comment(bytes, i + 2) {
                    return Some(close);
                }
                i = next_line(bytes, i + 2);
            }
            // `#[` opens an attribute, which is code; any other `#` is a line comment.
            b'#' if bytes.get(i + 1) != Some(&b'[') => {
                if let Some(close) = scan_line_comment(bytes, i + 1) {
                    return Some(close);
                }
                i = next_line(bytes, i + 1);
            }
            b'<' if bytes[i..].starts_with(b"<<<") => match skip_heredoc(bytes, i + 3) {
                Some(end) => i = end,
                None => i += 3,
            },
            _ => i += 1,
        }
    }
    None
}

/// Result of scanning a quoted token that may contain PHP close tags inside line comments.
enum QuotedScan {
    Closed(usize),
    PhpCloseTag(usize),
    Unterminated,
}

/// Returns the index just past the closing quote of the string opening at `open`, or `None`
/// when it never closes. A backslash escapes the next byte in all three quote styles.
///
/// Double-quoted and backtick strings INTERPOLATE, and a complex interpolation `{$expr}` (or
/// `${expr}`) may contain a string of the SAME quote style: `"{$a["?>"]}"` is valid PHP. Ending
/// the outer string at that inner quote left the `?>` inside it looking like code, and the block
/// was cut there. MEASURED: reference runs `echo "{$a["?>"]}";` and prints the value; elephc
/// raised a parse error. The interpolation is skipped as a balanced `{...}`, its own strings
/// included, before the outer string resumes. Found by GLM.
fn skip_quoted(bytes: &[u8], open: usize) -> QuotedScan {
    let quote = bytes[open];
    let interpolates = quote != b'\'';
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'{' if interpolates && bytes.get(i + 1) == Some(&b'$') => {
                match skip_interpolation(bytes, i) {
                    QuotedScan::Closed(end) => i = end,
                    other => return other,
                }
            }
            b'$' if interpolates && bytes.get(i + 1) == Some(&b'{') => {
                match skip_interpolation(bytes, i + 1) {
                    QuotedScan::Closed(end) => i = end,
                    other => return other,
                }
            }
            byte if byte == quote => return QuotedScan::Closed(i + 1),
            _ => i += 1,
        }
    }
    QuotedScan::Unterminated
}

/// Returns the index just past the `}` that balances the `{` at `open`, skipping any string
/// inside the braces. `None` when it never balances, which leaves the string unterminated —
/// the parser then reports what is wrong, as PHP does.
fn skip_interpolation(bytes: &[u8], open: usize) -> QuotedScan {
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return QuotedScan::Closed(i);
                }
            }
            b'\'' | b'"' | b'`' => match skip_quoted(bytes, i) {
                QuotedScan::Closed(end) => i = end,
                other => return other,
            },
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                match find_from(bytes, i + 2, b"*/") {
                    Some(end) => i = end + 2,
                    None => return QuotedScan::Unterminated,
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                if let Some(close) = scan_line_comment(bytes, i + 2) {
                    return QuotedScan::PhpCloseTag(close);
                }
                i = next_line(bytes, i + 2);
            }
            b'#' if bytes.get(i + 1) != Some(&b'[') => {
                if let Some(close) = scan_line_comment(bytes, i + 1) {
                    return QuotedScan::PhpCloseTag(close);
                }
                i = next_line(bytes, i + 1);
            }
            _ => i += 1,
        }
    }
    QuotedScan::Unterminated
}

/// Returns the offset of `needle` at or after `from`.
fn find_from(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

/// Returns the `?>` that ends a line comment before its newline, if there is one.
fn scan_line_comment(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() && bytes[i] != b'\n' {
        if bytes[i] == b'?' && bytes.get(i + 1) == Some(&b'>') {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Returns the index just past the newline ending the line that contains `from`.
fn next_line(bytes: &[u8], from: usize) -> usize {
    find_from(bytes, from, b"\n").map_or(bytes.len(), |newline| newline + 1)
}

/// Whether `byte` can continue a PHP label.
fn is_label_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// Skips a heredoc or nowdoc whose `<<<` ends at `after_marker`, returning the index just past
/// its closing label. `None` when this is not a heredoc opener — the caller then treats `<<<`
/// as ordinary code. The closing label may be indented and is followed by any non-label byte,
/// as PHP 7.3+ allows.
fn skip_heredoc(bytes: &[u8], after_marker: usize) -> Option<usize> {
    let mut i = after_marker;
    while matches!(bytes.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    let quote = match bytes.get(i) {
        Some(&q @ (b'\'' | b'"')) => {
            i += 1;
            Some(q)
        }
        _ => None,
    };
    let label_start = i;
    while bytes.get(i).is_some_and(|&byte| is_label_byte(byte)) {
        i += 1;
    }
    let label = &bytes[label_start..i];
    if label.is_empty() || label[0].is_ascii_digit() {
        return None;
    }
    if let Some(q) = quote {
        if bytes.get(i) != Some(&q) {
            return None;
        }
        i += 1;
    }
    if bytes.get(i) == Some(&b'\r') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'\n') {
        return None;
    }
    let mut line = i + 1;
    while line < bytes.len() {
        let mut j = line;
        while matches!(bytes.get(j), Some(b' ' | b'\t')) {
            j += 1;
        }
        if bytes[j..].starts_with(label)
            && !bytes.get(j + label.len()).is_some_and(|&byte| is_label_byte(byte))
        {
            return Some(j + label.len());
        }
        line = next_line(bytes, line);
    }
    // Never closed: the body runs to the end of the file, so there is no closing tag.
    Some(bytes.len())
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins where the tags cut a file (tagless files, literal output around blocks, several
    //! blocks, a file ending inside PHP, close tags hidden in strings and comments) and that
    //! the pieces then parse as ONE program.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The cut is asserted on the PIECE LIST, not just "it parses": a piece in the wrong
    //!   place still parses, and prints the wrong thing.

    use super::*;

    /// Parses a fixture without touching the process-wide byte-keyed parse memo.
    fn segment_script_fresh(bytes: &[u8]) -> Vec<ScriptSegment> {
        segment_script(bytes, ParseMode::Fresh)
    }

    /// Returns a compact description of where the tags cut `bytes`, for shape assertions.
    fn shape(bytes: &[u8]) -> Vec<String> {
        split_script(bytes)
            .iter()
            .map(|piece| match piece {
                ScriptPiece::Html { bytes, .. } => {
                    format!("out({})", String::from_utf8_lossy(bytes))
                }
                ScriptPiece::Code { echo: true, .. } => "echo".to_string(),
                ScriptPiece::Code { .. } => "code".to_string(),
            })
            .collect()
    }

    /// Returns whether a fixture parsed as one program.
    fn parses(bytes: &[u8]) -> bool {
        matches!(segment_script_fresh(bytes).as_slice(), [ScriptSegment::Code(_)])
    }

    /// Verifies a `?>` inside a string, a block comment or a heredoc does NOT close PHP mode.
    ///
    /// A plain byte search split `<?php echo "?>";` at the quote and handed the parser an
    /// unterminated string, so a valid file could not be included or compiled. MEASURED:
    /// reference runs `<?php echo "a?>b", 'c?>d';` and prints both strings whole.
    #[test]
    fn a_quoted_close_tag_does_not_end_the_block() {
        let sources: [&[u8]; 3] = [
            b"<?php echo \"a?>b\"; ?>tail",
            b"<?php echo 'c?>d'; ?>tail",
            b"<?php /* ?> */ $x = 1; ?>tail",
        ];
        for source in sources {
            assert_eq!(
                shape(source),
                ["code", "out(tail)"],
                "{}",
                String::from_utf8_lossy(source)
            );
        }
        // Three more shapes, checked on the SEARCH rather than the segment shape: the eval
        // parser accepts neither shell-exec backticks nor heredoc/nowdoc (a separate gap —
        // even a heredoc with no `?>` in it fails there), so those blocks would be parse
        // errors either way. What matters here is that their `?>` is not taken as the close.
        let searched: [(&[u8], usize); 3] = [
            (b"<?php $s = `echo ?>`; ?>tail", 22),
            (b"<?php $h = <<<EOT\n?>\nEOT;\n?>tail", 26),
            (b"<?php $n = <<<'EOT'\n  ?>\n  EOT;\n?>tail", 32),
        ];
        for (source, close) in searched {
            assert_eq!(
                find_php_close_tag(source, 5),
                Some(close),
                "{}",
                String::from_utf8_lossy(source)
            );
        }
    }

    /// Verifies a `?>` inside a string NESTED in `{$...}` / `${...}` interpolation stays in the
    /// string.
    ///
    /// `"{$a["?>"]}"` is valid PHP: the interpolation holds a string of the same quote style.
    /// Ending the outer string at that inner quote exposed the `?>` as code. MEASURED:
    /// reference prints the value; elephc raised a parse error. Found by GLM, whose own input
    /// `"{$a["k"]}"` happened to balance and worked before — the `?>` is what breaks it.
    #[test]
    fn a_comment_inside_an_interpolation_is_inert() {
        // MEASURED on reference PHP 8.5.10: each prints `v` then the trailing `X`.
        let sources: [&[u8]; 4] = [
            b"<?php $a = ['k' => 'v']; echo \"{$a[/* \" */ \"k\"]}\"; ?>X",
            b"<?php $a = ['k' => 'v']; echo \"{$a[/* } ?> */ \"k\"]}\"; ?>X",
            b"<?php $a = ['k' => 'v']; echo \"{$a[ // \" }\n\"k\"]}\"; ?>X",
            b"<?php $a = ['k' => 'v']; echo \"{$a[ # \" }\n\"k\"]}\"; ?>X",
        ];
        for source in sources {
            assert_eq!(
                shape(source),
                ["code", "out(X)"],
                "{}",
                String::from_utf8_lossy(source)
            );
        }
    }

    /// Verifies a `?>` inside a string interpolation's array key does not close the PHP block.
    #[test]
    fn interpolation_hides_a_nested_close_tag() {
        let sources: [&[u8]; 3] = [
            b"<?php $a = ['?>' => 1]; echo \"{$a[\"?>\"]}\"; ?>tail",
            b"<?php $a = ['?>' => 1]; echo \"x{$a[\"?>\"]}y\"; ?>tail",
            b"<?php $a = ['k' => ['?>' => 1]]; echo \"{$a['k'][\"?>\"]}\"; ?>tail",
        ];
        for source in sources {
            assert_eq!(
                shape(source),
                ["code", "out(tail)"],
                "{}",
                String::from_utf8_lossy(source)
            );
        }
        let dollar_brace: &[u8] = b"<?php echo \"${a[\"?>\"]}\"; ?>tail";
        assert_eq!(
            find_php_close_tag(dollar_brace, 5),
            Some(dollar_brace.len() - 6),
            "the `${{...}}` form is skipped the same way"
        );
    }

    /// Delimiters and quotes inside interpolation comments do not affect string balancing.
    #[test]
    fn interpolation_comments_hide_quotes_braces_and_close_tags() {
        let source: &[u8] = b"<?php $a = ['ok']; echo \"{$a[ /* quote: \\\" brace: } close: ?> */ 0]}\"; ?>tail";
        assert_eq!(
            shape(source),
            ["code", "out(tail)"],
            "{}",
            String::from_utf8_lossy(source)
        );
        assert_eq!(
            find_php_close_tag(source, 5),
            Some(source.len() - 6),
            "the block comment's quote, brace, and close tag are all inert"
        );

        let line_comment: &[u8] = b"<?php echo \"{$a[ // ?>\n 0]}\"; ?>tail";
        let line_comment_close = find_from(line_comment, 5, b"?>").unwrap();
        assert_eq!(
            find_php_close_tag(line_comment, 5),
            Some(line_comment_close),
            "a close tag in an interpolation line comment still ends PHP mode"
        );
    }

    /// Verifies a `?>` inside a `//` or `#` line comment DOES close PHP mode, as PHP specifies,
    /// while `#[` opens an attribute rather than a comment. MEASURED: reference ends the block
    /// at a line comment's `?>` and prints the rest of the line.
    #[test]
    fn a_line_comment_close_tag_ends_the_block() {
        assert_eq!(
            shape(b"<?php // note ?>after"),
            ["code", "out(after)"]
        );
        assert_eq!(
            shape(b"<?php # note ?>after"),
            ["code", "out(after)"]
        );
        assert_eq!(
            shape(b"<?php #[Attr('?>')] function f() {} ?>after"),
            ["code", "out(after)"],
            "an attribute's quoted `?>` is inside a string, not a comment"
        );
    }

    /// Verifies a file with no PHP tag is one literal output piece.
    #[test]
    fn a_tagless_file_is_pure_output() {
        assert_eq!(shape(b"plain text"), ["out(plain text)"]);
    }

    /// Verifies an empty file has no pieces, and still parses (to nothing).
    #[test]
    fn an_empty_file_has_no_pieces() {
        assert!(split_script(b"").is_empty());
        assert!(parses(b""));
    }

    /// Verifies literal text around a code block becomes output pieces on both sides.
    #[test]
    fn output_surrounds_a_closed_code_block() {
        assert_eq!(shape(b"A<?php $x = 1; ?>B"), ["out(A)", "code", "out(B)"]);
    }

    /// Verifies a file ending inside a code block has no trailing output piece.
    #[test]
    fn an_unclosed_final_block_has_no_trailing_output() {
        assert_eq!(shape(b"A<?php $x = 1;"), ["out(A)", "code"]);
    }

    /// Verifies several blocks alternate with the literal runs between them.
    #[test]
    fn several_blocks_alternate_with_their_separators() {
        assert_eq!(
            shape(b"<?php $a = 1; ?>mid<?php $b = 2; ?>end"),
            ["code", "out(mid)", "code", "out(end)"]
        );
    }

    /// Verifies two adjacent blocks produce no empty output piece between them.
    #[test]
    fn adjacent_blocks_produce_no_empty_output() {
        assert_eq!(shape(b"<?php $a = 1; ?><?php $b = 2; ?>"), ["code", "code"]);
    }

    /// Verifies the opening tag is matched case-insensitively, as the scanner did.
    #[test]
    fn the_open_tag_is_case_insensitive() {
        assert_eq!(shape(b"<?PHP $a = 1; ?>"), ["code"]);
    }

    /// Verifies `<?php` opens code only before whitespace or the end of the file, as PHP 8.5's
    /// scanner requires, and `<?=` opens an echo while a bare `<?` stays HTML.
    ///
    /// MEASURED on reference PHP 8.5.10: `<?phpX not code ?>` is printed verbatim, and
    /// `<? echo 'x'; ?>` too (`short_open_tag` is off).
    #[test]
    fn open_tags_follow_the_php_scanner() {
        assert_eq!(shape(b"<?phpX not code ?>"), ["out(<?phpX not code ?>)"]);
        assert_eq!(shape(b"a<?php"), ["out(a)", "code"]);
        assert_eq!(shape(b"a<?php\tb();"), ["out(a)", "code"]);
        assert_eq!(shape(b"a<?= $x ?>b"), ["out(a)", "echo", "out(b)"]);
        assert_eq!(shape(b"<? echo 1; ?>"), ["out(<? echo 1; ?>)"]);
        assert_eq!(
            shape("a<?php\u{a0}b".as_bytes()),
            ["out(a<?php\u{a0}b)"],
            "a no-break space is not an open-tag separator"
        );
    }

    /// Verifies `?>` swallows exactly one newline after it, as the Zend scanner does.
    ///
    /// MEASURED: reference prints `aB` for `<?php echo 'a'; ?>` then a newline then `B`, for
    /// `\n`, `\r\n` and `\r` alike. The newline used to be printed.
    #[test]
    fn a_close_tag_swallows_one_newline() {
        assert_eq!(shape(b"<?php ?>\nB"), ["code", "out(B)"]);
        assert_eq!(shape(b"<?php ?>\r\nB"), ["code", "out(B)"]);
        assert_eq!(shape(b"<?php ?>\rB"), ["code", "out(B)"]);
        assert_eq!(shape(b"<?php ?>\n\nB"), ["code", "out(\nB)"]);
    }

    /// Verifies a block that spans the tags parses, which parsing each block alone could not.
    ///
    /// MEASURED on reference PHP 8.5.10: the list prints `<ul><li>1</li><li>2</li></ul>`, and
    /// the alternative syntax and a function body spanning blocks run too. Each used to be a
    /// parse error when included.
    #[test]
    fn a_block_spanning_the_tags_parses_as_one_program() {
        assert!(parses(b"<ul><?php foreach ([1, 2] as $r) { ?><li><?= $r ?></li><?php } ?></ul>"));
        assert!(parses(b"<?php if (true): ?>yes<?php else: ?>no<?php endif; ?>"));
        assert!(parses(b"<?php function row($v) { ?><td><?= $v ?></td><?php } ?>"));
    }

    /// Verifies inline HTML keeps bytes that are not UTF-8: only the PHP blocks are lexed.
    #[test]
    fn inline_html_may_hold_any_bytes() {
        assert!(parses(b"caf\xe9 <?php echo 1; ?> \xff"));
    }

    /// Verifies a parse error anywhere fails the WHOLE file, before any of it runs.
    ///
    /// php-src compiles a whole script before running it. Replay used to run the blocks ahead
    /// of the broken one first: MEASURED, reference prints nothing but the parse error for
    /// `before<?php echo 'ran'; ?>middle<?php echo ; ?>`, elephc printed `beforeranmiddle`.
    #[test]
    fn a_parse_error_anywhere_fails_the_whole_file() {
        assert!(matches!(
            segment_script_fresh(b"A<?php echo 1; ?>B<?php $ ?>C").as_slice(),
            [ScriptSegment::ParseError(_)]
        ));
    }

    /// Verifies a parsed file is charged its whole source length in the cache budget.
    #[test]
    fn a_parsed_file_is_charged_its_source_length() {
        let source = b"A<?php $x = 1; ?>B";
        let segments = segment_script_fresh(source);

        assert_eq!(segments[0].memory_footprint(), source.len());
    }
}
