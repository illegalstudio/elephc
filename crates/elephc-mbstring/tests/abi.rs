//! Purpose:
//! Tests the mbstring bridge's wire ownership, binary errors, and request isolation.
//!
//! Called from:
//! - Cargo's focused mbstring ABI integration test binary.
//!
//! Key details:
//! - Calls exercise the actual exported C functions rather than bypassing dispatch.
//! - Every allocated payload is released before its borrowed argument storage dies.

use elephc_builtin_contract::{mbstring_abi::*, RuntimeBuiltinId};
use elephc_mbstring::abi::{elephc_mbstring_call_v1, elephc_mbstring_release_v1, elephc_mbstring_reset_v1};

/// Copies a live wire buffer before its owning bridge result is released.
unsafe fn copy(bytes: *const u8, len: u64) -> Vec<u8> {
    if len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(bytes, len as usize).to_vec() } }
}

/// Executes a real ABI call and returns safe copies after releasing its wire result twice.
fn call(op: u32, args: &[MbArgV1]) -> (u64, i64, Vec<u8>, Vec<u8>) {
    let mut result = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(op, args.as_ptr(), args.len() as u64, &mut result); }
    let copied = (result.kind, result.value, unsafe { copy(result.bytes, result.len) },
        unsafe { copy(result.diagnostics, result.diagnostics_len) });
    unsafe { elephc_mbstring_release_v1(&mut result); elephc_mbstring_release_v1(&mut result); }
    assert!(result.bytes.is_null() && result.diagnostics.is_null());
    copied
}

/// Verifies the exported strlen dispatch covers canonical codecs and binary diagnostics.
#[test]
fn mbstring_abi_strlen_and_errors() {
    elephc_mbstring_reset_v1();
    let op = RuntimeBuiltinId::MbStrlen.as_u32();
    for encoding in elephc_mbstring::encoding::Encoding::all() {
        let subject = b"\0A\x80\xff\x1b$B!!\x1b(B";
        let result = call(op, &[MbArgV1::string(subject), MbArgV1::string(encoding.name().as_bytes())]);
        assert_eq!((result.0, result.1), (RESULT_INT, encoding.strlen(subject) as i64), "{}", encoding.name());
    }
    assert_eq!(call(op, &[MbArgV1::string("日本語".as_bytes())]).1, 3);
    assert_eq!(call(op, &[MbArgV1::string("日本語".as_bytes()), MbArgV1::null()]).1, 3);
    assert_eq!(call(op, &[MbArgV1::string(b"ab"), MbArgV1::string(b"UTF-8\0ignored")]).1, 2);
    let result = call(op, &[MbArgV1::string(b"ab"), MbArgV1::string(b"bad\xff\0ignored")]);
    assert_eq!(result.0, RESULT_VALUE_ERROR);
    assert_eq!(result.2, b"mb_strlen(): Argument #2 ($encoding) must be a valid encoding, \"bad\xff\" given");
}

/// Verifies deprecation state persists across calls but resets for a new request or thread.
#[test]
fn mbstring_abi_request_cache_isolation() {
    elephc_mbstring_reset_v1();
    let op = RuntimeBuiltinId::MbStrlen.as_u32();
    let args = [MbArgV1::string(b"YQ=="), MbArgV1::string(b"BASE64")];
    let first = call(op, &args);
    assert_eq!(first.1, 1);
    assert_eq!(first.3, b"Deprecated: mb_strlen(): Handling Base64 via mbstring is deprecated; use base64_encode/base64_decode instead\n");
    assert!(call(op, &args).3.is_empty());
    assert!(!std::thread::spawn(move || call(op, &[MbArgV1::string(b"YQ=="), MbArgV1::string(b"BASE64")]).3).join().unwrap().is_empty());
    assert!(call(op, &args).3.is_empty());
    elephc_mbstring_reset_v1();
    assert_eq!(call(op, &args).3, first.3);
}

/// Verifies malformed metadata and unknown operation IDs fail without fabricated PHP values.
#[test]
fn mbstring_abi_fail_closed() {
    let op = RuntimeBuiltinId::MbStrlen.as_u32();
    assert_eq!(call(u32::MAX, &[]).0, RESULT_UNSUPPORTED);
    assert_eq!(call(RuntimeBuiltinId::Abs.as_u32(), &[MbArgV1::integer(1)]).0, RESULT_UNSUPPORTED);
    assert_eq!(call(op, &[]).0, RESULT_UNSUPPORTED);
    assert_eq!(call(op, &[MbArgV1::integer(1)]).0, RESULT_FATAL);
    let invalid = MbArgV1 { kind: ARG_STRING, value: 0, bytes: std::ptr::null(), len: 1 };
    assert_eq!(call(op, &[invalid]).0, RESULT_FATAL);
    let mut out = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(op, std::ptr::null(), 1, &mut out); }
    assert_eq!(out.kind, RESULT_FATAL);
    unsafe { elephc_mbstring_release_v1(&mut out); elephc_mbstring_release_v1(std::ptr::null_mut()); }
}

/// Verifies every exposed text operation owns its result and resolves arguments through the engine.
#[test]
fn mbstring_abi_text_operations() {
    let cases = [
        (RuntimeBuiltinId::MbStrwidth, "漢字abc", RESULT_INT, 7, ""),
        (RuntimeBuiltinId::MbStrtoupper, "Straße", RESULT_STRING, 0, "STRASSE"),
        (RuntimeBuiltinId::MbStrtolower, "ΟΔΟΣ", RESULT_STRING, 0, "οδος"),
        (RuntimeBuiltinId::MbUcfirst, "ßeta", RESULT_STRING, 0, "Sseta"),
        (RuntimeBuiltinId::MbLcfirst, "İstanbul", RESULT_STRING, 0, "i\u{307}stanbul"),
    ];
    for (operation, subject, kind, value, expected) in cases {
        let result = call(operation.as_u32(), &[MbArgV1::string(subject.as_bytes())]);
        assert_eq!((result.0, result.1, result.2.as_slice()), (kind, value, expected.as_bytes()));
    }
    let folded = call(RuntimeBuiltinId::MbConvertCase.as_u32(), &[
        MbArgV1::string("Straße".as_bytes()), MbArgV1::integer(3), MbArgV1::null(),
    ]);
    assert_eq!((folded.0, folded.2.as_slice()), (RESULT_STRING, b"strasse".as_slice()));
    let trimmed = call(RuntimeBuiltinId::MbStrimwidth.as_u32(), &[
        MbArgV1::string("漢字abc".as_bytes()), MbArgV1::integer(1), MbArgV1::integer(4), MbArgV1::string(b"!"),
    ]);
    assert_eq!((trimmed.0, trimmed.2.as_slice()), (RESULT_STRING, "字a!".as_bytes()));
}

/// Verifies diagnostics survive later ValueErrors and the explicit-encoding cache spans functions.
#[test]
fn mbstring_abi_text_diagnostic_order() {
    elephc_mbstring_reset_v1();
    let result = call(RuntimeBuiltinId::MbConvertCase.as_u32(), &[
        MbArgV1::string(b"YQ=="), MbArgV1::integer(8), MbArgV1::string(b"BASE64"),
    ]);
    assert_eq!(result.0, RESULT_VALUE_ERROR);
    assert_eq!(result.2, b"mb_convert_case(): Argument #2 ($mode) must be one of the MB_CASE_* constants");
    assert!(result.3.starts_with(b"Deprecated: mb_convert_case(): Handling Base64"));
    let reused = call(RuntimeBuiltinId::MbStrwidth.as_u32(), &[MbArgV1::string(b"YQ=="), MbArgV1::string(b"BASE64")]);
    assert!(reused.3.is_empty());
    let operation = RuntimeBuiltinId::MbStrimwidth.as_u32();
    let invalid_start = call(operation, &[MbArgV1::string(b"abc"), MbArgV1::integer(4), MbArgV1::integer(-99)]);
    assert_eq!(invalid_start.0, RESULT_VALUE_ERROR);
    assert!(invalid_start.3.is_empty());
    let invalid_width = call(operation, &[MbArgV1::string(b"abc"), MbArgV1::integer(0), MbArgV1::integer(-99)]);
    assert_eq!(invalid_width.0, RESULT_VALUE_ERROR);
    assert_eq!(invalid_width.3, b"Deprecated: mb_strimwidth(): passing a negative integer to argument #3 ($width) is deprecated\n");
}

/// Verifies nullable lengths, boolean inputs, and optional scalar outputs cross the C boundary.
#[test]
fn mbstring_abi_scalar_kinds_and_defaults() {
    elephc_mbstring_reset_v1();
    let subject = MbArgV1::string("a猫b".as_bytes());
    let tail = call(RuntimeBuiltinId::MbSubstr.as_u32(), &[subject, MbArgV1::integer(1), MbArgV1::null()]);
    assert_eq!((tail.0, tail.2.as_slice()), (RESULT_STRING, "猫b".as_bytes()));
    assert!(call(RuntimeBuiltinId::MbSubstr.as_u32(), &[subject, MbArgV1::integer(1), MbArgV1::integer(0)]).2.is_empty());
    let prefix = call(RuntimeBuiltinId::MbStrstr.as_u32(), &[subject, MbArgV1::string("猫".as_bytes()), MbArgV1::boolean(true)]);
    assert_eq!((prefix.0, prefix.2.as_slice()), (RESULT_STRING, b"a".as_slice()));
    for (operation, args) in [
        (RuntimeBuiltinId::MbOrd, vec![MbArgV1::string(b"\xff")]),
        (RuntimeBuiltinId::MbChr, vec![MbArgV1::integer(-1)]),
        (RuntimeBuiltinId::MbStrpos, vec![subject, MbArgV1::string(b"missing")]),
        (RuntimeBuiltinId::MbStrstr, vec![subject, MbArgV1::string(b"missing")]),
    ] {
        let result = call(operation.as_u32(), &args);
        assert_eq!((result.0, result.1), (RESULT_BOOL, 0));
        assert!(result.2.is_empty());
    }
    let zero = call(RuntimeBuiltinId::MbOrd.as_u32(), &[MbArgV1::string(b"\0")]);
    assert_eq!((zero.0, zero.1), (RESULT_INT, 0));
    let empty = call(RuntimeBuiltinId::MbStrstr.as_u32(), &[MbArgV1::string(b""), MbArgV1::string(b"")]);
    assert_eq!(empty.0, RESULT_STRING);
    assert!(empty.2.is_empty());
    let malformed = MbArgV1 { kind: ARG_BOOL, value: 2, ..MbArgV1::null() };
    assert_eq!(call(RuntimeBuiltinId::MbStrstr.as_u32(), &[subject, subject, malformed]).0, RESULT_FATAL);
}

/// Verifies zero-argument getters, cross-operation state, and reset/thread isolation.
#[test]
fn mbstring_abi_settings_and_zero_argument_pointer() {
    elephc_mbstring_reset_v1();
    let get = RuntimeBuiltinId::MbInternalEncoding.as_u32();
    let mut out = MbResultV1::default();
    unsafe { elephc_mbstring_call_v1(get, std::ptr::null(), 0, &mut out); }
    assert_eq!(out.kind, RESULT_STRING);
    assert_eq!(unsafe { copy(out.bytes, out.len) }, b"UTF-8");
    unsafe { elephc_mbstring_release_v1(&mut out); }
    assert_eq!(call(get, &[MbArgV1::string(b"ISO-8859-1")]).0, RESULT_BOOL);
    assert_eq!(call(get, &[MbArgV1::null()]).2, b"ISO-8859-1");
    assert_eq!(call(RuntimeBuiltinId::MbStrlen.as_u32(), &[MbArgV1::string("é".as_bytes())]).1, 2);
    assert_eq!(std::thread::spawn(move || call(get, &[]).2).join().unwrap(), b"UTF-8");
    assert_eq!(call(get, &[]).2, b"ISO-8859-1");
    assert_eq!(call(RuntimeBuiltinId::MbLanguage.as_u32(), &[MbArgV1::string(b"Japanese")]).1, 1);
    assert_eq!(call(RuntimeBuiltinId::MbLanguage.as_u32(), &[]).2, b"Japanese");
    assert_eq!(call(RuntimeBuiltinId::MbHttpOutput.as_u32(), &[MbArgV1::string(b"pass")]).1, 1);
    assert_eq!(call(RuntimeBuiltinId::MbHttpOutput.as_u32(), &[]).2, b"pass");
    elephc_mbstring_reset_v1();
    assert_eq!(call(get, &[]).2, b"UTF-8");
    assert_eq!(call(RuntimeBuiltinId::MbLanguage.as_u32(), &[]).2, b"neutral");
}

/// Verifies prechecks do not emit or populate the explicit encoding deprecation cache.
#[test]
fn mbstring_abi_preencoding_validation_order() {
    for (operation, args, prefix) in [
        (RuntimeBuiltinId::MbOrd, vec![MbArgV1::string(b""), MbArgV1::string(b"BASE64")], "mb_ord(): Argument #1"),
        (RuntimeBuiltinId::MbSubstrCount, vec![MbArgV1::string(b"YQ=="), MbArgV1::string(b""), MbArgV1::string(b"BASE64")], "mb_substr_count(): Argument #2"),
        (RuntimeBuiltinId::MbSubstr, vec![MbArgV1::string(b"YQ=="), MbArgV1::integer(i64::MIN), MbArgV1::null(), MbArgV1::string(b"BASE64")], "mb_substr(): Argument #2"),
        (RuntimeBuiltinId::MbConvertKana, vec![MbArgV1::string(b"YQ=="), MbArgV1::string(b"!"), MbArgV1::string(b"BASE64")], "mb_convert_kana(): Argument #2"),
    ] {
        elephc_mbstring_reset_v1();
        let result = call(operation.as_u32(), &args);
        assert_eq!(result.0, RESULT_VALUE_ERROR);
        assert!(result.2.starts_with(prefix.as_bytes()));
        assert!(result.3.is_empty());
        assert!(!call(RuntimeBuiltinId::MbStrlen.as_u32(), &[MbArgV1::string(b"YQ=="), MbArgV1::string(b"BASE64")]).3.is_empty());
    }
}

/// Verifies all encoding metadata results against the independent PHP reflection fixture.
#[test]
fn mbstring_abi_encoding_metadata() {
    let baseline: serde_json::Value = serde_json::from_str(include_str!("../../../scripts/mbstring/php_surface.json")).unwrap();
    for (name, expected) in baseline["encodings"].as_object().unwrap() {
        let aliases = call(RuntimeBuiltinId::MbEncodingAliases.as_u32(), &[MbArgV1::string(name.as_bytes())]);
        assert_eq!(aliases.0, RESULT_STRING_ARRAY, "{name}");
        let strings = decode_string_array(&aliases.2, aliases.1 as usize).expect("valid alias framing");
        let strings: Vec<_> = strings.into_iter().map(|bytes| std::str::from_utf8(bytes).unwrap()).collect();
        assert_eq!(serde_json::to_value(strings).unwrap(), expected["aliases"], "{name}");
        let mime = call(RuntimeBuiltinId::MbPreferredMimeName.as_u32(), &[MbArgV1::string(name.as_bytes())]);
        if let Some(expected) = expected["mime"].as_str().filter(|name| !name.is_empty()) {
            assert_eq!((mime.0, mime.2.as_slice()), (RESULT_STRING, expected.as_bytes()), "{name}");
            assert!(mime.3.is_empty(), "{name}");
        } else {
            assert_eq!((mime.0, mime.1), (RESULT_BOOL, 0), "{name}");
            assert_eq!(mime.3, format!("Warning: mb_preferred_mime_name(): No MIME preferred name corresponding to \"{name}\"\n").as_bytes());
        }
    }
}

/// Returns all canonical names through owned wire framing without disturbing request settings.
#[test]
fn mbstring_abi_list_encodings() {
    elephc_mbstring_reset_v1();
    call(RuntimeBuiltinId::MbInternalEncoding.as_u32(), &[MbArgV1::string(b"SJIS")]);
    let result = call(RuntimeBuiltinId::MbListEncodings.as_u32(), &[]);
    assert_eq!(result.0, RESULT_STRING_ARRAY);
    assert!(result.3.is_empty());
    let names: Vec<_> = decode_string_array(&result.2, result.1 as usize).unwrap()
        .into_iter().map(|bytes| std::str::from_utf8(bytes).unwrap()).collect();
    let baseline: serde_json::Value = serde_json::from_str(
        include_str!("../../../scripts/mbstring/php_surface.json")).unwrap();
    assert_eq!(serde_json::to_value(names).unwrap(), baseline["encoding_order"]);
    assert_eq!(call(RuntimeBuiltinId::MbInternalEncoding.as_u32(), &[]).2, b"SJIS");
    assert_eq!(call(RuntimeBuiltinId::MbListEncodings.as_u32(), &[MbArgV1::null()]).0, RESULT_UNSUPPORTED);
}

/// Verifies string-array results preserve empty arrays, binary elements, and fixed-width tails.
#[test]
fn mbstring_abi_split_arrays() {
    for (subject, length, encoding, expected) in [
        ("Aé猫B".as_bytes(), 2, "UTF-8", vec!["Aé".as_bytes(), "猫B".as_bytes()]),
        (b"".as_slice(), 1, "UTF-8", Vec::new()),
        (b"\0\xff\x80".as_slice(), 2, "8bit", vec![b"\0\xff".as_slice(), b"\x80".as_slice()]),
        (b"A\0\xff".as_slice(), 1, "UCS-2LE", vec![b"A\0".as_slice(), b"\xff".as_slice()]),
        (b"A\0\xff".as_slice(), 1, "UTF-16LE", vec![b"A\0".as_slice(), b"?\0".as_slice()]),
    ] {
        let result = call(RuntimeBuiltinId::MbStrSplit.as_u32(), &[
            MbArgV1::string(subject), MbArgV1::integer(length), MbArgV1::string(encoding.as_bytes()),
        ]);
        assert_eq!(result.0, RESULT_STRING_ARRAY, "{encoding}");
        assert_eq!(decode_string_array(&result.2, result.1 as usize), Some(expected), "{encoding}");
    }
    for (length, suffix) in [(0, "must be greater than 0"), (-1, "must be greater than 0"), (1073741824, "is too large")] {
        let result = call(RuntimeBuiltinId::MbStrSplit.as_u32(), &[
            MbArgV1::string(b"a"), MbArgV1::integer(length), MbArgV1::string(b"invalid"),
        ]);
        assert_eq!(result.0, RESULT_VALUE_ERROR);
        assert_eq!(result.2, format!("mb_str_split(): Argument #2 ($length) {suffix}").as_bytes());
    }
}

/// Verifies MIME lookup bypasses the encoding cache while aliases use ordinary lookup semantics.
#[test]
fn mbstring_abi_metadata_cache_and_binary_names() {
    elephc_mbstring_reset_v1();
    let alias = RuntimeBuiltinId::MbEncodingAliases.as_u32();
    let mime = RuntimeBuiltinId::MbPreferredMimeName.as_u32();
    let base64 = [MbArgV1::string(b"BASE64")];
    assert!(!call(alias, &base64).3.is_empty());
    assert_eq!(call(mime, &[MbArgV1::string(b"UTF-8")]).2, b"UTF-8");
    assert!(call(alias, &base64).3.is_empty());
    let unavailable = call(mime, &[MbArgV1::string(b"UTF7-IMAP\0tail")]);
    assert_eq!((unavailable.0, unavailable.1), (RESULT_BOOL, 0));
    assert_eq!(unavailable.3, b"Warning: mb_preferred_mime_name(): No MIME preferred name corresponding to \"UTF7-IMAP\"\n");
    assert!(call(alias, &base64).3.is_empty());
    let utf = call(alias, &[MbArgV1::string(b"UTF-8\0ignored")]);
    assert_eq!(decode_string_array(&utf.2, utf.1 as usize), Some(vec![b"utf8".as_slice()]));
    assert!(!call(alias, &base64).3.is_empty());
    for (op, name) in [(alias, "mb_encoding_aliases"), (mime, "mb_preferred_mime_name")] {
        let result = call(op, &[MbArgV1::string(b"bad\xff\0tail")]);
        assert_eq!(result.0, RESULT_VALUE_ERROR);
        let mut expected = format!("{name}(): Argument #1 ($encoding) must be a valid encoding, \"bad").into_bytes();
        expected.extend_from_slice(b"\xff\" given");
        assert_eq!(result.2, expected);
    }
}

/// Preserves substitution codepoints, named modes, failures, and binary replacement output.
#[test]
fn mbstring_abi_substitution_settings() {
    elephc_mbstring_reset_v1();
    let op = RuntimeBuiltinId::MbSubstituteCharacter.as_u32();
    assert_eq!(call(op, &[]), (RESULT_INT, 63, vec![], vec![]));
    for code in [0, 33, 0xd7ff, 0xe000, 0x10ffff] {
        assert_eq!(call(op, &[MbArgV1::integer(code)]), (RESULT_BOOL, 1, vec![], vec![]));
        assert_eq!(call(op, &[MbArgV1::null()]), (RESULT_INT, code, vec![], vec![]));
    }
    call(op, &[MbArgV1::integer(33)]);
    for (mode, canonical, replacement) in [(b"NoNe".as_slice(), b"none".as_slice(), b"".as_slice()),
        (b"LONG", b"long", b"!"), (b"entity", b"entity", b"!")] {
        assert_eq!(call(op, &[MbArgV1::string(mode)]).0, RESULT_BOOL);
        assert_eq!(call(op, &[]), (RESULT_STRING, 0, canonical.to_vec(), vec![]));
        assert_eq!(call(RuntimeBuiltinId::MbScrub.as_u32(), &[MbArgV1::string(&[255])]).2, replacement);
    }
    for invalid in [b"".as_slice(), b"65", b"none\0ignored", b" none", b"\xff"] {
        let result = call(op, &[MbArgV1::string(invalid)]);
        assert_eq!(result.0, RESULT_VALUE_ERROR);
        assert_eq!(result.2, b"mb_substitute_character(): Argument #1 ($substitute_character) must be \"none\", \"long\", \"entity\" or a valid codepoint");
        assert_eq!(call(op, &[]).2, b"entity");
    }
    for invalid in [-1, 0xd800, 0xdfff, 0x110000, i64::MAX, i64::MIN] {
        let result = call(op, &[MbArgV1::integer(invalid)]);
        assert_eq!(result.0, RESULT_VALUE_ERROR);
        assert_eq!(result.2, b"mb_substitute_character(): Argument #1 ($substitute_character) is not a valid codepoint");
        assert_eq!(call(op, &[]).2, b"entity");
    }
    assert_eq!(call(op, &[MbArgV1::boolean(true)]).0, RESULT_FATAL);
    call(op, &[MbArgV1::integer(0)]);
    assert_eq!(call(RuntimeBuiltinId::MbScrub.as_u32(), &[MbArgV1::string(&[255])]).2, [0]);
}

/// Isolates substitution settings by thread and resets the remembered character for new requests.
#[test]
fn mbstring_abi_substitution_request_isolation() {
    elephc_mbstring_reset_v1();
    let op = RuntimeBuiltinId::MbSubstituteCharacter.as_u32();
    call(op, &[MbArgV1::integer(33)]);
    call(op, &[MbArgV1::string(b"none")]);
    assert_eq!(std::thread::spawn(move || call(op, &[])).join().unwrap().1, 63);
    assert_eq!(call(op, &[]).2, b"none");
    elephc_mbstring_reset_v1();
    assert_eq!(call(op, &[]), (RESULT_INT, 63, vec![], vec![]));
}
