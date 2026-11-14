//! Purpose:
//! Bounds repeated boxed object-operator graphs while preserving all supported ABIs.
//!
//! Called from:
//! - The focused AST-to-EIR lowering unit tests.
//!
//! Key details:
//! - XML and BcMath enable distinct dispatch protocols without opaque eval's catalog.
//! - The original helper body retains handler dispatch and scalar fallbacks.

use super::*;
use crate::ir::{Immediate, Op};

/// Multiple boxed sites share one graph per operator, including after EIR optimization.
fn verify_boxed_operator_graphs_are_shared(target: &str) {
    let source = "<?php \
        function sharedBoxedAdd(mixed $left, mixed $right): mixed { return $left + $right; } \
        function anotherBoxedAdd(mixed $left, mixed $right): mixed { return $left + $right; } \
        function sharedBoxedInt(mixed $left, int $right): mixed { return $left + $right; } \
        function sharedIntBoxed(int $left, mixed $right): mixed { return $left + $right; } \
        function sharedBoxedIncrement(mixed $value): mixed { return ++$value; } \
        function sharedBoxedLess(mixed $left, mixed $right): bool { return $left < $right; } \
        $xml = new SimpleXMLElement('<r>6</r>'); \
        $number = new BcMath\\Number('2'); echo sharedBoxedAdd($number, 1); \
        echo sharedBoxedAdd($xml, 2), anotherBoxedAdd(2, $xml), sharedBoxedLess($xml, 8), \
            sharedBoxedInt($xml, 2), sharedIntBoxed(2, $xml), sharedBoxedIncrement('az');";
    let mut module = try_lower_source_at_for_target_with_reachability(
        source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(), true,
    ).unwrap();
    for optimized in [false, true] {
        if optimized { crate::ir_passes::optimize_module(&mut module); }
        for (name, operation) in [("sharedBoxedAdd", "add"), ("anotherBoxedAdd", "add"),
            ("sharedBoxedLess", "lt"), ("sharedBoxedInt", "add"), ("sharedIntBoxed", "add"),
            ("sharedBoxedIncrement", "inc")] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let helper = format!("__eir_boxed_operator@{operation}");
            let calls = function.instructions.iter().filter(|instruction| {
                instruction.op == Op::Call && match instruction.immediate {
                    Some(Immediate::Data(id)) => module.data.function_names[id.as_raw() as usize] == helper,
                    _ => false,
                }
            }).count();
            assert_eq!(calls, 1, "{target}: {name}, optimized={optimized}");
            assert!(!function.instructions.iter().any(|instruction| instruction.op == Op::InstanceOf),
                "{target}: {name} must not duplicate the object dispatch graph");
        }
        for operation in ["add", "lt", "inc"] {
            let helper = format!("__eir_boxed_operator@{operation}");
            let bodies = module.functions.iter().filter(|function| function.name == helper).collect::<Vec<_>>();
            assert_eq!(bodies.len(), 1, "{target}: exactly one {helper}");
            assert!(bodies[0].flags.is_synthetic && bodies[0].flags.is_internal);
            assert!(bodies[0].instructions.iter().any(|instruction| instruction.op == Op::InstanceOf),
                "{target}: the helper retains internal class precedence");
            if operation != "lt" {
                let positions = |class: &str| bodies[0].instructions.iter().enumerate()
                    .filter_map(|(position, instruction)| {
                        (instruction.op == Op::StaticMethodCall && matches!(instruction.immediate,
                            Some(Immediate::Data(id)) if module.data.strings.get(id.as_raw() as usize)
                                == Some(&format!("{class}::__elephc_add"))))
                            .then_some(position)
                    }).collect::<Vec<_>>();
                let native = positions("BcMath\\Number");
                let conversion = positions("SimpleXMLElement");
                assert_eq!(native.len(), 2, "{target}: {operation} retains both native handlers");
                assert_eq!(conversion.len(), 2, "{target}: {operation} retains both XML fallbacks");
                assert!(native.iter().max() < conversion.iter().min(),
                    "{target}: {operation} tries native handlers before numeric conversion");
            }
        }
        crate::ir::validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains(&crate::names::function_symbol("__eir_boxed_operator@add")));
    }
}

supported_target_cases!(boxed_operator_graphs_are_shared, verify_boxed_operator_graphs_are_shared);
