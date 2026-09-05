//! Purpose:
//! Normalizes PCNTL signal-set array values into the contiguous integer bridge ABI.
//!
//! Called from:
//! - `super::pcntl_signals` before signal-mask and synchronous-wait bridge calls.
//!
//! Key details:
//! - Indexed and associative arrays share PHP weak integer coercion, warnings, and deprecations.
//! - Fresh normalization arrays are released on success, OS failure, and catchable validation errors.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, CodegenIrError, Result};
use crate::types::PhpType;

pub(super) fn load_signal_int_array(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
    argument: usize,
) -> Result<bool> {
    let ty = ctx.value_php_type(value)?.codegen_repr();
    ctx.load_value_to_result(value)?;
    match ty {
        PhpType::Array(element) if matches!(&*element, PhpType::Int | PhpType::Never) => {
            Ok(false)
        }
        PhpType::Array(element) if signal_codegen_element_supported(&element) => {
            normalize_indexed_signal_array(ctx, &element, name, argument)?;
            Ok(true)
        }
        PhpType::AssocArray { value: element, .. }
            if signal_codegen_element_supported(&element) =>
        {
            normalize_assoc_signal_array(ctx, &element, name, argument)?;
            Ok(true)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{name} signal array storage {other:?}",
        ))),
    }
}

/// Returns whether one concrete array element representation can be normalized to an integer.
fn signal_codegen_element_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Str
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Never
            | PhpType::TaggedScalar
            | PhpType::Object(_)
            | PhpType::Callable
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

const NORMALIZE_SOURCE_OFFSET: usize = 0;
const NORMALIZE_DEST_OFFSET: usize = 8;
const NORMALIZE_INDEX_OFFSET: usize = 16;
const NORMALIZE_COUNT_OFFSET: usize = 24;
const NORMALIZE_CURSOR_OFFSET: usize = 32;
const NORMALIZE_FRAME_BYTES: usize = 48;

/// Copies an indexed scalar array into fresh contiguous integer storage in source order.
fn normalize_indexed_signal_array(
    ctx: &mut FunctionContext<'_>,
    element: &PhpType,
    name: &str,
    argument: usize,
) -> Result<()> {
    let loop_label = ctx.next_label("pcntl_signal_normalize_indexed_loop");
    let done = ctx.next_label("pcntl_signal_normalize_indexed_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, NORMALIZE_FRAME_BYTES);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NORMALIZE_SOURCE_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the source signal count
            abi::emit_store_to_sp(ctx.emitter, "x9", NORMALIZE_COUNT_OFFSET);
            ctx.emitter.instruction("mov x0, x9");                              // allocate one destination slot per source value
            ctx.emitter.instruction("mov x1, #8");                              // normalized signals use eight-byte integer slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // load the source signal count
            abi::emit_store_to_sp(ctx.emitter, "r9", NORMALIZE_COUNT_OFFSET);
            ctx.emitter.instruction("mov rdi, r9");                             // allocate one destination slot per source value
            ctx.emitter.instruction("mov esi, 8");                              // normalized signals use eight-byte integer slots
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NORMALIZE_DEST_OFFSET);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NORMALIZE_INDEX_OFFSET);
    ctx.emitter.label(&loop_label);
    emit_load_indexed_signal_value(ctx, element, &done);
    emit_loaded_signal_value_as_int(ctx, element, name, argument)?;
    store_normalized_signal_value(ctx);
    abi::emit_jump(ctx.emitter, &loop_label);
    ctx.emitter.label(&done);
    finish_normalized_signal_array(ctx);
    Ok(())
}

/// Loads one indexed source slot into the common `x3/x4` or `rcx/r8` payload registers.
fn emit_load_indexed_signal_value(
    ctx: &mut FunctionContext<'_>,
    element: &PhpType,
    done: &str,
) {
    let wide = matches!(element.codegen_repr(), PhpType::Str | PhpType::TaggedScalar);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", NORMALIZE_INDEX_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", NORMALIZE_COUNT_OFFSET);
            ctx.emitter.instruction("cmp x10, x9");                             // stop after every source value is normalized
            ctx.emitter.instruction(&format!("b.ge {done}"));                   // finish the normalized array
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x11", NORMALIZE_SOURCE_OFFSET);
            ctx.emitter.instruction(&format!("add x11, x11, x10, lsl #{}", if wide { 4 } else { 3 })); // select the source slot
            ctx.emitter.instruction("add x11, x11, #24");                       // skip the indexed-array header
            ctx.emitter.instruction("ldr x3, [x11]");                           // load the source payload low word
            if wide {
                ctx.emitter.instruction("ldr x4, [x11, #8]");                   // load the source payload high word
            }
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", NORMALIZE_INDEX_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", NORMALIZE_COUNT_OFFSET);
            ctx.emitter.instruction("cmp r10, r9");                             // stop after every source value is normalized
            ctx.emitter.instruction(&format!("jge {done}"));                    // finish the normalized array
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", NORMALIZE_SOURCE_OFFSET);
            ctx.emitter.instruction(&format!("shl r10, {}", if wide { 4 } else { 3 })); // scale by the source slot width
            ctx.emitter.instruction("add r11, r10");                            // select the source slot
            ctx.emitter.instruction("mov rcx, QWORD PTR [r11 + 24]");           // load the source payload low word
            if wide {
                ctx.emitter.instruction("mov r8, QWORD PTR [r11 + 32]");        // load the source payload high word
            }
        }
    }
}

/// Copies associative-array values into fresh integer storage while deliberately ignoring keys.
fn normalize_assoc_signal_array(
    ctx: &mut FunctionContext<'_>,
    element: &PhpType,
    name: &str,
    argument: usize,
) -> Result<()> {
    let loop_label = ctx.next_label("pcntl_signal_normalize_assoc_loop");
    let done = ctx.next_label("pcntl_signal_normalize_assoc_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, NORMALIZE_FRAME_BYTES);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NORMALIZE_SOURCE_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_hash_count");
            abi::emit_store_to_sp(ctx.emitter, "x0", NORMALIZE_COUNT_OFFSET);
            ctx.emitter.instruction("mov x1, #8");                              // normalized signals use eight-byte integer slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the source hash to the count helper
            abi::emit_call_label(ctx.emitter, "__rt_hash_count");
            abi::emit_store_to_sp(ctx.emitter, "rax", NORMALIZE_COUNT_OFFSET);
            ctx.emitter.instruction("mov rdi, rax");                            // allocate one slot per associative value
            ctx.emitter.instruction("mov esi, 8");                              // normalized signals use eight-byte integer slots
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NORMALIZE_DEST_OFFSET);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NORMALIZE_INDEX_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NORMALIZE_CURSOR_OFFSET);
    ctx.emitter.label(&loop_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", NORMALIZE_SOURCE_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", NORMALIZE_CURSOR_OFFSET);
            abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
            ctx.emitter.instruction("cmp x0, #-1");                             // detect the insertion-order end sentinel
            ctx.emitter.instruction(&format!("b.eq {done}"));                   // finish after the last associative value
            abi::emit_store_to_sp(ctx.emitter, "x0", NORMALIZE_CURSOR_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", NORMALIZE_SOURCE_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", NORMALIZE_CURSOR_OFFSET);
            abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
            ctx.emitter.instruction("cmp rax, -1");                             // detect the insertion-order end sentinel
            ctx.emitter.instruction(&format!("je {done}"));                     // finish after the last associative value
            abi::emit_store_to_sp(ctx.emitter, "rax", NORMALIZE_CURSOR_OFFSET);
        }
    }
    emit_loaded_signal_value_as_int(ctx, element, name, argument)?;
    store_normalized_signal_value(ctx);
    abi::emit_jump(ctx.emitter, &loop_label);
    ctx.emitter.label(&done);
    finish_normalized_signal_array(ctx);
    Ok(())
}

/// Stores one converted integer and advances the destination index.
fn store_normalized_signal_value(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", NORMALIZE_INDEX_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x11", NORMALIZE_DEST_OFFSET);
            ctx.emitter.instruction("add x11, x11, #24");                       // address the normalized payload base
            ctx.emitter.instruction("str x0, [x11, x10, lsl #3]");              // store the normalized signal value
            ctx.emitter.instruction("add x10, x10, #1");                        // advance the destination index
            abi::emit_store_to_sp(ctx.emitter, "x10", NORMALIZE_INDEX_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", NORMALIZE_INDEX_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", NORMALIZE_DEST_OFFSET);
            ctx.emitter.instruction("mov QWORD PTR [r11 + r10 * 8 + 24], rax"); // store the normalized signal value
            ctx.emitter.instruction("add r10, 1");                              // advance the destination index
            abi::emit_store_to_sp(ctx.emitter, "r10", NORMALIZE_INDEX_OFFSET);
        }
    }
}

/// Publishes the normalized length and returns the fresh indexed-array pointer.
fn finish_normalized_signal_array(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", NORMALIZE_DEST_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", NORMALIZE_INDEX_OFFSET);
            ctx.emitter.instruction("str x9, [x0]");                            // publish the normalized logical length
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", NORMALIZE_DEST_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", NORMALIZE_INDEX_OFFSET);
            ctx.emitter.instruction("mov QWORD PTR [rax], r9");                 // publish the normalized logical length
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, NORMALIZE_FRAME_BYTES);
}

/// Converts one loaded array payload to PHP's weak integer-parameter value.
fn emit_loaded_signal_value_as_int(
    ctx: &mut FunctionContext<'_>,
    element: &PhpType,
    name: &str,
    argument: usize,
) -> Result<()> {
    match element.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Void | PhpType::Never => {
            move_loaded_signal_low_to_result(ctx);
        }
        PhpType::Float => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction("fmov d0, x3"),        // move the loaded signal float into the shared FP result register
                Arch::X86_64 => ctx.emitter.instruction("movq xmm0, rcx"),      // move the loaded signal float into the shared FP result register
            }
            super::strings::emit_signal_float_result_to_int(ctx);
        }
        PhpType::TaggedScalar => {
            let null = ctx.next_label("pcntl_signal_tagged_null");
            let done = ctx.next_label("pcntl_signal_tagged_done");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x4, #8");                      // runtime tag 8 marks null
                    ctx.emitter.instruction(&format!("b.eq {null}"));           // null coerces to zero
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp r8, 8");                       // runtime tag 8 marks null
                    ctx.emitter.instruction(&format!("je {null}"));             // null coerces to zero
                }
            }
            move_loaded_signal_low_to_result(ctx);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&null);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            ctx.emitter.label(&done);
        }
        PhpType::Str => emit_signal_string_to_int(ctx, name, argument),
        PhpType::Object(class_name) => {
            emit_loaded_signal_type_error(ctx, name, argument, &class_name);
        }
        PhpType::Callable => {
            emit_loaded_signal_type_error(ctx, name, argument, "Closure");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_loaded_mixed_signal_value_as_int(ctx, name, argument)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{name} signal scalar coercion for {other:?}",
            )))
        }
    }
    Ok(())
}

/// Releases partial normalization and throws the exact signal element `TypeError`.
fn emit_loaded_signal_type_error(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    argument: usize,
    actual_type: &str,
) {
    release_in_progress_normalized_signal_array(ctx, 0);
    super::super::exceptions::emit_type_error(
        ctx,
        &format!(
            "{name}(): Argument #{argument} ($signals) signals must be of type int, {actual_type} given"
        ),
    );
}

/// Moves the common loaded low payload word into the integer result register.
fn move_loaded_signal_low_to_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x3"),                 // return the loaded scalar payload
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rcx"),                // return the loaded scalar payload
    }
}

/// Unboxes a Mixed signal value and applies the same scalar conversion dispatch.
fn emit_loaded_mixed_signal_value_as_int(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    argument: usize,
) -> Result<()> {
    let from_int = ctx.next_label("pcntl_signal_mixed_int");
    let from_string = ctx.next_label("pcntl_signal_mixed_string");
    let from_float = ctx.next_label("pcntl_signal_mixed_float");
    let from_null = ctx.next_label("pcntl_signal_mixed_null");
    let done = ctx.next_label("pcntl_signal_mixed_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x3");                              // pass the boxed Mixed source to the unbox helper
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            for (tag, label) in [(0, &from_int), (3, &from_int), (1, &from_string), (2, &from_float), (8, &from_null)] {
                ctx.emitter.instruction(&format!("cmp x0, #{tag}"));            // select a PHP integer-coercible runtime tag
                ctx.emitter.instruction(&format!("b.eq {label}"));              // dispatch the selected payload
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rcx");                            // pass the boxed Mixed source to the unbox helper
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            for (tag, label) in [(0, &from_int), (3, &from_int), (1, &from_string), (2, &from_float), (8, &from_null)] {
                ctx.emitter.instruction(&format!("cmp rax, {tag}"));            // select a PHP integer-coercible runtime tag
                ctx.emitter.instruction(&format!("je {label}"));                // dispatch the selected payload
            }
        }
    }
    release_in_progress_normalized_signal_array(ctx, 0);
    super::super::exceptions::emit_type_error(
        ctx,
        &format!(
            "{name}(): Argument #{argument} ($signals) signals must be of type int, invalid value given"
        ),
    );
    ctx.emitter.label(&from_int);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // use the unboxed integer or boolean payload
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // use the unboxed integer or boolean payload
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_string);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x1");                              // move the unboxed string pointer into the shared payload register
            ctx.emitter.instruction("mov x4, x2");                              // move the unboxed string length into the shared payload register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rdi");                            // move the unboxed string pointer into the shared payload register
            ctx.emitter.instruction("mov r8, rdx");                             // move the unboxed string length into the shared payload register
        }
    }
    emit_signal_string_to_int(ctx, name, argument);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_float);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("fmov d0, x1"),                // move the unboxed float payload into the shared FP result register
        Arch::X86_64 => ctx.emitter.instruction("movq xmm0, rdi"),              // move the unboxed float payload into the shared FP result register
    }
    super::strings::emit_signal_float_result_to_int(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_null);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done);
    Ok(())
}

const STRING_COERCE_PTR_OFFSET: usize = 0;
const STRING_COERCE_LEN_OFFSET: usize = 8;
const STRING_COERCE_INT_OFFSET: usize = 16;
const STRING_COERCE_FULL_OFFSET: usize = 24;
const STRING_COERCE_FRAME_BYTES: usize = 32;

/// Coerces a loaded PHP string to int, emitting PHP's leading-numeric warning and precision deprecation.
fn emit_signal_string_to_int(ctx: &mut FunctionContext<'_>, name: &str, argument: usize) {
    let numeric = ctx.next_label("pcntl_signal_string_numeric");
    let fully_numeric = ctx.next_label("pcntl_signal_string_fully_numeric");
    let done = ctx.next_label("pcntl_signal_string_coerce_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, STRING_COERCE_FRAME_BYTES);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_store_to_sp(ctx.emitter, "x3", STRING_COERCE_PTR_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, "x4", STRING_COERCE_LEN_OFFSET);
            ctx.emitter.instruction("mov x1, x3");                              // pass the original string pointer to the bounded C-string helper
            ctx.emitter.instruction("mov x2, x4");                              // pass the original string length to the bounded C-string helper
            abi::emit_call_label(ctx.emitter, "__rt_cstr");
            abi::emit_call_label(ctx.emitter, "__rt_php_num_scan");
            abi::emit_store_to_sp(ctx.emitter, "x1", STRING_COERCE_FULL_OFFSET);
            ctx.emitter.instruction("ldrb w9, [x0]");                           // test whether any leading numeric run exists
            ctx.emitter.instruction(&format!("cbnz w9, {numeric}"));            // numeric prefixes follow weak PHP coercion
        }
        Arch::X86_64 => {
            abi::emit_store_to_sp(ctx.emitter, "rcx", STRING_COERCE_PTR_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, "r8", STRING_COERCE_LEN_OFFSET);
            ctx.emitter.instruction("mov rax, rcx");                            // pass the original string pointer to the bounded C-string helper
            ctx.emitter.instruction("mov rdx, r8");                             // pass the original string length to the bounded C-string helper
            abi::emit_call_label(ctx.emitter, "__rt_cstr");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the writable C string to PHP's numeric scanner
            abi::emit_call_label(ctx.emitter, "__rt_php_num_scan");
            abi::emit_store_to_sp(ctx.emitter, "rdx", STRING_COERCE_FULL_OFFSET);
            ctx.emitter.instruction("cmp BYTE PTR [rax], 0");                   // test whether any leading numeric run exists
            ctx.emitter.instruction(&format!("jne {numeric}"));                 // numeric prefixes follow weak PHP coercion
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, STRING_COERCE_FRAME_BYTES);
    release_in_progress_normalized_signal_array(ctx, 0);
    super::super::exceptions::emit_type_error(
        ctx,
        &format!(
            "{name}(): Argument #{argument} ($signals) signals must be of type int, string given"
        ),
    );
    ctx.emitter.label(&numeric);
    reload_signal_string_result(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), STRING_COERCE_INT_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), STRING_COERCE_FULL_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbnz {}, {fully_numeric}", abi::secondary_scratch_reg(ctx.emitter))), // fully numeric strings skip the warning
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", abi::secondary_scratch_reg(ctx.emitter), abi::secondary_scratch_reg(ctx.emitter))); // inspect PHP's fully-numeric flag
            ctx.emitter.instruction(&format!("jnz {fully_numeric}"));           // fully numeric strings skip the warning
        }
    }
    emit_static_signal_diagnostic(ctx, "Warning: A non-numeric value encountered\n");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fully_numeric);
    reload_signal_string_result(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
    emit_signal_float_string_precision_check(ctx, &done);
    emit_signal_float_string_deprecation(ctx);
    ctx.emitter.label(&done);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), STRING_COERCE_INT_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, STRING_COERCE_FRAME_BYTES);
}

/// Reloads the saved signal string into the target's canonical string-result registers.
fn reload_signal_string_result(ctx: &mut FunctionContext<'_>) {
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, ptr, STRING_COERCE_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, len, STRING_COERCE_LEN_OFFSET);
}

/// Branches past the deprecation when the numeric string's float value is exactly integral.
fn emit_signal_float_string_precision_check(ctx: &mut FunctionContext<'_>, done: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", STRING_COERCE_INT_OFFSET);
            ctx.emitter.instruction("scvtf d1, x9");                            // convert the truncated integer back to double
            ctx.emitter.instruction("fcmp d0, d1");                             // did truncation lose fractional precision?
            ctx.emitter.instruction(&format!("b.eq {done}"));                   // exact integral float strings emit no deprecation
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", STRING_COERCE_INT_OFFSET);
            ctx.emitter.instruction("cvtsi2sd xmm1, r10");                      // convert the truncated integer back to double
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // did truncation lose fractional precision?
            ctx.emitter.instruction(&format!("je {done}"));                     // exact integral float strings emit no deprecation
        }
    }
}

/// Emits PHP's exact float-string precision-loss deprecation with the original value quoted.
fn emit_signal_float_string_deprecation(ctx: &mut FunctionContext<'_>) {
    emit_static_signal_diagnostic(ctx, "Deprecated: Implicit conversion from float-string \"");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", STRING_COERCE_PTR_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", STRING_COERCE_LEN_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", STRING_COERCE_PTR_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", STRING_COERCE_LEN_OFFSET);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    emit_static_signal_diagnostic(ctx, "\" to int loses precision\n");
}

/// Emits one suppressible static PCNTL coercion diagnostic fragment.
fn emit_static_signal_diagnostic(ctx: &mut FunctionContext<'_>, message: &str) {
    let (label, len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Records whether the currently returned signal array must be released after its bridge call.
pub(super) fn store_normalized_signal_array(ctx: &mut FunctionContext<'_>, normalized: bool, offset: usize) {
    if normalized {
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    }
}

/// Releases a temporary normalized signal array without touching borrowed typed input arrays.
pub(super) fn release_normalized_signal_array(ctx: &mut FunctionContext<'_>, normalized: bool, offset: usize) {
    if !normalized {
        return;
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
}

/// Releases the destination being built when scalar coercion throws before normalization completes.
fn release_in_progress_normalized_signal_array(ctx: &mut FunctionContext<'_>, extra_offset: usize) {
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        extra_offset + NORMALIZE_DEST_OFFSET,
    );
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
}
