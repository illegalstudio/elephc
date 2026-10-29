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
    public static function &staticItems(): mixed { return self::$staticData; }
}
class ReferenceOverrideStable { public function plain(): mixed { return [3]; } }
function readReferenceOverride(ReferenceOverrideParent $value): mixed { return $value->items(); }
function readReferenceStable(ReferenceOverrideStable $value): mixed { return $value->plain(); }
echo json_encode(readReferenceOverride(new ReferenceOverrideChild()));
echo json_encode(ReferenceOverrideChild::forwarded());
echo json_encode(readReferenceStable(new ReferenceOverrideStable()));
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
}

/// Schedules one target separately to preserve the ordinary unit-test timeout.
macro_rules! reference_override_target_test {
    ($name:ident, $target:literal) => {
        /// Checks reference override result adaptation on this supported target.
        #[test]
        fn $name() { check_reference_override_target($target); }
    };
}

reference_override_target_test!(reference_override_results_macos, "macos-aarch64");
reference_override_target_test!(reference_override_results_ios, "ios-arm64");
reference_override_target_test!(reference_override_results_ios_sim, "ios-sim-arm64");
reference_override_target_test!(reference_override_results_linux_arm, "linux-aarch64");
reference_override_target_test!(reference_override_results_linux_x86, "linux-x86_64");
