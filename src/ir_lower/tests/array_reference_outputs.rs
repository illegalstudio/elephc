//! Purpose:
//! Verifies caller storage and builtin result types after declared PHP array reference calls.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Validation and assembly emission cover every supported target without running a native binary.

use crate::codegen::platform::Target;
use std::path::Path;

/// Positional, named, callable, method and conditional reference calls preserve keyed output types.
#[test]
fn php_array_reference_outputs_lower_on_every_target() {
    let source = r#"<?php
function addKey(array &$items): bool { $items["added"] = 7; return true; }
class ArrayWriter {
    public function write(array &$items): void { $items["method"] = 8; }
}
$direct = [1];
addKey($direct);
echo implode(",", array_keys($direct));
$named = [2];
addKey(items: $named);
echo implode(",", array_keys($named));
$callback = addKey(...);
$callable = [3];
$callback($callable);
echo implode(",", array_keys($callable));
$writer = new ArrayWriter();
$method = [4];
$writer->write(items: $method);
echo implode(",", array_keys($method));
$conditional = [5];
$hash = ["old" => 6];
$argc > 0 && addKey($conditional);
$argc > 0 && addKey($hash);
echo implode(",", array_keys($conditional)), implode(",", array_keys($hash));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
