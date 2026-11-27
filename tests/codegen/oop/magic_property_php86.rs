//! Purpose:
//! PHP 8.6 magic-property `??`/`empty()` materialization behavior.
//!
//! Called from:
//! - `cargo test --test codegen_tests_oop magic_property_php86`.
//!
//! Key details:
//! - `__isset()` that writes the name into the property table makes `??`/`empty()`
//!   read that value directly instead of calling `__get()`; `isset()` is unchanged.
//! - The 8.5 profile keeps the pre-8.6 `__isset` then `__get` sequence.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// Compiles and runs one fixture with the PHP 8.5 profile.
fn run85(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php85)
}

/// `??` returns the value that `__isset()` materialized instead of calling `__get()`.
#[test]
fn test_magic_property_coalesce_materializes_php86() {
    let source = r#"<?php
#[AllowDynamicProperties]
class CoalesceMaterializing {
    public function __isset(string $name): bool {
        if ($name === "x") {
            $this->x = 42;
            return true;
        }
        return false;
    }
    public function __get(string $name): mixed {
        echo "get:", $name, ";";
        return 99;
    }
}
$c = new CoalesceMaterializing();
echo $c->x ?? "fallback", "|", isset($c->x) ? "set" : "unset", "\n";
"#;
    assert_eq!(run86(source), "42|set\n");
    assert_eq!(run85(source), "get:x;99|set\n");
}

/// `empty()` tests the value materialized by `__isset()` without calling `__get()`.
#[test]
fn test_magic_property_empty_materializes_php86() {
    let source = r#"<?php
#[AllowDynamicProperties]
class EmptyMaterializing {
    public int $value = 0;
    public function __isset(string $name): bool {
        if ($name === "x") {
            $this->x = $this->value;
            return true;
        }
        return false;
    }
    public function __get(string $name): mixed {
        echo "get:", $name, ";";
        return 99;
    }
}
$zero = new EmptyMaterializing();
$zero->value = 0;
echo empty($zero->x) ? "empty" : "full", "\n";
$full = new EmptyMaterializing();
$full->value = 7;
echo empty($full->x) ? "empty" : "full", "\n";
$c = new EmptyMaterializing();
echo $c->x ?? "fallback", "\n";
"#;
    assert_eq!(run86(source), "empty\nfull\n0\n");
    assert_eq!(run85(source), "get:x;full\nget:x;full\nget:x;99\n");
}

/// A truthful `__isset()` that stores nothing still reads through `__get()`.
#[test]
fn test_magic_property_unmaterialized_still_calls_get_php86() {
    let source = r#"<?php
class VirtualProperty {
    public function __isset(string $name): bool {
        return true;
    }
    public function __get(string $name): mixed {
        echo "get:", $name, ";";
        return 7;
    }
}
$v = new VirtualProperty();
echo $v->x ?? "fallback", "\n";
echo empty($v->x) ? "empty" : "full", "\n";
"#;
    assert_eq!(run86(source), "get:x;7\nget:x;full\n");
}
