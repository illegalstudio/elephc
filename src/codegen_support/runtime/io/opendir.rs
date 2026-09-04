//! Purpose:
//! Emits the `__rt_opendir` runtime helper, which opens a directory stream
//! through libc `opendir` and returns its descriptor plus owning `DIR*`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - libc `opendir` yields a `DIR*`; `dirfd` recovers the raw descriptor that
//!   becomes the PHP directory stream resource value.
//! - The caller adopts the returned `DIR*` into `StreamState.backend_aux`.
//! - Native, glob, and userspace directory backends return distinct kinds.

use crate::codegen_support::runtime::data::{
    OPENDIR_WARNING_HEAD, SCANDIR_OPEN_WARNING_MIDDLE,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// opendir: open a directory stream and return its descriptor.
/// Input:  AArch64 x1/x2 = directory path string
///         x86_64  rax/rdx = directory path string
/// Output: descriptor, backend aux, and backend kind; or -1 on failure.
pub fn emit_opendir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_opendir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: opendir ---");
    emitter.label_global("__rt_opendir");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(-1),
        super::fopen::DisabledWrapperNotice::FailedToOpen {
            name_symbol: "_uww_name_opendir",
            name_len: 7,
            directory: true,
        },
    );

    // -- userspace stream-wrapper probe (registered scheme://) --
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // probe frame, save fp/lr
    emitter.instruction("mov x29, sp");                                         // establish the probe frame pointer
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save path ptr/len for the fall-through
    emitter.instruction("bl __rt_user_wrapper_opendir");                        // path in x1/x2 → fd | -1 | -2
    emitter.instruction("cmn x0, #2");                                          // is the result the "not a wrapper" sentinel (-2)?
    emitter.instruction("b.eq __rt_opendir_uw_fall");                           // no registered scheme matched → fall through to libc
    emitter.instruction("mov x1, #0");                                          // userspace directory ownership stays in its wrapper registry
    emitter.instruction("mov x2, #6");                                          // backend kind 6 dispatches userspace directory callbacks
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the probe frame
    emitter.instruction("ret");                                                 // return the synthetic fd or false sentinel
    emitter.label("__rt_opendir_uw_fall");
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // restore the path ptr/len
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the probe frame

    // -- glob:// scheme probe before the libc opendir path --
    emitter.instruction("cmp x2, #7");                                          // "glob://" needs at least seven bytes
    emitter.instruction("b.lt __rt_opendir_no_glob");                           // too short → fall through to libc opendir
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load scheme byte 0
    emitter.instruction("cmp w9, #103");                                        // 'g'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load scheme byte 1
    emitter.instruction("cmp w9, #108");                                        // 'l'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #2]");                                   // load scheme byte 2
    emitter.instruction("cmp w9, #111");                                        // 'o'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #3]");                                   // load scheme byte 3
    emitter.instruction("cmp w9, #98");                                         // 'b'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #4]");                                   // load scheme byte 4
    emitter.instruction("cmp w9, #58");                                         // ':'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #5]");                                   // load scheme byte 5
    emitter.instruction("cmp w9, #47");                                         // '/'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #6]");                                   // load scheme byte 6
    emitter.instruction("cmp w9, #47");                                         // '/'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("b __rt_opendir_glob");                                 // glob:// path: tail-call into the synthetic helper
    emitter.label("__rt_opendir_no_glob");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // frame for the saved registers and the DIR* slot
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- null-terminate the directory path --
    emitter.instruction("bl __rt_path_cstr");                                   // convert the path to a C string, x0 = C string
    emitter.instruction("str x0, [sp, #16]");                                   // the C path libc opens
    emitter.instruction("bl __rt_path_diag_name");                              // php names the URL the program wrote
    emitter.instruction("str x0, [sp, #24]");                                   // the path the warning below names
    emitter.instruction("ldr x0, [sp, #16]");                                   // libc gets the path itself

    // -- open the directory stream --
    emitter.bl_c("opendir");
    emitter.instruction("cbz x0, __rt_opendir_fail");                           // a NULL DIR* means opendir failed
    emitter.instruction("str x0, [sp, #16]");                                   // save the DIR* across the dirfd call

    // -- recover the underlying descriptor with dirfd --
    emitter.bl_c("dirfd");

    emitter.instruction("ldr x1, [sp, #16]");                                   // return the owning DIR* as backend auxiliary state
    emitter.instruction("mov x2, #4");                                          // backend kind 4 identifies native directory iteration
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the directory descriptor

    emitter.label("__rt_opendir_fail");
    // -- php says WHY, in `scandir()`'s wording, and elephc said nothing --
    // MEASURED: `Warning: opendir(nope): Failed to open directory: No such file or directory`.
    super::path_op_warning::emit_libc_call_aarch64(
        emitter,
        "_warn_opendir_head",
        OPENDIR_WARNING_HEAD.len(),
        Some("[sp, #24]"),
        "_scandir_open_warn_mid",
        SCANDIR_OPEN_WARNING_MIDDLE.len(),
    );
    emitter.instruction("mov x0, #-1");                                         // -1 reports an opendir failure
    emitter.instruction("mov x1, #0");                                          // failed opens have no backend auxiliary owner
    emitter.instruction("mov x2, #4");                                          // retain a deterministic native backend discriminator
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for opendir.
fn emit_opendir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: opendir ---");
    emitter.label_global("__rt_opendir");
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(-1),
        super::fopen::DisabledWrapperNotice::FailedToOpen {
            name_symbol: "_uww_name_opendir",
            name_len: 7,
            directory: true,
        },
    );

    // -- userspace stream-wrapper probe (registered scheme://) --
    emitter.instruction("push rbp");                                            // probe frame: preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the probe frame pointer
    emitter.instruction("sub rsp, 16");                                         // spill slot for the saved path
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save path ptr for the fall-through
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save path len for the fall-through
    emitter.instruction("call __rt_user_wrapper_opendir");                      // path in rax/rdx → fd | -1 | -2
    emitter.instruction("cmp rax, -2");                                         // is the result the "not a wrapper" sentinel (-2)?
    emitter.instruction("je __rt_opendir_uw_fall_x86");                         // no registered scheme matched → fall through to libc
    emitter.instruction("xor edx, edx");                                        // userspace directory ownership stays in its wrapper registry
    emitter.instruction("mov ecx, 6");                                          // backend kind 6 dispatches userspace directory callbacks
    emitter.instruction("add rsp, 16");                                         // release the probe frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the synthetic fd or false sentinel
    emitter.label("__rt_opendir_uw_fall_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the path ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the path len
    emitter.instruction("add rsp, 16");                                         // release the probe frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer

    // -- glob:// scheme probe before the libc opendir path --
    emitter.instruction("cmp rdx, 7");                                          // "glob://" needs at least seven bytes
    emitter.instruction("jl __rt_opendir_no_glob_x86");                         // too short → fall through to libc opendir
    emitter.instruction("movzx ecx, BYTE PTR [rax + 0]");                       // load scheme byte 0
    emitter.instruction("cmp ecx, 103");                                        // 'g'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 1]");                       // load scheme byte 1
    emitter.instruction("cmp ecx, 108");                                        // 'l'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 2]");                       // load scheme byte 2
    emitter.instruction("cmp ecx, 111");                                        // 'o'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 3]");                       // load scheme byte 3
    emitter.instruction("cmp ecx, 98");                                         // 'b'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 4]");                       // load scheme byte 4
    emitter.instruction("cmp ecx, 58");                                         // ':'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 5]");                       // load scheme byte 5
    emitter.instruction("cmp ecx, 47");                                         // '/'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 6]");                       // load scheme byte 6
    emitter.instruction("cmp ecx, 47");                                         // '/'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("jmp __rt_opendir_glob");                               // glob:// path: tail-call into the synthetic helper
    emitter.label("__rt_opendir_no_glob_x86");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // the DIR* handle, and the name the warning prints

    // -- null-terminate the directory path --
    emitter.instruction("call __rt_path_cstr");                                 // convert the path to a C string, rax = C string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the C path libc opens
    // See the AArch64 counterpart: php names the URL the program wrote.
    emitter.instruction("call __rt_path_diag_name");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the name the warning below prints
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // libc gets the path itself

    // -- open the directory stream --
    emitter.instruction("mov rdi, rax");                                        // C-string path argument for opendir
    emitter.bl_c("opendir");
    emitter.instruction("test rax, rax");                                       // a NULL DIR* means opendir failed
    emitter.instruction("jz __rt_opendir_fail_x86");                            // bail out on an opendir failure
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the DIR* across the dirfd call

    // -- recover the underlying descriptor with dirfd --
    emitter.instruction("mov rdi, rax");                                        // DIR* argument for dirfd
    emitter.bl_c("dirfd");

    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // return the owning DIR* as backend auxiliary state
    emitter.instruction("mov ecx, 4");                                          // backend kind 4 identifies native directory iteration
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the directory descriptor

    emitter.label("__rt_opendir_fail_x86");
    // See the AArch64 counterpart: php says WHY, in `scandir()`'s wording.
    super::path_op_warning::emit_libc_call_x86_64(
        emitter,
        "_warn_opendir_head",
        OPENDIR_WARNING_HEAD.len(),
        Some("[rbp - 16]"),
        "_scandir_open_warn_mid",
        SCANDIR_OPEN_WARNING_MIDDLE.len(),
    );
    emitter.instruction("mov rax, -1");                                         // -1 reports an opendir failure
    emitter.instruction("xor edx, edx");                                        // failed opens have no backend auxiliary owner
    emitter.instruction("mov ecx, 4");                                          // retain a deterministic native backend discriminator
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
