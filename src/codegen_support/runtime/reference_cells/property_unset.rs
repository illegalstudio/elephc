//! Purpose:
//! Detaches object-owned typed reference properties without invalidating surviving aliases.
//!
//! Called from:
//! - Native EIR PropUnset and the eval native-property bridge.
//!
//! Key details:
//! - A fresh uninitialized cell is published before the object's old owner is released.
//! - Reentrant destructors may recreate the property without a later overwrite.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;

/// Addresses an eligible object slot and invokes the shared native detach operation.
pub(crate) fn emit_owned_property_unset(emitter: &mut Emitter, base: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_reg_move(emitter, "x0", base);
            abi::emit_load_int_immediate(emitter, "x9", offset as i64);
            emitter.instruction("add x0, x0, x9");                              // pass the physical reference-property slot address
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea rax, [{base} + {offset}]"));      // pass the physical reference-property slot address
        }
    }
    abi::emit_call_label(emitter, "__rt_reference_property_unset");
}

/// Replaces the object's cell owner with uninitialized storage, then retires the old owner.
pub(super) fn emit_property_unset(emitter: &mut Emitter, native_validation: bool) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_property_unset");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, scratch, result, 0);
    abi::store_at_offset(emitter, scratch, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [{scratch}, #-8]"));          // read the old managed cell's payload descriptor
            emitter.instruction("ubfx x0, x0, #8, #7");                         // preserve the declared native payload shape
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [{scratch} - 8]")); // read the old managed cell's payload descriptor
            emitter.instruction("shr rdi, 8");                                  // position the payload tag for cell allocation
            emitter.instruction("and edi, 127");                                // exclude heap-kind and collector flag bits
        }
    }
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::emit_load_int_immediate(emitter, scratch, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::emit_store_zero_to_address(emitter, result, 8);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 8);
    if native_validation {
        abi::store_at_offset(emitter, result, 24);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 24);
        let entry = emitter.target.extern_symbol("__elephc_reference_sources_rebind_v1");
        abi::emit_call_label(emitter, &entry);
        super::source_lifecycle::require_success(emitter);
        abi::load_at_offset(emitter, result, 24);
        super::source_lifecycle::associate_result(emitter);
    }
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use std::path::Path;

    /// Selected runtimes move the physical type source before the displaced cell can run PHP.
    #[test]
    fn native_reference_unset_rebinds_before_retiring_the_old_cell() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_property_unset(&mut emitter, true);
            let assembly = emitter.output();
            let publish = if target.arch == Arch::AArch64 { "str x0, [x10]" }
                else { "mov QWORD PTR [r10], rax" };
            let rebound = assembly.find("__elephc_reference_sources_rebind_v1").unwrap();
            let associated = assembly.find("__rt_reference_source_associate_native").unwrap();
            let release = assembly.find("__rt_decref_any").unwrap();
            assert!(assembly.find(publish).unwrap() < rebound && rebound < associated && associated < release,
                "{name}: the old wrapper loses this binding before a destructor can assign through it");
            let mut emitter = Emitter::new(target);
            emit_property_unset(&mut emitter, false);
            assert!(!emitter.output().contains("__elephc_reference_sources"),
                "{name}: ordinary runtimes must not gain a native registry dependency");
        }
    }

    /// Every ABI detaches the object's cell before a release can execute a destructor.
    #[test]
    fn owned_reference_unset_publishes_before_release_on_every_target() {
        let source = "<?php class ReferenceUnsetTarget { public string $value = 'old'; \
            public function &valueRef(): string { return $this->value; } } \
            $object = new ReferenceUnsetTarget(); $alias =& $object->valueRef(); unset($object->value);";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                source, Path::new("main.php"), Path::new("."), target,
            );
            let user = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(user.contains("__rt_reference_property_unset"), "{name}: native detach call");
            let mut emitter = Emitter::new(target);
            emit_property_unset(&mut emitter, false);
            let runtime = emitter.output();
            let publish = if target.arch == Arch::AArch64 { "str x0, [x10]" }
                else { "mov QWORD PTR [r10], rax" };
            let release = runtime.find("__rt_decref_any").unwrap();
            assert!(runtime.find(publish).is_some_and(|position| position < release), "{name}: detach before destructor");
        }
    }
}
