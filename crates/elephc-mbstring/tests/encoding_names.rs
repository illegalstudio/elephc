//! Purpose:
//! Regression tests for PHP 8.5.13's exact encoding-name matching (GH-23958).
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test engine encoding_names::` through Rust's test harness.
//!
//! Key details:
//! - Truncated `auto` and `pass` tokens previously resolved as the full names,
//!   including an empty list element, so tests cover the API, the literal
//!   `mb_http_output()` setter, and the `mbstring.http_output` INI handler.

use elephc_mbstring::encoding::EncodingList;
use elephc_mbstring::error::MbError;
use elephc_mbstring::state::State;

/// Verifies comma-separated encoding lists accept only the complete `auto`
/// token, rejecting truncated names and trailing empty elements like PHP 8.5.13.
#[test]
fn truncated_auto_tokens_are_rejected_in_comma_lists() {
    let mut state = State::default();
    let defaults = state.detect_order().to_vec();

    for name in [b"".as_slice(), b"a", b"au", b"aut", b"AUT"] {
        let error = state
            .set_detect_order(EncodingList::CommaSeparated(name))
            .expect_err("a truncated auto token must not resolve as auto");
        match error {
            // An entirely empty list is rejected by the non-empty check rather
            // than by encoding lookup; both still refuse the truncated name.
            MbError::Value(message) if name.is_empty() => assert!(
                message.contains("must specify at least one encoding"),
                "unexpected empty-list diagnostic: {message}"
            ),
            MbError::Value(message) => assert!(
                message.contains("contains invalid encoding"),
                "unexpected diagnostic for {:?}: {message}",
                String::from_utf8_lossy(name)
            ),
            other => panic!("unexpected error for {name:?}: {other:?}"),
        }
        assert_eq!(
            state.detect_order(),
            defaults.as_slice(),
            "a rejected name must not change the detection order"
        );
    }

    state
        .set_detect_order(EncodingList::CommaSeparated(b"UTF-8,a"))
        .expect_err("a trailing truncated auto token must be rejected");
    state
        .set_detect_order(EncodingList::CommaSeparated(b"auto"))
        .expect("the exact auto token still expands");
    assert_eq!(state.detect_order(), defaults.as_slice());

    state
        .set_detect_order(EncodingList::CommaSeparated(b"UTF-8,auto"))
        .expect("auto in a comma list still expands");
    assert!(state.detect_order().len() > 1);
}

/// Verifies `mb_http_output()` accepts only the complete lowercase `pass`
/// token and rejects the truncated spellings PHP 8.5.13 now refuses.
#[test]
fn truncated_pass_tokens_are_rejected_by_http_output() {
    let mut state = State::default();

    for name in [b"p".as_slice(), b"pa", b"pas", b"PASS"] {
        let error = state
            .set_http_output(name)
            .expect_err("a truncated or uppercase pass token must not resolve as pass");
        match error {
            MbError::Value(message) => assert!(
                message.contains("must be a valid encoding"),
                "unexpected diagnostic for {name:?}: {message}"
            ),
            other => panic!("unexpected error for {name:?}: {other:?}"),
        }
        assert_eq!(
            state.http_output().name(),
            "UTF-8",
            "a rejected name must not change the output encoding"
        );
    }

    state
        .set_http_output(b"pass")
        .expect("the exact pass token is accepted");
    assert_eq!(state.http_output().name(), "pass");
}

/// Verifies the `mbstring.http_output` INI handler rejects truncated `pass`
/// tokens instead of silently enabling the pass-through encoding.
#[test]
fn truncated_pass_tokens_are_rejected_by_ini_output_handler() {
    let mut state = State::default();

    for name in [b"p".as_slice(), b"pa", b"pas", b"PASS"] {
        let update = state.ini_set(b"mbstring.http_output", name, |_| Ok(()));
        assert!(
            !update.accepted,
            "the INI handler must reject {name:?}"
        );
        assert_eq!(state.http_output().name(), "UTF-8");
    }

    let update = state.ini_set(b"mbstring.http_output", b"pass", |_| Ok(()));
    assert!(update.accepted);
    assert_eq!(state.http_output().name(), "pass");
}
