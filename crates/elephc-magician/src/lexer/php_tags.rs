//! Purpose:
//! Locates PHP open and close tags without confusing bytes inside lexical content for syntax.
//! This small scanner serves source-block splitting and eval-fragment validation before EvalIR
//! tokenization starts.
//!
//! Called from:
//! - `crate::interpreter::include_exec` while splitting included PHP source blocks.
//! - `crate::parser::parse_fragment()` while rejecting actual PHP opening tags in eval input.
//!
//! Key details:
//! - Quoted strings, backticks, block comments, and heredocs hide tag-shaped byte sequences.
//! - A closing tag ends a line comment, matching PHP source-mode comment semantics.

/// Returns whether an eval fragment contains an actual PHP opening tag IN PHP MODE.
///
/// Eval'd code starts in PHP mode, and a `?>` switches it to inline HTML until the next
/// `<?php` / `<?=`, exactly as in a file, so an open tag after a close tag is legal:
/// Twig's `eval('?>' . $compiledTemplate)` starts with `?><?php`. Only a `<?` met while in
/// PHP mode is the syntax error php reports (`eval('<?php echo 1;')`).
pub(crate) fn contains_php_open_tag(bytes: &[u8]) -> bool {
    let mut cursor = 0;
    loop {
        let Some(open) = find_php_tag(bytes, cursor, b"<?", false) else {
            return false;
        };
        match find_php_tag(bytes, cursor, b"?>", true) {
            Some(close) if close < open => match html_open_tag_code_start(bytes, close + 2) {
                Some(code_start) => cursor = code_start,
                None => return false,
            },
            _ => return true,
        }
    }
}

/// Byte offset where code resumes after the next open tag in inline HTML starting at `start`.
///
/// Uses the eval lexer's rule (`super::inline_html`): `<?php` followed by an ASCII separator
/// or the end of input, case-insensitively, or `<?=`; a short `<?` is HTML.
fn html_open_tag_code_start(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    while index + 1 < bytes.len() {
        if bytes[index] == b'<' && bytes[index + 1] == b'?' {
            if bytes.get(index + 2) == Some(&b'=') {
                return Some(index + 3);
            }
            if bytes
                .get(index + 2..index + 5)
                .is_some_and(|word| word.eq_ignore_ascii_case(b"php"))
                && bytes
                    .get(index + 5)
                    .copied()
                    .is_none_or(crate::script_cache::segments::is_open_tag_separator)
            {
                return Some(index + 5);
            }
        }
        index += 1;
    }
    None
}

/// Finds the next PHP closing tag at code level after `start`.
pub(crate) fn find_php_close_tag(bytes: &[u8], start: usize) -> Option<usize> {
    find_php_tag(bytes, start, b"?>", true)
}

/// Tracks lexical regions where PHP tag-shaped bytes are ordinary source content.
enum PhpTagScanState {
    Code,
    Quoted { quote: u8, escaped: bool },
    LineComment,
    BlockComment,
    Heredoc { label: Vec<u8> },
}

/// Finds `tag` only where PHP source treats it as syntax.
fn find_php_tag(
    bytes: &[u8],
    start: usize,
    tag: &[u8],
    tag_ends_line_comment: bool,
) -> Option<usize> {
    let mut index = start;
    let mut state = PhpTagScanState::Code;
    let mut line_start = true;

    while index < bytes.len() {
        match &mut state {
            PhpTagScanState::Code => {
                if bytes[index..].starts_with(tag) {
                    return Some(index);
                }
                match bytes[index] {
                    b'\'' | b'"' | b'`' => {
                        state = PhpTagScanState::Quoted {
                            quote: bytes[index],
                            escaped: false,
                        };
                        line_start = false;
                        index += 1;
                    }
                    b'/' if bytes.get(index + 1) == Some(&b'/') => {
                        state = PhpTagScanState::LineComment;
                        line_start = false;
                        index += 2;
                    }
                    b'#' => {
                        state = PhpTagScanState::LineComment;
                        line_start = false;
                        index += 1;
                    }
                    b'/' if bytes.get(index + 1) == Some(&b'*') => {
                        state = PhpTagScanState::BlockComment;
                        line_start = false;
                        index += 2;
                    }
                    b'<' if bytes[index..].starts_with(b"<<<") => {
                        if let Some((label, body_start)) = parse_heredoc_opening(bytes, index) {
                            state = PhpTagScanState::Heredoc { label };
                            line_start = true;
                            index = body_start;
                        } else {
                            line_start = false;
                            index += 1;
                        }
                    }
                    b'\n' => {
                        line_start = true;
                        index += 1;
                    }
                    _ => {
                        line_start = false;
                        index += 1;
                    }
                }
            }
            PhpTagScanState::Quoted { quote, escaped } => {
                let current = bytes[index];
                if *escaped {
                    *escaped = false;
                } else if current == b'\\' {
                    *escaped = true;
                } else if current == *quote {
                    state = PhpTagScanState::Code;
                }
                line_start = current == b'\n';
                index += 1;
            }
            PhpTagScanState::LineComment => {
                if tag_ends_line_comment && bytes[index..].starts_with(tag) {
                    return Some(index);
                }
                if bytes[index] == b'\n' {
                    state = PhpTagScanState::Code;
                    line_start = true;
                }
                index += 1;
            }
            PhpTagScanState::BlockComment => {
                if bytes[index..].starts_with(b"*/") {
                    state = PhpTagScanState::Code;
                    line_start = false;
                    index += 2;
                } else {
                    line_start = bytes[index] == b'\n';
                    index += 1;
                }
            }
            PhpTagScanState::Heredoc { label } => {
                if line_start {
                    if let Some(after_label) = heredoc_terminator_end(bytes, index, label) {
                        state = PhpTagScanState::Code;
                        line_start = false;
                        index = after_label;
                        continue;
                    }
                }
                line_start = bytes[index] == b'\n';
                index += 1;
            }
        }
    }
    None
}

/// Parses a valid heredoc/nowdoc opener and returns its terminator label and body offset.
fn parse_heredoc_opening(bytes: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    let mut cursor = start.checked_add(3)?;
    while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
        cursor += 1;
    }
    let quote = match bytes.get(cursor) {
        Some(b'\'' | b'"') => {
            let quote = bytes[cursor];
            cursor += 1;
            Some(quote)
        }
        _ => None,
    };
    let label_start = cursor;
    if !bytes.get(cursor).is_some_and(|byte| is_heredoc_label_start(*byte)) {
        return None;
    }
    cursor += 1;
    while bytes
        .get(cursor)
        .is_some_and(|byte| is_heredoc_label_continue(*byte))
    {
        cursor += 1;
    }
    let label = bytes[label_start..cursor].to_vec();
    if quote.is_some_and(|quote| bytes.get(cursor) != Some(&quote)) {
        return None;
    }
    if quote.is_some() {
        cursor += 1;
    }
    while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'\r') {
        cursor += 1;
    }
    (bytes.get(cursor) == Some(&b'\n')).then_some((label, cursor + 1))
}

/// Returns whether a byte starts a PHP heredoc label.
fn is_heredoc_label_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic() || byte >= 0x80
}

/// Returns whether a byte continues a PHP heredoc label.
fn is_heredoc_label_continue(byte: u8) -> bool {
    is_heredoc_label_start(byte) || byte.is_ascii_digit()
}

/// Returns the first byte after a valid heredoc terminator line at `start`.
fn heredoc_terminator_end(bytes: &[u8], start: usize, label: &[u8]) -> Option<usize> {
    let mut cursor = start;
    while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
        cursor += 1;
    }
    if !bytes.get(cursor..)?.starts_with(label) {
        return None;
    }
    cursor += label.len();
    if bytes.get(cursor) == Some(&b';') {
        cursor += 1;
    }
    match bytes.get(cursor) {
        None | Some(b'\n' | b'\r') => Some(cursor),
        Some(b'?') if bytes.get(cursor + 1) == Some(&b'>') => Some(cursor),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{contains_php_open_tag, find_php_close_tag};

    /// Verifies regex literals cannot truncate an included PHP code block.
    #[test]
    fn close_tag_ignores_regex_string_content() {
        let source = br#"<?php $pattern = '/\\{(?:<.*?>)?\\}/'; echo 'ok'; ?>html"#;
        let expected = source
            .windows(2)
            .rposition(|window| window == b"?>")
            .expect("fixture must contain its actual close tag");

        assert_eq!(find_php_close_tag(source, 5), Some(expected));
    }

    /// Verifies strings, block comments, backticks, and heredocs hide close-tag bytes.
    #[test]
    fn close_tag_ignores_non_code_regions() {
        let source = br#"<?php
$single = '?>';
$double = "escaped \\\"?>";
$command = `?>`;
/* ?> */
$doc = <<<'DOC'
?>
DOC;
echo 'ok'; ?>html"#;
        let expected = source
            .windows(2)
            .rposition(|window| window == b"?>")
            .expect("fixture must contain its actual close tag");

        assert_eq!(find_php_close_tag(source, 5), Some(expected));
    }

    /// Verifies a closing tag ends a line comment, as it does in PHP source mode.
    #[test]
    fn close_tag_ends_line_comment() {
        assert_eq!(find_php_close_tag(b"<?php // ?>html", 5), Some(9));
    }

    /// Verifies open-tag-looking text in lexical content remains valid eval source.
    #[test]
    fn opening_tag_ignores_non_code_regions() {
        let source = br#"echo '<?php'; /* <?php */ $doc = <<<'DOC'
<?php
DOC;
// <?php
echo "done";"#;

        assert!(!contains_php_open_tag(source));
    }

    /// Verifies an actual opening tag still remains detectable in code position.
    #[test]
    fn opening_tag_detects_code_position() {
        assert!(contains_php_open_tag(b"echo 'before'; <?php echo 'after';"));
    }

    /// Verifies an open tag after a close tag is inline-HTML mode switching, not an error.
    #[test]
    fn opening_tag_after_close_tag_is_legal() {
        assert!(!contains_php_open_tag(b"?><?php\nclass A {}\n"));
        assert!(!contains_php_open_tag(b"echo 1; ?>html<?= 2 ?>more<?php echo 3;"));
        assert!(!contains_php_open_tag(b"// c ?>h<?php echo 4;"));
        assert!(contains_php_open_tag(b"?>a<?php echo 5; <?php echo 6;"));
        assert!(!contains_php_open_tag(b"?>a <? short is html"));
    }
}
