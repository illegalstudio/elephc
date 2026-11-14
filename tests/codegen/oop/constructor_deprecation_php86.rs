//! Purpose:
//! Runs php-src 8.6's constructor/destructor return-value deprecation fixture.
//!
//! Called from:
//! - `cargo test --test codegen_tests constructor_deprecation_php86`.
//!
//! Key details:
//! - The source is transcribed from
//!   `Zend/tests/magic_methods/constructor_destructor_return.phpt`.
//! - php-src's `--EXPECTF--` wildcards the file and line; elephc's synthetic startup call
//!   has no user source location, so the test compares the six message prefixes and their
//!   order instead of the location suffix.

use crate::support::*;

/// Emits one deprecation per offending constructor/destructor, in declaration order.
#[test]
fn test_constructor_deprecation_php86_php_src_fixture() {
    let out = compile_and_run_capture_merged(r#"<?php

class A {
    public function __construct() { return ''; }
    public function __destruct() { return ''; }
}

class B {
    public function __construct() { return $this->voidMethod(); }
    public function __destruct() { return $this->voidMethod(); }

    public function voidMethod(): void { }
}

class Gen {
    public function __construct() { yield ''; }
    public function __destruct() { yield ''; }
}

"#);
    let messages: Vec<&str> = out.stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split(" in ").next().unwrap_or(line))
        .collect();
    assert_eq!(
        messages,
        vec![
            "Deprecated: Returning a value from a constructor is deprecated",
            "Deprecated: Returning a value from a destructor is deprecated",
            "Deprecated: Returning a value from a constructor is deprecated",
            "Deprecated: Returning a value from a destructor is deprecated",
            "Deprecated: Making a constructor a Generator is deprecated",
            "Deprecated: Making a destructor a Generator is deprecated",
        ]
    );
}
