//! Purpose:
//! Verifies shared Core query configuration, raw INI ownership, and request lifecycle.
//!
//! Called from:
//! - The focused mbstring integration harness with a real native PCRE2 provider.
//!
//! Key details:
//! - One ABI test owns process configuration; other tests use independent pure state.
//! - Query callbacks read the same mutations returned by the Core INI ABI.

use std::ffi::c_void;
use elephc_builtin_contract::mbstring_abi::{*, ini::*, coercion::MbHostStringV1, invoke::{MbQueryConfigV1, QUERY_CONFIG_ENTRY, QUERY_CONFIG_FIELD, QUERY_CONFIG_DIAGNOSTIC}};
use elephc_mbstring::{abi::*, state::{CoreIni, IniString}};

#[path = "support/mime_provider.rs"]
mod mime_provider;

/// Retains the supplied raw setting pairs without text normalization.
fn settings(values: &[(&[u8], &[u8])]) -> Vec<(Vec<u8>, Vec<u8>)> {
    values.iter().map(|(name, value)| (name.to_vec(), value.to_vec())).collect()
}

/// Checks quantity suffixes, rejected negative/empty settings, final override selection, and NUL boundaries.
#[test]
fn core_ini_startup_validation_and_raw_values() {
    let (state, warnings) = CoreIni::with_overrides(&settings(&[
        (b"max_input_vars", b"1"), (b"max_input_vars", b"2K"),
        (b"max_input_nesting_level", b"-1"), (b"arg_separator.input", b""),
        (b"display_errors", b"stderr"), (b"MAX_INPUT_VARS", b"0"),
    ]));
    assert!(warnings.is_empty());
    assert_eq!(state.max_variables(), 2048);
    assert_eq!(state.max_nesting(), 64);
    assert_eq!(state.separators(), b"&");
    assert_eq!(state.display_errors(), 2);
    assert_eq!(&*state.get(b"max_input_vars").unwrap(), b"2K");
    assert_eq!(&*state.get(b"max_input_nesting_level").unwrap(), b"64");
    assert!(state.get(b"display_errors\0").is_none());
    let (state, warnings) = CoreIni::with_overrides(&settings(&[
        (b"arg_separator.input", b";&\0ignored"), (b"max_input_vars", b"3cats"),
        (b"max_input_nesting_level", b"0"),
    ]));
    assert_eq!(state.separators(), b";&");
    assert_eq!(&*state.get(b"arg_separator.input").unwrap(), b";&\0ignored");
    assert_eq!((state.max_variables(), state.max_nesting()), (3, 0));
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].level, 2);
    assert_eq!(warnings[0].message, b"Invalid \"max_input_vars\" setting. Invalid quantity \"3cats\": unknown multiplier \"s\", interpreting as \"3\" for backwards compatibility");
}

/// Preserves parsed display mode, exact raw values, modification permissions, and startup restoration.
#[test]
fn core_ini_runtime_access_display_modes_and_restore() {
    let mut state = CoreIni::default();
    for (value, mode) in [(b"0".as_slice(), 0), (b"on", 1), (b"stderr", 2),
        (b"256", 0), (b"257", 1), (b"258", 2), (b"259", 1), (b"-256", 0),
        (b"false", 0), (b"on\0ignored", 0), (b" 2tail", 2)] {
        let previous = state.get(b"display_errors").unwrap();
        assert_eq!(state.set(b"display_errors", IniString::fresh(value)).unwrap(), previous);
        assert_eq!(state.display_errors(), mode, "{value:?}");
        assert_eq!(&*state.get(b"display_errors").unwrap(), value);
    }
    for name in [b"arg_separator.input".as_slice(), b"max_input_vars", b"max_input_nesting_level", b"missing"] {
        assert!(state.set(name, IniString::new(b"0")).is_none());
        state.restore(name);
    }
    assert_eq!((state.max_variables(), state.max_nesting(), state.separators()), (1000, 64, b"&".as_slice()));
    state.restore(b"display_errors");
    assert_eq!(&*state.get(b"display_errors").unwrap(), b"1");
    assert_eq!(state.display_errors(), 1);
}

/// Pins PHP 8.5's mail defaults, runtime access, port conversion, and request restoration.
#[test]
fn core_ini_mail_state_tracks_php_access_and_defaults() {
    let mut state = CoreIni::default();
    assert_eq!(state.smtp_host(), b"localhost");
    assert_eq!(state.smtp_port(), 25);
    assert_eq!(state.sendmail_from(), None);
    #[cfg(windows)]
    assert_eq!(state.sendmail_path(), None);
    #[cfg(not(windows))]
    assert_eq!(state.sendmail_path(), Some(b"/usr/sbin/sendmail -t -i".as_slice()));
    assert_eq!(state.mail_force_extra_parameters(), None);
    assert_eq!(state.mail_cr_lf_mode(), b"crlf");
    assert!(!state.mail_mixed_lf_and_crlf());

    assert_eq!(&*state.set(b"SMTP", IniString::new(b"127.0.0.1")).unwrap(), b"localhost");
    assert_eq!(&*state.set(b"smtp_port", IniString::new(b"0")).unwrap(), b"25");
    assert_eq!(&*state.set(b"sendmail_from", IniString::new(b"sender@example.test")).unwrap(), b"");
    assert_eq!(state.smtp_host(), b"127.0.0.1");
    assert_eq!(state.smtp_port(), 25);
    assert_eq!(state.sendmail_from(), Some(b"sender@example.test".as_slice()));
    assert!(state.set(b"sendmail_path", IniString::new(b"mailer")).is_none());
    assert!(state.set(b"mail.cr_lf_mode", IniString::new(b"invalid")).is_none());
    for value in [b"".as_slice(), b"crlf", b"lf", b"mixed", b"os"] {
        assert!(state.set(b"mail.cr_lf_mode", IniString::new(value)).is_none());
        let (configured, diagnostics) = CoreIni::with_overrides(&settings(&[
            (b"mail.cr_lf_mode", value),
        ]));
        assert!(diagnostics.is_empty(), "{value:?}");
        assert_eq!(configured.mail_cr_lf_mode(), value);
    }
    state.reset();
    assert_eq!(state.smtp_host(), b"localhost");
    assert_eq!(state.smtp_port(), 25);
    assert_eq!(state.sendmail_from(), None);
    assert_eq!(state.mail_cr_lf_mode(), b"crlf");
}

/// Invalid startup mail line-ending policy emits PHP's startup error and retains `crlf`.
#[test]
fn core_ini_invalid_mail_cr_lf_mode_retains_default() {
    let (state, diagnostics) = CoreIni::with_overrides(&settings(&[
        (b"mail.cr_lf_mode", b"invalid"),
    ]));
    assert_eq!(state.mail_cr_lf_mode(), b"crlf");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].level, 1);
    assert_eq!(diagnostics[0].message,
        b"Invalid value \"invalid\" for mail.cr_lf_mode. Must be one of: \"crlf\", \"lf\", \"mixed\", \"os\"");
}

/// Keeps `ini_get_all()` name-sorted while preserving nullable global/local values.
#[test]
fn core_ini_mail_inventory_is_sorted_and_nullable() {
    let state = CoreIni::default();
    let (graph, owners) = state.all(true);
    let root = &graph.arrays()[0];
    let names: Vec<_> = root.iter().map(|(key, _)| match key {
        elephc_builtin_contract::mbstring_abi::array::Key::String(name) => name.as_slice(),
        _ => panic!("Core INI keys are strings"),
    }).collect();
    assert_eq!(names, [b"SMTP".as_slice(), b"arg_separator.input", b"display_errors",
        b"mail.cr_lf_mode", b"mail.force_extra_parameters", b"mail.mixed_lf_and_crlf",
        b"max_input_nesting_level", b"max_input_vars", b"sendmail_from", b"sendmail_path",
        b"smtp_port"]);
    let null_details = graph.arrays().iter().skip(1).flat_map(|entries| entries.iter())
        .filter(|(_, value)| matches!(value, elephc_builtin_contract::mbstring_abi::array::Value::Null))
        .count();
    #[cfg(windows)]
    assert_eq!(null_details, 6);
    #[cfg(not(windows))]
    assert_eq!(null_details, 4);
    assert_eq!(owners.len() + null_details, 22);
}

/// Keeps original global/raw identities while scalar getters normalize one-byte strings.
#[test]
fn core_ini_array_and_scalar_identity_contracts() {
    let mut state = CoreIni::default();
    let raw = IniString::fresh(b"0");
    state.set(b"display_errors", raw.clone()).unwrap();
    assert!(!state.get(b"display_errors").unwrap().same_identity(&raw));
    let (_, plain) = state.all(false);
    assert!(plain.iter().find(|(array, entry, _)| *array == 0 && *entry == 2).unwrap().2.same_identity(&raw));
    let (_, details) = state.all(true);
    assert!(details.iter().find(|(array, entry, _)| *array == 3 && *entry == 1).unwrap().2.same_identity(&raw));
    let original = details.iter().find(|(array, entry, _)| *array == 3 && *entry == 0).unwrap().2.clone();
    state.reset();
    assert!(state.get(b"display_errors").unwrap().same_identity(&original));
    assert_eq!(&*raw, b"0");
}

/// Records startup warnings after complete state installation and without borrowing live request state.
unsafe extern "C" fn diagnostic(context: *mut c_void, level: u32, bytes: *const u8, length: u64) -> i32 {
    if !context.is_null() {
        let warnings = unsafe { &mut *context.cast::<Vec<(u32, Vec<u8>)>>() };
        let bytes = if length == 0 { &[] } else { unsafe { std::slice::from_raw_parts(bytes, length as usize) } };
        warnings.push((level, bytes.to_vec()));
    }
    0
}

/// Calls the real Core ABI with already-coerced arguments and returns its independently owned result.
fn call(op: u32, args: &[MbArgV1]) -> MbResultV1 {
    let host = MbIniHostV1 { version: 1, size: 24, context: std::ptr::null_mut(), diagnostic: Some(diagnostic) };
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_core_ini_v1(op, args.as_ptr(), args.len() as u64, &host, &mut result) }, 0);
    result
}

/// Copies borrowed query separator bytes before any subsequent host call can retire the request.
fn query(phase: u32) -> (Vec<u8>, i64, i64, u64) {
    let mut output = MbQueryConfigV1 { separators: MbHostStringV1 { bytes: std::ptr::null(), len: 0, owner: std::ptr::null_mut() },
        max_variables: -1, max_nesting: -1, display_errors: 99 };
    assert_eq!(unsafe { elephc_mbstring_query_configuration_v1(std::ptr::null_mut(), phase, &mut output) }, 0);
    assert!(output.separators.owner.is_null());
    let bytes = unsafe { std::slice::from_raw_parts(output.separators.bytes, output.separators.len as usize) }.to_vec();
    (bytes, output.max_variables, output.max_nesting, output.display_errors)
}

/// Connects actual startup, live mutation, V5 reads, request reset, and thread initialization through one bridge.
#[test]
fn core_ini_abi_and_query_provider_share_request_state() {
    let provider = mime_provider::provider();
    assert_eq!(unsafe { elephc_mbstring_mime_provider_v1(&provider) }, 0);
    let arguments = [b"UTF-8".as_slice(), b"UTF-8", b"UTF-8", b"arg_separator.input", b";&\0rest",
        b"max_input_vars", b"2", b"max_input_nesting_level", b"1", b"display_errors", b"256"]
        .map(MbArgV1::string);
    let mut warnings: Vec<(u32, Vec<u8>)> = Vec::new();
    let host = MbIniHostV1 { version: 1, size: 24, context: (&mut warnings as *mut Vec<_>).cast(), diagnostic: Some(diagnostic) };
    let mut result = MbResultV1::default();
    assert_eq!(unsafe { elephc_mbstring_configure_v1(arguments.as_ptr(), arguments.len() as u64, &host, &mut result) }, 0);
    unsafe { elephc_mbstring_release_v1(&mut result); }
    assert!(warnings.is_empty());
    assert_eq!(query(QUERY_CONFIG_ENTRY), (b";&".to_vec(), 2, 1, 0));
    let name = MbArgV1::string(b"display_errors");
    let mut previous = call(INI_SET, &[name, MbArgV1::string(b"stderr")]);
    assert_eq!(previous.kind, RESULT_INI_STRING);
    assert_eq!(unsafe { std::slice::from_raw_parts(previous.bytes, previous.len as usize) }, b"256");
    assert_eq!(query(QUERY_CONFIG_DIAGNOSTIC), (b";&".to_vec(), 2, 1, 1));
    let mut all = call(INI_GET_ALL, &[MbArgV1::boolean(true)]);
    let bytes = unsafe { std::slice::from_raw_parts(all.bytes, all.len as usize) };
    let (graph, owners) = decode_ini_array(bytes, all.value as u64).unwrap();
    assert_eq!(graph.arrays()[0].len(), 11);
    #[cfg(windows)]
    assert_eq!(owners.len(), 16);
    #[cfg(not(windows))]
    assert_eq!(owners.len(), 18);
    unsafe { elephc_mbstring_release_v1(&mut all); }
    std::thread::spawn(|| assert_eq!(query(QUERY_CONFIG_ENTRY), (b";&".to_vec(), 2, 1, 0))).join().unwrap();
    assert_eq!(query(QUERY_CONFIG_FIELD).3, 1);
    let mut restored = call(INI_SET, &[name, string_argument(previous.value as u64)]);
    unsafe { elephc_mbstring_release_v1(&mut restored); elephc_mbstring_release_v1(&mut previous); }
    assert_eq!(query(QUERY_CONFIG_DIAGNOSTIC).3, 0);
    let mut changed = call(INI_SET, &[name, MbArgV1::string(b"1")]);
    unsafe { elephc_mbstring_release_v1(&mut changed); }
    let mut restored = call(INI_RESTORE, &[name]);
    assert_eq!(restored.kind, RESULT_NULL);
    unsafe { elephc_mbstring_release_v1(&mut restored); }
    assert_eq!(query(QUERY_CONFIG_DIAGNOSTIC).3, 0);
    let mut changed = call(INI_SET, &[name, MbArgV1::string(b"stderr")]);
    unsafe { elephc_mbstring_release_v1(&mut changed); }
    elephc_mbstring_reset_v1();
    assert_eq!(query(QUERY_CONFIG_ENTRY), (b";&".to_vec(), 2, 1, 0));
    let mut invalid = MbQueryConfigV1 { separators: MbHostStringV1 { bytes: b"old".as_ptr(), len: 3, owner: 1_usize as *mut c_void },
        max_variables: -1, max_nesting: -1, display_errors: 99 };
    assert_eq!(unsafe { elephc_mbstring_query_configuration_v1(std::ptr::null_mut(), 99, &mut invalid) }, 1);
    assert!(invalid.separators.owner.is_null() && invalid.separators.bytes.is_null());
    assert_eq!((invalid.separators.len, invalid.max_variables, invalid.max_nesting, invalid.display_errors), (0, 0, 0, 0));
    assert_eq!(unsafe { elephc_mbstring_query_configuration_v1(std::ptr::null_mut(), 99, std::ptr::null_mut()) }, 1);
}
