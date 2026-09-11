//! Purpose:
//! Verifies Unicode codecs against exhaustive short-input and seeded PHP fixtures.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test codecs`.
//!
//! Key details:
//! - Independently captured hashes include validity, length, decoding, scrubbing, and case.
//! - Inputs cover every one- and two-byte sequence, BOMs, truncation, and raw UCS values.

use elephc_mbstring::encoding::{Encoding, Substitute, SubstituteMode, UnicodeEncoding};
use elephc_mbstring::{text, unicode::CaseMode};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Reconstructs the deterministic byte corpus used by the PHP oracle generator.
fn build_inputs() -> Vec<Vec<u8>> {
    let mut inputs = vec![Vec::new()];
    inputs.extend((0..=255).map(|byte| vec![byte as u8]));
    inputs.extend((0..=65535).map(|pair| (pair as u16).to_be_bytes().to_vec()));
    let mut state = 42u32;
    for sample in 0..1024 {
        let mut input = Vec::new();
        for _ in 0..3 + sample % 33 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            input.push((state >> 24) as u8);
        }
        inputs.push(input);
    }
    for hex in [
        "fffe4100", "feff0041", "0000feff00000041", "fffe000041000000",
        "0000d800", "00110000", "ffffffff", "d8004100", "d8000041",
        "eda080", "f0908080", "f4908080", "f0808080", "e08080",
    ] {
        inputs.push((0..hex.len()).step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect());
    }
    inputs
}

/// Shares the large deterministic input corpus across independently scheduled codec tests.
fn inputs() -> &'static [Vec<u8>] {
    static INPUTS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    INPUTS.get_or_init(build_inputs)
}

/// Parses the checked-in PHP oracle once for all codec tests in this process.
fn oracle() -> &'static serde_json::Value {
    static ORACLE: OnceLock<serde_json::Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/codecs.json"))
            .expect("codec fixture must contain valid JSON")
    })
}

/// Hashes every observable result with its byte length to prevent boundary ambiguity.
fn check_codec(encoding: Encoding, input: &[u8], hash: &mut Sha256) {
    let substitute = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    let decoded = encoding.decode(input);
    hash.update((encoding.strlen(input) as u32).to_le_bytes());
    hash.update([u8::from(decoded.is_valid())]);
    for output in [
        text::convert_encoding(input, encoding, Encoding::lookup(b"UCS-4BE").unwrap(), substitute),
        text::scrub(input, encoding, substitute),
        text::convert_case(input, CaseMode::Upper, encoding, substitute),
    ] {
        hash.update((output.len() as u32).to_le_bytes());
        hash.update(output);
    }
}

/// Compares one codec's decoding, validity, length, replacement, and uppercase with PHP.
fn check_codec_against_php(name: &str) {
    let oracle = oracle();
    let inputs = inputs();
    let expected = oracle["sha256"][name]
        .as_str()
        .unwrap_or_else(|| panic!("missing PHP hash for codec `{name}`"));
    let encoding = Encoding::lookup(name.as_bytes())
        .unwrap_or_else(|| panic!("codec `{name}` must be registered"));
    let mut hash = Sha256::new();
    for input in inputs {
        check_codec(encoding, input, &mut hash);
    }
    let mut count = inputs.len();
    if matches!(name, "EUC-JP" | "eucJP-win" | "EUC-JP-2004" | "CP51932") {
        for pair in 0..=65535u32 {
            check_codec(encoding, &[0x8f, (pair >> 8) as u8, pair as u8], &mut hash);
        }
        count += 65536;
    }
    assert_eq!(
        count as u64,
        oracle["input_counts"][name]
            .as_u64()
            .unwrap_or_else(|| panic!("missing PHP input count for codec `{name}`")),
        "{name} corpus size"
    );
    assert_eq!(
        format!("{:x}", hash.finalize()),
        expected,
        "{name} PHP oracle hash"
    );
}

/// Generates one independently timed test per codec and records the covered oracle names.
macro_rules! codec_tests {
    ($($test_name:ident => $encoding:literal),+ $(,)?) => {
        /// Exact codec names covered by the generated tests.
        const CODEC_NAMES: &[&str] = &[$($encoding),+];

        $(
            #[doc = concat!("Compares the `", $encoding, "` codec with its PHP oracle.")]
            #[test]
            fn $test_name() {
                check_codec_against_php($encoding);
            }
        )+
    };
}

codec_tests!(
    unicode_codec_base64_matches_php => "BASE64",
    unicode_codec_quoted_printable_matches_php => "Quoted-Printable",
    unicode_codec_uuencode_matches_php => "UUENCODE",
    unicode_codec_html_entities_matches_php => "HTML-ENTITIES",
    unicode_codec_ascii_matches_php => "ASCII",
    unicode_codec_8bit_matches_php => "8bit",
    unicode_codec_jis_matches_php => "JIS",
    unicode_codec_iso_2022_jp_matches_php => "ISO-2022-JP",
    unicode_codec_iso_2022_jp_ms_matches_php => "ISO-2022-JP-MS",
    unicode_codec_cp50220_matches_php => "CP50220",
    unicode_codec_cp50221_matches_php => "CP50221",
    unicode_codec_cp50222_matches_php => "CP50222",
    unicode_codec_iso_2022_jp_2004_matches_php => "ISO-2022-JP-2004",
    unicode_codec_iso_2022_jp_mobile_kddi_matches_php => "ISO-2022-JP-MOBILE#KDDI",
    unicode_codec_iso_2022_kr_matches_php => "ISO-2022-KR",
    unicode_codec_hz_matches_php => "HZ",
    unicode_codec_euc_tw_matches_php => "EUC-TW",
    unicode_codec_gb18030_matches_php => "GB18030",
    unicode_codec_gb18030_2022_matches_php => "GB18030-2022",
    unicode_codec_utf_7_matches_php => "UTF-7",
    unicode_codec_utf7_imap_matches_php => "UTF7-IMAP",
    unicode_codec_utf_8_matches_php => "UTF-8",
    unicode_codec_utf_8_mobile_docomo_matches_php => "UTF-8-Mobile#DOCOMO",
    unicode_codec_utf_8_mobile_kddi_a_matches_php => "UTF-8-Mobile#KDDI-A",
    unicode_codec_utf_8_mobile_kddi_b_matches_php => "UTF-8-Mobile#KDDI-B",
    unicode_codec_utf_8_mobile_softbank_matches_php => "UTF-8-Mobile#SOFTBANK",
    unicode_codec_utf_16_matches_php => "UTF-16",
    unicode_codec_utf_16be_matches_php => "UTF-16BE",
    unicode_codec_utf_16le_matches_php => "UTF-16LE",
    unicode_codec_utf_32_matches_php => "UTF-32",
    unicode_codec_utf_32be_matches_php => "UTF-32BE",
    unicode_codec_utf_32le_matches_php => "UTF-32LE",
    unicode_codec_ucs_2_matches_php => "UCS-2",
    unicode_codec_ucs_2be_matches_php => "UCS-2BE",
    unicode_codec_ucs_2le_matches_php => "UCS-2LE",
    unicode_codec_ucs_4_matches_php => "UCS-4",
    unicode_codec_ucs_4be_matches_php => "UCS-4BE",
    unicode_codec_ucs_4le_matches_php => "UCS-4LE",
    unicode_codec_windows_1252_matches_php => "Windows-1252",
    unicode_codec_windows_1254_matches_php => "Windows-1254",
    unicode_codec_iso_8859_1_matches_php => "ISO-8859-1",
    unicode_codec_iso_8859_2_matches_php => "ISO-8859-2",
    unicode_codec_iso_8859_3_matches_php => "ISO-8859-3",
    unicode_codec_iso_8859_4_matches_php => "ISO-8859-4",
    unicode_codec_iso_8859_5_matches_php => "ISO-8859-5",
    unicode_codec_iso_8859_6_matches_php => "ISO-8859-6",
    unicode_codec_iso_8859_7_matches_php => "ISO-8859-7",
    unicode_codec_iso_8859_8_matches_php => "ISO-8859-8",
    unicode_codec_iso_8859_9_matches_php => "ISO-8859-9",
    unicode_codec_iso_8859_10_matches_php => "ISO-8859-10",
    unicode_codec_iso_8859_13_matches_php => "ISO-8859-13",
    unicode_codec_iso_8859_14_matches_php => "ISO-8859-14",
    unicode_codec_iso_8859_15_matches_php => "ISO-8859-15",
    unicode_codec_iso_8859_16_matches_php => "ISO-8859-16",
    unicode_codec_windows_1251_matches_php => "Windows-1251",
    unicode_codec_cp866_matches_php => "CP866",
    unicode_codec_koi8_r_matches_php => "KOI8-R",
    unicode_codec_koi8_u_matches_php => "KOI8-U",
    unicode_codec_armscii_8_matches_php => "ArmSCII-8",
    unicode_codec_cp850_matches_php => "CP850",
    unicode_codec_sjis_matches_php => "SJIS",
    unicode_codec_cp932_matches_php => "CP932",
    unicode_codec_sjis_win_matches_php => "SJIS-win",
    unicode_codec_sjis_2004_matches_php => "SJIS-2004",
    unicode_codec_sjis_mac_matches_php => "SJIS-mac",
    unicode_codec_euc_cn_matches_php => "EUC-CN",
    unicode_codec_cp936_matches_php => "CP936",
    unicode_codec_big_5_matches_php => "BIG-5",
    unicode_codec_cp950_matches_php => "CP950",
    unicode_codec_euc_kr_matches_php => "EUC-KR",
    unicode_codec_uhc_matches_php => "UHC",
    unicode_codec_sjis_mobile_docomo_matches_php => "SJIS-Mobile#DOCOMO",
    unicode_codec_sjis_mobile_kddi_matches_php => "SJIS-Mobile#KDDI",
    unicode_codec_sjis_mobile_softbank_matches_php => "SJIS-Mobile#SOFTBANK",
    unicode_codec_euc_jp_matches_php => "EUC-JP",
    unicode_codec_eucjp_win_matches_php => "eucJP-win",
    unicode_codec_euc_jp_2004_matches_php => "EUC-JP-2004",
    unicode_codec_cp51932_matches_php => "CP51932",
);

/// Keeps the generated codec tests and both oracle maps complete and in exact agreement.
#[test]
fn unicode_codec_inventory_matches_php_oracle() {
    let oracle = oracle();
    assert_eq!(
        inputs().len() as u64,
        oracle["input_count"]
            .as_u64()
            .expect("PHP oracle must declare the shared input count")
    );

    let mut declared = CODEC_NAMES.to_vec();
    let declared_count = declared.len();
    declared.sort_unstable();
    declared.dedup();
    assert_eq!(
        declared.len(),
        declared_count,
        "generated codec tests must not repeat an encoding"
    );

    for field in ["sha256", "input_counts"] {
        let mut oracle_names: Vec<_> = oracle[field]
            .as_object()
            .unwrap_or_else(|| panic!("PHP oracle field `{field}` must be an object"))
            .keys()
            .map(String::as_str)
            .collect();
        oracle_names.sort_unstable();
        assert_eq!(
            declared, oracle_names,
            "generated codec tests must cover every `{field}` entry exactly once"
        );
    }
}

/// Preserves input byte offsets and PHP's distinction between malformed and unmappable units.
#[test]
fn unicode_offsets_and_substitution_modes() {
    use UnicodeEncoding::*;
    let decoded = Utf8.decode(b"a\xc3\xa9\xe2\x82");
    assert_eq!(decoded.offsets, [0, 1, 3]);
    assert!(!decoded.is_valid());
    assert_eq!(Utf16.decode(&[0xff, 0xfe, 0x41, 0]).offsets, [2]);
    let long = Substitute { mode: SubstituteMode::Long, character: b'?' as u32 };
    let entity = Substitute { mode: SubstituteMode::Entity, ..long };
    assert_eq!(Ascii.encode(&[0x2603], long), b"U+2603");
    assert_eq!(Ascii.encode(&[0x2603], entity), b"&#x2603;");
    assert_eq!(Ascii.encode(&[u32::MAX], long), b"?");
    assert_eq!(Ascii.encode(&[0x2603], Substitute { mode: SubstituteMode::None, ..long }), b"");
}
