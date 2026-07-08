//! Purpose:
//! Writes the v2 JSON source map for generated assembly when requested by the CLI.
//! Captures function ranges (`@fn`/`@endfn`), EIR block labels (`@block`), and
//! instruction mappings (`@src`) emitted by codegen into a stable machine-readable
//! schema, plus a PHP-line → assembly-range inverse index and a source checksum.
//!
//! Called from:
//! - `crate::pipeline::compile()` after user assembly generation.
//!
//! Key details:
//! - The schema contract is documented in `docs/compiling/source-maps.md`:
//!   within version 2 fields are only added, never removed or renamed.
//! - JSON escaping is kept local so source paths and generated snippets remain valid map data.

use std::fs;
use std::path::Path;

/// One `@fn`..`@endfn` region: PHP-level name, assembly entry symbol, the
/// 1-based assembly line range covering the emitted function, and whether the
/// body is compiler-generated rather than user-written PHP.
struct FunctionRange {
    name: String,
    symbol: String,
    asm_start: usize,
    asm_end: Option<usize>,
    synthetic: bool,
}

/// One assembly label line inside a function region. `block` carries the EIR
/// basic-block name when the label was announced by an `@block` marker.
struct LabelEntry {
    name: String,
    asm_line: usize,
    function: usize,
    block: Option<String>,
}

/// One `@src` instruction mapping. Optional fields stay representable for
/// v1-style markers: `php_end_*` require an `end=` marker, `op`/`origin`
/// require their keys, `function` requires an enclosing `@fn` region.
struct MappingEntry {
    asm_line: usize,
    php_line: usize,
    php_col: usize,
    php_end_line: Option<usize>,
    php_end_col: Option<usize>,
    op: Option<String>,
    origin: Option<String>,
    function: Option<usize>,
}

/// Builds the v2 source map for `asm` and writes it to `output_path`.
///
/// `asm` is the full generated assembly text, `source_path` the original PHP
/// file (also read back to compute the `source_sha256` checksum), and
/// `asm_path` the assembly file the map describes.
pub fn write_source_map(
    asm: &str,
    source_path: &Path,
    asm_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let source_sha256 = fs::read(source_path).ok().map(|bytes| sha256_hex(&bytes));
    let json = build_source_map(asm, source_path, asm_path, source_sha256.as_deref())?;
    fs::write(output_path, json)
        .map_err(|err| format!("failed to write source map '{}': {}", output_path.display(), err))
}

/// Parses codegen markers out of the assembly text and renders the v2 JSON map.
///
/// Recognized markers (comment-prefix agnostic, matched by substring; values
/// are space-separated `key=value` tokens):
/// - `@fn name=<php_name> symbol=<entry_symbol> [synthetic=1]` opens a function
///   region; `@endfn` closes it (an unterminated region closes at the last line);
/// - `@block name=<eir_block>` names the EIR basic block of the next label line;
/// - `@src line=<L> col=<C> [end=<EL>:<EC>] [op=<opcode>] [origin=<pass>]`
///   maps the assembly emitted for one EIR instruction.
///
/// Label lines (`<ident>:` at column 0) inside a function region are recorded
/// as label entries; labels outside any region (runtime glue, data) are skipped.
fn build_source_map(
    asm: &str,
    source_path: &Path,
    asm_path: &Path,
    source_sha256: Option<&str>,
) -> Result<String, String> {
    let mut functions: Vec<FunctionRange> = Vec::new();
    let mut labels: Vec<LabelEntry> = Vec::new();
    let mut mappings: Vec<MappingEntry> = Vec::new();
    let mut open_function: Option<usize> = None;
    let mut pending_block: Option<String> = None;
    let mut last_line = 0;

    for (idx, line) in asm.lines().enumerate() {
        let asm_line = idx + 1;
        last_line = asm_line;

        if let Some(marker) = line.split("@fn ").nth(1) {
            let name = marker_value(marker, "name").ok_or_else(|| {
                format!("invalid source-map function marker '{}'", line.trim())
            })?;
            let symbol = marker_value(marker, "symbol").ok_or_else(|| {
                format!("invalid source-map function marker '{}'", line.trim())
            })?;
            if let Some(open) = open_function.take() {
                functions[open].asm_end = Some(asm_line.saturating_sub(1));
            }
            open_function = Some(functions.len());
            functions.push(FunctionRange {
                name: name.to_string(),
                symbol: symbol.to_string(),
                asm_start: asm_line,
                asm_end: None,
                synthetic: marker_value(marker, "synthetic") == Some("1"),
            });
            pending_block = None;
            continue;
        }

        if line.contains("@endfn") {
            if let Some(open) = open_function.take() {
                functions[open].asm_end = Some(asm_line);
            }
            pending_block = None;
            continue;
        }

        if let Some(marker) = line.split("@block ").nth(1) {
            pending_block = marker_value(marker, "name").map(str::to_string);
            continue;
        }

        if let Some(marker) = line.split("@src ").nth(1) {
            let php_line = parse_marker_number(marker, "line")?;
            let php_col = parse_marker_number(marker, "col")?;
            let (php_end_line, php_end_col) = match marker_value(marker, "end") {
                Some(end) => {
                    let (end_line, end_col) = end.split_once(':').ok_or_else(|| {
                        format!("invalid source-map end marker '{}'", end)
                    })?;
                    let end_line = end_line.parse::<usize>().map_err(|err| {
                        format!("invalid source-map end marker '{}': {}", end, err)
                    })?;
                    let end_col = end_col.parse::<usize>().map_err(|err| {
                        format!("invalid source-map end marker '{}': {}", end, err)
                    })?;
                    (Some(end_line), Some(end_col))
                }
                None => (None, None),
            };
            mappings.push(MappingEntry {
                asm_line,
                php_line,
                php_col,
                php_end_line,
                php_end_col,
                op: marker_value(marker, "op").map(str::to_string),
                origin: marker_value(marker, "origin").map(str::to_string),
                function: open_function,
            });
            continue;
        }

        if let Some(function) = open_function {
            if let Some(name) = label_line_name(line) {
                labels.push(LabelEntry {
                    name: name.to_string(),
                    asm_line,
                    function,
                    block: pending_block.take(),
                });
            }
        }
    }

    if let Some(open) = open_function {
        functions[open].asm_end = Some(last_line);
    }

    Ok(render_json(
        source_path,
        asm_path,
        source_sha256,
        &functions,
        &labels,
        &mappings,
    ))
}

/// Returns the value of a space-separated `key=value` token inside a marker
/// tail, or `None` when the key is absent.
fn marker_value<'a>(marker: &'a str, key: &str) -> Option<&'a str> {
    marker
        .split_whitespace()
        .find_map(|token| token.strip_prefix(key)?.strip_prefix('='))
}

/// Parses a required numeric `key=value` token from a `@src` marker tail.
fn parse_marker_number(marker: &str, key: &str) -> Result<usize, String> {
    let value = marker
        .split_whitespace()
        .find_map(|token| token.strip_prefix(key)?.strip_prefix('='))
        .ok_or_else(|| format!("invalid source-map {} marker '{}'", key, marker.trim()))?;
    value.parse::<usize>().map_err(|err| {
        format!("invalid source-map {} marker '{}': {}", key, value, err)
    })
}

/// Returns the label name when `line` is an assembly label definition:
/// an identifier starting at column 0 and terminated by a trailing `:`.
fn label_line_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_end();
    let name = trimmed.strip_suffix(':')?;
    if name.is_empty() || line.starts_with(char::is_whitespace) {
        return None;
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.'));
    valid.then_some(name)
}

/// One PHP source line's inverse index: the assembly line ranges implementing it.
struct LineRanges {
    php_line: usize,
    asm_ranges: Vec<(usize, usize)>,
}

/// Builds the PHP-line → assembly-range inverse index from the parsed mappings.
///
/// Each mapping's extent runs from its marker line to the line before the next
/// mapping in the same function (or the function's end when it is the last one).
/// Ranges for the same PHP line are merged when overlapping or adjacent.
fn build_line_index(functions: &[FunctionRange], mappings: &[MappingEntry]) -> Vec<LineRanges> {
    let mut ranges: Vec<(usize, usize, usize)> = Vec::new(); // (php_line, start, end)
    for (idx, mapping) in mappings.iter().enumerate() {
        let end = mappings
            .get(idx + 1)
            .filter(|next| next.function == mapping.function)
            .map(|next| next.asm_line.saturating_sub(1))
            .or_else(|| {
                mapping
                    .function
                    .and_then(|function| functions.get(function))
                    .and_then(|function| function.asm_end)
            })
            .unwrap_or(mapping.asm_line)
            .max(mapping.asm_line);
        ranges.push((mapping.php_line, mapping.asm_line, end));
    }
    ranges.sort();

    let mut lines: Vec<LineRanges> = Vec::new();
    for (php_line, start, end) in ranges {
        let entry = match lines.iter_mut().find(|entry| entry.php_line == php_line) {
            Some(entry) => entry,
            None => {
                lines.push(LineRanges {
                    php_line,
                    asm_ranges: Vec::new(),
                });
                lines.last_mut().expect("just pushed")
            }
        };
        match entry.asm_ranges.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => entry.asm_ranges.push((start, end)),
        }
    }
    lines.sort_by_key(|entry| entry.php_line);
    lines
}

/// Renders the parsed entries as the v2 JSON document.
fn render_json(
    source_path: &Path,
    asm_path: &Path,
    source_sha256: Option<&str>,
    functions: &[FunctionRange],
    labels: &[LabelEntry],
    mappings: &[MappingEntry],
) -> String {
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"format\": \"elephc-source-map\",\n");
    json.push_str("  \"version\": 2,\n");
    json.push_str(&format!(
        "  \"source\": \"{}\",\n",
        escape_json(&source_path.display().to_string())
    ));
    json.push_str(&format!(
        "  \"source_sha256\": {},\n",
        match source_sha256 {
            Some(hash) => format!("\"{}\"", escape_json(hash)),
            None => "null".to_string(),
        }
    ));
    json.push_str(&format!(
        "  \"asm\": \"{}\",\n",
        escape_json(&asm_path.display().to_string())
    ));

    json.push_str("  \"functions\": [\n");
    for (idx, function) in functions.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"name\": \"{}\", \"symbol\": \"{}\", \"asm_start\": {}, \"asm_end\": {}, \"synthetic\": {}}}",
            escape_json(&function.name),
            escape_json(&function.symbol),
            function.asm_start,
            function.asm_end.unwrap_or(function.asm_start),
            function.synthetic
        ));
        push_separator(&mut json, idx, functions.len());
    }
    json.push_str("  ],\n");

    json.push_str("  \"labels\": [\n");
    for (idx, label) in labels.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"name\": \"{}\", \"asm_line\": {}, \"function\": {}, \"block\": {}}}",
            escape_json(&label.name),
            label.asm_line,
            label.function,
            match &label.block {
                Some(block) => format!("\"{}\"", escape_json(block)),
                None => "null".to_string(),
            }
        ));
        push_separator(&mut json, idx, labels.len());
    }
    json.push_str("  ],\n");

    json.push_str("  \"mappings\": [\n");
    for (idx, mapping) in mappings.iter().enumerate() {
        let op = match &mapping.op {
            Some(op) => format!("\"{}\"", escape_json(op)),
            None => "null".to_string(),
        };
        let origin = match &mapping.origin {
            Some(origin) => format!("\"{}\"", escape_json(origin)),
            None => "null".to_string(),
        };
        let function = match mapping.function {
            Some(function) => function.to_string(),
            None => "null".to_string(),
        };
        let optional_number = |value: Option<usize>| match value {
            Some(value) => value.to_string(),
            None => "null".to_string(),
        };
        json.push_str(&format!(
            "    {{\"asm_line\": {}, \"php_line\": {}, \"php_col\": {}, \
             \"php_end_line\": {}, \"php_end_col\": {}, \
             \"op\": {}, \"origin\": {}, \"function\": {}}}",
            mapping.asm_line,
            mapping.php_line,
            mapping.php_col,
            optional_number(mapping.php_end_line),
            optional_number(mapping.php_end_col),
            op,
            origin,
            function
        ));
        push_separator(&mut json, idx, mappings.len());
    }
    json.push_str("  ],\n");

    let lines = build_line_index(functions, mappings);
    json.push_str("  \"lines\": [\n");
    for (idx, entry) in lines.iter().enumerate() {
        let ranges = entry
            .asm_ranges
            .iter()
            .map(|(start, end)| format!("[{}, {}]", start, end))
            .collect::<Vec<_>>()
            .join(", ");
        json.push_str(&format!(
            "    {{\"php_line\": {}, \"asm_ranges\": [{}]}}",
            entry.php_line, ranges
        ));
        push_separator(&mut json, idx, lines.len());
    }
    json.push_str("  ]\n");
    json.push_str("}\n");
    json
}

/// Appends the array-element separator: a comma for every element except the last.
fn push_separator(json: &mut String, idx: usize, len: usize) {
    if idx + 1 != len {
        json.push(',');
    }
    json.push('\n');
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

/// Computes the lowercase-hex SHA-256 digest of `bytes` (FIPS 180-4).
/// Implemented locally so the compiler needs no cryptography dependency for
/// a simple integrity checksum.
fn sha256_hex(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut data = bytes.to_vec();
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in data.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|word| format!("{:08x}", word)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic assembly exercising every v2 marker: a function region with a
    /// block-tagged label, an op/end-tagged mapping, an origin-tagged mapping,
    /// a synthetic function, and a bare v1-style mapping outside any region.
    const SAMPLE_ASM: &str = "\
    # @src line=9 col=2
    mov x0, #1
    # @fn name=foo symbol=_php_foo
_php_foo:
    # @src line=3 col=5 end=3:12 op=store_local
    str x0, [sp]
    # @block name=loop_body
Lbb_loop_body_1:
    # @src line=4 col=9 op=const_i64 origin=const_fold
    mov x1, #7
    ret
    # @endfn name=foo
    # @fn name=_class_propinit_0 symbol=_class_propinit_0 synthetic=1
_class_propinit_0:
    ret
    # @endfn name=_class_propinit_0
";

    /// Verifies the v2 envelope: format name without a version suffix, integer
    /// schema version, both paths, and the source checksum (null when unknown).
    #[test]
    fn v2_envelope_has_format_version_and_paths() {
        let json = build_source_map(SAMPLE_ASM, Path::new("a.php"), Path::new("a.s"), Some("ab12"))
            .unwrap();
        assert!(json.contains("\"format\": \"elephc-source-map\""), "{json}");
        assert!(json.contains("\"version\": 2"), "{json}");
        assert!(json.contains("\"source\": \"a.php\""), "{json}");
        assert!(json.contains("\"asm\": \"a.s\""), "{json}");
        assert!(json.contains("\"source_sha256\": \"ab12\""), "{json}");

        let no_hash =
            build_source_map(SAMPLE_ASM, Path::new("a.php"), Path::new("a.s"), None).unwrap();
        assert!(no_hash.contains("\"source_sha256\": null"), "{no_hash}");
    }

    /// Verifies `@fn`/`@endfn` markers become functions entries with the PHP
    /// name, entry symbol, 1-based line range, and synthetic flag.
    #[test]
    fn v2_records_function_ranges() {
        let json =
            build_source_map(SAMPLE_ASM, Path::new("a.php"), Path::new("a.s"), None).unwrap();
        assert!(
            json.contains(
                "{\"name\": \"foo\", \"symbol\": \"_php_foo\", \"asm_start\": 3, \
                 \"asm_end\": 12, \"synthetic\": false}"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "{\"name\": \"_class_propinit_0\", \"symbol\": \"_class_propinit_0\", \
                 \"asm_start\": 13, \"asm_end\": 16, \"synthetic\": true}"
            ),
            "{json}"
        );
    }

    /// Verifies labels inside a function range are recorded with their owning
    /// function index and the EIR block name from a preceding `@block` marker.
    #[test]
    fn v2_records_labels_with_block_names() {
        let json =
            build_source_map(SAMPLE_ASM, Path::new("a.php"), Path::new("a.s"), None).unwrap();
        assert!(
            json.contains(
                "{\"name\": \"_php_foo\", \"asm_line\": 4, \"function\": 0, \"block\": null}"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "{\"name\": \"Lbb_loop_body_1\", \"asm_line\": 8, \"function\": 0, \
                 \"block\": \"loop_body\"}"
            ),
            "{json}"
        );
    }

    /// Verifies `@src` mappings carry the end position, opcode, origin, and
    /// owning function when present, and null placeholders when absent.
    #[test]
    fn v2_records_mappings_with_all_fields() {
        let json =
            build_source_map(SAMPLE_ASM, Path::new("a.php"), Path::new("a.s"), None).unwrap();
        assert!(
            json.contains(
                "{\"asm_line\": 5, \"php_line\": 3, \"php_col\": 5, \
                 \"php_end_line\": 3, \"php_end_col\": 12, \
                 \"op\": \"store_local\", \"origin\": null, \"function\": 0}"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "{\"asm_line\": 9, \"php_line\": 4, \"php_col\": 9, \
                 \"php_end_line\": null, \"php_end_col\": null, \
                 \"op\": \"const_i64\", \"origin\": \"const_fold\", \"function\": 0}"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "{\"asm_line\": 1, \"php_line\": 9, \"php_col\": 2, \
                 \"php_end_line\": null, \"php_end_col\": null, \
                 \"op\": null, \"origin\": null, \"function\": null}"
            ),
            "{json}"
        );
    }

    /// Verifies the inverse index maps each PHP line to the assembly ranges
    /// implementing it: a mapping extends to the next mapping in the same
    /// function, and the last mapping extends to the function end.
    #[test]
    fn v2_builds_php_line_inverse_index() {
        let json =
            build_source_map(SAMPLE_ASM, Path::new("a.php"), Path::new("a.s"), None).unwrap();
        assert!(
            json.contains("{\"php_line\": 3, \"asm_ranges\": [[5, 8]]}"),
            "{json}"
        );
        assert!(
            json.contains("{\"php_line\": 4, \"asm_ranges\": [[9, 12]]}"),
            "{json}"
        );
        assert!(
            json.contains("{\"php_line\": 9, \"asm_ranges\": [[1, 1]]}"),
            "{json}"
        );
    }

    /// Verifies a malformed `@src` line number still fails map generation with
    /// a descriptive error, as in v1.
    #[test]
    fn v2_rejects_malformed_src_marker() {
        let err = build_source_map(
            "# @src line=x col=1\n",
            Path::new("a.php"),
            Path::new("a.s"),
            None,
        )
        .unwrap_err();
        assert!(err.contains("invalid source-map line marker"), "{err}");
    }

    /// Verifies an unterminated function region is closed at the last assembly
    /// line instead of being dropped.
    #[test]
    fn v2_closes_unterminated_function_at_eof() {
        let asm = "# @fn name=main symbol=_main\n_main:\n    ret\n";
        let json = build_source_map(asm, Path::new("a.php"), Path::new("a.s"), None).unwrap();
        assert!(
            json.contains(
                "{\"name\": \"main\", \"symbol\": \"_main\", \"asm_start\": 1, \
                 \"asm_end\": 3, \"synthetic\": false}"
            ),
            "{json}"
        );
    }

    /// Verifies the local SHA-256 against the FIPS 180-4 test vectors for the
    /// empty string and "abc".
    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

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
