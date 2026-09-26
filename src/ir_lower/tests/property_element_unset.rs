//! Purpose:
//! Pins how EIR lowering removes an element of an array held in an object or static property
//! (`unset($this->data[$k])`, `unset(self::$cache[$k])`, issue #750), and which property shapes
//! it refuses with a source diagnostic.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Both instance shapes fetch the property with `PropGetForWrite` and remove in place, with no
//!   acquired copy in flight and no `PropSet` after the removal: the removed value's destructor
//!   may throw or write the property again.
//! - A packed-list property and an untyped static array are refused, naming the declared `array`
//!   type as the fix.

use crate::codegen::platform::Target;
use crate::ir::{Function, Op};
use crate::ir_lower::LoweringError;
use std::path::Path;

/// The five first-class targets every property-element removal must agree on.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Lowers `source` for the host target and returns the refusal message it must produce.
fn refusal(source: &str) -> crate::errors::CompileError {
    match super::try_lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::detect_host(),
    ) {
        Ok(_) => panic!("expected EIR lowering to refuse this program, but it lowered"),
        Err(LoweringError::Unsupported(error)) => error,
        Err(other) => panic!("expected an unsupported-shape refusal, got {other:?}"),
    }
}

/// Returns the opcodes of `function` in emission order.
fn ops(function: &Function) -> Vec<Op> {
    function.instructions.iter().map(|inst| inst.op).collect()
}

/// An associative property and a declared-`array` property both fetch the container for write
/// and remove in place; neither acquires a copy nor stores one back.
#[test]
fn property_element_unset_removes_in_place_on_every_target() {
    let source = r#"<?php
class Store {
    public $items = ["x" => 1, "y" => 2];
    public array $typed = ["p" => 1, "q" => 2];
    public function dropItem(string $k): void { unset($this->items[$k]); }
    public function dropTyped(string $k): void { unset($this->typed[$k]); }
}
$s = new Store();
$s->dropItem("x");
$s->dropTyped("p");
echo count($s->items), count($s->typed);
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        for (method, remove) in [("Store::dropItem", Op::HashUnset), ("Store::dropTyped", Op::OffsetUnset)] {
            let function = module
                .class_methods
                .iter()
                .find(|function| function.name == method)
                .unwrap_or_else(|| panic!("{name}: {method} was not lowered"));
            let ops = ops(function);
            let fetch = ops
                .iter()
                .position(|op| *op == Op::PropGetForWrite)
                .unwrap_or_else(|| panic!("{name}: {method} must fetch the property for write"));
            let removal = ops
                .iter()
                .position(|op| *op == remove)
                .unwrap_or_else(|| panic!("{name}: {method} must remove through {remove:?}"));
            assert_eq!(
                fetch + 1,
                removal,
                "{name}: {method} removes straight from the fetched container: {ops:?}"
            );
            assert!(
                !ops.iter().any(|op| matches!(op, Op::PropSet | Op::PropGet)),
                "{name}: {method} must not hold or store back a copy of the property: {ops:?}"
            );
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A declared-`array` static property is separated and removed through `OffsetUnset`.
#[test]
fn static_property_element_unset_separates_then_removes_on_every_target() {
    let source = r#"<?php
class Cache {
    public static array $cache = ["a" => 1, "b" => 2];
    public static function forget(string $k): void { unset(self::$cache[$k]); }
}
Cache::forget("a");
echo count(Cache::$cache);
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let function = module
            .class_methods
            .iter()
            .find(|function| function.name == "Cache::forget")
            .unwrap_or_else(|| panic!("{name}: Cache::forget was not lowered"));
        let ops = ops(function);
        let clone = ops.iter().position(|op| *op == Op::MixedClone);
        let removal = ops.iter().position(|op| *op == Op::OffsetUnset);
        assert!(
            matches!((clone, removal), (Some(clone), Some(removal)) if clone < removal),
            "{name}: the static cell is separated before the removal: {ops:?}"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// An untyped property whose default is a list keeps packed storage every method reads, so an
/// element removal is refused at its line with the declared-`array` fix.
#[test]
fn packed_list_property_element_unset_is_refused() {
    let error = refusal(
        r#"<?php
class L {
    public $list = [10, 20, 30];
    public function drop(int $i): void { unset($this->list[$i]); }
}
$l = new L();
$l->drop(1);
"#,
    );
    assert!(
        error.message.contains("removing an element of `$list` needs hash storage")
            && error.message.contains("Declare the property `array`"),
        "{}",
        error.message
    );
    assert_eq!(error.span.line, 4, "the refusal names the unset line");
}

/// An untyped static array property has no separated-cell path, so its element removal is
/// refused with the declared-`array` fix.
#[test]
fn untyped_static_array_element_unset_is_refused() {
    let error = refusal(
        r#"<?php
class Cache {
    public static $untyped = ["a" => "x", "b" => "y"];
    public static function forget(string $k): void { unset(self::$untyped[$k]); }
}
Cache::forget("a");
"#,
    );
    assert!(
        error.message.contains("an element of static property `$untyped`")
            && error.message.contains("declared `array`"),
        "{}",
        error.message
    );
    assert_eq!(error.span.line, 4, "the refusal names the unset line");
}
