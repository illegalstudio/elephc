//! Purpose:
//! Emits the user-defined stream-filter dispatch helpers
//! (`__rt_stream_filter_register`, `__rt_resolve_user_filter_id`,
//! `__rt_apply_user_stream_filter`) and the per-class lookup glue that
//! `stream_filter_register`, `stream_filter_append`/`prepend`, and
//! `__rt_apply_stream_filter` invoke.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `stream_filter_register` / `stream_filter_append` /
//!   `stream_filter_prepend` builtins; the read/write filter dispatch
//!   inside `__rt_apply_stream_filter` for IDs >= USER_FILTER_ID_BASE.
//!
//! Key details:
//! - `_user_filter_registry` capacity = 128 entries × 32 bytes each.
//!   Empty slot has filter_name_ptr == 0.
//! - User filter IDs are `USER_FILTER_ID_BASE + slot_index` so they
//!   don't collide with the existing built-in IDs (1..=4). The byte is
//!   stored in the per-fd `_stream_read_filters[fd]` /
//!   `_stream_write_filters[fd]` tables exactly like built-ins.
//! - User filters may use the simplified `filter(string $data): string`
//!   contract or PHP's four-argument bucket-brigade form. Optional lifecycle
//!   hooks are honored, and `$this->params` is seeded before `onCreate()`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

mod attach;
pub use attach::emit_stream_filter_attach_user;

const USER_FILTER_REGISTRATIONS_CAP: u32 = 128;

/// `__rt_stream_filter_register`: scan `_user_filter_registry` for the
/// first empty slot, store the (filter_name, class_name) tuple, and
/// return 1. Returns 0 when the table is full.
///
/// Input:  AArch64 x0=name_ptr x1=name_len x2=class_ptr x3=class_len.
///         x86_64  rdi=name_ptr rsi=name_len rdx=class_ptr rcx=class_len.
/// Output: 1 = registered, 0 = table full.
///
/// Both strings are persisted with `__rt_str_persist` before being stored: the
/// registration outlives the call, so keeping the caller's pointer means the
/// registered filter name silently follows whatever that buffer holds later.
/// A literal argument hides this — it lives in rodata and never moves.
pub fn emit_stream_filter_register(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_filter_register_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_filter_register ---");
    emitter.label_global("__rt_stream_filter_register");

    // Frame: [sp,#0..16] x29/x30, [sp,#16] free slot base, [sp,#24] class-name
    //   pointer, [sp,#32] class-name length.
    emitter.instruction("sub sp, sp, #48");                                     // frame for the two string-persist calls
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    abi::emit_symbol_address(emitter, "x4", "_user_filter_registry");
    emitter.instruction("mov x5, #0");                                          // filter slot index
    emitter.label("__rt_sfr_scan");
    emitter.instruction(&format!("cmp x5, #{USER_FILTER_REGISTRATIONS_CAP}"));  // is the filter registry full?
    emitter.instruction("b.ge __rt_sfr_full");                                  // no empty slot remains
    emitter.instruction("add x6, x4, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x7, [x6]");                                        // load the slot's filter-name pointer
    emitter.instruction("cbz x7, __rt_sfr_store");                              // a null pointer marks an empty slot
    emitter.instruction("add x5, x5, #1");                                      // advance the slot index
    emitter.instruction("b __rt_sfr_scan");                                     // continue scanning

    emitter.label("__rt_sfr_store");
    emitter.instruction("str x6, [sp, #16]");                                   // save the free slot base across the persist calls
    emitter.instruction("str x2, [sp, #24]");                                   // save the class-name pointer across the filter-name persist
    emitter.instruction("str x3, [sp, #32]");                                   // save the class-name length across the filter-name persist
    emitter.instruction("mov x2, x1");                                          // filter-name length → persist length input
    emitter.instruction("mov x1, x0");                                          // filter-name pointer → persist pointer input
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned filter name, x2 = length
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the free slot base
    emitter.instruction("str x1, [x6]");                                        // owned filter-name pointer
    emitter.instruction("str x2, [x6, #8]");                                    // filter-name length
    emitter.instruction("ldr x1, [sp, #24]");                                   // class-name pointer → persist pointer input
    emitter.instruction("ldr x2, [sp, #32]");                                   // class-name length → persist length input
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned class name, x2 = length
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the free slot base
    emitter.instruction("str x1, [x6, #16]");                                   // owned class-name pointer
    emitter.instruction("str x2, [x6, #24]");                                   // class-name length
    emitter.instruction("mov x0, #1");                                          // return true for a successful registration
    emitter.instruction("b __rt_sfr_ret");                                      // share the common epilogue

    emitter.label("__rt_sfr_full");
    emitter.instruction("mov x0, #0");                                          // return false when the registry is full

    emitter.label("__rt_sfr_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream filter register.
fn emit_stream_filter_register_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_filter_register ---");
    emitter.label_global("__rt_stream_filter_register");

    // Frame: [rbp-8] free slot base, [rbp-16] class-name pointer, [rbp-24]
    //   class-name length. push rbp then sub rsp,48 keeps rsp 16-aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // frame for the two string-persist calls

    abi::emit_symbol_address(emitter, "r8", "_user_filter_registry");           // registry base
    emitter.instruction("xor r9, r9");                                          // filter slot index
    emitter.label("__rt_sfr_scan_x86");
    emitter.instruction(&format!("cmp r9, {}", USER_FILTER_REGISTRATIONS_CAP)); // is the filter registry full?
    emitter.instruction("jge __rt_sfr_full_x86");                               // no empty slot remains
    emitter.instruction("mov r10, r9");                                         // copy the slot index for scaling
    emitter.instruction("shl r10, 5");                                          // slot offset = index * 32
    emitter.instruction("add r10, r8");                                         // slot base = table + offset
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the slot's filter-name pointer
    emitter.instruction("test r11, r11");                                       // a null pointer marks an empty slot
    emitter.instruction("jz __rt_sfr_store_x86");                               // store here
    emitter.instruction("inc r9");                                              // advance the slot index
    emitter.instruction("jmp __rt_sfr_scan_x86");                               // continue scanning

    emitter.label("__rt_sfr_store_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the free slot base across the persist calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the class-name pointer across the filter-name persist
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the class-name length across the filter-name persist
    emitter.instruction("mov rax, rdi");                                        // filter-name pointer → persist pointer input
    emitter.instruction("mov rdx, rsi");                                        // filter-name length → persist length input
    emitter.instruction("call __rt_str_persist");                               // rax = owned filter name, rdx = length
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the free slot base
    emitter.instruction("mov QWORD PTR [r10], rax");                            // owned filter-name pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // filter-name length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // class-name pointer → persist pointer input
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // class-name length → persist length input
    emitter.instruction("call __rt_str_persist");                               // rax = owned class name, rdx = length
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the free slot base
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // owned class-name pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rdx");                       // class-name length
    emitter.instruction("mov eax, 1");                                          // return true for a successful registration
    emitter.instruction("jmp __rt_sfr_ret_x86");                                // share the common epilogue

    emitter.label("__rt_sfr_full_x86");
    emitter.instruction("xor eax, eax");                                        // return false when the registry is full

    emitter.label("__rt_sfr_ret_x86");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// `__rt_resolve_user_filter_id`: scan `_user_filter_registry` for an
/// entry whose filter-name matches the input string byte-for-byte. On
/// match return `USER_FILTER_ID_BASE + slot_index` (an 8-bit value that
/// fits in the per-fd `_stream_*_filters[fd]` byte). On miss return 0
/// — the caller treats 0 as "unknown filter name" and lets the PHP-side
/// `stream_filter_append`/`prepend` return false.
///
/// Input:  AArch64 x0=name_ptr x1=name_len.
///         x86_64  rdi=name_ptr rsi=name_len.
/// Output: u8 ID (128..=255) on match, 0 on miss.
pub fn emit_resolve_user_filter_id(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resolve_user_filter_id_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: resolve_user_filter_id ---");
    emitter.label_global("__rt_resolve_user_filter_id");

    abi::emit_symbol_address(emitter, "x4", "_user_filter_registry");
    emitter.instruction("mov x5, #0");                                          // filter slot index
    emitter.label("__rt_rufi_scan");
    emitter.instruction(&format!("cmp x5, #{USER_FILTER_REGISTRATIONS_CAP}"));  // scanned every registry slot?
    emitter.instruction("b.ge __rt_rufi_miss");                                 // no match found
    emitter.instruction("add x6, x4, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x7, [x6]");                                        // stored filter-name pointer
    emitter.instruction("cbz x7, __rt_rufi_next");                              // skip empty slots
    emitter.instruction("ldr x8, [x6, #8]");                                    // stored filter-name length
    emitter.instruction("cmp x8, x1");                                          // do the lengths match?
    emitter.instruction("b.ne __rt_rufi_next");                                 // length mismatch — try the next slot

    // -- lengths match: compare the bytes --
    emitter.instruction("mov x9, #0");                                          // byte compare index
    emitter.label("__rt_rufi_cmp");
    emitter.instruction("cmp x9, x1");                                          // compared every byte?
    emitter.instruction("b.ge __rt_rufi_match");                                // bytes fully match
    emitter.instruction("ldrb w10, [x7, x9]");                                  // stored byte
    emitter.instruction("ldrb w11, [x0, x9]");                                  // input byte
    emitter.instruction("cmp w10, w11");                                        // do the bytes differ?
    emitter.instruction("b.ne __rt_rufi_next");                                 // mismatch — try the next slot
    emitter.instruction("add x9, x9, #1");                                      // advance the byte cursor
    emitter.instruction("b __rt_rufi_cmp");                                     // continue comparing

    emitter.label("__rt_rufi_match");
    emitter.instruction("add x0, x5, #128");                                    // return USER_FILTER_ID_BASE + slot_index
    emitter.instruction("ret");                                                 // return the resolved id

    emitter.label("__rt_rufi_next");
    emitter.instruction("add x5, x5, #1");                                      // advance the slot index
    emitter.instruction("b __rt_rufi_scan");                                    // continue scanning

    emitter.label("__rt_rufi_miss");
    emitter.instruction("mov x0, #0");                                          // return 0 for unknown filter name
    emitter.instruction("ret");                                                 // return to the caller
}

/// `__rt_apply_user_stream_filter`: dispatch helper invoked from
/// `__rt_apply_stream_filter` when the per-fd filter id is in the user
/// range (>= 128). Resolves the cached instance from
/// `_user_filter_instances[fd*2 + dir]`, locates the class's filter
/// method through `_user_filter_vtable_<class_id>` slot 0, and invokes
/// `filter($this, string)` via the standard elephc method ABI.
///
/// Input:  AArch64 x0=fd x1=buf_ptr x2=buf_len x3=dir (0=read, 1=write).
///         x86_64  rdi=fd rsi=buf_ptr rdx=buf_len rcx=dir.
/// Output: x1/x2 (rax/rdx on x86_64) hold the post-filter string — the
///         same pair the method returned. When the instance or method
///         is absent the helper passes through `(buf_ptr, buf_len)`
///         unchanged so the caller's write/read sees the raw buffer.
pub fn emit_apply_user_stream_filter(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_apply_user_stream_filter_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: apply_user_stream_filter ---");
    emitter.label_global("__rt_apply_user_stream_filter");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- look up the cached instance: _user_filter_instances[fd*2 + dir] --
    emitter.instruction("add x4, x0, x0");                                      // fd*2
    emitter.instruction("add x4, x4, x3");                                      // fd*2 + dir
    abi::emit_symbol_address(emitter, "x5", "_user_filter_instances");
    emitter.instruction("ldr x0, [x5, x4, lsl #3]");                            // obj = instances[slot] (loaded into the method's $this register)
    emitter.instruction("cbz x0, __rt_aufs_passthrough");                       // no instance attached → pass the buffer through unchanged

    // -- look up the filter() method pointer in the class's user-filter vtable --
    emitter.instruction("ldr x6, [x0]");                                        // class_id at the head of the obj
    abi::emit_symbol_address(emitter, "x7", "_user_filter_vtable_ptrs");
    emitter.instruction("ldr x7, [x7, x6, lsl #3]");                            // per-class user-filter vtable
    emitter.instruction("ldr x8, [x7]");                                        // slot 0 = filter() method pointer
    emitter.instruction("cbz x8, __rt_aufs_passthrough");                       // class did not implement filter() → pass through

    // -- check slot 3 (arity flag). 1 = PHP-canonical 4-arg brigade dispatch.
    emitter.instruction("ldr x9, [x7, #24]");                                   // slot 3 = brigade-arity flag (0 = simple-string, 1 = brigade)
    emitter.instruction("cbz x9, __rt_aufs_simple");                            // flag=0 → fall through to the existing simple-string path
    emitter.instruction("mov x3, x8");                                          // method_ptr → 4th arg of brigade_invoke
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore caller frame before tail-call
    emitter.instruction("add sp, sp, #16");                                     // release this helper's frame
    emitter.instruction("b __rt_user_filter_brigade_invoke");                   // tail-call the brigade dispatcher with (x0=obj, x1=buf, x2=len, x3=method)

    emitter.label("__rt_aufs_simple");
    // -- copy the input bytes into _stream_filter_buf so the method's concat_buf
    //    writes don't overlap with its input (the fread path's bytes live in
    //    concat_buf, and the method's prologue typically rewinds _concat_off). --
    abi::emit_symbol_address(emitter, "x9", "_stream_filter_buf");
    emitter.instruction("mov x10, #0");                                         // byte copy index
    emitter.label("__rt_aufs_copy");
    emitter.instruction("cmp x10, x2");                                         // copied every byte?
    emitter.instruction("b.ge __rt_aufs_copy_done");                            // full input copied
    emitter.instruction("ldrb w11, [x1, x10]");                                 // load a source byte
    emitter.instruction("strb w11, [x9, x10]");                                 // store into the isolated scratch buffer
    emitter.instruction("add x10, x10, #1");                                    // advance the copy index
    emitter.instruction("b __rt_aufs_copy");                                    // continue copying
    emitter.label("__rt_aufs_copy_done");
    emitter.instruction("mov x1, x9");                                          // method receives the scratch-buffer pointer as its $data ptr

    // -- call filter($this=x0, string=x1/x2) → returns string in x1/x2 --
    emitter.instruction("blr x8");                                              // invoke the user filter method
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the post-filter string in x1/x2

    emitter.label("__rt_aufs_passthrough");
    // x1/x2 already hold the original buf/len; nothing to do.
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the original buf/len unchanged
}

/// Emits the Linux x86_64 stream runtime helper for apply user stream filter.
fn emit_apply_user_stream_filter_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: apply_user_stream_filter ---");
    emitter.label_global("__rt_apply_user_stream_filter");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // -- look up the cached instance: _user_filter_instances[fd*2 + dir] --
    emitter.instruction("mov r9, rdi");                                         // fd
    emitter.instruction("add r9, r9");                                          // fd*2
    emitter.instruction("add r9, rcx");                                         // fd*2 + dir
    abi::emit_symbol_address(emitter, "r10", "_user_filter_instances");         // load runtime data address
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // obj = instances[slot] (loaded into the method's $this register)
    emitter.instruction("test rdi, rdi");                                       // any instance attached?
    emitter.instruction("jz __rt_aufs_passthrough_x86");                        // no instance → pass the buffer through unchanged

    // -- look up the filter() method pointer in the class's user-filter vtable --
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // class_id at the head of the obj
    abi::emit_symbol_address(emitter, "r10", "_user_filter_vtable_ptrs");       // load runtime data address
    emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");                  // per-class user-filter vtable
    emitter.instruction("mov r8, QWORD PTR [r10]");                             // slot 0 = filter() method pointer
    emitter.instruction("test r8, r8");                                         // class did not implement filter()?
    emitter.instruction("jz __rt_aufs_passthrough_x86");                        // pass through if so
    // -- slot 3 (arity flag): non-zero means PHP-canonical 4-arg brigade dispatch.
    emitter.instruction("mov r9, QWORD PTR [r10 + 24]");                        // slot 3 = brigade arity flag
    emitter.instruction("test r9, r9");                                         // check whether the runtime value is zero
    emitter.instruction("jz __rt_aufs_simple_x");                               // 0 → fall through to existing simple-string path
    emitter.instruction("mov rcx, r8");                                         // method_ptr → SysV 4th arg of brigade_invoke
    // brigade_invoke takes (rdi=$this, rsi=buf, rdx=len, rcx=method); rdi/rsi/rdx already hold the right values.
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("jmp __rt_user_filter_brigade_invoke");                 // tail-call into the brigade dispatcher

    emitter.label("__rt_aufs_simple_x");
    emitter.instruction("mov r10, r8");                                         // existing simple-string path expects the method ptr in r10
    // -- copy the input bytes into _stream_filter_buf so the method's concat_buf
    //    writes don't overlap with its input (fread bytes live in concat_buf and
    //    the method's prologue typically rewinds _concat_off). --
    emitter.instruction("push r10");                                            // save filter method ptr across the copy loop
    emitter.instruction("push rdi");                                            // save $this across the copy loop
    abi::emit_symbol_address(emitter, "r10", "_stream_filter_buf");             // load runtime data address
    emitter.instruction("xor r9, r9");                                          // byte copy index
    emitter.label("__rt_aufs_copy_x86");
    emitter.instruction("cmp r9, rdx");                                         // copied every byte?
    emitter.instruction("jge __rt_aufs_copy_done_x86");                         // full input copied
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r9]");                     // load a source byte
    emitter.instruction("mov BYTE PTR [r10 + r9], r11b");                       // store into the isolated scratch buffer
    emitter.instruction("inc r9");                                              // advance the copy index
    emitter.instruction("jmp __rt_aufs_copy_x86");                              // continue copying
    emitter.label("__rt_aufs_copy_done_x86");
    emitter.instruction("mov rsi, r10");                                        // method receives the scratch-buffer pointer as its $data ptr
    emitter.instruction("pop rdi");                                             // restore $this
    emitter.instruction("pop r10");                                             // restore filter method ptr

    // -- call filter($this=rdi, string=rsi/rdx) → returns string in rax/rdx --
    emitter.instruction("call r10");                                            // invoke the user filter method
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the post-filter string in rax/rdx

    emitter.label("__rt_aufs_passthrough_x86");
    // rsi=buf, rdx=len; expose them as the elephc string-result pair.
    emitter.instruction("mov rax, rsi");                                        // copy buf pointer into the string-result ptr register
    // rdx already holds the length in the string-result len register.
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the original buf/len unchanged
}

/// Emits the Linux x86_64 stream runtime helper for resolve user filter id.
fn emit_resolve_user_filter_id_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve_user_filter_id ---");
    emitter.label_global("__rt_resolve_user_filter_id");

    abi::emit_symbol_address(emitter, "r8", "_user_filter_registry");           // registry base
    emitter.instruction("xor r9, r9");                                          // filter slot index
    emitter.label("__rt_rufi_scan_x86");
    emitter.instruction(&format!("cmp r9, {}", USER_FILTER_REGISTRATIONS_CAP)); // scanned every registry slot?
    emitter.instruction("jge __rt_rufi_miss_x86");                              // no match found
    emitter.instruction("mov r10, r9");                                         // copy the slot index for scaling
    emitter.instruction("shl r10, 5");                                          // slot offset = index * 32
    emitter.instruction("add r10, r8");                                         // slot base = table + offset
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // stored filter-name pointer
    emitter.instruction("test rax, rax");                                       // is this slot empty?
    emitter.instruction("jz __rt_rufi_next_x86");                               // skip empty slots
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // stored filter-name length
    emitter.instruction("cmp r11, rsi");                                        // do the lengths match?
    emitter.instruction("jne __rt_rufi_next_x86");                              // length mismatch — try the next slot

    // -- lengths match: compare the bytes --
    emitter.instruction("xor rcx, rcx");                                        // byte compare index
    emitter.label("__rt_rufi_cmp_x86");
    emitter.instruction("cmp rcx, rsi");                                        // compared every byte?
    emitter.instruction("jge __rt_rufi_match_x86");                             // bytes fully match
    emitter.instruction("movzx edx, BYTE PTR [rax + rcx]");                     // stored byte
    emitter.instruction("movzx r11d, BYTE PTR [rdi + rcx]");                    // input byte
    emitter.instruction("cmp dl, r11b");                                        // do the bytes differ?
    emitter.instruction("jne __rt_rufi_next_x86");                              // mismatch — try the next slot
    emitter.instruction("inc rcx");                                             // advance the byte cursor
    emitter.instruction("jmp __rt_rufi_cmp_x86");                               // continue comparing

    emitter.label("__rt_rufi_match_x86");
    emitter.instruction("lea rax, [r9 + 128]");                                 // return USER_FILTER_ID_BASE + slot_index
    emitter.instruction("ret");                                                 // return the resolved id

    emitter.label("__rt_rufi_next_x86");
    emitter.instruction("inc r9");                                              // advance the slot index
    emitter.instruction("jmp __rt_rufi_scan_x86");                              // continue scanning

    emitter.label("__rt_rufi_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return 0 for unknown filter name
    emitter.instruction("ret");                                                 // return to the caller
}

/// Detaches each unique user-filter instance and runs onClose before retiring its owner.
/// Both direction slots are cleared before callbacks; scoped owners cover callback exceptions.
/// Input: first C argument is the descriptor. Output: integer result preserves that descriptor.
pub fn emit_user_filter_release_fd(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_filter_release_fd ---");
    emitter.label_global("__rt_user_filter_release_fd");
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    detach_filter_instances(emitter);

    // Publish in reverse callback order so each normal retirement removes the top record.
    for offset in [24, 16] {
        let address = abi::tertiary_scratch_reg(emitter);
        abi::emit_frame_slot_address(emitter, address, offset);
        abi::emit_push_call_operand_owner(emitter, address, false);
    }
    for (direction, offset) in [16, 24].into_iter().enumerate() {
        let done = format!("__rt_ufrf_dir{direction}_closed");
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
        abi::emit_branch_if_int_result_zero(emitter, &done);
        emit_filter_on_close(emitter, &done);
        emitter.label(&done);
        abi::emit_pop_call_operand_owner(emitter);
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
        abi::emit_store_zero_to_local_slot(emitter, offset);
        abi::emit_call_label(emitter, "__rt_decref_any");
    }
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), 8);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

/// Moves the registry's two direction owners into frame slots, deduplicating a shared instance.
fn detach_filter_instances(emitter: &mut Emitter) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_user_filter_instances");
            emitter.instruction("add x9, x9, x0, lsl #4");                      // address the descriptor's adjacent read and write slots
            emitter.instruction("ldp x10, x11, [x9]");                          // capture both registry pointers before callbacks can reenter
            emitter.instruction("stp xzr, xzr, [x9]");                          // detach both slots before closing either instance
            emitter.instruction("cmp x10, x11");                                // STREAM_FILTER_ALL stores one owner in two direction slots
            emitter.instruction("csel x11, xzr, x11, eq");                      // keep a shared instance only in the first owning slot
            abi::store_at_offset(emitter, "x10", 16);
            abi::store_at_offset(emitter, "x11", 24);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r9", "_user_filter_instances");
            emitter.instruction("shl rax, 4");                                  // two eight-byte direction slots belong to each descriptor
            emitter.instruction("add r9, rax");                                 // address this descriptor's registry pair
            emitter.instruction("mov r10, QWORD PTR [r9]");                     // preserve the read instance before clearing the registry
            emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                 // preserve the write instance before clearing the registry
            emitter.instruction("mov QWORD PTR [r9], 0");                       // detach the read instance before any PHP callback
            emitter.instruction("mov QWORD PTR [r9 + 8], 0");                   // detach the write instance before any PHP callback
            emitter.instruction("cmp r10, r11");                                // detect the one owner shared by STREAM_FILTER_ALL
            emitter.instruction("jne __rt_ufrf_distinct_instances");            // distinct direction instances each keep their own owner
            emitter.instruction("xor r11d, r11d");                              // retire a shared pointer exactly once
            emitter.label("__rt_ufrf_distinct_instances");
            abi::store_at_offset(emitter, "r10", 16);
            abi::store_at_offset(emitter, "r11", 24);
        }
    }
}

/// Calls a detached instance's optional onClose hook using caller-saved lookup registers.
fn emit_filter_on_close(emitter: &mut Emitter, done: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // read the instance's class id while its stack owner is published
            abi::emit_symbol_address(emitter, "x10", "_user_filter_vtable_ptrs");
            emitter.instruction("ldr x9, [x10, x9, lsl #3]");                   // select the class's stream-filter method table
            emitter.instruction("ldr x9, [x9, #16]");                           // load the optional onClose entry
            emitter.instruction(&format!("cbz x9, {done}"));                    // a missing hook still retires the instance owner
            emitter.instruction("blr x9");                                      // call onClose with the instance in x0
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, QWORD PTR [rax]");                    // read the instance's class id without changing callee-saved registers
            abi::emit_symbol_address(emitter, "r9", "_user_filter_vtable_ptrs");
            emitter.instruction("mov r11, QWORD PTR [r9 + rcx * 8]");           // select the class's stream-filter method table
            emitter.instruction("mov r11, QWORD PTR [r11 + 16]");               // load the optional onClose entry
            emitter.instruction("test r11, r11");                               // a missing hook still retires the instance owner
            emitter.instruction(&format!("jz {done}"));                         // skip only the absent PHP callback
            emitter.instruction("mov rdi, rax");                                // pass the rooted instance as the PHP receiver
            emitter.instruction("call r11");                                    // call onClose while both remaining owners are unwind-visible
        }
    }
}
