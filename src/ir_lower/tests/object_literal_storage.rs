//! Purpose:
//! Regression tests for the element storage EIR lowering gives an indexed array literal whose
//! items are instances of different classes (Code Journal task 80b3dd39).
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - The checker infers the common ancestor as the element of such a literal, so lowering has
//!   to allocate object slots of that class. Boxed `mixed` cells bound to a declared
//!   `array<Base>` slot were read as object pointers.
//! - Classes with no shared class ancestor still allocate boxed `mixed` cells in both layers.

use crate::ir::Op;
use crate::types::PhpType;

/// Every supported target; lowering must type the literal the same way on all of them.
const TARGETS: [&str; 5] =
    ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"];

/// Returns the PHP type of the first indexed array allocated by `function` on `target`.
fn first_literal_type(source: &str, target: &str, function: &str) -> PhpType {
    let module = super::lower_source_at_for_target(
        source,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let function = module
        .functions
        .iter()
        .find(|candidate| candidate.name.eq_ignore_ascii_case(function))
        .unwrap_or_else(|| panic!("{target}: {function} is lowered"));
    let array = function
        .instructions
        .iter()
        .find(|inst| inst.op == Op::ArrayNew)
        .and_then(|inst| inst.result)
        .unwrap_or_else(|| panic!("{target}: {} allocates its literal", function.name));
    function.value(array).expect("the literal value is defined").php_type.clone()
}

/// Sibling classes, a base and child mixture, and an interface with one implementation all
/// keep object slots typed as their common class, while unrelated classes still box.
#[test]
fn related_class_literals_keep_object_storage_on_all_targets() {
    let source = r#"<?php
interface Shape {}
abstract class Base implements Shape {}
final class First extends Base {}
final class Second extends Base {}
final class Stranger {}
function siblings(): void { array<Base> $items = [new First(), new First(), new Second()]; echo count($items); }
function inferred(): void { $items = [new Second(), new First()]; echo count($items); }
function mixture(Base $base): void { $items = [new Second(), $base, new First()]; echo count($items); }
function shapes(Shape $shape): void { $items = [new First(), $shape]; echo count($items); }
function unrelated(): void { $items = [new First(), new Stranger()]; echo count($items); }
siblings(); inferred(); mixture(new First()); shapes(new Second()); unrelated();
"#;
    let object_array = |class: &str| PhpType::Array(Box::new(PhpType::Object(class.to_string())));
    for target in TARGETS {
        for function in ["siblings", "inferred", "mixture"] {
            assert_eq!(
                first_literal_type(source, target, function),
                object_array("Base"),
                "{target}: {function}"
            );
        }
        assert_eq!(first_literal_type(source, target, "shapes"), object_array("Shape"), "{target}");
        assert_eq!(
            first_literal_type(source, target, "unrelated"),
            PhpType::Array(Box::new(PhpType::Mixed)),
            "{target}"
        );
    }
}
