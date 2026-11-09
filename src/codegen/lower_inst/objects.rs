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

// Matches `SharedCodegenState`'s emitted-method sets, which this module borrows; see there.
use crate::fast_hash::FastSet as HashSet;

use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::codegen_support::sentinels::THROWABLE_CREATION_LINE_OFFSET;
use crate::codegen::{
    abi, callable_descriptor, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed, runtime_value_tag,
};
use crate::intrinsics::IntrinsicCall;
use crate::ir::{
    Immediate, Instruction, LocalSlotId, Op, PropertyFetchMode, ValueDef, ValueId,
};
use crate::codegen_support::dynamic_new::known_dynamic_new_builtin_class_names;
use crate::names::{label_fragment, method_symbol, php_symbol_key};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, InterfaceInfo, PhpType, ScalarWidth};

use super::super::context::FunctionContext;
use super::{
    box_method_result_as_mixed, builtins, callables, cast_loaded_mixed_pointer_to_result,
    direct_call_stack_pad_bytes,
    emit_call_arg_temp_cleanups, emit_resolved_method_call, expect_data,
    coerce_loaded_value_to_tagged_scalar, emit_instance_method_descriptor_entry_wrapper,
    emit_loaded_assoc_array_to_mixed,
    emit_loaded_indexed_array_to_mixed, emit_mixed_string_for_persistent_store,
    emit_ref_arg_writebacks, exceptions, expect_operand, iterators, load_value_to_first_int_arg,
    materialize_method_call_args_with_receiver_reg_and_refs, resolve_method_call_target,
    property_values, store_if_result, store_method_call_result, RefArgCellLifetime,
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

pub(in crate::codegen) use reflection::emit_reflection_class_table;
pub(super) use reflection::{
    lower_refl_at, lower_refl_callable_name, lower_refl_pool_str, lower_refl_row_lookup, lower_refl_word,
    lower_reflection_fill_members,
    lower_reflection_materialize,
};

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
    /// Storage width of a packed field declared with a C width name (`i32`, `u8`, `f32`), whose
    /// `php_type` is the `int` or `float` it reads as; `None` for 8-byte fields.
    width: Option<ScalarWidth>,
    is_reference: bool,
    /// Argument diagnostic prefix when this temporary slot models an inherited setter.
    abstract_setter_type_error_prefix: Option<String>,
}

impl PropertySlot {
    /// Whether the slot holds its container pointer directly, so an in-place mutation can
    /// publish into it with a plain store: declared, not packed, not behind a reference cell.
    pub(super) fn holds_container_directly(&self) -> bool {
        self.is_declared && !self.is_packed && !self.is_reference
    }

    /// The type a store releases the slot's PREVIOUS value as.
    ///
    /// A Throwable's `trace` slot is declared `array`, which is a boxed `Mixed` for stores, but
    /// `__rt_throwable_capture_trace` publishes a RAW array there (the reader and the object free
    /// dispatch on the heap kind for exactly that reason). Releasing the raw array as a box read
    /// its header as a tag and payload: a trace of 12 frames (tag 12, a global reference marker)
    /// faulted in `ReflectionProperty::setValue($e, $trace)`, which is how Symfony's
    /// `ClassExistenceResource::throwOnRequiredClass()` and `ErrorHandler` replace a trace.
    /// A heap-kind dispatching release is correct for both shapes.
    pub(super) fn release_type(&self) -> PhpType {
        if self.property == "trace"
            && self.offset == crate::codegen_support::throwable_layout::TRACE_OFFSET
            && !self.is_packed
            && !self.is_reference
            && self.php_type.codegen_repr() == PhpType::Mixed
        {
            return PhpType::Array(Box::new(PhpType::Mixed));
        }
        self.php_type.clone()
    }
}

/// Declared-property candidate reachable from a `Mixed` object receiver.
#[derive(Clone)]
struct MixedPropertyCandidate {
    class_id: u64,
    slot: PropertySlot,
}

/// One arm of a `Mixed` receiver's property READ dispatch.
///
/// The slot still selects the arm, because the dispatch is on the runtime class id, but a class
/// whose scope may not reach the name answers php's catchable `Error` instead of the storage.
struct MixedPropertyReadCandidate {
    /// Class id and slot the arm matches on. The slot selects the arm even when it is never read.
    candidate: MixedPropertyCandidate,
    /// What the arm does once its class id and name have matched.
    kind: MixedPropertyReadKind,
}

/// What one `Mixed` receiver READ arm does once its class id and property name have matched.
///
/// The arm is selected by the receiver's runtime class id and the name, but only `Slot` reads
/// storage. The other three are php's scope-dependent answers for the same name, and each of them
/// exists precisely so the declared slot is NOT read.
enum MixedPropertyReadKind {
    /// Read the declared slot.
    Slot,
    /// Raise php's catchable access `Error` carrying this verbatim message.
    Refuse(String),
    /// Answer from the per-instance dynamic hash, warning `Undefined property` on a READ miss.
    ScopeDynamic,
    /// The class declares the accessor php consults first. A literal-name value read or `??` calls
    /// `__get` (after `__isset` for `??`); `isset()` still answers php `null` silently, and a
    /// runtime name cannot reach `__get` / `__isset` yet.
    MagicDeferred,
}

/// Resolved object property default metadata for fixed-offset initialization.
#[derive(Clone)]
struct PropertyDefault {
    offset: usize,
    value: LiteralDefaultValue,
    /// `true` when the slot holds a ref-cell pointer (an object-owned reference property);
    /// the default is written THROUGH the cell instead of directly into the slot.
    is_reference: bool,
}

/// Strategy used to initialize a newly allocated object's physical property slots.
enum PropertyInitialization {
    /// Copy backend-supported literal defaults directly into their slots.
    Inline(Vec<PropertyDefault>),
    /// Run the EIR initializer so constant expressions retain their runtime branch semantics.
    Thunk { class_id: u64 },
}

impl PropertyInitialization {
    /// Returns whether initialization requires calling the class's EIR thunk.
    fn uses_thunk(&self) -> bool {
        matches!(self, Self::Thunk { .. })
    }

    /// Literal defaults the allocator may fold into the class's static object template.
    fn inline_defaults(&self) -> Vec<PropertyDefault> {
        match self {
            Self::Inline(defaults) => defaults.clone(),
            Self::Thunk { .. } => Vec::new(),
        }
    }

    /// Replaces the inline defaults with those the allocator left unassigned; a thunk stays.
    fn with_remaining_inline(&self, remaining: Vec<PropertyDefault>) -> Self {
        match self {
            Self::Inline(_) => Self::Inline(remaining),
            Self::Thunk { class_id } => Self::Thunk { class_id: *class_id },
        }
    }
}

/// Concrete class that a dynamic factory can instantiate in this EIR module.
struct DynamicNewCandidate {
    class_name: String,
    class_id: u64,
    property_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: Vec<usize>,
    owned_reference_property_offsets: Vec<(usize, PhpType, bool)>,
    property_initialization: PropertyInitialization,
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
mod reflection_constructor;
// The one stderr-and-exit sequence for the whole lowering tree. Every caller names it rather than
// open-coding the platform split, so `Emitter::syscall`'s macOS-to-Linux mapping has a single site.
pub(super) use dynamic_factory::emit_fatal_message;
mod dynamic_pdo;
mod dynamic_class_flags;
mod property_defaults;
mod known_property_reads;
mod mixed_property_reads;
#[cfg(test)]
pub(crate) use mixed_property_reads::MIXED_READ_SEARCHES;
mod dynamic_property_read_entry;
mod dynamic_property_read_resolution;
mod runtime_property_writes;
mod named_property_writes;
mod instanceof_entry;
mod object_class_id;
mod class_name_id;
mod closure_scope_gate;
mod allocation_clone;
mod interface_layout;
mod property_resolution;
pub(in crate::codegen::lower_inst) mod property_compatibility;
mod property_loads;
mod property_fetch_for_write;
mod property_stores;
mod property_hook_writes;
mod property_store_values;
mod typed_property_guards;
mod instanceof_helpers;
mod mixed_property_type_guard;
mod extern_fields;
pub(super) use mixed_property_type_guard::emit_string_into_int_diagnostics;
mod outlined_dynamic_property;
mod arm_groups;
mod readonly_guard;
mod dynamic_new_table;
pub(in crate::codegen) use dynamic_new_table::emit_too_few_routine as emit_dynamic_new_too_few_routine;

#[allow(unused_imports)]
use fixed_new::*;
#[allow(unused_imports)]
use clone_and_spl::*;
#[allow(unused_imports)]
use iterator_iterator::*;
#[allow(unused_imports)]
use throwable_new::*;
pub(in crate::codegen::lower_inst) use throwable_new::try_lower_builtin_throwable_parent_constructor as lower_builtin_throwable_parent_constructor_call;
#[allow(unused_imports)]
use fiber_dynamic_entry::*;
#[allow(unused_imports)]
use dynamic_mixed_candidates::*;
#[allow(unused_imports)]
use dynamic_factory::*;
#[allow(unused_imports)]
pub(in crate::codegen::lower_inst) use dynamic_pdo::*;
pub(in crate::codegen::lower_inst) use dynamic_class_flags::*;
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
pub(super) use allocation_clone::emit_object_allocation;
#[allow(unused_imports)]
use interface_layout::*;
#[allow(unused_imports)]
use property_resolution::*;
#[allow(unused_imports)]
use property_compatibility::*;
#[allow(unused_imports)]
pub(in crate::codegen::lower_inst) use instanceof_helpers::{
    class_name_immediate, classify_named_target, emit_match_call,
};
#[allow(unused_imports)]
use property_loads::*;
#[allow(unused_imports)]
use property_stores::*;
use property_hook_writes::*;
#[allow(unused_imports)]
use property_store_values::*;
#[allow(unused_imports)]
use typed_property_guards::*;
#[allow(unused_imports)]
use instanceof_helpers::*;
#[allow(unused_imports)]
use mixed_property_type_guard::*;
use extern_fields::*;
pub(in crate::codegen::lower_inst) use mixed_property_type_guard::emit_string_result_into_int_or_jump;
#[allow(unused_imports)]
use outlined_dynamic_property::*;

/// Writes the bodies of every shared property helper the function just emitted asked for: the
/// typed-property guards and the runtime-name ladders. A body may ask for another helper, so the
/// two drains alternate until neither has anything left.
pub(in crate::codegen) fn emit_pending_property_guard_helpers(
    module: &crate::ir::Module,
    emitter: &mut crate::codegen_support::emit::Emitter,
    data: &mut crate::codegen::data_section::DataSection,
    shared: &mut crate::codegen::shared_state::SharedCodegenState,
) -> Result<()> {
    loop {
        mixed_property_type_guard::emit_pending_property_guard_helpers(module, emitter, data, shared)?;
        if !outlined_dynamic_property::emit_pending_dynamic_property_helpers(
            module, emitter, data, shared,
        )? {
            return Ok(());
        }
    }
}
pub(super) use dynamic_property_read_entry::{lower_dynamic_prop_get, lower_nullsafe_prop_get};
pub(super) use dynamic_property_read_resolution::lower_dynamic_prop_ref_cell;
pub(super) use fiber_dynamic_entry::{
    lower_dynamic_object_new, lower_dynamic_object_new_mixed,
    lower_dynamic_object_new_without_constructor_mixed,
};
pub(super) use fixed_new::lower_object_new;
pub(super) use instanceof_entry::{lower_instanceof, lower_instanceof_dynamic};
pub(super) use object_class_id::lower_object_class_id;
pub(super) use class_name_id::lower_class_name_to_id;
pub(in crate::codegen::lower_inst) use closure_scope_gate::emit_closure_scope_gate;
pub(in crate::codegen::lower_inst) use class_name_id::emit_class_id_of_value;
pub(super) use known_property_reads::{
    lower_load_prop_ref_cell, lower_load_prop_ref_cell_checked, lower_prop_get,
    lower_prop_initialized,
};
pub(super) use property_stores::lower_bind_prop_ref_cell;
pub(super) use property_fetch_for_write::lower_prop_get_for_write;
pub(super) use mixed_property_type_guard::{BOOL_TAG, OBJECT_TAG, RUNTIME_TAGS};
pub(super) use property_store_values::{lower_packed_field_mixed_to_int, release_adopted_mixed_source};
pub(super) use property_resolution::{
    emit_boxed_null, emit_nullable_receiver_object_payload, nullable_object_receiver_class,
    raw_value_php_type,
};
pub(super) use runtime_property_writes::{
    lower_dynamic_prop_set, lower_dynamic_prop_unset_runtime, lower_prop_set, lower_prop_unset,
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

/// Publishes a container mutated in place through a borrowed property read.
pub(super) fn store_in_place_container_property_owner(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    slot: &PropertySlot,
    value: ValueId,
) -> Result<()> {
    store_in_place_container_property(ctx, object, slot, value)
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
/// `hash_offset`, offset 8 for `stdClass`, just past the fixed slots for an
/// `#[AllowDynamicProperties]` class. The unique table is installed before `__rt_hash_unset`
/// detaches the entry and releases its value. A destructor may replace the property table
/// or throw, so no receiver storage is touched after the release helper returns.
/// Removing an absent key is a no-op inside the helper.
///
/// The receiver register is caller-saved, so it is parked on the temporary stack across
/// the COW call and reloaded before the unique table pointer is stored back.
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
            abi::emit_call_label(ctx.emitter, "__rt_hash_ensure_unique");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "x0", object_reg, hash_offset);
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(                                   // load the receiver table before publishing a unique replacement
                "mov rdi, QWORD PTR [{} + {}]",
                object_reg, hash_offset
            ));                                                                 // load the dynamic-property hash pointer from the receiver
            abi::emit_call_label(ctx.emitter, "__rt_hash_ensure_unique");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, hash_offset);
            abi::emit_reg_move(ctx.emitter, "rdi", "rax");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
        }
    }
    Ok(())
}

/// Names the reason a resolved fixed property slot cannot represent PHP's "removed"
/// state, or `None` when the uninitialized marker is a faithful encoding for it.
///
/// A by-reference slot is supported: the removal detaches its cell and installs a fresh
/// uninitialized one (`detach_and_release_unset_property_value`).
fn unset_unsupported_slot_reason(
    ctx: &FunctionContext<'_>,
    slot: &PropertySlot,
) -> Option<&'static str> {
    if slot.is_packed {
        return Some("packed class field");
    }
    // An object-OWNED reference slot (`$x = &$obj->p` promoted it) is broken and re-armed with a
    // fresh cell by `detach_and_release_unset_property_value`. A BORROWED one -- a promoted
    // `&$param` -- holds the caller's cell, possibly the address of the caller's frame slot, which
    // that release would treat as a heap cell: `unset($r->p)` decremented a "header" that was the
    // neighbouring local and the program crashed on its next use.
    if slot.is_reference
        && !ctx
            .module
            .class_infos
            .get(&slot.class_name)
            .is_some_and(|info| info.owned_reference_properties.contains(&slot.property))
    {
        return Some("by-reference property");
    }
    if !slot.is_declared && !slot_supports_untyped_unset_marker(slot) {
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

/// Lowers the object-handle reservation ops around a `new` whose arguments may allocate:
/// each is one call to a register-preserving runtime helper.
pub(super) fn lower_object_handle_reservation(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    match inst.op {
        Op::ObjectHandleReserve => {
            abi::emit_call_label(ctx.emitter, "__rt_object_handle_reserve");
            match inst.result {
                Some(result) => ctx.store_result_value(result),
                None => Ok(()),
            }
        }
        Op::ObjectHandlePend => {
            let handle = *inst
                .operands
                .first()
                .ok_or_else(|| CodegenIrError::invalid_module("object_handle_pend needs a handle"))?;
            ctx.load_value_to_result(handle)?;
            abi::emit_call_label(ctx.emitter, "__rt_object_handle_pend");
            Ok(())
        }
        _ => {
            abi::emit_call_label(ctx.emitter, "__rt_object_handle_unpend");
            Ok(())
        }
    }
}
