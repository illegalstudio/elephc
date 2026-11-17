//! Purpose:
//! Verifies key cleanup for native probes and reads after final local storage widening.
//!
//! Called from:
//! - The AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - A later foreach boxes the consumer's earlier string key slot on every target.

use crate::ir::{Immediate, Op};
use crate::types::PhpType;

/// Hash probes and reads retire detached keys while global-key probes preserve their borrow.
fn verify_native_isset_key_owner(target: &str) {
    let source = format!("{}\n{}\n{}", include_str!("../../../tests/fixtures/foreach_key_lifetime.php"),
        include_str!("../../../tests/fixtures/indexed_key_lifetime.php").trim_start_matches("<?php"),
        r#"function nativeIndexedKey(array<int> $items, string $key): bool {
            return isset($items[str_repeat($key, 1)]);
        }
        function nativeGlobalKey(): bool {
            global $globalProbeKey;
            array<string, int> $items = ['a' => 1];
            return isset($items[$globalProbeKey]);
        }
        $globalProbeKey = 'a';
        echo (int)nativeIndexedKey([7], '0'), (int)nativeGlobalKey();"#);
    let module = super::lower_source_at_for_target(
        &source,
        std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let function = module.class_methods.iter().find(|function| function.name == "History::step")
        .expect("consumer history step");
    for op in [Op::HashIsset, Op::HashGet] {
        let probe = function.instructions.iter().find(|inst| inst.op == op)
            .expect("native hash probe or existing-entry read");
        let key = probe.operands[1];
        let load = function.instructions.iter().find(|inst| inst.result == Some(key))
            .expect("key producer");
        assert_eq!(load.op, Op::LoadLocal, "{target}: generated key read");
        let Some(Immediate::LocalSlot(slot)) = load.immediate else { panic!("key local slot"); };
        assert_eq!(function.locals[slot.as_raw() as usize].php_type.codegen_repr(), PhpType::Mixed,
            "{target}: later foreach widens the key storage");
        assert_eq!(function.value(key).unwrap().php_type, PhpType::Str);
        assert_eq!(function.instructions.iter().filter(|inst|
            inst.op == Op::Release && inst.operands == [key]).count(), 1,
            "{target}: {op:?} must retire its independent string key");
    }
    let indexed = module.functions.iter().find(|function| function.name == "nativeIndexedKey")
        .expect("indexed array probe");
    let probe = indexed.instructions.iter().find(|inst| inst.op == Op::ArrayGetMixedKeySilent)
        .expect("string-key indexed probe");
    let key = probe.operands[1];
    assert_eq!(indexed.instructions.iter().filter(|inst|
        inst.op == Op::Release && inst.operands == [key]).count(), 1,
        "{target}: indexed isset must retire its fresh string key");
    for (name, op) in [("readIndexedLocal", Op::ArrayGetMixedKey),
        ("readIndexedSilent", Op::ArrayGetMixedKeySilent)]
    {
        let read = module.functions.iter().find(|function| function.name == name)
            .expect("ordinary or silent indexed read");
        let key = read.instructions.iter().find(|inst| inst.op == op)
            .expect("indexed string-key read").operands[1];
        assert_eq!(read.value(key).unwrap().php_type, PhpType::Str);
        assert_eq!(read.instructions.iter().filter(|inst|
            inst.op == Op::Release && inst.operands == [key]).count(), 1,
            "{target}: {op:?} must retire its detached local key");
    }
    let global = module.functions.iter().find(|function| function.name == "nativeGlobalKey")
        .expect("global-key hash probe");
    let key = global.instructions.iter().find(|inst| inst.op == Op::HashIsset)
        .expect("global-key probe").operands[1];
    assert!(global.instructions.iter().any(|inst| inst.op == Op::LoadGlobal && inst.result == Some(key)),
        "{target}: the key must borrow its live global box");
    assert!(!global.instructions.iter().any(|inst| inst.op == Op::Release && inst.operands == [key]),
        "{target}: a native probe must preserve its borrowed global key");
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
}

supported_target_cases!(native_isset_key_owners, verify_native_isset_key_owner);
