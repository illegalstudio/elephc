//! Purpose:
//! Verifies cleanup of implicit scalar-to-Mixed local loads used by entity indices.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - The original consumer fixture must release all three boxed index reads on every target.
//! - Same-storage Mixed local views remain borrowed through finalization.

use crate::codegen::platform::Target;
use crate::ir::{Immediate, Op};
use crate::types::PhpType;
use std::path::Path;

/// Each boxed index load is retired once after scalar coercion on all supported targets.
#[test]
fn entity_scalar_load_coercions_release_on_every_target() {
    let source = format!("{}\nrun(1);", include_str!("../../../tests/fixtures/entity_slot_owners.php"));
    for target in [
        "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            &source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        let function = module.class_methods.iter().find(|function| function.name == "Table::create")
            .expect("entity creation method");
        let mut boxed_reads = 0;
        for load in &function.instructions {
            if load.op != Op::LoadLocal || load.result_php_type != PhpType::Mixed {
                continue;
            }
            let Some(Immediate::LocalSlot(slot)) = load.immediate else { continue; };
            if function.locals[slot.as_raw() as usize].php_type != PhpType::Int {
                continue;
            }
            let value = load.result.expect("boxed load result");
            assert_eq!(function.instructions.iter().filter(|inst| {
                inst.op == Op::Release && inst.operands == [value]
            }).count(), 1, "{target}: {value:?} needs one scalar conversion release");
            boxed_reads += 1;
        }
        assert_eq!(boxed_reads, 3, "{target}: the original fixture has three boxed index reads");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
