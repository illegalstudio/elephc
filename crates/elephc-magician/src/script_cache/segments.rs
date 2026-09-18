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

/// Finds the next PHP closing tag after a code block start.
pub(crate) fn find_php_close_tag(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|window| window == b"?>")
        .map(|offset| start + offset)
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
