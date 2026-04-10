use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::{abi, platform::Arch};
use crate::parser::ast::TypeExpr;
use crate::types::packed_type_size;
use super::{emit_expr, retain_borrowed_heap_arg, Expr, ExprKind, PhpType};

pub(super) fn emit_array_literal(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if elems.is_empty() {
        emitter.instruction("mov x0, #4");                                      // initial capacity: 4 (grows dynamically)
        emitter.instruction("mov x1, #16");                                     // element size: 16 bytes (supports int and string)
        emitter.instruction("bl __rt_array_new");                               // call runtime to heap-allocate array struct
        return PhpType::Array(Box::new(PhpType::Int));
    }

    let has_spread = elems.iter().any(|e| matches!(e.kind, ExprKind::Spread(_)));
    if has_spread {
        return emit_array_literal_with_spread(elems, emitter, ctx, data);
    }

    let first_ty = match &elems[0].kind {
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            PhpType::Array(Box::new(PhpType::Int))
        }
        _ => PhpType::Int,
    };
    let es: usize = match &first_ty {
        PhpType::Str => 16,
        _ => 8,
    };

    emitter.comment("array literal");
    emitter.instruction(&format!("mov x0, #{}", elems.len()));                  // capacity: exact element count (grows if needed)
    emitter.instruction(&format!("mov x1, #{}", es));                           // element size in bytes (8=int/ptr, 16=string)
    emitter.instruction("bl __rt_array_new");                                   // call runtime to heap-allocate array struct
    emitter.instruction("str x0, [sp, #-16]!");                                 // save array pointer on stack while filling

    let mut actual_elem_ty = PhpType::Int;
    for (i, elem) in elems.iter().enumerate() {
        let ty = emit_expr(elem, emitter, ctx, data);
        if i == 0 {
            actual_elem_ty = ty.clone();
        }
        retain_borrowed_heap_arg(emitter, elem, &ty);
        emitter.instruction("ldr x9, [sp]");                                    // peek array pointer from stack (no pop)
        if i == 0 {
            emit_array_value_type_stamp(emitter, "x9", &ty);
        }
        match &ty {
            PhpType::Int | PhpType::Bool | PhpType::Callable => {
                emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8)); // store int/bool/callable element at data offset
            }
            PhpType::Float => {
                emitter.instruction(&format!("str d0, [x9, #{}]", 24 + i * 8)); // store float element at data offset
            }
            PhpType::Str => {
                emitter.instruction(&format!("str x1, [x9, #{}]", 24 + i * 16)); //store string pointer at data offset
                emitter.instruction(&format!("str x2, [x9, #{}]", 24 + i * 16 + 8)); //store string length right after pointer
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8)); // store array/object pointer at data offset
            }
            _ => {}
        }
        emitter.instruction(&format!("mov x10, #{}", i + 1));                   // new length after adding this element
        emitter.instruction("str x10, [x9]");                                   // write updated length to array header
    }

    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer from stack into x0
    PhpType::Array(Box::new(actual_elem_ty))
}

pub(super) fn emit_array_literal_with_spread(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("array literal with spread");
    emitter.instruction("mov x0, #16");                                         // initial capacity: 16 elements
    emitter.instruction("mov x1, #8");                                          // element size: 8 bytes (int-sized)
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #-16]!");                                 // save dest array pointer on stack

    let mut actual_elem_ty = PhpType::Int;

    for (i, elem) in elems.iter().enumerate() {
        if let ExprKind::Spread(inner) = &elem.kind {
            emitter.comment("spread array into dest");
            let src_ty = emit_expr(inner, emitter, ctx, data);
            if (i == 0 || actual_elem_ty == PhpType::Int)
                && matches!(&src_ty, PhpType::Array(_))
            {
                if let PhpType::Array(inner) = &src_ty {
                    actual_elem_ty = inner.as_ref().clone();
                }
            }
            emitter.instruction("mov x1, x0");                                  // x1 = source array pointer
            emitter.instruction("ldr x0, [sp]");                                // x0 = dest array pointer (peek)
            if matches!(&src_ty, PhpType::Array(inner) if inner.is_refcounted()) {
                emitter.instruction("bl __rt_array_merge_into_refcounted");     // append src elements while retaining borrowed heap payloads
            } else {
                emitter.instruction("bl __rt_array_merge_into");                // append all src elements to dest array
            }
            emitter.instruction("str x0, [sp]");                                // persist the possibly-grown dest array pointer after the spread merge
        } else {
            let ty = emit_expr(elem, emitter, ctx, data);
            if i == 0 || actual_elem_ty == PhpType::Int {
                actual_elem_ty = ty.clone();
            }
            retain_borrowed_heap_arg(emitter, elem, &ty);
            emitter.instruction("ldr x9, [sp]");                                // peek dest array pointer from stack
            match &ty {
                PhpType::Int | PhpType::Bool => {
                    emitter.instruction("mov x1, x0");                          // x1 = value to push
                    emitter.instruction("mov x0, x9");                          // x0 = array pointer
                    emitter.instruction("bl __rt_array_push_int");              // push value onto array
                    emitter.instruction("str x0, [sp]");                        // persist the possibly-grown dest array pointer after the push
                }
                PhpType::Float => {
                    emitter.instruction("fmov x1, d0");                         // move float bits to int register
                    emitter.instruction("mov x0, x9");                          // x0 = array pointer
                    emitter.instruction("bl __rt_array_push_int");              // push value onto array
                    emitter.instruction("str x0, [sp]");                        // persist the possibly-grown dest array pointer after the push
                }
                _ => {
                    emitter.instruction("mov x1, x0");                          // x1 = value to push
                    emitter.instruction("mov x0, x9");                          // x0 = array pointer
                    if ty.is_refcounted() {
                        emitter.instruction("bl __rt_array_push_refcounted");   // push retained refcounted payload and stamp array metadata
                    } else {
                        emitter.instruction("bl __rt_array_push_int");          // push value onto array
                    }
                    emitter.instruction("str x0, [sp]");                        // persist the possibly-grown dest array pointer after the push
                }
            }
        }
    }

    emitter.instruction("ldr x0, [sp], #16");                                   // pop dest array pointer from stack into x0
    PhpType::Array(Box::new(actual_elem_ty))
}

pub(super) fn emit_array_value_type_stamp(
    emitter: &mut Emitter,
    array_reg: &str,
    elem_ty: &PhpType,
) {
    let value_type_tag = match elem_ty {
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Str => 1,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Void => 8,
        _ => return,
    };
    emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg));             // load the packed array kind word from the heap header
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // keep only the persistent indexed-array metadata bits
    emitter.instruction(&format!("mov x11, #{}", value_type_tag));              // materialize the runtime array value_type tag
    emitter.instruction("lsl x11, x11, #8");                                    // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("orr x10, x10, x11");                                   // combine the heap kind with the array value_type tag
    emitter.instruction(&format!("str x10, [{}, #-8]", array_reg));             // persist the packed array kind word in the heap header
}

pub(super) fn emit_assoc_array_literal(
    pairs: &[(Expr, Expr)],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("assoc array literal");

    let first_value_ty = super::super::functions::infer_contextual_type(&pairs[0].1, ctx);
    let value_type_tag = super::super::runtime_value_tag(&first_value_ty);

    emitter.instruction(&format!("mov x0, #{}", std::cmp::max(pairs.len() * 2, 16))); //initial capacity: max of pair count*2 or 16
    emitter.instruction(&format!("mov x1, #{}", value_type_tag));               // value type tag
    emitter.instruction("bl __rt_hash_new");                                    // create hash table → x0=ptr
    emitter.instruction("str x0, [sp, #-16]!");                                 // save hash table pointer

    let mut val_ty = PhpType::Int;
    for (i, pair) in pairs.iter().enumerate() {
        emit_expr(&pair.0, emitter, ctx, data);
        emitter.instruction("stp x1, x2, [sp, #-16]!");                         // save key ptr/len
        let ty = emit_expr(&pair.1, emitter, ctx, data);
        retain_borrowed_heap_arg(emitter, &pair.1, &ty);
        if i == 0 {
            val_ty = ty.clone();
        } else if ty != val_ty {
            val_ty = PhpType::Mixed;
        }
        let (val_lo, val_hi) = match &ty {
            PhpType::Int | PhpType::Bool => ("x0", "xzr"),
            PhpType::Str => {
                emitter.instruction("bl __rt_str_persist");                     // copy string to heap, x1=heap_ptr, x2=len
                ("x1", "x2")
            }
            PhpType::Float => {
                emitter.instruction("fmov x9, d0");                             // move float bits to integer register
                ("x9", "xzr")
            }
            _ => ("x0", "xzr"),
        };
        emitter.instruction(&format!("mov x3, {}", val_lo));                    // value_lo
        emitter.instruction(&format!("mov x4, {}", val_hi));                    // value_hi
        emitter.instruction(&format!("mov x5, #{}", super::super::runtime_value_tag(&ty))); //value_tag for this specific assoc entry
        emitter.instruction("ldp x1, x2, [sp], #16");                           // pop key ptr/len
        emitter.instruction("ldr x0, [sp]");                                    // peek hash table pointer
        emitter.instruction("bl __rt_hash_set");                                // insert key-value pair (x0 = table, may be new)
        emitter.instruction("str x0, [sp]");                                    // update stored table pointer after possible growth
    }

    emitter.instruction("ldr x0, [sp], #16");                                   // pop hash table pointer into x0

    let key_ty = match &pairs[0].0.kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        _ => PhpType::Str,
    };

    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(val_ty),
    }
}

pub(super) fn emit_match_expr(
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
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save string subject
        }
        PhpType::Float => {
            emitter.instruction("str d0, [sp, #-16]!");                         // save float subject
        }
        _ => {
            emitter.instruction("str x0, [sp, #-16]!");                         // save int/bool subject
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
                PhpType::Str => {
                    emitter.instruction("mov x3, x1");                          // move pattern ptr to x3
                    emitter.instruction("mov x4, x2");                          // move pattern len to x4
                    emitter.instruction("ldp x1, x2, [sp]");                    // peek subject string
                    emitter.instruction("bl __rt_str_eq");                      // compare strings → x0=1 if equal
                }
                PhpType::Float => {
                    emitter.instruction("ldr d1, [sp]");                        // peek subject float
                    emitter.instruction("fcmp d1, d0");                         // compare floats
                    emitter.instruction("cset x0, eq");                         // x0=1 if equal
                }
                _ => {
                    emitter.instruction("ldr x9, [sp]");                        // peek subject int/bool
                    emitter.instruction("cmp x9, x0");                          // compare integers
                    emitter.instruction("cset x0, eq");                         // x0=1 if equal
                }
            }
            emitter.instruction(&format!("cbnz x0, {}", arm_label));            // if matched, jump to arm body
            if i == patterns.len() - 1 {
                emitter.instruction(&format!("b {}", next_arm));                // no pattern matched → try next arm
            }
            let _ = pat_ty;
        }

        emitter.label(&arm_label);
        result_ty = emit_expr(result, emitter, ctx, data);
        emitter.instruction(&format!("b {}", end_label));                       // jump to end after evaluating arm
        emitter.label(&next_arm);
    }

    if let Some(def) = default {
        result_ty = emit_expr(def, emitter, ctx, data);
    } else {
        emitter.instruction("bl __rt_match_unhandled");                         // fatal when no arm matched and the match has no default arm
    }

    emitter.label(&end_label);
    emitter.instruction("add sp, sp, #16");                                     // deallocate subject save slot
    result_ty
}

pub(super) fn emit_array_access(
    array: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let arr_ty = emit_expr(array, emitter, ctx, data);

    if let PhpType::Buffer(elem_ty) = &arr_ty {
        emitter.instruction("str x0, [sp, #-16]!");                             // push buffer pointer while evaluating the index expression
        emit_expr(index, emitter, ctx, data);
        emitter.instruction("ldr x9, [sp], #16");                               // pop buffer pointer into scratch register x9
        emitter.comment("buffer access");
        let uaf_ok = ctx.next_label("buf_uaf_ok");
        emitter.instruction(&format!("cbnz x9, {}", uaf_ok));                   // skip fatal if buffer pointer is valid
        emitter.instruction("b __rt_buffer_use_after_free");                    // abort — buffer was freed
        emitter.label(&uaf_ok);
        let elem_ty = *elem_ty.clone();
        let bounds_ok = ctx.next_label("buffer_idx_ok");
        let oob_ok = ctx.next_label("buf_oob_ok");
        emitter.instruction("cmp x0, #0");                                      // reject negative buffer indexes
        emitter.instruction(&format!("b.ge {}", oob_ok));                       // skip fatal if index is non-negative
        emitter.instruction("b __rt_buffer_bounds_fail");                       // abort — negative index
        emitter.label(&oob_ok);
        emitter.instruction("ldr x10, [x9]");                                   // load buffer length from header
        emitter.instruction("cmp x0, x10");                                     // compare index against logical buffer length
        emitter.instruction(&format!("b.lo {}", bounds_ok));                    // continue once the index is in range
        emitter.instruction("mov x1, x10");                                     // pass buffer length to the bounds-failure helper
        emitter.instruction("bl __rt_buffer_bounds_fail");                      // abort with a dedicated buffer bounds message
        emitter.label(&bounds_ok);
        emitter.instruction("ldr x12, [x9, #8]");                               // load element stride from the buffer header
        emitter.instruction("add x9, x9, #16");                                 // skip the buffer header to reach the payload base
        emitter.instruction("madd x9, x0, x12, x9");                            // compute payload base + index*stride
        match &elem_ty {
            PhpType::Float => {
                emitter.instruction("ldr d0, [x9]");                            // load scalar float element from the contiguous payload
                return PhpType::Float;
            }
            PhpType::Packed(name) => {
                emitter.instruction("mov x0, x9");                              // expose the packed element address as a typed pointer
                return PhpType::Pointer(Some(name.clone()));
            }
            _ => {
                emitter.instruction("ldr x0, [x9]");                            // load scalar/pointer element from the contiguous payload
                return elem_ty;
            }
        }
    }

    if arr_ty == PhpType::Str {
        emitter.instruction("stp x1, x2, [sp, #-16]!");                         // save string ptr/len while evaluating the index expression
        emit_expr(index, emitter, ctx, data);
        emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the indexed string after the index expression
        emitter.comment("string indexing");

        let non_negative = ctx.next_label("str_idx_pos");
        let oob = ctx.next_label("str_idx_oob");
        let end = ctx.next_label("str_idx_end");

        // -- lower $str[$i] to substr-style access with length 1 --
        emitter.instruction("cmp x0, #0");                                      // check whether the requested string offset is negative
        emitter.instruction(&format!("b.ge {}", non_negative));                 // keep non-negative offsets as-is
        emitter.instruction("add x0, x2, x0");                                  // convert negative offsets to length + offset
        emitter.instruction("cmp x0, #0");                                      // check whether the adjusted offset still points before the string
        emitter.instruction(&format!("b.lt {}", oob));                          // negative offsets beyond -len return empty string
        emitter.label(&non_negative);
        emitter.instruction("cmp x0, x2");                                      // compare the offset against the string length
        emitter.instruction(&format!("b.ge {}", oob));                          // offsets at or beyond length return empty string
        emitter.instruction("add x1, x1, x0");                                  // advance the string pointer to the selected character
        emitter.instruction("mov x2, #1");                                      // string indexing returns exactly one character when in bounds
        emitter.instruction(&format!("b {}", end));                             // skip the out-of-bounds fallback
        emitter.label(&oob);
        emitter.instruction("mov x2, #0");                                      // out-of-bounds: return empty string
        emitter.label(&end);

        return PhpType::Str;
    }

    if let PhpType::AssocArray { value, .. } = &arr_ty {
        let val_ty = *value.clone();
        emitter.instruction("str x0, [sp, #-16]!");                             // push hash table pointer
        let _key_ty = emit_expr(index, emitter, ctx, data);
        emitter.instruction("mov x3, x1");                                      // move key ptr to x3 (temp)
        emitter.instruction("mov x4, x2");                                      // move key len to x4 (temp)
        emitter.instruction("ldr x0, [sp], #16");                               // pop hash table pointer
        emitter.instruction("mov x1, x3");                                      // key ptr
        emitter.instruction("mov x2, x4");                                      // key len
        emitter.comment("assoc array access");
        emitter.instruction("bl __rt_hash_get");                                // lookup key → x0=found, x1=val_lo, x2=val_hi, x3=val_tag

        let not_found = ctx.next_label("hash_miss");
        let done = ctx.next_label("hash_done");
        emitter.instruction(&format!("cbz x0, {not_found}"));                   // if found=0, jump to not-found handler

        match &val_ty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("mov x0, x1");                              // move value to x0
            }
            PhpType::Str => {}
            PhpType::Float => {
                emitter.instruction("fmov d0, x1");                             // move bits to float register
            }
            PhpType::Mixed => {
                super::super::emit_box_runtime_payload_as_mixed(emitter, "x3", "x1", "x2"); // box the borrowed entry payload into a mixed cell
            }
            _ => {
                emitter.instruction("mov x0, x1");                              // move value to x0
            }
        }
        emitter.instruction(&format!("b {done}"));                              // skip not-found fallback

        emitter.label(&not_found);
        emitter.instruction("movz x0, #0xFFFE");                                // load lowest 16 bits of null sentinel
        emitter.instruction("movk x0, #0xFFFF, lsl #16");                       // insert bits 16-31 of null sentinel
        emitter.instruction("movk x0, #0xFFFF, lsl #32");                       // insert bits 32-47 of null sentinel
        emitter.instruction("movk x0, #0x7FFF, lsl #48");                       // insert bits 48-63 of null sentinel
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
            abi::emit_load_from_address(emitter, len_reg, array_reg, 0);         // load array length from header (offset 0)
            emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));   // compare index against array length
            emitter.instruction(&format!("b.ge {null_label}"));                 // index >= length → null sentinel
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 0", result_reg));             // check if index is negative
            emitter.instruction(&format!("jl {null_label}"));                   // negative index → null sentinel
            abi::emit_load_from_address(emitter, len_reg, array_reg, 0);         // load array length from header (offset 0)
            emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));   // compare index against array length
            emitter.instruction(&format!("jge {null_label}"));                  // index >= length → null sentinel
        }
    }

    match &elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip 24-byte array header to reach data
                    emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", result_reg, array_reg, result_reg)); // load element at array + index*8
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip 24-byte array header to reach data
                    emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", result_reg, array_reg, result_reg)); // load element at array + index*8
                }
            }
        }
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

pub(super) fn emit_buffer_new(
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
    emitter.instruction(&format!("mov x1, #{}", stride));                       // pass element stride to the buffer allocation helper
    emitter.instruction("bl __rt_buffer_new");                                  // allocate the buffer header + contiguous payload
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
        TypeExpr::Buffer(inner) => PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx))),
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Nullable(_) | TypeExpr::Union(_) => PhpType::Int,
    }
}
