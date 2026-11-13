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


/// Shared native dispatch keeps trusted PHP arithmetic separate from user operation guarantees.
#[test]
fn strict_boxed_operator_helpers_preserve_caller_policy_on_all_targets() {
    use crate::source::SourceMode;
    use elephc_semantics::{SemanticProfile, StrictFeature};
    let source = "<?php function userAdd(mixed $a, mixed $b): mixed { return $a + $b; } function userPow(mixed $a, mixed $b): mixed { return $a ** $b; } function userInc(mixed $a): mixed { return ++$a; }";
    let internal = source.replace("user", "trusted");
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    program.extend(crate::parser::parse_with_mode(&crate::lexer::tokenize(&internal).unwrap(), SourceMode::Internal).unwrap());
    program.extend(crate::parser::parse(&crate::lexer::tokenize("<?php $number = new BcMath\\Number('2');").unwrap()).unwrap());
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::core_prelude::inject_if_used(program, &mut inventory);
    let program = crate::bcmath_prelude::inject_if_used(program, false, &mut inventory);
    let program = crate::name_resolver::resolve(program).unwrap();
    for feature in [StrictFeature::Overflow, StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
                for (operation, function_suffix) in [("add", "Add"), ("pow", "Pow"), ("inc", "Inc")] {
                    for (prefix, trusted) in [("user", false), ("trusted", true)] {
                        let helper_name = format!("__eir_boxed_operator@{operation}{}", if trusted { "@internal" } else { "" });
                        let caller = module.functions.iter().find(|function| function.name == format!("{prefix}{function_suffix}")).unwrap();
                        assert!(caller.instructions.iter().any(|inst| inst.op == Op::Call && matches!(inst.immediate,
                            Some(Immediate::Data(id)) if module.data.function_names[id.as_raw() as usize] == helper_name)),
                            "{name}, {feature:?}, opt={optimize}: {prefix}{function_suffix}");
                        let helper = module.functions.iter().find(|function| function.name == helper_name).unwrap();
                        assert!(!helper.flags.is_builtin_contract_wrapper);
                        assert_eq!(helper.instructions.iter().any(|inst| inst.arithmetic_policy().checked_overflow()),
                            !trusted && feature == StrictFeature::Overflow, "{name}: {helper_name}, {feature:?}");
                        assert_eq!(helper.instructions.iter().any(|inst| inst.op == Op::ThrowTypeError),
                            !trusted && feature != StrictFeature::Overflow, "{name}: {helper_name}, {feature:?}");
                    }
                }
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}
