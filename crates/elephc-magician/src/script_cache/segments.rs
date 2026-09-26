//! Purpose:
//! Splits an included PHP file into the alternating literal-output and parsed-code
//! segments the interpreter replays. This is the unit the runtime script cache
//! stores: doing it once per file replaces the per-include `<?php`/`?>` byte scan
//! and the per-block source hash the byte-keyed eval parse cache needed.
//!
//! Called from:
//! - `crate::script_cache::store` when filling an entry.
//! - `crate::interpreter::include_exec` for the uncached path.
//!
//! Key details:
//! - Segmentation is an exact refactor of the loop `eval_execute_include_bytes` used
//!   to run inline: same open/close tag scan, same handling of a file that ends
//!   inside a code block (no trailing output segment), same `int(1)` result.
//! - Parsing is EAGER over the whole file, as php-src compiles a whole script. A
//!   parse failure is STORED, never raised here: the original code only reached a
//!   later block by executing the earlier ones, so the error must still surface at
//!   replay position, not at fill time.
//! - Empty output runs are dropped at build time because `eval_echo_include_bytes`
//!   returned early on them; keeping them would add no-op echo hooks.
//! - [`ParseMode`] is load-bearing, not a tuning knob. With the script cache OFF this
//!   segmentation runs on EVERY include, so it must keep using the byte-keyed parse
//!   memo the inline loop used; dropping it regressed a 63 KiB include from 1.58 ms
//!   to 5.38 ms. With the cache ON the script entry IS the memo, and routing through
//!   the byte-keyed cache as well would only pin a second copy of every fragment.

use crate::errors::EvalParseError;
use crate::eval_ir::EvalProgram;
use crate::parse_cache::parse_fragment_cached;
use crate::parser;
use std::sync::Arc;

/// Whether block parses should go through the byte-keyed eval parse memo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParseMode {
    /// The caller keeps the result: parse directly, memoizing nothing.
    Fresh,
    /// The caller discards the result: reuse the byte-keyed parse cache.
    Memoized,
}

/// One replayable piece of an included file.
///
/// Serializable as one unit: this is what the on-disk file cache stores, so a warm start
/// replays exactly the segments an in-memory hit would.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub(crate) enum ScriptSegment {
    /// Literal bytes outside any `<?php … ?>` block, echoed verbatim.
    Output(Arc<[u8]>),
    /// A parsed code block, ready to execute.
    Code(Arc<EvalProgram>),
    /// A code block that failed to parse, raised only if replay reaches it.
    ParseError(EvalParseError),
}

impl ScriptSegment {
    /// Returns the byte footprint this segment contributes to the cache budget.
    ///
    /// Output segments own their bytes; a parsed block is charged its source length,
    /// which is the same accounting `opcache_get_status()` already applies to the
    /// compile-time manifest entries.
    pub(crate) fn memory_footprint(&self) -> usize {
        match self {
            Self::Output(bytes) => bytes.len(),
            Self::Code(program) => program.source_len(),
            Self::ParseError(_) => 0,
        }
    }
}

/// Splits a PHP source file into replayable segments, parsing every code block.
pub(crate) fn segment_script(bytes: &[u8], mode: ParseMode) -> Vec<ScriptSegment> {
    let mut segments = Vec::new();
    let mut cursor = 0;
    while let Some((tag_start, code_start)) = find_php_open_tag(bytes, cursor) {
        push_output(&mut segments, &bytes[cursor..tag_start]);
        let close = find_php_close_tag(bytes, code_start);
        let code_end = close.unwrap_or(bytes.len());
        segments.push(parse_block(&bytes[code_start..code_end], mode));
        // A file that ends inside a code block emits no trailing output: the original
        // loop returned as soon as it saw a missing close tag.
        let Some(close) = close else {
            return segments;
        };
        cursor = close + 2;
    }
    push_output(&mut segments, &bytes[cursor..]);
    segments
}

/// Parses one code block into a segment, storing rather than raising a failure.
fn parse_block(code: &[u8], mode: ParseMode) -> ScriptSegment {
    let parsed = match mode {
        ParseMode::Fresh => parser::parse_fragment(code).map(Arc::new),
        ParseMode::Memoized => parse_fragment_cached(code),
    };
    match parsed {
        Ok(program) => ScriptSegment::Code(program),
        Err(error) => ScriptSegment::ParseError(error),
    }
}

/// Appends an output segment unless the run is empty.
fn push_output(segments: &mut Vec<ScriptSegment>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    segments.push(ScriptSegment::Output(Arc::from(bytes)));
}

/// Finds the next `<?php` opening tag and returns tag and code byte offsets.
pub(crate) fn find_php_open_tag(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    bytes
        .get(start..)?
        .windows(5)
        .position(is_php_open_tag)
        .map(|offset| {
            let tag_start = start + offset;
            (tag_start, tag_start + 5)
        })
}

/// Returns true when a five-byte window is a case-insensitive `<?php` tag.
fn is_php_open_tag(window: &[u8]) -> bool {
    window.len() == 5
        && window[0] == b'<'
        && window[1] == b'?'
        && window[2].eq_ignore_ascii_case(&b'p')
        && window[3].eq_ignore_ascii_case(&b'h')
        && window[4].eq_ignore_ascii_case(&b'p')
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
    //! Pins the segmentation against the shapes the inline loop used to handle:
    //! tagless files, leading and trailing literal output, several code blocks, a
    //! file ending inside PHP, and a block that does not parse.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These assert the SEGMENT LIST, not just "no panic": the replay in
    //!   `include_exec` is only correct if the shape matches the original loop.

    use super::*;

    /// Segments a fixture without touching the process-wide byte-keyed parse memo.
    fn segment_script_fresh(bytes: &[u8]) -> Vec<ScriptSegment> {
        segment_script(bytes, ParseMode::Fresh)
    }

    /// Returns a compact description of a segment list, for shape assertions.
    fn shape(segments: &[ScriptSegment]) -> Vec<String> {
        segments
            .iter()
            .map(|segment| match segment {
                ScriptSegment::Output(bytes) => {
                    format!("out({})", String::from_utf8_lossy(bytes))
                }
                ScriptSegment::Code(_) => "code".to_string(),
                ScriptSegment::ParseError(error) => format!("err({error:?})"),
            })
            .collect()
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
                shape(&segment_script_fresh(source)),
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
                shape(&segment_script_fresh(source)),
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
                shape(&segment_script_fresh(source)),
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
            shape(&segment_script_fresh(source)),
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
            shape(&segment_script_fresh(b"<?php // note ?>after")),
            ["code", "out(after)"]
        );
        assert_eq!(
            shape(&segment_script_fresh(b"<?php # note ?>after")),
            ["code", "out(after)"]
        );
        assert_eq!(
            shape(&segment_script_fresh(b"<?php #[Attr('?>')] function f() {} ?>after")),
            ["code", "out(after)"],
            "an attribute's quoted `?>` is inside a string, not a comment"
        );
    }

    /// Verifies a file with no PHP tag is one literal output segment.
    #[test]
    fn a_tagless_file_is_pure_output() {
        assert_eq!(shape(&segment_script_fresh(b"plain text")), ["out(plain text)"]);
    }

    /// Verifies an empty file produces no segments at all.
    #[test]
    fn an_empty_file_has_no_segments() {
        assert!(segment_script_fresh(b"").is_empty());
    }

    /// Verifies literal text around a code block becomes output segments on both sides.
    #[test]
    fn output_surrounds_a_closed_code_block() {
        assert_eq!(
            shape(&segment_script_fresh(b"A<?php $x = 1; ?>B")),
            ["out(A)", "code", "out(B)"]
        );
    }

    /// Verifies a file ending inside a code block emits no trailing output segment.
    ///
    /// This is the case the original loop short-circuited with an early `int(1)`.
    #[test]
    fn an_unclosed_final_block_has_no_trailing_output() {
        assert_eq!(shape(&segment_script_fresh(b"A<?php $x = 1;")), ["out(A)", "code"]);
    }

    /// Verifies several blocks alternate with the literal runs between them.
    #[test]
    fn several_blocks_alternate_with_their_separators() {
        assert_eq!(
            shape(&segment_script_fresh(b"<?php $a = 1; ?>mid<?php $b = 2; ?>end")),
            ["code", "out(mid)", "code", "out(end)"]
        );
    }

    /// Verifies two adjacent blocks produce no empty output segment between them.
    #[test]
    fn adjacent_blocks_produce_no_empty_output() {
        assert_eq!(
            shape(&segment_script_fresh(b"<?php $a = 1; ?><?php $b = 2; ?>")),
            ["code", "code"]
        );
    }

    /// Verifies the opening tag is matched case-insensitively, as the scanner did.
    #[test]
    fn the_open_tag_is_case_insensitive() {
        assert_eq!(shape(&segment_script_fresh(b"<?PHP $a = 1; ?>")), ["code"]);
    }

    /// Verifies a block that does not parse is stored as an error rather than raised.
    ///
    /// Storing it is what lets replay surface the failure at the block's own position,
    /// after the earlier blocks have run and produced their output.
    #[test]
    fn an_unparsable_block_is_stored_not_raised() {
        let segments = segment_script_fresh(b"A<?php $ ?>B");

        assert_eq!(segments.len(), 3);
        assert!(matches!(segments[1], ScriptSegment::ParseError(_)));
    }

    /// Verifies output segments are charged their byte length in the cache budget.
    #[test]
    fn output_segments_are_charged_their_bytes() {
        let segment = ScriptSegment::Output(Arc::from(&b"12345"[..]));

        assert_eq!(segment.memory_footprint(), 5);
    }
}
