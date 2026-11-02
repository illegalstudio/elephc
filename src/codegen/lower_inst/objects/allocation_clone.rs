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
pub(in crate::codegen::lower_inst) fn emit_object_allocation(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    property_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: &[usize],
    owned_reference_property_offsets: &[(usize, PhpType, bool)],
) -> Result<()> {
    let dynamic_properties_offset = dynamic_property_hash_offset(property_count);
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
            ctx.emitter.instruction(&format!(                                   // materialize the x86_64 object heap kind word
                "mov r10, 0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(4)
            ));
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the object payload
            ctx.emitter.instruction("call __rt_object_handle_acquire");         // bind the new object to its PHP object handle
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the compile-time class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the class id at object payload offset zero
        }
    }
    let object_reg = abi::int_result_reg(ctx.emitter);
    for index in 0..property_count {
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
    for (offset, php_type, starts_uninitialized) in owned_reference_property_offsets {
        emit_owned_reference_property_cell(
            ctx,
            object_reg,
            *offset,
            php_type,
            *starts_uninitialized,
        );
    }
    if allow_dynamic_properties {
        emit_dynamic_property_hash_init(ctx, object_reg, dynamic_properties_offset);
    }
    Ok(())
}

/// The two image words a literal default writes into its slot, when the default needs no
/// allocation and can therefore live in the class's static allocation template.
fn template_slot_words(
    ctx: &mut FunctionContext<'_>,
    value: &LiteralDefaultValue,
) -> Option<[crate::codegen_support::data_section::DataWord; 2]> {
    use crate::codegen_support::data_section::DataWord;
    Some(match value {
        LiteralDefaultValue::Int(v) | LiteralDefaultValue::TaggedInt(v) => [DataWord::U64(*v as u64), DataWord::U64(0)],
        LiteralDefaultValue::Bool(v) => [DataWord::U64(u64::from(*v)), DataWord::U64(0)],
        LiteralDefaultValue::Float(v) => [DataWord::U64(v.to_bits()), DataWord::U64(0)],
        LiteralDefaultValue::Null => [DataWord::U64(0), DataWord::U64(0)],
        LiteralDefaultValue::NullSentinel => [DataWord::U64(RUNTIME_NULL_SENTINEL as u64), DataWord::U64(0)],
        LiteralDefaultValue::TaggedNull => [
            DataWord::U64(crate::codegen_support::sentinels::NULL_SENTINEL as u64),
            DataWord::U64(crate::codegen_support::sentinels::TAGGED_SCALAR_TAG_NULL as u64),
        ],
        LiteralDefaultValue::Str(text) => {
            let (label, len) = ctx.data.add_string(&crate::string_bytes::literal_bytes(text));
            [DataWord::Symbol(label), DataWord::U64(len as u64)]
        }
        _ => return None,
    })
}

/// Splits a class's literal defaults into the ones its allocation template carries and the rest.
///
/// The rest (arrays, enum cases, boxed values, the property-init thunk marker) allocate or run
/// code, so they are assigned by the class's `_class_propinit_<id>` thunk instead.
pub(super) fn partition_template_defaults(
    ctx: &mut FunctionContext<'_>,
    defaults: &[PropertyDefault],
) -> (
    Vec<(usize, [crate::codegen_support::data_section::DataWord; 2])>,
    Vec<PropertyDefault>,
) {
    let mut folded = Vec::new();
    let mut rest = Vec::new();
    for default in defaults {
        match template_slot_words(ctx, &default.value) {
            Some(words) if !default.is_reference => folded.push((default.offset, words)),
            _ => rest.push(default.clone()),
        }
    }
    (folded, rest)
}

/// Allocates one instance of `class_name` and returns the property defaults still to assign.
///
/// A class with no per-object cells and no hidden ancestor slots is built from its static
/// template by one shared routine (`__rt_object_new_template`), and whatever the template cannot
/// hold is left to the class's `_class_propinit_<id>` thunk; the rest keep the inline sequence.
/// Shared by the fixed-class `new` and every arm of the dynamic-new ladders: those arms used the
/// inline sequence -- one store per slot and default, about 40 instructions per class per ladder
/// -- in each of the dynamic-new helpers of a Symfony build.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_class_instance_allocation(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    class_id: u64,
    property_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: &[usize],
    owned_reference_property_offsets: &[(usize, PhpType, bool)],
    property_defaults: Vec<PropertyDefault>,
) -> Result<Vec<PropertyDefault>> {
    let has_hidden_slot = ctx
        .module
        .class_infos
        .get(class_name)
        .is_some_and(|info| (0..info.properties.len()).any(|index| info.property_slot_is_hidden(index)));
    // A Throwable's payload slots (message, previous, trace) are initialized and released by the
    // exception machinery in an order the inline sequence pins, so those classes keep it.
    let throwable_like = super::super::is_throwable_like_class(ctx, class_name);
    let from_template = owned_reference_property_offsets.is_empty()
        && !has_hidden_slot
        && std::env::var_os("ELEPHC_NO_OBJECT_TEMPLATE").is_none();
    if std::env::var_os("ELEPHC_OP_STATS").is_some() && !from_template {
        eprintln!(
            "NEWFALLBACK {} owned_refs={} hidden={}",
            class_name,
            !owned_reference_property_offsets.is_empty(),
            has_hidden_slot
        );
    }
    if !from_template {
        emit_object_allocation(
            ctx,
            class_id,
            property_count,
            allow_dynamic_properties,
            uninitialized_marker_offsets,
            owned_reference_property_offsets,
        )?;
        return Ok(property_defaults);
    }
    let (folded, rest) = partition_template_defaults(ctx, &property_defaults);
    emit_object_allocation_from_template(
        ctx,
        class_id,
        property_count,
        allow_dynamic_properties,
        uninitialized_marker_offsets,
        &folded,
    );
    Ok(if rest.is_empty() {
        Vec::new()
    } else if throwable_like {
        // A Throwable's payload defaults (the empty trace, the boxed null `previous`) are
        // released by the exception machinery in the order the inline stores pin, so they
        // stay inline; every other class hands its allocating defaults to the init thunk.
        rest
    } else {
        vec![PropertyDefault {
            offset: 0,
            value: LiteralDefaultValue::InitThunk { class_id },
            is_reference: false,
        }]
    })
}

/// Allocates and initializes an object with ONE call into `__rt_object_new_template`.
///
/// The template is the class's static image: class id, zeroed slots, typed-uninitialized markers
/// and the literal defaults that need no allocation. The caller handles everything else.
pub(super) fn emit_object_allocation_from_template(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    property_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: &[usize],
    folded_defaults: &[(usize, [crate::codegen_support::data_section::DataWord; 2])],
) {
    let template = object_template_label(
        ctx,
        class_id,
        property_count,
        allow_dynamic_properties,
        uninitialized_marker_offsets,
        folded_defaults,
    );
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), &template);
    abi::emit_call_label(ctx.emitter, "__rt_object_new_template");
    if allow_dynamic_properties {
        let object_reg = abi::int_result_reg(ctx.emitter);
        emit_dynamic_property_hash_init(ctx, object_reg, dynamic_property_hash_offset(property_count));
    }
}

/// Builds (or finds) the static allocation template of a class and returns its data label.
pub(super) fn object_template_label(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    property_count: usize,
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: &[usize],
    folded_defaults: &[(usize, [crate::codegen_support::data_section::DataWord; 2])],
) -> String {
    use crate::codegen_support::data_section::DataWord;
    // A class with a property hash keeps one more word after the slots; the template zeroes it
    // and the site creates the hash.
    let hash_bytes = if allow_dynamic_properties { 8 } else { 0 };
    let payload_size = dynamic_property_hash_offset(property_count) + hash_bytes;
    let mut words = vec![DataWord::U64(payload_size as u64), DataWord::U64(class_id)];
    words.extend((0..(payload_size - 8) / 8).map(|_| DataWord::U64(0)));
    // `words[1 + n]` is the payload word at byte offset `8 * n`.
    for offset in uninitialized_marker_offsets {
        words[1 + offset / 8] = DataWord::U64(UNINITIALIZED_TYPED_PROPERTY_SENTINEL as u64);
    }
    for (offset, pair) in folded_defaults {
        words[1 + offset / 8] = pair[0].clone();
        words[1 + offset / 8 + 1] = pair[1].clone();
    }
    ctx.data.add_words(words)
}

/// Allocates a zero-initialized 16-byte reference cell for an object-owned reference
/// property and stores the cell pointer in the property slot. The cell must exist from
/// construction so every (deref) access to the reference property reads a valid pointer;
/// the property default (if any) is written through the cell by `emit_property_default`.
pub(super) fn emit_owned_reference_property_cell(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    offset: usize,
    php_type: &PhpType,
    starts_uninitialized: bool,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let cell_reg = if object_reg == abi::secondary_scratch_reg(ctx.emitter) {
        abi::tertiary_scratch_reg(ctx.emitter)
    } else {
        abi::secondary_scratch_reg(ctx.emitter)
    };
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0),
        crate::codegen_support::runtime::reference_cells::payload_tag(php_type));
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_new");
    if starts_uninitialized {
        let marker_reg = abi::tertiary_scratch_reg(ctx.emitter);
        abi::emit_load_int_immediate(
            ctx.emitter,
            marker_reg,
            UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
        );
        abi::emit_store_to_address(ctx.emitter, marker_reg, result_reg, 0);     // mark the owned reference payload as typed-uninitialized
    } else {
        abi::emit_store_zero_to_address(ctx.emitter, result_reg, 0);            // zero the cell value word
    }
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

/// Returns property slot offsets whose copied string payload must be COPIED for the clone.
///
/// A string is the one owned property payload that carries no refcount: a `string` slot owns an
/// independent `__rt_str_persist` block, which is why every store into one persists and every
/// release frees outright. `__rt_incref` on such a block therefore retained NOTHING, so a clone
/// and its source shared one block and the source's next assignment freed it under the clone —
/// `foreach ($chunks as $c) { $s->string = $c; $out[] = clone $s; }` printed the LAST chunk for
/// every element, and 2-byte garbage once the block had been reused. Under `--heap-debug` the
/// same program stopped at "bad refcount", the incref writing a refcount word a string block
/// does not have. An `array` property in the same class survived, which is what made the defect
/// look string-specific: arrays ARE refcounted, so their incref was real.
pub(super) fn cloned_property_persist_offsets(class_info: &ClassInfo) -> Vec<usize> {
    class_info
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, (property, php_type))| {
            if class_info.property_slot_is_reference(index, property) {
                return None;
            }
            (php_type.codegen_repr() == PhpType::Str).then_some(8 + index * 16)
        })
        .collect()
}

/// Returns true when a property slot's low word owns heap storage after a shallow copy.
///
/// A `callable` / `Closure` slot holds a refcounted descriptor too: object teardown releases it
/// (`__rt_callable_descriptor_release`), so the clone needs its own share. Without one, Symfony's
/// `ServiceLocator::withContext()` (`clone $this`) left the clone pointing at the factory closure
/// the discarded original then freed, and every later `get()` jumped into freed memory.
pub(super) fn property_clone_needs_retain(php_type: &PhpType) -> bool {
    let repr = php_type.codegen_repr();
    repr.is_refcounted() || repr == PhpType::Callable || php_type.is_closure_object()
}

/// Copies declared 16-byte property slots and gives the clone its own heap-backed payloads.
pub(super) fn emit_clone_declared_property_slots(
    ctx: &mut FunctionContext<'_>,
    source_reg: &str,
    dest_reg: &str,
    property_count: usize,
    retained_offsets: &[usize],
    persisted_offsets: &[usize],
    owned_references: &[(usize, PhpType, bool)],
) {
    for index in 0..property_count {
        let offset = 8 + index * 16;
        emit_copy_property_slot(ctx, source_reg, dest_reg, offset);
        if owned_references
            .iter()
            .any(|(owned_offset, _, _)| *owned_offset == offset)
        {
            let result = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, source_reg);
            abi::emit_push_reg(ctx.emitter, dest_reg);
            abi::emit_load_from_address(ctx.emitter, result, dest_reg, offset);
            abi::emit_call_label(ctx.emitter, "__rt_reference_cell_clone");
            abi::emit_pop_reg(ctx.emitter, dest_reg);
            abi::emit_pop_reg(ctx.emitter, source_reg);
            abi::emit_store_to_address(ctx.emitter, result, dest_reg, offset);
        } else if retained_offsets.contains(&offset) {
            emit_retain_cloned_property_pointer(ctx, source_reg, dest_reg, offset);
        }
        if persisted_offsets.contains(&offset) {
            emit_persist_cloned_property_string(ctx, source_reg, dest_reg, offset);
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

/// Retains the copied low-word pointer for array, hash, object, or Mixed slots.
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

/// Gives a cloned `string` slot its OWN heap block instead of the source object's.
///
/// String payloads carry no refcount, so the shallow copy left both objects pointing at one
/// `__rt_str_persist` block that only one of them could free. `__rt_str_persist` re-run on the
/// copied pointer/length pair produces the independent block the clone must own, exactly what a
/// store into a `string` property already does. The scratch registers the copy loop holds the
/// source and destination objects in (`x10`/`x9`, `r10`/`r11`) are not the helper's pointer and
/// length registers, so restoring them cannot clobber the fresh copy.
///
/// A DECLARED-but-unassigned typed property carries `UNINITIALIZED_TYPED_PROPERTY_SENTINEL` in
/// the slot's high word, where an initialized string keeps its length. Copying that pair through
/// the persist helper would read `0x7fff_ffff_ffff_fffd` bytes, so the marker is left exactly as
/// the shallow copy placed it — `clone` has to preserve "still uninitialized", which is what
/// `isset($this->path)` reads afterwards. The comparison is repeated after the register restores
/// because the helper call clobbers the flags; `pop`/`ldp` do not, and the pointer and length
/// registers survive both restores, so the second compare sees the same operands as the first.
pub(super) fn emit_persist_cloned_property_string(
    ctx: &mut FunctionContext<'_>,
    source_reg: &str,
    dest_reg: &str,
    offset: usize,
) {
    let skip_label = ctx.next_label("object_clone_string_uninitialized");
    let done_label = ctx.next_label("object_clone_string_persisted");
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, source_reg);
    abi::emit_push_reg(ctx.emitter, dest_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_from_address(ctx.emitter, "x1", dest_reg, offset);
            abi::emit_load_from_address(ctx.emitter, "x2", dest_reg, offset + 8);
            abi::emit_load_int_immediate(
                ctx.emitter,
                sentinel_reg,
                UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
            );
            ctx.emitter
                .instruction(&format!("cmp x2, {}", sentinel_reg));             // is the copied slot still the uninitialized typed-property marker?
            ctx.emitter
                .instruction(&format!("b.eq {}", skip_label));                  // an uninitialized slot owns no string block to copy
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.label(&skip_label);
            abi::emit_pop_reg(ctx.emitter, dest_reg);
            abi::emit_pop_reg(ctx.emitter, source_reg);
            abi::emit_load_int_immediate(
                ctx.emitter,
                sentinel_reg,
                UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
            );
            ctx.emitter
                .instruction(&format!("cmp x2, {}", sentinel_reg));             // re-test the marker now that the call has clobbered the flags
            ctx.emitter
                .instruction(&format!("b.eq {}", done_label));                  // leave the uninitialized marker exactly as the shallow copy placed it
            abi::emit_store_to_address(ctx.emitter, "x1", dest_reg, offset);
            abi::emit_store_to_address(ctx.emitter, "x2", dest_reg, offset + 8);
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_load_from_address(ctx.emitter, "rax", dest_reg, offset);
            abi::emit_load_from_address(ctx.emitter, "rdx", dest_reg, offset + 8);
            abi::emit_load_int_immediate(
                ctx.emitter,
                sentinel_reg,
                UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
            );
            ctx.emitter
                .instruction(&format!("cmp rdx, {}", sentinel_reg));            // is the copied slot still the uninitialized typed-property marker?
            ctx.emitter
                .instruction(&format!("je {}", skip_label));                    // an uninitialized slot owns no string block to copy
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.label(&skip_label);
            abi::emit_pop_reg(ctx.emitter, dest_reg);
            abi::emit_pop_reg(ctx.emitter, source_reg);
            abi::emit_load_int_immediate(
                ctx.emitter,
                sentinel_reg,
                UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
            );
            ctx.emitter
                .instruction(&format!("cmp rdx, {}", sentinel_reg));            // re-test the marker now that the call has clobbered the flags
            ctx.emitter
                .instruction(&format!("je {}", done_label));                    // leave the uninitialized marker exactly as the shallow copy placed it
            abi::emit_store_to_address(ctx.emitter, "rax", dest_reg, offset);
            abi::emit_store_to_address(ctx.emitter, "rdx", dest_reg, offset + 8);
            ctx.emitter.label(&done_label);
        }
    }
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
