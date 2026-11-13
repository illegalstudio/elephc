//! Purpose:
//! Pins hidden semantic arguments on native OPcache calls for every supported ABI.
//!
//! Called from:
//! - The compiler library unit-test harness.
//!
//! Key details:
//! - Parses are profile-specific; invalidation remains an operation on the whole path.

use super::*;
use crate::codegen::{data_section::DataSection, emit::Emitter, frame, platform::Target,
    shared_helper::helper_function, shared_state::SharedCodegenState};
use crate::ir::{Effects, IrType, Module, Op, Ownership, ValueId};
use crate::types::PhpType;

/// Verifies each profile-sensitive operation receives ABI version and profile bits on all targets.
#[test]
fn strict_cache_path_calls_pass_profiles_on_all_supported_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for operation in [OpcachePathOperation::IsCached, OpcachePathOperation::Compile,
            OpcachePathOperation::InFileCache, OpcachePathOperation::Discard, OpcachePathOperation::SoftInvalidate] {
            let mut module = Module::new(target);
            module.semantic_profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Bools, true);
            module.required_runtime_features.eval_bridge = true;
            let mut function = helper_function("opcache_profile", PhpType::Void);
            function.params[0].php_type = PhpType::Str;
            function.params[0].ir_type = IrType::Str;
            function.values[0].php_type = PhpType::Str;
            function.values[0].ir_type = IrType::Str;
            let layout = frame::layout_for_function(&function, target, false, false, false);
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            let mut shared = SharedCodegenState::default();
            let inst = Instruction::new(Op::RuntimeCall, vec![ValueId::from_raw(0)], None, None,
                IrType::Void, PhpType::Void, Ownership::NonHeap, Effects::PURE, None);
            {
                let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                    &mut shared, layout, false, false, false, None);
                lower_opcache_rt_path_call(&mut ctx, &inst, operation).unwrap();
            }
            let asm = emitter.output();
            let (_, suffix) = operation.names();
            assert!(asm.contains(&target.extern_symbol(&format!("__elephc_eval_opcache_rt_{suffix}"))), "{name}: {asm}");
            let (version, bits) = match target.arch {
                Arch::AArch64 => ("mov x2, #1", "mov x3, #32"),
                Arch::X86_64 => ("mov rdx, 1", "mov rcx, 32"),
            };
            assert_eq!(asm.contains(version), operation.uses_profile(), "{name}: {asm}");
            assert_eq!(asm.contains(bits), operation.uses_profile(), "{name}: {asm}");
        }
    }
}
