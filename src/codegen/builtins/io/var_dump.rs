use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

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
        PhpType::Int => {
            // -- check for null sentinel --
            let not_null = ctx.next_label("vd_not_null");
            let done = ctx.next_label("vd_done");
            emitter.instruction("movz x9, #0xFFFE");                            // load lowest 16 bits of null sentinel
            emitter.instruction("movk x9, #0xFFFF, lsl #16");                   // insert bits 16-31
            emitter.instruction("movk x9, #0xFFFF, lsl #32");                   // insert bits 32-47
            emitter.instruction("movk x9, #0x7FFF, lsl #48");                   // insert bits 48-63
            emitter.instruction("cmp x0, x9");                                  // compare with null sentinel
            emitter.instruction(&format!("b.ne {}", not_null));                 // branch if not null
            // -- print "NULL\n" --
            let (lbl, len) = data.add_string(b"NULL\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", lbl));             // load NULL string page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", lbl));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", len));                  // string length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            emitter.instruction(&format!("b {}", done));                        // skip int path
            // -- print "int(VALUE)\n" --
            emitter.label(&not_null);
            let (pre, pre_len) = data.add_string(b"int(");
            emitter.instruction(&format!("adrp x1, {}@PAGE", pre));             // load "int(" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", pre));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", pre_len));              // prefix length
            emitter.instruction("str x0, [sp, #-16]!");                         // save int value
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            emitter.instruction("ldr x0, [sp], #16");                           // restore int value
            emitter.instruction("bl __rt_itoa");                                // convert int to string → x1/x2
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            let (suf, suf_len) = data.add_string(b")\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", suf));             // load ")\n" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", suf));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", suf_len));              // suffix length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            emitter.label(&done);
        }
        PhpType::Float => {
            // -- print "float(VALUE)\n" --
            let (pre, pre_len) = data.add_string(b"float(");
            emitter.instruction("bl __rt_ftoa");                                // convert float to string → x1/x2
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save float string
            let (pre2, _) = (pre.clone(), pre_len);
            emitter.instruction(&format!("adrp x1, {}@PAGE", pre2));            // load "float(" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", pre2));      // resolve address
            emitter.instruction(&format!("mov x2, #{}", pre_len));              // prefix length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore float string
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            let (suf, suf_len) = data.add_string(b")\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", suf));             // load ")\n" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", suf));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", suf_len));              // suffix length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
        }
        PhpType::Str => {
            // -- print "string(LEN) \"VALUE\"\n" --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save string ptr/len
            let (pre, pre_len) = data.add_string(b"string(");
            emitter.instruction(&format!("adrp x1, {}@PAGE", pre));             // load "string(" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", pre));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", pre_len));              // prefix length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            // -- print the string length as an integer --
            emitter.instruction("ldp x1, x2, [sp]");                            // peek string ptr/len (don't pop)
            emitter.instruction("mov x0, x2");                                  // move length to x0 for itoa
            emitter.instruction("bl __rt_itoa");                                // convert length to decimal string
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            // -- print ') "' --
            let (mid, mid_len) = data.add_string(b") \"");
            emitter.instruction(&format!("adrp x1, {}@PAGE", mid));             // load ') "' page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", mid));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", mid_len));              // middle part length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            // -- print the actual string value --
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop string ptr/len
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            // -- print '"\n' --
            let (end, end_len) = data.add_string(b"\"\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", end));             // load '"\n' page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", end));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", end_len));              // suffix length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
        }
        PhpType::Bool => {
            // -- print "bool(true)\n" or "bool(false)\n" --
            let true_label = ctx.next_label("vd_true");
            let done = ctx.next_label("vd_done");
            emitter.instruction("cmp x0, #0");                                  // test boolean value
            emitter.instruction(&format!("b.ne {}", true_label));               // branch if true
            let (f_str, f_len) = data.add_string(b"bool(false)\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", f_str));           // load "bool(false)\n" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", f_str));     // resolve address
            emitter.instruction(&format!("mov x2, #{}", f_len));                // string length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            emitter.instruction(&format!("b {}", done));                        // skip true path
            emitter.label(&true_label);
            let (t_str, t_len) = data.add_string(b"bool(true)\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", t_str));           // load "bool(true)\n" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", t_str));     // resolve address
            emitter.instruction(&format!("mov x2, #{}", t_len));                // string length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            emitter.label(&done);
        }
        PhpType::Void => {
            // -- print "NULL\n" --
            let (lbl, len) = data.add_string(b"NULL\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", lbl));             // load "NULL\n" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", lbl));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", len));                  // string length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
        }
        PhpType::Array(elem_ty) | PhpType::AssocArray { value: elem_ty, .. } => {
            // -- print simplified array dump --
            let (pre, pre_len) = data.add_string(b"array(");
            emitter.instruction("str x0, [sp, #-16]!");                         // save array pointer
            emitter.instruction(&format!("adrp x1, {}@PAGE", pre));             // load "array(" page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", pre));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", pre_len));              // prefix length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            // -- print element count --
            emitter.instruction("ldr x0, [sp]");                                // peek array pointer
            emitter.instruction("ldr x0, [x0]");                                // load array length
            emitter.instruction("bl __rt_itoa");                                // convert to string
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            // -- print ") {\n}\n" --
            let (suf, suf_len) = data.add_string(b") {\n}\n");
            emitter.instruction(&format!("adrp x1, {}@PAGE", suf));             // load suffix page
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", suf));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", suf_len));              // suffix length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.instruction("mov x16, #4");                                 // syscall write
            emitter.instruction("svc #0x80");                                   // invoke kernel
            emitter.instruction("ldr x0, [sp], #16");                           // pop array pointer
            let _ = elem_ty;
        }
    }
    Some(PhpType::Void)
}
