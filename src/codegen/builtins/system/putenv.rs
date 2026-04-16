use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("putenv()");
    // -- evaluate the KEY=VALUE string --
    emit_expr(&args[0], emitter, ctx, data);
    let copy_loop = ctx.next_label("putenv_copy");
    let copy_done = ctx.next_label("putenv_copy_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- copy string to heap so it persists (putenv keeps the pointer) --
            emitter.instruction("add x0, x2, #1");                              // heap size = string len + 1 (null terminator)
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save string ptr/len
            emitter.instruction("bl __rt_heap_alloc");                          // allocate persistent buffer → x0=heap_ptr
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore string ptr/len
            emitter.instruction("mov x3, x0");                                  // save heap ptr in x3
            // -- copy bytes to heap --
            emitter.instruction("mov x4, #0");                                  // copy index = 0
            emitter.label(&copy_loop);
            emitter.instruction("cmp x4, x2");                                  // compare index with length
            emitter.instruction(&format!("b.ge {}", copy_done));                // done if index >= length
            emitter.instruction("ldrb w5, [x1, x4]");                           // load byte from source
            emitter.instruction("strb w5, [x3, x4]");                           // store byte to heap
            emitter.instruction("add x4, x4, #1");                              // increment index
            emitter.instruction(&format!("b {}", copy_loop));                   // continue copying
            emitter.label(&copy_done);
            emitter.instruction("strb wzr, [x3, x4]");                          // null-terminate on heap
            // -- call putenv with heap-allocated string --
            emitter.instruction("mov x0, x3");                                  // pass heap cstr to putenv
            emitter.bl_c("putenv");                                             // set env var, returns 0 on success
            // -- convert return value to bool (0=success → true=1) --
            emitter.instruction("cmp x0, #0");                                  // check if putenv returned 0 (success)
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if success, 0 if failure
        }
        Arch::X86_64 => {
            // -- copy string to heap so it persists (putenv keeps the pointer) --
            emitter.instruction("sub rsp, 16");                                 // reserve an aligned spill area for the source ptr/len across heap allocation
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // save the source string pointer before requesting a persistent buffer
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the source string length before requesting a persistent buffer
            emitter.instruction("mov rax, rdx");                                // seed the heap allocation size from the source string length
            emitter.instruction("add rax, 1");                                  // heap size = string len + 1 (null terminator)
            emitter.instruction("call __rt_heap_alloc");                        // allocate persistent buffer → rax=heap_ptr
            emitter.instruction("mov rcx, QWORD PTR [rsp]");                    // restore the source string pointer after heap allocation
            emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                 // restore the source string length after heap allocation
            emitter.instruction("add rsp, 16");                                 // release the aligned source spill area after heap allocation
            emitter.instruction("mov r9, rax");                                 // save the persistent destination buffer pointer for the copy loop and putenv call
            // -- copy bytes to heap --
            emitter.instruction("mov r10, 0");                                  // copy index = 0
            emitter.label(&copy_loop);
            emitter.instruction("cmp r10, r8");                                 // compare the current copy index against the source string length
            emitter.instruction(&format!("jae {}", copy_done));                 // stop once every source byte has been copied into the persistent buffer
            emitter.instruction("mov r11b, BYTE PTR [rcx + r10]");              // load one byte from the source string payload
            emitter.instruction("mov BYTE PTR [r9 + r10], r11b");               // store the copied byte into the persistent environment buffer
            emitter.instruction("add r10, 1");                                  // advance the copy index to the next byte
            emitter.instruction(&format!("jmp {}", copy_loop));                 // continue copying until the full KEY=VALUE string has been persisted
            emitter.label(&copy_done);
            emitter.instruction("mov BYTE PTR [r9 + r10], 0");                  // append the trailing C null terminator after the copied KEY=VALUE bytes
            // -- call putenv with heap-allocated string --
            emitter.instruction("mov rdi, r9");                                 // pass the persistent KEY=VALUE buffer in the SysV first-argument register
            emitter.bl_c("putenv");                                             // set env var, returns 0 on success
            // -- convert return value to bool (0=success → true=1) --
            emitter.instruction("cmp rax, 0");                                  // check if putenv returned 0 (success) on Linux x86_64
            emitter.instruction("sete al");                                     // set AL when putenv succeeded so the result becomes a PHP true value
            emitter.instruction("movzx rax, al");                               // zero-extend the boolean result back into the native integer result register
        }
    }
    Some(PhpType::Bool)
}
