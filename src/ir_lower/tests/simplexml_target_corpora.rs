//! Purpose:
//! Emits independent SimpleXML boundary, arithmetic and node-view corpora on every target.
//!
//! Called from:
//! - Supported-target AST-to-EIR CI tests, without target linking.
//!
//! Key details:
//! - Preserves every fixture formerly appended to the core XML/DOM target program.
//! - Smaller class catalogs bound each emitter invocation without increasing CI timeouts.

/// Lowers one complete corpus with the same frontend and target pipeline as the core probes.
fn lower_corpus(source: &str, target: &str) -> crate::ir::Module {
    super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap())
}

/// Object argument, return and property boundaries retain their guarded native ABI.
fn verify_simplexml_target_boundaries(target: &str) {
    let module = lower_corpus(include_str!("../../../tests/fixtures/simplexml/typed_object_boundaries.php"), target);
    let reference_parameter = module.functions.iter().find(|function| function.name == "acceptRefBoundaryXml")
        .expect("typed object references must keep their callee-entry validator");
    assert!(reference_parameter.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::InstanceOf),
        "{target}: by-reference object parameters need a nominal class guard");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    for message in [
        "acceptBoundaryXml(): Argument #1 ($node) must be of type SimpleXMLElement, null given",
        "returnBoundaryXml(): Return value must be of type SimpleXMLElement, null returned",
        "Cannot assign null to property BoundaryXmlBox::$node of type SimpleXMLElement",
    ] {
        assert!(assembly.contains(message), "{target}: missing native object boundary: {message}");
    }
}

supported_target_cases!(simplexml_target_boundaries, verify_simplexml_target_boundaries);

/// Generator entry validation remains eager before the coroutine owns its arguments.
fn verify_simplexml_target_generators(target: &str) {
    let boundaries = include_str!("../../../tests/fixtures/simplexml/typed_object_boundaries.php");
    let (declarations, _) = boundaries.split_once("function acceptBoundaryXml(").unwrap();
    let source = format!("{declarations}\n{}",
        include_str!("../../../tests/fixtures/simplexml/generator_object_boundaries.php").trim_start_matches("<?php"));
    let module = lower_corpus(&source, target);
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let guard_name = crate::names::generator_parameter_guard_name("generatorBoundaryXml");
    let guard = module.closures.iter().find(|function| function.name == guard_name)
        .expect("generator parameters must have an ordinary eager EIR validator");
    assert!(!guard.flags.is_generator && guard.flags.is_synthetic);
    assert!(guard.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::IsNull));
    let constructor = &assembly[assembly.find(&format!("{}:", crate::names::function_symbol("generatorBoundaryXml"))).unwrap()..];
    let validation = constructor.find(&crate::names::function_symbol(&guard_name)).unwrap();
    let allocation = constructor.find("__rt_fiber_construct").unwrap();
    assert!(validation < allocation, "{target}: null validation must precede coroutine allocation");
}

supported_target_cases!(simplexml_target_generators, verify_simplexml_target_generators);

/// Every arithmetic lineage fixture still reaches each platform emitter.
fn verify_simplexml_target_arithmetic(target: &str) {
    let module = lower_corpus(include_str!("../../../tests/fixtures/simplexml/iterator_arithmetic.php"), target);
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

supported_target_cases!(simplexml_target_arithmetic, verify_simplexml_target_arithmetic);

/// Attribute, comment and processing-instruction write paths still emit on every target.
fn verify_simplexml_target_node_views(target: &str) {
    let module = lower_corpus(include_str!("../../../tests/fixtures/simplexml/node_views.php"), target);
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

supported_target_cases!(simplexml_target_node_views, verify_simplexml_target_node_views);
