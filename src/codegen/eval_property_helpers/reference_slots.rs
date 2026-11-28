//! Purpose:
//! Resolves eval bridge property payloads through their native reference-cell layout.
//!
//! Called from:
//! - Property getters, setters, initialization probes and retaining replacement stores.
//!
//! Key details:
//! - A reference property's object slot keeps the cell pointer across value writes.
//! - A typed reference's uninitialized sentinel occupies the cell's low payload word.

use super::*;
use std::borrow::Cow;

/// Selects the payload address while leaving ordinary fixed object slots unchanged.
pub(super) fn payload_slot<'a>(
    emitter: &mut Emitter, slot: &'a EvalPropertySlot, base: &str,
) -> Cow<'a, EvalPropertySlot> {
    if !slot.is_reference { return Cow::Borrowed(slot); }
    abi::emit_load_from_address(emitter, base, base, slot.offset);
    let mut payload = slot.clone();
    payload.offset = 0;
    payload.is_reference = false;
    Cow::Owned(payload)
}

/// Selects the same uninitialized marker address used by ordinary native property reads.
pub(super) fn marker_address(emitter: &mut Emitter, slot: &EvalPropertySlot, base: &str) -> usize {
    if slot.is_reference {
        abi::emit_load_from_address(emitter, base, base, slot.offset);
        0
    } else { slot.offset + 8 }
}

/// Returns an owned value copy from a promoted wrapper, then opens the raw fallback block.
pub(super) fn begin_canonical_read(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
) -> Option<String> {
    if !slot.is_reference || crate::types::canonical_property_reference_payload(&slot.ty).is_none() {
        return None;
    }
    let raw = format!("{}_canonical_raw", slot_body_label_raw(slot, "get"));
    let done = format!("{}_canonical_done", slot_body_label_raw(slot, "get"));
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the native object
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset));    // select its managed reference cell
            emitter.instruction("bl __rt_reference_cell_canonical");            // inspect its current physical representation
            emitter.instruction(&format!("cbz x0, {raw}"));                     // raw cells retain their declared bridge path
            emitter.instruction("bl __rt_mixed_deref");                         // expose the wrapper's current concrete child
            emitter.instruction("bl __rt_mixed_clone");                         // return a value copy independent from the alias
            emitter.instruction(&format!("b {done}"));                          // join after the cloned canonical read
        }
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the native object
            emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", slot.offset)); // select its managed reference cell
            emitter.instruction("call __rt_reference_cell_canonical");          // inspect its current physical representation
            emitter.instruction("test rax, rax");                               // test whether the cell owns a canonical wrapper
            emitter.instruction(&format!("jz {raw}"));                          // raw cells retain their declared bridge path
            emitter.instruction("call __rt_mixed_deref");                       // expose the wrapper's current concrete child
            emitter.instruction("call __rt_mixed_clone");                       // return a value copy independent from the alias
            emitter.instruction(&format!("jmp {done}"));                        // join after the cloned canonical read
        }
    }
    emitter.label(&raw);
    Some(done)
}

/// Publishes a converted numeric scalar through a canonical reference wrapper when present.
///
/// Eval scope import can promote an object-owned scalar reference cell from its raw declaration
/// layout to a persistent Mixed wrapper. Direct property writes must preserve that wrapper and
/// replace its boxed child instead of overwriting the cell's wrapper pointer with raw scalar bits.
pub(super) fn emit_canonical_scalar_store(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    result_reg: &str,
) -> bool {
    if !slot.is_reference
        || !matches!(slot.ty.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::Float)
    {
        return false;
    }
    let raw = format!("{}_canonical_raw", slot_body_label_raw(slot, "set"));
    let done = format!("{}_canonical_done", slot_body_label_raw(slot, "set"));
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("str {result_reg}, [sp, #56]"));       // preserve the converted scalar across canonical-shape probing
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the native object
            emitter.instruction(&format!("ldr x9, [x9, #{}]", slot.offset));    // select its managed reference cell
            emitter.instruction("str x9, [sp, #64]");                           // preserve the raw cell for the legacy branch
            emitter.instruction("mov x0, x9");                                  // probe the cell's current physical representation
            emitter.instruction("bl __rt_reference_cell_canonical");            // inspect its current physical representation
            emitter.instruction(&format!("cbz x0, {raw}"));                     // raw cells keep their declared scalar ABI
            emitter.instruction("str x0, [sp, #72]");                           // preserve the persistent wrapper
            emitter.instruction(&format!("ldr {result_reg}, [sp, #56]"));       // restore the converted scalar for boxing
            crate::codegen::emit_box_current_value_as_mixed(emitter, &slot.ty);
            emitter.instruction("ldr x9, [sp, #72]");                           // recover the wrapper after Mixed allocation
            emitter.instruction("ldr x10, [x9, #8]");                           // snapshot its displaced child
            emitter.instruction("str x0, [x9, #8]");                            // publish the replacement before cleanup
            emitter.instruction("mov x0, x10");                                 // retire the previous boxed child
            emitter.instruction("bl __rt_decref_mixed");                        // release the displaced boxed child
            emitter.instruction(&format!("b {done}"));                          // join after canonical replacement
            emitter.label(&raw);
            emitter.instruction("ldr x9, [sp, #64]");                           // recover the raw reference cell
            emitter.instruction(&format!("ldr {result_reg}, [sp, #56]"));       // restore the converted scalar
            emitter.instruction(&format!("str {result_reg}, [x9]"));            // publish through the legacy scalar layout
            emitter.instruction("str xzr, [x9, #8]");                           // clear its typed-property marker
        }
        Arch::X86_64 => {
            if slot.ty.codegen_repr() == PhpType::Float {
                emitter.instruction(&format!("movsd QWORD PTR [rbp - 64], {result_reg}")); // preserve the converted float across canonical-shape probing
            } else {
                emitter.instruction(&format!("mov QWORD PTR [rbp - 64], {result_reg}")); // preserve the converted integer across canonical-shape probing
            }
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the native object
            emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", slot.offset)); // select its managed reference cell
            emitter.instruction("mov rax, r11");                                // probe the cell's current physical representation
            emitter.instruction("call __rt_reference_cell_canonical");          // inspect its current physical representation
            emitter.instruction("test rax, rax");                               // test whether the cell owns a canonical wrapper
            emitter.instruction(&format!("jz {raw}"));                          // raw cells keep their declared scalar ABI
            emitter.instruction("mov QWORD PTR [rbp - 72], rax");               // preserve the persistent wrapper
            if slot.ty.codegen_repr() == PhpType::Float {
                emitter.instruction(&format!("movsd {result_reg}, QWORD PTR [rbp - 64]")); // restore the converted float for boxing
            } else {
                emitter.instruction(&format!("mov {result_reg}, QWORD PTR [rbp - 64]")); // restore the converted integer for boxing
            }
            crate::codegen::emit_box_current_value_as_mixed(emitter, &slot.ty);
            emitter.instruction("mov r11, QWORD PTR [rbp - 72]");               // recover the wrapper after Mixed allocation
            emitter.instruction("mov r10, QWORD PTR [r11 + 8]");                // snapshot its displaced child
            emitter.instruction("mov QWORD PTR [r11 + 8], rax");                // publish the replacement before cleanup
            emitter.instruction("mov rax, r10");                                // retire the previous boxed child
            emitter.instruction("call __rt_decref_mixed");                      // release the displaced boxed child
            emitter.instruction(&format!("jmp {done}"));                        // join after canonical replacement
            emitter.label(&raw);
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // recover the native object
            emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", slot.offset)); // recover its raw reference cell
            if slot.ty.codegen_repr() == PhpType::Float {
                emitter.instruction(&format!("movsd {result_reg}, QWORD PTR [rbp - 64]")); // restore the converted float
                emitter.instruction(&format!("movsd QWORD PTR [r11], {result_reg}")); // publish through the legacy float layout
            } else {
                emitter.instruction(&format!("mov {result_reg}, QWORD PTR [rbp - 64]")); // restore the converted integer
                emitter.instruction(&format!("mov QWORD PTR [r11], {result_reg}")); // publish through the legacy integer layout
            }
            emitter.instruction("mov QWORD PTR [r11 + 8], 0");                  // clear its typed-property marker
        }
    }
    emitter.label(&done);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Eval getters, retaining stores and probes all follow the managed cell on every target.
    #[test]
    fn native_reference_property_bridge_preserves_cell_addresses_on_every_target() {
        let tokens = crate::lexer::tokenize("<?php class NativeReferenceBridge { \
            public mixed $value = null; \
            public function &valueRef(): mixed { return $this->value; } }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = Module::new(target);
            module.class_infos.insert("NativeReferenceBridge".into(), checked.classes["NativeReferenceBridge"].clone());
            let slots = collect_eval_property_slots(&module);
            let slot = slots.iter().find(|slot| slot.property == "value").unwrap();
            assert!(slot.is_reference, "{name}: checker promotion");
            let mut get = Emitter::new(target);
            let mut set = Emitter::new(target);
            let mut probe = Emitter::new(target);
            match target.arch {
                Arch::AArch64 => {
                    emit_aarch64_box_property_slot(&mut get, slot);
                    emit_aarch64_property_initialized_flag(&mut probe, slot);
                }
                Arch::X86_64 => {
                    emit_x86_64_box_property_slot(&mut get, slot);
                    emit_x86_64_property_initialized_flag(&mut probe, slot);
                }
            }
            emit_owned_slot_replacement(&mut set, slot);
            let get = get.output();
            let set = set.output();
            let probe = probe.output();
            let dereference = match target.arch {
                Arch::AArch64 => format!("ldr x9, [x9, #{}]", slot.offset),
                Arch::X86_64 => format!("mov r11, QWORD PTR [r11 + {}]", slot.offset),
            };
            assert!(get.contains(&dereference) && set.contains(&dereference), "{name}: payload follows the cell");
            let canonical = get.find("__rt_reference_cell_canonical").unwrap();
            let deref = get.find("__rt_mixed_deref").unwrap();
            let clone = get.find("__rt_mixed_clone").unwrap();
            assert!(canonical < deref && deref < clone, "{name}: promoted reads return an independent concrete value");
            assert!(get.contains("__rt_mixed_clone"), "{name}: by-value read detaches the pointee");
            let marker = if target.arch == Arch::AArch64 { "ldr x11, [x10, #0]" }
                else { "mov rax, QWORD PTR [r11 + 0]" };
            assert!(probe.contains(marker), "{name}: reference initialization lives in the cell");
        }
    }

    /// Direct eval stores preserve promoted scalar wrappers on every target.
    #[test]
    fn native_reference_scalar_stores_preserve_canonical_wrappers_on_every_target() {
        let tokens = crate::lexer::tokenize("<?php class NativeScalarReferenceBridge { \
            public int $value = 1; \
            public float $weight = 1.5; \
            public function &valueRef(): int { return $this->value; } \
            public function &weightRef(): float { return $this->weight; } }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = Module::new(target);
            module.class_infos.insert("NativeScalarReferenceBridge".into(), checked.classes["NativeScalarReferenceBridge"].clone());
            let slots = collect_eval_property_slots(&module);
            for property in ["value", "weight"] {
                let slot = slots.iter().find(|slot| slot.property == property).unwrap();
                assert!(slot.is_reference, "{name} {property}: checker promotion");
                let mut emitter = Emitter::new(target);
                let result = if slot.ty.codegen_repr() == PhpType::Float {
                    abi::float_result_reg(&emitter)
                } else {
                    abi::int_result_reg(&emitter)
                }.to_string();
                assert!(emit_canonical_scalar_store(&mut emitter, slot, &result));
                let assembly = emitter.output();
                let probe = assembly.find("__rt_reference_cell_canonical").unwrap();
                let box_value = assembly.find("__rt_mixed_from_value").unwrap();
                let publish = assembly.find(if target.arch == Arch::AArch64 {
                    "str x0, [x9, #8]"
                } else {
                    "mov QWORD PTR [r11 + 8], rax"
                }).unwrap();
                let release = assembly.find("__rt_decref_mixed").unwrap();
                assert!(probe < box_value && box_value < publish && publish < release, "{name} {property}: {assembly}");
                assert!(assembly.contains("canonical_raw"), "{name} {property}: raw reference cells keep their declaration layout");
            }
        }
    }
}
