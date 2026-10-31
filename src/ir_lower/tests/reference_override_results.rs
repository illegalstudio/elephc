//! Purpose:
//! Verifies value callers adapt the physical return ABI of reference overrides.
//!
//! Called from:
//! - The AST-to-EIR unit suite, with one independently scheduled case per target.
//!
//! Key details:
//! - Instance and late-static calls keep both normal and reference-returning arms.
//! - Unaffected inheritance families retain their original dispatch sequence.

/// Emits affected and unaffected callers and checks snapshot conversion in the affected bodies.
fn check_reference_override_target(target: &str) {
    let source = r#"<?php
class ReferenceOverrideParent {
    public function items(): mixed { return []; }
    public static function staticItems(): mixed { return []; }
    public static function forwarded(): mixed { return static::staticItems(); }
}
class ReferenceOverrideChild extends ReferenceOverrideParent {
    public mixed $data = [1, 2];
    public static mixed $staticData = null;
    public function &items(): mixed { return $this->data; }
    public static function &staticItems(): mixed { $value = self::$staticData; return $value; }
}
class ReferenceOverrideStable { public function plain(): mixed { return [3]; } }
class ReferenceOverrideOtherArity { public function items(int $unused): mixed { return []; } }
function readReferenceOverride(ReferenceOverrideParent $value): mixed { return $value->items(); }
function readReferenceStable(ReferenceOverrideStable $value): mixed { return $value->plain(); }
function readErasedReferenceOverride(mixed $value): mixed { return $value->items(); }
echo json_encode(readReferenceOverride(new ReferenceOverrideChild()));
echo json_encode(ReferenceOverrideChild::forwarded());
echo json_encode(readReferenceStable(new ReferenceOverrideStable()));
echo json_encode(readErasedReferenceOverride(new ReferenceOverrideChild()));
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    for symbol in [
        crate::names::function_symbol("readReferenceOverride"),
        crate::names::static_method_symbol("ReferenceOverrideParent", "forwarded"),
    ] {
        let body = assembly.split_once(&format!("{symbol}:\n")).unwrap().1;
        let body = body.split("\n.globl ").next().unwrap();
        assert!(body.contains("reference_override_value"), "{target}: selected entry needs an ABI branch");
        let clone = body.find("__rt_mixed_clone").expect("a by-value use copies the pointee");
        let release = body.find("__rt_reference_cell_release").expect("the transferred cell is retired");
        assert!(clone < release, "{target}: copy before releasing the lease");
        let tail = &body[release..];
        assert!(tail.contains(if module.target.arch == crate::codegen::platform::Arch::AArch64 {
            "mov x15, #1"
        } else { "mov r11d, 1" }), "{target}: caller receives an owned value");
    }
    let stable = crate::names::function_symbol("readReferenceStable");
    let body = assembly.split_once(&format!("{stable}:\n")).unwrap().1;
    let body = body.split("\n.globl ").next().unwrap();
    assert!(!body.contains("reference_override_value"), "{target}: unaffected families gain no branch");
    let erased = module.functions.iter().find(|function| function.name == "readErasedReferenceOverride").unwrap();
    assert!(!erased.instructions.iter().any(|inst| inst.op == crate::ir::Op::AdoptRefCellPtr),
        "{target}: the erased caller requests a value");
    let symbol = crate::names::function_symbol("readErasedReferenceOverride");
    let body = assembly.split_once(&format!("{symbol}:\n")).unwrap().1;
    let body = body.split("\n.globl ").next().unwrap();
    assert_eq!(body.matches("__rt_mixed_clone").count(), 2,
        "{target}: both the inherited override and the direct reference candidate need value copies");
}

/// Schedules one target separately to preserve the ordinary unit-test timeout.
macro_rules! reference_override_target_test {
    ($name:ident, $target:literal) => {
        /// Checks reference override result adaptation on this supported target.
        #[test]
        fn $name() {
            check_reference_override_target($target);
            check_known_reference_call_target($target);
        }
    };
}

reference_override_target_test!(reference_override_results_macos, "macos-aarch64");
reference_override_target_test!(reference_override_results_ios, "ios-arm64");
reference_override_target_test!(reference_override_results_ios_sim, "ios-sim-arm64");
reference_override_target_test!(reference_override_results_linux_arm, "linux-aarch64");
reference_override_target_test!(reference_override_results_linux_x86, "linux-x86_64");

/// Payload-typed reference call SSA values reach adoption without backend snapshot conversion.
fn check_known_reference_call_target(target: &str) {
    use crate::ir::Op;
    let source = r#"<?php
final class KnownReferenceCallOwner {
    public mixed $data = [1, 2];
    public string $tag = 'tag';
    public function &items(): mixed { return $this->data; }
    public function &text(): string { return $this->tag; }
}
function adoptKnownReferenceCalls(KnownReferenceCallOwner $owner): void {
    $alias = &$owner->items();
    $value = $owner->items();
    $textAlias = &$owner->text();
    $text = $owner->text();
    echo json_encode($value), $text;
}
adoptKnownReferenceCalls(new KnownReferenceCallOwner());
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let function = module.functions.iter().find(|function| function.name == "adoptKnownReferenceCalls").unwrap();
    let calls = function.instructions.iter().filter(|inst| inst.op == Op::MethodCall).collect::<Vec<_>>();
    assert_eq!(calls.len(), 4, "{target}: both alias and value uses must retain their reference call");
    for call in calls {
        let result = call.result.unwrap();
        assert!(matches!(function.value(result).unwrap().php_type, crate::types::PhpType::Mixed | crate::types::PhpType::Str),
            "{target}: the SSA type describes the payload, not its temporary physical cell");
        assert!(function.instructions.iter().any(|inst| inst.op == Op::AdoptRefCellPtr
            && inst.operands.first() == Some(&result)), "{target}: adoption consumes the original cell result");
    }
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let symbol = crate::names::function_symbol("adoptKnownReferenceCalls");
    let body = assembly.split_once(&format!("{symbol}:\n")).unwrap().1;
    let body = body.split("\n.globl ").next().unwrap();
    let call = if module.target.arch == crate::codegen::platform::Arch::AArch64 { "bl" } else { "call" };
    for name in ["items", "text"] {
        let callee = format!("{call} {}", crate::names::method_symbol("KnownReferenceCallOwner", name));
        let mut count = 0;
        for (position, _) in body.match_indices(&callee) {
            count += 1;
            let after = &body[position + callee.len()..];
            let adopt = after.find("__rt_reference_cell_owner").expect("the call must reach raw cell adoption");
            let raw = &after[..adopt];
            for conversion in ["__rt_mixed_clone", "__rt_mixed_from_value", "__rt_reference_cell_release"] {
                assert!(!raw.contains(conversion), "{target}: {name} must reach adoption without {conversion}");
            }
        }
        assert_eq!(count, 2, "{target}: {name} covers alias and value uses");
    }
}
