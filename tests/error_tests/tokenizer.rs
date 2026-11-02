//! Purpose:
//! Pins tokenizer argument diagnostics and PhpToken's final constructor contract.
//!
//! Called from:
//! - The frontend error suite in CI, without linking the bridge.
//!
//! Key details:
//! - Every failure checks a tokenizer-specific diagnostic rather than missing injection.

use super::*;

/// Both tokenizer functions reject missing and surplus arguments.
#[test]
fn tokenizer_function_arity_errors() {
    for (source, name) in [
        ("<?php token_get_all();", "token_get_all"),
        ("<?php token_get_all('x', 0, 1);", "token_get_all"),
        ("<?php token_name();", "token_name"),
        ("<?php token_name(1, 2);", "token_name"),
    ] {
        let error = check_source(source).expect_err("invalid arity must fail");
        assert!(error.contains(name) && !error.contains("Undefined"), "{error}");
    }
}

/// Tokenizer string and integer parameters reject array input.
#[test]
fn tokenizer_function_argument_type_errors() {
    for source in ["<?php token_get_all([]);", "<?php token_name([]);"] {
        let error = check_source(source).expect_err("invalid type must fail");
        assert!(!error.contains("Undefined"), "{error}");
        assert!(error.contains("token_") && (error.contains("string") || error.contains("int")), "{error}");
    }
}

/// PHP permits subclassing PhpToken but forbids replacing its final constructor.
#[test]
fn tokenizer_phptoken_constructor_is_final() {
    let error = check_source("<?php class BadToken extends PhpToken { public function __construct() {} }")
        .expect_err("final constructor override must fail");
    assert!(error.contains("final") && !error.contains("Undefined"), "{error}");
}
