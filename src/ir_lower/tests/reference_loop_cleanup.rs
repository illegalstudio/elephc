//! Purpose:
//! Checks deferred overwrite cleanup when a loop promotes a local to a reference cell.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - All five targets must retire the payload on both raw and promoted predecessor paths.

use crate::codegen::platform::Target;
use crate::ir::Op;
use std::path::Path;

/// A back-edge promotion cannot turn a deferred payload retirement into a no-op.
#[test]
fn captured_loop_reinitialization_retires_raw_and_reference_payloads_on_every_target() {
    let source = r#"<?php
for ($i = 0; $i < 4; $i++) {
    $counter = 0;
    $increment = function() use (&$counter): void { $counter++; };
    $increment();
    echo $counter;
}
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        assert!(module.functions.iter().flat_map(|function| &function.instructions)
            .any(|inst| inst.op == Op::ReleaseLocalSlot), "{name}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(asm.contains("refcounted_writeback_release_ref_cell"),
            "{name}: reinitialization must retire the promoted cell's payload too");
        assert!(asm.contains("__rt_decref_mixed"), "{name}");
    }
}
