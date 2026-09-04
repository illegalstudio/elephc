//! Purpose:
//! Allocates compact Throwable payloads and initializes constructor fields.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Creation lines, message storage, previous-object retention, and target layouts are unchanged.

use super::*;
use crate::codegen_support::sentinels::THROWABLE_TRACE_EXACT_OFFSET;

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
    let creation_line = inst.span.map_or(0, |span| span.line);
    emit_throwable_allocation(ctx, class_id, creation_line);
    preserve_throwable_for_init(ctx);
    emit_throwable_message_fields(ctx, inst.operands.first().copied())?;
    emit_throwable_code_field(ctx, inst.operands.get(1).copied())?;
    emit_throwable_previous_field(ctx, inst.operands.get(2).copied())?;
    restore_throwable_after_init(ctx);
    store_if_result(ctx, inst)
}

/// Returns true for builtin classes that share PHP's compact Throwable payload.
pub(super) fn is_builtin_throwable_payload_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "Error"
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
        && !class_declares_own_instance_properties(class_name, class_info)
        && !class_declares_own_constructor(class_name, class_info)
}

/// Returns true when `class_name` declares an instance property of its own.
pub(super) fn class_declares_own_instance_properties(class_name: &str, class_info: &ClassInfo) -> bool {
    class_info
        .property_declaring_classes
        .values()
        .any(|declaring_class| declaring_class == class_name)
}

/// Returns true when `class_name` declares its own `__construct` method.
pub(super) fn class_declares_own_constructor(class_name: &str, class_info: &ClassInfo) -> bool {
    let constructor_key = php_symbol_key("__construct");
    class_info
        .method_declaring_classes
        .get(&constructor_key)
        .is_some_and(|declaring_class| declaring_class == class_name)
}

/// Compact Throwable payload bytes: class_id + message(16) + code(16) + previous(16).
const THROWABLE_COMPACT_PAYLOAD_SIZE: u64 = 56;

/// Allocates a compact Throwable payload and stamps its heap kind, class id and creation line.
///
/// `creation_line` is the ONE-BASED source line of the `new` expression, or `0` when the
/// instruction carried no span. PHP records the line where a Throwable is CONSTRUCTED, not where
/// it is thrown — `$e = new RuntimeException(...)` on line 2 followed by `throw $e;` on line 5
/// reports line 2 — so the value belongs here rather than on the throw terminator.
pub(super) fn emit_throwable_allocation(ctx: &mut FunctionContext<'_>, class_id: u64, creation_line: u32) {
    // php captures a trace when a Throwable is CONSTRUCTED. Resetting here is what stops an
    // earlier builtin exception — caught, and so never reported — from leaving its frame behind
    // for this one to print as its own. Nothing is live yet: the allocation starts below.
    //
    // NOT inside a synthesized body. A `new` there is php-src's own internal `new`, and the frame
    // the caller recorded for the builtin-class method now running is part of THIS exception's
    // trace — `#0 p.php(13): SplFileInfo->getSize()`. Resetting would throw away the one frame
    // that makes the chain complete.
    if !function_is_synthetic(ctx) {
        abi::emit_call_label(ctx.emitter, "__rt_trace_reset");
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- allocate and stamp the compact Throwable payload --
            ctx.emitter.instruction(
                &format!("mov x0, #{}", THROWABLE_COMPACT_PAYLOAD_SIZE)
            );                                                                  // request compact Throwable payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 marks runtime object payloads
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the Throwable payload
            ctx.emitter.instruction("bl __rt_object_handle_acquire");           // bind the new object to its PHP object handle
            ctx.emitter.instruction(&format!("mov x9, #{}", class_id));         // materialize the Throwable runtime class id
            ctx.emitter.instruction("str x9, [x0]");                            // store class id at payload offset zero
            emit_throwable_creation_line_aarch64(ctx, "x0", "x9", creation_line);
            emit_throwable_trace_exact(ctx, "x0", "x9");
            ctx.emitter.instruction("str xzr, [x0, #40]");                      // previous defaults to null until constructor init
        }
        Arch::X86_64 => {
            // -- allocate and stamp the compact Throwable payload --
            ctx.emitter.instruction(
                &format!("mov rax, {}", THROWABLE_COMPACT_PAYLOAD_SIZE)
            );                                                                  // request compact Throwable payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(6)
            ));                                                                 // materialize the x86_64 Throwable heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the Throwable payload
            ctx.emitter.instruction("call __rt_object_handle_acquire");         // bind the new object to its PHP object handle
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the Throwable runtime class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store class id at payload offset zero
            emit_throwable_creation_line_x86_64(ctx, "rax", creation_line);
            emit_throwable_trace_exact(ctx, "rax", "r10");
            ctx.emitter.instruction("mov QWORD PTR [rax + 40], 0");             // previous defaults to null until constructor init
        }
    }
}

/// Reports whether the function being lowered was SYNTHESIZED rather than written by the user.
///
/// A builtin class compiled from a synthesized PHP AST is in php-src's position: its `new` is
/// internal, and the frame that belongs in the trace is the CALL the program made into it.
fn function_is_synthetic(ctx: &FunctionContext<'_>) -> bool {
    ctx.function.flags.is_synthetic
}

/// Stamps whether this Throwable's recorded frame list is COMPLETE.
///
/// Two sites can prove it, and they prove it differently. A `new` in USER code knows its own
/// function: `main` has nothing above it, so an empty frame list IS the whole chain — php prints
/// `#0 {main}` — while a `new` inside any other function would need that function's frame and its
/// callers, which nothing here can walk. A `new` inside a SYNTHESIZED body cannot know: the answer
/// belongs to whoever called into the builtin class, and that site publishes it.
///
/// The proof is stamped onto the VALUE because by report time the constructing site is gone. A
/// global consulted then would be answering for whatever was constructed last.
fn emit_throwable_trace_exact(ctx: &mut FunctionContext<'_>, payload_reg: &str, scratch_reg: &str) {
    if function_is_synthetic(ctx) {
        abi::emit_load_symbol_to_reg(ctx.emitter, scratch_reg, "_rt_trace_site_exact", 0);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, scratch_reg, i64::from(ctx.is_main));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!(
            "str {}, [{}, #{}]",
            scratch_reg, payload_reg, THROWABLE_TRACE_EXACT_OFFSET
        )), // whether the report may print the frames recorded for this exception
        Arch::X86_64 => ctx.emitter.instruction(&format!(
            "mov QWORD PTR [{} + {}], {}",
            payload_reg, THROWABLE_TRACE_EXACT_OFFSET, scratch_reg
        )), // whether the report may print the frames recorded for this exception
    }
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
        // A `new` with no span of its own is one a SYNTHESIZED body performed — an SPL
        // constructor, a stat getter. php reports the line of the call the program made INTO that
        // method, because its own `new` lives in php-src and has no php line: MEASURED,
        // `(new SplFileInfo("nope"))->getSize()` reports the line of the `getSize()` call.
        abi::emit_load_symbol_to_reg(ctx.emitter, scratch_reg, "_rt_internal_call_line", 0);
        ctx.emitter.instruction(&format!(
            "str {}, [{}, #{}]",
            scratch_reg, payload_reg, THROWABLE_CREATION_LINE_OFFSET
        ));                                                                     // the caller's line, or zero when there was no call into a builtin
        return;
    }
    abi::emit_load_int_immediate(ctx.emitter, scratch_reg, i64::from(creation_line));
    ctx.emitter.instruction(&format!(
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
    if creation_line == 0 {
        // See the AArch64 counterpart: a `new` with no span belongs to a synthesized body, and
        // php names the call the program made into it.
        abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_rt_internal_call_line", 0);
        ctx.emitter.instruction(&format!(
            "mov QWORD PTR [{} + {}], r10",
            payload_reg, THROWABLE_CREATION_LINE_OFFSET
        ));                                                                     // the caller's line, or zero when there was no call into a builtin
        return;
    }
    ctx.emitter.instruction(&format!(
        "mov QWORD PTR [{} + {}], {}",
        payload_reg, THROWABLE_CREATION_LINE_OFFSET, creation_line
    ));                                                                         // store the one-based source line of the `new` expression
}

/// Saves the newly allocated Throwable object while constructor operands are loaded.
pub(super) fn preserve_throwable_for_init(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Restores the initialized Throwable object to the canonical object result register.
pub(super) fn restore_throwable_after_init(ctx: &mut FunctionContext<'_>) {
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
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
        ctx.load_string_value_to_regs(message, "x1", "x2")?;
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
        ctx.load_string_value_to_regs(message, "rax", "rdx")?;
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

/// Writes PHP's `$previous` object pointer into the compact Throwable payload at offset 40.
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
        ctx.emitter.instruction("str x0, [x9, #40]");                           // store Throwable previous pointer
    } else {
        // -- initialize an omitted previous Throwable --
        ctx.emitter.instruction("ldr x9, [sp]");                                // reload the saved Throwable object for previous initialization
        ctx.emitter.instruction("str xzr, [x9, #40]");                          // previous defaults to null
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
        ctx.emitter.instruction("mov QWORD PTR [r11 + 40], rax");               // store Throwable previous pointer
    } else {
        // -- initialize an omitted previous Throwable --
        ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                    // reload the saved Throwable object for previous initialization
        ctx.emitter.instruction("mov QWORD PTR [r11 + 40], 0");                 // previous defaults to null
    }
    Ok(())
}
