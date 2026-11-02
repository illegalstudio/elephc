//! Purpose:
//! Boolean, string, and Mixed-array membership loops.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Lowers strict or loose membership of a gradual needle in a typed indexed container whose
/// elements require boxed runtime comparison (nested arrays, hashes, objects, or resources).
///
/// The shared runtime scanner already obtains each element through the generic array accessor,
/// preserving PHP's recursive array equality and ownership behavior. A typed indexed source can
/// enter that scanner directly by passing its raw pointer and runtime kind instead of first
/// manufacturing a transient boxed container.
pub(super) fn lower_in_array_typed_container_with_mixed_needle(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    mode: InArrayMode,
) -> Result<()> {
    let needle_ty = ctx.value_php_type(needle)?.codegen_repr();
    if !matches!(needle_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "typed mixed-container in_array needle PHP type {:?}",
            needle_ty
        )));
    }
    let container_ty = ctx.value_php_type(array)?.codegen_repr();
    if !matches!(container_ty, PhpType::Array(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "typed mixed-container in_array PHP type {:?}",
            container_ty
        )));
    }

    ctx.load_value_to_result(needle)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                            // typed indexed payload becomes the scanner container
            abi::emit_pop_reg(ctx.emitter, "x0");                             // restore the borrowed boxed Mixed needle
            abi::emit_load_int_immediate(ctx.emitter, "x2", 4);               // runtime kind 4 = indexed array
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x3",
                i64::from(matches!(mode, InArrayMode::Strict)),
            );
            abi::emit_load_int_immediate(ctx.emitter, "x4", 0);               // in_array returns a boolean, never the matching key
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                          // typed indexed payload becomes the scanner container
            abi::emit_pop_reg(ctx.emitter, "rdi");                            // restore the borrowed boxed Mixed needle
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 4);              // runtime kind 4 = indexed array
            abi::emit_load_int_immediate(
                ctx.emitter,
                "rcx",
                i64::from(matches!(mode, InArrayMode::Strict)),
            );
            abi::emit_load_int_immediate(ctx.emitter, "r8", 0);               // in_array returns a boolean, never the matching key
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_in_array_mixed_container");
    Ok(())
}

/// Lowers `array_search()` over an indexed or associative container held in boxed gradual storage.
pub(super) fn lower_array_search_mixed_container(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: &PhpType,
    mode: InArrayMode,
) -> Result<()> {
    lower_mixed_container_search(ctx, needle, array, needle_ty, mode, true)
}

/// Validates, unboxes, and scans one gradual array container for membership or its matching key.
fn lower_mixed_container_search(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: &PhpType,
    mode: InArrayMode,
    return_key: bool,
) -> Result<()> {
    let indexed_label = ctx.next_label("in_array_dynamic_indexed");
    let hash_label = ctx.next_label("in_array_dynamic_hash");
    let setup_label = ctx.next_label("in_array_dynamic_setup");
    let wrong_tag_label = ctx.next_label("in_array_dynamic_wrong_tag");

    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // tag 4 selects an indexed-array payload
            ctx.emitter.instruction(&format!("b.eq {}", indexed_label));        // preserve the unboxed indexed pointer for setup
            ctx.emitter.instruction("cmp x0, #5");                              // tag 5 selects an associative-array payload
            ctx.emitter.instruction(&format!("b.eq {}", hash_label));           // preserve the unboxed hash pointer for setup
            ctx.emitter.instruction(&format!("b {}", wrong_tag_label));         // every other runtime value violates the array parameter
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // tag 4 selects an indexed-array payload
            ctx.emitter.instruction(&format!("je {}", indexed_label));          // preserve the unboxed indexed pointer for setup
            ctx.emitter.instruction("cmp rax, 5");                              // tag 5 selects an associative-array payload
            ctx.emitter.instruction(&format!("je {}", hash_label));             // preserve the unboxed hash pointer for setup
            ctx.emitter.instruction(&format!("jmp {}", wrong_tag_label));       // every other runtime value violates the array parameter
        }
    }
    super::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_tag_label,
        &|given| {
            format!(
                "{}(): Argument #2 ($haystack) must be of type array, {} given",
                if return_key { "array_search" } else { "in_array" },
                given
            )
        },
    );

    ctx.emitter.label(&indexed_label);
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_store_to_sp(ctx.emitter, "x1", 0);                     // preserve the unboxed indexed-array pointer
            abi::emit_load_int_immediate(ctx.emitter, "x9", 4);
            abi::emit_store_to_sp(ctx.emitter, "x9", 8);                     // record indexed runtime kind for the iterator helper
        }
        Arch::X86_64 => {
            abi::emit_store_to_sp(ctx.emitter, "rdi", 0);                    // preserve the unboxed indexed-array pointer
            abi::emit_load_int_immediate(ctx.emitter, "r9", 4);
            abi::emit_store_to_sp(ctx.emitter, "r9", 8);                     // record indexed runtime kind for the iterator helper
        }
    }
    abi::emit_jump(ctx.emitter, &setup_label);

    ctx.emitter.label(&hash_label);
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_store_to_sp(ctx.emitter, "x1", 0);                     // preserve the unboxed associative-array pointer
            abi::emit_load_int_immediate(ctx.emitter, "x9", 5);
            abi::emit_store_to_sp(ctx.emitter, "x9", 8);                     // record associative runtime kind for the iterator helper
        }
        Arch::X86_64 => {
            abi::emit_store_to_sp(ctx.emitter, "rdi", 0);                    // preserve the unboxed associative-array pointer
            abi::emit_load_int_immediate(ctx.emitter, "r9", 5);
            abi::emit_store_to_sp(ctx.emitter, "r9", 8);                     // record associative runtime kind for the iterator helper
        }
    }

    ctx.emitter.label(&setup_label);
    let needle_repr = needle_ty.codegen_repr();
    let borrowed_needle = matches!(&needle_repr, PhpType::Mixed | PhpType::Union(_));
    ctx.load_value_to_result(needle)?;
    emit_box_current_value_as_mixed(ctx.emitter, &needle_repr);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x3",
                i64::from(matches!(mode, InArrayMode::Strict)),
            );
            abi::emit_load_int_immediate(ctx.emitter, "x4", i64::from(return_key));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 8);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "rcx",
                i64::from(matches!(mode, InArrayMode::Strict)),
            );
            abi::emit_load_int_immediate(ctx.emitter, "r8", i64::from(return_key));
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_in_array_mixed_container");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 24);
    if !borrowed_needle {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            16,
        );
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    }
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        24,
    );
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Scans a bool array against a precomputed AArch64 boolean needle register.
pub(super) fn lower_in_array_bool_array_with_preloaded_needle_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    needle_reg: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_bool_loop");
    let found_label = ctx.next_label("in_array_bool_found");
    let end_label = ctx.next_label("in_array_bool_end");
    let done_label = ctx.next_label("in_array_bool_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed bool-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed bool payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the bool membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all bool elements are scanned
    ctx.emitter.instruction("ldr x13, [x10, x12, lsl #3]");                     // load the current bool element
    ctx.emitter.instruction(&format!("cmp x13, {}", needle_reg));               // compare element bool against needle bool
    ctx.emitter.instruction(&format!("b.eq {}", found_label));                  // stop as soon as a loosely equal bool is found
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed bool element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining bool payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a matching bool
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no bool element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Scans a bool array against a precomputed x86_64 boolean needle register.
pub(super) fn lower_in_array_bool_array_with_preloaded_needle_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    needle_reg: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_bool_loop");
    let found_label = ctx.next_label("in_array_bool_found");
    let end_label = ctx.next_label("in_array_bool_end");
    let done_label = ctx.next_label("in_array_bool_done");

    ctx.load_value_to_reg(array, "r11")?;
    ctx.emitter.instruction("mov r12, QWORD PTR [r11]");                        // load indexed bool-array length before scanning payload slots
    ctx.emitter.instruction("lea r11, [r11 + 24]");                             // point at the first indexed bool payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the bool membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r12");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all bool elements are scanned
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + r13*8]");                // load the current bool element
    ctx.emitter.instruction(&format!("cmp rax, {}", needle_reg));               // compare element bool against needle bool
    ctx.emitter.instruction(&format!("je {}", found_label));                    // stop as soon as a loosely equal bool is found
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed bool element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining bool payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a matching bool
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no bool element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers string indexed-array membership with a linear scan and the selected string equality helper.
pub(super) fn lower_in_array_string(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_string_aarch64(ctx, needle, array, eq_helper),
        Arch::X86_64 => lower_in_array_string_x86_64(ctx, needle, array, eq_helper),
    }
}

/// Emits the AArch64 string-array membership loop.
pub(super) fn lower_in_array_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_loop");
    let found_label = ctx.next_label("in_array_str_found");
    let end_label = ctx.next_label("in_array_str_end");
    let done_label = ctx.next_label("in_array_str_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed string-array payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all string elements are scanned
    ctx.emitter.instruction("lsl x13, x12, #4");                                // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // load the current string element pointer for comparison
    ctx.emitter.instruction("add x14, x13, #8");                                // compute the current string element length-slot offset
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    ctx.load_string_value_to_regs(needle, "x3", "x4")?;
    abi::emit_call_label(ctx.emitter, eq_helper);
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter
        .instruction(&format!("cbnz x0, {}", found_label));                     // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed string element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding the searched string
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no indexed string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a string-needle membership scan over an indexed `array<Mixed>`.
///
/// Each 8-byte slot holds a boxed Mixed cell, so every cell is unboxed and the string-tagged ones
/// are compared with the selected string equality helper, mirroring the concrete string-array path.
pub(super) fn lower_in_array_mixed_string(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_mixed_string_aarch64(ctx, needle, array, eq_helper),
        Arch::X86_64 => lower_in_array_mixed_string_x86_64(ctx, needle, array, eq_helper),
    }
}

/// Lowers an integer-needle membership scan over boxed-Mixed indexed-array slots.
///
/// The runtime helper dispatches on each cell's tag so loose mode preserves PHP
/// numeric-string, float, bool, and null comparison rules while strict mode
/// accepts only an integer-tagged cell with the same payload.
pub(super) fn lower_in_array_mixed_int(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    strict: bool,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(needle, "x1")?;
            abi::emit_load_int_immediate(ctx.emitter, "x2", i64::from(strict));
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(needle, "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", i64::from(strict));
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_in_array_mixed_int");
    Ok(())
}

/// Emits the AArch64 boxed-Mixed-array string membership loop.
pub(super) fn lower_in_array_mixed_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mix_loop");
    let not_string_label = ctx.next_label("in_array_mix_not_string");
    let have_flag_label = ctx.next_label("in_array_mix_have_flag");
    let found_label = ctx.next_label("in_array_mix_found");
    let end_label = ctx.next_label("in_array_mix_end");
    let done_label = ctx.next_label("in_array_mix_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load array<Mixed> length before scanning boxed slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first boxed Mixed cell slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against the array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false once every cell is scanned
    ctx.emitter.instruction("ldr x0, [x10, x12, lsl #3]");                      // load the current boxed Mixed cell pointer from its 8-byte slot
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox"); // unbox the cell → x0=tag, x1=string ptr, x2=string len
    ctx.emitter.instruction("cmp x0, #1");                                      // is this cell a string value (runtime tag 1)?
    ctx.emitter
        .instruction(&format!("b.ne {}", not_string_label));                    // non-string cells can never equal a string needle
    ctx.load_string_value_to_regs(needle, "x3", "x4")?;
    abi::emit_call_label(ctx.emitter, eq_helper); // compare the unboxed string element (x1/x2) against the needle (x3/x4)
    ctx.emitter.instruction(&format!("b {}", have_flag_label));                 // carry the str-eq result into the shared match-flag join
    ctx.emitter.label(&not_string_label);
    ctx.emitter.instruction("mov x0, #0");                                      // a non-string cell yields a not-matched flag
    ctx.emitter.label(&have_flag_label);
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter
        .instruction(&format!("cbnz x0, {}", found_label));                     // stop as soon as a cell matches the needle
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next boxed Mixed cell
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning the remaining cells
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a matching cell
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no cell matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 boxed-Mixed-array string membership loop.
pub(super) fn lower_in_array_mixed_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mix_loop");
    let not_string_label = ctx.next_label("in_array_mix_not_string");
    let have_flag_label = ctx.next_label("in_array_mix_have_flag");
    let found_label = ctx.next_label("in_array_mix_found");
    let end_label = ctx.next_label("in_array_mix_end");
    let done_label = ctx.next_label("in_array_mix_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load array<Mixed> length before scanning boxed slots
    ctx.emitter.instruction("lea r12, [r10 + 24]");                             // point at the first boxed Mixed cell slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against the array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false once every cell is scanned
    ctx.emitter.instruction("mov rax, QWORD PTR [r12 + r13*8]");                // load the boxed Mixed cell pointer into rax (the unbox input register)
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox"); // unbox the cell → rax=tag, rdi=string ptr, rdx=string len
    ctx.emitter.instruction("cmp rax, 1");                                      // is this cell a string value (runtime tag 1)?
    ctx.emitter
        .instruction(&format!("jne {}", not_string_label));                     // non-string cells can never equal a string needle
    ctx.emitter.instruction("mov rsi, rdx");                                    // move the unboxed string length into the comparison argument
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, eq_helper); // compare the unboxed string element (rdi/rsi) against the needle (rdx/rcx)
    ctx.emitter.instruction(&format!("jmp {}", have_flag_label));               // carry the str-eq result into the shared match-flag join
    ctx.emitter.label(&not_string_label);
    ctx.emitter.instruction("xor eax, eax");                                    // a non-string cell yields a not-matched flag
    ctx.emitter.label(&have_flag_label);
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // did the current cell match the needle?
    ctx.emitter.instruction(&format!("jne {}", found_label));                   // stop as soon as a cell matches the needle
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next boxed Mixed cell
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning the remaining cells
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a matching cell
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no cell matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 string-array membership loop.
pub(super) fn lower_in_array_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_loop");
    let found_label = ctx.next_label("in_array_str_found");
    let end_label = ctx.next_label("in_array_str_end");
    let done_label = ctx.next_label("in_array_str_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("lea r12, [r10 + 24]");                             // point at the first indexed string-array payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all string elements are scanned
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the scan index before scaling it to a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("mov rdi, QWORD PTR [r12 + rcx]");                  // load the current string element pointer for comparison
    ctx.emitter
        .instruction("mov rsi, QWORD PTR [r12 + rcx + 8]");                     // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, eq_helper);
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // check whether the current string element matched the needle
    ctx.emitter.instruction(&format!("jne {}", found_label));                   // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed string element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding the searched string
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no indexed string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a Mixed-needle membership scan over a concrete indexed scalar array.
///
/// The needle is an already-boxed Mixed cell (e.g. an untyped `mixed` parameter). Each concrete
/// element is boxed into a temporary Mixed cell via `__rt_mixed_from_value` (which
/// heap-persists its own copy), compared against the needle with the selected runtime helper, then
/// released with `__rt_decref_mixed`. Both `__rt_mixed_loose_eq` and
/// `__rt_mixed_strict_eq` yield a boolean, so a match is a non-zero result. Integer and boolean
/// arrays use 8-byte payload slots; string arrays use 16-byte pairs.
pub(super) fn lower_in_array_concrete_mixed_needle(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    element_ty: &PhpType,
    eq_helper: &str,
    match_on_zero: bool,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            lower_in_array_concrete_mixed_needle_aarch64(
                ctx,
                needle,
                array,
                element_ty,
                eq_helper,
                match_on_zero,
            )
        }
        Arch::X86_64 => {
            lower_in_array_concrete_mixed_needle_x86_64(
                ctx,
                needle,
                array,
                element_ty,
                eq_helper,
                match_on_zero,
            )
        }
    }
}

/// Emits the boxed-Mixed-array membership loop for a boxed Mixed needle.
///
/// `eq_helper` takes two boxed Mixed cells in the first two argument registers and returns in the
/// integer result register. The canonical strict and loose helpers both yield a boolean, while
/// `match_on_zero` remains available for callers of a three-way comparator.
///
/// Unlike the concrete-string-array variant below, the elements are ALREADY boxed cells, so no
/// per-element boxing (and therefore no matching decref) is needed.
pub(super) fn lower_in_array_mixed_mixed(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
    match_on_zero: bool,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_mixed_mixed_aarch64(ctx, needle, array, eq_helper, match_on_zero),
        Arch::X86_64 => lower_in_array_mixed_mixed_x86_64(ctx, needle, array, eq_helper, match_on_zero),
    }
}

/// Emits the AArch64 boxed-Mixed-array membership loop for a boxed Mixed needle.
///
/// State (index, length, payload base, needle cell) lives in a 32-byte SP-relative frame so it
/// survives the comparison call.
fn lower_in_array_mixed_mixed_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
    match_on_zero: bool,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mm_loop");
    let found_label = ctx.next_label("in_array_mm_found");
    let end_label = ctx.next_label("in_array_mm_end");
    let done_label = ctx.next_label("in_array_mm_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x11, [x10]");                                  // load the array<Mixed> length before scanning boxed cell slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first boxed Mixed cell slot
    ctx.load_value_to_reg(needle, "x0")?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_store_to_sp(ctx.emitter, "x0", 24); // stash the boxed Mixed needle cell pointer in the state frame
    abi::emit_store_to_sp(ctx.emitter, "x11", 8); // stash the array length in the state frame
    abi::emit_store_to_sp(ctx.emitter, "x10", 16); // stash the cell base pointer in the state frame
    ctx.emitter.instruction("mov x12, #0");                                     // start the membership scan at index zero
    abi::emit_store_to_sp(ctx.emitter, "x12", 0);
    ctx.emitter.label(&loop_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x12", 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x13", 8);
    ctx.emitter.instruction("cmp x12, x13");                                    // compare the scan index against the array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false once every cell is scanned
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x13", 16);
    ctx.emitter.instruction("ldr x1, [x13, x12, lsl #3]");                      // load the current boxed Mixed cell as the second comparison argument
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 24); // reload the needle cell as the first comparison argument
    abi::emit_call_label(ctx.emitter, eq_helper);
    if match_on_zero {
        ctx.emitter.instruction(&format!("cbz x0, {}", found_label));           // a zero PHP compare sign is a loose match
    } else {
        ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));          // a non-zero strict-eq result is a match
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x12", 0);
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next boxed Mixed cell
    abi::emit_store_to_sp(ctx.emitter, "x12", 0);
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning the remaining cells
    ctx.emitter.label(&found_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a matching cell
    ctx.emitter.instruction(&format!("b {}", done_label));
    ctx.emitter.label(&end_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no cell matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 boxed-Mixed-array membership loop for a boxed Mixed needle.
///
/// State (index, length, payload base, needle cell) lives in a 32-byte SP-relative frame so it
/// survives the comparison call.
fn lower_in_array_mixed_mixed_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
    match_on_zero: bool,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mm_loop");
    let found_label = ctx.next_label("in_array_mm_found");
    let end_label = ctx.next_label("in_array_mm_end");
    let done_label = ctx.next_label("in_array_mm_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load the array<Mixed> length before scanning boxed cell slots
    ctx.emitter.instruction("lea r10, [r10 + 24]");                             // point at the first boxed Mixed cell slot
    ctx.load_value_to_reg(needle, "rax")?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_store_to_sp(ctx.emitter, "rax", 24); // stash the boxed Mixed needle cell pointer in the state frame
    abi::emit_store_to_sp(ctx.emitter, "r11", 8); // stash the array length in the state frame
    abi::emit_store_to_sp(ctx.emitter, "r10", 16); // stash the cell base pointer in the state frame
    ctx.emitter.instruction("xor ecx, ecx");                                    // start the membership scan at index zero
    abi::emit_store_to_sp(ctx.emitter, "rcx", 0);
    ctx.emitter.label(&loop_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", 8);
    ctx.emitter.instruction("cmp rax, rcx");                                    // compare the scan index against the array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false once every cell is scanned
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", 16);
    ctx.emitter.instruction("mov rsi, QWORD PTR [rcx + rax*8]");                // load the current boxed Mixed cell as the second comparison argument
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 24); // reload the needle cell as the first comparison argument
    abi::emit_call_label(ctx.emitter, eq_helper);
    ctx.emitter.instruction("test rax, rax");                                   // inspect the comparison outcome for this cell
    if match_on_zero {
        ctx.emitter.instruction(&format!("je {}", found_label));                // a zero PHP compare sign is a loose match
    } else {
        ctx.emitter.instruction(&format!("jne {}", found_label));               // a non-zero strict-eq result is a match
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 0);
    ctx.emitter.instruction("add rax, 1");                                      // advance to the next boxed Mixed cell
    abi::emit_store_to_sp(ctx.emitter, "rax", 0);
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning the remaining cells
    ctx.emitter.label(&found_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a matching cell
    ctx.emitter.instruction(&format!("jmp {}", done_label));
    ctx.emitter.label(&end_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no cell matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the AArch64 concrete-scalar-array membership loop for a boxed Mixed needle.
///
/// State (index, length, payload base, needle cell, boxed element cell, comparison result) lives in
/// a 48-byte SP-relative frame so it survives the boxing, comparison, and decref calls.
fn lower_in_array_concrete_mixed_needle_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    element_ty: &PhpType,
    eq_helper: &str,
    match_on_zero: bool,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_smn_loop");
    let found_label = ctx.next_label("in_array_smn_found");
    let end_label = ctx.next_label("in_array_smn_end");
    let done_label = ctx.next_label("in_array_smn_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x11, [x10]");                                  // load the concrete scalar-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed scalar payload slot
    ctx.load_value_to_reg(needle, "x0")?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 48);
    abi::emit_store_to_sp(ctx.emitter, "x0", 24); // stash the boxed Mixed needle cell pointer in the state frame
    abi::emit_store_to_sp(ctx.emitter, "x11", 8); // stash the scalar-array length in the state frame
    abi::emit_store_to_sp(ctx.emitter, "x10", 16); // stash the payload base pointer in the state frame
    ctx.emitter.instruction("mov x12, #0");                                     // start the membership scan at index zero
    abi::emit_store_to_sp(ctx.emitter, "x12", 0);
    ctx.emitter.label(&loop_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x12", 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x13", 8);
    ctx.emitter.instruction("cmp x12, x13");                                    // compare the scan index against the array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false once every element is scanned
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x13", 16);
    match element_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction("ldr x0, [x13, x12, lsl #3]");              // load the current 8-byte integer-like element for boxing
        }
        PhpType::Str => {
            ctx.emitter.instruction("lsl x14, x12, #4");                        // scale the element index by the 16-byte string slot width
            ctx.emitter.instruction("add x13, x13, x14");                       // compute the current string element slot address
            ctx.emitter.instruction("ldr x1, [x13]");                           // load the current string element pointer for boxing
            ctx.emitter.instruction("ldr x2, [x13, #8]");                       // load the current string element length for boxing
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "Mixed-needle in_array concrete element PHP type {:?}",
                other
            )))
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, element_ty); // box the element into a temporary owned Mixed cell (x0)
    abi::emit_store_to_sp(ctx.emitter, "x0", 32); // stash the temporary element cell for the later decref
    ctx.emitter.instruction("mov x1, x0");                                      // pass the boxed element as the second comparison argument
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 24); // reload the needle cell as the first comparison argument
    abi::emit_call_label(ctx.emitter, eq_helper);
    abi::emit_store_to_sp(ctx.emitter, "x0", 40); // stash the comparison result across the element decref
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 32);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 40);
    if match_on_zero {
        ctx.emitter.instruction(&format!("cbz x0, {}", found_label));           // a zero PHP compare sign is a loose match
    } else {
        ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));          // a non-zero strict-eq result is a match
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x12", 0);
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed scalar element
    abi::emit_store_to_sp(ctx.emitter, "x12", 0);
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning the remaining scalar elements
    ctx.emitter.label(&found_label);
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a matching element
    ctx.emitter.instruction(&format!("b {}", done_label));
    ctx.emitter.label(&end_label);
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no element matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 concrete-scalar-array membership loop for a boxed Mixed needle.
///
/// State (index, length, payload base, needle cell, boxed element cell, comparison result) lives in
/// a 48-byte SP-relative frame so it survives the boxing, comparison, and decref calls.
fn lower_in_array_concrete_mixed_needle_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    element_ty: &PhpType,
    eq_helper: &str,
    match_on_zero: bool,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_smn_loop");
    let found_label = ctx.next_label("in_array_smn_found");
    let end_label = ctx.next_label("in_array_smn_end");
    let done_label = ctx.next_label("in_array_smn_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load the concrete scalar-array length before scanning payload slots
    ctx.emitter.instruction("lea r10, [r10 + 24]");                             // point at the first indexed scalar payload slot
    ctx.load_value_to_reg(needle, "rax")?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 48);
    abi::emit_store_to_sp(ctx.emitter, "rax", 24); // stash the boxed Mixed needle cell pointer in the state frame
    abi::emit_store_to_sp(ctx.emitter, "r11", 8); // stash the scalar-array length in the state frame
    abi::emit_store_to_sp(ctx.emitter, "r10", 16); // stash the payload base pointer in the state frame
    ctx.emitter.instruction("xor ecx, ecx");                                    // start the membership scan at index zero
    abi::emit_store_to_sp(ctx.emitter, "rcx", 0);
    ctx.emitter.label(&loop_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", 8);
    ctx.emitter.instruction("cmp rax, rcx");                                    // compare the scan index against the array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false once every element is scanned
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", 16);
    match element_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rcx + rax*8]");        // load the current 8-byte integer-like element for boxing
        }
        PhpType::Str => {
            ctx.emitter.instruction("shl rax, 4");                              // scale the element index by the 16-byte string slot width
            ctx.emitter.instruction("add rcx, rax");                            // compute the current string element slot address
            ctx.emitter.instruction("mov rax, QWORD PTR [rcx]");                // load the current string element pointer for boxing
            ctx.emitter.instruction("mov rdx, QWORD PTR [rcx + 8]");            // load the current string element length for boxing
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "Mixed-needle in_array concrete element PHP type {:?}",
                other
            )))
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, element_ty); // box the element into a temporary owned Mixed cell (rax)
    abi::emit_store_to_sp(ctx.emitter, "rax", 32); // stash the temporary element cell for the later decref
    ctx.emitter.instruction("mov rsi, rax");                                    // pass the boxed element as the second comparison argument
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 24); // reload the needle cell as the first comparison argument
    abi::emit_call_label(ctx.emitter, eq_helper);
    abi::emit_store_to_sp(ctx.emitter, "rax", 40); // stash the comparison result across the element decref
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 32);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 40);
    ctx.emitter.instruction("test rax, rax");                                   // inspect the comparison outcome for this element
    if match_on_zero {
        ctx.emitter.instruction(&format!("je {}", found_label));                // a zero PHP compare sign is a loose match
    } else {
        ctx.emitter.instruction(&format!("jne {}", found_label));               // a non-zero strict-eq result is a match
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 0);
    ctx.emitter.instruction("add rax, 1");                                      // advance to the next indexed scalar element
    abi::emit_store_to_sp(ctx.emitter, "rax", 0);
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning the remaining scalar elements
    ctx.emitter.label(&found_label);
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a matching element
    ctx.emitter.instruction(&format!("jmp {}", done_label));
    ctx.emitter.label(&end_label);
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no element matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}
