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
use crate::names::{label_fragment, method_symbol, php_symbol_key};
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
    materialize_method_call_args_with_receiver_reg_and_refs, resolve_method_call_target,
    mixed_simplexml_candidates, MixedSimpleXmlCandidate,
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
mod iterator_iterator;
pub(in crate::codegen::lower_inst) mod throwable_new;
mod fiber_dynamic_entry;
mod dynamic_mixed_candidates;
mod dynamic_factory;
mod dynamic_pdo;
mod property_defaults;
mod known_property_reads;
mod mixed_property_reads;
mod dynamic_property_read_entry;
mod dynamic_property_read_resolution;
mod runtime_property_writes;
mod named_property_writes;
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
use dynamic_property_read_entry::*;
#[allow(unused_imports)]
use dynamic_property_read_resolution::*;
#[allow(unused_imports)]
use runtime_property_writes::*;
#[allow(unused_imports)]
use named_property_writes::*;
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
pub(super) use fixed_new::lower_object_new;
pub(super) use instanceof_entry::{lower_instanceof, lower_instanceof_dynamic};
pub(super) use known_property_reads::{
    lower_load_prop_ref_cell, lower_prop_get, lower_prop_initialized,
};
pub(super) use property_fetch_for_write::lower_prop_get_for_write;
pub(super) use property_resolution::{
    emit_boxed_null, emit_nullable_receiver_object_payload, nullable_object_receiver_class,
    raw_value_php_type,
};
pub(super) use runtime_property_writes::{
    lower_dynamic_prop_set, lower_prop_set, lower_prop_unset,
};
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

/// Allocates one ordinary compiler-owned internal value object with declared slots only.
pub(super) fn emit_internal_value_object_allocation(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    if !crate::internal_extensions::is_native_value_object_class(normalized) {
        return Err(CodegenIrError::unsupported(format!(
            "internal-extension value-object class {}",
            normalized
        )));
    }
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    let class_id = class_info.class_id;
    let property_count = class_info.properties.len();
    let allow_dynamic_properties = class_info.allow_dynamic_properties;
    let uninitialized_marker_offsets = uninitialized_property_marker_offsets(class_info);
    let owned_reference_property_offsets = owned_reference_property_offsets(class_info);
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        allow_dynamic_properties,
        &uninitialized_marker_offsets,
        &owned_reference_property_offsets,
    )
}

/// Allocates one native wrapper value without storing it into an EIR result slot.
pub(super) fn emit_internal_extension_wrapper_value(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    context_reg: &str,
    handle_reg: &str,
) -> Result<()> {
    emit_internal_extension_wrapper_value_impl(ctx, class_name, context_reg, handle_reg)
}

/// Allocates a bridge-selected registered descendant while preserving native wrapper state.
pub(super) fn emit_registered_internal_extension_wrapper_value(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    context_reg: &str,
    handle_reg: &str,
) -> Result<()> {
    emit_internal_extension_wrapper_value_impl(ctx, class_name, context_reg, handle_reg)
}

/// Materializes one native wrapper or verified userland descendant with native state.
fn emit_internal_extension_wrapper_value_impl(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    context_reg: &str,
    handle_reg: &str,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    let wrapper_supported = crate::internal_extensions::is_native_wrapper_class(normalized)
        || crate::internal_extensions::is_native_wrapper_descendant(
            &ctx.module.class_infos,
            normalized,
        );
    if !wrapper_supported {
        return Err(CodegenIrError::unsupported(format!(
            "internal-extension wrapper class {}",
            normalized
        )));
    }
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    let class_id = class_info.class_id;
    let property_count = class_info.properties.len();
    let allow_dynamic_properties = class_info.allow_dynamic_properties;
    let uninitialized_marker_offsets = uninitialized_property_marker_offsets(class_info);
    let owned_reference_property_offsets = owned_reference_property_offsets(class_info);
    let hidden_base = dynamic_property_hash_offset(property_count);
    let cache_miss = ctx.next_label("dom_wrapper_cache_miss");
    let cache_done = ctx.next_label("dom_wrapper_cache_done");
    let context_restore = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let handle_restore = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    let context_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let handle_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);

    abi::emit_push_reg_pair(ctx.emitter, context_reg, handle_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, context_arg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, handle_arg, 8);
    abi::emit_call_label(ctx.emitter, "__rt_dom_wrapper_cache_get");
    abi::emit_pop_reg_pair(ctx.emitter, &context_restore, &handle_restore);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &cache_miss);
    abi::emit_jump(ctx.emitter, &cache_done);

    ctx.emitter.label(&cache_miss);
    abi::emit_push_reg(ctx.emitter, &context_restore);
    abi::emit_push_reg(ctx.emitter, &handle_restore);
    emit_object_allocation_with_hidden_slots(
        ctx,
        class_id,
        property_count,
        crate::internal_extensions::hidden_slot_count_for(
            &ctx.module.class_infos,
            normalized,
        ),
        allow_dynamic_properties,
        &uninitialized_marker_offsets,
        &owned_reference_property_offsets,
    )?;
    let object_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_pop_reg(ctx.emitter, &handle_restore);
    abi::emit_pop_reg(ctx.emitter, &context_restore);

    abi::emit_store_to_address(ctx.emitter, &context_restore, &object_reg, hidden_base + 16);
    abi::emit_store_to_address(ctx.emitter, &handle_restore, &object_reg, hidden_base + 32);
    abi::emit_load_int_immediate(ctx.emitter, &handle_restore, 1);
    abi::emit_store_to_address(ctx.emitter, &handle_restore, &object_reg, hidden_base);
    abi::emit_load_int_immediate(ctx.emitter, &handle_restore, class_id as i64);
    abi::emit_store_to_address(ctx.emitter, &handle_restore, &object_reg, hidden_base + 8);
    abi::emit_store_to_address(ctx.emitter, &handle_restore, &object_reg, hidden_base + 48);
    abi::emit_store_zero_to_address(
        ctx.emitter,
        &object_reg,
        hidden_base + crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET,
    );
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_reg_move(ctx.emitter, object_arg, &object_reg);
    abi::emit_load_from_address(ctx.emitter, context_arg, object_arg, hidden_base + 16);
    abi::emit_load_from_address(ctx.emitter, handle_arg, object_arg, hidden_base + 32);
    abi::emit_call_label(ctx.emitter, "__rt_dom_wrapper_cache_put");
    ctx.emitter.label(&cache_done);
    Ok(())
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
