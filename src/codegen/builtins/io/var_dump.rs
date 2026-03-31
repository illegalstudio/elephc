use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

fn emit_write_literal(emitter: &mut Emitter, data: &mut DataSection, bytes: &[u8]) {
    let (lbl, len) = data.add_string(bytes);
    emitter.instruction(&format!("adrp x1, {}@PAGE", lbl));                     // load the literal page address
    emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", lbl));               // resolve the literal string address
    emitter.instruction(&format!("mov x2, #{}", len));                          // load the literal string length
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.instruction("mov x16, #4");                                         // syscall write
    emitter.instruction("svc #0x80");                                           // invoke kernel
}

fn emit_var_dump_int(emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    let not_null = ctx.next_label("vd_not_null");
    let done = ctx.next_label("vd_done");
    emitter.instruction("movz x9, #0xFFFE");                                    // load lowest 16 bits of the null sentinel
    emitter.instruction("movk x9, #0xFFFF, lsl #16");                           // insert sentinel bits 16-31
    emitter.instruction("movk x9, #0xFFFF, lsl #32");                           // insert sentinel bits 32-47
    emitter.instruction("movk x9, #0x7FFF, lsl #48");                           // insert sentinel bits 48-63
    emitter.instruction("cmp x0, x9");                                          // compare the int payload with the null sentinel
    emitter.instruction(&format!("b.ne {}", not_null));                         // branch to the ordinary int path when not null
    emit_write_literal(emitter, data, b"NULL\n");
    emitter.instruction(&format!("b {}", done));                                // skip the int formatter after printing NULL
    emitter.label(&not_null);
    emitter.instruction("str x0, [sp, #-16]!");                                 // save the integer payload before prefix writes clobber x0
    emit_write_literal(emitter, data, b"int(");
    emitter.instruction("ldr x0, [sp], #16");                                   // restore the integer payload after the prefix write
    emitter.instruction("bl __rt_itoa");                                        // convert the integer payload to decimal text
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.instruction("mov x16, #4");                                         // syscall write
    emitter.instruction("svc #0x80");                                           // invoke kernel
    emit_write_literal(emitter, data, b")\n");
    emitter.label(&done);
}

fn emit_var_dump_float(emitter: &mut Emitter, data: &mut DataSection) {
    emitter.instruction("bl __rt_ftoa");                                        // convert the float payload to decimal text
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // save the converted float text across literal writes
    emit_write_literal(emitter, data, b"float(");
    emitter.instruction("ldp x1, x2, [sp], #16");                               // restore the converted float text
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.instruction("mov x16, #4");                                         // syscall write
    emitter.instruction("svc #0x80");                                           // invoke kernel
    emit_write_literal(emitter, data, b")\n");
}

fn emit_var_dump_string(emitter: &mut Emitter, data: &mut DataSection) {
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // save the string payload across prefix and suffix writes
    emit_write_literal(emitter, data, b"string(");
    emitter.instruction("ldp x1, x2, [sp]");                                    // peek the saved string pointer and length without popping
    emitter.instruction("mov x0, x2");                                          // move the string length into the itoa argument register
    emitter.instruction("bl __rt_itoa");                                        // convert the string length to decimal text
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.instruction("mov x16, #4");                                         // syscall write
    emitter.instruction("svc #0x80");                                           // invoke kernel
    emit_write_literal(emitter, data, b") \"");
    emitter.instruction("ldp x1, x2, [sp], #16");                               // restore the original string payload
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.instruction("mov x16, #4");                                         // syscall write
    emitter.instruction("svc #0x80");                                           // invoke kernel
    emit_write_literal(emitter, data, b"\"\n");
}

fn emit_var_dump_bool(emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    let true_label = ctx.next_label("vd_true");
    let done = ctx.next_label("vd_done");
    emitter.instruction("cmp x0, #0");                                          // test the boolean payload
    emitter.instruction(&format!("b.ne {}", true_label));                       // branch when the payload is true
    emit_write_literal(emitter, data, b"bool(false)\n");
    emitter.instruction(&format!("b {}", done));                                // skip the true branch after printing false
    emitter.label(&true_label);
    emit_write_literal(emitter, data, b"bool(true)\n");
    emitter.label(&done);
}

fn emit_var_dump_null(emitter: &mut Emitter, data: &mut DataSection) {
    emit_write_literal(emitter, data, b"NULL\n");
}

fn emit_var_dump_array(emitter: &mut Emitter, data: &mut DataSection) {
    emitter.instruction("str x0, [sp, #-16]!");                                 // save the array/hash pointer across literal writes
    emit_write_literal(emitter, data, b"array(");
    emitter.instruction("ldr x0, [sp]");                                        // reload the saved array/hash pointer
    emitter.instruction("ldr x0, [x0]");                                        // load the container element count from the header
    emitter.instruction("bl __rt_itoa");                                        // convert the element count to decimal text
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.instruction("mov x16, #4");                                         // syscall write
    emitter.instruction("svc #0x80");                                           // invoke kernel
    emit_write_literal(emitter, data, b") {\n}\n");
    emitter.instruction("ldr x0, [sp], #16");                                   // restore the array/hash pointer after printing
}

fn emit_var_dump_callable(emitter: &mut Emitter, data: &mut DataSection) {
    emit_write_literal(emitter, data, b"callable\n");
}

fn emit_var_dump_object_name(emitter: &mut Emitter, data: &mut DataSection, class_name: &str) {
    let obj_str = format!("object({})\n", class_name);
    emit_write_literal(emitter, data, obj_str.as_bytes());
}

fn emit_var_dump_dynamic_object(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let mut classes: Vec<_> = ctx
        .classes
        .iter()
        .map(|(class_name, class_info)| (class_name.clone(), class_info.class_id))
        .collect();
    classes.sort_by_key(|(_, class_id)| *class_id);
    let mut cases = Vec::with_capacity(classes.len());
    let null_label = ctx.next_label("vd_object_null");
    let fallback = ctx.next_label("vd_object_fallback");
    let done = ctx.next_label("vd_object_done");

    emitter.instruction(&format!("cbz x0, {}", null_label));                    // null object pointers print as NULL
    emitter.instruction("ldr x9, [x0]");                                        // load the runtime class id from the object header
    for (class_name, class_id) in classes {
        let case = ctx.next_label("vd_object_case");
        cases.push((case.clone(), class_name.clone()));
        emitter.instruction(&format!("cmp x9, #{}", class_id));                 // compare the runtime class id against a known class id
        emitter.instruction(&format!("b.eq {}", case));                         // branch when the class id matches this known class
    }
    emitter.instruction(&format!("b {}", fallback));                            // unknown runtime class ids fall back to a generic object marker
    for (case, class_name) in cases {
        emitter.label(&case);
        emit_var_dump_object_name(emitter, data, &class_name);
        emitter.instruction(&format!("b {}", done));                            // finish after printing the matching object class
    }
    emitter.label(&null_label);
    emit_var_dump_null(emitter, data);
    emitter.instruction(&format!("b {}", done));                                // finish after printing NULL for a null object pointer
    emitter.label(&fallback);
    emit_write_literal(emitter, data, b"object\n");
    emitter.label(&done);
}

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("var_dump()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match &ty {
        PhpType::Int => emit_var_dump_int(emitter, ctx, data),
        PhpType::Float => emit_var_dump_float(emitter, data),
        PhpType::Str => emit_var_dump_string(emitter, data),
        PhpType::Bool => emit_var_dump_bool(emitter, ctx, data),
        PhpType::Void => emit_var_dump_null(emitter, data),
        PhpType::Mixed => {
            let int_case = ctx.next_label("vd_mixed_int");
            let string_case = ctx.next_label("vd_mixed_string");
            let float_case = ctx.next_label("vd_mixed_float");
            let bool_case = ctx.next_label("vd_mixed_bool");
            let array_case = ctx.next_label("vd_mixed_array");
            let object_case = ctx.next_label("vd_mixed_object");
            let null_case = ctx.next_label("vd_mixed_null");
            let done = ctx.next_label("vd_mixed_done");

            emitter.instruction("bl __rt_mixed_unbox");                         // unwrap the boxed mixed payload before formatting it
            emitter.instruction("cmp x0, #0");                                  // does the mixed payload hold an int?
            emitter.instruction(&format!("b.eq {}", int_case));                 // ints reuse the ordinary int var_dump formatter
            emitter.instruction("cmp x0, #1");                                  // does the mixed payload hold a string?
            emitter.instruction(&format!("b.eq {}", string_case));              // strings reuse the ordinary string var_dump formatter
            emitter.instruction("cmp x0, #2");                                  // does the mixed payload hold a float?
            emitter.instruction(&format!("b.eq {}", float_case));               // floats reuse the ordinary float var_dump formatter
            emitter.instruction("cmp x0, #3");                                  // does the mixed payload hold a bool?
            emitter.instruction(&format!("b.eq {}", bool_case));                // bools reuse the ordinary bool var_dump formatter
            emitter.instruction("cmp x0, #4");                                  // does the mixed payload hold an indexed array?
            emitter.instruction(&format!("b.eq {}", array_case));               // arrays reuse the ordinary array var_dump formatter
            emitter.instruction("cmp x0, #5");                                  // does the mixed payload hold an associative array?
            emitter.instruction(&format!("b.eq {}", array_case));               // associative arrays reuse the ordinary array var_dump formatter
            emitter.instruction("cmp x0, #6");                                  // does the mixed payload hold an object/callable heap value?
            emitter.instruction(&format!("b.eq {}", object_case));              // objects use runtime class-id dispatch for their name
            emitter.instruction(&format!("b {}", null_case));                   // null and unknown tags print as NULL

            emitter.label(&int_case);
            emitter.instruction("mov x0, x1");                                  // move the unboxed int payload into the standard int result register
            emit_var_dump_int(emitter, ctx, data);
            emitter.instruction(&format!("b {}", done));                        // finish after printing the mixed int payload

            emitter.label(&string_case);
            emit_var_dump_string(emitter, data);                                     // x1/x2 already carry the unboxed string payload
            emitter.instruction(&format!("b {}", done));                        // finish after printing the mixed string payload

            emitter.label(&float_case);
            emitter.instruction("fmov d0, x1");                                 // move the unboxed float bits into the FP argument register
            emit_var_dump_float(emitter, data);
            emitter.instruction(&format!("b {}", done));                        // finish after printing the mixed float payload

            emitter.label(&bool_case);
            emitter.instruction("mov x0, x1");                                  // move the unboxed bool payload into the standard bool register
            emit_var_dump_bool(emitter, ctx, data);
            emitter.instruction(&format!("b {}", done));                        // finish after printing the mixed bool payload

            emitter.label(&array_case);
            emitter.instruction("mov x0, x1");                                  // move the unboxed container pointer into x0
            emit_var_dump_array(emitter, data);
            emitter.instruction(&format!("b {}", done));                        // finish after printing the mixed array payload

            emitter.label(&object_case);
            emitter.instruction("mov x0, x1");                                  // move the unboxed object pointer into x0
            emit_var_dump_dynamic_object(emitter, ctx, data);
            emitter.instruction(&format!("b {}", done));                        // finish after printing the mixed object payload

            emitter.label(&null_case);
            emit_var_dump_null(emitter, data);                                       // print NULL for null/unknown mixed payloads
            emitter.label(&done);
        }
        PhpType::Array(elem_ty) | PhpType::AssocArray { value: elem_ty, .. } => {
            emit_var_dump_array(emitter, data);
            let _ = elem_ty;
        }
        PhpType::Callable => emit_var_dump_callable(emitter, data),
        PhpType::Object(class_name) => emit_var_dump_object_name(emitter, data, class_name),
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            // -- print pointer as hex address followed by newline --
            emitter.instruction("bl __rt_ptoa");                                // x0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            let (lbl, len) = data.add_string(b"\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", lbl));             // load newline page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", lbl));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", len));                  // string length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
        }
    }
    Some(PhpType::Void)
}
