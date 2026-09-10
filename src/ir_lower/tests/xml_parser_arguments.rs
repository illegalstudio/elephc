//! Purpose:
//! Pins guarded XML parser dispatch for boxed call arguments on all supported targets.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Emits assembly without linking the optional XML native library.
//! - Parser validation must precede any narrowed object use inside the prelude.

/// Boxed XML parser operands use the same guarded prelude on every supported ABI.
#[test]
fn boxed_xml_parser_arguments_are_guarded_on_every_target() {
    use crate::ir::Op;
    use crate::types::PhpType;
    let source = r#"<?php
function xmlParserSource(mixed $parser): array { return [$parser]; }
function installXmlHandlers(mixed $parser): void {
    xml_set_element_handler(...xmlParserSource($parser), start_handler: null, end_handler: null);
    xml_set_character_data_handler($parser, null);
    xml_set_processing_instruction_handler($parser, null);
    xml_set_default_handler($parser, null);
    xml_set_unparsed_entity_decl_handler($parser, null);
    xml_set_notation_decl_handler($parser, null);
    xml_set_external_entity_ref_handler($parser, null);
    xml_set_start_namespace_decl_handler($parser, null);
    xml_set_end_namespace_decl_handler($parser, null);
}
installXmlHandlers(xml_parser_create());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let setters: Vec<_> = module.functions.iter().filter(|function| {
            function.name.starts_with("__elephc_xml_set_") && function.name.ends_with("_handler")
        }).collect();
        assert_eq!(setters.len(), 9, "{target}");
        for setter in setters {
            assert_eq!(setter.params[0].php_type, PhpType::Mixed, "{target}: {}", setter.name);
            assert!(setter.instructions.iter().any(|inst| inst.op == Op::InstanceOf),
                "{target}: {} must guard its boxed parser", setter.name);
            assert!(setter.blocks.iter().any(|block| matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. }))),
                "{target}: {} must reject an invalid runtime class", setter.name);
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
