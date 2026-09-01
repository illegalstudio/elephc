//! Purpose:
//! Lowers object metadata opcodes for the Phase 04 EIR backend.
//! Supports simple object allocation, declared property access, and named or dynamic `instanceof` checks.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Object payload layout must match the runtime helpers:
//!   heap kind word before payload, class id at payload offset 0, then 16 bytes
//!   per declared property slot plus an optional dynamic-property hash pointer.
//! - Reference properties store a pointer to a local or heap ref-cell in the
//!   property slot, while normal declared properties store values directly.
//! - This slice intentionally rejects interface method entries that need missing
//!   EIR symbols and non-literal default property expressions until their runtime
//!   paths land.

use std::collections::HashSet;

use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::codegen_support::sentinels::THROWABLE_CREATION_LINE_OFFSET;
use crate::codegen::{
    abi, callable_descriptor, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed, runtime_value_tag,
};
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::codegen_support::dynamic_new::known_dynamic_new_builtin_class_names;
use crate::names::{
    label_fragment, method_symbol, php_symbol_key, property_hook_get_method,
    property_hook_set_method,
};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

use super::super::context::FunctionContext;
use super::{
    builtins, callables, cast_loaded_mixed_pointer_to_result, direct_call_stack_pad_bytes,
    expect_data,
    coerce_loaded_value_to_tagged_scalar, emit_instance_method_descriptor_entry_wrapper,
    emit_loaded_assoc_array_to_mixed,
    emit_loaded_indexed_array_to_mixed, emit_mixed_string_for_persistent_store,
    emit_ref_arg_writebacks, expect_operand, iterators, load_value_to_first_int_arg,
    emit_dynamic_instance_method_call, materialize_method_call_args_with_receiver_reg_and_refs,
    emit_date_special_trace_begin,
    resolve_method_call_target, MethodCallTarget,
    emit_runtime_callable_invoker_inline, property_values, store_if_result,
    store_method_call_result,
};
use crate::codegen::fibers;
use crate::codegen::literal_defaults::{
    emit_array_literal_default_to_result, emit_assoc_array_literal_default_to_result,
    emit_boxed_bool_literal_to_result, emit_boxed_float_literal_to_result,
    emit_boxed_int_literal_to_result, emit_boxed_null_literal_to_result,
    emit_boxed_string_literal_default_to_result, emit_empty_assoc_array_literal_to_result,
    emit_string_literal_default_to_result, emit_tagged_int_literal_to_result,
    emit_tagged_null_literal_to_result, literal_default_value, LiteralDefaultValue,
};
use crate::codegen::{CodegenIrError, Result};

mod reflection;

const RUNTIME_NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;
const ITERATOR_ITERATOR_DOWNCAST_MESSAGE: &str =
    "Class to downcast to not found or not base class or does not implement Traversable";

/// Resolved declared-property storage metadata for a known object receiver.
#[derive(Clone)]
pub(super) struct PropertySlot {
    class_name: String,
    property: String,
    php_type: PhpType,
    offset: usize,
    is_declared: bool,
    is_packed: bool,
    is_reference: bool,
}

/// Declared-property candidate reachable from a `Mixed` object receiver.
struct MixedPropertyCandidate {
    class_id: u64,
    slot: PropertySlot,
}

/// Resolved object property default metadata for fixed-offset initialization.
struct PropertyDefault {
    offset: usize,
    value: LiteralDefaultValue,
    /// `true` when the slot holds a ref-cell pointer (an object-owned reference property);
    /// the default is written THROUGH the cell instead of directly into the slot.
    is_reference: bool,
}

/// Concrete class that a dynamic factory can instantiate in this EIR module.
struct DynamicNewCandidate {
    class_name: String,
    class_id: u64,
    property_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: Vec<usize>,
    owned_reference_property_offsets: Vec<usize>,
    property_defaults: Vec<PropertyDefault>,
    constructor_impl: Option<ConstructorCallTarget>,
}

/// Constructor metadata needed after object allocation has produced `$this`.
struct ConstructorCallTarget {
    impl_class: String,
    param_types: Vec<PhpType>,
    ref_params: Vec<bool>,
    sig: crate::types::FunctionSig,
    /// Symbol to call instead of the constructor itself, when the site passes fewer arguments
    /// than the constructor declares. `ir_lower` emits `_class_ctor_<id>_<argc>` for exactly
    /// those pairs; it takes what the site passed and calls the real constructor with the
    /// declared defaults appended. Codegen cannot build those defaults itself — by the time it
    /// runs, arguments are materialized SSA values and a default is still an expression.
    padding_thunk: Option<String>,
}


mod fixed_new;
mod clone_and_spl;
mod mixed_clone;
mod iterator_iterator;
pub(in crate::codegen::lower_inst) mod throwable_new;
mod fiber_dynamic_entry;
mod dynamic_mixed_candidates;
mod dynamic_factory;
mod dynamic_pdo;
mod property_defaults;
mod known_property_reads;
mod mixed_property_reads;
mod property_read_compat;
mod dynamic_property_read_entry;
mod dynamic_property_read_resolution;
mod runtime_property_writes;
mod named_property_writes;
mod dynamic_property_write_compat;
mod instanceof_entry;
mod allocation_clone;
mod interface_layout;
mod property_resolution;
mod property_compatibility;
mod property_loads;
mod property_fetch_for_write;
mod property_stores;
mod property_store_values;
mod typed_property_guards;
mod instanceof_helpers;

#[allow(unused_imports)]
use fixed_new::*;
#[allow(unused_imports)]
use clone_and_spl::*;
#[allow(unused_imports)]
use mixed_clone::*;
#[allow(unused_imports)]
use iterator_iterator::*;
#[allow(unused_imports)]
use throwable_new::*;
#[allow(unused_imports)]
use fiber_dynamic_entry::*;
#[allow(unused_imports)]
use dynamic_mixed_candidates::*;
#[allow(unused_imports)]
use dynamic_factory::*;
#[allow(unused_imports)]
pub(in crate::codegen::lower_inst) use dynamic_pdo::*;
#[allow(unused_imports)]
use property_defaults::*;
#[allow(unused_imports)]
use known_property_reads::*;
#[allow(unused_imports)]
use mixed_property_reads::*;
#[allow(unused_imports)]
use property_read_compat::*;
#[allow(unused_imports)]
use dynamic_property_read_entry::*;
#[allow(unused_imports)]
use dynamic_property_read_resolution::*;
#[allow(unused_imports)]
use runtime_property_writes::*;
#[allow(unused_imports)]
use named_property_writes::*;
#[allow(unused_imports)]
use dynamic_property_write_compat::*;
#[allow(unused_imports)]
use instanceof_entry::*;
#[allow(unused_imports)]
use allocation_clone::*;
#[allow(unused_imports)]
use interface_layout::*;
#[allow(unused_imports)]
use property_resolution::*;
#[allow(unused_imports)]
use property_compatibility::*;
#[allow(unused_imports)]
use property_loads::*;
#[allow(unused_imports)]
use property_stores::*;
#[allow(unused_imports)]
use property_store_values::*;
#[allow(unused_imports)]
use typed_property_guards::*;
#[allow(unused_imports)]
use instanceof_helpers::*;

pub(super) use dynamic_property_read_entry::{lower_dynamic_prop_get, lower_nullsafe_prop_get};
pub(super) use fiber_dynamic_entry::{
    lower_dynamic_object_new, lower_dynamic_object_new_mixed,
    lower_dynamic_object_new_without_constructor_mixed,
};
pub(super) use fixed_new::{lower_object_new, lower_object_new_without_constructor};
pub(super) use instanceof_entry::{lower_instanceof, lower_instanceof_dynamic};
pub(super) use known_property_reads::{
    lower_load_prop_ref_cell, lower_prop_get, lower_prop_initialized,
};
pub(super) use property_fetch_for_write::lower_prop_get_for_write;
pub(super) use property_store_values::lower_packed_field_mixed_to_int;
pub(super) use property_resolution::{
    emit_boxed_null, emit_nullable_receiver_object_payload, nullable_object_receiver_class,
    raw_value_php_type,
};
pub(super) use runtime_property_writes::{
    lower_dynamic_prop_set, lower_prop_set, lower_prop_unset,
};
pub(super) use dynamic_property_write_compat::lower_dynamic_property_fetch_for_write;
pub(super) use clone_and_spl::lower_object_clone_shallow;

/// Resolves the declared property slot targeted by a direct container mutation.
pub(super) fn resolve_mutated_container_property(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let property = property_name_immediate(ctx, inst)?;
    resolve_property_slot(ctx, object, property, inst)
}

/// Stores a mutating builtin's retained container result through the object-lowering facade.
pub(super) fn store_mutated_container_property_owner(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    slot: &PropertySlot,
    value: ValueId,
) -> Result<()> {
    store_mutated_container_property(ctx, object, slot, value)
}

/// Stamps a declared property slot with the uninitialized-typed-property marker.
///
/// The payload word is zeroed and the high word receives
/// `UNINITIALIZED_TYPED_PROPERTY_SENTINEL`, the same encoding a typed property without
/// a default carries, so every existing consumer (`PropInitialized`, the read guard,
/// `__rt_obj_prop_name`/`__rt_obj_prop_value`) already understands the state.
fn emit_property_uninitialized_marker(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) {
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset);
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    abi::emit_store_to_address(ctx.emitter, marker_reg, base_reg, slot.offset + 8);
}

/// Removes a dynamic property from the receiver's property hash (`unset($obj->name)`).
///
/// The receiver stores its dynamic properties in a hash whose pointer lives at
/// `hash_offset` — offset 8 for `stdClass`, just past the fixed slots for an
/// `#[AllowDynamicProperties]` class. `__rt_hash_unset` copy-on-write splits the table,
/// releases the removed key and the boxed `Mixed` value the entry owned, tombstones the
/// slot so other probe chains survive, and returns the unique table pointer, which is
/// stored back into the receiver. Removing an absent key is a no-op inside the helper,
/// so `unset($obj->never_set)` and a repeated `unset()` both behave like PHP.
///
/// The receiver register is caller-saved, so it is parked on the temporary stack across
/// the helper call and reloaded before the table pointer is stored back.
fn lower_dynamic_prop_unset(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    hash_offset: usize,
) -> Result<()> {
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x0, [{}, #{}]", object_reg, hash_offset)); // load the dynamic-property hash pointer from the receiver
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "x0", object_reg, hash_offset);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "mov rdi, QWORD PTR [{} + {}]",
                object_reg, hash_offset
            ));                                                                 // load the dynamic-property hash pointer from the receiver
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, hash_offset);
        }
    }
    Ok(())
}

/// Names the reason a resolved fixed property slot cannot represent PHP's "removed"
/// state, or `None` when the uninitialized marker is a faithful encoding for it.
///
/// Used only for the diagnostic text; the ordering matters because a slot can be both
/// undeclared and by-reference, and the by-reference storage is the more specific
/// obstacle to report.
fn unset_unsupported_slot_reason(slot: &PropertySlot) -> Option<&'static str> {
    if slot.is_packed {
        return Some("packed class field");
    }
    if slot.is_reference {
        return Some("by-reference property");
    }
    if !slot.is_declared {
        return Some("untyped property slot");
    }
    None
}

/// Writes the tagged scalar currently held in the result registers into a property slot: the
/// payload word at `offset` and the runtime tag word at `offset + 8`, matching the layout the
/// tagged-scalar property load and store helpers use.
fn emit_tagged_scalar_property_default_store(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    offset: usize,
) {
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        object_reg,
        offset,
    );
    abi::emit_store_to_address(
        ctx.emitter,
        crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
        object_reg,
        offset + 8,
    );
}
