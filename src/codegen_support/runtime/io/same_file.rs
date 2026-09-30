//! Purpose:
//! Emits `__rt_same_file`, which answers whether two paths name the same file (the same device
//! and inode), the test `copy()` makes before it opens anything.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform` via `io::emit_same_file`.
//!
//! Key details:
//! - php-src's `php_copy_file_ctx()` stats both ends and FAILS, silently, when they share
//!   `st_dev` and `st_ino`: copying a file onto itself, through any spelling or hard link, would
//!   otherwise open the destination for writing — truncating the very bytes it is about to read.
//! - Either `stat()` failing answers "not the same", which leaves the open that follows to report
//!   a missing source exactly as php does.
//! - `__rt_path_cstr` writes one scratch buffer, so the source is statted before the destination
//!   path is converted.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_same_file`.
///
/// Input:  AArch64 x1/x2 = source path, x3/x4 = destination path; x86_64 rax/rdx = source,
///         rdi/rsi = destination
/// Output: x0/rax = 1 when both paths stat and share device and inode, else 0
pub fn emit_same_file(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

fn emit_aarch64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size(emitter.target.arch);
    let dest_off = (2 * stat_buf + 15) & !15;
    let frame_size = dest_off + 32;
    let save_offset = frame_size - 16;
    let ino_off = plat.stat_ino_offset();
    let dev_off = plat.stat_dev_offset();

    emitter.blank();
    emitter.comment("--- runtime: same_file ---");
    emitter.label_global("__rt_same_file");
    emitter.instruction(&format!("sub sp, sp, #{frame_size}"));                  // two stat buffers, the destination path, the frame record
    emitter.instruction(&format!("stp x29, x30, [sp, #{save_offset}]"));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{save_offset}"));                // establish the frame pointer
    emitter.instruction(&format!("stp x3, x4, [sp, #{dest_off}]"));              // keep the destination until the source is statted
    emitter.instruction("bl __rt_path_cstr");                                    // x0 = the source as a C string
    emitter.instruction("add x1, sp, #0");                                       // the source's stat buffer
    emitter.syscall(338);
    emitter.instruction("cbnz x0, __rt_same_file_no");                           // an unstatable source is not the destination
    emitter.instruction(&format!("ldp x1, x2, [sp, #{dest_off}]"));              // the destination path
    emitter.instruction("bl __rt_path_cstr");                                    // x0 = the destination as a C string
    emitter.instruction(&format!("add x1, sp, #{stat_buf}"));                    // the destination's stat buffer
    emitter.syscall(338);
    emitter.instruction("cbnz x0, __rt_same_file_no");                           // a destination that does not exist yet is another file
    emitter.instruction(&format!("ldr x9, [sp, #{ino_off}]"));                   // the source inode
    emitter.instruction(&format!("ldr x10, [sp, #{}]", stat_buf + ino_off));      // the destination inode
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.ne __rt_same_file_no");                               // different inodes are different files
    emitter.instruction(&plat.stat_dev_load_instr("x9", "sp", dev_off));          // the source device
    emitter.instruction(&plat.stat_dev_load_instr("x10", "sp", stat_buf + dev_off)); // the destination device
    emitter.instruction("cmp x9, x10");
    emitter.instruction("cset x0, eq");                                          // one inode on one device is one file
    emitter.instruction("b __rt_same_file_done");
    emitter.label("__rt_same_file_no");
    emitter.instruction("mov x0, #0");                                           // not provably the same file
    emitter.label("__rt_same_file_done");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{save_offset}]"));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{frame_size}"));                  // release the frame
    emitter.instruction("ret");
}

fn emit_x86_64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size(emitter.target.arch);
    let dest_off = (2 * stat_buf + 15) & !15;
    let frame_size = dest_off + 16;
    let ino_off = plat.stat_ino_offset();
    let dev_off = plat.stat_dev_offset();

    emitter.blank();
    emitter.comment("--- runtime: same_file ---");
    emitter.label_global("__rt_same_file");
    emitter.instruction("push rbp");                                             // keep the caller frame pointer
    emitter.instruction("mov rbp, rsp");
    emitter.instruction(&format!("sub rsp, {frame_size}"));                      // two stat buffers and the destination path, 16-byte aligned
    emitter.instruction(&format!("mov QWORD PTR [rsp + {dest_off}], rdi"));      // keep the destination until the source is statted
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rsi", dest_off + 8));
    emitter.instruction("call __rt_path_cstr");                                  // rax = the source as a C string
    emitter.instruction("mov rdi, rax");
    emitter.instruction("lea rsi, [rsp]");                                       // the source's stat buffer
    emitter.instruction("call stat");
    emitter.instruction("test eax, eax");
    emitter.instruction("jne __rt_same_file_no");                                // an unstatable source is not the destination
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {dest_off}]"));      // the destination path
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", dest_off + 8));
    emitter.instruction("call __rt_path_cstr");                                  // rax = the destination as a C string
    emitter.instruction("mov rdi, rax");
    emitter.instruction(&format!("lea rsi, [rsp + {stat_buf}]"));                // the destination's stat buffer
    emitter.instruction("call stat");
    emitter.instruction("test eax, eax");
    emitter.instruction("jne __rt_same_file_no");                                // a destination that does not exist yet is another file
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {ino_off}]"));        // the source inode
    emitter.instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", stat_buf + ino_off)); // against the destination inode
    emitter.instruction("jne __rt_same_file_no");                                // different inodes are different files
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {dev_off}]"));        // the source device
    emitter.instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", stat_buf + dev_off)); // against the destination device
    emitter.instruction("sete al");                                              // one inode on one device is one file
    emitter.instruction("movzx eax, al");
    emitter.instruction("jmp __rt_same_file_done");
    emitter.label("__rt_same_file_no");
    emitter.instruction("xor eax, eax");                                         // not provably the same file
    emitter.label("__rt_same_file_done");
    emitter.instruction("mov rsp, rbp");                                         // release the frame
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
