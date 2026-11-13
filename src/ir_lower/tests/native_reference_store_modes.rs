//! Purpose:
//! Preserves assignment-site strictness and the original boxed RHS at native reference stores.
//!
//! Called from:
//! - The supported-target AST-to-EIR suite in CI.
//!
//! Key details:
//! - A parameter declaration describes the raw ABI, not the source assignment mode.
//! - The canonical publisher must see the original RHS before legacy scalar narrowing.

use crate::ir::{Immediate, Op, ValueDef};

/// Weak and strict source files carry explicit modes without unboxing the original assignment.
fn verify_native_reference_store_modes(target: &str) {
    for strict in [false, true] {
        let source = format!("<?php {} \
            function nativeModeStore(int &$value, mixed $original): void {{ $value = $original; }}",
            if strict { "declare(strict_types=1);" } else { "" });
        let mut module = super::lower_source_at_for_target(
            &source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "nativeModeStore").unwrap();
        let store = function.instructions.iter().find(|inst| inst.op == Op::StoreRefCell).unwrap();
        assert_eq!(store.operands.len(), 2, "{target}: original RHS and source mode");
        assert_eq!(function.value(store.operands[0]).unwrap().php_type, crate::types::PhpType::Mixed,
            "{target}: preserve the original RHS before raw integer coercion");
        let ValueDef::Instruction { inst, .. } = function.value(store.operands[1]).unwrap().def else {
            panic!("{target}: source mode must be an explicit constant");
        };
        let mode = function.instruction(inst).unwrap();
        assert_eq!(mode.op, Op::ConstBool);
        assert_eq!(mode.immediate, Some(Immediate::Bool(strict)), "{target}: assignment-site mode");
        let ordinary = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
        assert!(!ordinary.contains("__rt_reference_publish_native"), "{target}: optional capability");
        crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
        let validated = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let publish = validated.find("__rt_reference_publish_native").unwrap();
        let narrow = validated[publish..].find("__rt_mixed_cast_int").unwrap();
        assert!(narrow > 0, "{target}: the raw fallback narrows only after canonical dispatch");
    }
}

supported_target_cases!(native_reference_store_modes, verify_native_reference_store_modes);

/// A typed parameter's body observes the published value without changing its entry payload ABI.
#[test]
fn native_scalar_reference_writes_refresh_body_value_views_on_every_target() {
    let source = "<?php function nativeScalarReplacement(): mixed { return 'changed'; } \
        function nativeScalarBodyView(int &$value): void { \
        $value = nativeScalarReplacement(); echo gettype($value), ':', $value; } \
        function &nativeScalarReferenceReturn(int &$value): int { $value = 3; return $value; }";
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "nativeScalarBodyView").unwrap();
        assert_eq!(function.params[0].php_type, crate::types::PhpType::Int, "{target}");
        assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::LoadRefCellValue).count(),
            2, "{target}: both consumers must copy the current value");
        let reference = module.functions.iter().find(|function|
            function.name == "nativeScalarReferenceReturn").unwrap();
        assert!(reference.instructions.iter().any(|inst| inst.op == Op::AcquireRefCell),
            "{target}: a typed reference return must retain its existing managed cell ABI");
        crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let body = assembly.split_once("@fn name=nativeScalarBodyView symbol=").unwrap().1
            .split_once("@endfn name=nativeScalarBodyView").unwrap().0;
        let publish = body.find("__rt_reference_publish_native").unwrap();
        assert!(body[publish..].contains("__rt_mixed_clone"), "{target}: later reads own their actual canonical value");
    }
}

/// A fresh boxed RHS exposes the extra source lease as a borrowed, unwind-visible parking slot.
fn verify_native_reference_store_source_owner(target: &str) {
    let source = r#"<?php
function nativeReferenceFreshValue(): mixed { return new stdClass(); }
function nativeReferenceFreshStore(mixed &$target): void { $target = nativeReferenceFreshValue(); }
"#;
    let mut module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
    let function = module.functions.iter().find(|function| function.name == "nativeReferenceFreshStore").unwrap();
    let store = function.instructions.iter().find(|inst| inst.op == Op::StoreRefCell).unwrap();
    assert_eq!(store.operands.len(), 3, "{target}: RHS, source mode and source owner");
    let owner = function.value(store.operands[2]).unwrap();
    assert_eq!(owner.ownership, crate::ir::Ownership::Borrowed, "{target}: parking owns the source");
    let ValueDef::Instruction { inst: load, .. } = owner.def else { panic!("{target}: owner load"); };
    let load = function.instruction(load).unwrap();
    assert_eq!(load.op, Op::LoadLocal);
    assert!(function.instructions.iter().any(|instruction|
        instruction.op == Op::PushCallOperandOwner && instruction.immediate == load.immediate),
        "{target}: source remains protected until transfer");
    let Some(Immediate::LocalSlot(parking)) = load.immediate else { panic!("{target}: parking slot"); };
    let mut malformed = module.clone();
    let function = malformed.functions.iter_mut().find(|function| function.name == "nativeReferenceFreshStore").unwrap();
    function.locals[parking.as_raw() as usize].kind = crate::ir::LocalKind::PhpLocal;
    assert!(crate::ir::validate_module(&malformed).is_err(),
        "{target}: a canonical store must never clear an ordinary PHP binding");
    for optimize in [false, true] {
        let mut candidate = module.clone();
        if optimize { crate::ir_passes::optimize_module(&mut candidate); }
        crate::ir::validate_module(&candidate).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&candidate, false, false).unwrap();
        assert!(assembly.contains("__rt_reference_publish_native"), "{target}: moving store publisher");
    }
}

supported_target_cases!(native_reference_store_source_owner, verify_native_reference_store_source_owner);

/// Parking retires the fresh source only after a separate store owner was acquired.
fn verify_native_reference_fresh_string_store_owner(target: &str) {
    let source = "<?php \
        function nativeFreshStringSource(): string { return str_repeat('ab', 2); } \
        function nativeFreshStringStore(string &$value): void { $value = nativeFreshStringSource(); }";
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let function = module.functions.iter().find(|function| function.name == "nativeFreshStringStore").unwrap();
    let store_index = function.instructions.iter().position(|inst| inst.op == Op::StoreRefCell).unwrap();
    let store = &function.instructions[store_index];
    let stored = store.operands[0];
    let ValueDef::Instruction { inst, .. } = function.value(stored).unwrap().def else {
        panic!("{target}: a retained store has a defining acquire");
    };
    let acquired = function.instruction(inst).unwrap();
    assert_eq!(acquired.op, Op::Acquire, "{target}: the destination owns an independent string copy");
    let source = acquired.operands[0];
    assert_ne!(source, stored);
    assert!(function.instructions[..store_index].iter().any(|inst|
        inst.op == Op::Release && inst.operands == [source]),
        "{target}: parking retires the original expression, not the acquired destination");
    assert!(!function.instructions[..store_index].iter().any(|inst|
        inst.op == Op::Release && inst.operands == [stored]),
        "{target}: the value passed to StoreRefCell must still be owned");
    assert_eq!(store.operands.len(), 3, "{target}: extra parking remains unwind-visible");
    for validation in [false, true] {
        for optimize in [false, true] {
            let mut candidate = module.clone();
            if validation { crate::ir_lower::enable_native_reference_validation(&mut candidate).unwrap(); }
            if optimize { crate::ir_passes::optimize_module(&mut candidate); }
            crate::ir::validate_module(&candidate).unwrap();
            crate::codegen::generate_user_asm_from_ir(&candidate, false, false)
                .unwrap_or_else(|error| panic!("{target}: validation={validation} optimize={optimize}: {error:?}"));
        }
    }
}

supported_target_cases!(native_reference_fresh_string_store_owner,
    verify_native_reference_fresh_string_store_owner);


/// Native static writes use their source mode, while Reflection's internal setter remains weak.
fn verify_native_static_reference_store_modes(target: &str) {
    for strict in [false, true] {
        let source = format!("<?php {} \
            class NativeStaticModeSource {{ public static mixed $value = 0; }} \
            function nativeStaticModeStore(mixed $original): void {{ \
                NativeStaticModeSource::$value = $original; \
                $class = new ReflectionClass(NativeStaticModeSource::class); \
                $class->setStaticPropertyValue('value', $original); }}",
            if strict { "declare(strict_types=1);" } else { "" });
        let mut module = super::lower_source_at_for_target(
            &source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "nativeStaticModeStore").unwrap();
        for (op, expected) in [(Op::StoreStaticProperty, strict), (Op::StoreReflectionStaticProperty, false)] {
            let store = function.instructions.iter().find(|inst| inst.op == op).unwrap();
            assert!(store.operands.len() >= 2, "{target}: original value and assignment mode");
            assert_eq!(function.value(store.operands[0]).unwrap().php_type, crate::types::PhpType::Mixed,
                "{target}: retain the original boxed value");
            let ValueDef::Instruction { inst, .. } = function.value(store.operands[1]).unwrap().def else {
                panic!("{target}: mode must be an explicit constant");
            };
            assert_eq!(function.instruction(inst).unwrap().immediate, Some(Immediate::Bool(expected)),
                "{target}: {op:?}");
        }
        crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
        for optimize in [false, true] {
            let mut candidate = module.clone();
            if optimize { crate::ir_passes::optimize_module(&mut candidate); }
            crate::ir::validate_module(&candidate).unwrap();
            let assembly = crate::codegen::generate_user_asm_from_ir(&candidate, false, false).unwrap();
            assert!(assembly.matches("__rt_reference_publish_native").count() >= 2,
                "{target}: direct and Reflection stores must both use the canonical publisher");
            assert!(assembly.contains("native_static_reference_store_raw"),
                "{target}: raw static storage keeps the ordinary fallback");
        }
    }
}

supported_target_cases!(native_static_reference_store_modes, verify_native_static_reference_store_modes);
