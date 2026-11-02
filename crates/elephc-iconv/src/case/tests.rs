//! Purpose:
//! Unit tests pinning `mb_strtoupper()` / `mb_strtolower()` to php 8.5's mbstring output.
//!
//! Called from:
//! - `cargo test -p elephc-iconv` through Rust's test harness.
//!
//! Key details:
//! - Every expected value below was produced by php 8.5.10 with mbstring; the comments
//!   name the rule each case pins.
//! - The chunk-boundary cases exist because php-src's final-sigma look-around behaves
//!   differently across its 64-code-point decode chunks, and elephc reproduces that.
//! - `ELEPHC_MB_CASE_ORACLE` names an optional file of php-generated cases
//!   (`mode<TAB>encoding-hex<TAB>input-hex<TAB>expected-hex` per line, `-` for a null
//!   encoding), written by `php scripts/mbstring/gen_case_oracle.php <cases> [seed]`;
//!   the ignored differential test replays it.

use super::{convert_case, CaseError, CaseMode};

/// Uppercases in UTF-8 and unwraps.
fn upper(input: &[u8]) -> Vec<u8> {
    convert_case(CaseMode::Upper, input, None).expect("UTF-8 always converts")
}

/// Lowercases in UTF-8 and unwraps.
fn lower(input: &[u8]) -> Vec<u8> {
    convert_case(CaseMode::Lower, input, None).expect("UTF-8 always converts")
}

/// Converts in an explicit encoding and unwraps.
fn convert_in(mode: CaseMode, input: &[u8], encoding: &str) -> Vec<u8> {
    convert_case(mode, input, Some(encoding.as_bytes())).expect("encoding accepted")
}

/// ASCII goes through the fast path and maps only letters.
#[test]
fn ascii_letters_map_and_everything_else_stays() {
    assert_eq!(upper(b"Hello, World! 123 \0 ~"), b"HELLO, WORLD! 123 \0 ~");
    assert_eq!(lower(b"Hello, World! 123"), b"hello, world! 123");
    assert_eq!(upper(b""), b"");
}

/// Full (SpecialCasing) mappings expand, and titlecase digraphs map to their case pair.
#[test]
fn full_case_mapping_matches_php() {
    assert_eq!(upper("straße éà ǆ ﬁ".as_bytes()), "STRASSE ÉÀ Ǆ FI".as_bytes());
    assert_eq!(upper("ŉǰΐ".as_bytes()), "\u{2BC}NJ\u{30C}\u{399}\u{308}\u{301}".as_bytes());
    assert_eq!(lower("İSTANBUL".as_bytes()), "i̇stanbul".as_bytes());
    assert_eq!(upper("ǅ ǈ ǋ ǲ".as_bytes()), "Ǆ Ǉ Ǌ Ǳ".as_bytes());
    assert_eq!(lower("ǅ ǈ ǋ ǲ".as_bytes()), "ǆ ǉ ǌ ǳ".as_bytes());
}

/// Scripts whose case pairs were added or changed in recent Unicode versions.
#[test]
fn version_sensitive_scripts_match_php() {
    // Georgian Mtavruli (Unicode 11) lowercases to Mkhedruli; Mkhedruli uppercases back.
    assert_eq!(lower("ᲐᲑᲒ".as_bytes()), "აბგ".as_bytes());
    assert_eq!(upper("აბგ".as_bytes()), "ᲐᲑᲒ".as_bytes());
    // Cherokee small letters (Unicode 8) uppercase to the original block.
    assert_eq!(upper("ꭰꭱ".as_bytes()), "ᎠᎡ".as_bytes());
    assert_eq!(lower("ᎠᎡ".as_bytes()), "ꭰꭱ".as_bytes());
    // Latin additions from Unicode 16 and 17.
    assert_eq!(lower("\u{A7CB}\u{A7DC}".as_bytes()), "\u{0264}\u{019B}".as_bytes());
    assert_eq!(upper("\u{A7CD}\u{A7DB}".as_bytes()), "\u{A7CC}\u{A7DA}".as_bytes());
}

/// Capital sigma lowercases to final sigma only at the end of a word.
#[test]
fn final_sigma_follows_mbstring_context_rules() {
    assert_eq!(lower("ΑΣ".as_bytes()), "ας".as_bytes());
    assert_eq!(lower("ΑΣ Α".as_bytes()), "ας α".as_bytes());
    assert_eq!(lower("ΑΣΑ".as_bytes()), "ασα".as_bytes());
    assert_eq!(lower("Σ".as_bytes()), "σ".as_bytes());
    assert_eq!(lower("ΣΑΣ".as_bytes()), "σας".as_bytes());
    // Case_Ignorable code points are skipped on both sides.
    assert_eq!(lower("ΑΣ'Α".as_bytes()), "ασ'α".as_bytes());
    assert_eq!(lower("Α'Σ".as_bytes()), "α'ς".as_bytes());
    assert_eq!(lower("ΑΣʰ".as_bytes()), "αςʰ".as_bytes());
    // An invalid byte is neither cased nor ignorable, so it ends the look-around.
    assert_eq!(lower(b"\xce\x91\xff\xce\xa3"), "α?σ".as_bytes());
    assert_eq!(lower(b"\xce\x91\xce\xa3\xff\xce\x91"), "ας?α".as_bytes());
    // Uppercasing never looks at context.
    assert_eq!(upper("ας".as_bytes()), "ΑΣ".as_bytes());
}

/// php-src's look-ahead across a chunk boundary tests Cased before Case_Ignorable.
#[test]
fn final_sigma_look_ahead_across_a_chunk_boundary() {
    // U+02B0 is both Cased and Case_Ignorable. Inside the chunk it is skipped, so the
    // sigma is final; as the first code point of the next chunk it counts as cased.
    let within = format!("{}Σʰ!", "Α".repeat(62));
    let across = format!("{}Σʰ!", "Α".repeat(63));
    let within = String::from_utf8(lower(within.as_bytes())).unwrap();
    let across = String::from_utf8(lower(across.as_bytes())).unwrap();
    assert_eq!(within.chars().nth(62), Some('ς'));
    assert_eq!(across.chars().nth(63), Some('σ'));
}

/// php-src's look-back across a chunk boundary reads the previous chunk's converted tail.
#[test]
fn final_sigma_look_back_across_a_chunk_boundary() {
    let within = String::from_utf8(lower("!ʰΣ".as_bytes())).unwrap();
    assert_eq!(within, "!ʰσ");
    let across = format!("{}ʰΣ", "!".repeat(63));
    let across = String::from_utf8(lower(across.as_bytes())).unwrap();
    assert_eq!(across.chars().nth(64), Some('ς'));
}

/// Each maximal invalid UTF-8 subpart becomes exactly one `?`.
#[test]
fn invalid_utf8_substitutes_one_question_mark_per_maximal_subpart() {
    assert_eq!(upper(b"a\xffb"), b"A?B");
    assert_eq!(
        upper(b"a\xffb\xc3c\xe2\x82d\xf0\x9f\x98e\xed\xa0\x80f\xc0\xafg\xf4\x90\x80\x80h\xe2\x82"),
        b"A?B?C?D?E???F??G????H?"
    );
    assert_eq!(upper(b"ab\xf0\x9f"), b"AB?");
    assert_eq!(lower(b"\xc3"), b"?");
}

/// Named UTF-8 spellings are case-insensitive, and the lookup stops at a NUL byte.
#[test]
fn utf8_aliases_are_accepted() {
    for name in ["UTF-8", "utf-8", "Utf8", "UTF-8\0junk"] {
        assert_eq!(convert_in(CaseMode::Upper, "é".as_bytes(), name), "É".as_bytes(), "{name}");
    }
}

/// Single-byte encodings decode, map, and re-encode through mbstring's own tables.
#[test]
fn single_byte_encodings_round_trip() {
    assert_eq!(convert_in(CaseMode::Upper, b"\xe9t\xe9", "ISO-8859-1"), b"\xc9T\xc9");
    // ÿ uppercases to U+0178 and µ to U+039C, neither of which Latin-1 holds; ß becomes SS.
    assert_eq!(convert_in(CaseMode::Upper, b"\xff\xb5\xdf", "latin1"), b"??SS");
    assert_eq!(convert_in(CaseMode::Lower, b"\xc9T", "ISO-8859-15"), b"\xe9t");
    // Windows-1252 keeps its C1 letters (š -> Š).
    assert_eq!(convert_in(CaseMode::Upper, b"\x9a", "cp1252"), b"\x8a");
    assert_eq!(convert_in(CaseMode::Upper, b"\xc0\xdf", "KOI8-R"), b"\xe0\xff");
}

/// ISO-8859-9 applies the Turkish dotted/dotless `i` rules.
#[test]
fn iso_8859_9_uses_turkish_i() {
    assert_eq!(convert_in(CaseMode::Upper, b"i\xfd", "ISO-8859-9"), b"\xddI");
    assert_eq!(convert_in(CaseMode::Lower, b"I\xdd", "ISO-8859-9"), b"\xfdi");
}

/// ASCII / 7bit reject high bytes, while 8bit treats bytes as Latin-1 code points.
#[test]
fn ascii_seven_bit_and_eight_bit() {
    assert_eq!(convert_in(CaseMode::Upper, b"abc\x80z", "ASCII"), b"ABC?Z");
    assert_eq!(convert_in(CaseMode::Upper, b"ab\xc3\xa9", "7bit"), b"AB??");
    assert_eq!(convert_in(CaseMode::Upper, b"ab\xc3\xa9", "8bit"), b"AB\xc3\xa9");
    assert_eq!(convert_in(CaseMode::Upper, b"abc\xe9\xff\xdf", "binary"), b"ABC\xc9?SS");
}

/// UTF-16 detects a BOM, drops it, and writes big-endian; an odd byte is one `?`.
#[test]
fn utf16_family() {
    assert_eq!(convert_in(CaseMode::Upper, b"\x00a\x00", "UTF-16"), b"\x00A\x00?");
    assert_eq!(convert_in(CaseMode::Upper, b"\xff\xfea\x00", "UTF-16"), b"\x00A");
    assert_eq!(convert_in(CaseMode::Upper, b"\xfe\xff\x00a", "UTF-16"), b"\x00A");
    assert_eq!(convert_in(CaseMode::Upper, b"a\x00", "UTF-16LE"), b"A\x00");
    assert_eq!(convert_in(CaseMode::Upper, b"abc\xe9\xff\xdf", "UTF-16"), b"abc\xe9\xff\xdf");
    assert_eq!(convert_in(CaseMode::Upper, b"abc\xe9\xff\xdf", "UTF-16LE"), b"abc\xe9?\x00");
}

/// UTF-32 validates code points, UCS-4 passes any 32-bit value through.
#[test]
fn utf32_and_ucs4_families() {
    assert_eq!(
        convert_in(CaseMode::Upper, b"abc\xe9\xff\xdf", "UTF-32"),
        b"\x00\x00\x00?\x00\x00\x00?"
    );
    assert_eq!(
        convert_in(CaseMode::Upper, b"abc\xe9\xff\xdf", "UCS-4"),
        b"abc\xe9\x00\x00\x00?"
    );
    assert_eq!(convert_in(CaseMode::Upper, b"abc\xe9\xff\xdf", "UCS-2"), b"abc\xe9\xff\xdf");
}

/// The libc-backed CJK encodings map fullwidth Latin, Greek, and Cyrillic letters.
#[test]
fn cjk_encodings_map_through_libc_iconv() {
    // "ａβд Z" in each encoding, php's uppercase, and php's lowercase of that uppercase.
    for (encoding, input, upper_bytes, lower_bytes) in [
        ("SJIS", "828183c08474205a", "826083a08444205a", "828183c08474207a"),
        ("EUC-JP", "a3e1a6c2a7d5205a", "a3c1a6a2a7a5205a", "a3e1a6c2a7d5207a"),
        ("GB18030", "a3e1a6c2a7d5205a", "a3c1a6a2a7a5205a", "a3e1a6c2a7d5207a"),
        ("BIG-5", "a2e9a35dc7cc205a", "a2cfa345c7b1205a", "a2e9a35dc7cc207a"),
        ("UHC", "a3e1a5e2acd5205a", "a3c1a5c2aca5205a", "a3e1a5e2acd5207a"),
    ] {
        let hex = |text: &str| -> Vec<u8> {
            (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
        };
        let uppered = convert_in(CaseMode::Upper, &hex(input), encoding);
        assert_eq!(uppered, hex(upper_bytes), "{encoding}");
        assert_eq!(convert_in(CaseMode::Lower, &uppered, encoding), hex(lower_bytes), "{encoding}");
    }
}

/// mbstring's perfect-hash lookup also accepts some name prefixes.
#[test]
fn perfect_hash_prefixes_are_valid_names() {
    for name in ["cp", "cp85", "iso-885", "ucs-", "windows", "sjis-m"] {
        let result = convert_case(CaseMode::Upper, b"a", Some(name.as_bytes()));
        assert!(!matches!(result, Err(CaseError::InvalidEncoding(_))), "{name}");
    }
    assert_eq!(convert_in(CaseMode::Upper, b"\xe9", "windows"), b"\xc9");
}

/// Unknown names produce php's exact `ValueError` message.
#[test]
fn unknown_encodings_render_php_value_error() {
    for (name, quoted) in [
        (&b"nope"[..], &b"nope"[..]),
        (b"", b""),
        (b"pass", b"pass"),
        (b"auto", b"auto"),
        (b"utf-8 ", b"utf-8 "),
        (b"UTF_8", b"UTF_8"),
        (b"\xffnope", b"\xffnope"),
        (b"nope\0x", b"nope"),
    ] {
        let error = convert_case(CaseMode::Upper, b"a", Some(name)).unwrap_err();
        assert_eq!(error, CaseError::InvalidEncoding(quoted.to_vec()));
    }
    let error = convert_case(CaseMode::Lower, b"a", Some(b"nope")).unwrap_err();
    assert_eq!(
        error.value_error_message("mb_strtolower"),
        b"mb_strtolower(): Argument #2 ($encoding) must be a valid encoding, \"nope\" given"
    );
}

/// Valid mbstring encodings without an elephc codec are refused, not remapped.
#[test]
fn unsupported_encodings_are_reported() {
    for name in ["UTF-7", "ISO-2022-JP", "HZ", "EUC-TW", "UTF-8-Mobile#DOCOMO", "qprint"] {
        let error = convert_case(CaseMode::Upper, b"a", Some(name.as_bytes())).unwrap_err();
        assert_eq!(error, CaseError::UnsupportedEncoding(name.as_bytes().to_vec()));
    }
    let error = convert_case(CaseMode::Upper, b"a", Some(b"BASE64")).unwrap_err();
    assert_eq!(error, CaseError::UnsupportedEncoding(b"BASE64".to_vec()));
    assert_eq!(
        error.value_error_message("mb_strtoupper"),
        b"mb_strtoupper(): Argument #2 ($encoding) must be an encoding elephc supports, \"BASE64\" given"
    );
}

/// Replays a php-generated differential corpus when one is provided.
#[test]
#[ignore = "needs ELEPHC_MB_CASE_ORACLE, a corpus generated by php with mbstring"]
fn differential_corpus_matches_php() {
    let Ok(path) = std::env::var("ELEPHC_MB_CASE_ORACLE") else {
        return;
    };
    let corpus = std::fs::read_to_string(path).expect("oracle file");
    let decode = |hex: &str| -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    };
    let mut failures = Vec::new();
    let mut total = 0;
    for line in corpus.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [mode, encoding, input, expected] = fields[..] else {
            continue;
        };
        total += 1;
        let mode = if mode == "upper" { CaseMode::Upper } else { CaseMode::Lower };
        let encoding = (encoding != "-").then(|| decode(encoding));
        let actual = match convert_case(mode, &decode(input), encoding.as_deref()) {
            Ok(bytes) => bytes,
            Err(error) => error.value_error_message(mode.function_name()),
        };
        if actual != decode(expected) {
            failures.push(line.to_string());
        }
    }
    assert!(failures.is_empty(), "{} of {total} cases differ:\n{}", failures.len(), failures.join("\n"));
}
