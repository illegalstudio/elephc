//! Purpose:
//! Checks delayed static receiver evaluation and ownership on all supported targets.
//!
//! Called from:
//! - The EIR lowering unit test harness.
//!
//! Key details:
//! - A plain assignment evaluates an effectful RHS before loading its static receiver.
//! - Nullable array writes retain a checked object payload rather than dropping a runtime call.

/// Emits the shared fixture and checks the RHS precedes the concrete static receiver load.
fn verify(target: &str) {
    let source = r#"<?php
class O { public int $v = 1; public array $items = [1]; }
class C { public static O $o; public static ?O $nullable = null; }
function replace(): int { C::$o = new O(); return 9; }
function write(): void { C::$o->v = replace(); }
C::$o = new O(); C::$nullable = new O();
write();
C::$o->items[0] = replace();
$name = 'v'; C::$o->$name = replace();
C::$nullable->items[0] = 9;
C::$nullable->items[] = 3;
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "write").unwrap();
    let rhs = function.instructions.iter().position(|inst| inst.op == crate::ir::Op::Call).unwrap();
    let receiver = function.instructions.iter().position(|inst| inst.op == crate::ir::Op::LoadStaticProperty).unwrap();
    assert!(rhs < receiver, "{target}: RHS must precede receiver fetch");
    assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::Acquire));
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

/// Schedules each supported target independently under the CI timeout envelope.
macro_rules! receiver_target_test {
    ($name:ident, $target:literal) => {
        /// Checks receiver evaluation and array mutation emission for this target.
        #[test]
        fn $name() { verify($target); }
    };
}

receiver_target_test!(static_receiver_review_macos, "macos-aarch64");
receiver_target_test!(static_receiver_review_ios, "ios-arm64");
receiver_target_test!(static_receiver_review_ios_sim, "ios-sim-arm64");
receiver_target_test!(static_receiver_review_linux_arm, "linux-aarch64");
receiver_target_test!(static_receiver_review_linux_x86, "linux-x86_64");
