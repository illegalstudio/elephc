//! Purpose:
//! Emits uninitialized typed-property guards and fatal paths.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Catchable Error publication and uncaught diagnostics retain their existing behavior.

use super::*;

/// Returns whether an untyped fixed slot can carry the post-`unset()` marker.
///
/// Type checking widens only reachable untyped slots to boxed `Mixed`. Packed fields and
/// reference-cell slots keep their distinct layouts and must never be interpreted this way.
pub(super) fn slot_supports_untyped_unset_marker(slot: &PropertySlot) -> bool {
    !slot.is_declared
        && !slot.is_packed
        && !slot.is_reference
        && slot.php_type.codegen_repr() == PhpType::Mixed
}

/// The class whose `__get` answers a read of this slot once `unset()` emptied it, if any.
///
/// php's lazy-initialization idiom: a constructor unsets a typed property and `__get` fills it on
/// first read (Symfony's `Constraint::$groups`). A property nothing unsets keeps php's `Error`,
/// and so does a read inside `__get`, which php's recursion guard answers from the slot. Only a
/// boxed `__get` answer is taken: its representation is the one every read result accepts.
fn lazy_property_get_class(ctx: &FunctionContext<'_>, slot: &PropertySlot) -> Option<String> {
    let class_name = slot.class_name.trim_start_matches('\\');
    let info = ctx.module.class_infos.get(class_name)?;
    let get = info.methods.get("__get")?;
    let answers_boxed = matches!(get.return_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_));
    (info.lazily_unset_properties.contains(&slot.property)
        && answers_boxed
        && !ctx.function.name.ends_with("__get"))
    .then(|| class_name.to_string())
}

/// Reads an emptied slot through `__get`; the returned label follows the slot read.
fn emit_lazy_property_get(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    object_reg: &str,
    class_name: &str,
    missing_result: PropertyReadMissingResult<'_>,
) -> Result<String> {
    let initialized_label = ctx.next_label("lazy_property_present");
    let done_label = ctx.next_label("lazy_property_read_done");
    let target = super::resolve_method_call_target(ctx, class_name, "__get", 2)?;
    let (label, len) = ctx.data.add_string(slot.property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, {}", object_reg));        // keep the receiver across the marker probe
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // spill the receiver for the magic call
            emit_typed_property_initialized_bool(ctx, slot, object_reg);
            ctx.emitter.instruction("mov x1, x0");                              // hold the initialized flag
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // restore the receiver
            ctx.emitter.instruction(&format!("cbnz x1, {}", initialized_label)); // read the slot once it holds a value
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rdi, {}", object_reg));       // keep the receiver across the marker probe
            emit_typed_property_initialized_bool(ctx, slot, object_reg);
            ctx.emitter.instruction("test rax, rax");                           // test the initialized flag
            ctx.emitter.instruction(&format!("jne {}", initialized_label));     // read the slot once it holds a value
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
    }
    super::super::emit_resolved_method_call(ctx, &target)?;
    if let PropertyReadMissingResult::Instruction(inst) = missing_result {
        cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&initialized_label);
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.instruction(&format!("mov {}, x0", object_reg));            // the slot read continues from the receiver
    } else {
        ctx.emitter.instruction(&format!("mov {}, rdi", object_reg));           // the slot read continues from the receiver
    }
    Ok(done_label)
}

/// Result representation one guarded property read must materialize on an absent slot.
pub(super) enum PropertyReadMissingResult<'a> {
    /// Match the EIR instruction's declared result registers.
    Instruction(&'a Instruction),
    /// Produce an owned boxed `Mixed` cell for runtime-class dispatch.
    Boxed,
}

/// Guards one fixed-slot read against both typed-uninitialized and untyped-removed states.
///
/// Typed value reads retain their catchable `Error`; probes yield null for an uninitialized slot.
/// A removed untyped slot warns for a value read, stays silent for a probe, and yields null. The
/// returned label must be emitted after the caller's initialized-value materialization so both
/// branches converge with the same `Mixed` result shape.
pub(super) fn emit_property_read_state_guard(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    object_reg: &str,
    mode: PropertyFetchMode,
    missing_result: PropertyReadMissingResult<'_>,
) -> Result<Option<String>> {
    if slot.is_declared && mode.is_read() {
        if let Some(class_name) = lazy_property_get_class(ctx, slot) {
            return emit_lazy_property_get(ctx, slot, object_reg, &class_name, missing_result)
                .map(Some);
        }
        if slot.is_reference {
            let cell_reg = reference_pointer_reg(ctx, object_reg);
            abi::emit_load_from_address(ctx.emitter, cell_reg, object_reg, slot.offset);
            emit_uninitialized_owned_ref_property_guard(ctx, slot, cell_reg);
        } else {
            emit_uninitialized_typed_property_guard(ctx, slot, object_reg);
        }
        return Ok(None);
    }
    if !slot.is_declared && !slot_supports_untyped_unset_marker(slot) {
        return Ok(None);
    }

    let initialized_label = ctx.next_label("property_present");
    let done_label = ctx.next_label("property_read_done");
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    if slot.is_reference {
        let cell_reg = reference_pointer_reg(ctx, object_reg);
        abi::emit_load_from_address(ctx.emitter, cell_reg, object_reg, slot.offset);
        abi::emit_load_from_address(ctx.emitter, marker_reg, cell_reg, 0);
    } else {
        abi::emit_load_from_address(ctx.emitter, marker_reg, object_reg, slot.offset + 8);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        sentinel_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(                                            // compare the property marker with the uninitialized sentinel
                &format!("cmp {}, {}", marker_reg, sentinel_reg)
            );
            ctx.emitter.instruction(                                            // read the fixed slot only while the property remains initialized
                &format!("b.ne {}", initialized_label)
            );
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(                                            // compare the property marker with the uninitialized sentinel
                &format!("cmp {}, {}", marker_reg, sentinel_reg)
            );
            ctx.emitter.instruction(                                            // read the fixed slot only while the property remains initialized
                &format!("jne {}", initialized_label)
            );
        }
    }
    if mode.is_read() {
        emit_undefined_property_warning(ctx, &slot.class_name, &slot.property);
    }
    match missing_result {
        PropertyReadMissingResult::Instruction(inst) => {
            ensure_dynamic_property_miss_supported(inst)?;
            emit_dynamic_property_miss_result(ctx, inst);
        }
        PropertyReadMissingResult::Boxed => emit_boxed_null(ctx),
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&initialized_label);
    Ok(Some(done_label))
}

/// Emits a fatal guard for reads from uninitialized typed properties.
pub(super) fn emit_uninitialized_typed_property_guard(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    object_reg: &str,
) {
    let initialized_label = ctx.next_label("typed_prop_initialized");
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, marker_reg, object_reg, slot.offset + 8);
    abi::emit_load_int_immediate(
        ctx.emitter,
        sentinel_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the property marker against the uninitialized sentinel
            ctx.emitter
                .instruction(&format!("b.ne {}", initialized_label)); // continue the property read once the slot has been initialized
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the property marker against the uninitialized sentinel
            ctx.emitter
                .instruction(&format!("jne {}", initialized_label)); // continue the property read once the slot has been initialized
        }
    }
    emit_uninitialized_typed_property_fatal(ctx, slot);
    ctx.emitter.label(&initialized_label);
}

/// Guards an object-owned reference property whose initialization marker lives in cell[0].
pub(super) fn emit_uninitialized_owned_ref_property_guard(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    cell_reg: &str,
) {
    let initialized_label = ctx.next_label("typed_ref_prop_initialized");
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, marker_reg, cell_reg, 0);
    abi::emit_load_int_immediate(
        ctx.emitter,
        sentinel_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the owned reference payload against the uninitialized sentinel
            ctx.emitter
                .instruction(&format!("b.ne {}", initialized_label)); // continue once the reference property has been initialized
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the owned reference payload against the uninitialized sentinel
            ctx.emitter
                .instruction(&format!("jne {}", initialized_label)); // continue once the reference property has been initialized
        }
    }
    emit_uninitialized_typed_property_fatal(ctx, slot);
    ctx.emitter.label(&initialized_label);
}

/// Compares a typed instance-property marker with the uninitialized sentinel.
pub(super) fn emit_typed_property_initialized_bool(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    object_reg: &str,
) {
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    if slot.is_reference {
        let cell_reg = reference_pointer_reg(ctx, object_reg);
        abi::emit_load_from_address(ctx.emitter, cell_reg, object_reg, slot.offset);
        abi::emit_load_from_address(ctx.emitter, marker_reg, cell_reg, 0);
    } else {
        abi::emit_load_from_address(ctx.emitter, marker_reg, object_reg, slot.offset + 8);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        sentinel_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(                                            // compare the property marker against the uninitialized sentinel
                &format!("cmp {}, {}", marker_reg, sentinel_reg)
            );
            ctx.emitter.instruction("cset x0, ne");                             // materialize true when the instance property is initialized
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(                                            // compare the property marker against the uninitialized sentinel
                &format!("cmp {}, {}", marker_reg, sentinel_reg)
            );
            ctx.emitter.instruction("setne al");                                // materialize true when the instance property is initialized
            ctx.emitter.instruction("movzx rax, al");                           // widen the initialization flag into the integer result register
        }
    }
}

/// Emits the runtime throw for an uninitialized typed-property read.
///
/// Constructs an `Error` object with the diagnostic message, publishes it to
/// `_exc_value`, and branches to `__rt_throw_current` so surrounding try/catch
/// blocks can observe and catch it. When no handler is registered, the uncaught
/// fast path prints the specific fatal diagnostic and exits, preserving the old behavior.
pub(super) fn emit_uninitialized_typed_property_fatal(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
) {
    let message = format!(
        "Typed property {}::${} must not be accessed before initialization",
        slot.class_name, slot.property
    );
    let fatal_message = format!("Fatal error: {}\n", message);
    let (fatal_label, fatal_len) = ctx.data.add_string(fatal_message.as_bytes());
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());

    // This is the codegen-raised throwable with four constants changed: always `Error`, never a
    // creation line, and a report that goes to stderr and exits 1. Measured on the Symfony
    // `--web` module it is 56 280 sites and 2 363 760 lines, so those constants move into the
    // record and the site becomes its address plus a call.
    if let Some(helper) = crate::codegen::shared_static_throw::shared_throw_label(ctx) {
        // php locates the Error at the line of the read. A read inside a shared helper body
        // belongs to the statement that called it; any other read names its own instruction.
        let creation_line = match ctx.current_source_location() {
            Some((_, line)) => line,
            None if crate::codegen::shared_static_throw::raises_on_calling_site(ctx) => {
                crate::codegen::shared_static_throw::LINE_OF_CALLING_SITE
            }
            None => 0,
        };
        let record = crate::codegen::shared_static_throw::throw_descriptor(
            ctx.data,
            "_spl_error_class_id",
            &message_label,
            message_len,
            creation_line,
            &fatal_label,
            fatal_len,
            crate::codegen::shared_static_throw::REPORT_TYPED_PROPERTY,
        );
        abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &record);
        abi::emit_call_label(ctx.emitter, helper);
        return;
    }

    emit_uninitialized_typed_property_uncaught_fatal_if_no_handler(
        ctx,
        &fatal_label,
        fatal_len,
    );
    super::super::exceptions::emit_error(ctx, &message);
}

/// Emits a no-handler fast path that preserves the specific typed-property fatal text.
pub(super) fn emit_uninitialized_typed_property_uncaught_fatal_if_no_handler(
    ctx: &mut FunctionContext<'_>,
    fatal_label: &str,
    fatal_len: usize,
) {
    let throw_label = ctx.next_label("typed_property_throw");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_exc_handler_top", 0);
            ctx.emitter.instruction(&format!("cbnz x9, {}", throw_label));      // keep typed-property errors catchable when a handler is active
            abi::emit_symbol_address(ctx.emitter, "x1", fatal_label);          // load the specific uninitialized typed-property fatal text
            ctx.emitter.instruction(&format!("mov x2, #{}", fatal_len));        // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the uninitialized typed-property fatal
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_exc_handler_top", 0);
            ctx.emitter.instruction("test r10, r10");                           // is there an active handler that can catch the Error?
            ctx.emitter.instruction(&format!("jne {}", throw_label));           // keep typed-property errors catchable when a handler is active
            abi::emit_symbol_address(ctx.emitter, "rsi", fatal_label);          // load the specific uninitialized typed-property fatal text
            ctx.emitter.instruction(&format!("mov edx, {}", fatal_len));        // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the uninitialized typed-property fatal
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the specific uninitialized typed-property fatal diagnostic
            abi::emit_exit(ctx.emitter, 1);
        }
    }
    ctx.emitter.label(&throw_label);
}
