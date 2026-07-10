//! Purpose:
//! Emits the `__rt_disk_space` runtime helper assembly for the disk_free_space
//! and disk_total_space builtins.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Calls `statfs` on the null-terminated path and multiplies the fundamental
//!   block size by the available or total block count; returns 0.0 on failure.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// disk_space: report available or total bytes of a filesystem.
/// Input:  x0 = mode (0 = available bytes, 1 = total bytes), x1/x2 = path
/// Output: d0 = byte count as a double (0.0 when `statfs` fails)
pub fn emit_disk_space(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_disk_space_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    // Frame: [0..16) saved x29/x30, [16..24) mode, [24..) the statfs buffer.
    let buf_base = 24usize;
    let bsize_abs = buf_base + plat.statfs_bsize_offset();
    let blocks_abs = buf_base + plat.statfs_blocks_offset();
    let bavail_abs = buf_base + plat.statfs_bavail_offset();
    let frame_size = (buf_base + plat.statfs_buf_size() + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: disk_space ---");
    emitter.label_global("__rt_disk_space");

    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // frame for saved regs, mode, statfs buffer
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the requested mode
    emitter.instruction("bl __rt_cstr");                                        // null-terminate the path; x0 = C string
    emitter.instruction(&format!("add x1, sp, #{}", buf_base));                 // pointer to the statfs buffer
    emitter.syscall(345);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_disk_space_ok")); // continue only when statfs succeeded
    emitter.instruction("fmov d0, xzr");                                        // statfs failed: report 0.0 bytes
    emitter.instruction("b __rt_disk_space_done");                              // skip the computation after a failure

    emitter.label("__rt_disk_space_ok");
    emitter.instruction(&format!("ldr w9, [sp, #{}]", bsize_abs));              // load the fundamental block size
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the requested mode
    emitter.instruction("cbz x10, __rt_disk_space_avail");                      // mode 0 selects available blocks
    emitter.instruction(&format!("ldr x11, [sp, #{}]", blocks_abs));            // mode 1: total block count
    emitter.instruction("b __rt_disk_space_count");                             // proceed to the multiplication
    emitter.label("__rt_disk_space_avail");
    emitter.instruction(&format!("ldr x11, [sp, #{}]", bavail_abs));            // available block count
    emitter.label("__rt_disk_space_count");
    emitter.instruction("mul x9, x9, x11");                                     // bytes = block size * block count
    emitter.instruction("ucvtf d0, x9");                                        // convert the byte count to a double

    emitter.label("__rt_disk_space_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // release the stack frame
    emitter.instruction("ret");                                                 // return the byte count in d0
}

/// Emits the Linux x86_64 stream runtime helper for disk space.
fn emit_disk_space_linux_x86_64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    // Frame: [rbp-8) mode, the statfs buffer occupies [rbp-buf_top .. rbp-8).
    let buf_top = 8 + plat.statfs_buf_size();
    let frame_size = (buf_top + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: disk_space ---");
    emitter.label_global("__rt_disk_space");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction(&format!("sub rsp, {}", frame_size));                   // frame for mode and the statfs buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested mode
    emitter.instruction("mov rax, rsi");                                        // move the path pointer where __rt_cstr expects it
    emitter.instruction("call __rt_cstr");                                      // null-terminate the path; rax = C string
    emitter.instruction("mov rdi, rax");                                        // statfs argument 1: the C-string path
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_top));              // statfs argument 2: the buffer
    emitter.instruction("mov eax, 137");                                        // Linux x86_64 syscall 137 = statfs
    emitter.instruction("syscall");                                             // query the filesystem
    emitter.instruction("test rax, rax");                                       // did statfs fail?
    emitter.instruction("js __rt_disk_space_fail_x86");                         // a negative result means failure

    emitter.instruction(&format!(                                               // load the fundamental block size
        "mov ecx, DWORD PTR [rbp - {}]",
        buf_top - plat.statfs_bsize_offset()
    ));
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the requested mode
    emitter.instruction("test rdx, rdx");                                       // mode 0 selects available blocks
    emitter.instruction("jz __rt_disk_space_avail_x86");                        // branch to the available-blocks path
    emitter.instruction(&format!(                                               // mode 1: total block count
        "mov r8, QWORD PTR [rbp - {}]",
        buf_top - plat.statfs_blocks_offset()
    ));
    emitter.instruction("jmp __rt_disk_space_count_x86");                       // proceed to the multiplication
    emitter.label("__rt_disk_space_avail_x86");
    emitter.instruction(&format!(                                               // available block count
        "mov r8, QWORD PTR [rbp - {}]",
        buf_top - plat.statfs_bavail_offset()
    ));
    emitter.label("__rt_disk_space_count_x86");
    emitter.instruction("mov rax, rcx");                                        // block size into the multiply accumulator
    emitter.instruction("imul rax, r8");                                        // bytes = block size * block count
    emitter.instruction("cvtsi2sd xmm0, rax");                                  // convert the byte count to a double
    emitter.instruction("jmp __rt_disk_space_done_x86");                        // skip the failure path

    emitter.label("__rt_disk_space_fail_x86");
    emitter.instruction("xorps xmm0, xmm0");                                    // statfs failed: report 0.0 bytes

    emitter.label("__rt_disk_space_done_x86");
    emitter.instruction(&format!("add rsp, {}", frame_size));                   // release the stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the byte count in xmm0
}
