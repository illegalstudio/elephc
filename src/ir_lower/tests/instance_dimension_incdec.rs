//! Purpose:
//! Emits native indexed incdec value expressions on every supported target.
//!
//! Called from:
//! - The supported-target EIR test harness.
//!
//! Key details:
//! - Local, property, computed receiver and nested places share write-context selection.
//! - The runtime suite separately checks values, callback order and ownership.

/// Native-capable value updates reach the shared selector and every platform emitter.
fn verify_instance_dimension_incdec_values(target: &str) {
    let source = r#"<?php
function stepRoot(SimpleXMLElement $doc): SimpleXMLElement { return $doc; }
function stepName(): string { return 'a'; }
function stepLocal(SimpleXMLElement $view): mixed { return $view[0]++; }
function stepProperty(SimpleXMLElement $doc): mixed { return ++$doc->a[0]; }
function stepComputed(SimpleXMLElement $doc): mixed { return stepRoot($doc)->{stepName()}[0]--; }
function stepNested(SimpleXMLElement $doc): mixed { return --$doc->a[0][0]; }
$doc = new SimpleXMLElement('<r><a>7</a></r>');
echo stepLocal($doc->a), stepProperty($doc), stepComputed($doc), stepNested($doc);
"#;
    let mut module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    for optimized in [false, true] {
        if optimized { crate::ir_passes::optimize_module(&mut module); }
        crate::ir::validate_module(&module).unwrap();
        for name in ["stepLocal", "stepProperty", "stepComputed", "stepNested"] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            assert!(function.instructions.iter().any(|instruction| {
                instruction.op == crate::ir::Op::MethodCall && match instruction.immediate {
                    Some(crate::ir::Immediate::Data(id)) =>
                        module.data.strings[id.as_raw() as usize] == "__elephc_dimension_select",
                    _ => false,
                }
            }), "{target} {name} optimized={optimized}: missing native writable selection\n{}",
                crate::ir::print_function(function));
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target} optimized={optimized}: {error:?}"));
    }
}

supported_target_cases!(instance_dimension_incdec_values, verify_instance_dimension_incdec_values);
