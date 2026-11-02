//! Purpose:
//! Checks deferred overwrite cleanup when a loop promotes a local to a reference cell.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - All five targets must retire the payload on both raw and promoted predecessor paths.

use crate::codegen::platform::Target;
use crate::ir::{Immediate, Op, RuntimeCallTarget};
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

/// A consuming nested write must clear the old owner even if promotion occurs later in the loop.
#[test]
fn nested_loop_mutation_defers_reference_owner_retirement_on_every_target() {
    let source = r#"<?php
function createPaths(array &$loops): void {
    $path = ['a', []];
    foreach (['b', 'c'] as $key) {
        $path[1][$key] = true;
        $loops[$key][] = &$path;
    }
}
$loops = [];
createPaths($loops);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("createPaths")).unwrap();
        let path = function.locals.iter()
            .find(|local| local.name.as_deref() == Some("path")).unwrap().id;
        let fetch = function.instructions.iter().position(|inst| {
            inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite))
        }).expect("nested parent uses a consuming fetch-for-write");
        assert!(function.instructions[..fetch].iter().any(|inst| {
            inst.op == Op::ReleaseLocalSlot && inst.immediate == Some(Immediate::LocalSlot(path))
        }), "{name}: consuming mutation must retire the runtime slot, not a detached owner load");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(asm.contains("refcounted_writeback_release_ref_cell"),
            "{name}: back-edge reference promotion needs representation-aware retirement");
    }
}
