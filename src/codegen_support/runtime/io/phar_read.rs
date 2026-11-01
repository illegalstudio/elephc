//! Purpose:
//! Emits the runtime `phar://` read helpers: `__rt_phar_read_entry`, which reads
//! and parses a PHAR archive at run time and materializes a named entry as a
//! readable stream, and `__rt_fopen_maybe_phar`, the `fopen` gate that
//! routes a non-literal `phar://...` read URL to it, and write-mode URLs to
//! the PHAR writer's dynamic URL open helper.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (and the minimal x86
//!   runtime) via `crate::codegen_support::runtime::io`.
//! - `__rt_fopen_maybe_phar` is called by the `fopen` lowering's generic
//!   (non-literal-URL) path instead of `__rt_fopen`.
//!
//! Key details:
//! - This is the runtime counterpart to the compile-time `extract_phar_entry`
//!   (`src/codegen_support/phar_stream.rs`). Literal `phar://` URLs still take
//!   the compile-time fast path (bytes embedded in the binary); only non-literal
//!   URLs reach here. The authenticating Rust bridge owns native, tar, ZIP, and
//!   whole-archive compressed reads; a missing bridge fails closed instead of
//!   entering the legacy assembly parser. Runtime write opens preserve the full
//!   URL until `fclose()` so the native bridge can split archive and entry names.
//! - Successful bridge reads tail-call `__rt_data_stream`, which writes the
//!   matched bytes to an unlinked tmpfile and rewinds it so the resulting file
//!   descriptor behaves like any other readable stream.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_phar_read_entry` and `__rt_fopen_maybe_phar` for the active target.
pub fn emit_phar_read(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_phar_read_linux_x86_64(emitter);
        return;
    }

    // ===== __rt_phar_read_entry(x0 = url ptr, x1 = url len) -> x0 = fd =====
    emitter.blank();
    emitter.comment("--- runtime: phar_read_entry ---");
    emitter.label_global("__rt_phar_read_entry");
    // Callee-saved layout (survive the file_get_contents / data_stream calls):
    //   x19 = archive buffer ptr   x20 = archive buffer length N
    //   x23 = entry name ptr       x24 = entry name length
    // Frame: [0]x19 [8]x20 [16]x21 [24]x22 [32]x23 [40]x24 [48]x29 [56]x30.
    emitter.instruction("sub sp, sp, #64");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save callee-saved x21/x22
    emitter.instruction("stp x23, x24, [sp, #32]");                             // save callee-saved x23/x24

    // -- bridge reader for native/tar/zip PHAR containers when published --
    abi::emit_symbol_address(emitter, "x9", "_elephc_phar_extract_url_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the optional elephc-phar bridge entry pointer
    emitter.instruction("cbz x9, __rt_phar_read_fail");                         // no authenticating bridge means the PHAR read must fail closed
    abi::emit_symbol_address(emitter, "x2", "_phar_extract_len");
    emitter.instruction("blr x9");                                              // elephc_phar_extract_url(url_ptr, url_len, &len)
    emitter.instruction("cbz x0, __rt_phar_read_fail");                         // bridge miss means archive or entry was not readable
    abi::emit_symbol_address(emitter, "x9", "_phar_extract_len");
    emitter.instruction("ldr x1, [x9]");                                        // load the extracted entry byte length
    emitter.instruction("bl __rt_data_stream");                                 // copy bridge bytes into a readable temp stream
    emitter.instruction("b __rt_phar_read_done");                               // return the bridge-created descriptor
    emitter.label("__rt_phar_read_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 → PHP false (missing archive/entry)
    emitter.label("__rt_phar_read_done");
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore callee-saved x19/x20
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x23, x24, [sp, #32]");                             // restore callee-saved x23/x24
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the entry stream descriptor


    // ===== __rt_fopen_maybe_phar(x1=fname ptr, x2=fname len, x3=mode ptr, x4=mode len) =====
    emitter.blank();
    emitter.comment("--- runtime: fopen_maybe_phar ---");
    emitter.label_global("__rt_fopen_maybe_phar");
    emit_php_stream_url_dispatch_aarch64(emitter);
    emitter.label("__rt_fopen_maybe_phar_not_php");
    emitter.instruction("cmp x2, #7");                                          // filename at least "phar://" long?
    emitter.instruction("b.lt __rt_fopen_maybe_phar_plain");                    // branch when comparison is below target
    emitter.instruction("ldrb w9, [x1, #0]");                                   // 'p'
    emitter.instruction("cmp w9, #0x70");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #1]");                                   // 'h'
    emitter.instruction("cmp w9, #0x68");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #2]");                                   // 'a'
    emitter.instruction("cmp w9, #0x61");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #3]");                                   // 'r'
    emitter.instruction("cmp w9, #0x72");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #4]");                                   // ':'
    emitter.instruction("cmp w9, #0x3a");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #5]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #6]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("cbz x4, __rt_fopen_maybe_phar_plain");                 // empty mode → not a PHAR wrapper open
    emitter.instruction("ldrb w9, [x3, #0]");                                   // mode[0]
    emitter.instruction("cmp w9, #0x72");                                       // 'r' (read)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_read");                     // read modes use the runtime PHAR reader
    emitter.instruction("cmp w9, #0x77");                                       // 'w' (write/truncate)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_write");                    // write modes use the runtime PHAR writer
    emitter.instruction("cmp w9, #0x61");                                       // 'a' (append)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_write");                    // append modes currently rewrite through the PHAR writer
    emitter.instruction("cmp w9, #0x63");                                       // 'c' (create)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_write");                    // create modes use the runtime PHAR writer
    emitter.instruction("cmp w9, #0x78");                                       // 'x' (create new)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_write");                    // exclusive create modes use the runtime PHAR writer
    emitter.instruction("b __rt_fopen_maybe_phar_plain");                       // unsupported PHAR mode falls back to generic open
    emitter.label("__rt_fopen_maybe_phar_read");
    emitter.instruction("mov x0, x1");                                          // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov x1, x2");                                          // url len → __rt_phar_read_entry arg1
    emitter.instruction("b __rt_phar_read_entry");                              // tail-call the runtime phar reader
    emitter.label("__rt_fopen_maybe_phar_write");
    emitter.instruction("mov x0, x1");                                          // url ptr → __rt_phar_write_open_url arg0
    emitter.instruction("mov x1, x2");                                          // url len → __rt_phar_write_open_url arg1
    emitter.instruction("b __rt_phar_write_open_url");                          // tail-call the runtime PHAR writer opener
    emitter.label("__rt_fopen_maybe_phar_plain");
    emitter.instruction("b __rt_fopen");                                        // tail-call the generic open (args intact)

    // ===== __rt_file_get_contents_maybe_phar(x1=url ptr, x2=url len) -> x1=str ptr, x2=len =====
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_maybe_phar ---");
    emitter.label_global("__rt_file_get_contents_maybe_phar");
    emitter.instruction("cmp x2, #7");                                          // at least "phar://" long?
    emitter.instruction("b.lt __rt_fgc_phar_plain");                            // branch when comparison is below target
    emitter.instruction("ldrb w9, [x1, #0]");                                   // 'p'
    emitter.instruction("cmp w9, #0x70");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #1]");                                   // 'h'
    emitter.instruction("cmp w9, #0x68");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #2]");                                   // 'a'
    emitter.instruction("cmp w9, #0x61");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #3]");                                   // 'r'
    emitter.instruction("cmp w9, #0x72");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #4]");                                   // ':'
    emitter.instruction("cmp w9, #0x3a");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #5]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #6]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
                                                     // phar:// read at run time: open the entry, slurp the fd, close it.
    emitter.instruction("sub sp, sp, #48");                                     // frame: [0]=fd [8]=str ptr [16]=str len [32]=x29 [40]=x30
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("mov x0, x1");                                          // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov x1, x2");                                          // url len → __rt_phar_read_entry arg1
    emitter.instruction("bl __rt_phar_read_entry");                             // x0 = entry stream fd (-1 on missing archive/entry)
    emitter.instruction("cmp x0, #0");                                          // did the phar read fail?
    emitter.instruction("b.lt __rt_fgc_phar_fail");                             // → boxed false
    emitter.instruction("str x0, [sp, #0]");                                    // save the fd for the close below
    emitter.instruction("bl __rt_stream_get_contents");                         // (x0=fd) → x1 = string ptr, x2 = length
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the slurped string ptr/len
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fd
    emitter.syscall(6); // close the entry stream fd
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // restore the string ptr/len as the result
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the entry contents string
    emitter.label("__rt_fgc_phar_fail");
    emitter.instruction("mov x1, #0");                                          // null string ptr → file_get_contents boxes false
    emitter.instruction("mov x2, #0");                                          // prepare AArch64 call argument
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the failure (boxed false)
    emitter.label("__rt_fgc_phar_plain");
    emitter.instruction("b __rt_file_get_contents");                            // tail-call the generic reader (args intact)
}

/// x86_64 Linux variant of the runtime `phar://` read helpers.
fn emit_phar_read_linux_x86_64(emitter: &mut Emitter) {
    // ===== __rt_phar_read_entry(rdi = url ptr, rsi = url len) -> rax = fd =====
    emitter.blank();
    emitter.comment("--- runtime: phar_read_entry ---");
    emitter.label_global("__rt_phar_read_entry");
    // Callee-saved layout (survive the file_get_contents / data_stream calls):
    //   r12 = archive buffer ptr   r13 = archive buffer length N
    //   r14 = entry name ptr       r15 = entry name length
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push r12");                                            // save callee-saved r12
    emitter.instruction("push r13");                                            // save callee-saved r13
    emitter.instruction("push r14");                                            // save callee-saved r14
    emitter.instruction("push r15");                                            // save callee-saved r15
    emitter.instruction("push rbx");                                            // save callee-saved rbx (data_section)
    emitter.instruction("sub rsp, 8");                                          // realign rsp to 16 (6 pushes left it 8-off) for the nested calls

    // -- bridge reader for native/tar/zip PHAR containers when published --
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_phar_extract_url_fn", 0); // load the optional elephc-phar bridge entry pointer
    emitter.instruction("test r9, r9");                                         // was the bridge reader published?
    emitter.instruction("jz __rt_phar_read_fail_x86");                          // no authenticating bridge means the PHAR read must fail closed
    abi::emit_symbol_address(emitter, "rdx", "_phar_extract_len"); // pass output-length scratch to the bridge
    emitter.instruction("call r9");                                             // elephc_phar_extract_url(url_ptr, url_len, &len)
    emitter.instruction("test rax, rax");                                       // did the bridge find archive bytes?
    emitter.instruction("jz __rt_phar_read_fail_x86");                          // bridge miss means archive or entry was not readable
    emitter.instruction("mov rdi, rax");                                        // pass extracted bytes to data_stream
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_phar_extract_len", 0); // load the extracted entry byte length
    emitter.instruction("call __rt_data_stream");                               // copy bridge bytes into a readable temp stream
    emitter.instruction("jmp __rt_phar_read_done_x86");                         // return the bridge-created descriptor
    emitter.label("__rt_phar_read_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 → PHP false
    emitter.label("__rt_phar_read_done_x86");
    emitter.instruction("add rsp, 8");                                          // undo the alignment padding
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the entry stream descriptor


    // ===== __rt_fopen_maybe_phar(rax=fname ptr, rdx=fname len, rdi=mode ptr, rsi=mode len) =====
    emitter.blank();
    emitter.comment("--- runtime: fopen_maybe_phar ---");
    emitter.label_global("__rt_fopen_maybe_phar");
    emit_php_stream_url_dispatch_x86_64(emitter);
    emitter.label("__rt_fopen_maybe_phar_not_php_x86");
    emitter.instruction("cmp rdx, 7");                                          // filename at least "phar://" long?
    emitter.instruction("jl __rt_fopen_maybe_phar_plain_x86");                  // branch when comparison is below target
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x61");                        // 'a'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x72");                        // 'r'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("test rsi, rsi");                                       // empty mode → not a PHAR wrapper open
    emitter.instruction("jz __rt_fopen_maybe_phar_plain_x86");                  // branch when the checked value is zero or equal
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x72");                        // mode[0] == 'r'?
    emitter.instruction("je __rt_fopen_maybe_phar_read_x86");                   // read modes use the runtime PHAR reader
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x77");                        // mode[0] == 'w'?
    emitter.instruction("je __rt_fopen_maybe_phar_write_x86");                  // write modes use the runtime PHAR writer
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x61");                        // mode[0] == 'a'?
    emitter.instruction("je __rt_fopen_maybe_phar_write_x86");                  // append modes currently rewrite through the PHAR writer
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x63");                        // mode[0] == 'c'?
    emitter.instruction("je __rt_fopen_maybe_phar_write_x86");                  // create modes use the runtime PHAR writer
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x78");                        // mode[0] == 'x'?
    emitter.instruction("je __rt_fopen_maybe_phar_write_x86");                  // exclusive create modes use the runtime PHAR writer
    emitter.instruction("jmp __rt_fopen_maybe_phar_plain_x86");                 // unsupported PHAR mode falls back to generic open
    emitter.label("__rt_fopen_maybe_phar_read_x86");
    emitter.instruction("mov rdi, rax");                                        // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov rsi, rdx");                                        // url len → __rt_phar_read_entry arg1
    emitter.instruction("jmp __rt_phar_read_entry");                            // tail-call the runtime phar reader
    emitter.label("__rt_fopen_maybe_phar_write_x86");
    emitter.instruction("mov rdi, rax");                                        // url ptr → __rt_phar_write_open_url arg0
    emitter.instruction("mov rsi, rdx");                                        // url len → __rt_phar_write_open_url arg1
    emitter.instruction("jmp __rt_phar_write_open_url");                        // tail-call the runtime PHAR writer opener
    emitter.label("__rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("jmp __rt_fopen");                                      // tail-call the generic open (args intact)

    // ===== __rt_file_get_contents_maybe_phar(rax=url ptr, rdx=url len) -> rax=str ptr, rdx=len =====
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_maybe_phar ---");
    emitter.label_global("__rt_file_get_contents_maybe_phar");
    emitter.instruction("cmp rdx, 7");                                          // at least "phar://" long?
    emitter.instruction("jl __rt_fgc_phar_plain_x86");                          // branch when comparison is below target
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x61");                        // 'a'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x72");                        // 'r'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
                                                        // phar:// read at run time: open the entry, slurp the fd, close it.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame: [rbp-8]=fd [rbp-16]=str ptr [rbp-24]=len
    emitter.instruction("mov rdi, rax");                                        // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov rsi, rdx");                                        // url len → __rt_phar_read_entry arg1
    emitter.instruction("call __rt_phar_read_entry");                           // rax = entry stream fd (-1 on missing archive/entry)
    emitter.instruction("cmp rax, 0");                                          // did the phar read fail?
    emitter.instruction("jl __rt_fgc_phar_fail_x86");                           // → boxed false
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the fd for the close below
    emitter.instruction("mov rdi, rax");                                        // fd → __rt_stream_get_contents arg
    emitter.instruction("call __rt_stream_get_contents");                       // rax = string ptr, rdx = length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the slurped string ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the slurped string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fd
    emitter.instruction("call close");                                          // close the entry stream fd
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the string ptr as the result
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // restore the string length as the result
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the entry contents string
    emitter.label("__rt_fgc_phar_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null string ptr → file_get_contents boxes false
    emitter.instruction("xor edx, edx");                                        // clear register value
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure (boxed false)
    emitter.label("__rt_fgc_phar_plain_x86");
    emitter.instruction("jmp __rt_file_get_contents");                          // tail-call the generic reader (args intact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies every supported architecture emits only the authenticating bridge path.
    #[test]
    fn missing_phar_bridge_never_enters_the_assembly_parser() {
        for (target, fail_branch, legacy_branch) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "cbz x9, __rt_phar_read_fail",
                "cbz x9, __rt_phar_read_asm_fallback",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "jz __rt_phar_read_fail_x86",
                "jz __rt_phar_read_asm_fallback_x86",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_phar_read(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains(fail_branch), "{target:?}");
            assert!(!assembly.contains(legacy_branch), "{target:?}");
            assert!(!assembly.contains("__rt_phar_read_asm_fallback:"), "{target:?}");
            assert!(!assembly.contains("__rt_phar_read_asm_fallback_x86:"), "{target:?}");
            assert!(!assembly.contains("__rt_phar_inflate_raw:"), "{target:?}");
            assert!(!assembly.contains("__rt_phar_bzip2_decompress:"), "{target:?}");
        }
    }
}

/// What a `php://` stream URL opens: a standard descriptor or an anonymous temp file.
#[derive(Clone, Copy)]
enum PhpStreamTarget {
    Fd(u8),
    TempFile,
}

/// The `php://` names a runtime `fopen()` resolves, as the literal-path lowering does
/// (`php_standard_stream_fd` / `is_php_memory_stream`): the same descriptors, the same temp file.
const PHP_STREAM_NAMES: &[(&str, PhpStreamTarget)] = &[
    ("stdin", PhpStreamTarget::Fd(0)),
    ("input", PhpStreamTarget::Fd(0)),
    ("stdout", PhpStreamTarget::Fd(1)),
    ("output", PhpStreamTarget::Fd(1)),
    ("stderr", PhpStreamTarget::Fd(2)),
    ("memory", PhpStreamTarget::TempFile),
    ("temp", PhpStreamTarget::TempFile),
];

/// Emits the ARM64 `php://` prefix check and name match at the head of `__rt_fopen_maybe_phar`.
///
/// A `php://` URL held in a variable used to reach the plain `open(2)` path and fail: only a
/// LITERAL was recognized at compile time. Symfony's `AbstractDumper` opens
/// `static::$defaultOutput` (`php://output`, `php://stdout`), so every Laravel boot warned
/// `fopen(): Failed to open stream`. Falls through to `__rt_fopen_maybe_phar_not_php`.
fn emit_php_stream_url_dispatch_aarch64(emitter: &mut Emitter) {
    emitter.instruction("cmp x2, #6");                                          // filename at least "php://" long?
    emitter.instruction("b.lt __rt_fopen_maybe_phar_not_php");                  // shorter names cannot be php:// streams
    for (index, byte) in b"php://".iter().enumerate() {
        emitter.instruction(&format!("ldrb w9, [x1, #{}]", index));             // load one byte of the php:// prefix
        emitter.instruction(&format!("cmp w9, #{}", byte));                     // compare it with the expected prefix byte
        emitter.instruction("b.ne __rt_fopen_maybe_phar_not_php");              // not a php:// URL
    }
    emitter.instruction("add x10, x1, #6");                                     // x10 = stream name after php://
    emitter.instruction("sub x11, x2, #6");                                     // x11 = stream name length
    for (name, target) in PHP_STREAM_NAMES {
        let next = format!("__rt_fopen_php_stream_not_{}", name);
        emitter.instruction(&format!("cmp x11, #{}", name.len()));              // does the name length match this stream?
        emitter.instruction(&format!("b.ne {}", next));                         // try the next stream name
        for (index, byte) in name.bytes().enumerate() {
            emitter.instruction(&format!("ldrb w9, [x10, #{}]", index));        // load one stream-name byte
            emitter.instruction(&format!("cmp w9, #{}", byte));                 // compare it with this stream's name
            emitter.instruction(&format!("b.ne {}", next));                     // try the next stream name
        }
        emit_php_stream_target_aarch64(emitter, *target);
        emitter.label(&next);
    }
    // `php://temp/maxmemory:NN` names the same temp stream.
    emitter.instruction("cmp x11, #5");                                         // long enough for "temp/"?
    emitter.instruction("b.lt __rt_fopen_maybe_phar_not_php");                  // no other php:// stream is recognized here
    for (index, byte) in b"temp/".iter().enumerate() {
        emitter.instruction(&format!("ldrb w9, [x10, #{}]", index));            // load one byte of the temp/ prefix
        emitter.instruction(&format!("cmp w9, #{}", byte));                     // compare it with the temp/ prefix
        emitter.instruction("b.ne __rt_fopen_maybe_phar_not_php");              // not a php://temp/ URL
    }
    emit_php_stream_target_aarch64(emitter, PhpStreamTarget::TempFile);
}

/// Emits the ARM64 result for one recognized `php://` stream.
fn emit_php_stream_target_aarch64(emitter: &mut Emitter, target: PhpStreamTarget) {
    match target {
        PhpStreamTarget::Fd(fd) => {
            // php opens a DUPLICATE of the standard descriptor, so releasing the stream (which
            // closes it) leaves the process's own stdout in place for later output.
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // save the frame pointer and return address around the C call
            emitter.instruction("mov x29, sp");                                 // establish a frame for the C call
            emitter.instruction(&format!("mov x0, #{}", fd));                   // the standard descriptor to duplicate
            emitter.bl_c("dup");                                                // x0 = duplicated descriptor (-1 on failure)
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the frame pointer and return address
            emitter.instruction("ret");                                         // return the descriptor to the fopen lowering
        }
        PhpStreamTarget::TempFile => {
            emitter.instruction("b __rt_tmpfile");                              // tail-call the anonymous temp-file opener
        }
    }
}

/// Emits the x86_64 `php://` prefix check and name match at the head of `__rt_fopen_maybe_phar`.
///
/// See [`emit_php_stream_url_dispatch_aarch64`]. Falls through to
/// `__rt_fopen_maybe_phar_not_php_x86`.
fn emit_php_stream_url_dispatch_x86_64(emitter: &mut Emitter) {
    emitter.instruction("cmp rdx, 6");                                          // filename at least "php://" long?
    emitter.instruction("jl __rt_fopen_maybe_phar_not_php_x86");                // shorter names cannot be php:// streams
    for (index, byte) in b"php://".iter().enumerate() {
        emitter.instruction(&format!("cmp BYTE PTR [rax + {}], {}", index, byte)); // compare one byte of the php:// prefix
        emitter.instruction("jne __rt_fopen_maybe_phar_not_php_x86");           // not a php:// URL
    }
    emitter.instruction("lea r10, [rax + 6]");                                  // r10 = stream name after php://
    emitter.instruction("lea r11, [rdx - 6]");                                  // r11 = stream name length
    for (name, target) in PHP_STREAM_NAMES {
        let next = format!("__rt_fopen_php_stream_not_{}_x86", name);
        emitter.instruction(&format!("cmp r11, {}", name.len()));               // does the name length match this stream?
        emitter.instruction(&format!("jne {}", next));                          // try the next stream name
        for (index, byte) in name.bytes().enumerate() {
            emitter.instruction(&format!("cmp BYTE PTR [r10 + {}], {}", index, byte)); // compare one stream-name byte
            emitter.instruction(&format!("jne {}", next));                      // try the next stream name
        }
        emit_php_stream_target_x86_64(emitter, *target);
        emitter.label(&next);
    }
    // `php://temp/maxmemory:NN` names the same temp stream.
    emitter.instruction("cmp r11, 5");                                          // long enough for "temp/"?
    emitter.instruction("jl __rt_fopen_maybe_phar_not_php_x86");                // no other php:// stream is recognized here
    for (index, byte) in b"temp/".iter().enumerate() {
        emitter.instruction(&format!("cmp BYTE PTR [r10 + {}], {}", index, byte)); // compare one byte of the temp/ prefix
        emitter.instruction("jne __rt_fopen_maybe_phar_not_php_x86");           // not a php://temp/ URL
    }
    emit_php_stream_target_x86_64(emitter, PhpStreamTarget::TempFile);
}

/// Emits the x86_64 result for one recognized `php://` stream.
fn emit_php_stream_target_x86_64(emitter: &mut Emitter, target: PhpStreamTarget) {
    match target {
        PhpStreamTarget::Fd(fd) => {
            // php opens a DUPLICATE of the standard descriptor; see the ARM64 arm.
            emitter.instruction("sub rsp, 8");                                  // realign the stack to 16 bytes for the C call
            emitter.instruction(&format!("mov edi, {}", fd));                   // the standard descriptor to duplicate
            emitter.instruction("call dup");                                    // rax = duplicated descriptor (-1 on failure)
            emitter.instruction("add rsp, 8");                                  // restore the caller's stack pointer
            emitter.instruction("ret");                                         // return the descriptor to the fopen lowering
        }
        PhpStreamTarget::TempFile => {
            emitter.instruction("jmp __rt_tmpfile");                            // tail-call the anonymous temp-file opener
        }
    }
}
