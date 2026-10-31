//! Purpose:
//! Ports php-src `php_strip_tags_ex()` with no allowed-tag list, the mode used by
//! `FILTER_SANITIZE_STRING`.
//!
//! Called from:
//! - `crate::sanitize::apply` for the deprecated string/stripped sanitizer.
//!
//! Key details:
//! - The four-state machine matches php-src's handling of tags, processing instructions,
//!   declarations, comments, quoted attributes and a trailing `<?xml` marker.
//! - With no allowed list, bytes inside a tag are dropped and the tag itself is removed.

/// One byte read at `index`, or NUL past the end, mirroring C string reads.
fn at(value: &[u8], index: usize) -> u8 { value.get(index).copied().unwrap_or(0) }

/// Returns whether the six bytes before `index` spell `doctype` case-insensitively.
fn is_doctype_before(value: &[u8], index: usize) -> bool {
    index > 6 && value[index - 6..index].eq_ignore_ascii_case(b"doctype")
}

/// Returns whether the five bytes before `index` spell `<?xml` case-insensitively.
fn is_xml_before(value: &[u8], index: usize) -> bool {
    index > 4
        && value[index - 4] == b'<'
        && value[index - 3] == b'?'
        && value[index - 2].eq_ignore_ascii_case(&b'x')
        && value[index - 1].eq_ignore_ascii_case(&b'm')
}

/// Ports `php_strip_tags_ex(value, len, NULL, 0, true)` byte for byte.
pub(crate) fn strip(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    let mut index = 0usize;
    let mut depth = 0i32;
    let mut in_quote = 0u8;
    let mut last_char = 0u8;
    let mut state = 0u8;
    let mut brackets = 0i32;
    let mut is_xml = false;
    loop {
        if index >= value.len() { break; }
        let byte = value[index];
        match state {
            0 => match byte {
                0 => {}
                b'<' => {
                    if in_quote != 0 { index += 1; continue; }
                    last_char = b'<';
                    state = 1;
                    index += 1;
                    continue;
                }
                b'>' => {
                    if depth != 0 {
                        depth -= 1;
                    } else if in_quote == 0 {
                        out.push(byte);
                    }
                }
                _ => out.push(byte),
            },
            1 => match byte {
                0 => {}
                b'<' => {
                    if in_quote == 0 { depth += 1; }
                }
                b'>' => {
                    if depth != 0 {
                        depth -= 1;
                    } else if in_quote == 0 {
                        last_char = b'>';
                        if is_xml && index >= 1 && value[index - 1] == b'-' {
                            // A `--` before `>` keeps the XML declaration open.
                        } else {
                            in_quote = 0;
                            state = 0;
                            is_xml = false;
                            index += 1;
                            continue;
                        }
                    }
                }
                b'"' | b'\'' => {
                    if index != 0 && (in_quote == 0 || byte == in_quote) {
                        in_quote = if in_quote != 0 { 0 } else { byte };
                    }
                }
                b'!' => {
                    if index >= 1 && value[index - 1] == b'<' {
                        state = 3;
                        last_char = byte;
                        index += 1;
                        continue;
                    }
                }
                b'?' => {
                    if index >= 1 && value[index - 1] == b'<' {
                        brackets = 0;
                        state = 2;
                        index += 1;
                        continue;
                    }
                }
                _ => {}
            },
            2 => match byte {
                b'(' => {
                    if last_char != b'"' && last_char != b'\'' {
                        last_char = b'(';
                        brackets += 1;
                    }
                }
                b')' => {
                    if last_char != b'"' && last_char != b'\'' {
                        last_char = b')';
                        brackets -= 1;
                    }
                }
                b'>' => {
                    if depth != 0 {
                        depth -= 1;
                    } else if in_quote == 0
                        && brackets == 0
                        && index >= 1
                        && last_char != b'"'
                        && value[index - 1] == b'?'
                    {
                        in_quote = 0;
                        state = 0;
                        index += 1;
                        continue;
                    }
                }
                b'"' | b'\'' => {
                    if index >= 1 && value[index - 1] != b'\\' {
                        if last_char == byte {
                            last_char = 0;
                        } else if last_char != b'\\' {
                            last_char = byte;
                        }
                        if index != 0 && (in_quote == 0 || byte == in_quote) {
                            in_quote = if in_quote != 0 { 0 } else { byte };
                        }
                    }
                }
                b'l' | b'L' => {
                    if state == 2 && is_xml_before(value, index) && at(value, index + 1) != 0 {
                        state = 1;
                        is_xml = true;
                        index += 1;
                        continue;
                    }
                }
                _ => {}
            },
            3 => match byte {
                b'>' => {
                    if depth != 0 {
                        depth -= 1;
                    } else if in_quote == 0 {
                        in_quote = 0;
                        state = 0;
                        index += 1;
                        continue;
                    }
                }
                b'"' | b'\'' => {
                    if index != 0 && value[index - 1] != b'\\' && (in_quote == 0 || byte == in_quote) {
                        in_quote = if in_quote != 0 { 0 } else { byte };
                    }
                }
                b'-' => {
                    if index >= 2 && value[index - 1] == b'-' && value[index - 2] == b'!' {
                        state = 4;
                        index += 1;
                        continue;
                    }
                }
                b'E' | b'e' => {
                    if is_doctype_before(value, index) && at(value, index + 1) != 0 {
                        state = 1;
                        index += 1;
                        continue;
                    }
                }
                _ => {}
            },
            4 => {
                if byte == b'>'
                    && in_quote == 0
                    && index >= 2
                    && value[index - 1] == b'-'
                    && value[index - 2] == b'-'
                {
                    in_quote = 0;
                    state = 0;
                    index += 1;
                    continue;
                }
            }
            _ => {}
        }
        index += 1;
    }
    out
}
