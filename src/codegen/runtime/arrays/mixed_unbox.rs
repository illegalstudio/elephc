use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// mixed_unbox: unwrap nested mixed cells into a concrete runtime payload triple.
/// Input:  x0 = boxed mixed pointer
/// Output: x0 = runtime value tag, x1 = value_lo, x2 = value_hi
pub fn emit_mixed_unbox(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_unbox_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_unbox ---");
    emitter.label_global("__rt_mixed_unbox");

    // -- null mixed pointers behave like null payloads --
    emitter.instruction("cbz x0, __rt_mixed_unbox_null");                       // null boxed values unwrap to the null runtime tag

    // -- keep following nested mixed payloads until we reach a concrete tag --
    emitter.label("__rt_mixed_unbox_loop");
    emitter.instruction("mov x10, x0");                                         // preserve the current mixed cell pointer while inspecting its tag
    emitter.instruction("ldr x9, [x0]");                                        // x9 = boxed payload tag
    emitter.instruction("cmp x9, #7");                                          // does this mixed box wrap another mixed value?
    emitter.instruction("b.ne __rt_mixed_unbox_done");                          // stop once the payload tag is concrete
    emitter.instruction("ldr x0, [x0, #8]");                                    // follow the nested mixed pointer stored in value_lo
    emitter.instruction("cbz x0, __rt_mixed_unbox_null");                       // null nested boxes unwrap to the null runtime tag
    emitter.instruction("b __rt_mixed_unbox_loop");                             // continue peeling nested mixed wrappers

    emitter.label("__rt_mixed_unbox_done");
    emitter.instruction("mov x0, x9");                                          // return the concrete runtime tag in x0
    emitter.instruction("ldr x1, [x10, #8]");                                   // return the concrete payload low word in x1
    emitter.instruction("ldr x2, [x10, #16]");                                  // return the concrete payload high word in x2
    emitter.instruction("ret");                                                 // return the unboxed payload triple

    emitter.label("__rt_mixed_unbox_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null has no low payload word
    emitter.instruction("mov x2, #0");                                          // null has no high payload word
    emitter.instruction("ret");                                                 // return the normalized null payload triple
}

fn emit_mixed_unbox_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_unbox ---");
    emitter.label_global("__rt_mixed_unbox");

    emitter.instruction("test rax, rax");                                       // null mixed pointers behave like null payloads
    emitter.instruction("je __rt_mixed_unbox_null");                            // null boxed values unwrap to the null runtime tag

    emitter.label("__rt_mixed_unbox_loop");
    emitter.instruction("mov r10, rax");                                        // preserve the current mixed cell pointer while inspecting its tag
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // r11 = boxed payload tag
    emitter.instruction("cmp r11, 7");                                          // does this mixed box wrap another mixed value?
    emitter.instruction("jne __rt_mixed_unbox_done");                           // stop once the payload tag is concrete
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // follow the nested mixed pointer stored in value_lo
    emitter.instruction("test rax, rax");                                       // null nested boxes unwrap to the null runtime tag
    emitter.instruction("je __rt_mixed_unbox_null");                            // normalize null nested boxes to the null runtime tag
    emitter.instruction("jmp __rt_mixed_unbox_loop");                           // continue peeling nested mixed wrappers

    emitter.label("__rt_mixed_unbox_done");
    emitter.instruction("mov rax, r11");                                        // return the concrete runtime tag in rax
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // return the concrete payload low word in rdi
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // return the concrete payload high word in rdx
    emitter.instruction("ret");                                                 // return the unboxed payload triple

    emitter.label("__rt_mixed_unbox_null");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor rdi, rdi");                                        // null has no low payload word
    emitter.instruction("xor rdx, rdx");                                        // null has no high payload word
    emitter.instruction("ret");                                                 // return the normalized null payload triple
}
