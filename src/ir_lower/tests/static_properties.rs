//! Purpose:
//! Pins executable shutdown ownership for emitted class static properties.
//!
//! Called from:
//! - The AST-to-EIR unit-test module through Rust's test harness.
//!
//! Key details:
//! - All five targets must skip uninitialized slots and detach inherited owners once.
//! - Destructor boundaries must finish remaining slots before propagating exceptions.

/// Every target roots a nullable static receiver before RHS evaluation and retires it after the store.
#[test]
fn nullable_static_property_write_receiver_is_retained_on_all_targets() {
    let source = r#"<?php
class ReviewPinnedObject { public int $v = 1; }
class ReviewPinnedHolder { public static ?ReviewPinnedObject $o = null; }
function replaceReviewPinnedObject(): int {
    ReviewPinnedHolder::$o = new ReviewPinnedObject();
    return 9;
}
function writeReviewPinnedObject(): void {
    ReviewPinnedHolder::$o->v = replaceReviewPinnedObject();
}
ReviewPinnedHolder::$o = new ReviewPinnedObject();
writeReviewPinnedObject();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source,
            std::path::Path::new("main.php"), std::path::Path::new("."), target);
        let function = module.functions.iter().find(|function| function.name == "writeReviewPinnedObject").unwrap();
        let store = function.instructions.iter().position(|inst| inst.op == crate::ir::Op::PropSet).unwrap();
        let object = function.value(function.instructions[store].operands[0]).unwrap();
        let crate::ir::ValueDef::Instruction { inst, .. } = object.def else {
            panic!("{name}: the property receiver must be an acquired value");
        };
        assert_eq!(function.instruction(inst).unwrap().op, crate::ir::Op::Acquire, "{name}");
        assert!(function.instructions[..store].iter().any(|inst| inst.op == crate::ir::Op::PushCallOperandOwner), "{name}");
        assert!(function.instructions[store + 1..].iter().any(|inst| inst.op == crate::ir::Op::PopCallOperandOwner), "{name}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Every target retires inherited class-static owners before bounded shutdown release.
#[test]
fn class_static_shutdown_ownership_is_emitted_on_all_targets() {
    let source = r#"<?php
class ShutdownEmitterOwner {
    public static string $text = "";
    public static mixed $payload = null;
    public static string $uninitialized;
}
class ShutdownEmitterChild extends ShutdownEmitterOwner {}
ShutdownEmitterChild::$text = str_repeat("x", $argc);
ShutdownEmitterChild::$payload = ["key" => $argc];
echo isset(ShutdownEmitterChild::$uninitialized);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        for property in ["text", "payload", "uninitialized"] {
            let symbol = crate::names::static_property_symbol("ShutdownEmitterOwner", property);
            let comment = format!("epilogue cleanup static property {symbol}");
            assert_eq!(asm.matches(&comment).count(), 1, "{name}: {property}");
            let cleanup = asm.split_once(&comment).unwrap().1;
            let retired = cleanup.find("retire the static property").unwrap();
            let released = cleanup.find("__rt_cleanup_preserve_exception").unwrap();
            assert!(retired < released, "{name}: {property} must retire before destructor entry");
            let inherited = crate::names::static_property_symbol("ShutdownEmitterChild", property);
            assert!(!asm.contains(&format!("epilogue cleanup static property {inherited}")), "{name}");
        }
        let tail = asm.rsplit_once("retire the static property").unwrap().1;
        assert!(tail.find("__rt_cleanup_preserve_exception").unwrap() < tail.find("__rt_throw_current").unwrap(), "{name}");
    }
}
