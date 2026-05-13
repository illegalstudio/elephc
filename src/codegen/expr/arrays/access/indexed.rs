//! Purpose:
//! Lowers indexed and associative array element reads including nullable and Mixed results.
//! Produces expression results while preserving container ownership and bounds/null behavior.
//!
//! Called from:
//! - `crate::codegen::expr::arrays::access`
//!
//! Key details:
//! - Element layout and boxed Mixed handling must stay aligned with array runtime helpers.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::emit_box_runtime_payload_as_mixed;
use crate::codegen::platform::Arch;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(crate) fn emit_array_access(
    array: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let arr_ty = emit_expr(array, emitter, ctx, data);
    emit_array_access_with_loaded_base(&arr_ty, index, emitter, ctx, data, false)
}

pub(crate) fn emit_array_access_with_loaded_base(
    arr_ty: &PhpType,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    box_nullable_base: bool,
) -> PhpType {
    if let PhpType::Buffer(elem_ty) = arr_ty {
        let buffer_reg = abi::symbol_scratch_reg(emitter);
        let len_reg = abi::temp_int_reg(emitter.target);
        let stride_reg = match emitter.target.arch {
            Arch::AArch64 => "x11",
            Arch::X86_64 => "rcx",
        };
        let result_reg = abi::int_result_reg(emitter);
        abi::emit_push_reg(emitter, result_reg);                                // preserve the buffer header pointer while evaluating the index expression
        emit_expr(index, emitter, ctx, data);
        abi::emit_pop_reg(emitter, buffer_reg);                                 // restore the buffer header pointer into a scratch register
        emitter.comment("buffer access");
        let uaf_ok = ctx.next_label("buf_uaf_ok");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cbnz {}, {}", buffer_reg, uaf_ok)); // skip the fatal helper when the buffer header pointer is still live
                emitter.instruction("b __rt_buffer_use_after_free");            // abort immediately when the buffer local was nulled by buffer_free()
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("test {}, {}", buffer_reg, buffer_reg)); // check whether the restored buffer header pointer is null
                emitter.instruction(&format!("jne {}", uaf_ok));                // continue only when the buffer header pointer is still live
                emitter.instruction("jmp __rt_buffer_use_after_free");          // abort immediately when the buffer local was nulled by buffer_free()
            }
        }
        emitter.label(&uaf_ok);
        let elem_ty = *elem_ty.clone();
        let bounds_ok = ctx.next_label("buffer_idx_ok");
        let oob_ok = ctx.next_label("buf_oob_ok");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {}, #0", result_reg));        // reject negative buffer indexes before touching the payload
                emitter.instruction(&format!("b.ge {}", oob_ok));               // continue once the requested index is non-negative
                emitter.instruction("b __rt_buffer_bounds_fail");               // abort immediately on negative buffer indexes
                emitter.label(&oob_ok);
                abi::emit_load_from_address(emitter, len_reg, buffer_reg, 0);            // load the logical buffer length from the header
                emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg)); // compare the requested index against the logical buffer length
                emitter.instruction(&format!("b.lo {}", bounds_ok));            // continue once the requested index is still in bounds
                emitter.instruction(&format!("mov x1, {}", len_reg));           // pass the logical buffer length to the fatal helper for parity with the ARM path
                emitter.instruction("bl __rt_buffer_bounds_fail");              // abort with the dedicated buffer-bounds diagnostic
                emitter.label(&bounds_ok);
                abi::emit_load_from_address(emitter, stride_reg, buffer_reg, 8);         // load the element stride from the buffer header
                emitter.instruction(&format!("add {}, {}, #16", buffer_reg, buffer_reg)); // skip the buffer header to reach the contiguous payload base
                emitter.instruction(&format!("madd {}, {}, {}, {}", buffer_reg, result_reg, stride_reg, buffer_reg)); // compute payload base + index*stride for the addressed buffer element
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {}, 0", result_reg));         // reject negative buffer indexes before touching the payload
                emitter.instruction(&format!("jge {}", oob_ok));                // continue once the requested index is non-negative
                emitter.instruction("jmp __rt_buffer_bounds_fail");             // abort immediately on negative buffer indexes
                emitter.label(&oob_ok);
                abi::emit_load_from_address(emitter, len_reg, buffer_reg, 0);            // load the logical buffer length from the header
                emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg)); // compare the requested index against the logical buffer length
                emitter.instruction(&format!("jl {}", bounds_ok));              // continue once the requested index is still in bounds
                emitter.instruction("jmp __rt_buffer_bounds_fail");             // abort with the dedicated buffer-bounds diagnostic
                emitter.label(&bounds_ok);
                abi::emit_load_from_address(emitter, stride_reg, buffer_reg, 8);         // load the element stride from the buffer header
                emitter.instruction(&format!("add {}, 16", buffer_reg));        // skip the buffer header to reach the contiguous payload base
                emitter.instruction(&format!("imul {}, {}", result_reg, stride_reg)); // scale the requested index by the element stride in bytes
                emitter.instruction(&format!("add {}, {}", buffer_reg, result_reg)); // advance the payload base to the addressed buffer element
            }
        }
        match &elem_ty {
            PhpType::Float => {
                abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), buffer_reg, 0); // load the floating-point payload from the addressed buffer element slot
                return PhpType::Float;
            }
            PhpType::Packed(name) => {
                emitter.instruction(&format!("mov {}, {}", result_reg, buffer_reg)); // expose the packed element address as a typed pointer result
                return PhpType::Pointer(Some(name.clone()));
            }
            _ => {
                abi::emit_load_from_address(emitter, result_reg, buffer_reg, 0);         // load the scalar or pointer payload from the addressed buffer element slot
                return elem_ty;
            }
        }
    }

    if *arr_ty == PhpType::Str {
        let (str_ptr_reg, str_len_reg) = abi::string_result_regs(emitter);
        abi::emit_push_reg_pair(emitter, str_ptr_reg, str_len_reg);             // preserve the indexed source string while evaluating the scalar offset expression
        emit_expr(index, emitter, ctx, data);
        emitter.comment("string indexing");

        let non_negative = ctx.next_label("str_idx_pos");
        let oob = ctx.next_label("str_idx_oob");
        let end = ctx.next_label("str_idx_end");

        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_pop_reg_pair(emitter, "x1", "x2");                    // restore the indexed source string into the standard AArch64 string result registers after evaluating the scalar offset
                emitter.instruction("cmp x0, #0");                              // check whether the requested string offset is negative
                emitter.instruction(&format!("b.ge {}", non_negative));         // keep non-negative offsets as-is
                emitter.instruction("add x0, x2, x0");                          // convert negative offsets to length + offset
                emitter.instruction("cmp x0, #0");                              // check whether the adjusted offset still points before the string
                emitter.instruction(&format!("b.lt {}", oob));                  // negative offsets beyond -len return empty string
                emitter.label(&non_negative);
                emitter.instruction("cmp x0, x2");                              // compare the offset against the string length
                emitter.instruction(&format!("b.ge {}", oob));                  // offsets at or beyond length return empty string
                emitter.instruction("add x1, x1, x0");                          // advance the string pointer to the selected character
                emitter.instruction("mov x2, #1");                              // string indexing returns exactly one character when in bounds
                emitter.instruction(&format!("b {}", end));                     // skip the out-of-bounds fallback
                emitter.label(&oob);
                emitter.instruction("mov x2, #0");                              // out-of-bounds: return empty string
            }
            Arch::X86_64 => {
                abi::emit_push_reg(emitter, "rax");                             // preserve the computed scalar string offset in its own temporary slot before the original string pair is restored from the older stack slot
                emitter.instruction("mov rax, QWORD PTR [rsp]");                // reload the computed scalar string offset from the top temporary stack slot without disturbing the older saved string pair yet
                emitter.instruction("mov r8, QWORD PTR [rsp + 16]");            // reload the indexed source string pointer from the older temporary stack slot below the saved scalar offset
                emitter.instruction("mov r9, QWORD PTR [rsp + 24]");            // reload the indexed source string length from the older temporary stack slot below the saved scalar offset
                emitter.instruction("add rsp, 32");                             // release both temporary stack slots after restoring the scalar index and indexed source string pair
                emitter.instruction("cmp rax, 0");                              // check whether the requested string offset is negative
                emitter.instruction(&format!("jge {}", non_negative));          // keep non-negative offsets as-is
                emitter.instruction("add rax, r9");                             // convert negative offsets to length + offset
                emitter.instruction("cmp rax, 0");                              // check whether the adjusted offset still points before the string
                emitter.instruction(&format!("jl {}", oob));                    // negative offsets beyond -len return empty string
                emitter.label(&non_negative);
                emitter.instruction("cmp rax, r9");                             // compare the offset against the string length
                emitter.instruction(&format!("jge {}", oob));                   // offsets at or beyond length return empty string
                emitter.instruction("add r8, rax");                             // advance the string pointer to the selected character
                emitter.instruction("mov rax, r8");                             // publish the addressed character pointer in the standard x86_64 string result pointer register
                emitter.instruction("mov rdx, 1");                              // string indexing returns exactly one character when in bounds
                emitter.instruction(&format!("jmp {}", end));                   // skip the out-of-bounds fallback
                emitter.label(&oob);
                emitter.instruction("mov rax, r8");                             // preserve the original string pointer as the empty-string base pointer for out-of-bounds indexing
                emitter.instruction("mov rdx, 0");                              // out-of-bounds: return empty string
            }
        }
        emitter.label(&end);

        return PhpType::Str;
    }

    if matches!(arr_ty, PhpType::Mixed) {
        // Mixed receiver: dispatch through the unified runtime helper. It
        // unboxes the cell, branches on the runtime tag (indexed array,
        // assoc, stdClass), and returns a Mixed cell — including
        // Mixed(null) for misses, non-container payloads, or unrelated
        // class types. The helper expects (mixed_ptr, key_lo, key_hi)
        // matching the convention of `emit_normalized_hash_key`.
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the boxed Mixed receiver across key evaluation
        crate::codegen::emit_normalized_hash_key(index, emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_pop_reg(emitter, "x0");                               // restore the Mixed receiver into the helper's first argument
                emitter.instruction("bl __rt_mixed_array_get");                 // dispatch to the unified array/hash/stdclass reader
            }
            Arch::X86_64 => {
                // emit_normalized_hash_key leaves key_lo in rax/rdx and
                // key_hi in rdx/-1; SysV expects (rdi, rsi, rdx) for
                // (mixed_ptr, key_lo, key_hi).
                emitter.instruction("mov rsi, rax");                            // shift key_lo from the hash-helper return register into the SysV second-arg slot
                abi::emit_pop_reg(emitter, "rdi");                              // restore the Mixed receiver into the SysV first-arg register
                emitter.instruction("call __rt_mixed_array_get");               // dispatch to the unified array/hash/stdclass reader
            }
        }
        return PhpType::Mixed;
    }

    let assoc_value_ty = match arr_ty {
        PhpType::AssocArray { value, .. } => Some(*value.clone()),
        PhpType::Union(members) => members.iter().find_map(|member| {
            if let PhpType::AssocArray { value, .. } = member {
                Some(*value.clone())
            } else {
                None
            }
        }),
        _ => None,
    };

    if let Some(val_ty) = assoc_value_ty {
        let boxed_assoc_base = matches!(arr_ty, PhpType::Mixed | PhpType::Union(_));
        let box_assoc_result =
            box_nullable_base && boxed_assoc_base && !matches!(val_ty.codegen_repr(), PhpType::Mixed);
        let boxed_assoc_fallback =
            box_assoc_result || matches!(val_ty.codegen_repr(), PhpType::Mixed);
        let done = ctx.next_label("hash_done");
        if boxed_assoc_base {
            let hash_payload = ctx.next_label("hash_payload");
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the boxed array|false value while evaluating the key expression
            crate::codegen::emit_normalized_hash_key(index, emitter, ctx, data);
            let (key_ptr_reg, key_len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);         // preserve the normalized key while unboxing the array|false value
            match emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_load_temporary_stack_slot(emitter, "x0", 16);
                    abi::emit_call_label(emitter, "__rt_mixed_unbox");          // inspect a boxed array|false value after the key expression has run
                    emitter.instruction("cmp x0, #5");                          // runtime tag 5 = associative array
                    emitter.instruction(&format!("b.eq {}", hash_payload));     // continue only when the boxed payload is a hash
                    abi::emit_release_temporary_stack(emitter, 32);             // discard the saved key and boxed base before returning the null-like fallback
                    if boxed_assoc_fallback {
                        objects_boxed_null_for_array_access(emitter);
                    } else {
                        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::MAX - 1);
                    }
                    emitter.instruction(&format!("b {}", done));                // skip hash lookup when the boxed value is false/null
                    emitter.label(&hash_payload);
                    emitter.instruction("mov x0, x1");                          // move the unboxed hash pointer into the standard result register
                    abi::emit_pop_reg_pair(emitter, "x1", "x2");                // restore the normalized key into the hash-get helper argument registers
                    abi::emit_release_temporary_stack(emitter, 16);             // discard the original boxed base after extracting its hash payload
                }
                Arch::X86_64 => {
                    abi::emit_load_temporary_stack_slot(emitter, "rax", 16);
                    abi::emit_call_label(emitter, "__rt_mixed_unbox");          // inspect a boxed array|false value after the key expression has run
                    emitter.instruction("cmp rax, 5");                          // runtime tag 5 = associative array
                    emitter.instruction(&format!("je {}", hash_payload));       // continue only when the boxed payload is a hash
                    abi::emit_release_temporary_stack(emitter, 32);             // discard the saved key and boxed base before returning the null-like fallback
                    if boxed_assoc_fallback {
                        objects_boxed_null_for_array_access(emitter);
                    } else {
                        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::MAX - 1);
                    }
                    emitter.instruction(&format!("jmp {}", done));              // skip hash lookup when the boxed value is false/null
                    emitter.label(&hash_payload);
                    emitter.instruction("mov r8, rdi");                         // preserve the unboxed hash pointer while restoring the normalized key
                    abi::emit_pop_reg_pair(emitter, "rsi", "rdx");              // restore the normalized key into the hash-get helper argument registers
                    abi::emit_release_temporary_stack(emitter, 16);             // discard the original boxed base after extracting its hash payload
                    emitter.instruction("mov rdi, r8");                         // pass the unboxed hash pointer as the first hash-get argument
                }
            }
        } else {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the hash-table pointer while evaluating the string key expression
            crate::codegen::emit_normalized_hash_key(index, emitter, ctx, data);
            let (key_ptr_reg, key_len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);         // preserve the computed key pointer and length while restoring the hash-table pointer
            match emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_pop_reg_pair(emitter, "x1", "x2");                // restore the key pointer and length from the top stack slot into the hash-get helper argument registers
                    abi::emit_pop_reg(emitter, "x0");                           // restore the saved hash-table pointer into the first hash-get helper argument register
                }
                Arch::X86_64 => {
                    abi::emit_pop_reg_pair(emitter, "rsi", "rdx");              // restore the key pointer and length from the top stack slot into the remaining SysV hash-get helper argument registers
                    abi::emit_pop_reg(emitter, "rdi");                          // restore the saved hash-table pointer into the first SysV hash-get helper argument register
                }
            }
        }
        emitter.comment("assoc array access");
        abi::emit_call_label(emitter, "__rt_hash_get");                            // lookup key and return found-flag plus borrowed payload words through the target runtime ABI

        let not_found = ctx.next_label("hash_miss");
        abi::emit_branch_if_int_result_zero(emitter, &not_found);                  // jump to the not-found handler when the hash lookup misses

        match emitter.target.arch {
            Arch::AArch64 => match &val_ty {
                PhpType::Int | PhpType::Bool => {
                    emitter.instruction("mov x0, x1");                          // move the borrowed associative-array scalar payload into the standard integer result register
                }
                PhpType::Str => {}
                PhpType::Float => {
                    emitter.instruction("fmov d0, x1");                         // move the borrowed associative-array float bits into the standard float result register
                }
                PhpType::Mixed => {
                    emit_box_runtime_payload_as_mixed(emitter, "x3", "x1", "x2"); // box the borrowed associative-array payload into an owned mixed cell
                }
                _ => {
                    emitter.instruction("mov x0, x1");                          // move the borrowed associative-array pointer payload into the standard integer result register
                }
            },
            Arch::X86_64 => match &val_ty {
                PhpType::Int | PhpType::Bool => {
                    emitter.instruction("mov rax, rdi");                        // move the borrowed associative-array scalar payload into the standard integer result register
                }
                PhpType::Str => {
                    emitter.instruction("mov rax, rdi");                        // move the borrowed associative-array string pointer into the standard x86_64 string result register
                    emitter.instruction("mov rdx, rsi");                        // move the borrowed associative-array string length into the paired x86_64 string result register
                }
                PhpType::Float => {
                    emitter.instruction("movq xmm0, rdi");                      // move the borrowed associative-array float bits into the standard float result register
                }
                PhpType::Mixed => {
                    emit_box_runtime_payload_as_mixed(emitter, "rcx", "rdi", "rsi"); // box the borrowed associative-array payload into an owned mixed cell
                }
                _ => {
                    emitter.instruction("mov rax, rdi");                        // move the borrowed associative-array pointer payload into the standard integer result register
                }
            },
        }
        if box_assoc_result {
            crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        }
        abi::emit_jump(emitter, &done);                                           // skip the not-found fallback after materializing the successful lookup result

        emitter.label(&not_found);
        if boxed_assoc_fallback {
            objects_boxed_null_for_array_access(emitter);
        } else {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::MAX - 1); // materialize the shared null sentinel for associative-array misses
        }
        emitter.label(&done);
        return if box_assoc_result { PhpType::Mixed } else { val_ty };
    }

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the array pointer while evaluating the index expression
    emit_expr(index, emitter, ctx, data);
    let array_reg = abi::symbol_scratch_reg(emitter);
    let len_reg = abi::temp_int_reg(emitter.target);
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_pop_reg(emitter, array_reg);                                      // restore the array pointer into a scratch register
    emitter.comment("array access");
    let (elem_ty, boxed_indexed_base) = indexed_array_element_type(arr_ty, box_nullable_base);

    let null_label = ctx.next_label("arr_null");
    let ok_label = ctx.next_label("arr_ok");
    if boxed_indexed_base {
        let array_payload = ctx.next_label("arr_payload");
        abi::emit_push_reg(emitter, result_reg);                                // preserve the evaluated array index while unboxing the nullable array base
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the boxed array base into the mixed-unbox input register
                abi::emit_call_label(emitter, "__rt_mixed_unbox");              // inspect the boxed array base after the index expression has run
                emitter.instruction("cmp x0, #4");                              // runtime tag 4 = indexed array
                emitter.instruction(&format!("b.eq {}", array_payload));        // continue only when the boxed payload is an indexed array
                abi::emit_release_temporary_stack(emitter, 16);                 // discard the saved index before returning a boxed null fallback
                objects_boxed_null_for_array_access(emitter);
                emitter.instruction(&format!("b {}", ok_label));                // skip indexed-array bounds checks when the base is not an array
                emitter.label(&array_payload);
                emitter.instruction(&format!("mov {}, x1", array_reg));         // move the unboxed indexed-array pointer into the scratch array register
                abi::emit_pop_reg(emitter, result_reg);                         // restore the evaluated index for bounds checking
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, {}", array_reg));        // move the boxed array base into the mixed-unbox input register
                abi::emit_call_label(emitter, "__rt_mixed_unbox");              // inspect the boxed array base after the index expression has run
                emitter.instruction("cmp rax, 4");                              // runtime tag 4 = indexed array
                emitter.instruction(&format!("je {}", array_payload));          // continue only when the boxed payload is an indexed array
                abi::emit_release_temporary_stack(emitter, 16);                 // discard the saved index before returning a boxed null fallback
                objects_boxed_null_for_array_access(emitter);
                emitter.instruction(&format!("jmp {}", ok_label));              // skip indexed-array bounds checks when the base is not an array
                emitter.label(&array_payload);
                emitter.instruction(&format!("mov {}, rdi", array_reg));        // move the unboxed indexed-array pointer into the scratch array register
                abi::emit_pop_reg(emitter, result_reg);                         // restore the evaluated index for bounds checking
            }
        }
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // check if index is negative
            emitter.instruction(&format!("b.lt {null_label}"));                 // negative index → null sentinel
            abi::emit_load_from_address(emitter, len_reg, array_reg, 0);       // load array length from header (offset 0)
            emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));   // compare index against array length
            emitter.instruction(&format!("b.ge {null_label}"));                 // index >= length → null sentinel
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 0", result_reg));             // check if index is negative
            emitter.instruction(&format!("jl {null_label}"));                   // negative index → null sentinel
            abi::emit_load_from_address(emitter, len_reg, array_reg, 0);       // load array length from header (offset 0)
            emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));   // compare index against array length
            emitter.instruction(&format!("jge {null_label}"));                  // index >= length → null sentinel
        }
    }

    match &elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip 24-byte array header to reach data
                emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", result_reg, array_reg, result_reg)); // load element at array + index*8
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip 24-byte array header to reach data
                emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", result_reg, array_reg, result_reg)); // load element at array + index*8
            }
        },
        PhpType::Float => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip 24-byte array header to reach the contiguous float payload
                emitter.instruction(&format!("ldr d0, [{}, {}, lsl #3]", array_reg, result_reg)); // load the float payload from data[index]
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip 24-byte array header to reach the contiguous float payload
                emitter.instruction(&format!("movsd xmm0, QWORD PTR [{} + {} * 8]", array_reg, result_reg)); // load the float payload from data[index]
            }
        },
        PhpType::Str => {
            let (ptr_reg, len_result_reg) = abi::string_result_regs(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("lsl {}, {}, #4", result_reg, result_reg)); // multiply index by 16 (string = ptr+len pair)
                    emitter.instruction(&format!("add {}, {}, {}", array_reg, array_reg, result_reg)); // add scaled index offset to array base
                    emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip 24-byte array header to reach data
                    abi::emit_load_from_address(emitter, ptr_reg, array_reg, 0); // load string pointer from element slot
                    abi::emit_load_from_address(emitter, len_result_reg, array_reg, 8); // load string length from element slot
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("shl {}, 4", result_reg));     // multiply index by 16 (string = ptr+len pair)
                    emitter.instruction(&format!("add {}, {}", array_reg, result_reg)); // add scaled index offset to array base
                    emitter.instruction(&format!("add {}, 24", array_reg));     // skip 24-byte array header to reach data
                    abi::emit_load_from_address(emitter, ptr_reg, array_reg, 0); // load string pointer from element slot
                    abi::emit_load_from_address(emitter, len_result_reg, array_reg, 8); // load string length from element slot
                }
            }
        }
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip 24-byte array header to reach data
                    emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", result_reg, array_reg, result_reg)); // load pointer at index
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip 24-byte array header to reach data
                    emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", result_reg, array_reg, result_reg)); // load pointer at index
                }
            }
        }
        _ => {}
    }
    if boxed_indexed_base {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &elem_ty);
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {ok_label}")),         // skip null sentinel fallback
        Arch::X86_64 => emitter.instruction(&format!("jmp {ok_label}")),        // skip null sentinel fallback
    }

    emitter.label(&null_label);
    if boxed_indexed_base || matches!(elem_ty, PhpType::Mixed | PhpType::Union(_)) {
        objects_boxed_null_for_array_access(emitter);
    } else {
        abi::emit_load_int_immediate(emitter, result_reg, 0x7fff_ffff_ffff_fffe); // materialize the runtime null sentinel for out-of-bounds access
    }
    emitter.label(&ok_label);

    if boxed_indexed_base { PhpType::Mixed } else { elem_ty }
}

fn indexed_array_element_type(arr_ty: &PhpType, box_nullable_base: bool) -> (PhpType, bool) {
    match arr_ty {
        PhpType::Array(elem_ty) => (*elem_ty.clone(), false),
        PhpType::Union(members) => {
            let elem_ty = members
                .iter()
                .find_map(|member| {
                    if let PhpType::Array(elem_ty) = member {
                        Some(*elem_ty.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or(PhpType::Int);
            (elem_ty, box_nullable_base)
        }
        _ => (PhpType::Int, false),
    }
}

fn objects_boxed_null_for_array_access(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(
        emitter,
        abi::int_result_reg(emitter),
        0x7fff_ffff_ffff_fffe,
    );
    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Void);
}
