//! Purpose:
//! Array pop and array_search target-specific lowering.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.
//! - A removed live slot transfers its owner into the result after COW separation.

use super::*;

/// Returns the runtime value_type tag used by the array-to-Mixed widening helper.
pub(super) fn runtime_value_tag(name: &str, elem: &PhpType) -> Result<u8> {
    match elem {
        PhpType::Int => Ok(0),
        PhpType::Str => Ok(1),
        PhpType::Float => Ok(2),
        PhpType::Bool => Ok(3),
        PhpType::Array(_) => Ok(4),
        PhpType::AssocArray { .. } => Ok(5),
        PhpType::Object(_) => Ok(6),
        PhpType::Mixed => Ok(7),
        PhpType::Void => Ok(8),
        PhpType::Callable => Ok(10),
        other => Err(CodegenIrError::unsupported(format!(
            "{} Mixed widening for element PHP type {:?}",
            name, other
        ))),
    }
}

/// Verifies a builtin receives an indexed array operand.
pub(super) fn require_indexed_array_builtin(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Array(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Returns the supported element payload type for an indexed-array `array_pop()`.
pub(super) fn array_pop_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Str
                    | PhpType::Callable
                    | PhpType::Mixed
                    | PhpType::Void
                    | PhpType::Never
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_pop indexed-array element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_pop for PHP type {:?}",
            other
        ))),
    }
}

/// Verifies the lowered `array_pop()` result uses PHP's `mixed` shape.
pub(super) fn require_array_pop_result_type(result_ty: &PhpType) -> Result<()> {
    if result_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_pop result PHP type {:?}",
        result_ty
    )))
}

/// Splits a shared indexed array before `array_pop()` mutates its header.
pub(super) fn ensure_unique_array_pop_source(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.store_result_value(array)
}

/// Emits the AArch64 `array_pop()` sequence for indexed arrays.
pub(super) fn lower_array_pop_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let empty_label = ctx.next_label("array_pop_empty");
    let done_label = ctx.next_label("array_pop_done");
    ctx.load_value_to_reg(array, "x0")?;
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the indexed-array length before deciding whether pop is empty
    ctx.emitter.instruction(&format!("cbz x9, {}", empty_label));               // return boxed null when array_pop() runs on an empty array
    ctx.emitter.instruction("sub x9, x9, #1");                                  // convert the old length into the removed last-element index
    ctx.emitter.instruction("str x9, [x0]");                                    // persist the shortened indexed-array length in the header
    emit_array_pop_value_aarch64(ctx, elem_ty)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, elem_ty);
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-array boxed-null path after loading the removed value
    ctx.emitter.label(&empty_label);
    emit_array_pop_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 `array_pop()` sequence for indexed arrays.
pub(super) fn lower_array_pop_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let empty_label = ctx.next_label("array_pop_empty");
    let done_label = ctx.next_label("array_pop_done");
    ctx.load_value_to_reg(array, "rax")?;
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the indexed-array length before deciding whether pop is empty
    ctx.emitter.instruction("test r10, r10");                                   // check whether the indexed array has any live elements
    ctx.emitter.instruction(&format!("jz {}", empty_label));                    // return boxed null when array_pop() runs on an empty array
    ctx.emitter.instruction("sub r10, 1");                                      // convert the old length into the removed last-element index
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // persist the shortened indexed-array length in the header
    emit_array_pop_value_x86_64(ctx, elem_ty)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, elem_ty);
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-array boxed-null path after loading the removed value
    ctx.emitter.label(&empty_label);
    emit_array_pop_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Loads the removed AArch64 indexed-array payload into the canonical result registers.
pub(super) fn emit_array_pop_value_aarch64(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) -> Result<()> {
    match elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first pointer-sized payload slot in the indexed array
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load the removed pointer-sized payload into the result register
        }
        PhpType::Float => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first float payload slot in the indexed array
            ctx.emitter.instruction("ldr d0, [x10, x9, lsl #3]");               // load the removed float payload into the result register
        }
        PhpType::Str => {
            ctx.emitter.instruction("lsl x10, x9, #4");                         // scale the removed index by the 16-byte string slot size
            ctx.emitter.instruction("add x10, x0, x10");                        // advance from the array base to the removed string slot
            ctx.emitter.instruction("add x10, x10, #24");                       // skip the indexed-array header before loading string payloads
            ctx.emitter.instruction("ldr x1, [x10]");                           // load the removed string pointer into the mixed payload register
            ctx.emitter.instruction("ldr x2, [x10, #8]");                       // load the removed string length into the mixed payload high word
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x0, #0");                              // materialize a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first refcounted payload slot in the indexed array
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load the removed heap pointer into the result register
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_pop element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Loads the removed x86_64 indexed-array payload into the canonical result registers.
pub(super) fn emit_array_pop_value_x86_64(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) -> Result<()> {
    match elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first pointer-sized payload slot in the indexed array
            ctx.emitter
                .instruction("mov rax, QWORD PTR [r11 + r10 * 8]"); // load the removed pointer-sized payload into the result register
        }
        PhpType::Float => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first float payload slot in the indexed array
            ctx.emitter
                .instruction("movsd xmm0, QWORD PTR [r11 + r10 * 8]"); // load the removed float payload into the result register
        }
        PhpType::Str => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first string payload slot in the indexed array
            ctx.emitter.instruction("shl r10, 4");                              // scale the removed index by the 16-byte string slot size
            ctx.emitter.instruction("add r11, r10");                            // advance to the removed string slot payload
            ctx.emitter.instruction("mov rax, QWORD PTR [r11]");                // load the removed string pointer into the string result register
            ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");            // load the removed string length into the string result register
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor eax, eax");                            // materialize a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first refcounted payload slot in the indexed array
            ctx.emitter
                .instruction("mov rax, QWORD PTR [r11 + r10 * 8]"); // load the removed heap pointer into the result register
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_pop element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Boxes PHP null for an empty `array_pop()` result.
pub(super) fn emit_array_pop_null(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}


/// Describes which indexed-array `array_search()` lowering path applies.
pub(super) enum ArraySearchCase {
    Empty,
    Scalar,
    String,
}

/// Verifies that an indexed-array `array_search()` call can use the scalar search helper.
pub(super) fn supported_array_search_case(needle_ty: PhpType, array_ty: PhpType) -> Result<ArraySearchCase> {
    let needle_ty = needle_ty.codegen_repr();
    match array_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Never | PhpType::Void => Ok(ArraySearchCase::Empty),
            PhpType::Int | PhpType::Bool if matches!(needle_ty, PhpType::Int | PhpType::Bool) => {
                Ok(ArraySearchCase::Scalar)
            }
            PhpType::Str if needle_ty == PhpType::Str => Ok(ArraySearchCase::String),
            elem_ty => Err(CodegenIrError::unsupported(format!(
                "array_search needle PHP type {:?} for indexed-array element PHP type {:?}",
                needle_ty, elem_ty
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "array_search for PHP array type {:?}",
            other
        ))),
    }
}

/// Lowers integer-like indexed-array search and boxes the PHP `int|false` result.
pub(super) fn lower_array_search_scalar(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(needle, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(needle, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_search");
    box_array_search_result(ctx);
    Ok(())
}

/// Lowers string indexed-array search and boxes the PHP `int|false` result.
pub(super) fn lower_array_search_string(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_search_string_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_array_search_string_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 string-array search loop.
pub(super) fn lower_array_search_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("array_search_str_loop");
    let found_label = ctx.next_label("array_search_str_found");
    let miss_label = ctx.next_label("array_search_str_miss");
    let done_label = ctx.next_label("array_search_str_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed string-array payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the string search at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", miss_label));                   // finish with false after all string elements are scanned
    ctx.emitter.instruction("lsl x13, x12, #4");                                // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // load the current string element pointer for comparison
    ctx.emitter.instruction("add x14, x13, #8");                                // compute the current string element length-slot offset
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    ctx.load_string_value_to_regs(needle, "x3", "x4")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter
        .instruction(&format!("cbnz x0, {}", found_label)); // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed string element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x1, x12");                                     // move the found index into the mixed helper payload register
    ctx.emitter.instruction("mov x2, #0");                                      // integer mixed payloads do not use a high word
    ctx.emitter.instruction("mov x0, #0");                                      // runtime tag 0 = integer
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip false boxing after producing the found index
    ctx.emitter.label(&miss_label);
    box_array_search_miss(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 string-array search loop.
pub(super) fn lower_array_search_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("array_search_str_loop");
    let found_label = ctx.next_label("array_search_str_found");
    let miss_label = ctx.next_label("array_search_str_miss");
    let done_label = ctx.next_label("array_search_str_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("lea r12, [r10 + 24]");                             // point at the first indexed string-array payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the string search at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", miss_label));                    // finish with false after all string elements are scanned
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the scan index before scaling it to a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("mov rdi, QWORD PTR [r12 + rcx]");                  // load the current string element pointer for comparison
    ctx.emitter
        .instruction("mov rsi, QWORD PTR [r12 + rcx + 8]"); // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // check whether the current string element matched the needle
    ctx.emitter.instruction(&format!("jne {}", found_label));                   // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed string element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rdi, r13");                                    // move the found index into the mixed helper payload register
    ctx.emitter.instruction("xor esi, esi");                                    // integer mixed payloads do not use a high word
    ctx.emitter.instruction("xor eax, eax");                                    // runtime tag 0 = integer
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip false boxing after producing the found index
    ctx.emitter.label(&miss_label);
    box_array_search_miss(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Boxes a raw array-search helper result into PHP `int|false` Mixed form.
pub(super) fn box_array_search_result(ctx: &mut FunctionContext<'_>) {
    let found_label = ctx.next_label("array_search_found");
    let end_label = ctx.next_label("array_search_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // distinguish a found index from the array_search() not-found sentinel
            ctx.emitter.instruction(&format!("b.ge {}", found_label));          // box a found index as an integer mixed result
            box_array_search_miss(ctx);
            ctx.emitter.instruction(&format!("b {}", end_label));               // skip integer boxing after producing false for a miss
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov x1, x0");                              // move the found index into the mixed helper payload register
            ctx.emitter.instruction("mov x2, #0");                              // integer mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // distinguish a found index from the array_search() not-found sentinel
            ctx.emitter.instruction(&format!("jge {}", found_label));           // box a found index as an integer mixed result
            box_array_search_miss(ctx);
            ctx.emitter.instruction(&format!("jmp {}", end_label));             // skip integer boxing after producing false for a miss
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov rdi, rax");                            // move the found index into the mixed helper payload register
            ctx.emitter.instruction("xor esi, esi");                            // integer mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
    }
}

/// Boxes `false` for an array-search miss.
pub(super) fn box_array_search_miss(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #0");                              // false mixed payload is zero
            ctx.emitter.instruction("mov x2, #0");                              // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = bool
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor edi, edi");                            // false mixed payload is zero
            ctx.emitter.instruction("xor esi, esi");                            // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = bool
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}
