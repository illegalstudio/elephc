//! Purpose:
//! Emits `__rt_user_wrapper_construct`, which runs a wrapper class's `__construct()` on a
//! freshly allocated instance.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - Every run-time wrapper instantiation: `fopen`, `url_stat`, the path operations, and the
//!   directory family — each right after `__rt_new_by_name`.
//!
//! Key details:
//! - php CONSTRUCTS a wrapper before it asks it anything. MEASURED on `php -n` 8.5.6, a class
//!   that announces itself prints `construct: context=NULL` and then `open: tag=built`: the
//!   constructor runs FIRST, before the context is assigned and before `stream_open()`. elephc
//!   allocated the object and seeded its property DEFAULTS but never called the constructor, so
//!   a wrapper that prepares its state there started empty.
//! - The pointer comes from the wrapper vtable's third trailing quad, and is 0 for a class that
//!   declares no constructor — which is most of them, so the common path is a load and a branch.
//! - PASS-THROUGH: the object arrives in the register `__rt_new_by_name` returned it in and
//!   leaves in the same one, so a call site adds one instruction and nothing else moves.

use crate::codegen_support::runtime::data::USER_WRAPPER_VTABLE_CTOR_OFFSET;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_user_wrapper_construct(obj) -> obj`.
///
/// AArch64 takes and returns `x0`; x86_64 takes and returns `rax`, matching
/// `__rt_new_by_name`'s own local convention so the call site is a single added line.
///
/// A null object passes straight through: the instantiation failed and the caller's own miss
/// path is what handles that.
pub fn emit_user_wrapper_construct(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: run a wrapper class's __construct ---");
    emitter.raw("    .p2align 2");                                              // 4-byte alignment for the helper entry
    emitter.label_global("__rt_user_wrapper_construct");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // frame: [0] = the object
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #16");                            // establish the helper frame pointer
            emitter.instruction("str x0, [sp, #0]");                            // the instance, which is also the answer
            emitter.instruction("cbz x0, __rt_uwctor_done");                    // nothing was allocated

            emitter.instruction("ldr x9, [x0]");                                // class_id at the head of every wrapper object
            abi::emit_symbol_address(emitter, "x10", "_user_wrapper_vtable_ptrs");
            emitter.instruction("ldr x10, [x10, x9, lsl #3]");                  // this class's wrapper vtable
            emitter.instruction(&format!(
                "ldr x11, [x10, #{USER_WRAPPER_VTABLE_CTOR_OFFSET}]"
            ));                                                                 // the constructor, or 0
            emitter.instruction("cbz x11, __rt_uwctor_done");                   // the class declares none

            emitter.instruction("ldr x0, [sp, #0]");                            // $this
            emitter.instruction("blr x11");                                     // php runs it before anything else
            emitter.instruction("ldr x0, [sp, #0]");                            // the object, whatever the constructor returned

            emitter.label("__rt_uwctor_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the helper frame
            emitter.instruction("ret");                                         // the instance, constructed
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame pointer
            emitter.instruction("sub rsp, 16");                                 // [rbp - 8] = the object
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // the instance, which is also the answer
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_uwctor_done_x86");                     // nothing was allocated

            emitter.instruction("mov r10, QWORD PTR [rax]");                    // class_id at the head of every wrapper object
            abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");
            emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");          // this class's wrapper vtable
            emitter.instruction(&format!(
                "mov r11, QWORD PTR [r11 + {USER_WRAPPER_VTABLE_CTOR_OFFSET}]"
            ));                                                                 // the constructor, or 0
            emitter.instruction("test r11, r11");
            emitter.instruction("jz __rt_uwctor_done_x86");                     // the class declares none

            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // $this
            emitter.instruction("call r11");                                    // php runs it before anything else
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // the object, whatever the constructor returned

            emitter.label("__rt_uwctor_done_x86");
            emitter.instruction("leave");                                       // restore rbp and rsp
            emitter.instruction("ret");                                         // the instance, constructed
        }
    }
}
