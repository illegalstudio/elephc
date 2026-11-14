//! Purpose:
//! Verifies the typed checked radix kernel before integrating PHP builtin entry points.
//!
//! Called from:
//! - Five-target EIR and native runtime kernel tests.
//!
//! Key details:
//! - A normal PHP harness supplies calls, catches and ownership; only its parser body is EIR-built.
//! - This does not bypass the public semantic-profile admission gate.

use crate::{codegen_support::platform::Target, ir::{Builder, Immediate, IrType, LocalKind,
    Op, Ownership, RuntimeCallTarget, Terminator}, types::PhpType};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds one typed string/base runtime boundary inside ordinary checked PHP call frames.
fn module(source: &str, target: Target) -> crate::ir::Module {
    let source = format!("<?php function checkedRadixKernel(string $value, int $base): int {{ return (int)$value; }} {source}");
    let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    let function = module.functions.iter_mut().find(|function| function.name == "checkedRadixKernel").unwrap();
    function.locals.clear(); function.blocks.clear(); function.values.clear(); function.instructions.clear();
    function.no_epilogue_cleanup_slots.clear(); function.inlined_scopes.clear();
    let mut builder = Builder::new(function);
    let entry = builder.create_named_block("entry", Vec::new()); builder.set_entry(entry); builder.position_at_end(entry);
    let text = builder.add_local(Some("value".into()), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
    let base = builder.add_local(Some("base".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
    let text_value = builder.emit_load_local(text, IrType::Str, PhpType::Str);
    builder.set_value_ownership(text_value, Ownership::Borrowed);
    let base_value = builder.emit_load_local(base, IrType::I64, PhpType::Int);
    let result = builder.emit_with_effects(Op::RuntimeCall, vec![text_value, base_value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::CheckedRadixInt)), IrType::I64, PhpType::Int,
        Ownership::NonHeap, Op::RuntimeCall.default_effects(), None).unwrap();
    builder.terminate(Terminator::Return { value: Some(result) });
    function.no_epilogue_cleanup_slots.insert(text);
    module
}

/// Every target validates the typed input signature and required immutable cast capability.
#[test]
fn strict_checked_radix_eir_emits_all_targets_and_rejects_missing_capability() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = module("echo checkedRadixKernel('ff', 16);", Target::parse(name).unwrap());
        crate::ir::validate_module(&module).unwrap();
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_checked_radix_to_int"));
        module.semantic_profile = SemanticProfile::PERMISSIVE;
        assert!(matches!(crate::ir::validate_module(&module), Err(crate::ir::ValidationError::SemanticProfileMismatch { .. })));
    }
}

/// Native bases and failures agree with the neutral parser while borrowed strings retain owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_checked_radix_native_matrix_matches_neutral_values_and_failures() {
    let mut inputs = Vec::new();
    for text in ["0", "-0", "ff", "-8000000000000000", "7fffffffffffffff", "8000000000000000",
        "8000000000000000z", "0x10", "0b10", "077", "09", "1e3", "0x", "0b", "junk", "1 0", " z ", "\u{b}42\u{c}"] {
        for base in [0, 1, 2, 8, 10, 16, 36, 37] { inputs.push((text.to_string(), base)); }
    }
    let mut source = String::new(); let mut expected = String::new();
    for (text, base) in &inputs {
        source.push_str(&format!("try {{ echo checkedRadixKernel('{text}', {base}); }} catch (ValueError $e) {{ echo $e->getMessage(); }} echo '|';"));
        match elephc_semantics::integer_cast_base::checked_string_to_int_base(text.as_bytes(), *base) {
            Ok(value) => expected.push_str(&value.to_string()), Err(failure) => expected.push_str(failure.message()),
        }
        expected.push('|');
    }
    source.push_str("try { checkedRadixKernel(str_repeat('f', 32), 16); } catch (ValueError $e) { echo 'owned'; }");
    expected.push_str("owned");
    let module = module(&source, Target::detect_host());
    for optimize in [false, true] {
        let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {stdout} {stderr}");
        let actual: Vec<_> = stdout.split('|').collect(); let expected: Vec<_> = expected.split('|').collect();
        assert_eq!(actual.len(), expected.len(), "opt={optimize}");
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(actual, expected, "opt={optimize} case={:?}", inputs.get(index));
        }
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
