//! Purpose:
//! Regression coverage for descendant-only DOM method dispatch from base and interface types.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - PHP resolves methods from the concrete DOM wrapper even when static metadata is broader.
//! - The EIR must retain receiver provenance while using boxed runtime class-id dispatch.

use crate::codegen::generate_user_asm_from_ir;
use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::{print_module, Immediate, Op};

use super::lower_source;

/// Verifies descendant-only DOM methods lower through runtime class dispatch on every target.
#[test]
fn lowers_dom_descendant_methods_from_base_and_interface_receivers() {
    let mut module = lower_source(
        r#"<?php
function move_legacy_node(DOMNode $node): void {
    $node->before("before");
    $node->after("after");
    $node->remove();
}

function move_nullable_legacy_node(?DOMNode $node): void {
    $node->before("nullable");
}

function configure_modern_node(Dom\Node $node): void {
    $node->setAttributeNS("urn:test", "t:key", "value");
}

function serialize_modern_parent(Dom\ParentNode $parent, Dom\Node $node): string|false {
    return $parent->saveXML($node);
}

$legacy = new DOMDocument();
$legacyNode = new DOMElement("child");
$legacy->appendChild($legacyNode);
move_legacy_node($legacyNode);
move_nullable_legacy_node($legacyNode);

$modern = Dom\XMLDocument::createEmpty();
$element = $modern->createElement("root");
$modern->appendChild($element);
configure_modern_node($element);
echo serialize_modern_parent($modern, $element);
"#,
    );
    let text = print_module(&module);
    assert!(
        text.matches("mixed_box").count() >= 5,
        "DOM base/interface calls did not retain boxed runtime dispatch: {text}"
    );
    assert!(
        text.matches("method_call").count() >= 5,
        "DOM runtime method calls were not retained: {text}"
    );

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target:?} DOM runtime dispatch failed: {error}"));
        assert!(
            assembly.contains("mixed_method_"),
            "{target:?} omitted DOM runtime class-id dispatch"
        );
    }
}

/// Verifies a disjoint union result cannot retain an owned DOM wrapper argument.
#[test]
fn releases_dom_wrapper_argument_when_native_result_union_is_disjoint() {
    let module = lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
echo $document->saveXML($document->documentElement);
"#,
    );
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let call = main
        .instructions
        .iter()
        .find(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension { opcode: 4333, .. })
                )
        })
        .expect("expected DOMDocument::saveXML internal-extension call");
    let wrapper_argument = call.operands[1];
    assert!(
        main.instructions.iter().any(|instruction| {
            instruction.op == Op::Release
                && instruction.operands.first().copied() == Some(wrapper_argument)
        }),
        "saveXML must release its owned documentElement argument after the call"
    );
}
