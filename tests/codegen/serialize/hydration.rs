//! Purpose:
//! End-to-end tests for `unserialize()` hydration of compiled (AOT) classes.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the serialize codegen test module.
//!
//! Key details:
//! - The interpreter hydrates a payload through a native/AOT branch once the compiled registry
//!   already answers `class_exists()`, which is exactly what an earlier `new` of the same class
//!   leaves behind. PHP writes a protected property as the mangled key `"\0*\0name"`; hydration
//!   must map that key onto the DECLARED slot instead of creating a dynamic property beside a
//!   still-uninitialized typed one.
//! - Expected output was measured on PHP 8.5.10.

use crate::support::*;

/// Verifies a protected typed property is restored after compiled code has touched the class.
///
/// Without the declared-slot hydration path the restored object keeps its typed-uninitialized
/// sentinel, and the first AOT read of `Base::$bag` raises
/// `Typed property Base::$bag must not be accessed before initialization` (the Symfony
/// `ContainerBuilder::$parameterBag` failure in dev).
#[test]
fn test_unserialize_restores_protected_typed_property_after_native_class_touch() {
    let out = compile_and_run(
        r#"<?php
interface Bag { public function v(): int; }
class BagImpl implements Bag { public function v(): int { return 7; } }
class Base {
    protected Bag $bag;
    public function __construct() { $this->bag = new BagImpl(); }
    public function getBag(): Bag { return $this->bag; }
}
class Child extends Base {}

$fresh = new Child();
echo "fresh:", $fresh->getBag()->v(), "\n";

$wire = "O:5:\"Child\":1:{s:6:\"\0*\0bag\";O:7:\"BagImpl\":0:{}}";
eval('$restored = unserialize($wire); echo "restored:", $restored->getBag()->v(), "\n";');
"#,
    );
    assert_eq!(out, "fresh:7\nrestored:7\n");
}
