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

    // -- recognise user-registered stream wrappers before opening a real file
    //    (Phase 10 dispatch v1: silent-false on match; the wrapper class is
    //    not yet invoked) --
    emitter.instruction("mov x9, #0");                                          // wrapper scheme scan index
    emitter.label("__rt_fopen_uw_scan");
    emitter.instruction("add x10, x9, #3");                                     // need three bytes for the \"://\" marker
    emitter.instruction("cmp x10, x2");                                         // do enough bytes remain in the path?
    emitter.instruction("b.gt __rt_fopen_uw_done");                             // no scheme separator found in the path
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load the candidate \":\" byte
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_fopen_uw_next");                             // not the start of the scheme marker
    emitter.instruction("add x12, x9, #1");                                     // index of the first '/'
    emitter.instruction("ldrb w11, [x1, x12]");                                 // load the candidate first '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_fopen_uw_next");                             // not the scheme marker
    emitter.instruction("add x12, x9, #2");                                     // index of the second '/'
    emitter.instruction("ldrb w11, [x1, x12]");                                 // load the candidate second '/' byte
    emitter.instruction("cmp w11, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_fopen_uw_next");                             // not the scheme marker
    emitter.instruction("b __rt_fopen_uw_check_wrappers");                      // \"://\" found at index x9 — check the registrations
    emitter.label("__rt_fopen_uw_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_fopen_uw_scan");                                // keep scanning for the scheme marker

    emitter.label("__rt_fopen_uw_check_wrappers");
    abi::emit_symbol_address(emitter, "x10", "_user_wrappers");
    emitter.instruction("mov x11, #0");                                         // wrapper slot index
    emitter.label("__rt_fopen_uw_slot");
    emitter.instruction("cmp x11, #64");                                        // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("b.ge __rt_fopen_uw_done");                             // no registered wrapper matched
    emitter.instruction("add x12, x10, x11, lsl #5");                           // slot base = table + index * 32
    emitter.instruction("ldr x13, [x12]");                                      // stored protocol pointer
    emitter.instruction("cbz x13, __rt_fopen_uw_slot_next");                    // empty slot — skip it
    emitter.instruction("ldr x14, [x12, #8]");                                  // stored protocol length
    emitter.instruction("cmp x14, x9");                                         // does the stored length match the scheme length?
    emitter.instruction("b.ne __rt_fopen_uw_slot_next");                        // length mismatch — try the next slot
    emitter.instruction("mov x15, #0");                                         // byte compare index
    emitter.label("__rt_fopen_uw_bytes");
    emitter.instruction("cmp x15, x9");                                         // compared every protocol byte?
    emitter.instruction("b.ge __rt_fopen_uw_match");                            // full match — dispatch into the user wrapper class
    emitter.instruction("ldrb w16, [x13, x15]");                                // stored protocol byte
    emitter.instruction("ldrb w17, [x1, x15]");                                 // path scheme byte
    emitter.instruction("cmp w16, w17");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_fopen_uw_slot_next");                        // protocol byte differs — try the next slot
    emitter.instruction("add x15, x15, #1");                                    // advance the compare index
    emitter.instruction("b __rt_fopen_uw_bytes");                               // continue comparing bytes
    emitter.label("__rt_fopen_uw_slot_next");
    emitter.instruction("add x11, x11, #1");                                    // advance the slot index
    emitter.instruction("b __rt_fopen_uw_slot");                                // continue scanning slots
    emitter.label("__rt_fopen_uw_done");

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

    // -- silent-fail entry for user-registered wrappers (no warning) --
    emitter.label("__rt_fopen_silent_fail");
    emitter.instruction("mov x0, #-1");                                         // return -1 without emitting a warning
    emitter.instruction("b __rt_fopen_return");                                 // share the common return path

    // -- user-wrapper dispatch: matched scheme, x12 = wrapper slot base --
    //    Stack scratch layout below the fopen frame:
    //      [sp, #0]  path ptr
    //      [sp, #8]  path len
    //      [sp, #16] mode ptr
    //      [sp, #24] mode len
    //      [sp, #32] obj ptr (from __rt_new_by_name)
    //      [sp, #40] handle slot index
    //      [sp, #48] stream_open ptr (saved across blr)
    //      [sp, #56] padding
    emitter.label("__rt_fopen_uw_match");
    emitter.instruction("sub sp, sp, #64");                                     // reserve wrapper-dispatch scratch below the fopen frame
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save path ptr/len across __rt_new_by_name and stream_open
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save mode ptr/len across __rt_new_by_name and stream_open
    emitter.instruction("str xzr, [sp, #32]");                                  // pre-initialise the obj slot to 0 so the fail path can tell whether an object was allocated

    // -- instantiate the wrapper class via __rt_new_by_name --
    emitter.instruction("ldr x1, [x12, #16]");                                  // wrapper class name pointer from the registry slot
    emitter.instruction("ldr x2, [x12, #24]");                                  // wrapper class name length from the registry slot
    emitter.instruction("bl __rt_new_by_name");                                 // returns obj pointer in x0, or 0 when the class is unknown
    emitter.instruction("cbz x0, __rt_fopen_uw_fail");                          // unknown class → silent fail with -1
    emitter.instruction("str x0, [sp, #32]");                                   // save the wrapper object pointer for later

    // -- look up stream_open in the per-class user-wrapper vtable (slot 0) --
    emitter.instruction("ldr x9, [x0]");                                        // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // per-class user-wrapper vtable for the resolved class
    emitter.instruction("ldr x11, [x10]");                                      // load the stream_open method pointer from slot 0
    emitter.instruction("cbz x11, __rt_fopen_uw_fail");                         // class did not implement stream_open → silent fail
    emitter.instruction("str x11, [sp, #48]");                                  // save stream_open ptr across the upcoming blr

    // -- allocate the first free slot in _user_wrapper_handles --
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("mov x12, #0");                                         // start scanning from handle slot 0
    emitter.label("__rt_fopen_uw_handle_scan");
    emitter.instruction("cmp x12, #256");                                       // does any free handle slot remain (USER_WRAPPER_HANDLES_CAP)?
    emitter.instruction("b.ge __rt_fopen_uw_fail");                             // table full → silent fail (obj is freed on the shared fail path)
    emitter.instruction("ldr x13, [x10, x12, lsl #3]");                         // load slot — null means free
    emitter.instruction("cbz x13, __rt_fopen_uw_handle_alloc");                 // free slot found
    emitter.instruction("add x12, x12, #1");                                    // advance to the next handle slot
    emitter.instruction("b __rt_fopen_uw_handle_scan");                         // keep scanning
    emitter.label("__rt_fopen_uw_handle_alloc");
    emitter.instruction("str x12, [sp, #40]");                                  // save the allocated handle slot index

    // -- call stream_open(obj, path, mode, options=0) --
    emitter.instruction("ldr x0, [sp, #32]");                                   // $this = wrapper object
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // path ptr/len → string-arg pair 1
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // mode ptr/len → string-arg pair 2
    emitter.instruction("mov x5, #0");                                          // options = 0 (PHP STREAM_USE_PATH/REPORT_ERRORS unused in v1)
    // -- 5th arg `?string &$opened_path` (Tier 2.2): address of a 16-byte
    //    scratch slot, zero'd just before the call. The wrapper may write to
    //    this slot via the PHP-faithful by-reference signature; elephc does
    //    not read the value back. --
    abi::emit_symbol_address(emitter, "x6", "_stream_open_opened_path_scratch");
    emitter.instruction("stp xzr, xzr, [x6]");                                  // zero the opened_path scratch slot before the call
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload stream_open method pointer
    emitter.instruction("blr x11");                                             // invoke stream_open on the wrapper object
    emitter.instruction("cbz x0, __rt_fopen_uw_fail");                          // stream_open returned false → silent fail (obj is freed on the shared fail path)

    // -- success: store obj in the handle slot and return the synthetic fd --
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the handle slot index
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload the wrapper object pointer
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("str x13, [x10, x12, lsl #3]");                         // _user_wrapper_handles[slot] = obj
    emitter.instruction("mov x0, #0x4000");                                     // low 16 bits of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl x0, x0, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("orr x0, x0, x12");                                     // synthetic fd = USER_WRAPPER_FD_BASE | slot index
    emitter.instruction("add sp, sp, #64");                                     // release the wrapper-dispatch scratch
    emitter.instruction("b __rt_fopen_return");                                 // share the common return path

    emitter.label("__rt_fopen_uw_fail");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the wrapper object pointer (or 0 if instantiation never happened)
    emitter.instruction("cbz x0, __rt_fopen_uw_fail_release");                  // no object to release — skip the deep-free
    emitter.instruction("bl __rt_object_free_deep");                            // free the wrapper object so failed dispatches do not leak
    emitter.label("__rt_fopen_uw_fail_release");
    emitter.instruction("add sp, sp, #64");                                     // release the wrapper-dispatch scratch before falling into the shared silent-fail path
    emitter.instruction("b __rt_fopen_silent_fail");                            // share the existing -1 return

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

    // -- recognise user-registered stream wrappers before opening a real file --
    emitter.instruction("xor r9, r9");                                          // wrapper scheme scan index
    emitter.label("__rt_fopen_uw_scan_x86");
    emitter.instruction("lea r10, [r9 + 3]");                                   // need three bytes for the \"://\" marker
    emitter.instruction("cmp r10, rdx");                                        // do enough bytes remain in the path?
    emitter.instruction("jg __rt_fopen_uw_done_x86");                           // no scheme separator found in the path
    emitter.instruction("movzx r11d, BYTE PTR [rax + r9]");                     // load the candidate \":\" byte
    emitter.instruction("cmp r11b, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_fopen_uw_next_x86");                          // not the start of the scheme marker
    emitter.instruction("lea r12, [r9 + 1]");                                   // index of the first '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate first '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_fopen_uw_next_x86");                          // not the scheme marker
    emitter.instruction("lea r12, [r9 + 2]");                                   // index of the second '/'
    emitter.instruction("movzx r11d, BYTE PTR [rax + r12]");                    // load the candidate second '/' byte
    emitter.instruction("cmp r11b, 47");                                        // is it '/'?
    emitter.instruction("jne __rt_fopen_uw_next_x86");                          // not the scheme marker
    emitter.instruction("jmp __rt_fopen_uw_check_wrappers_x86");                // \"://\" found at r9 — check the registrations
    emitter.label("__rt_fopen_uw_next_x86");
    emitter.instruction("inc r9");                                              // advance the scan index
    emitter.instruction("jmp __rt_fopen_uw_scan_x86");                          // keep scanning for the scheme marker

    emitter.label("__rt_fopen_uw_check_wrappers_x86");
    abi::emit_symbol_address(emitter, "r10", "_user_wrappers");                 // wrapper table base
    emitter.instruction("xor r11, r11");                                        // wrapper slot index
    emitter.label("__rt_fopen_uw_slot_x86");
    emitter.instruction("cmp r11, 64");                                         // checked every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("jge __rt_fopen_uw_done_x86");                          // no registered wrapper matched
    emitter.instruction("mov r12, r11");                                        // copy the slot index for scaling
    emitter.instruction("shl r12, 5");                                          // slot offset = index * 32
    emitter.instruction("add r12, r10");                                        // slot base = table + offset
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // stored protocol pointer
    emitter.instruction("test r13, r13");                                       // is this slot empty?
    emitter.instruction("jz __rt_fopen_uw_slot_next_x86");                      // skip empty slots
    emitter.instruction("mov r14, QWORD PTR [r12 + 8]");                        // stored protocol length
    emitter.instruction("cmp r14, r9");                                         // does the stored length match the scheme length?
    emitter.instruction("jne __rt_fopen_uw_slot_next_x86");                     // length mismatch — try the next slot
    emitter.instruction("xor r15, r15");                                        // byte compare index
    emitter.label("__rt_fopen_uw_bytes_x86");
    emitter.instruction("cmp r15, r9");                                         // compared every protocol byte?
    emitter.instruction("jge __rt_fopen_uw_match_x86");                         // full match — dispatch into the user wrapper class
    emitter.instruction("movzx ecx, BYTE PTR [r13 + r15]");                     // stored protocol byte
    emitter.instruction("movzx r8d, BYTE PTR [rax + r15]");                     // path scheme byte
    emitter.instruction("cmp cl, r8b");                                         // do the bytes match?
    emitter.instruction("jne __rt_fopen_uw_slot_next_x86");                     // protocol byte differs — try the next slot
    emitter.instruction("inc r15");                                             // advance the compare index
    emitter.instruction("jmp __rt_fopen_uw_bytes_x86");                         // continue comparing bytes
    emitter.label("__rt_fopen_uw_slot_next_x86");
    emitter.instruction("inc r11");                                             // advance the slot index
    emitter.instruction("jmp __rt_fopen_uw_slot_x86");                          // continue scanning slots
    emitter.label("__rt_fopen_uw_done_x86");

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
    emitter.instruction("test eax, eax");                                       // did libc open() return a negative C int descriptor?
    emitter.instruction("jns __rt_fopen_opened_x86");                           // skip the warning when fopen() succeeded
    emitter.label("__rt_fopen_fail_x86");
    emit_fopen_failed_warning(emitter);
    emitter.instruction("mov rax, -1");                                         // normalize all open failures to the PHP false sentinel path
    emitter.instruction("jmp __rt_fopen_return_x86");                           // skip eof-flag reset on failed opens
    emitter.label("__rt_fopen_silent_fail_x86");
    emitter.instruction("mov rax, -1");                                         // return -1 without emitting a warning (user wrapper match)
    emitter.instruction("jmp __rt_fopen_return_x86");                           // share the common return path

    // -- user-wrapper dispatch: matched scheme, r12 = wrapper slot base --
    //    Stack scratch layout below the fopen frame:
    //      [rsp + 0]  path ptr
    //      [rsp + 8]  path len
    //      [rsp + 16] mode ptr
    //      [rsp + 24] mode len
    //      [rsp + 32] obj ptr (from __rt_new_by_name)
    //      [rsp + 40] handle slot index
    //      [rsp + 48] stream_open ptr (saved across call rax)
    //      [rsp + 56] padding
    emitter.label("__rt_fopen_uw_match_x86");
    emitter.instruction("sub rsp, 64");                                         // reserve wrapper-dispatch scratch below the fopen frame
    emitter.instruction("mov QWORD PTR [rsp + 0], rax");                        // save path ptr across __rt_new_by_name and stream_open
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // save path len across __rt_new_by_name and stream_open
    emitter.instruction("mov QWORD PTR [rsp + 16], rdi");                       // save mode ptr across __rt_new_by_name and stream_open
    emitter.instruction("mov QWORD PTR [rsp + 24], rsi");                       // save mode len across __rt_new_by_name and stream_open
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // pre-initialise the obj slot to 0 so the fail path can tell whether an object was allocated

    // -- instantiate the wrapper class via __rt_new_by_name --
    emitter.instruction("mov rax, QWORD PTR [r12 + 16]");                       // wrapper class name pointer from the registry slot
    emitter.instruction("mov rdx, QWORD PTR [r12 + 24]");                       // wrapper class name length from the registry slot
    emitter.instruction("call __rt_new_by_name");                               // returns obj pointer in rax, or 0 when the class is unknown
    emitter.instruction("test rax, rax");                                       // unknown class?
    emitter.instruction("jz __rt_fopen_uw_fail_x86");                           // unknown class → silent fail with -1
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // save the wrapper object pointer for later

    // -- look up stream_open in the per-class user-wrapper vtable (slot 0) --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable for the resolved class
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the stream_open method pointer from slot 0
    emitter.instruction("test r11, r11");                                       // class did not implement stream_open?
    emitter.instruction("jz __rt_fopen_uw_fail_x86");                           // no stream_open → silent fail
    emitter.instruction("mov QWORD PTR [rsp + 48], r11");                       // save stream_open ptr across the upcoming call

    // -- allocate the first free slot in _user_wrapper_handles --
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("xor r12, r12");                                        // start scanning from handle slot 0
    emitter.label("__rt_fopen_uw_handle_scan_x86");
    emitter.instruction("cmp r12, 256");                                        // does any free handle slot remain (USER_WRAPPER_HANDLES_CAP)?
    emitter.instruction("jge __rt_fopen_uw_fail_x86");                          // table full → silent fail (obj is freed on the shared fail path)
    emitter.instruction("mov r13, QWORD PTR [r10 + r12 * 8]");                  // load slot — null means free
    emitter.instruction("test r13, r13");                                       // is this slot free?
    emitter.instruction("jz __rt_fopen_uw_handle_alloc_x86");                   // free slot found
    emitter.instruction("inc r12");                                             // advance to the next handle slot
    emitter.instruction("jmp __rt_fopen_uw_handle_scan_x86");                   // keep scanning
    emitter.label("__rt_fopen_uw_handle_alloc_x86");
    emitter.instruction("mov QWORD PTR [rsp + 40], r12");                       // save the allocated handle slot index

    // -- call stream_open(obj, path, mode, options=0, opened_path_addr) --
    //    The 7th int-arg (opened_path scratch address) overflows the 6-reg
    //    SysV limit and must be passed on the stack at [rsp+0]. The 16-byte
    //    sub/add preserves rsp 16-byte alignment at the call point and
    //    shifts the dispatch scratch frame by +16 only across the call;
    //    we load r11 (stream_open ptr) BEFORE the sub so its [rsp+48]
    //    reference is still valid.
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // $this = wrapper object
    emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                        // path ptr
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // path len
    emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                       // mode ptr
    emitter.instruction("mov r8,  QWORD PTR [rsp + 24]");                       // mode len
    emitter.instruction("xor r9d, r9d");                                        // options = 0 (PHP STREAM_USE_PATH/REPORT_ERRORS unused in v1)
    emitter.instruction("mov r11, QWORD PTR [rsp + 48]");                       // reload stream_open method pointer before the stack shift
    abi::emit_symbol_address(emitter, "r10", "_stream_open_opened_path_scratch"); // address of the 16-byte opened_path scratch slot
    emitter.instruction("mov QWORD PTR [r10], 0");                              // zero the opened_path low half before the call
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // zero the opened_path high half before the call
    emitter.instruction("sub rsp, 16");                                         // reserve a 16-byte stack-arg slot for the 7th int arg (rsp stays 16-aligned)
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // 7th arg (opened_path address) at [rsp+0]
    emitter.instruction("call r11");                                            // invoke stream_open on the wrapper object
    emitter.instruction("add rsp, 16");                                         // release the stack-arg slot
    emitter.instruction("test rax, rax");                                       // did stream_open return false?
    emitter.instruction("jz __rt_fopen_uw_fail_x86");                           // stream_open returned false → silent fail (obj is freed on the shared fail path)

    // -- success: store obj in the handle slot and return the synthetic fd --
    emitter.instruction("mov r12, QWORD PTR [rsp + 40]");                       // reload the handle slot index
    emitter.instruction("mov r13, QWORD PTR [rsp + 32]");                       // reload the wrapper object pointer
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov QWORD PTR [r10 + r12 * 8], r13");                  // _user_wrapper_handles[slot] = obj
    emitter.instruction("mov rax, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("or rax, r12");                                         // synthetic fd = USER_WRAPPER_FD_BASE | slot index
    emitter.instruction("add rsp, 64");                                         // release the wrapper-dispatch scratch
    emitter.instruction("jmp __rt_fopen_return_x86");                           // share the common return path

    emitter.label("__rt_fopen_uw_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // reload the wrapper object pointer (or 0 if instantiation never happened)
    emitter.instruction("test rdi, rdi");                                       // any object to release?
    emitter.instruction("jz __rt_fopen_uw_fail_release_x86");                   // no object to release — skip the deep-free
    emitter.instruction("call __rt_object_free_deep");                          // free the wrapper object so failed dispatches do not leak
    emitter.label("__rt_fopen_uw_fail_release_x86");
    emitter.instruction("add rsp, 64");                                         // release the wrapper-dispatch scratch before falling into the shared silent-fail path
    emitter.instruction("jmp __rt_fopen_silent_fail_x86");                      // share the existing -1 return

    emitter.label("__rt_fopen_opened_x86");
    emitter.instruction("cdqe");                                                // normalize the successful C int fd into the runtime's 64-bit descriptor value
    abi::emit_symbol_address(emitter, "r10", "_eof_flags");                     // materialize the eof-flag table for the newly opened descriptor
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
