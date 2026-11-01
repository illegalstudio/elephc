//! Purpose:
//! Emits `__rt_zip_register_stream_meta`, which records the URI and kind of a
//! synthetic user-wrapper stream so `stream_get_meta_data()` can report zip metadata.
//!
//! Called from:
//! - `crate::codegen_support::runtime::io::fopen` on every successful user-wrapper open.
//! - `crate::codegen_support::io::stream_get_meta_data` when the descriptor is synthetic.
//!
//! Key details:
//! - Kind 1 is a native `zip://` fopen; kind 2 is the `elephc-zip-memory://` stream
//!   returned by `ZipArchive::getStream*()`. Other user wrappers store kind 0.
//! - Each handle keeps a 256-byte URI, enough for the fixture paths and the entry name
//!   reported by `getStream*()`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the helper for the active target.
pub fn emit_zip_register_stream_meta(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// AArch64 helper: x0 = handle slot, x1 = path pointer, x2 = path length.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zip_register_stream_meta ---");
    emitter.label_global("__rt_zip_register_stream_meta");
    emitter.instruction("sub sp, sp, #64");                                     // reserve callee-saved spill space
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save callee-saved x21/x22
    emitter.instruction("mov x19, x0");                                         // x19 = handle slot
    emitter.instruction("mov x20, x1");                                         // x20 = URI bytes
    emitter.instruction("mov x21, x2");                                         // x21 = URI length
    emitter.instruction("mov x22, #0");                                         // x22 = kind (0 = generic)

    // -- zip:// prefix --
    emitter.instruction("cmp x21, #6");                                         // at least "zip://" long?
    emitter.instruction("b.lt __rt_zrsm_memory");                               // too short for zip://
    emitter.instruction("ldrb w9, [x20, #0]");                                  // 'z'
    emitter.instruction("cmp w9, #0x7a");                                       // compare 'z'
    emitter.instruction("b.ne __rt_zrsm_memory");                               // not zip://
    emitter.instruction("ldrb w9, [x20, #1]");                                  // 'i'
    emitter.instruction("cmp w9, #0x69");                                       // compare 'i'
    emitter.instruction("b.ne __rt_zrsm_memory");                               // not zip://
    emitter.instruction("ldrb w9, [x20, #2]");                                  // 'p'
    emitter.instruction("cmp w9, #0x70");                                       // compare 'p'
    emitter.instruction("b.ne __rt_zrsm_memory");                               // not zip://
    emitter.instruction("ldrb w9, [x20, #3]");                                  // ':'
    emitter.instruction("cmp w9, #0x3a");                                       // compare ':'
    emitter.instruction("b.ne __rt_zrsm_memory");                               // not zip://
    emitter.instruction("ldrb w9, [x20, #4]");                                  // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare '/'
    emitter.instruction("b.ne __rt_zrsm_memory");                               // not zip://
    emitter.instruction("ldrb w9, [x20, #5]");                                  // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare '/'
    emitter.instruction("b.ne __rt_zrsm_memory");                               // not zip://
    emitter.instruction("mov x22, #1");                                         // kind 1 = direct zip:// open
    emitter.instruction("b __rt_zrsm_store");                                   // record metadata

    // -- elephc-zip-memory:// prefix --
    emitter.label("__rt_zrsm_memory");
    emitter.instruction("cmp x21, #20");                                        // at least "elephc-zip-memory://" long?
    emitter.instruction("b.lt __rt_zrsm_store");                                // too short for the memory scheme
    for (offset, byte) in "elephc-zip-memory://".bytes().enumerate() {
        emitter.instruction(&format!("ldrb w9, [x20, #{}]", offset));           // scheme byte
        emitter.instruction(&format!("cmp w9, #{}", byte));                     // compare scheme byte
        emitter.instruction("b.ne __rt_zrsm_store");                            // scheme differs
    }
    emitter.instruction("mov x22, #2");                                         // kind 2 = getStream memory wrapper
    emitter.instruction("add x20, x20, #20");                                   // skip the scheme prefix
    emitter.instruction("sub x21, x21, #20");                                   // remaining "<token>/<entry>"
    emitter.instruction("mov x9, #0");                                          // token separator index
    emitter.label("__rt_zrsm_separator");
    emitter.instruction("cmp x9, x21");                                         // scanned the whole remainder?
    emitter.instruction("b.ge __rt_zrsm_no_entry");                             // no separator → empty URI
    emitter.instruction("ldrb w10, [x20, x9]");                                 // load a candidate byte
    emitter.instruction("cmp w10, #0x2f");                                      // is it '/'?
    emitter.instruction("b.eq __rt_zrsm_entry");                               // entry starts after the slash
    emitter.instruction("add x9, x9, #1");                                      // advance the scan
    emitter.instruction("b __rt_zrsm_separator");                              // keep scanning
    emitter.label("__rt_zrsm_entry");
    emitter.instruction("add x20, x20, x9");                                    // advance to the separator
    emitter.instruction("add x20, x20, #1");                                    // skip the slash
    emitter.instruction("sub x21, x21, x9");                                    // remaining bytes from the separator
    emitter.instruction("sub x21, x21, #1");                                    // drop the separator byte
    emitter.instruction("b __rt_zrsm_store");                                  // record metadata
    emitter.label("__rt_zrsm_no_entry");
    emitter.instruction("mov x21, #0");                                         // no entry component

    emitter.label("__rt_zrsm_store");
    abi_symbol(emitter, "x9", "_zip_stream_kind");
    emitter.instruction("strb w22, [x9, x19]");                                 // _zip_stream_kind[slot] = kind
    abi_symbol(emitter, "x9", "_zip_stream_uri");
    emitter.instruction("mov x10, x19");                                        // slot index
    emitter.instruction("lsl x10, x10, #8");                                    // slot * 256
    emitter.instruction("add x9, x9, x10");                                     // destination URI buffer
    emitter.instruction("mov x10, #0");                                         // copied byte count
    emitter.label("__rt_zrsm_copy");
    emitter.instruction("cmp x10, x21");                                        // copied the whole URI?
    emitter.instruction("b.ge __rt_zrsm_copied");                               // copy finished
    emitter.instruction("cmp x10, #255");                                       // stay inside the 256-byte slot?
    emitter.instruction("b.ge __rt_zrsm_copied");                               // URI is capped
    emitter.instruction("ldrb w11, [x20, x10]");                                // source byte
    emitter.instruction("strb w11, [x9, x10]");                                 // destination byte
    emitter.instruction("add x10, x10, #1");                                    // advance the copy
    emitter.instruction("b __rt_zrsm_copy");                                   // copy the next byte
    emitter.label("__rt_zrsm_copied");
    abi_symbol(emitter, "x9", "_zip_stream_uri_len");
    emitter.instruction("str x10, [x9, x19, lsl #3]");                          // _zip_stream_uri_len[slot] = copied
    emitter.instruction("mov x0, #0x4000");                                     // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl x0, x0, #16");                                     // form 0x40000000
    emitter.instruction("orr x0, x0, x19");                                     // synthetic fd = base | slot
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore callee-saved x19/x20
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the synthetic descriptor
}

/// x86_64 helper: rdi = handle slot, rsi = path pointer, rdx = path length.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zip_register_stream_meta ---");
    emitter.label_global("__rt_zip_register_stream_meta");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push r12");                                            // save callee-saved r12
    emitter.instruction("push r13");                                            // save callee-saved r13
    emitter.instruction("push r14");                                            // save callee-saved r14
    emitter.instruction("push r15");                                            // save callee-saved r15
    emitter.instruction("push rbx");                                            // save callee-saved rbx
    emitter.instruction("sub rsp, 8");                                          // realign the stack
    emitter.instruction("mov r12, rdi");                                        // r12 = handle slot
    emitter.instruction("mov r13, rsi");                                        // r13 = URI bytes
    emitter.instruction("mov r14, rdx");                                        // r14 = URI length
    emitter.instruction("xor r15d, r15d");                                      // r15 = kind (0 = generic)

    // -- zip:// prefix --
    emitter.instruction("cmp r14, 6");                                          // at least "zip://" long?
    emitter.instruction("jl __rt_zrsm_memory_x86");                             // too short for zip://
    emitter.instruction("cmp BYTE PTR [r13 + 0], 0x7a");                        // 'z'
    emitter.instruction("jne __rt_zrsm_memory_x86");                            // not zip://
    emitter.instruction("cmp BYTE PTR [r13 + 1], 0x69");                        // 'i'
    emitter.instruction("jne __rt_zrsm_memory_x86");                            // not zip://
    emitter.instruction("cmp BYTE PTR [r13 + 2], 0x70");                        // 'p'
    emitter.instruction("jne __rt_zrsm_memory_x86");                            // not zip://
    emitter.instruction("cmp BYTE PTR [r13 + 3], 0x3a");                        // ':'
    emitter.instruction("jne __rt_zrsm_memory_x86");                            // not zip://
    emitter.instruction("cmp BYTE PTR [r13 + 4], 0x2f");                        // '/'
    emitter.instruction("jne __rt_zrsm_memory_x86");                            // not zip://
    emitter.instruction("cmp BYTE PTR [r13 + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_zrsm_memory_x86");                            // not zip://
    emitter.instruction("mov r15d, 1");                                         // kind 1 = direct zip:// open
    emitter.instruction("jmp __rt_zrsm_store_x86");                             // record metadata

    // -- elephc-zip-memory:// prefix --
    emitter.label("__rt_zrsm_memory_x86");
    emitter.instruction("cmp r14, 20");                                         // at least the memory scheme?
    emitter.instruction("jl __rt_zrsm_store_x86");                              // too short
    for (offset, byte) in "elephc-zip-memory://".bytes().enumerate() {
        emitter.instruction(&format!("cmp BYTE PTR [r13 + {}], {}", offset, byte)); // scheme byte
        emitter.instruction("jne __rt_zrsm_store_x86");                        // scheme differs
    }
    emitter.instruction("mov r15d, 2");                                         // kind 2 = getStream memory wrapper
    emitter.instruction("add r13, 20");                                          // skip the scheme prefix
    emitter.instruction("sub r14, 20");                                          // remaining "<token>/<entry>"
    emitter.instruction("xor ecx, ecx");                                        // token separator index
    emitter.label("__rt_zrsm_separator_x86");
    emitter.instruction("cmp rcx, r14");                                        // scanned the whole remainder?
    emitter.instruction("jge __rt_zrsm_no_entry_x86");                         // no separator → empty URI
    emitter.instruction("cmp BYTE PTR [r13 + rcx], 0x2f");                      // is it '/'?
    emitter.instruction("je __rt_zrsm_entry_x86");                             // entry starts after the slash
    emitter.instruction("inc rcx");                                             // advance the scan
    emitter.instruction("jmp __rt_zrsm_separator_x86");                        // keep scanning
    emitter.label("__rt_zrsm_entry_x86");
    emitter.instruction("add r13, rcx");                                        // advance to the separator
    emitter.instruction("inc r13");                                             // skip the slash
    emitter.instruction("sub r14, rcx");                                        // remaining bytes from the separator
    emitter.instruction("dec r14");                                             // drop the separator byte
    emitter.instruction("jmp __rt_zrsm_store_x86");                            // record metadata
    emitter.label("__rt_zrsm_no_entry_x86");
    emitter.instruction("xor r14d, r14d");                                      // no entry component

    emitter.label("__rt_zrsm_store_x86");
    abi_symbol(emitter, "r9", "_zip_stream_kind");
    emitter.instruction("mov BYTE PTR [r9 + r12], r15b");                      // _zip_stream_kind[slot] = kind
    abi_symbol(emitter, "r9", "_zip_stream_uri");
    emitter.instruction("mov r10, r12");                                        // slot index
    emitter.instruction("shl r10, 8");                                          // slot * 256
    emitter.instruction("add r9, r10");                                         // destination URI buffer
    emitter.instruction("xor ecx, ecx");                                        // copied byte count
    emitter.label("__rt_zrsm_copy_x86");
    emitter.instruction("cmp rcx, r14");                                        // copied the whole URI?
    emitter.instruction("jge __rt_zrsm_copied_x86");                           // copy finished
    emitter.instruction("cmp rcx, 255");                                        // stay inside the 256-byte slot?
    emitter.instruction("jge __rt_zrsm_copied_x86");                           // URI is capped
    emitter.instruction("mov al, BYTE PTR [r13 + rcx]");                       // source byte
    emitter.instruction("mov BYTE PTR [r9 + rcx], al");                        // destination byte
    emitter.instruction("inc rcx");                                             // advance the copy
    emitter.instruction("jmp __rt_zrsm_copy_x86");                             // copy the next byte
    emitter.label("__rt_zrsm_copied_x86");
    abi_symbol(emitter, "r9", "_zip_stream_uri_len");
    emitter.instruction("mov QWORD PTR [r9 + r12 * 8], rcx");                   // _zip_stream_uri_len[slot] = copied
    emitter.instruction("mov rax, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("or rax, r12");                                         // synthetic fd = base | slot
    emitter.instruction("add rsp, 8");                                          // undo the alignment padding
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the synthetic descriptor
}

/// Emits a symbol address through the target-aware ABI helper.
fn abi_symbol(emitter: &mut Emitter, reg: &str, symbol: &str) {
    crate::codegen_support::abi::emit_symbol_address(emitter, reg, symbol);
}
