//! Purpose:
//! Emits the `__rt_fopen`, `__rt_cstr` runtime helper assembly for fopen.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// The fixed warning text emitted when `fopen()` fails to open a file.
const FOPEN_FAILED_WARNING: &str = "Warning: fopen(): Failed to open stream\n";

/// fopen: open a file and return its file descriptor.
/// Input:  x1/x2=filename string, x3/x4=mode string
/// Output: x0=file descriptor (or negative on error)
pub fn emit_fopen(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fopen_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fopen ---");
    emitter.label_global("__rt_fopen");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save mode string for later parsing --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save mode ptr and len on stack

    // -- null-terminate the filename via __rt_cstr --
    emitter.instruction("bl __rt_cstr");                                        // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- parse mode string to derive open() flags --
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload mode ptr and len
    emitter.instruction("cmp x4, #0");                                          // reject an empty fopen() mode before reading the first byte
    emitter.instruction("b.eq __rt_fopen_fail");                                // empty modes fail like PHP and return false
    emitter.instruction("ldrb w9, [x3]");                                       // load first character of mode string

    // -- check for 'r' mode --
    emitter.instruction("cmp w9, #0x72");                                       // compare with 'r'
    emitter.instruction("b.ne __rt_fopen_check_w");                             // if not 'r', check for 'w'
    emitter.instruction("mov x1, #0");                                          // O_RDONLY = 0
    emitter.instruction("b __rt_fopen_check_plus");                             // proceed to check for '+' modifier

    // -- check for 'w' mode --
    emitter.label("__rt_fopen_check_w");
    emitter.instruction("cmp w9, #0x77");                                       // compare with 'w'
    emitter.instruction("b.ne __rt_fopen_check_a");                             // if not 'w', check for 'a'
    emitter.instruction(&format!("mov x1, #0x{:X}", emitter.platform.o_wronly_creat_trunc())); // O_WRONLY|O_CREAT|O_TRUNC
    emitter.instruction("b __rt_fopen_check_plus");                             // proceed to check for '+' modifier

    // -- check for 'a' mode (append) --
    emitter.label("__rt_fopen_check_a");
    emitter.instruction("cmp w9, #0x61");                                       // compare with 'a'
    emitter.instruction("b.ne __rt_fopen_fail");                                // reject unsupported fopen() mode letters
    emitter.instruction(&format!("mov x1, #0x{:X}", emitter.platform.o_wronly_creat_append())); // O_WRONLY|O_CREAT|O_APPEND
    // fall through to check_plus

    // -- check if second char is '+' to enable read+write --
    emitter.label("__rt_fopen_check_plus");
    emitter.instruction("cmp x4, #1");                                          // check if mode string has more than 1 char
    emitter.instruction("b.le __rt_fopen_do_open");                             // if only 1 char, skip '+' check
    emitter.instruction("ldrb w10, [x3, #1]");                                  // load second character of mode string
    emitter.instruction("cmp w10, #0x2B");                                      // compare with '+'
    emitter.instruction("b.ne __rt_fopen_do_open");                             // if not '+', keep original flags
    // -- upgrade to O_RDWR: clear O_RDONLY/O_WRONLY bits, set O_RDWR --
    emitter.instruction("and x1, x1, #0xFFFFFFFFFFFFFFFC");                     // clear lowest 2 bits (O_RDONLY/O_WRONLY)
    emitter.instruction("orr x1, x1, #0x2");                                    // set O_RDWR flag

    // -- perform the open syscall --
    emitter.label("__rt_fopen_do_open");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction("mov x2, #0x1A4");                                      // file mode 0644 (octal)
    emitter.syscall(5);

    // -- check if open failed --
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: check if return value is negative
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fopen_opened")); // branch if syscall succeeded
    emitter.label("__rt_fopen_fail");
    emit_fopen_failed_warning(emitter);
    emitter.instruction("mov x0, #-1");                                         // return -1 to indicate failure
    emitter.instruction("b __rt_fopen_return");                                 // skip eof-flag reset on failed opens

    // -- restore frame and return fd in x0 --
    emitter.label("__rt_fopen_opened");
    abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("strb wzr, [x9, x0]");                                  // clear stale EOF state for the newly opened descriptor
    emitter.label("__rt_fopen_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller with fd in x0
}

/// Emits the x86_64 Linux variant of `__rt_fopen`.
/// Accepts filename (rdi=ptr, rsi=len) and mode (rax=ptr, rdx=len) as ElephC string registers,
/// converts both to null-terminated C strings via `__rt_cstr`/`__rt_cstr2`, parses the mode character
/// to derive Linux `open()` flags, calls libc `open()`, and returns the file descriptor in rax
/// (or -1 on failure). Clears the EOF flag entry for the newly opened descriptor before returning.
///
/// # Call-ABI
/// - Filename: rdi=pointer, rsi=length
/// - Mode: rax=pointer, rdx=length (ElephC string registers, caller-preserved across `__rt_cstr`)
/// - Returns: rax=fd or -1
fn emit_fopen_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fopen ---");
    emitter.label_global("__rt_fopen");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fopen() uses stack locals for path and mode parsing
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary pathname and mode spill slots
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack space for the saved mode pair, cstring path, and cstring mode pointers

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the elephc mode pointer while the filename string is converted to a C string
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the elephc mode length while the filename string is converted to a C string
    emitter.instruction("call __rt_cstr");                                      // convert the elephc filename in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the C pathname pointer for the later libc open() call

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the elephc mode pointer into the standard x86_64 string-result pointer register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the elephc mode length into the standard x86_64 string-result length register
    emitter.instruction("call __rt_cstr2");                                     // convert the elephc mode string into the secondary null-terminated C string buffer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the C mode pointer for the mode-flag parser below

    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // load the C mode string pointer so fopen() can inspect the first mode byte
    emitter.instruction("movzx r11d, BYTE PTR [r10]");                          // load the first fopen() mode character to choose the base Linux open() flags
    emitter.instruction("cmp r11b, 0x72");                                      // does the mode string start with 'r' for read-only access?
    emitter.instruction("jne __rt_fopen_check_w_x86");                          // if not, fall through to the write-mode checks
    emitter.instruction("xor esi, esi");                                        // O_RDONLY = 0 for the Linux read-only fopen() path
    emitter.instruction("jmp __rt_fopen_check_plus_x86");                       // continue with the optional '+' upgrade after selecting the base flags

    emitter.label("__rt_fopen_check_w_x86");
    emitter.instruction("cmp r11b, 0x77");                                      // does the mode string start with 'w' for truncate-on-open writes?
    emitter.instruction("jne __rt_fopen_check_a_x86");                          // if not, fall through to the append-mode check
    emitter.instruction(&format!("mov esi, 0x{:X}", emitter.platform.o_wronly_creat_trunc())); // select O_WRONLY|O_CREAT|O_TRUNC for the Linux write-mode fopen() path
    emitter.instruction("jmp __rt_fopen_check_plus_x86");                       // continue with the optional '+' upgrade after selecting the base flags

    emitter.label("__rt_fopen_check_a_x86");
    emitter.instruction("cmp r11b, 0x61");                                      // does the mode string start with 'a' for append writes?
    emitter.instruction("jne __rt_fopen_fail_x86");                             // reject unsupported fopen() mode letters
    emitter.instruction(&format!("mov esi, 0x{:X}", emitter.platform.o_wronly_creat_append())); // select O_WRONLY|O_CREAT|O_APPEND for the Linux append-mode fopen() path

    emitter.label("__rt_fopen_check_plus_x86");
    emitter.instruction("cmp BYTE PTR [r10 + 1], 0x2B");                        // does the mode string request the read-write '+' fopen() upgrade?
    emitter.instruction("jne __rt_fopen_do_open_x86");                          // keep the base flags when the mode string does not contain '+'
    emitter.instruction("and esi, 0xFFFFFFFC");                                 // clear the low access-mode bits before upgrading the Linux fopen() flags to O_RDWR
    emitter.instruction("or esi, 0x2");                                         // set O_RDWR so 'r+'/'w+'/'a+' open the file for both reading and writing

    emitter.label("__rt_fopen_do_open_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the converted C pathname as the first libc open() argument
    emitter.instruction("mov edx, 0x1A4");                                      // pass mode 0644 for create-capable fopen() modes
    emitter.instruction("call open");                                           // open the requested file through libc open() using the parsed fopen() flags
    emitter.instruction("test rax, rax");                                       // did libc open() return a negative failure descriptor?
    emitter.instruction("jns __rt_fopen_opened_x86");                           // skip the warning when fopen() succeeded
    emitter.label("__rt_fopen_fail_x86");
    emit_fopen_failed_warning(emitter);
    emitter.instruction("mov rax, -1");                                         // normalize all open failures to the PHP false sentinel path
    emitter.instruction("jmp __rt_fopen_return_x86");                           // skip eof-flag reset on failed opens
    emitter.label("__rt_fopen_opened_x86");
    emitter.instruction("lea r10, [rip + _eof_flags]");                         // materialize the eof-flag table for the newly opened descriptor
    emitter.instruction("mov BYTE PTR [r10 + rax], 0");                         // clear stale EOF state before returning the descriptor
    emitter.label("__rt_fopen_return_x86");

    emitter.instruction("add rsp, 32");                                         // release the temporary pathname and mode spill slots before returning the file descriptor
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the x86_64 fopen() helper completes
    emitter.instruction("ret");                                                 // return the libc open() file descriptor or negative error value in rax
}

/// Emits the fixed "fopen() failed" warning via the diagnostic runtime helper.
/// AArch64: passes pointer in x1, length in x2, calls `__rt_diag_warning`.
/// x86_64: passes pointer in rdi, length in esi, calls `__rt_diag_warning`.
/// Uses `FOPEN_FAILED_WARNING` as the diagnostic text.
fn emit_fopen_failed_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", "_diag_fopen_failed_msg");  // pass the fopen() warning text pointer to the diagnostic helper
            emitter.instruction(&format!("mov x2, #{}", FOPEN_FAILED_WARNING.len())); // pass the fopen() warning byte length to the diagnostic helper
            emitter.instruction("bl __rt_diag_warning");                        // emit or suppress the fopen() failure warning
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", "_diag_fopen_failed_msg"); // pass the fopen() warning text pointer to the diagnostic helper
            emitter.instruction(&format!("mov esi, {}", FOPEN_FAILED_WARNING.len())); // pass the fopen() warning byte length to the diagnostic helper
            emitter.instruction("call __rt_diag_warning");                      // emit or suppress the fopen() failure warning
        }
    }
}
