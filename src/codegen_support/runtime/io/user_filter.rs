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
use elephc_memoir::buffers::{stream_scratch, stream_tables};

mod attach;
pub use attach::emit_stream_filter_attach_user;

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

    emitter.instruction("mov x5, #0");                                          // filter slot index
    emitter.label("__rt_sfr_scan");
    emitter.instruction(&format!("cmp x5, #{}", stream_tables::FILTER_REGISTRATION_CAPACITY));  // is the filter registry full?
    emitter.instruction("b.ge __rt_sfr_full");                                  // no empty slot remains
    stream_tables::emit_registration_entry_address(stream_tables::USER_FILTER_REGISTRY, emitter, "x6", "x5", "x4");
    stream_tables::emit_load_registration_field(emitter, "x7", "x6", stream_tables::RegistrationField::NamePointer);
    emitter.instruction("cbz x7, __rt_sfr_store");                              // a null pointer marks an empty slot
    emitter.instruction("add x5, x5, #1");                                      // advance the slot index
    emitter.instruction("b __rt_sfr_scan");                                     // continue scanning

    emitter.label("__rt_sfr_store");
    elephc_memoir::strings::owner_pair::emit_register_row(emitter);
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

    emitter.instruction("xor r9, r9");                                          // filter slot index
    emitter.label("__rt_sfr_scan_x86");
    emitter.instruction(&format!("cmp r9, {}", stream_tables::FILTER_REGISTRATION_CAPACITY)); // is the filter registry full?
    emitter.instruction("jge __rt_sfr_full_x86");                               // no empty slot remains
    stream_tables::emit_registration_entry_address(stream_tables::USER_FILTER_REGISTRY, emitter, "r10", "r9", "r8");
    stream_tables::emit_load_registration_field(emitter, "r11", "r10", stream_tables::RegistrationField::NamePointer);
    emitter.instruction("test r11, r11");                                       // a null pointer marks an empty slot
    emitter.instruction("jz __rt_sfr_store_x86");                               // store here
    emitter.instruction("inc r9");                                              // advance the slot index
    emitter.instruction("jmp __rt_sfr_scan_x86");                               // continue scanning

    emitter.label("__rt_sfr_store_x86");
    elephc_memoir::strings::owner_pair::emit_register_row_with_live_second_pair(emitter);
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

    emitter.instruction("mov x5, #0");                                          // filter slot index
    emitter.label("__rt_rufi_scan");
    emitter.instruction(&format!("cmp x5, #{}", stream_tables::FILTER_REGISTRATION_CAPACITY));  // scanned every registry slot?
    emitter.instruction("b.ge __rt_rufi_miss");                                 // no match found
    stream_tables::emit_registration_entry_address(stream_tables::USER_FILTER_REGISTRY, emitter, "x6", "x5", "x4");
    stream_tables::emit_load_registration_field(emitter, "x7", "x6", stream_tables::RegistrationField::NamePointer);
    emitter.instruction("cbz x7, __rt_rufi_next");                              // skip empty slots
    stream_tables::emit_load_registration_field(emitter, "x8", "x6", stream_tables::RegistrationField::NameLength);
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
/// Output: x1/x2 (rax/rdx on x86_64) hold an independent post-filter string.
///         Simple callbacks transfer owned returns and persist borrowed returns.
///         The brigade path returns its non-heap output scratch buffer.
pub fn emit_apply_user_stream_filter(emitter: &mut Emitter) {
    stream_scratch::emit_acquire_filter_input(emitter);
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
    stream_tables::emit_load_filter_instance(emitter, "x0", "x4", "x5");
    emitter.instruction("cbz x0, __rt_aufs_passthrough");                       // no instance attached → pass the buffer through unchanged

    // -- look up the filter() method pointer in the class's user-filter vtable --
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x6", "x0");
    abi::emit_symbol_address(emitter, "x7", "_user_filter_vtable_ptrs");
    emitter.instruction("ldr x7, [x7, x6, lsl #3]");                            // per-class user-filter vtable
    stream_tables::emit_load_user_filter_vtable_field(
        emitter,
        "x8",
        "x7",
        stream_tables::UserFilterVtableField::Filter,
    );
    emitter.instruction("cbz x8, __rt_aufs_passthrough");                       // class did not implement filter() → pass through

    // -- check slot 3 (arity flag). 1 = PHP-canonical 4-arg brigade dispatch.
    stream_tables::emit_load_user_filter_vtable_field(
        emitter,
        "x9",
        "x7",
        stream_tables::UserFilterVtableField::BrigadeArity,
    );
    emitter.instruction("cbz x9, __rt_aufs_simple");                            // flag=0 → fall through to the existing simple-string path
    emitter.instruction("mov x3, x8");                                          // method_ptr → 4th arg of brigade_invoke
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore caller frame before tail-call
    emitter.instruction("add sp, sp, #16");                                     // release this helper's frame
    emitter.instruction("b __rt_user_filter_brigade_invoke");                   // tail-call the brigade dispatcher with (x0=obj, x1=buf, x2=len, x3=method)

    emitter.label("__rt_aufs_simple");
    stream_scratch::emit_stage_callback_input(emitter, "x8");

    // -- call filter($this=x0, string=x1/x2) → returns string in x1/x2 --
    emitter.instruction("blr x8");                                              // invoke the user filter method
    abi::emit_branch_if_php_return_owned(emitter, "__rt_aufs_owned");
    abi::emit_call_label(emitter, "__rt_str_persist");
    emitter.label("__rt_aufs_owned");
    stream_scratch::emit_finish_callback_input(emitter);
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the post-filter string in x1/x2

    emitter.label("__rt_aufs_passthrough");
    // An absent callback still returns independently releasable bytes.
    abi::emit_call_label(emitter, "__rt_filter_acquire_input");
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
    stream_tables::emit_load_filter_instance(emitter, "rdi", "r9", "r10");
    emitter.instruction("test rdi, rdi");                                       // any instance attached?
    emitter.instruction("jz __rt_aufs_passthrough_x86");                        // no instance → pass the buffer through unchanged

    // -- look up the filter() method pointer in the class's user-filter vtable --
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r11", "rdi");
    abi::emit_symbol_address(emitter, "r10", "_user_filter_vtable_ptrs");       // load runtime data address
    emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");                  // per-class user-filter vtable
    stream_tables::emit_load_user_filter_vtable_field(
        emitter,
        "r8",
        "r10",
        stream_tables::UserFilterVtableField::Filter,
    );
    emitter.instruction("test r8, r8");                                         // class did not implement filter()?
    emitter.instruction("jz __rt_aufs_passthrough_x86");                        // pass through if so
    // -- slot 3 (arity flag): non-zero means PHP-canonical 4-arg brigade dispatch.
    stream_tables::emit_load_user_filter_vtable_field(
        emitter,
        "r9",
        "r10",
        stream_tables::UserFilterVtableField::BrigadeArity,
    );
    emitter.instruction("test r9, r9");                                         // check whether the runtime value is zero
    emitter.instruction("jz __rt_aufs_simple_x");                               // 0 → fall through to existing simple-string path
    emitter.instruction("mov rcx, r8");                                         // method_ptr → SysV 4th arg of brigade_invoke
    // brigade_invoke takes (rdi=$this, rsi=buf, rdx=len, rcx=method); rdi/rsi/rdx already hold the right values.
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("jmp __rt_user_filter_brigade_invoke");                 // tail-call into the brigade dispatcher

    emitter.label("__rt_aufs_simple_x");
    stream_scratch::emit_stage_callback_input(emitter, "r8");

    // -- call filter($this=rdi, string=rsi/rdx) → returns string in rax/rdx --
    emitter.instruction("call r8");                                             // invoke the user filter method
    abi::emit_branch_if_php_return_owned(emitter, "__rt_aufs_owned_x86");
    abi::emit_call_label(emitter, "__rt_str_persist");
    emitter.label("__rt_aufs_owned_x86");
    stream_scratch::emit_finish_callback_input(emitter);
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the post-filter string in rax/rdx

    emitter.label("__rt_aufs_passthrough_x86");
    // rsi=buf, rdx=len; expose them as the elephc string-result pair.
    emitter.instruction("mov rax, rsi");                                        // copy buf pointer into the string-result ptr register
    // rdx already holds the length in the string-result len register.
    abi::emit_call_label(emitter, "__rt_filter_acquire_input");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the original buf/len unchanged
}

/// Emits the Linux x86_64 stream runtime helper for resolve user filter id.
fn emit_resolve_user_filter_id_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve_user_filter_id ---");
    emitter.label_global("__rt_resolve_user_filter_id");

    emitter.instruction("xor r9, r9");                                          // filter slot index
    emitter.label("__rt_rufi_scan_x86");
    emitter.instruction(&format!("cmp r9, {}", stream_tables::FILTER_REGISTRATION_CAPACITY)); // scanned every registry slot?
    emitter.instruction("jge __rt_rufi_miss_x86");                              // no match found
    stream_tables::emit_registration_entry_address(stream_tables::USER_FILTER_REGISTRY, emitter, "r10", "r9", "r8");
    stream_tables::emit_load_registration_field(emitter, "rax", "r10", stream_tables::RegistrationField::NamePointer);
    emitter.instruction("test rax, rax");                                       // is this slot empty?
    emitter.instruction("jz __rt_rufi_next_x86");                               // skip empty slots
    stream_tables::emit_load_registration_field(emitter, "r11", "r10", stream_tables::RegistrationField::NameLength);
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
