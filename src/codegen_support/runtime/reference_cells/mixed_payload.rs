//! Purpose:
//! Keeps native Mixed reference cells and persistent PHP value aliases on the same storage.
//!
//! Called from:
//! - Native reference-property capture, local/property stores and eval property replacement.
//!
//! Key details:
//! - Kind-eight cells retain their existing descriptor seven and low-word sentinel.
//! - A requested reference lazily wraps the current box without copying its PHP value.
//! - Writers follow the wrapper's immediate child slot and preserve the wrapper identity.

use super::*;

/// Retains an existing managed cell or gives a borrowed Mixed slot an owned wrapper-backed cell.
pub(super) fn emit_retain_mixed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_retain_mixed");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_cell_retain_mixed_borrowed");
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, "__rt_reference_cell_retain_mixed_done");
    emitter.label("__rt_reference_cell_retain_mixed_borrowed");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_reference_cell_promote_mixed");
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, 16);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 7);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::load_at_offset(emitter, scratch, 16);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    emitter.label("__rt_reference_cell_retain_mixed_done");
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Selects the writable child slot of an existing reference without changing any owner counts.
pub(crate) fn follow_mixed_reference_slot(
    emitter: &mut Emitter, address: &str, value: &str, marker: &str, done: &str,
) {
    debug_assert!(address != value && address != marker && value != marker);
    abi::emit_load_from_address(emitter, value, address, 0);
    abi::emit_load_int_immediate(emitter, marker, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {value}, {done}"));               // empty Mixed storage has no reference wrapper
            emitter.instruction(&format!("cmp {value}, {marker}"));             // preserve the low-word uninitialized marker
            emitter.instruction(&format!("b.eq {done}"));                       // never dereference the property sentinel
            emitter.instruction(&format!("ldr {marker}, [{value}]"));           // inspect the stored box without borrowing its child
            emitter.instruction(&format!("cmp {marker}, #7"));                  // persistent references use the nested Mixed tag
            emitter.instruction(&format!("b.ne {done}"));                       // ordinary boxes remain directly writable
            emitter.instruction(&format!("ldr {marker}, [{value}, #16]"));      // inspect the persistent reference marker
            emitter.instruction(&format!("cmp {marker}, #1"));                  // ordinary nested boxes are values rather than aliases
            emitter.instruction(&format!("b.ne {done}"));                       // retain the original native address for ordinary values
            emitter.instruction(&format!("add {address}, {value}, #8"));        // publish assignments into the stable wrapper's child slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {value}, {value}"));             // empty Mixed storage has no reference wrapper
            emitter.instruction(&format!("jz {done}"));                         // preserve the native null storage address
            emitter.instruction(&format!("cmp {value}, {marker}"));             // preserve the low-word uninitialized marker
            emitter.instruction(&format!("je {done}"));                         // never dereference the property sentinel
            emitter.instruction(&format!("cmp QWORD PTR [{value}], 7"));        // persistent references use the nested Mixed tag
            emitter.instruction(&format!("jne {done}"));                        // ordinary boxes remain directly writable
            emitter.instruction(&format!("cmp QWORD PTR [{value} + 16], 1"));   // distinguish a PHP reference from an ordinary nested box
            emitter.instruction(&format!("jne {done}"));                        // retain the native address for an ordinary value
            emitter.instruction(&format!("lea {address}, [{value} + 8]"));      // publish assignments into the stable wrapper's child slot
        }
    }
    emitter.label(done);
}

/// Installs one persistent PHP wrapper while leaving the native cell's address and ownership intact.
pub(super) fn emit_promote_mixed(emitter: &mut Emitter, native_validation: bool) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_promote_mixed");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::store_at_offset(emitter, result, 16);
    abi::emit_load_int_immediate(emitter, scratch, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // do not allocate over an uninitialized property
            emitter.instruction("b.eq __rt_reference_cell_promote_mixed_done"); // leave the existing initialization guard authoritative
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // do not allocate over an uninitialized property
            emitter.instruction("je __rt_reference_cell_promote_mixed_done");   // leave the existing initialization guard authoritative
        }
    }
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_reference_cell_promote_mixed_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x1", 16);
            abi::emit_load_int_immediate(emitter, "x0", 7);
            abi::emit_load_int_immediate(emitter, "x2", 1);
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "rdi", 16);
            abi::emit_load_int_immediate(emitter, "rax", 7);
            abi::emit_load_int_immediate(emitter, "rsi", 1);
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_reference_cell_promote_mixed_done");
    abi::load_at_offset(emitter, result, 8);
    if native_validation {
        super::source_lifecycle::associate_result(emitter);
    }
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target owns a promoted Mixed constructor cell and retires its replaced placeholder.
    #[test]
    fn promoted_mixed_constructor_reference_ownership_on_every_target() {
        let source = "<?php class OwnedMixedConstructor { \
            public function __construct(public mixed &$value = 'default') {} } \
            class OwnedMixedChild extends OwnedMixedConstructor {} \
            class OwnedPrivateMixedConstructor { \
                public function __construct(private mixed &$value) {} } \
            class OwnedPrivateMixedShadow extends OwnedPrivateMixedConstructor { \
                public string $value = 'shadow'; } \
            function makeOwnedMixed(mixed $value): OwnedMixedChild { \
                return new OwnedMixedChild($value); } \
            function makeFreshOwnedMixed(): OwnedMixedConstructor { \
                return new OwnedMixedConstructor($fresh); } \
            function makeDynamicOwnedMixed(string $class): mixed { \
                $value = 'before'; $object = new $class($value); \
                $value = 'after'; return $object; } \
            $object = makeOwnedMixed('initial'); $object->value = 'updated'; \
            $freshObject = makeFreshOwnedMixed();";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
            );
            for class in ["OwnedMixedConstructor", "OwnedMixedChild", "OwnedPrivateMixedConstructor", "OwnedPrivateMixedShadow"] {
                assert!(module.class_infos[class].property_slot_owns_reference(0), "{name}: {class}");
            }
            let dynamic = module.functions.iter().find(|function| function.name == "makeDynamicOwnedMixed").unwrap();
            assert_eq!(dynamic.instructions.iter().filter(|inst| inst.op == crate::ir::Op::ObjectNew).count(), 4,
                "{name}: inherited constructors retain normalization beneath private property shadows");
            let user = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(user.contains("__rt_reference_cell_retain_mixed"), "{name}: constructor retains its binding");
            assert!(user.contains("__rt_reference_cell_release"), "{name}: placeholder owner retires");
            let mut emitter = Emitter::new(target);
            emit_retain_mixed(&mut emitter);
            let runtime = emitter.output();
            assert!(runtime.contains("__rt_reference_cell_owner"), "{name}: reuse exact managed owners");
            assert!(runtime.contains("__rt_reference_cell_promote_mixed"), "{name}: borrowed slots share a persistent wrapper");
            assert!(!runtime.contains("__rt_mixed_clone"), "{name}: keep the original PHP reference identity");
        }
    }
}
