//! Purpose:
//! Runs php-src 8.6's Debugable Enums fixtures through the native backend.
//!
//! Called from:
//! - `cargo test --test codegen_tests enum_debug_info_php86`.
//!
//! Key details:
//! - Sources and expected outputs are transcribed from
//!   `Zend/tests/enum/debugInfo/{backed_enum_value,magic_method,missing_magic}.phpt`.
//! - `param_validation` and `return_validation` stay in the error suite; the
//!   visibility fixture warns and still dumps the empty debug-info body.

use elephc::php_version::PhpVersion;

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

/// php-src `visibility_validation.phpt`: a non-public method warns and still dumps.
#[test]
fn test_enum_debug_info_visibility_validation_php86_php_src_fixture() {
    let out = compile_and_run_capture_merged_with_php_version(
        r#"<?php

enum Foo {
	case Bar;

	private function __debugInfo(): array {
		return [];
	}
}

var_dump(Foo::Bar);

"#,
        PhpVersion::Php86,
    );
    assert!(
        out.contains("Warning: The magic method Foo::__debugInfo() must have public visibility"),
        "{out}"
    );
    assert!(out.contains("enum(Foo::Bar) (0) {\n}"), "{out}");
}

/// PHP only warns about visibility: the non-public method still supplies the entries.
#[test]
fn test_enum_debug_info_private_method_returns_entries_php86() {
    let out = compile_and_run(
        r#"<?php

enum Foo {
	case Bar;

	private function __debugInfo(): array {
		return ["x" => 1];
	}
}

var_dump(Foo::Bar);

"#,
    );
    assert!(out.contains("enum(Foo::Bar) (1) {"), "{out}");
    assert!(out.contains("[\"x\"]=>"), "{out}");
    assert!(out.contains("int(1)"), "{out}");
}

/// An empty public `__debugInfo()` renders the empty body, not the plain enum line.
#[test]
fn test_enum_debug_info_empty_array_php86() {
    let out = compile_and_run(
        r#"<?php

enum Foo {
	case Bar;

	public function __debugInfo(): array {
		return [];
	}
}

var_dump(Foo::Bar);

"#,
    );
    assert_eq!(out.trim_end(), "enum(Foo::Bar) (0) {\n}");
}
