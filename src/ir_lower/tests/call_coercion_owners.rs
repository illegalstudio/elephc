//! Purpose:
//! Pins unwind registration for implicit call-argument coercions on every target.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Backend-created boxes are not EIR local owners and need their own cleanup records.

/// Two implicit array boxes receive paired records around the native call on every supported ABI.
#[test]
fn implicit_array_argument_boxes_have_unwind_records_on_all_targets() {
    let source = r#"<?php
function coercionTarget(array $left, array $right): int { return count($left) + count($right); }
function coercionCaller(int $seed): int {
    $left = [$seed];
    $right = ["key" => $seed];
    return coercionTarget($left, $right);
}
echo coercionCaller($argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let body = asm.split_once("@fn name=coercionCaller ").unwrap().1
            .split_once("@endfn name=coercionCaller").unwrap().0;
        let invoke = body.lines().find(|line| {
            let line = line.trim_start();
            (line.starts_with("bl ") || line.starts_with("call ")) && line.contains("coercionTarget")
        }).unwrap_or_else(|| panic!("{target}: missing native call in {body}"));
        let (before, after) = body.split_once(invoke).unwrap();
        assert_eq!(before.matches("publish temporary call operand owner").count(), 2, "{target}: {body}");
        assert_eq!(after.matches("detach temporary call operand owner").count(), 2, "{target}: {body}");
        let (inner, outer) = if target == "linux-x86_64" {
            ("mov r10, QWORD PTR [rsp + 80]", "mov r10, QWORD PTR [rsp + 16]")
        } else {
            ("ldr x10, [sp, #80]", "ldr x10, [sp, #16]")
        };
        assert!(after.find(inner).unwrap() < after.find(outer).unwrap(), "{target}: {after}");
    }
}
