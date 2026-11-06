//! Purpose:
//! Unit tests for the iconv engine, pinned against PHP 8's observable behavior.
//!
//! Called from:
//! - `cargo test -p elephc-iconv` through Rust's test harness.
//!
//! Key details:
//! - Expectations were captured from `php -r` on a glibc host; charsets used here
//!   (`UTF-8`, `ISO-8859-1`, `ISO-2022-JP`, `ASCII`, `UCS-4LE`) exist in every
//!   supported platform's iconv.
//! - Diagnostics are asserted as full message bodies because both backends render them.

use crate::error::IconvError;
use crate::ffi::Converter;
use crate::mime::encode::{MimeEncodeOptions, Scheme};
use crate::{convert, mime, search};

/// Verifies that a round trip through ISO-8859-1 preserves representable characters.
#[test]
fn converts_between_utf8_and_latin1() {
    let latin1 = convert::convert(b"UTF-8", b"ISO-8859-1", "caf\u{e9}".as_bytes()).unwrap();
    assert_eq!(latin1, b"caf\xe9");
    let utf8 = convert::convert(b"ISO-8859-1", b"UTF-8", &latin1).unwrap();
    assert_eq!(utf8, "caf\u{e9}".as_bytes());
}

/// Verifies that an unknown charset reports php-src's wrong-encoding warning.
#[test]
fn unknown_charset_reports_wrong_encoding() {
    let error = convert::convert(b"NOPEENC", b"UTF-8", b"x").unwrap_err();
    assert_eq!(
        error.php_message("iconv"),
        "iconv(): Wrong encoding, conversion from \"NOPEENC\" to \"UTF-8\" is not allowed"
    );
}

/// Verifies that malformed and truncated input map onto php-src's two notices.
#[test]
fn malformed_input_reports_notices() {
    let illegal = convert::convert(b"UTF-8", b"UTF-8", b"abc\xc3(def").unwrap_err();
    assert_eq!(illegal, IconvError::IllegalSequence);
    assert_eq!(
        illegal.php_message("iconv"),
        "iconv(): Detected an illegal character in input string"
    );
    let truncated = convert::convert(b"UTF-8", b"UTF-8", b"abc\xc3").unwrap_err();
    assert_eq!(truncated, IconvError::IncompleteChar);
    assert_eq!(
        truncated.php_message("iconv"),
        "iconv(): Detected an incomplete multibyte character in input string"
    );
}

/// Verifies the two libc target suffixes PHP programs rely on.
///
/// `//IGNORE` needs php-src's own skip loop because glibc still reports `EILSEQ`, and
/// `//TRANSLIT` needs a UTF-8 `LC_CTYPE`, which opening a converter installs.
///
/// The replacement text `//TRANSLIT` picks is the platform iconv's, not PHP's: glibc
/// approximates `\u{e9}` as `e`, GNU libiconv as `'e`. PHP reports whichever its own
/// provider produces, so both spellings are pinned rather than one of them asserted
/// everywhere. What matters on either platform is that the character was approximated
/// instead of replaced by `?`, which is what the `LC_CTYPE` setup buys.
#[test]
fn honors_translit_and_ignore_suffixes() {
    let translit =
        convert::convert(b"UTF-8", b"ASCII//TRANSLIT", "h\u{e9}llo".as_bytes()).unwrap();
    assert_eq!(translit, if cfg!(target_os = "macos") { &b"h'ello"[..] } else { &b"hello"[..] });
    assert_eq!(
        convert::convert(b"UTF-8", b"ISO-8859-1//IGNORE", "a\u{65e5}\u{672c}b".as_bytes())
            .unwrap(),
        b"ab"
    );
}

/// Verifies converters for one charset pair keep their own `//TRANSLIT` and `//IGNORE`
/// options while each other is open (#811).
///
/// Apple's iconv (the Citrus-based one since macOS 14) keys converter state on the
/// suffix-free charset pair and lets every `iconv_open()` overwrite that pair's
/// transliterate and discard flags, so the descriptor opened last decided how every live
/// descriptor of the pair converted. `iconv_mime_encode()` opens a plain `UTF-8 -> ASCII`
/// converter, which is how a MIME test running alongside turned
/// `honors_translit_and_ignore_suffixes` intermittently red. Holding the descriptors in one
/// thread makes the collision deterministic; glibc keeps descriptors independent, and there
/// this pins the same contract.
#[test]
fn converters_for_one_charset_pair_keep_their_own_suffixes() {
    let approximated = if cfg!(target_os = "macos") { &b"h'ello"[..] } else { &b"hello"[..] };
    let subject = "h\u{e9}llo".as_bytes();
    let mut translit = Converter::open(b"UTF-8", b"ASCII//TRANSLIT").unwrap();
    let mut plain = Converter::open(b"UTF-8", b"ASCII").unwrap();
    assert_eq!(translit.convert_all(subject).unwrap(), approximated);
    assert_eq!(plain.convert_all(subject), Err(IconvError::IllegalSequence));

    let mut discard = Converter::open(b"UTF-8", b"ASCII//IGNORE").unwrap();
    assert_eq!(plain.convert_all(subject), Err(IconvError::IllegalSequence));
    assert_eq!(translit.convert_all(subject).unwrap(), approximated);
    assert_eq!(discard.convert_all_ignoring(subject, true).unwrap(), b"hllo");
}

/// Apple's discard mode must reach libc even after a plain descriptor resets the charset pair.
#[cfg(target_vendor = "apple")]
#[test]
fn apple_ignore_restores_descriptor_discard_mode_without_userspace_skipping() {
    let subject = "h\u{e9}llo".as_bytes();
    let mut discard = Converter::open(b"UTF-8", b"ASCII//IGNORE").unwrap();
    let mut plain = Converter::open(b"UTF-8", b"ASCII").unwrap();

    for _ in 0..3 {
        assert_eq!(plain.convert_all(subject), Err(IconvError::IllegalSequence));
        // convert_all has no illegal-sequence skip loop: libc must discard the character.
        assert_eq!(discard.convert_all(subject).unwrap(), b"hllo");
    }
    assert_eq!(plain.convert_all(subject), Err(IconvError::IllegalSequence));
}

/// Verifies opening a converter changes only `LC_CTYPE`, leaving `LC_NUMERIC` in `C`.
///
/// The bridge installs a UTF-8 character-classification locale for `//TRANSLIT`, but
/// Apple's libc numbers `LC_CTYPE` 2 and uses 0 for `LC_ALL`, so on macOS and iOS the first
/// conversion replaced every category of the process locale, decimal separator included.
#[test]
fn opening_a_converter_changes_only_lc_ctype() {
    drop(Converter::open(b"UTF-8", b"ISO-8859-1").unwrap());
    assert_eq!(crate::ffi::numeric_locale_name(), b"C");
}

/// Verifies the search pair stops at its first match instead of scanning a malformed tail.
///
/// php-src's scanner records a conversion failure only once it has produced the character
/// that precedes the bad bytes, so a match found earlier still wins.
#[test]
fn search_stops_before_a_malformed_tail() {
    assert_eq!(
        search::strpos(b"abc\xc3(def", b"a", 0, None).unwrap(),
        Some(0)
    );
    assert!(matches!(
        search::strpos(b"abc\xc3(def", b"c", 0, None),
        Err(search::SearchFailure::Conversion(IconvError::IllegalSequence))
    ));
    assert_eq!(search::strpos(b"\xe9", b"l", 0, None).unwrap(), None);
    assert!(matches!(
        search::strpos(b"\xe9", b"l", 1, None),
        Err(search::SearchFailure::OffsetOutOfRange)
    ));
}

/// Verifies character counting against both a multibyte and a single-byte charset.
#[test]
fn counts_characters_per_charset() {
    assert_eq!(search::strlen("h\u{e9}llo".as_bytes(), None).unwrap(), 5);
    assert_eq!(
        search::strlen("h\u{e9}llo".as_bytes(), Some(b"ISO-8859-1")).unwrap(),
        6
    );
    assert_eq!(search::strlen(b"", None).unwrap(), 0);
}

/// Verifies PHP's `substr()` offset and length conventions on characters, not bytes.
#[test]
fn slices_by_character_offsets() {
    let subject = "h\u{e9}llo".as_bytes();
    assert_eq!(search::substr(subject, 1, Some(3), None).unwrap(), "\u{e9}ll".as_bytes());
    assert_eq!(search::substr(subject, -3, None, None).unwrap(), b"llo");
    assert_eq!(search::substr(subject, 1, Some(-1), None).unwrap(), "\u{e9}ll".as_bytes());
    assert_eq!(search::substr(subject, 10, None, None).unwrap(), b"");
    assert_eq!(search::substr(subject, 1, Some(0), None).unwrap(), b"");
    assert_eq!(search::substr(subject, -99, Some(2), None).unwrap(), "h\u{e9}".as_bytes());
}

/// Verifies whole-subject conversion preserves shift state while counting and slicing.
#[test]
fn stateful_charset_slices_round_trip() {
    let subject =
        convert::convert(b"UTF-8", b"ISO-2022-JP", "\u{65e5}\u{672c}\u{8a9e}".as_bytes()).unwrap();
    assert_eq!(search::strlen(&subject, Some(b"ISO-2022-JP")).unwrap(), 3);

    for (index, expected) in ["\u{65e5}", "\u{672c}", "\u{8a9e}"].iter().enumerate() {
        let slice =
            search::substr(&subject, index as i64, Some(1), Some(b"ISO-2022-JP")).unwrap();
        let decoded = convert::convert(b"ISO-2022-JP", b"UTF-8", &slice).unwrap();
        assert_eq!(decoded, expected.as_bytes());
    }
}

/// Verifies forward and backward search, including PHP's empty-needle and offset rules.
#[test]
fn finds_character_positions() {
    let subject = "h\u{e9}llo".as_bytes();
    assert_eq!(search::strpos(subject, b"l", 0, None).unwrap(), Some(2));
    assert_eq!(search::strpos(subject, b"l", -2, None).unwrap(), Some(3));
    assert_eq!(search::strpos(subject, b"z", 0, None).unwrap(), None);
    assert_eq!(search::strpos(subject, b"", 0, None).unwrap(), None);
    assert_eq!(search::strrpos(b"abcabc", b"bc", None).unwrap(), Some(4));
    assert!(matches!(
        search::strpos(subject, b"l", 99, None),
        Err(search::SearchFailure::OffsetOutOfRange)
    ));
}

/// Verifies that base64 encoded-words fold at php-src's line budget.
#[test]
fn encodes_mime_words_with_folding() {
    let options = MimeEncodeOptions::default();
    let encoded = mime::encode::mime_encode(
        b"Subject",
        "Pr\u{fc}fung Pr\u{fc}fung Pr\u{fc}fung Pr\u{fc}fung Pr\u{fc}fung Pr\u{fc}fung".as_bytes(),
        &options,
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(encoded).unwrap(),
        "Subject: =?UTF-8?B?UHLDvGZ1bmcgUHLDvGZ1bmcgUHLDvGZ1bmcgUHLDvGZ1bmc=?=\r\n \
         =?UTF-8?B?IFByw7xmdW5nIFByw7xmdW5n?="
    );
}

/// Verifies the quoted-printable scheme and its escaping table.
#[test]
fn encodes_quoted_printable_words() {
    let options = MimeEncodeOptions {
        scheme: Scheme::QuotedPrintable,
        ..MimeEncodeOptions::default()
    };
    let encoded =
        mime::encode::mime_encode(b"Subject", "Pr\u{fc}fung ok".as_bytes(), &options).unwrap();
    assert_eq!(
        String::from_utf8(encoded).unwrap(),
        "Subject: =?UTF-8?Q?Pr=C3=BCfung=20ok?="
    );
}

/// Verifies decoding of encoded-words mixed with literal header text.
#[test]
fn decodes_mime_words() {
    let decoded = mime::decode::mime_decode(
        b"plain =?UTF-8?Q?Pr=C3=BCfung?= tail",
        0,
        None,
    )
    .unwrap();
    assert_eq!(decoded, "plain Pr\u{fc}fung tail".as_bytes());
}

/// Verifies that whitespace between adjacent encoded-words disappears.
#[test]
fn drops_whitespace_between_adjacent_words() {
    let decoded = mime::decode::mime_decode(b"=?UTF-8?Q?a?= =?UTF-8?Q?b?=", 0, None).unwrap();
    assert_eq!(decoded, b"ab");
}

/// Verifies that a malformed encoded-word fails unless the caller opts to continue.
#[test]
fn malformed_words_respect_continue_on_error() {
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?X?zz?=", 0, None).unwrap_err(),
        IconvError::MalformedString
    );
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?X?zz?=", crate::MODE_CONTINUE_ON_ERROR, None).unwrap(),
        b"=?UTF-8?X?zz?="
    );
}

/// Verifies malformed quoted-printable escapes fail or preserve the raw word by mode.
#[test]
fn malformed_quoted_printable_words_respect_continue_on_error() {
    for payload in [
        b"=ZZ".as_slice(),
        b"=Z",
        b"a=Zb",
        b"==41",
        b"ab=  ",
        b"=  X",
        b"= X",
        b"=\t\tX",
        b"=  _",
        b"=  =41",
        b"ab=  X",
    ] {
        let mut word = b"=?UTF-8?Q?".to_vec();
        word.extend_from_slice(payload);
        word.extend_from_slice(b"?=");

        assert_eq!(
            mime::decode::mime_decode(&word, 0, None).unwrap_err(),
            IconvError::MalformedString
        );
        assert_eq!(
            mime::decode::mime_decode(&word, crate::MODE_CONTINUE_ON_ERROR, None).unwrap(),
            word
        );
    }
}

/// Verifies only hexadecimal digits and line endings may follow a soft-break whitespace run.
#[test]
fn quoted_printable_soft_break_acceptance_matches_php() {
    for byte in 0u8..=u8::MAX {
        let accepted = mime::quoted_printable::decode(&[b'=', b' ', b' ', byte]).is_some();
        assert_eq!(
            accepted,
            byte.is_ascii_hexdigit() || matches!(byte, b'\r' | b'\n'),
            "unexpected acceptance for byte 0x{byte:02x}"
        );
    }
}

/// Verifies php-src's accepted trailing and linear-whitespace `Q` escape edge cases.
#[test]
fn quoted_printable_escape_edges_match_php() {
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?Q?a=?=", 0, None).unwrap(),
        b"a"
    );
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?Q?= 41?=", 0, None).unwrap(),
        b"1"
    );
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?Q?=\t41?=", 0, None).unwrap(),
        b"1"
    );
    for input in [
        b"=?UTF-8?Q?=  41?=".as_slice(),
        b"=?UTF-8?Q?=   41?=",
        b"=?UTF-8?Q?=\t\t41?=",
        b"=?UTF-8?Q?= \t41?=",
        b"=?UTF-8?Q?=\t 41?=",
    ] {
        assert_eq!(mime::decode::mime_decode(input, 0, None).unwrap(), b"1");
    }
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?Q?a=  41b?=", 0, None).unwrap(),
        b"a1b"
    );
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?Q?=  41=42?=", 0, None).unwrap(),
        b"1B"
    );
    for (input, expected) in [
        (b"=?UTF-8?Q?=\n41?=".as_slice(), b"41".as_slice()),
        (b"=?UTF-8?Q?=\r41?=", b"41"),
        (b"=?UTF-8?Q?=\r\n41?=", b"41"),
        (b"=?UTF-8?Q?ab=\n?=", b"ab"),
    ] {
        assert_eq!(mime::decode::mime_decode(input, 0, None).unwrap(), expected);
    }
}

/// Verifies header-block decoding rejects malformed quoted-printable encoded-words.
#[test]
fn malformed_quoted_printable_header_fails() {
    assert_eq!(
        mime::decode::mime_decode_headers(b"Subject: =?UTF-8?Q?=ZZ?=", 0, None).unwrap_err(),
        IconvError::MalformedString
    );
    assert_eq!(
        mime::decode::mime_decode_headers(
            b"Subject: =?UTF-8?Q?=  X?=\r\nFrom: a@b.c\r\n",
            0,
            None,
        )
        .unwrap_err(),
        IconvError::MalformedString
    );
}

/// Verifies that strict mode keeps an undelimited encoded-word literal.
#[test]
fn strict_mode_requires_delimited_words() {
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?Q?a?=x", crate::MODE_STRICT, None).unwrap(),
        b"=?UTF-8?Q?a?=x"
    );
    assert_eq!(
        mime::decode::mime_decode(b"=?UTF-8?Q?a?=x", 0, None).unwrap(),
        b"ax"
    );
}

/// Verifies that repeated header names collect every value in order.
#[test]
fn decodes_headers_into_ordered_entries() {
    let headers = mime::decode::mime_decode_headers(
        b"Subject: =?ISO-8859-1?Q?Pr=FCfung?=\r\nTo: a@b.c\r\nTo: d@e.f\r\n\r\nbody",
        0,
        None,
    )
    .unwrap();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0].0, b"Subject");
    assert_eq!(headers[0].1, vec!["Pr\u{fc}fung".as_bytes().to_vec()]);
    assert_eq!(headers[1].0, b"To");
    assert_eq!(
        headers[1].1,
        vec![b"a@b.c".to_vec(), b"d@e.f".to_vec()]
    );
}

/// Verifies that a folded continuation line joins with a single space.
#[test]
fn folded_headers_join_with_one_space() {
    let headers = mime::decode::mime_decode_headers(b"A: 1\r\n 2\r\nB: 3", 0, None).unwrap();
    assert_eq!(headers[0].1, vec![b"1 2".to_vec()]);
    assert_eq!(headers[1].1, vec![b"3".to_vec()]);
}

/// Verifies that the encoding trio starts at UTF-8 and follows `iconv_set_encoding()`.
#[test]
fn tracks_the_encoding_trio() {
    use crate::encoding_state::{get, set, EncodingKind};
    assert_eq!(get(EncodingKind::Input), "UTF-8");
    assert!(set(EncodingKind::Output, b"ISO-8859-15"));
    assert_eq!(get(EncodingKind::Output), "ISO-8859-15");
    assert!(set(EncodingKind::Output, b"UTF-8"));
}

/// Verifies that the packed array format round-trips ordered entries.
#[test]
fn packs_entries_with_length_prefixes() {
    let packed = crate::abi::result::pack_entries(&[(b"A".to_vec(), vec![b"1".to_vec()])]);
    let mut expected = Vec::new();
    expected.extend_from_slice(&1u64.to_ne_bytes());
    expected.extend_from_slice(&1u64.to_ne_bytes());
    expected.push(b'A');
    expected.extend_from_slice(&1u64.to_ne_bytes());
    expected.extend_from_slice(&1u64.to_ne_bytes());
    expected.push(b'1');
    assert_eq!(packed, expected);
}
