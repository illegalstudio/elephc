//! Purpose:
//! Checks PHP 8.6's magic-method validation for enum `__debugInfo()`.
//!
//! Called from:
//! - `cargo test --test error_tests enum_debug_info`.
//!
//! Key details:
//! - Sources are transcribed from
//!   `Zend/tests/enum/debugInfo/{param_validation,return_validation}.phpt`.
//! - php-src reports the same two failures as fatal errors at declaration time; elephc
//!   rejects them while compiling. The visibility-warning fixture stays out until the
//!   runtime dump path can warn and still render `enum(E::C) (0) { }`.

use super::*;

/// php-src `param_validation.phpt`.
#[test]
fn test_enum_debug_info_rejects_parameters_php86() {
    expect_error(
        r#"<?php

enum Foo {
	case Bar;

	public function __debugInfo(mixed $arg): array {
		return [];
	}
}

var_dump(Foo::Bar);

"#,
        "Method Foo::__debugInfo() cannot take arguments",
    );
}

/// php-src `return_validation.phpt`.
#[test]
fn test_enum_debug_info_rejects_non_array_return_php86() {
    expect_error(
        r#"<?php

enum Foo {
	case Bar;

	public function __debugInfo(): int {
		return 5;
	}
}

var_dump(Foo::Bar);

"#,
        "Foo::__debugInfo(): Return type must be ?array",
    );
}
