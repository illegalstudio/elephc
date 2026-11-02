//! Purpose:
//! Verifies mutation-aware unset collection and its backend results on every target.
//!
//! Called from:
//! - The AST-to-EIR unit test suite.
//!
//! Key details:
//! - Real removals retain the existing final safe point and effectful keys cannot gate it.

use crate::codegen::platform::Target;
use crate::ir::{Op, Terminator};
use crate::types::PhpType;
use std::path::Path;

/// Both array representations gate collection on removal flags on all supported ABIs.
#[test]
fn missing_key_unset_collection_is_conditional_on_every_target() {
    let source = r#"<?php
function removeDeclaredHeaders(array &$items, string $key): void {
    unset($items[strtolower($key)], $items["other"]);
}
function nextUnsetKey(): string { echo "key"; return "absent"; }
function removeComputedHeader(array $items): void { unset($items[nextUnsetKey()]); }
function removeMixedHeader(mixed &$items): void { unset($items["other"]); }
$items = ["keep" => 1];
$key = $argc > 0 ? "absent" : "missing";
unset($items[strtolower($key)], $items["other"]);
$declared = ["keep" => 1];
removeDeclaredHeaders($declared, $key);
removeComputedHeader($items);
$mixed = ["keep" => 1];
removeMixedHeader($mixed);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source,
            Path::new("main.php"), Path::new("."), Target::parse(name).unwrap());
        let mut gated = 0;
        let mut removals = 0;
        let mut raw = 0;
        let mut boxed = 0;
        for function in &module.functions {
            for inst in &function.instructions {
                if matches!(inst.op, Op::HashUnset | Op::OffsetUnset) {
                    removals += 1;
                    raw += usize::from(inst.op == Op::HashUnset);
                    boxed += usize::from(inst.op == Op::OffsetUnset);
                    assert!(inst.result.is_some(), "{name}: removal needs a presence result");
                    assert_eq!(inst.result_php_type, PhpType::Bool, "{name}");
                }
            }
            for block in &function.blocks {
                if !block.name.starts_with("unset.collect") { continue; }
                gated += 1;
                assert!(block.instructions.iter().any(|id|
                    function.instruction(*id).unwrap().op == Op::GcCollect), "{name}");
                assert!(function.blocks.iter().any(|parent| matches!(parent.terminator,
                    Some(Terminator::CondBr { then_target, .. }) if then_target == block.id)),
                    "{name}: collection must have a conditional predecessor");
            }
            if function.name == "removeComputedHeader" {
                assert!(!function.blocks.iter().any(|block| block.name.starts_with("unset.collect")),
                    "{name}: an effectful key retains unconditional collection");
            }
        }
        assert_eq!(removals, 6, "{name}");
        assert!(raw > 0 && boxed > 0, "{name}: cover both array representations");
        assert_eq!(gated, 3, "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
