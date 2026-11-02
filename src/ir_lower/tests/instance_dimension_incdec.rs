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

/// Recognizes literal names through ownership-only wrappers without treating captured locals as constants.
fn literal_property_name(function: &crate::ir::Function, mut value: crate::ir::ValueId) -> bool {
    loop {
        let Some(producer) = function.instructions.iter().find(|inst| inst.result == Some(value)) else {
            return false;
        };
        match producer.op {
            crate::ir::Op::ConstStr => return true,
            crate::ir::Op::Borrow | crate::ir::Op::Acquire | crate::ir::Op::EnsureOwned | crate::ir::Op::StrPersist => {
                let Some(source) = producer.operands.first() else { return false; };
                value = *source;
            }
            _ => return false,
        }
    }
}

/// A captured XML class guard leaves only the live ordinary runtime-name read ladder.
fn verify_captured_property_incdec_dispatch_is_shared(target: &str) {
    let source = r#"<?php
class CapturedKernelXml extends SimpleXMLElement {}
function capturedKernelUpdate(mixed $object, string $name): void { $object->{$name}[0]++; }
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    crate::ir::validate_module(&module).unwrap();
    let function = module.functions.iter().find(|function| function.name == "capturedKernelUpdate").unwrap();
    let reads = function.instructions.iter().filter(|inst| inst.op == crate::ir::Op::DynamicPropGet
        && inst.operands.get(1).is_some_and(|name| !literal_property_name(function, *name))).count();
    assert_eq!(reads, 1, "{target}: preserve one ordinary runtime-name read without dead XML copies");
}

supported_target_cases!(captured_property_incdec_dispatch_is_shared, verify_captured_property_incdec_dispatch_is_shared);
