//! Purpose:
//! Checks boxed PHP-array unpacking through signature-based Core call lowering.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Every supported target must emit iterator-based key binding, never raw array reads of boxes.

use crate::codegen::platform::Target;
use crate::ir::Op;
use std::path::Path;

/// Returned arrays reach class-introspection specialization with runtime keys on every supported ABI.
#[test]
fn boxed_core_call_spreads_use_typed_iterators_on_every_target() {
    let source = r#"<?php
class BoxedSpreadTarget { public int $x = 7; public function m(): void {} }
function boxedSpreadNames(string $name): array { return [13 => $name]; }
function boxedSpreadObjects(): array { return ['object_or_class' => new BoxedSpreadTarget()]; }
function boxedSpreadEmpty(): array { return []; }
function boxedSpreadRepeat(): array { return ['times' => 2, 'string' => 'ok']; }
echo get_class_vars(...boxedSpreadNames(BoxedSpreadTarget::class))['x'];
echo implode(',', call_user_func('get_class_methods', ...boxedSpreadObjects()));
echo get_class_vars(...boxedSpreadEmpty(), ...boxedSpreadNames(BoxedSpreadTarget::class))['x'];
echo str_repeat(...boxedSpreadRepeat());
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let starts = module.functions.iter().flat_map(|function| &function.instructions)
            .filter(|inst| inst.op == Op::IterStart).count();
        assert!(starts >= 5, "{name}: boxed spreads must bind iterator keys");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("Unknown named parameter in unpacked call"), "{name}");
        assert!(assembly.contains("Named parameter overwrites previous argument"), "{name}");
    }
}
