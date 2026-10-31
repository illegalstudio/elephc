//! Purpose:
//! Keeps typed element-store effects precise without weakening dynamic reference boundaries.
//!
//! Called from:
//! - Whole-module EIR refinement tests in CI.
//!
//! Key details:
//! - Instructions and direct-call summaries must agree on the same typed-store proof.

use crate::ir::{Effects, Immediate, Op};

/// Typed scalar writes stay callback-free while a Mixed-key/value store keeps throwing effects.
#[test]
fn typed_container_store_effects_refine_instructions_and_callable_summaries() {
    let source = r#"<?php
function preciseIndexedWriter(array<int> $items, int $key, int $value): array<int> {
    $items[$key] = $value; return $items;
}
function preciseHashWriter(array<string, int> $items, string $key, int $value): array<string, int> {
    $items[$key] = $value; return $items;
}
function callbackCapableWriter(array $items, mixed $key, mixed $value): array {
    $items[$key] = $value; return $items;
}
$indexed = preciseIndexedWriter([1], 0, 2);
$hashed = preciseHashWriter(['x' => 1], 'x', 2);
$dynamic = callbackCapableWriter([1, 'x'], $argc > 0 ? 0 : 'x', 2);
echo count($indexed), count($hashed), count($dynamic);
"#;
    let module = super::lower_source(source);
    let callback_effects = Effects::ALLOC_CONCAT | Effects::OUTPUT | Effects::MAY_WARN
        | Effects::MAY_THROW | Effects::WRITES_GLOBAL | Effects::BLOCKING_IO | Effects::NETWORK_IO;
    let storage_effects = Effects::READS_HEAP | Effects::WRITES_HEAP | Effects::ALLOC_HEAP
        | Effects::REFCOUNT_OP | Effects::MAY_FATAL;
    for name in ["preciseIndexedWriter", "preciseHashWriter"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        let store = function.instructions.iter().find(|inst| matches!(inst.op, Op::ArraySet | Op::HashSet)).unwrap();
        assert!(store.effects.contains(storage_effects), "{name}: preserve actual container ownership effects");
        assert!(!store.effects.intersects(callback_effects), "{name}: {:?}", store.effects);
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        let call = main.instructions.iter().find(|inst| inst.op == Op::Call && match inst.immediate {
            Some(Immediate::Data(id)) => module.data.function_names.get(id.as_raw() as usize)
                .is_some_and(|candidate| candidate == name),
            _ => false,
        }).unwrap();
        assert!(!call.effects.intersects(callback_effects), "{name}: {:?}", call.effects);
    }
    let function = module.functions.iter().find(|function| function.name == "callbackCapableWriter").unwrap();
    let store = function.instructions.iter().find(|inst|
        matches!(inst.op, Op::ArraySet | Op::HashSet | Op::ArraySetMixedKey)).unwrap();
    assert!(store.effects.contains(Effects::MAY_THROW | Effects::ALLOC_CONCAT | Effects::MAY_WARN),
        "unproven stores must retain callback and nullable-reference refusal effects");
}
