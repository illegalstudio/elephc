//! Purpose:
//! Pins the distinction between scalar and heap result staging for native enum interface calls.
//!
//! Called from:
//! - The AST-to-EIR unit test harness, with one independently scheduled case per target.
//!
//! Key details:
//! - A rejected call leaves its result slot untouched, so scalar storage cannot be a heap root.
//! - Heap-bearing results keep their prepublished cleanup record and every target still emits.

use crate::ir::{Immediate, LocalKind, Op, Ownership};
use crate::types::PhpType;

/// Checks both result-storage contracts before emitting the selected target's assembly.
fn check_enum_result_owner_target(target: &str) {
    let source = r#"<?php
interface I { public function f(int $a, int $b): int; }
enum G implements I { case A; public function f(int $a = 1, int $b): int { return $a + $b; } }
interface J { public function f(int $a, int $b): string; }
enum H implements J { case A; public function f(int $a = 1, int $b): string { return 'ok'; } }
function good(): int { return 3; }
function missing_int(I $value): int { return $value->f(...[good()]); }
function missing_string(J $value): string { return $value->f(...[good()]); }
try { missing_int(G::A); } catch (ArgumentCountError $e) { echo $e->getMessage(); }
try { missing_string(H::A); } catch (ArgumentCountError $e) { echo $e->getMessage(); }
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    for name in ["missing_int", "missing_string"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        let roots: Vec<_> = function.instructions.iter().filter_map(|instruction| {
            if instruction.op != Op::PushCallOperandOwner { return None; }
            let Some(Immediate::LocalSlot(slot)) = instruction.immediate else { panic!("owner slot"); };
            Some(&function.locals[slot.as_raw() as usize])
        }).collect();
        assert!(roots.iter().all(|local| Ownership::php_type_needs_lifetime_tracking(&local.php_type)),
            "{target}: {name} must never publish scalar storage as a heap pointer: {roots:?}");
        if name == "missing_string" {
            assert!(roots.iter().any(|local| local.php_type == PhpType::Str && local.kind == LocalKind::OwnedTemp),
                "{target}: the string result must retain its prepublished root");
        }
    }
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    for name in ["missing_int", "missing_string"] {
        let symbol = crate::names::function_symbol(name);
        assert!(assembly.contains(&symbol), "{target}: {symbol}");
    }
}

/// Preserves the existing per-test timeout by scheduling each target separately.
macro_rules! enum_result_owner_target_test {
    ($name:ident, $target:literal) => {
        /// Checks scalar and heap-bearing enum result owners on this supported target.
        #[test]
        fn $name() { check_enum_result_owner_target($target); }
    };
}

enum_result_owner_target_test!(enum_result_owners_macos, "macos-aarch64");
enum_result_owner_target_test!(enum_result_owners_ios, "ios-arm64");
enum_result_owner_target_test!(enum_result_owners_ios_sim, "ios-sim-arm64");
enum_result_owner_target_test!(enum_result_owners_linux_arm, "linux-aarch64");
enum_result_owner_target_test!(enum_result_owners_linux_x86, "linux-x86_64");
