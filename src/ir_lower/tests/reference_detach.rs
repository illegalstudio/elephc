//! Purpose:
//! Pins checker-authorized detachment of captured reference bindings.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Retiring a binding must preserve the old closure's concrete payload ABI.
//! - New captures use separate owner slots, and all supported targets emit the same semantics.

use crate::codegen::platform::Target;
use crate::ir::{Immediate, LocalKind, Op};
use crate::types::PhpType;
use std::path::Path;

/// The original loop's final unset cannot retroactively box the captured string storage.
#[test]
fn captured_string_detach_preserves_payload_type_on_all_targets() {
    let source = r#"<?php
$saved = static fn(): string => "";
for ($i = 0; $i < 4; $i++) {
    $text = str_repeat("x", $i + 1);
    $read = function() use (&$text): string { return $text; };
    if ($i === 0) { $saved = $read; }
    echo $read(), ":", $saved(), "|";
    unset($read);
}
unset($text, $saved);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        let (slot, owner) = main.instructions.iter().find_map(|inst| match inst.immediate {
            Some(Immediate::LocalSlotPair { first, second })
                if inst.op == Op::PromoteLocalRefCell
                    && main.locals[first.as_raw() as usize].name.as_deref() == Some("text") =>
            { Some((first, second)) }
            _ => None,
        }).expect("the loop must promote its captured text");
        assert_eq!(main.locals[slot.as_raw() as usize].php_type, PhpType::Str, "{name}");
        assert_eq!(main.locals[owner.as_raw() as usize].kind, LocalKind::RefCell, "{name}");
        assert_eq!(main.locals[owner.as_raw() as usize].php_type, PhpType::Str, "{name}");
        let detached = main.instructions.iter().position(|inst| {
            inst.op == Op::UnsetLocal && inst.immediate == Some(Immediate::LocalSlot(slot))
        }).expect("the final unset must clear promotion state");
        let release = &main.instructions[detached - 1];
        assert_eq!(release.op, Op::ReleaseLocalRefCell, "{name}");
        assert_eq!(release.immediate, Some(Immediate::LocalSlot(owner)), "{name}");
        let zero = &main.instructions[detached + 1];
        assert_eq!(zero.op, Op::ZeroLocalSlot, "{name}");
        assert_eq!(zero.immediate, Some(Immediate::LocalSlot(slot)), "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Probing an unset name cannot give the replacement capture boxed storage or reuse its old owner.
#[test]
fn rebound_string_captures_have_distinct_concrete_owner_slots_on_all_targets() {
    let source = r#"<?php
$text = str_repeat("x", $argc);
$old = function() use (&$text): string { return $text; };
unset($text);
echo isset($text) ? "bad" : "unset";
$text = "new" . $argc;
$fresh = function() use (&$text): string { return $text; };
echo $old(), ":", $fresh();
unset($text, $old, $fresh);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        let pairs: Vec<_> = main.instructions.iter().filter_map(|inst| match inst.immediate {
            Some(Immediate::LocalSlotPair { first, second })
                if inst.op == Op::PromoteLocalRefCell
                    && main.locals[first.as_raw() as usize].name.as_deref() == Some("text") =>
            { Some((first, second)) }
            _ => None,
        }).collect();
        assert_eq!(pairs.len(), 2, "{name}");
        assert_ne!(pairs[0].0, pairs[1].0, "{name}: replacement binding needs separate storage");
        assert_ne!(pairs[0].1, pairs[1].1, "{name}: the old descriptor must keep its old cell");
        for (slot, owner) in pairs {
            assert_eq!(main.locals[slot.as_raw() as usize].php_type, PhpType::Str, "{name}");
            assert_eq!(main.locals[owner.as_raw() as usize].php_type, PhpType::Str, "{name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
