//! Purpose:
//! Checks that typed reference-return staging does not narrow raw switch sibling slots.
//!
//! Called from:
//! - The AST-to-EIR regression suite on every supported target.
//!
//! Key details:
//! - Each return creates its own checked object cell from nullable local storage.
//! - Handler recovery is a separate boundary because a throwing finally may expose the cell.

/// Independent switch paths each promote the nullable value instead of trusting a sibling's alias.
#[test]
fn nullable_object_reference_return_switch_siblings_promote_on_every_target() {
    let source = r#"<?php
class SiblingReferenceNode {}
class SiblingReferenceHolder {
    public ?SiblingReferenceNode $node = null;
    public ?SiblingReferenceNode $fallback = null;
}
function &siblingReferenceReturn(SiblingReferenceHolder $holder, int $choice): SiblingReferenceNode {
    $node = $holder->node;
    switch ($choice) {
        case 1: return $node;
        default: $node = null; var_dump($node); $node = $holder->fallback; return $node;
    }
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "siblingReferenceReturn").unwrap();
        let aliases = function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::AliasLocalRefCell)
            .filter(|inst| matches!(inst.immediate, Some(crate::ir::Immediate::LocalSlotPair { first, .. })
                if function.locals[first.as_raw() as usize].name.as_deref() == Some("node"))).count();
        assert_eq!(aliases, 2, "{target}: each raw sibling needs its own checked typed cell");
        assert_eq!(function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::AcquireRefCell).count(), 2,
            "{target}: both return paths transfer their own cell lease");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
