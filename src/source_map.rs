//! Purpose:
//! Writes a compact JSON source map for generated assembly when requested by the CLI.
//! Captures source filename, assembly path, and line mappings emitted by codegen.
//!
//! Called from:
//! - `crate::pipeline::compile()` after user assembly generation.
//!
//! Key details:
//! - JSON escaping is kept local so source paths and generated snippets remain valid map data.

use std::fs;
use std::path::Path;

/// Parses assembly comments in the form `@src line=<php_line> col=<php_col>` emitted
/// by codegen, builds a JSON source map mapping each assembly line to its PHP source
/// position, and writes the result to `output_path`.
///
/// `asm` is the full generated assembly text. `source_path` is the original PHP file.
/// The output JSON uses the format `elephc-source-map-v1`.
pub fn write_source_map(
    asm: &str,
    source_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let mut entries = Vec::new();

    for (idx, line) in asm.lines().enumerate() {
        let Some(marker) = line.split("@src line=").nth(1) else {
            continue;
        };
        let Some((line_part, col_part)) = marker.split_once(" col=") else {
            continue;
        };
        let php_line = line_part.trim().parse::<usize>().map_err(|err| {
            format!("invalid source-map line marker '{}': {}", line_part.trim(), err)
        })?;
        let php_col = col_part.trim().parse::<usize>().map_err(|err| {
            format!("invalid source-map column marker '{}': {}", col_part.trim(), err)
        })?;
        entries.push((idx + 1, php_line, php_col));
    }

    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"format\": \"elephc-source-map-v1\",\n");
    json.push_str(&format!(
        "  \"source\": \"{}\",\n",
        escape_json(&source_path.display().to_string())
    ));
    json.push_str("  \"entries\": [\n");
    for (idx, (asm_line, php_line, php_col)) in entries.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"asm_line\": {}, \"php_line\": {}, \"php_col\": {}}}",
            asm_line, php_line, php_col
        ));
        if idx + 1 != entries.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  ]\n");
    json.push_str("}\n");

    fs::write(output_path, json)
        .map_err(|err| format!("failed to write source map '{}': {}", output_path.display(), err))
}

/// Escapes a string for use inside a JSON value: backslashes, quotes, and control
/// characters are escaped so the result is valid JSON text (RFC 8259 §7). All control
/// characters below 0x20 are escaped — `\b`/`\f`/`\n`/`\r`/`\t` use their short forms,
/// any other control char uses a `\u00XX` sequence.
fn escape_json(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies control characters below 0x20 are escaped per RFC 8259: backspace/form-feed
    /// use their short forms and other control chars (vertical tab, ESC) use `\u00XX`, so a
    /// path containing such bytes still produces valid JSON instead of raw control bytes.
    #[test]
    fn escapes_control_characters() {
        let escaped = escape_json("a\u{08}b\u{0c}c\u{0b}d\u{1b}e");
        assert_eq!(escaped, "a\\bb\\fc\\u000bd\\u001be");
    }

    /// Verifies the previously handled cases (backslash, quote, newline, tab) still escape.
    #[test]
    fn escapes_quotes_backslashes_and_whitespace() {
        let escaped = escape_json("a\\b\"c\nd\te");
        assert_eq!(escaped, "a\\\\b\\\"c\\nd\\te");
    }
}
