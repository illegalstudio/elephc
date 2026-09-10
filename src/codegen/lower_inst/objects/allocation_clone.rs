//! Purpose:
//! Emits generic object allocation, property cells, and shallow clone storage.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Heap layout, refcount retention, and dynamic hash cloning remain ABI-stable.

use super::*;

/// Emits allocation, class-id stamping, and declared-property slot initialization.
pub(super) fn emit_object_allocation(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    property_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: &[usize],
    owned_reference_property_offsets: &[usize],
) -> Result<()> {
    emit_object_allocation_with_hidden_slots(
        ctx,
        class_id,
        property_count,
        0,
        allow_dynamic_properties,
        uninitialized_marker_offsets,
        owned_reference_property_offsets,
    )
}

/// Allocates one named class, reserving native-wrapper metadata outside PHP-visible slots.
pub(super) fn emit_named_class_object_allocation(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    class_id: u64,
    property_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: &[usize],
    owned_reference_property_offsets: &[usize],
) -> Result<()> {
    emit_object_allocation_with_hidden_slots(
        ctx,
        class_id,
        property_count,
        crate::internal_extensions::hidden_slot_count_for(
            &ctx.module.class_infos,
            class_name.trim_start_matches('\\'),
        ),
        allow_dynamic_properties,
        uninitialized_marker_offsets,
        owned_reference_property_offsets,
    )
}

/// Emits an ordinary object allocation with compiler-hidden trailing slots before dynamic storage.
pub(super) fn emit_object_allocation_with_hidden_slots(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    property_count: usize,
    hidden_slot_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: &[usize],
    owned_reference_property_offsets: &[usize],
) -> Result<()> {
    let storage_slot_count = property_count + hidden_slot_count;
    let dynamic_properties_offset = dynamic_property_hash_offset(storage_slot_count);
    let dynamic_properties_bytes = if allow_dynamic_properties { 8 } else { 0 };
    let payload_size = dynamic_properties_offset + dynamic_properties_bytes;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov x0, #{}", payload_size)); // request object payload storage for the class id and property slots
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #4");                              // heap kind 4 marks object instances for ownership helpers
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the object payload
            ctx.emitter.instruction("bl __rt_object_handle_acquire");           // bind the new object to its PHP object handle
            ctx.emitter.instruction(&format!("mov x10, #{}", class_id));        // materialize the compile-time class id
            ctx.emitter.instruction("str x10, [x0]");                           // store the class id at object payload offset zero
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov rax, {}", payload_size)); // request object payload storage for the class id and property slots
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(4)
            ));                                                                 // materialize the x86_64 object heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the object payload
            ctx.emitter.instruction("call __rt_object_handle_acquire");         // bind the new object to its PHP object handle
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the compile-time class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the class id at object payload offset zero
        }
    }
    let object_reg = abi::int_result_reg(ctx.emitter);
    for index in 0..storage_slot_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset);
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset + 8);
    }
    if !uninitialized_marker_offsets.is_empty() {
        let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_int_immediate(
            ctx.emitter,
            marker_reg,
            UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
        );
        for offset in uninitialized_marker_offsets {
            abi::emit_store_to_address(ctx.emitter, marker_reg, object_reg, *offset);
        }
    }
    for offset in owned_reference_property_offsets {
        emit_owned_reference_property_cell(ctx, object_reg, *offset);
    }
    if allow_dynamic_properties {
        emit_dynamic_property_hash_init(ctx, object_reg, dynamic_properties_offset);
    }
    Ok(())
}

/// Allocates a zero-initialized 16-byte reference cell for an object-owned reference
/// property and stores the cell pointer in the property slot. The cell must exist from
/// construction so every (deref) access to the reference property reads a valid pointer;
/// the property default (if any) is written through the cell by `emit_property_default`.
pub(super) fn emit_owned_reference_property_cell(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    offset: usize,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 16);                  // 16-byte ref cell: value at +0, tag/len at +8
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    abi::emit_store_zero_to_address(ctx.emitter, result_reg, 0);                // zero the cell value word
    abi::emit_store_zero_to_address(ctx.emitter, result_reg, 8);               // zero the cell tag/length word
    abi::emit_reg_move(ctx.emitter, cell_reg, result_reg);                      // preserve the cell pointer across the object restore
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, cell_reg, object_reg, offset);      // store the cell pointer in the reference-property slot
}

/// Returns the byte offset of the dynamic-property hash pointer for this layout.
pub(super) fn dynamic_property_hash_offset(property_count: usize) -> usize {
    8 + property_count * 16
}

/// Returns property slot offsets whose copied low word must be retained for the cloned owner.
pub(super) fn cloned_property_retain_offsets(class_info: &ClassInfo) -> Vec<usize> {
    class_info
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, (property, php_type))| {
            if class_info.property_slot_is_reference(index, property) {
                return None;
            }
            property_clone_needs_retain(php_type).then_some(8 + index * 16)
        })
        .collect()
}

/// Returns true when a property slot's low word owns heap storage after a shallow copy.
pub(super) fn property_clone_needs_retain(php_type: &PhpType) -> bool {
    let php_type = php_type.codegen_repr();
    matches!(php_type, PhpType::Str) || php_type.is_refcounted()
}

/// Copies declared 16-byte property slots and retains heap-backed child payloads.
pub(super) fn emit_clone_declared_property_slots(
    ctx: &mut FunctionContext<'_>,
    source_reg: &str,
    dest_reg: &str,
    property_count: usize,
    retained_offsets: &[usize],
) {
    for index in 0..property_count {
        let offset = 8 + index * 16;
        emit_copy_property_slot(ctx, source_reg, dest_reg, offset);
        if retained_offsets.contains(&offset) {
            emit_retain_cloned_property_pointer(ctx, source_reg, dest_reg, offset);
        }
    }
}

/// Copies one 16-byte declared-property slot from the source object to the clone.
pub(super) fn emit_copy_property_slot(
    ctx: &mut FunctionContext<'_>,
    source_reg: &str,
    dest_reg: &str,
    offset: usize,
) {
    let low_reg = abi::int_result_reg(ctx.emitter);
    let high_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, low_reg, source_reg, offset);
    abi::emit_load_from_address(ctx.emitter, high_reg, source_reg, offset + 8);
    abi::emit_store_to_address(ctx.emitter, low_reg, dest_reg, offset);
    abi::emit_store_to_address(ctx.emitter, high_reg, dest_reg, offset + 8);
}

/// Retains the copied low-word pointer for string, array, hash, object, or Mixed slots.
pub(super) fn emit_retain_cloned_property_pointer(
    ctx: &mut FunctionContext<'_>,
    source_reg: &str,
    dest_reg: &str,
    offset: usize,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, source_reg);
    abi::emit_push_reg(ctx.emitter, dest_reg);
    abi::emit_load_from_address(ctx.emitter, result_reg, dest_reg, offset);
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_pop_reg(ctx.emitter, dest_reg);
    abi::emit_pop_reg(ctx.emitter, source_reg);
}

/// Replaces the constructor-seeded dynamic-property hash with a shallow clone of the source hash.
pub(super) fn emit_clone_dynamic_property_hash(
    ctx: &mut FunctionContext<'_>,
    source_reg: &str,
    dest_reg: &str,
    offset: usize,
) {
    emit_release_existing_dynamic_property_hash(ctx, source_reg, dest_reg, offset);
    let null_label = ctx.next_label("object_clone_dyn_props_null");
    let done_label = ctx.next_label("object_clone_dyn_props_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, source_reg, offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", result_reg, null_label)); // missing dynamic-property hash clones as a null hash pointer
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", result_reg, result_reg)); // check whether the source dynamic-property hash exists
            ctx.emitter.instruction(&format!("jz {}", null_label));             // missing dynamic-property hash clones as a null hash pointer
        }
    }
    abi::emit_push_reg(ctx.emitter, source_reg);
    abi::emit_push_reg(ctx.emitter, dest_reg);
    let hash_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if hash_arg != result_reg {
        abi::emit_reg_move(ctx.emitter, hash_arg, result_reg);
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_clone_shallow");
    abi::emit_pop_reg(ctx.emitter, dest_reg);
    abi::emit_pop_reg(ctx.emitter, source_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, dest_reg, offset);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    abi::emit_store_zero_to_address(ctx.emitter, dest_reg, offset);
    ctx.emitter.label(&done_label);
}

/// Releases the empty hash allocated while constructing the clone shell.
pub(super) fn emit_release_existing_dynamic_property_hash(
    ctx: &mut FunctionContext<'_>,
    source_reg: &str,
    dest_reg: &str,
    offset: usize,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, source_reg);
    abi::emit_push_reg(ctx.emitter, dest_reg);
    abi::emit_load_from_address(ctx.emitter, result_reg, dest_reg, offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    abi::emit_pop_reg(ctx.emitter, dest_reg);
    abi::emit_pop_reg(ctx.emitter, source_reg);
}

/// Allocates the per-object dynamic-property hash and stores it in the object payload.
pub(super) fn emit_dynamic_property_hash_init(ctx: &mut FunctionContext<'_>, object_reg: &str, offset: usize) {
    let hash_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, object_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 4);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x1",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction(&format!("mov {}, x0", hash_reg));          // preserve the dynamic-property hash across object restore
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 4);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "rsi",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction(&format!("mov {}, rax", hash_reg));         // preserve the dynamic-property hash across object restore
        }
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, hash_reg, object_reg, offset);
}
