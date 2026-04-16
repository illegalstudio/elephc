use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use crate::parser::ast::TypeExpr;
use crate::types::packed_type_size;
use super::super::{emit_expr, Expr, PhpType};

pub(crate) fn emit_match_expr(
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: &Option<Box<Expr>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("match expression");
    let subj_ty = emit_expr(subject, emitter, ctx, data);
    match &subj_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                 // save the string subject in one temporary stack slot using the active target ABI
        }
        PhpType::Float => {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));  // save the float subject in one temporary stack slot using the active target ABI
        }
        _ => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save the scalar subject in one temporary stack slot using the active target ABI
        }
    }

    let end_label = ctx.next_label("match_end");
    let mut result_ty = PhpType::Void;

    for (patterns, result) in arms {
        let arm_label = ctx.next_label("match_arm");
        let next_arm = ctx.next_label("match_next");

        for (i, pattern) in patterns.iter().enumerate() {
            let pat_ty = emit_expr(pattern, emitter, ctx, data);
            match &subj_ty {
                PhpType::Str => match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("mov x3, x1");                      // move the pattern string pointer into the AArch64 right-hand compare register
                        emitter.instruction("mov x4, x2");                      // move the pattern string length into the AArch64 right-hand compare register
                        emitter.instruction("ldp x1, x2, [sp]");                // reload the saved subject string into the AArch64 left-hand compare registers
                        abi::emit_call_label(emitter, "__rt_str_eq");       // compare the subject and pattern strings through the shared runtime helper
                    }
                    Arch::X86_64 => {
                        emitter.instruction("mov rcx, rdx");                    // move the pattern string length into the SysV fourth argument register expected by __rt_str_eq
                        emitter.instruction("mov rdx, rax");                    // move the pattern string pointer into the SysV third argument register expected by __rt_str_eq
                        emitter.instruction("mov rdi, QWORD PTR [rsp]");        // reload the saved subject string pointer into the SysV first argument register
                        emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");    // reload the saved subject string length into the SysV second argument register
                        abi::emit_call_label(emitter, "__rt_str_eq");       // compare the subject and pattern strings through the shared runtime helper
                    }
                },
                PhpType::Float => match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldr d1, [sp]");                    // reload the saved subject float into the AArch64 scratch compare register
                        emitter.instruction("fcmp d1, d0");                     // compare the saved subject float against the current pattern float
                        emitter.instruction("cset x0, eq");                     // materialize the float equality result in the canonical AArch64 integer result register
                    }
                    Arch::X86_64 => {
                        emitter.instruction("movsd xmm1, QWORD PTR [rsp]");     // reload the saved subject float into the x86_64 scratch compare register
                        emitter.instruction("ucomisd xmm1, xmm0");              // compare the saved subject float against the current pattern float
                        emitter.instruction("sete al");                         // materialize the float equality result in the low x86_64 result byte
                        emitter.instruction("movzx eax, al");                   // widen the x86_64 boolean byte back into the canonical integer result register
                    }
                },
                _ => match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldr x9, [sp]");                    // reload the saved scalar subject into an AArch64 scratch register
                        emitter.instruction("cmp x9, x0");                      // compare the saved scalar subject against the current pattern scalar
                        emitter.instruction("cset x0, eq");                     // materialize the scalar equality result in the canonical AArch64 integer result register
                    }
                    Arch::X86_64 => {
                        emitter.instruction("mov r10, QWORD PTR [rsp]");        // reload the saved scalar subject into an x86_64 scratch register
                        emitter.instruction("cmp r10, rax");                    // compare the saved scalar subject against the current pattern scalar
                        emitter.instruction("sete al");                         // materialize the scalar equality result in the low x86_64 result byte
                        emitter.instruction("movzx eax, al");                   // widen the x86_64 boolean byte back into the canonical integer result register
                    }
                },
            }
            abi::emit_branch_if_int_result_nonzero(emitter, &arm_label);        // jump to the current match arm once the subject equals the current pattern
            if i == patterns.len() - 1 {
                abi::emit_jump(emitter, &next_arm);                             // continue with the next match arm when this arm's patterns all miss
            }
            let _ = pat_ty;
        }

        emitter.label(&arm_label);
        result_ty = emit_expr(result, emitter, ctx, data);
        abi::emit_jump(emitter, &end_label);                                    // skip the remaining match arms after evaluating the selected arm expression
        emitter.label(&next_arm);
    }

    if let Some(def) = default {
        result_ty = emit_expr(def, emitter, ctx, data);
    } else {
        abi::emit_call_label(emitter, "__rt_match_unhandled");                  // abort when no arm matched and the match expression has no default arm
    }

    emitter.label(&end_label);
    abi::emit_release_temporary_stack(emitter, 16);                             // release the saved subject slot without clobbering the match expression result registers
    result_ty
}

pub(crate) fn emit_array_access(
    array: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let arr_ty = emit_expr(array, emitter, ctx, data);

    if let PhpType::Buffer(elem_ty) = &arr_ty {
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

    if arr_ty == PhpType::Str {
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

    if let PhpType::AssocArray { value, .. } = &arr_ty {
        let val_ty = *value.clone();
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the hash-table pointer while evaluating the string key expression
        let _key_ty = emit_expr(index, emitter, ctx, data);
        let (key_ptr_reg, key_len_reg) = abi::string_result_regs(emitter);
        abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);                 // preserve the computed key pointer and length while restoring the hash-table pointer
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_pop_reg_pair(emitter, "x1", "x2");                        // restore the key pointer and length from the top stack slot into the hash-get helper argument registers
                abi::emit_pop_reg(emitter, "x0");                                   // restore the saved hash-table pointer into the first hash-get helper argument register
            }
            Arch::X86_64 => {
                abi::emit_pop_reg_pair(emitter, "rsi", "rdx");                      // restore the key pointer and length from the top stack slot into the remaining SysV hash-get helper argument registers
                abi::emit_pop_reg(emitter, "rdi");                                  // restore the saved hash-table pointer into the first SysV hash-get helper argument register
            }
        }
        emitter.comment("assoc array access");
        abi::emit_call_label(emitter, "__rt_hash_get");                            // lookup key and return found-flag plus borrowed payload words through the target runtime ABI

        let not_found = ctx.next_label("hash_miss");
        let done = ctx.next_label("hash_done");
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
                    super::super::super::emit_box_runtime_payload_as_mixed(emitter, "x3", "x1", "x2"); // box the borrowed associative-array payload into an owned mixed cell
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
                    super::super::super::emit_box_runtime_payload_as_mixed(emitter, "rcx", "rdi", "rsi"); // box the borrowed associative-array payload into an owned mixed cell
                }
                _ => {
                    emitter.instruction("mov rax, rdi");                        // move the borrowed associative-array pointer payload into the standard integer result register
                }
            },
        }
        abi::emit_jump(emitter, &done);                                           // skip the not-found fallback after materializing the successful lookup result

        emitter.label(&not_found);
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::MAX - 1); // materialize the shared null sentinel for associative-array misses
        emitter.label(&done);
        return val_ty;
    }

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the array pointer while evaluating the index expression
    emit_expr(index, emitter, ctx, data);
    let array_reg = abi::symbol_scratch_reg(emitter);
    let len_reg = abi::temp_int_reg(emitter.target);
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_pop_reg(emitter, array_reg);                                      // restore the array pointer into a scratch register
    emitter.comment("array access");
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };

    let null_label = ctx.next_label("arr_null");
    let ok_label = ctx.next_label("arr_ok");
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
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {ok_label}")),         // skip null sentinel fallback
        Arch::X86_64 => emitter.instruction(&format!("jmp {ok_label}")),        // skip null sentinel fallback
    }

    emitter.label(&null_label);
    abi::emit_load_int_immediate(emitter, result_reg, 0x7fff_ffff_ffff_fffe);   // materialize the runtime null sentinel for out-of-bounds access
    emitter.label(&ok_label);

    elem_ty
}

pub(crate) fn emit_buffer_new(
    element_type: &TypeExpr,
    len: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let len_ty = emit_expr(len, emitter, ctx, data);
    let elem_ty = resolve_buffer_element_type(element_type, ctx);
    let stride = packed_type_size(&elem_ty, &ctx.packed_classes).unwrap_or(8);
    if len_ty != PhpType::Int {
        emitter.comment("WARNING: buffer_new length was not statically typed as int");
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x1, #{}", stride));               // pass the element stride to the ARM buffer allocation helper in the second integer argument register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", stride));               // pass the element stride to the x86_64 buffer allocation helper without clobbering the computed length in rax
        }
    }
    abi::emit_call_label(emitter, "__rt_buffer_new");                           // allocate the buffer header plus contiguous payload through the target-aware runtime helper
    PhpType::Buffer(Box::new(elem_ty))
}

fn resolve_buffer_element_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::Ptr(target) => {
            PhpType::Pointer(target.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Named(name) => {
            if ctx.packed_classes.contains_key(name.as_str()) {
                PhpType::Packed(name.as_str().to_string())
            } else {
                PhpType::Int
            }
        }
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx)))
        }
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Nullable(_) | TypeExpr::Union(_) => PhpType::Int,
    }
}
