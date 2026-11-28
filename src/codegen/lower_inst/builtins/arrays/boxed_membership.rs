//! Purpose:
//! Lowers membership over boxed arrays and values outside the scalar fast paths.
//!
//! Called from:
//! - `super::callback_builtins::lower_in_array_with_mode()`.
//! - `super::boxed_reduce::lower_array_reduce()` for borrowed argument cells.
//!
//! Key details:
//! - Stack cells borrow EIR operands and never acquire or release their payloads.
//! - Runtime validation rejects non-arrays before reading container metadata.

use super::*;

/// Keeps the existing concrete scalar fast paths and selects generic comparison otherwise.
pub(super) fn needs_dynamic_membership(needle: &PhpType, array: &PhpType) -> bool {
    let scalar = |ty: &PhpType| matches!(ty.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::Str);
    if !scalar(needle) { return true; }
    match array.codegen_repr() {
        PhpType::Array(element) => !scalar(&element),
        PhpType::AssocArray { value, .. } => !scalar(&value),
        _ => true,
    }
}

/// Invokes the borrowed-cell scanner and turns an invalid haystack into a catchable TypeError.
pub(super) fn lower_dynamic_membership(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    mode: InArrayMode,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, 64);
    store_borrowed_cell(ctx, needle, 0)?;
    store_borrowed_cell(ctx, array, 32)?;
    for (index, offset) in [(0, 0), (1, 32)] {
        abi::emit_temporary_stack_address(
            ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), offset,
        );
    }
    abi::emit_load_int_immediate(
        ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2),
        i64::from(matches!(mode, InArrayMode::Strict)),
    );
    abi::emit_call_label(ctx.emitter, "__rt_in_array_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    let valid = ctx.next_label("in_array_boxed_valid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // only invalid haystacks return a negative sentinel
            ctx.emitter.instruction(&format!("b.ge {valid}"));                  // both boolean results are valid
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // distinguish the invalid sentinel from false and true
            ctx.emitter.instruction(&format!("jns {valid}"));                   // preserve the helper's boolean result
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, "in_array(): Argument #2 ($haystack) must be of type array",
    );
    ctx.emitter.label(&valid);
    Ok(())
}

/// `array_search()` over boxed arrays and values outside the scalar fast paths.
///
/// The scan is `in_array()`'s own (`__rt_array_search_boxed` is the same emitter keeping each
/// element's key), so the two builtins cannot disagree about which element is equal. `$strict`
/// is passed through at run time: with mixed elements it is observable (`"1"` loosely equals `1`).
/// A hit boxes the borrowed key — `__rt_mixed_from_value` copies a string key — and a miss boxes
/// false, the `int|string|false` php returns.
pub(super) fn lower_dynamic_array_search(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    strict: Option<ValueId>,
) -> Result<()> {
    const STRICT_SLOT: usize = 64;
    const STACK_BYTES: usize = 80;
    abi::emit_reserve_temporary_stack(ctx.emitter, STACK_BYTES);
    store_borrowed_cell(ctx, needle, 0)?;
    store_borrowed_cell(ctx, array, 32)?;
    let result = abi::int_result_reg(ctx.emitter);
    match strict {
        Some(strict) => {
            ctx.load_value_to_result(strict)?;
        }
        None => abi::emit_load_int_immediate(ctx.emitter, result, 0),
    }
    abi::emit_store_to_sp(ctx.emitter, result, STRICT_SLOT);
    for (index, offset) in [(0, 0), (1, 32)] {
        abi::emit_temporary_stack_address(
            ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), offset,
        );
    }
    abi::emit_load_temporary_stack_slot(
        ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), STRICT_SLOT,
    );
    abi::emit_call_label(ctx.emitter, "__rt_array_search_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, STACK_BYTES);
    let valid = ctx.next_label("array_search_boxed_valid");
    let miss = ctx.next_label("array_search_boxed_miss");
    let string_key = ctx.next_label("array_search_boxed_string_key");
    let done = ctx.next_label("array_search_boxed_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // only invalid haystacks return a negative sentinel
            ctx.emitter.instruction(&format!("b.ge {valid}"));                  // a miss or a hit is a valid scan
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // distinguish the invalid sentinel from a miss and a hit
            ctx.emitter.instruction(&format!("jns {valid}"));                   // a miss or a hit is a valid scan
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, "array_search(): Argument #2 ($haystack) must be of type array",
    );
    ctx.emitter.label(&valid);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {miss}"));                // zero reports that no element was equal
            ctx.emitter.instruction("cmn x2, #1");                              // a high word of -1 marks an integer key
            ctx.emitter.instruction(&format!("b.ne {string_key}"));             // string keys box with the string tag
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer key
            ctx.emitter.instruction("mov x2, xzr");                             // integer Mixed payloads do not use a high word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {done}"));                      // skip the string and miss boxing paths
            ctx.emitter.label(&string_key);
            ctx.emitter.instruction("mov x0, #1");                              // runtime tag 1 = string key, x1/x2 = pointer/length
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {done}"));                      // skip the miss boxing path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // zero reports that no element was equal
            ctx.emitter.instruction(&format!("je {miss}"));                     // box false for a miss
            ctx.emitter.instruction("cmp rdx, -1");                             // a high word of -1 marks an integer key
            ctx.emitter.instruction(&format!("jne {string_key}"));              // string keys box with the string tag
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer key, rdi = key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // skip the string and miss boxing paths
            ctx.emitter.label(&string_key);
            ctx.emitter.instruction("mov rsi, rdx");                            // move the string-key length into the Mixed high word
            ctx.emitter.instruction("mov eax, 1");                              // runtime tag 1 = string key, rdi = pointer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // skip the miss boxing path
        }
    }
    ctx.emitter.label(&miss);
    super::box_array_search_miss(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Writes a borrowed tag/payload triple without allocating a managed Mixed cell.
pub(super) fn store_borrowed_cell(ctx: &mut FunctionContext<'_>, value: ValueId, offset: usize) -> Result<()> {
    let ty = ctx.value_php_type(value)?.codegen_repr();
    ctx.load_value_to_result(value)?;
    let result = abi::int_result_reg(ctx.emitter);
    let tag = abi::secondary_scratch_reg(ctx.emitter);
    let (string_lo, string_hi) = abi::string_result_regs(ctx.emitter);
    if ty == PhpType::Str {
        abi::emit_store_to_sp(ctx.emitter, string_lo, offset + 8);
        abi::emit_store_to_sp(ctx.emitter, string_hi, offset + 16);
    } else {
        if ty == PhpType::Float {
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction("fmov x0, d0"),        // preserve the float bit pattern without numerical conversion
                Arch::X86_64 => ctx.emitter.instruction("movq rax, xmm0"),      // write IEEE bits to the borrowed cell's payload
            }
        }
        abi::emit_store_to_sp(ctx.emitter, result, offset + 8);
        abi::emit_load_int_immediate(ctx.emitter, tag, 0);
        abi::emit_store_to_sp(ctx.emitter, tag, offset + 16);
    }
    if ty == PhpType::TaggedScalar {
        let scalar_tag = match ctx.emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdx" };
        abi::emit_store_to_sp(ctx.emitter, scalar_tag, offset);
        return Ok(());
    }
    abi::emit_load_int_immediate(ctx.emitter, tag, crate::codegen::runtime_value_tag(&ty) as i64);
    if ty == PhpType::Iterable || matches!(&ty, PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed) {
        let ready = ctx.next_label("borrowed_array_tag_ready");
        abi::emit_load_int_immediate(ctx.emitter, tag, 4);
        crate::codegen_support::sentinels::emit_branch_if_null_container(
            ctx.emitter, result, abi::tertiary_scratch_reg(ctx.emitter), &ready,
        );
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x11, [x0, #-8]");                  // inspect the actual container heap kind
                ctx.emitter.instruction("and x11, x11, #255");                  // discard element tags and persistence flags
                ctx.emitter.instruction("cmp x11, #3");                         // promoted arrays use hash storage
                ctx.emitter.instruction("mov x11, #5");                         // the Mixed runtime tag for a hash payload
                ctx.emitter.instruction("csel x10, x11, x10, eq");              // preserve packed storage unless this is a hash
                if ty == PhpType::Iterable {
                    ctx.emitter.instruction("ldr x11, [x0, #-8]");              // iterable payloads can also be iterator objects
                    ctx.emitter.instruction("and x11, x11, #255");              // extract the object's heap kind
                    ctx.emitter.instruction("cmp x11, #4");                     // distinguish objects from array payloads
                    ctx.emitter.instruction("mov x11, #6");                     // retain object comparison semantics for iterators
                    ctx.emitter.instruction("csel x10, x11, x10, eq");          // select the concrete borrowed object tag
                }
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov r11, QWORD PTR [rax - 8]");        // inspect the actual container heap kind
                ctx.emitter.instruction("and r11, 255");                        // discard element tags and persistence flags
                ctx.emitter.instruction("cmp r11, 3");                          // promoted arrays use hash storage
                ctx.emitter.instruction("mov r11, 5");                          // the Mixed runtime tag for a hash payload
                ctx.emitter.instruction("cmove r10, r11");                      // preserve packed storage unless this is a hash
                if ty == PhpType::Iterable {
                    ctx.emitter.instruction("mov r11, QWORD PTR [rax - 8]");    // iterable payloads can also be iterator objects
                    ctx.emitter.instruction("and r11, 255");                    // extract the object's heap kind
                    ctx.emitter.instruction("cmp r11, 4");                      // distinguish objects from array payloads
                    ctx.emitter.instruction("mov r11, 6");                      // retain object comparison semantics for iterators
                    ctx.emitter.instruction("cmove r10, r11");                  // select the concrete borrowed object tag
                }
            }
        }
        ctx.emitter.label(&ready);
    }
    abi::emit_store_to_sp(ctx.emitter, tag, offset);
    Ok(())
}
