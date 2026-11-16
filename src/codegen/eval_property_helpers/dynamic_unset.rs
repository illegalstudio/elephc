//! Purpose:
//! Detaches eval-declared backing properties from their native property hash.
//!
//! Called from:
//! - Authorized eval property unset after fixed native slot dispatch misses.
//!
//! Key details:
//! - Writing null would mutate an existing reference instead of removing its physical binding.
//! - The unique table is installed before removal can release values or invoke destructors.
//! - The receiver and property name use the enclosing unset/getter frame layout.

use super::{abi, Arch, Emitter, Module};

/// Removes one authorized eval storage entry while keeping captured references independent.
pub(super) fn emit(module: &Module, emitter: &mut Emitter, fail: &str, done: &str) {
    let (receiver, name, length, slot) = match module.target.arch {
        Arch::AArch64 => (48, 64, 56, 32),
        Arch::X86_64 => (24, 8, 16, 40),
    };
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let success = "__rt_eval_typed_property_unset_dynamic_success";
    abi::load_at_offset(emitter, abi::int_arg_reg_name(module.target, 0), receiver);
    let owns = module.target.extern_symbol("__elephc_eval_dynamic_object_owns_properties");
    abi::emit_call_label(emitter, &owns);
    abi::emit_branch_if_int_result_zero(emitter, fail);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(module.target, 0), receiver);
    abi::emit_call_label(emitter, super::dynamic_properties::SLOT_HELPER);
    abi::emit_branch_if_int_result_zero(emitter, fail);
    abi::store_at_offset(emitter, result, slot);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_branch_if_int_result_zero(emitter, success);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(module.target, 0), result);
    abi::emit_call_label(emitter, "__rt_hash_ensure_unique");
    abi::load_at_offset(emitter, scratch, slot);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(module.target, 0), result);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(module.target, 1), name);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(module.target, 2), length);
    abi::emit_call_label(emitter, "__rt_hash_unset");
    emitter.label(success);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_jump(emitter, done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported ABI installs unique storage before removing a captured property binding.
    #[test]
    fn eval_dynamic_property_unset_detaches_before_retirement_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = Module::new(Target::parse(name).unwrap());
            let mut emitter = Emitter::new(module.target);
            emit(&module, &mut emitter, "miss", "done");
            let assembly = emitter.output();
            let probe = assembly.find("__elephc_eval_dynamic_object_owns_properties").unwrap();
            let split = assembly.find("__rt_hash_ensure_unique").unwrap();
            let unset = assembly.find("__rt_hash_unset").unwrap();
            assert!(probe < split && split < unset, "{name}");
            if module.target.arch == Arch::X86_64 {
                assert!(assembly[..split].contains("mov rdi, rax"), "{name}: COW uses its C-ABI input register");
            }
            // Register allocation belongs to ABI helpers; require the actual pointer store shape.
            let between = &assembly[split..unset];
            assert!(between.contains("str x0, [") || between.contains("mov QWORD PTR ["), "{name}");
            assert!(!assembly[unset..].contains("QWORD PTR ["), "{name}: no table publication after removal");
        }
    }

    /// Clang accepts both target frame layouts and the protected dynamic unset fallback.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn eval_dynamic_property_unset_assembles_on_every_target() {
        use std::{io::Write, process::{Command, Stdio}};
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            let module = Module::new(Target::parse(name).unwrap());
            let mut emitter = Emitter::new(module.target);
            super::super::unset::emit_property_unset_helper(&module, &mut emitter,
                &mut crate::codegen::data_section::DataSection::new(), &[]);
            let syntax = if module.target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let assembly = format!("{syntax}.text\n{}", emitter.output());
            let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c",
                "-o", "/dev/null", "-"]).stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
            process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
            let output = process.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
