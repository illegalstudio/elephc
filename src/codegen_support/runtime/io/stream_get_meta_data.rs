//! Purpose:
//! Emits the `__rt_stream_get_meta_data` runtime helper, which builds the
//! PHP-compatible metadata hash describing an open stream resource.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Returns a `{string => mixed}` hash with the nine documented keys. `eof`
//!   comes from the `_eof_flags` table; `seekable`/`stream_type` are derived
//!   from `lseek`; `blocked`/`mode` from `fcntl(F_GETFL)`.
//! - `wrapper_type` is reported as `"plainfile"` and `uri` as the empty string:
//!   elephc does not track per-resource open paths.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;

/// stream_get_meta_data: build the metadata hash for a stream descriptor.
/// Input:  AArch64 x0 = descriptor / x86_64 rdi = descriptor
/// Output: pointer to a `{string => mixed}` hash table
pub fn emit_stream_get_meta_data(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_meta_data_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let nonblock = plat.o_nonblock();

    emitter.blank();
    emitter.comment("--- runtime: stream_get_meta_data ---");
    emitter.label_global("__rt_stream_get_meta_data");

    // Frame (96 bytes): [0]=fd [8]=hash [16]=seekable [24]=blocked [32]=eof
    //                   [40]=mode_ptr [48]=mode_len [56]=stype_ptr [64]=stype_len
    //                   [80]=x29 [88]=x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the metadata frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the stream descriptor

    // -- seekability: lseek(fd, 0, SEEK_CUR) --
    emitter.instruction("mov x1, #0");                                          // offset 0
    emitter.instruction("mov x2, #1");                                          // SEEK_CUR
    emitter.syscall(199);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means lseek failed
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_sgmd_seekable")); // lseek ok: the stream is seekable

    // -- not seekable: socket-like stream --
    emitter.instruction("mov x9, #0");                                          // seekable = false
    emitter.instruction("str x9, [sp, #16]");                                   // save the seekable flag
    abi::emit_symbol_address(emitter, "x10", "_meta_stype_socket");             // load page of the "tcp_socket" literal
    emitter.instruction("str x10, [sp, #56]");                                  // save the stream_type pointer
    emitter.instruction("mov x10, #10");                                        // length of "tcp_socket"
    emitter.instruction("str x10, [sp, #64]");                                  // save the stream_type length
    emitter.instruction("b __rt_sgmd_seek_done");                               // skip the seekable branch

    emitter.label("__rt_sgmd_seekable");
    emitter.instruction("mov x9, #1");                                          // seekable = true
    emitter.instruction("str x9, [sp, #16]");                                   // save the seekable flag
    abi::emit_symbol_address(emitter, "x10", "_meta_stype_stdio");              // load page of the "STDIO" literal
    emitter.instruction("str x10, [sp, #56]");                                  // save the stream_type pointer
    emitter.instruction("mov x10, #5");                                         // length of "STDIO"
    emitter.instruction("str x10, [sp, #64]");                                  // save the stream_type length
    emitter.label("__rt_sgmd_seek_done");

    // -- blocking mode + access mode: fcntl(fd, F_GETFL, 0) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the stream descriptor
    emitter.instruction("mov x1, #3");                                          // F_GETFL
    emitter.instruction("mov x2, #0");                                          // unused third argument
    emitter.syscall(92);
    emitter.instruction(&format!("mov x9, #{}", nonblock));                     // the O_NONBLOCK flag bit
    emitter.instruction("tst x0, x9");                                          // is the O_NONBLOCK bit set?
    emitter.instruction("cset x10, eq");                                        // blocked = 1 when O_NONBLOCK is clear
    emitter.instruction("str x10, [sp, #24]");                                  // save the blocked flag
    emitter.instruction("and x9, x0, #3");                                      // isolate the O_ACCMODE access bits
    emitter.instruction("cmp x9, #1");                                          // O_WRONLY?
    emitter.instruction("b.eq __rt_sgmd_mode_w");                               // write-only stream
    emitter.instruction("cmp x9, #2");                                          // O_RDWR?
    emitter.instruction("b.eq __rt_sgmd_mode_rw");                              // read-write stream

    abi::emit_symbol_address(emitter, "x10", "_meta_mode_r");                   // load page of the "r" literal
    emitter.instruction("mov x11, #1");                                         // length of "r"
    emitter.instruction("b __rt_sgmd_mode_done");                               // mode resolved
    emitter.label("__rt_sgmd_mode_w");
    abi::emit_symbol_address(emitter, "x10", "_meta_mode_w");                   // load page of the "w" literal
    emitter.instruction("mov x11, #1");                                         // length of "w"
    emitter.instruction("b __rt_sgmd_mode_done");                               // mode resolved
    emitter.label("__rt_sgmd_mode_rw");
    abi::emit_symbol_address(emitter, "x10", "_meta_mode_rw");                  // load page of the "r+" literal
    emitter.instruction("mov x11, #2");                                         // length of "r+"
    emitter.label("__rt_sgmd_mode_done");
    emitter.instruction("str x10, [sp, #40]");                                  // save the mode pointer
    emitter.instruction("str x11, [sp, #48]");                                  // save the mode length

    // -- end-of-file flag from the _eof_flags table --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the stream descriptor
    abi::emit_symbol_address(emitter, "x9", "_eof_flags");                      // load page of the EOF flag table
    emitter.instruction("ldrb w10, [x9, x0]");                                  // load _eof_flags[fd]
    emitter.instruction("cmp w10, #0");                                         // has end-of-file been observed?
    emitter.instruction("cset x10, ne");                                        // eof = 1 when the flag byte is set
    emitter.instruction("str x10, [sp, #32]");                                  // save the eof flag

    // -- create the metadata hash (capacity 16, value type = mixed) --
    emitter.instruction("mov x0, #16");                                         // initial capacity
    emitter.instruction("mov x1, #7");                                          // value type = mixed
    emitter.instruction("bl __rt_hash_new");                                    // allocate the hash; x0 = hash pointer
    emitter.instruction("str x0, [sp, #8]");                                    // save the hash pointer

    emit_set_bool_const(emitter, "_meta_key_timed_out", 9, 0);
    emit_set_bool_slot(emitter, "_meta_key_blocked", 7, 24);
    emit_set_bool_slot(emitter, "_meta_key_eof", 3, 32);
    emit_set_int_const(emitter, "_meta_key_unread_bytes", 12);
    emit_set_str_slots(emitter, "_meta_key_stream_type", 11, 56, 64);
    emit_set_str_const(emitter, "_meta_key_wrapper_type", 12, "_meta_wrapper_plainfile", 9);
    emit_set_str_slots(emitter, "_meta_key_mode", 4, 40, 48);
    emit_set_bool_slot(emitter, "_meta_key_seekable", 8, 16);
    emit_set_str_const(emitter, "_meta_key_uri", 3, "_meta_wrapper_plainfile", 0);

    // -- return the completed hash --
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the final hash pointer
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the metadata frame
    emitter.instruction("ret");                                                 // return the metadata hash pointer
}

/// Emit one `__rt_hash_set` with the value already staged in x3/x4/x5.
fn emit_hash_put_aarch64(emitter: &mut Emitter, key_sym: &str, key_len: i64) {
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    abi::emit_symbol_address(emitter, "x1", key_sym);                           // load page of the key literal
    emitter.instruction(&format!("mov x2, #{}", key_len));                      // key length
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry; x0 = updated hash
    emitter.instruction("str x0, [sp, #8]");                                    // persist any post-grow hash pointer
}

/// Emits the set bool const stream runtime helper.
fn emit_set_bool_const(emitter: &mut Emitter, key_sym: &str, key_len: i64, value: i64) {
    emitter.instruction(&format!("mov x3, #{}", value));                        // value_lo = boolean payload
    emitter.instruction("mov x4, #0");                                          // value_hi unused for booleans
    emitter.instruction("mov x5, #3");                                          // value tag = bool
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the set bool slot stream runtime helper.
fn emit_set_bool_slot(emitter: &mut Emitter, key_sym: &str, key_len: i64, slot: i64) {
    emitter.instruction(&format!("ldr x3, [sp, #{}]", slot));                   // value_lo = computed boolean
    emitter.instruction("mov x4, #0");                                          // value_hi unused for booleans
    emitter.instruction("mov x5, #3");                                          // value tag = bool
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the set int const stream runtime helper.
fn emit_set_int_const(emitter: &mut Emitter, key_sym: &str, key_len: i64) {
    emitter.instruction("mov x3, #0");                                          // value_lo = 0 (elephc keeps no read buffer)
    emitter.instruction("mov x4, #0");                                          // value_hi unused for integers
    emitter.instruction("mov x5, #0");                                          // value tag = int
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the set str const stream runtime helper.
fn emit_set_str_const(emitter: &mut Emitter, key_sym: &str, key_len: i64, val_sym: &str, val_len: i64) {
    abi::emit_symbol_address(emitter, "x3", val_sym);                           // load page of the value literal
    emitter.instruction(&format!("mov x4, #{}", val_len));                      // value_hi = string length
    emitter.instruction("mov x5, #1");                                          // value tag = string
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the set str slots stream runtime helper.
fn emit_set_str_slots(emitter: &mut Emitter, key_sym: &str, key_len: i64, ptr_slot: i64, len_slot: i64) {
    emitter.instruction(&format!("ldr x3, [sp, #{}]", ptr_slot));               // value_lo = string pointer
    emitter.instruction(&format!("ldr x4, [sp, #{}]", len_slot));               // value_hi = string length
    emitter.instruction("mov x5, #1");                                          // value tag = string
    emit_hash_put_aarch64(emitter, key_sym, key_len);
}

/// Emits the Linux x86_64 stream runtime helper for stream get meta data.
fn emit_stream_get_meta_data_linux_x86_64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let nonblock = plat.o_nonblock();

    emitter.blank();
    emitter.comment("--- runtime: stream_get_meta_data ---");
    emitter.label_global("__rt_stream_get_meta_data");

    // Frame (rbp-relative): [-8]=fd [-16]=hash [-24]=seekable [-32]=blocked
    //                       [-40]=eof [-48]=mode_ptr [-56]=mode_len
    //                       [-64]=stype_ptr [-72]=stype_len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve the metadata spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the stream descriptor

    // -- seekability: lseek(fd, 0, SEEK_CUR) --
    emitter.instruction("xor esi, esi");                                        // offset 0
    emitter.instruction("mov edx, 1");                                          // SEEK_CUR
    emitter.instruction("mov eax, 8");                                          // Linux x86_64 syscall 8 = lseek
    emitter.instruction("syscall");                                             // probe whether the descriptor is seekable
    emitter.instruction("test rax, rax");                                       // did lseek fail with a negative result?
    emitter.instruction("jns __rt_sgmd_seekable_x86");                          // lseek ok: the stream is seekable

    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // seekable = false
    abi::emit_symbol_address(emitter, "r10", "_meta_stype_socket");             // address of the "tcp_socket" literal
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save the stream_type pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], 10");                        // save the stream_type length
    emitter.instruction("jmp __rt_sgmd_seek_done_x86");                         // skip the seekable branch

    emitter.label("__rt_sgmd_seekable_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // seekable = true
    abi::emit_symbol_address(emitter, "r10", "_meta_stype_stdio");              // address of the "STDIO" literal
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save the stream_type pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], 5");                         // save the stream_type length
    emitter.label("__rt_sgmd_seek_done_x86");

    // -- blocking mode + access mode: fcntl(fd, F_GETFL, 0) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the stream descriptor
    emitter.instruction("mov esi, 3");                                          // F_GETFL
    emitter.instruction("xor edx, edx");                                        // unused third argument
    emitter.instruction("mov eax, 72");                                         // Linux x86_64 syscall 72 = fcntl
    emitter.instruction("syscall");                                             // read the descriptor flags
    emitter.instruction(&format!("mov r9d, {}", nonblock));                     // the O_NONBLOCK flag bit
    emitter.instruction("test rax, r9");                                        // is the O_NONBLOCK bit set?
    emitter.instruction("sete r10b");                                           // blocked = 1 when O_NONBLOCK is clear
    emitter.instruction("movzx r10, r10b");                                     // widen the blocked flag to a full word
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the blocked flag
    emitter.instruction("and rax, 3");                                          // isolate the O_ACCMODE access bits
    emitter.instruction("cmp rax, 1");                                          // O_WRONLY?
    emitter.instruction("je __rt_sgmd_mode_w_x86");                             // write-only stream
    emitter.instruction("cmp rax, 2");                                          // O_RDWR?
    emitter.instruction("je __rt_sgmd_mode_rw_x86");                            // read-write stream

    abi::emit_symbol_address(emitter, "r10", "_meta_mode_r");                   // address of the "r" literal
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the mode pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // save the mode length
    emitter.instruction("jmp __rt_sgmd_mode_done_x86");                         // mode resolved
    emitter.label("__rt_sgmd_mode_w_x86");
    abi::emit_symbol_address(emitter, "r10", "_meta_mode_w");                   // address of the "w" literal
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the mode pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // save the mode length
    emitter.instruction("jmp __rt_sgmd_mode_done_x86");                         // mode resolved
    emitter.label("__rt_sgmd_mode_rw_x86");
    abi::emit_symbol_address(emitter, "r10", "_meta_mode_rw");                  // address of the "r+" literal
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the mode pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 2");                         // save the mode length
    emitter.label("__rt_sgmd_mode_done_x86");

    // -- end-of-file flag from the _eof_flags table --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the stream descriptor
    abi::emit_symbol_address(emitter, "r10", "_eof_flags");                     // address of the EOF flag table
    emitter.instruction("movzx r11, BYTE PTR [r10 + rdi]");                     // load _eof_flags[fd]
    emitter.instruction("test r11, r11");                                       // has end-of-file been observed?
    emitter.instruction("setne r11b");                                          // eof = 1 when the flag byte is set
    emitter.instruction("movzx r11, r11b");                                     // widen the eof flag to a full word
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the eof flag

    // -- create the metadata hash (capacity 16, value type = mixed) --
    emitter.instruction("mov rdi, 16");                                         // initial capacity
    emitter.instruction("mov rsi, 7");                                          // value type = mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the hash; rax = hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the hash pointer

    emit_set_bool_const_x86(emitter, "_meta_key_timed_out", 9, 0);
    emit_set_bool_slot_x86(emitter, "_meta_key_blocked", 7, 32);
    emit_set_bool_slot_x86(emitter, "_meta_key_eof", 3, 40);
    emit_set_int_const_x86(emitter, "_meta_key_unread_bytes", 12);
    emit_set_str_slots_x86(emitter, "_meta_key_stream_type", 11, 64, 72);
    emit_set_str_const_x86(emitter, "_meta_key_wrapper_type", 12, "_meta_wrapper_plainfile", 9);
    emit_set_str_slots_x86(emitter, "_meta_key_mode", 4, 48, 56);
    emit_set_bool_slot_x86(emitter, "_meta_key_seekable", 8, 24);
    emit_set_str_const_x86(emitter, "_meta_key_uri", 3, "_meta_wrapper_plainfile", 0);

    // -- return the completed hash --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the final hash pointer
    emitter.instruction("add rsp, 80");                                         // release the metadata spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the metadata hash pointer
}

/// Emit one `__rt_hash_set` with the value already staged in rcx/r8/r9.
fn emit_hash_put_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64) {
    abi::emit_symbol_address(emitter, "rsi", key_sym);                          // key pointer
    emitter.instruction(&format!("mov rdx, {}", key_len));                      // key length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // hash pointer (first argument)
    emitter.instruction("call __rt_hash_set");                                  // insert the entry; rax = updated hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // persist any post-grow hash pointer
}

/// Emits the set bool const x86 stream runtime helper.
fn emit_set_bool_const_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64, value: i64) {
    emitter.instruction(&format!("mov rcx, {}", value));                        // value_lo = boolean payload
    emitter.instruction("xor r8d, r8d");                                        // value_hi unused for booleans
    emitter.instruction("mov r9, 3");                                           // value tag = bool
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Emits the set bool slot x86 stream runtime helper.
fn emit_set_bool_slot_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64, slot: i64) {
    emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {}]", slot));       // value_lo = computed boolean
    emitter.instruction("xor r8d, r8d");                                        // value_hi unused for booleans
    emitter.instruction("mov r9, 3");                                           // value tag = bool
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Emits the set int const x86 stream runtime helper.
fn emit_set_int_const_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64) {
    emitter.instruction("xor ecx, ecx");                                        // value_lo = 0 (elephc keeps no read buffer)
    emitter.instruction("xor r8d, r8d");                                        // value_hi unused for integers
    emitter.instruction("xor r9d, r9d");                                        // value tag = int
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Emits the set str const x86 stream runtime helper.
fn emit_set_str_const_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64, val_sym: &str, val_len: i64) {
    abi::emit_symbol_address(emitter, "rcx", val_sym);                          // value_lo = string pointer
    emitter.instruction(&format!("mov r8, {}", val_len));                       // value_hi = string length
    emitter.instruction("mov r9, 1");                                           // value tag = string
    emit_hash_put_x86(emitter, key_sym, key_len);
}

/// Emits the set str slots x86 stream runtime helper.
fn emit_set_str_slots_x86(emitter: &mut Emitter, key_sym: &str, key_len: i64, ptr_slot: i64, len_slot: i64) {
    emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {}]", ptr_slot));   // value_lo = string pointer
    emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {}]", len_slot));    // value_hi = string length
    emitter.instruction("mov r9, 1");                                           // value tag = string
    emit_hash_put_x86(emitter, key_sym, key_len);
}
