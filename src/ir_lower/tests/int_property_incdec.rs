//! Purpose:
//! Regression coverage for allocation-free postfix updates of plain declared `int` properties.
//!
//! Called from:
//! - The AST-to-EIR unit suite through `crate::ir_lower::tests`.
//!
//! Key details:
//! - A final class's plain int slot consumes the checked producer directly, so the parser's
//!   capture-once `$new` clone must not survive into the property write.

use crate::codegen::platform::Target;
use std::path::Path;

/// Verifies a final int property increment keeps the checked producer at the property boundary.
#[test]
fn final_int_property_increment_bypasses_the_capture_clone() {
    let source = r#"<?php
final class Counter {
    private int $count = 0;
    public function reserve(): int { return 4 * $this->count++; }
}
"#;
    let module = super::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::parse("linux-x86_64").expect("target"),
    );
    let function = module
        .class_methods
        .iter()
        .find(|function| function.name == "Counter::reserve")
        .expect("Counter::reserve");
    let text = crate::ir::print_function(function);
    assert!(
        text.contains("property_boundary_checked_to_int"),
        "int property increment must narrow at the property boundary:\n{text}"
    );
    assert!(
        !text.contains("mixed_clone"),
        "int property increment must not clone the captured boxed value:\n{text}"
    );
}
