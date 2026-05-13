use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_depth_enter: increment `_json_active_depth` and compare against
/// `_json_depth_limit`. When the depth crosses the limit, route through
/// `__rt_json_throw_error(JSON_ERROR_DEPTH)`; when the throw flag is clear
/// the helper returns and the encoder keeps producing partial output (the
/// error slot stays populated for `json_last_error()` to surface).
///
/// The depth counter is incremented unconditionally so the matching exit
/// helper can decrement once per container regardless of whether the
/// limit was breached.
pub(crate) fn emit_json_depth_enter(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_enter_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_depth_enter ---");
    emitter.label_global("__rt_json_depth_enter");

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x10, [x9]");                                       // load the current encoding depth
    emitter.instruction("add x10, x10, #1");                                    // bump the depth for the new container
    emitter.instruction("str x10, [x9]");                                       // publish the bumped depth
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_depth_limit");
    emitter.instruction("ldr x11, [x11]");                                      // load the user-supplied / default depth limit
    emitter.instruction("cmp x10, x11");                                        // is the new depth still within the budget?
    emitter.instruction("b.le __rt_json_depth_enter_ok");                       // budget is fine, keep going (encode semantics: <= limit OK)
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and link register before the throw helper
    emitter.instruction("mov x29, sp");                                         // establish a stable frame for the helper call
    emitter.instruction("mov x0, #1");                                          // JSON_ERROR_DEPTH = 1
    emitter.instruction("bl __rt_json_throw_error");                            // record the error and throw when JSON_THROW_ON_ERROR is set
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and link register after a no-throw return
    emitter.label("__rt_json_depth_enter_ok");
    emitter.instruction("ret");                                                 // return to the container encoder
}

/// __rt_json_depth_exit: decrement `_json_active_depth` so a sibling
/// container can re-use the slot it just freed.
pub(crate) fn emit_json_depth_exit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_exit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_depth_exit ---");
    emitter.label_global("__rt_json_depth_exit");

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x10, [x9]");                                       // load the current encoding depth
    emitter.instruction("sub x10, x10, #1");                                    // step back out of the just-finished container
    emitter.instruction("str x10, [x9]");                                       // publish the decremented depth
    emitter.instruction("ret");                                                 // return to the container encoder
}

fn emit_enter_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_depth_enter ---");
    emitter.label_global("__rt_json_depth_enter");

    emitter.instruction("mov rax, QWORD PTR [rip + _json_active_depth]");       // load the current encoding depth
    emitter.instruction("add rax, 1");                                          // bump the depth for the new container
    emitter.instruction("mov QWORD PTR [rip + _json_active_depth], rax");       // publish the bumped depth
    emitter.instruction("mov rdx, QWORD PTR [rip + _json_depth_limit]");        // load the depth limit
    emitter.instruction("cmp rax, rdx");                                        // is the new depth still within the budget?
    emitter.instruction("jle __rt_json_depth_enter_ok_x");                      // budget is fine, keep going (encode semantics: <= limit OK)
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the throw helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for the helper call
    emitter.instruction("mov rax, 1");                                          // JSON_ERROR_DEPTH = 1
    emitter.instruction("call __rt_json_throw_error");                          // record the error and throw when JSON_THROW_ON_ERROR is set
    emitter.instruction("mov rsp, rbp");                                        // unwind the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.label("__rt_json_depth_enter_ok_x");
    emitter.instruction("ret");                                                 // return to the container encoder
}

fn emit_exit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_depth_exit ---");
    emitter.label_global("__rt_json_depth_exit");

    emitter.instruction("mov rax, QWORD PTR [rip + _json_active_depth]");       // load the current encoding depth
    emitter.instruction("sub rax, 1");                                          // step back out of the just-finished container
    emitter.instruction("mov QWORD PTR [rip + _json_active_depth], rax");       // publish the decremented depth
    emitter.instruction("ret");                                                 // return to the container encoder
}
