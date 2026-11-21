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
//! - Chopper scans a whole file across PHP and HTML tags before Magician lowers it.
//! - Inline HTML retains its original bytes, even if it is not valid UTF-8.
//! - The cache stores a single program or a parse failure for the whole source.
//! - ParseMode determines whether this entry point keeps its own byte-keyed memo.

use crate::errors::EvalParseError;
use crate::eval_ir::EvalProgram;
#[cfg(test)]
use crate::lexer::tokenize_script;
use crate::lexer::tokenize_script_with_semantics;
use crate::parse_cache::parse_script_cached;
use crate::parser;
use elephc_semantics::SemanticProfile;
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
    /// Confirms that a cached AST was normalized under the header's semantic policy.
    pub(crate) fn matches_semantic_profile(&self, profile: SemanticProfile) -> bool {
        match self {
            Self::Code(program) => program.matches_semantic_profile(profile),
            Self::ParseError(_) => true,
        }
    }

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
#[cfg(test)]
pub(crate) fn segment_script(bytes: &[u8], mode: ParseMode) -> Vec<ScriptSegment> {
    segment_script_with_semantics(bytes, mode, SemanticProfile::PERMISSIVE)
}

/// Parses or memoizes a file with the caller's immutable semantic rules.
pub(crate) fn segment_script_with_semantics(
    bytes: &[u8],
    mode: ParseMode,
    profile: SemanticProfile,
) -> Vec<ScriptSegment> {
    let parsed = match mode {
        ParseMode::Fresh => parse_script_with_semantics(bytes, profile).map(Arc::new),
        ParseMode::Memoized => parse_script_cached(bytes, profile),
    };
    vec![match parsed {
        Ok(program) => ScriptSegment::Code(program),
        Err(error) => ScriptSegment::ParseError(error),
    }]
}

/// Parses a whole PHP file under the caller's immutable semantic profile.
pub(crate) fn parse_script_with_semantics(
    bytes: &[u8],
    profile: SemanticProfile,
) -> Result<EvalProgram, EvalParseError> {
    parser::parse_tokens_with_semantics(
        tokenize_script_with_semantics(bytes, profile)?,
        bytes.len(),
        profile,
    )
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
        use elephc_chopper::scanner::{scan_runtime, token_name};

        scan_runtime(bytes, true).unwrap().tokens.iter()
            .filter_map(|token| match token_name(token.id) {
                Some("T_OPEN_TAG") => Some("code".to_string()),
                Some("T_OPEN_TAG_WITH_ECHO") => Some("echo".to_string()),
                Some("T_INLINE_HTML") => Some(format!("out({})",
                    String::from_utf8_lossy(&bytes[token.pos..token.pos + token.len]))),
                _ => None,
            })
            .collect()
    }

    /// Reports the PHP close-tag offset from Chopper's PHP tokenization profile.
    fn first_close_tag(bytes: &[u8]) -> Option<usize> {
        use elephc_chopper::scanner::{scan, token_name};

        scan(bytes).unwrap().tokens.into_iter()
            .find(|token| token_name(token.id) == Some("T_CLOSE_TAG"))
            .map(|token| token.pos)
    }

    /// Returns whether a fixture parsed as one program.
    fn parses(bytes: &[u8]) -> bool {
        matches!(segment_script_fresh(bytes).as_slice(), [ScriptSegment::Code(_)])
    }

    /// Verifies the shared file scan keeps tag transitions and magic constant lines.
    #[test]
    fn chopper_file_scan_preserves_tokens_across_tags() {
        let sources: [&[u8]; 6] = [
            b"before\n<?php if ($enabled) { ?>\r\n<?=(__LINE__)?>\nafter <?php }",
            b"<?php echo 'a?>b'; /* ?> */ ?>\nnext <?= __LINE__ ?>",
            b"<?php // comment ?>\nhtml <?PHP echo __LINE__;",
            b"plain HTML with <?phpX and <? ignored\n",
            b"<?php namespace App; ?><?php echo __NAMESPACE__;",
            b"\xff<?php echo \"line {$name}\"; ?>\xfe<?= __LINE__ ?>",
        ];
        for source in sources {
            assert!(parses(source), "{source:?}");
        }
        let tokens = tokenize_script(b"before\n<?php echo __LINE__; ?>\n<?=__LINE__?>").unwrap();
        let lines: Vec<_> = tokens.iter().filter_map(|token| match token.kind() {
            crate::lexer::TokenKind::Magic(crate::eval_ir::EvalMagicConst::Line(line)) => Some(*line),
            _ => None,
        }).collect();
        assert_eq!(lines, [2, 3]);
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
                first_close_tag(source),
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
            first_close_tag(dollar_brace),
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
            first_close_tag(source),
            Some(source.len() - 6),
            "the block comment's quote, brace, and close tag are all inert"
        );

        let line_comment: &[u8] = b"<?php echo \"{$a[ // ?>\n 0]}\"; ?>tail";
        let line_comment_close = line_comment.windows(2).position(|bytes| bytes == b"?>").unwrap();
        assert_eq!(
            first_close_tag(line_comment),
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
        assert!(shape(b"").is_empty());
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
