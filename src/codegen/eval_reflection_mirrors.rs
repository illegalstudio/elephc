//! Purpose:
//! Stores the PHP-visible string mirrors (`name`, `class`) of Reflection objects that
//! libelephc-magician materializes, honouring object-owned reference properties.
//!
//! Called from:
//! - `crate::codegen::eval_reflection_helpers` (`ReflectionAttribute::$name`).
//! - `crate::codegen::eval_reflection_owner_helpers` (`$name` and `$class` of the owners).
//!
//! Key details:
//! - A property the program ever binds with `=&` is an object-owned reference property:
//!   compiled code expects its slot to hold a reference cell, never the value itself
//!   (`ClassInfo::property_slot_owns_reference`). Compiled construction allocates that cell
//!   (`emit_owned_reference_property_cell`); these helpers must do the same, or a later
//!   reference write stores a 16-byte pair through the 8-byte string it finds there.
//! - The incoming string is the persisted pair `__rt_str_persist` leaves in the target's
//!   string result registers; it becomes the cell's (or the slot's) owned value.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::types::{ClassInfo, PhpType};

/// Returns whether the slot of `property` holds an object-owned reference cell.
pub(super) fn reflection_property_owns_reference(info: &ClassInfo, property: &str) -> bool {
    info.property_offsets
        .get(property)
        .is_some_and(|offset| info.property_slot_owns_reference((offset - 8) / 16))
}

/// Stores the persisted string pair into one mirror slot, through a fresh reference cell when
/// the slot owns one.
///
/// `reload_object` must load the owner object pointer into `object_reg`; it runs after any
/// cell allocation, so it may read the helper's stack frame at its usual offsets.
pub(super) fn emit_store_reflection_mirror_string(
    emitter: &mut Emitter,
    object_reg: &str,
    offset: usize,
    owns_reference: bool,
    reload_object: impl Fn(&mut Emitter),
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    if !owns_reference {
        reload_object(emitter);
        abi::emit_store_to_address(emitter, ptr_reg, object_reg, offset);
        abi::emit_store_to_address(emitter, len_reg, object_reg, offset + 8);
        return;
    }
    // -- wrap the mirror string in the object-owned reference cell its slot expects --
    abi::emit_push_reg(emitter, ptr_reg);
    abi::emit_push_reg(emitter, len_reg);
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        crate::codegen_support::runtime::reference_cells::payload_tag(&PhpType::Str),
    );
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    let cell_reg = abi::int_result_reg(emitter);
    let value_ptr_reg = abi::secondary_scratch_reg(emitter);
    let value_len_reg = abi::tertiary_scratch_reg(emitter);
    abi::emit_pop_reg(emitter, value_len_reg);
    abi::emit_pop_reg(emitter, value_ptr_reg);
    abi::emit_store_to_address(emitter, value_ptr_reg, cell_reg, 0);
    abi::emit_store_to_address(emitter, value_len_reg, cell_reg, 8);
    reload_object(emitter);
    abi::emit_store_to_address(emitter, cell_reg, object_reg, offset);
    abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
}
