//! Purpose:
//! Runs php-src 8.6's Debugable Enums fixtures through the native backend.
//!
//! Called from:
//! - `cargo test --test codegen_tests enum_debug_info_php86`.
//!
//! Key details:
//! - Sources and expected outputs are transcribed from
//!   `Zend/tests/enum/debugInfo/{backed_enum_value,magic_method,missing_magic}.phpt`.
//! - `visibility_validation`, `param_validation`, and `return_validation` stay out:
//!   elephc does not yet emit the magic-method visibility warning or reject a non-array
//!   `__debugInfo()` return type on enums.

use crate::support::*;

/// php-src `backed_enum_value.phpt`.
#[test]
fn test_enum_debug_info_backed_enum_value_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

enum Foo: string {
	case Bar = "Baz";

	public function __debugInfo() {
		return [__CLASS__ . '::' . $this->name . ' = ' . $this->value];
	}
}

var_dump(Foo::Bar);

"#);
    assert_eq!(out.trim(), r#"enum(Foo::Bar) (1) {
  [0]=>
  string(14) "Foo::Bar = Baz"
}
"#.trim());
}

/// php-src `magic_method.phpt`.
#[test]
fn test_enum_debug_info_magic_method_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

enum Foo {
	case Bar;

	public function __debugInfo() {
		return [$this->name . ' is a case of the ' . __CLASS__ . ' enum'];
	}
}

var_dump(Foo::Bar);

"#);
    assert_eq!(out.trim(), r#"enum(Foo::Bar) (1) {
  [0]=>
  string(29) "Bar is a case of the Foo enum"
}
"#.trim());
}

/// php-src `missing_magic.phpt`.
#[test]
fn test_enum_debug_info_missing_magic_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

enum Foo {
	case Bar;
}

var_dump(Foo::Bar);

"#);
    assert_eq!(out.trim(), r#"enum(Foo::Bar)
"#.trim());
}
