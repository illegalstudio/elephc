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
            assert!(get.contains("__rt_mixed_clone"), "{name}: by-value read detaches the pointee");
            let marker = if target.arch == Arch::AArch64 { "ldr x11, [x10, #0]" }
                else { "mov rax, QWORD PTR [r11 + 0]" };
            assert!(probe.contains(marker), "{name}: reference initialization lives in the cell");
        }
    }
}
