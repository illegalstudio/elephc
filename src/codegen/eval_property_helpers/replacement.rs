//! Purpose:
//! Transfers prepared heap values into native properties and releases displaced owners.
//!
//! Called from:
//! - The eval property bridge's string, Mixed, array, and object setters.
//!
//! Key details:
//! - Validation and acquisition happen first, so failed writes leave storage intact.
//! - The new value is visible before releasing the old one can invoke a destructor.

use super::*;

/// Installs an already-owned result and releases the previous slot owner through heap-kind dispatch.
pub(super) fn emit_owned_slot_replacement(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    let object_frame_offset = match emitter.target.arch {
        Arch::AArch64 => 64,
        Arch::X86_64 => 24,
    };
    let object = abi::symbol_scratch_reg(emitter);
    let previous = abi::secondary_scratch_reg(emitter);
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, object, object_frame_offset);
    abi::emit_load_from_address(emitter, previous, object, slot.offset);
    if slot.ty.codegen_repr() == PhpType::Str {
        let (pointer, length) = abi::string_result_regs(emitter);
        abi::emit_store_to_address(emitter, pointer, object, slot.offset);
        abi::emit_store_to_address(emitter, length, object, slot.offset + 8);
    } else {
        abi::emit_store_to_address(emitter, result, object, slot.offset);
        abi::emit_store_zero_to_address(emitter, object, slot.offset + 8);
    }
    abi::emit_reg_move(emitter, result, previous);
    abi::emit_call_label(emitter, "__rt_decref_any");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target publishes the new payload and initialization word before releasing the old owner.
    #[test]
    fn replacement_publishes_value_before_releasing_old_storage() {
        let source = "<?php class OwnedSlots { public string $text; public mixed $value; public array $items; public object $child; }";
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = Module::new(target);
            module.class_infos.insert("OwnedSlots".into(), checked.classes["OwnedSlots"].clone());
            for slot in collect_eval_property_slots(&module) {
                let mut emitter = Emitter::new(target);
                emit_owned_slot_replacement(&mut emitter, &slot);
                let asm = emitter.output();
                assert_eq!(asm.matches("__rt_decref_any").count(), 1, "{name}");
                let marker = match target.arch {
                    Arch::AArch64 => format!("[x9, #{}]", slot.offset + 8),
                    Arch::X86_64 => format!("[r11 + {}]", slot.offset + 8),
                };
                assert!(asm.find(&marker).unwrap() < asm.find("__rt_decref_any").unwrap(), "{name}");
            }
        }
    }
}
