use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use super::super::{emit_expr, retain_borrowed_heap_arg, Expr, ExprKind, PhpType};

pub(crate) fn emit_array_literal(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if emitter.target.arch == Arch::X86_64
        && !elems.iter().any(|e| matches!(e.kind, ExprKind::Spread(_)))
    {
        return emit_array_literal_linux_x86_64(elems, emitter, ctx, data);
    }

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

fn emit_array_literal_linux_x86_64(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let first_ty = match elems.first().map(|expr| &expr.kind) {
        Some(ExprKind::StringLiteral(_)) => PhpType::Str,
        Some(ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) => {
            PhpType::Array(Box::new(PhpType::Int))
        }
        _ => PhpType::Int,
    };
    let elem_size = match &first_ty {
        PhpType::Str => 16,
        _ => 8,
    };
    let capacity = elems.len().max(4);

    emitter.comment("array literal");
    abi::emit_load_int_immediate(emitter, "rdi", capacity as i64);             // choose an indexed-array capacity that matches the x86_64 literal size policy
    abi::emit_load_int_immediate(emitter, "rsi", elem_size as i64);            // choose the runtime element slot width that matches the inferred literal element family
    abi::emit_call_label(emitter, "__rt_array_new");                            // allocate a real elephc indexed array so heap headers and runtime metadata stay valid on x86_64
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save array pointer on stack while filling literal elements

    let mut actual_elem_ty = first_ty;
    for (i, elem) in elems.iter().enumerate() {
        let ty = emit_expr(elem, emitter, ctx, data);
        if i == 0 {
            actual_elem_ty = ty.clone();
        }
        retain_borrowed_heap_arg(emitter, elem, &ty);
        emitter.instruction("mov r11, QWORD PTR [rsp]");                        // peek array pointer from the temporary stack slot
        if i == 0 {
            emit_array_value_type_stamp(emitter, "r11", &ty);                   // stamp the packed x86_64 array value_type tag once the first literal element fixes the runtime family
        }
        match &ty {
            PhpType::Int | PhpType::Bool | PhpType::Callable => {
                abi::emit_store_to_address(
                    emitter,
                    abi::int_result_reg(emitter),
                    "r11",
                    24 + i * 8,
                );
            }
            PhpType::Float => {
                abi::emit_store_to_address(
                    emitter,
                    abi::float_result_reg(emitter),
                    "r11",
                    24 + i * 8,
                );
            }
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                abi::emit_store_to_address(emitter, ptr_reg, "r11", 24 + i * 16);
                abi::emit_store_to_address(emitter, len_reg, "r11", 24 + i * 16 + 8);
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                abi::emit_store_to_address(
                    emitter,
                    abi::int_result_reg(emitter),
                    "r11",
                    24 + i * 8,
                );
            }
            _ => {}
        }
        abi::emit_load_int_immediate(emitter, "r10", (i + 1) as i64);           // materialize the logical indexed-array length after inserting this literal element
        abi::emit_store_to_address(emitter, "r10", "r11", 0);                   // publish the updated indexed-array length in the real array header
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return array pointer in the target integer result register
    PhpType::Array(Box::new(actual_elem_ty))
}

pub(crate) fn emit_array_literal_with_spread(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if emitter.target.arch == Arch::X86_64 {
        return emit_array_literal_with_spread_linux_x86_64(elems, emitter, ctx, data);
    }

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

fn emit_array_literal_with_spread_linux_x86_64(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("array literal with spread");
    emitter.instruction("mov rdi, 16");                                         // seed the destination indexed array with the same fixed initial capacity used by the ARM64 spread helper
    emitter.instruction("mov rsi, 8");                                          // use 8-byte slots because this helper still constructs scalar or pointer packed indexed arrays
    abi::emit_call_label(emitter, "__rt_array_new");                            // allocate the destination indexed array through the x86_64 runtime constructor
    abi::emit_push_reg(emitter, "rax");                                         // preserve the destination indexed-array pointer on the stack while evaluating spread sources and explicit elements

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
            emitter.instruction("mov rsi, rax");                                // place the source indexed-array pointer in the x86_64 merge helper source register
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // reload the destination indexed-array pointer from the stack without disturbing the literal construction state
            if matches!(&src_ty, PhpType::Array(inner) if inner.is_refcounted()) {
                abi::emit_call_label(emitter, "__rt_array_merge_into_refcounted"); // append retained child pointers from the source indexed array into the destination
            } else {
                abi::emit_call_label(emitter, "__rt_array_merge_into");         // append plain scalar payloads from the source indexed array into the destination
            }
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // persist the possibly-grown destination indexed-array pointer after the spread merge
        } else {
            let ty = emit_expr(elem, emitter, ctx, data);
            if i == 0 || actual_elem_ty == PhpType::Int {
                actual_elem_ty = ty.clone();
            }
            retain_borrowed_heap_arg(emitter, elem, &ty);
            emitter.instruction("mov r11, QWORD PTR [rsp]");                    // reload the destination indexed-array pointer from the stack without popping it
            match &ty {
                PhpType::Int | PhpType::Bool => {
                    emitter.instruction("mov rsi, rax");                        // place the scalar payload in the x86_64 append helper value register
                    emitter.instruction("mov rdi, r11");                        // place the destination indexed-array pointer in the x86_64 append helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_int");       // append the scalar payload into the destination indexed array
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // persist the possibly-grown destination indexed-array pointer after the append
                }
                PhpType::Float => {
                    emitter.instruction("movq rsi, xmm0");                      // move the floating-point payload bits into the scalar append helper value register
                    emitter.instruction("mov rdi, r11");                        // place the destination indexed-array pointer in the x86_64 append helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_int");       // append the floating-point payload bits as an 8-byte scalar slot
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // persist the possibly-grown destination indexed-array pointer after the append
                }
                PhpType::Str => {
                    emitter.instruction("mov rsi, rax");                        // place the string pointer in the x86_64 string append helper payload register
                    emitter.instruction("mov rdi, r11");                        // place the destination indexed-array pointer in the x86_64 string append helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_str");       // persist and append the string payload into the destination indexed array
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // persist the possibly-grown destination indexed-array pointer after the append
                }
                _ => {
                    emitter.instruction("mov rsi, rax");                        // place the payload pointer or scalar bits in the shared x86_64 append helper value register
                    emitter.instruction("mov rdi, r11");                        // place the destination indexed-array pointer in the shared x86_64 append helper receiver register
                    if ty.is_refcounted() {
                        abi::emit_call_label(emitter, "__rt_array_push_refcounted"); // append the retained refcounted payload and stamp the indexed-array value_type metadata
                    } else {
                        abi::emit_call_label(emitter, "__rt_array_push_int");   // append the payload bits through the scalar append helper
                    }
                    emitter.instruction("mov QWORD PTR [rsp], rax");            // persist the possibly-grown destination indexed-array pointer after the append
                }
            }
        }
    }

    abi::emit_pop_reg(emitter, "rax");                                          // pop the completed destination indexed-array pointer into the standard x86_64 expression result register
    PhpType::Array(Box::new(actual_elem_ty))
}

pub(crate) fn emit_array_value_type_stamp(
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
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg));     // load the packed array kind word from the heap header
            emitter.instruction("mov x12, #0x80ff");                            // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x10, x10, x12");                           // keep only the persistent indexed-array metadata bits
            emitter.instruction(&format!("mov x11, #{}", value_type_tag));      // materialize the runtime array value_type tag
            emitter.instruction("lsl x11, x11, #8");                            // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("orr x10, x10, x11");                           // combine the heap kind with the array value_type tag
            emitter.instruction(&format!("str x10, [{}, #-8]", array_reg));     // persist the packed array kind word in the heap header
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "r12");                                 // preserve the x86_64 nested-call scratch register before reusing it as a temporary array-stamp helper
            emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg)); // load the packed array kind word from the heap header
            emitter.instruction("mov r12, 0xffffffff000080ff");                 // materialize the x86_64 heap-kind preservation mask without clobbering the array base register
            emitter.instruction("and r10, r12");                                // preserve the x86_64 heap magic marker plus the indexed-array kind and persistent COW flag
            emitter.instruction(&format!("mov r12, {}", value_type_tag));       // materialize the runtime array value_type tag in a scratch register that does not alias the array base register
            emitter.instruction("shl r12, 8");                                  // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("or r10, r12");                                 // combine the preserved heap kind with the stamped array value_type tag
            emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg)); // persist the packed array kind word in the heap header
            abi::emit_pop_reg(emitter, "r12");                                  // restore the x86_64 nested-call scratch register after the array value-type stamp is complete
        }
    }
}
