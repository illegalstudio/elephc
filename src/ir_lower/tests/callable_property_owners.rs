//! Purpose:
//! Checks the source-owner contract for retained callable property stores.
//!
//! Called from:
//! - The AST-to-EIR test module.
//!
//! Key details:
//! - Temporary descriptors need explicit retirement; borrowed parameters remain owned by callers.

use crate::codegen::platform::Target;
use crate::ir::Op;
use crate::types::PhpType;
use std::path::Path;

/// Callable property stores retire temporary descriptors without consuming borrowed parameters.
#[test]
fn callable_property_stores_balance_temporary_sources_on_every_target() {
    let source = r#"<?php
class PropertyCallableOwner {
    public $callback;
    public function install(int $number): void {
        $this->callback = static fn(): int => $number;
    }
    public function borrow(callable $callback): void {
        $this->callback = $callback;
    }
}
$owner = new PropertyCallableOwner();
$owner->install(42);
$callback = static fn(): int => 7;
$owner->borrow($callback);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        for (method_name, retires_source) in [("install", true), ("borrow", false)] {
            let method = module.class_methods.iter()
                .find(|method| method.name == format!("PropertyCallableOwner::{method_name}"))
                .unwrap();
            let store_index = method.instructions.iter().position(|inst| inst.op == Op::PropSet).unwrap();
            let source = method.instructions[store_index].operands[1];
            assert_eq!(method.value(source).unwrap().php_type.codegen_repr(), PhpType::Callable, "{name}");
            let released = method.instructions[store_index + 1..].iter()
                .any(|inst| inst.op == Op::Release && inst.operands == [source]);
            assert_eq!(released, retires_source, "{name}: {method_name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
