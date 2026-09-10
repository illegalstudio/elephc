//! Purpose:
//! Regression coverage for compiler-only DOM native debug projections.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The hidden projection must lower through locked virtual-property operations.
//! - Its callable ABI must remain valid on every supported target.

use crate::codegen::platform::{Arch, Platform, Target};
use crate::codegen::generate_user_asm_from_ir;
use crate::ir::print_module;

use super::lower_source;

/// Verifies the hidden namespace-node projection lowers and emits on every target.
#[test]
fn lowers_dom_namespace_debug_projection_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$xpath = new DOMXPath($document);
$nodes = $xpath->query('//namespace::*');
var_dump($nodes->item(0));
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("function DOMNameSpaceNode::__debugInfo"),
        "missing compiler-only DOM namespace projection: {text}"
    );
    for opcode in [4610, 4612, 4611, 4616, 4608, 4609, 4607] {
        assert!(
            text.contains(&format!("internal_extension#{opcode}")),
            "DOM namespace projection omitted opcode {opcode}: {text}"
        );
    }
    assert!(
        text.contains("function DOMElement::__debugInfo"),
        "missing compiler-only DOMElement projection: {text}"
    );
    for opcode in [4598, 4592, 4590, 4626, 4631, 4619, 4617, 4634] {
        assert!(
            text.contains(&format!("internal_extension#{opcode}")),
            "DOMElement projection omitted opcode {opcode}: {text}"
        );
    }
    for (class_name, opcodes) in [
        ("DOMNodeList", &[4635][..]),
        ("DOMNamedNodeMap", &[4606][..]),
        ("Dom\\NodeList", &[4537][..]),
        ("Dom\\NamedNodeMap", &[4519][..]),
        ("Dom\\DtdNamedNodeMap", &[4497][..]),
        ("Dom\\HTMLCollection", &[4518][..]),
        ("Dom\\TokenList", &[4542, 4543][..]),
    ] {
        assert!(
            text.contains(&format!("function {class_name}::__debugInfo")),
            "missing compiler-only {class_name} collection projection: {text}"
        );
        for opcode in opcodes {
            assert!(
                text.contains(&format!("internal_extension#{opcode}")),
                "{class_name} projection omitted opcode {opcode}: {text}"
            );
        }
    }

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target:?} DOM debug projection failed: {error}"));
        assert!(
            assembly.contains("_class_debug_info_adapter_"),
            "{target:?} omitted the DOM debug adapter"
        );
    }
}
