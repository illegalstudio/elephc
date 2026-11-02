//! Purpose:
//! Allocates compact Throwable payloads and records construction lines for both object layouts.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Ordinary objects record creation lines after property defaults initialize the shared prefix.
//! - Message storage and previous-object retention follow the receiver's native layout.

use super::*;

/// Lowers builtin Throwable allocation using the compact runtime payload layout.
pub(super) fn lower_builtin_throwable_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    class_id: u64,
) -> Result<()> {
    if inst.operands.len() > 3 {
        return Err(CodegenIrError::unsupported(format!(
            "{}::__construct with {} EIR operands",
            class_name,
            inst.operands.len()
        )));
    }
    // PHP's `getLine()` is the line of the `new`, not of the `throw`, and this instruction IS the
    // `new` — so the span it already carries is exactly the right one. A missing span (an
    // exception synthesized by an optimizer pass, say) degrades to zero rather than to a guess.
    // Injected prelude code has a synthetic span; php attributes those throws to the user call
    // site published in `_php_diagnostic_line` before entering the prelude helper.
    let creation_line = inst.span.map_or(0, |span| span.source_line());
    let runtime_line = inst.span.is_some_and(|span| !span.is_from_source());
    emit_throwable_allocation(ctx, &class_name, class_id, creation_line, runtime_line);
    preserve_throwable_for_init(ctx);
    emit_throwable_message_fields(ctx, inst.operands.first().copied())?;
    emit_throwable_code_field(ctx, inst.operands.get(1).copied())?;
    emit_throwable_previous_field(ctx, inst.operands.get(2).copied())?;
    restore_throwable_after_init(ctx);
    let object_reg = abi::int_result_reg(ctx.emitter);
    emit_throwable_trace_capture(ctx, object_reg);
    store_if_result(ctx, inst)
}

/// Records the frames live at this `new` as the Throwable's `getTrace()`.
///
/// PHP fills a Throwable's trace when the object is created, not when it is thrown. The frames
/// come from the interpreter when one is running, and otherwise from the compiled activation
/// chain, which only has frame readers that materialize a frame in a module that can read a trace
/// (`frame::module_materializes_backtrace`); every other module emits nothing here and keeps the
/// declared empty trace (its print-only readers would write `#-1 ...` to stdout). `object_reg` survives the call.
pub(super) fn emit_throwable_trace_capture(ctx: &mut FunctionContext<'_>, object_reg: &str) {
    if !ctx.module.required_runtime_features.eval_bridge
        && !crate::codegen::frame::module_materializes_backtrace(ctx.module)
    {
        return;
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, object_reg);
    if object_reg != result_reg {
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("mov x0, {}", object_reg)), // pass the Throwable to the trace helper
            Arch::X86_64 => ctx.emitter.instruction(&format!("mov rax, {}", object_reg)), // pass the Throwable to the trace helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_throwable_capture_trace");
    abi::emit_pop_reg(ctx.emitter, object_reg);
}

/// Returns true for builtin classes that share PHP's compact Throwable payload.
pub(super) fn is_builtin_throwable_payload_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "Error"
            | "ClosedGeneratorException"
            | "CompileError"
            | "ParseError"
            | "RequestParseBodyException"
            | "TypeError"
            | "ArgumentCountError"
            | "ValueError"
            | "ArithmeticError"
            | "DivisionByZeroError"
            | "AssertionError"
            | "UnhandledMatchError"
            | "Exception"
            | "RuntimeException"
            | "ReflectionException"
            | "JsonException"
            | "FiberError"
            | "LogicException"
            | "BadFunctionCallException"
            | "BadMethodCallException"
            | "DomainException"
            | "InvalidArgumentException"
            | "LengthException"
            | "OutOfRangeException"
            | "OutOfBoundsException"
            | "OverflowException"
            | "RangeException"
            | "UnderflowException"
            | "UnexpectedValueException"
    )
}

/// Returns a class id for Throwable-compatible classes that can use the compact payload.
pub(super) fn throwable_payload_class_id(ctx: &FunctionContext<'_>, class_name: &str) -> Option<u64> {
    let class_info = ctx.module.class_infos.get(class_name)?;
    if is_builtin_throwable_payload_class(class_name)
        || throwable_payload_compatible_user_class(ctx, class_name, class_info)
    {
        Some(class_info.class_id)
    } else {
        None
    }
}

/// Returns true when a user subclass can reuse the compact Throwable storage layout.
pub(super) fn throwable_payload_compatible_user_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    class_info: &ClassInfo,
) -> bool {
    super::super::is_throwable_like_class(ctx, class_name)
        && throwable_inherits_only_builtin_storage_and_constructor(class_info)
}

/// Requires the complete inheritance chain, not only the leaf, to retain the builtin layout.
fn throwable_inherits_only_builtin_storage_and_constructor(class_info: &ClassInfo) -> bool {
    class_info
        .property_declaring_classes
        .values()
        .all(|owner| is_builtin_throwable_payload_class(owner))
        && class_info.method_declaring_classes.get("__construct")
            .is_none_or(|owner| is_builtin_throwable_payload_class(owner))
}

/// Initializes the inherited builtin Throwable fields for a lexical parent constructor call.
pub(in crate::codegen::lower_inst) fn try_lower_builtin_throwable_parent_constructor(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    impl_class: &str,
    method_name: &str,
    this_slot: LocalSlotId,
) -> Result<bool> {
    if method_name != "__construct" || !is_builtin_throwable_payload_class(impl_class) {
        return Ok(false);
    }
    if inst.operands.len() > 3 {
        return Err(CodegenIrError::unsupported(format!(
            "{}::__construct with {} EIR operands",
            impl_class,
            inst.operands.len()
        )));
    }

    ctx.load_local_to_result(this_slot)?;
    preserve_throwable_for_init(ctx);
    emit_throwable_message_fields(ctx, inst.operands.first().copied())?;
    emit_throwable_code_field(ctx, inst.operands.get(1).copied())?;
    emit_release_constructed_previous(ctx);
    emit_throwable_previous_field(ctx, inst.operands.get(2).copied())?;
    restore_throwable_after_init(ctx);
    super::super::store_call_result(ctx, inst, &PhpType::Void)?;
    Ok(true)
}

/// Releases the value an already-allocated object holds in its `previous` slot before the
/// parent constructor overwrites it.
///
/// A subclass with its own constructor is allocated through the generic object path, which
/// seeds the `?Throwable $previous` property with a boxed null. Overwriting that slot without a
/// release leaked one box per constructed exception (every Twig and Symfony exception with a
/// constructor). `__rt_decref_any` ignores null and non-heap words.
fn emit_release_constructed_previous(ctx: &mut FunctionContext<'_>) {
    let offset = crate::codegen_support::throwable_layout::PREVIOUS_OFFSET;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp]");                            // reload the Throwable being constructed
            ctx.emitter.instruction(&format!("ldr x0, [x9, #{}]", offset));     // load the previous slot's current owner
            ctx.emitter.instruction(&format!("str xzr, [x9, #{}]", offset));    // clear the slot before releasing what it held
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                // reload the Throwable being constructed
            ctx.emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", offset)); // load the previous slot's current owner
            ctx.emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", offset)); // clear the slot before releasing what it held
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");                      // release the default (or earlier) previous value
}

/// Stamps ordinary Throwable metadata outside every PHP-visible property slot.
pub(super) fn emit_regular_throwable_creation_line(
    ctx: &mut FunctionContext<'_>,
    source: ValueId,
    object_reg: &str,
    class_id: u64,
) {
    let offset = crate::codegen_support::throwable_layout::ordinary_creation_line_offset_by_id(
        class_id, &ctx.module.class_infos,
    ).expect("ordinary Throwable must reserve its construction metadata");
    let creation_line = ctx.function.value(source).and_then(|value| {
        let ValueDef::Instruction { inst, .. } = value.def else { return None };
        ctx.function.instruction(inst)?.span
    }).map_or(0, |span| span.source_line());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x9", i64::from(creation_line));
            abi::emit_store_to_address(ctx.emitter, "x9", object_reg, offset);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(                                   // record the allocation line outside mutable PHP property storage
                "mov QWORD PTR [{object_reg} + {offset}], {creation_line}"
            ));                                                                 // preserve construction metadata across property writes
        }
    }
}

/// Allocates a compact Throwable payload and stamps its heap kind, class id and creation line.
///
/// `creation_line` is the ONE-BASED source line of the `new` expression, or `0` when the
/// instruction carried no span. PHP records the line where a Throwable is CONSTRUCTED, not where
/// it is thrown — `$e = new RuntimeException(...)` on line 2 followed by `throw $e;` on line 5
/// reports line 2 — so the value belongs here rather than on the throw terminator.
///
/// The payload size comes from [`crate::types::object_payload_size`], the same authority that
/// builds `_class_object_payload_sizes`, so the dynamic-property tail this class owns is reserved
/// here and zeroed by the allocator instead of being read past the payload at free time.
pub(super) fn emit_throwable_allocation(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    class_id: u64,
    creation_line: u32,
    runtime_line: bool,
) {
    let class_info = ctx.module.class_infos.values()
        .find(|info| info.class_id == class_id)
        .expect("Throwable allocation requires class metadata");
    let payload_size = crate::types::object_payload_size(class_name, class_info);
    crate::codegen_support::throwable_layout::emit_allocate(ctx.emitter, payload_size);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- stamp the complete declared Throwable payload --
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 marks runtime object payloads
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the Throwable payload
            ctx.emitter.instruction("bl __rt_object_handle_acquire");           // bind the new object to its PHP object handle
            ctx.emitter.instruction(&format!("mov x9, #{}", class_id));         // materialize the Throwable runtime class id
            ctx.emitter.instruction("str x9, [x0]");                            // store class id at payload offset zero
            if runtime_line {
                emit_throwable_runtime_line_aarch64(ctx, "x0", "x9");
            } else {
                emit_throwable_creation_line_aarch64(ctx, "x0", "x9", creation_line);
            }
            ctx.emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // previous defaults to null until constructor init
        }
        Arch::X86_64 => {
            // -- stamp the complete declared Throwable payload --
            ctx.emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(6)
            ));                                                                 // materialize the x86_64 Throwable heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the Throwable payload
            ctx.emitter.instruction("call __rt_object_handle_acquire");         // bind the new object to its PHP object handle
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the Throwable runtime class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store class id at payload offset zero
            if runtime_line {
                emit_throwable_runtime_line_x86_64(ctx, "rax", "r10");
            } else {
                emit_throwable_creation_line_x86_64(ctx, "rax", creation_line);
            }
            ctx.emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // previous defaults to null until constructor init
        }
    }
    // The allocation stamped the entry file; `new` written in an included or autoloaded file
    // names that file instead (both are static data, so nothing is released).
    if let Some(path) = ctx.span_source_file(ctx.current_instruction_span()).map(str::to_string) {
        let (label, len) = ctx.data.add_string(path.as_bytes());
        let object_reg = abi::int_result_reg(ctx.emitter);
        crate::codegen_support::throwable_layout::emit_store_static_file(
            ctx.emitter,
            object_reg,
            &label,
            len,
        );
    }
}

/// Writes the caller line published by `prepare_call_site` into an AArch64 Throwable payload.
fn emit_throwable_runtime_line_aarch64(
    ctx: &mut FunctionContext<'_>,
    payload_reg: &str,
    scratch_reg: &str,
) {
    // `prepare_call_site` publishes one `{file descriptor, line}` record through `php_site`.
    let site = abi::php_site_symbol(&ctx.emitter.target);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch_reg, &site, 0);
    ctx.emitter.instruction(&format!("ldr {scratch_reg}, [{scratch_reg}, #8]")); // the published site's line
    ctx.emitter.instruction(&format!(                                           // record the caller's source line for a prelude-synthesized throwable
        "str {}, [{}, #{}]",
        scratch_reg, payload_reg, THROWABLE_CREATION_LINE_OFFSET
    ));                                                                         // store the one-based caller line of the internal function
}

/// Writes the caller line published by `prepare_call_site` into an x86_64 Throwable payload.
fn emit_throwable_runtime_line_x86_64(
    ctx: &mut FunctionContext<'_>,
    payload_reg: &str,
    scratch_reg: &str,
) {
    let site = abi::php_site_symbol(&ctx.emitter.target);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch_reg, &site, 0);
    ctx.emitter.instruction(&format!("mov {scratch_reg}, QWORD PTR [{scratch_reg} + 8]")); // the published site's line
    ctx.emitter.instruction(&format!(                                           // record the caller's source line for a prelude-synthesized throwable
        "mov QWORD PTR [{} + {}], {}",
        payload_reg, THROWABLE_CREATION_LINE_OFFSET, scratch_reg
    ));                                                                         // store the one-based caller line of the internal function
}

/// Writes an AArch64 Throwable creation line into the payload, via `scratch_reg`.
///
/// `payload_reg` holds the freshly allocated payload. The store is UNCONDITIONAL even for line
/// `0`: `__rt_heap_alloc` recycles blocks without zeroing them, so an unwritten slot would hand
/// `getLine()` whatever the previous owner left behind.
pub(in crate::codegen::lower_inst) fn emit_throwable_creation_line_aarch64(
    ctx: &mut FunctionContext<'_>,
    payload_reg: &str,
    scratch_reg: &str,
    creation_line: u32,
) {
    if creation_line == 0 {
        ctx.emitter.instruction(&format!(                                       // initialize the creation line even when no source span is available
            "str xzr, [{}, #{}]",
            payload_reg, THROWABLE_CREATION_LINE_OFFSET
        ));                                                                     // no span on the allocating instruction: an unknown line reads back as zero
        return;
    }
    abi::emit_load_int_immediate(ctx.emitter, scratch_reg, i64::from(creation_line));
    ctx.emitter.instruction(&format!(                                           // record the source line associated with this allocation
        "str {}, [{}, #{}]",
        scratch_reg, payload_reg, THROWABLE_CREATION_LINE_OFFSET
    ));                                                                         // store the one-based source line of the `new` expression
}

/// Writes an x86_64 Throwable creation line into the payload.
///
/// Mirrors [`emit_throwable_creation_line_aarch64`]; the two architectures emit independently, and
/// an upstream fix has already been lost once by living on only one of them.
pub(in crate::codegen::lower_inst) fn emit_throwable_creation_line_x86_64(
    ctx: &mut FunctionContext<'_>,
    payload_reg: &str,
    creation_line: u32,
) {
    ctx.emitter.instruction(&format!(                                           // record the source line associated with this allocation
        "mov QWORD PTR [{} + {}], {}",
        payload_reg, THROWABLE_CREATION_LINE_OFFSET, creation_line
    ));                                                                         // store the one-based source line of the `new` expression (zero when unknown)
}

/// Saves the newly allocated Throwable object while constructor operands are loaded.
pub(super) fn preserve_throwable_for_init(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Restores the initialized Throwable object to the canonical object result register.
pub(super) fn restore_throwable_after_init(ctx: &mut FunctionContext<'_>) {
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Stamps the construction site after ordinary inherited property defaults.
pub(super) fn emit_throwable_creation_site(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    creation_line: u32,
) {
    // Inside the shared `new $class(...)` helper the `new` was written at the CALLING site, which
    // published its file and line (`outlined_dynamic_new::emit_outlined_dynamic_new_call`).
    if super::super::outlined_dynamic_new::inside_dynamic_new_ctor_helper(ctx) {
        use crate::codegen_support::throwable_layout::{
            emit_replace_initial_file_from_site, DYNAMIC_NEW_SITE_BYTES, DYNAMIC_NEW_SITE_SYMBOL,
        };
        ctx.data.add_comm(DYNAMIC_NEW_SITE_SYMBOL.to_string(), DYNAMIC_NEW_SITE_BYTES);
        emit_replace_initial_file_from_site(ctx.emitter, object_reg);
        let scratch = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_symbol_to_reg(ctx.emitter, scratch, DYNAMIC_NEW_SITE_SYMBOL, 16);
        abi::emit_store_to_address(
            ctx.emitter,
            scratch,
            object_reg,
            crate::codegen_support::sentinels::THROWABLE_CREATION_LINE_OFFSET as usize,
        );
        emit_throwable_trace_capture(ctx, object_reg);
        return;
    }
    // `getFile()` names the file the `new` was written in: an included or autoloaded class file
    // has its own, and only the entry file's code uses `_script_source_file`.
    let file = ctx
        .span_source_file(ctx.current_instruction_span())
        .map(str::to_string)
        .map(|path| ctx.data.add_string(path.as_bytes()));
    crate::codegen_support::throwable_layout::emit_replace_initial_file_with(
        ctx.emitter,
        object_reg,
        file.as_ref().map(|(label, len)| (label.as_str(), *len)),
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throwable_creation_line_aarch64(ctx, object_reg, "x9", creation_line),
        Arch::X86_64 => emit_throwable_creation_line_x86_64(ctx, object_reg, creation_line),
    }
    emit_throwable_trace_capture(ctx, object_reg);
}

/// Writes the message pointer and length into the compact Throwable payload.
pub(super) fn emit_throwable_message_fields(
    ctx: &mut FunctionContext<'_>,
    message: Option<ValueId>,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throwable_message_fields_aarch64(ctx, message),
        Arch::X86_64 => emit_throwable_message_fields_x86_64(ctx, message),
    }
}

/// Writes AArch64 Throwable message fields from a string operand or an empty default.
pub(super) fn emit_throwable_message_fields_aarch64(
    ctx: &mut FunctionContext<'_>,
    message: Option<ValueId>,
) -> Result<()> {
    if let Some(message) = message {
        crate::codegen::lower_inst::builtins::strings::load_value_as_string_to_regs(
            ctx,
            message,
            "Throwable message",
            "x1",
            "x2",
        )?;
        abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    } else {
        emit_empty_string_to_regs(ctx, "x1", "x2");
    }
    ctx.emitter.instruction("ldr x9, [sp]");                                    // reload the saved Throwable object for message initialization
    ctx.emitter.instruction("str x1, [x9, #8]");                                // store Throwable message pointer
    ctx.emitter.instruction("str x2, [x9, #16]");                               // store Throwable message length
    Ok(())
}

/// Writes x86_64 Throwable message fields from a string operand or an empty default.
pub(super) fn emit_throwable_message_fields_x86_64(
    ctx: &mut FunctionContext<'_>,
    message: Option<ValueId>,
) -> Result<()> {
    if let Some(message) = message {
        crate::codegen::lower_inst::builtins::strings::load_value_as_string_to_regs(
            ctx,
            message,
            "Throwable message",
            "rax",
            "rdx",
        )?;
        abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    } else {
        emit_empty_string_to_regs(ctx, "rax", "rdx");
    }
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                        // reload the saved Throwable object for message initialization
    ctx.emitter.instruction("mov QWORD PTR [r11 + 8], rax");                    // store Throwable message pointer
    ctx.emitter.instruction("mov QWORD PTR [r11 + 16], rdx");                   // store Throwable message length
    Ok(())
}

/// Materializes the shared empty string constant into a string register pair.
pub(super) fn emit_empty_string_to_regs(ctx: &mut FunctionContext<'_>, ptr_reg: &str, len_reg: &str) {
    let (label, len) = ctx.data.add_string(b"");
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Writes the integer exception code into the compact Throwable payload.
pub(super) fn emit_throwable_code_field(ctx: &mut FunctionContext<'_>, code: Option<ValueId>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throwable_code_field_aarch64(ctx, code),
        Arch::X86_64 => emit_throwable_code_field_x86_64(ctx, code),
    }
}

/// Writes the AArch64 Throwable code field from an integer operand or zero default.
pub(super) fn emit_throwable_code_field_aarch64(
    ctx: &mut FunctionContext<'_>,
    code: Option<ValueId>,
) -> Result<()> {
    if let Some(code) = code {
        ctx.load_value_to_reg(code, "x1")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "x1", 0);
    }
    ctx.emitter.instruction("ldr x9, [sp]");                                    // reload the saved Throwable object for code initialization
    ctx.emitter.instruction("str x1, [x9, #24]");                               // store Throwable code
    Ok(())
}

/// Writes the x86_64 Throwable code field from an integer operand or zero default.
pub(super) fn emit_throwable_code_field_x86_64(
    ctx: &mut FunctionContext<'_>,
    code: Option<ValueId>,
) -> Result<()> {
    if let Some(code) = code {
        ctx.load_value_to_reg(code, "rax")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
    }
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                        // reload the saved Throwable object for code initialization
    ctx.emitter.instruction("mov QWORD PTR [r11 + 24], rax");                   // store Throwable code
    Ok(())
}

/// Writes PHP's `$previous` object pointer into its declared Throwable property slot.
pub(super) fn emit_throwable_previous_field(
    ctx: &mut FunctionContext<'_>,
    previous: Option<ValueId>,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throwable_previous_field_aarch64(ctx, previous),
        Arch::X86_64 => emit_throwable_previous_field_x86_64(ctx, previous),
    }
}

/// Writes the AArch64 Throwable previous field, retaining a non-null previous object.
pub(super) fn emit_throwable_previous_field_aarch64(
    ctx: &mut FunctionContext<'_>,
    previous: Option<ValueId>,
) -> Result<()> {
    if let Some(previous) = previous {
        // -- normalize and retain the previous Throwable --
        let store_label = ctx.next_label("throwable_previous_store");
        let null_label = ctx.next_label("throwable_previous_null");
        ctx.load_value_to_reg(previous, "x0")?;
        ctx.emitter.instruction(&format!("cbz x0, {}", null_label));            // missing previous → store null
        abi::emit_load_int_immediate(ctx.emitter, "x9", RUNTIME_NULL_SENTINEL);
        ctx.emitter.instruction("cmp x0, x9");                                  // is previous the in-band null sentinel?
        ctx.emitter.instruction(&format!("b.eq {}", null_label));               // treat sentinel as null payload
        abi::emit_call_label(ctx.emitter, "__rt_incref"); // retain previous for the Throwable payload
        ctx.emitter.instruction(&format!("b {}", store_label));                 // keep the retained previous pointer
        ctx.emitter.label(&null_label);
        ctx.emitter.instruction("mov x0, xzr");                                 // compact payload stores raw null, not the in-band sentinel
        ctx.emitter.label(&store_label);
        ctx.emitter.instruction("ldr x9, [sp]");                                // reload the saved Throwable object for previous initialization
        ctx.emitter.instruction(&format!("str x0, [x9, #{}]", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // store Throwable previous pointer
    } else {
        // -- initialize an omitted previous Throwable --
        ctx.emitter.instruction("ldr x9, [sp]");                                // reload the saved Throwable object for previous initialization
        ctx.emitter.instruction(&format!("str xzr, [x9, #{}]", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // previous defaults to null
    }
    Ok(())
}

/// Writes the x86_64 Throwable previous field, retaining a non-null previous object.
pub(super) fn emit_throwable_previous_field_x86_64(
    ctx: &mut FunctionContext<'_>,
    previous: Option<ValueId>,
) -> Result<()> {
    if let Some(previous) = previous {
        // -- normalize and retain the previous Throwable --
        let store_label = ctx.next_label("throwable_previous_store");
        let null_label = ctx.next_label("throwable_previous_null");
        ctx.load_value_to_reg(previous, "rax")?;
        ctx.emitter.instruction("test rax, rax");                               // missing previous → store null
        ctx.emitter.instruction(&format!("jz {}", null_label));                 // branch around retain for a missing previous
        abi::emit_load_int_immediate(ctx.emitter, "r10", RUNTIME_NULL_SENTINEL);
        ctx.emitter.instruction("cmp rax, r10");                                // is previous the in-band null sentinel?
        ctx.emitter.instruction(&format!("je {}", null_label));                 // treat sentinel as null payload
        abi::emit_call_label(ctx.emitter, "__rt_incref"); // retain previous for the Throwable payload
        ctx.emitter.instruction(&format!("jmp {}", store_label));               // keep the retained previous pointer
        ctx.emitter.label(&null_label);
        ctx.emitter.instruction("xor rax, rax");                                // compact payload stores raw null, not the in-band sentinel
        ctx.emitter.label(&store_label);
        ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                    // reload the saved Throwable object for previous initialization
        ctx.emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // store Throwable previous pointer
    } else {
        // -- initialize an omitted previous Throwable --
        ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                    // reload the saved Throwable object for previous initialization
        ctx.emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET)); // previous defaults to null
    }
    Ok(())
}
