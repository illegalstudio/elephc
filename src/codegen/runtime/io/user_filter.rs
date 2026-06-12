//! Purpose:
//! Emits the user-defined stream-filter dispatch helpers
//! (`__rt_stream_filter_register`, `__rt_resolve_user_filter_id`,
//! `__rt_apply_user_stream_filter`) and the per-class lookup glue that
//! `stream_filter_register`, `stream_filter_append`/`prepend`, and
//! `__rt_apply_stream_filter` invoke.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
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

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const USER_FILTER_REGISTRATIONS_CAP: u32 = 128;

/// `__rt_stream_filter_register`: scan `_user_filter_registry` for the
/// first empty slot, store the (filter_name, class_name) tuple, and
/// return 1. Returns 0 when the table is full.
///
/// Input:  AArch64 x0=name_ptr x1=name_len x2=class_ptr x3=class_len.
///         x86_64  rdi=name_ptr rsi=name_len rdx=class_ptr rcx=class_len.
/// Output: 1 = registered, 0 = table full.
pub fn emit_stream_filter_register(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_filter_register_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_filter_register ---");
    emitter.label_global("__rt_stream_filter_register");

    abi::emit_symbol_address(emitter, "x4", "_user_filter_registry");
    emitter.instruction("mov x5, #0");                                          // filter slot index
    emitter.label("__rt_sfr_scan");
    emitter.instruction(&format!("cmp x5, #{}", USER_FILTER_REGISTRATIONS_CAP)); // is the filter registry full?
    emitter.instruction("b.ge __rt_sfr_full");                                  // no empty slot remains
    emitter.instruction("add x6, x4, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x7, [x6]");                                        // load the slot's filter-name pointer
    emitter.instruction("cbz x7, __rt_sfr_store");                              // a null pointer marks an empty slot
    emitter.instruction("add x5, x5, #1");                                      // advance the slot index
    emitter.instruction("b __rt_sfr_scan");                                     // continue scanning

    emitter.label("__rt_sfr_store");
    emitter.instruction("str x0, [x6]");                                        // filter-name pointer
    emitter.instruction("str x1, [x6, #8]");                                    // filter-name length
    emitter.instruction("str x2, [x6, #16]");                                   // class-name pointer
    emitter.instruction("str x3, [x6, #24]");                                   // class-name length
    emitter.instruction("mov x0, #1");                                          // return true for a successful registration
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_sfr_full");
    emitter.instruction("mov x0, #0");                                          // return false when the registry is full
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream filter register.
fn emit_stream_filter_register_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_filter_register ---");
    emitter.label_global("__rt_stream_filter_register");

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
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // filter-name pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], rsi");                        // filter-name length
    emitter.instruction("mov QWORD PTR [r10 + 16], rdx");                       // class-name pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rcx");                       // class-name length
    emitter.instruction("mov eax, 1");                                          // return true for a successful registration
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_sfr_full_x86");
    emitter.instruction("xor eax, eax");                                        // return false when the registry is full
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
    emitter.instruction(&format!("cmp x5, #{}", USER_FILTER_REGISTRATIONS_CAP)); // scanned every registry slot?
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

/// `__rt_stream_filter_attach_user`: end-to-end attach helper for a user
/// filter. Resolves the filter name into an id, instantiates the
/// associated class via `__rt_new_by_name`, caches the instance in
/// `_user_filter_instances[fd*2 + dir]`, and records the id byte in
/// `_stream_read_filters[fd]` / `_stream_write_filters[fd]` according to
/// the requested mode (STREAM_FILTER_READ=1, WRITE=2, ALL=3).
///
/// Input:  AArch64 x0=fd x1=name_ptr x2=name_len x3=mode x4=params_mixed.
///         x86_64  rdi=fd rsi=name_ptr rdx=name_len rcx=mode r8=params_mixed.
/// Output: 1 on success, 0 on unknown filter name or instantiation
///         failure. The instances / per-fd tables are only mutated on
///         success.
pub fn emit_stream_filter_attach_user(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_filter_attach_user_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_filter_attach_user ---");
    emitter.label_global("__rt_stream_filter_attach_user");

    // Frame (64 bytes):
    //   [sp, #0]  fd
    //   [sp, #8]  mode
    //   [sp, #16] params Mixed*
    //   [sp, #24] resolved id
    //   [sp, #32] obj_ptr stash (success path only)
    //   [sp, #48] saved x29
    //   [sp, #56] saved x30
    emitter.instruction("sub sp, sp, #64");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save fd
    emitter.instruction("str x3, [sp, #8]");                                    // save mode
    emitter.instruction("str x4, [sp, #16]");                                   // save boxed filter params

    // -- resolve filter name → id --
    emitter.instruction("mov x0, x1");                                          // move name_ptr into the resolver's first arg
    emitter.instruction("mov x1, x2");                                          // move name_len into the resolver's second arg
    emitter.instruction("bl __rt_resolve_user_filter_id");                      // x0 = id (>=128) or 0
    emitter.instruction("cbz x0, __rt_sfau_fail_release_params");               // unknown filter name → fail and release params
    emitter.instruction("str x0, [sp, #24]");                                   // save the resolved id across __rt_new_by_name

    // -- look up class_name in the registry slot --
    emitter.instruction("sub x9, x0, #128");                                    // slot_index = id - USER_FILTER_ID_BASE
    abi::emit_symbol_address(emitter, "x10", "_user_filter_registry");
    emitter.instruction("add x10, x10, x9, lsl #5");                            // registry entry = base + slot_index * 32
    emitter.instruction("ldr x1, [x10, #16]");                                  // class_name pointer
    emitter.instruction("ldr x2, [x10, #24]");                                  // class_name length
    emitter.instruction("bl __rt_new_by_name");                                 // x0 = obj or 0
    emitter.instruction("cbz x0, __rt_sfau_fail_release_params");               // class missing → fail and release params
    emitter.instruction("str x0, [sp, #32]");                                   // save the obj pointer for the instances table

    // -- seed php_user_filter::$params before onCreate() observes $this --
    emitter.instruction("ldr x6, [x0]");                                        // class_id at the head of the obj
    abi::emit_symbol_address(emitter, "x7", "_user_filter_vtable_ptrs");
    emitter.instruction("ldr x7, [x7, x6, lsl #3]");                            // per-class user-filter vtable
    emitter.instruction("ldr x9, [x7, #32]");                                   // slot 4 = params property byte offset
    emitter.instruction("cbz x9, __rt_sfau_params_no_slot");                    // no params slot → release the unused boxed value
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload boxed params for transfer into the object
    emitter.instruction("str x10, [x0, x9]");                                   // store params in the inherited public property slot
    emitter.instruction("add x11, x0, x9");                                     // compute the params property slot address
    emitter.instruction("str xzr, [x11, #8]");                                  // clear the high word for the Mixed pointer slot
    emitter.instruction("b __rt_sfau_params_done");                             // skip unused-params release
    emitter.label("__rt_sfau_params_no_slot");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload boxed params that no object slot will own
    emitter.instruction("bl __rt_decref_any");                                  // release params when the filter class lacks the slot
    emitter.instruction("ldr x0, [sp, #32]");                                   // restore obj pointer after the release call
    emitter.label("__rt_sfau_params_done");

    // -- onCreate() lifecycle hook (vtable slot 1). When present, the
    //    user's filter class can refuse the attach by returning false.
    //    If the method is absent in the vtable, the slot is null and we
    //    fall through. --
    emitter.instruction("ldr x6, [x0]");                                        // class_id at the head of the obj
    abi::emit_symbol_address(emitter, "x7", "_user_filter_vtable_ptrs");
    emitter.instruction("ldr x7, [x7, x6, lsl #3]");                            // per-class user-filter vtable
    emitter.instruction("ldr x8, [x7, #8]");                                    // slot 1 = onCreate() method pointer
    emitter.instruction("cbz x8, __rt_sfau_oncreate_skip");                     // method absent → skip the hook
    emitter.instruction("blr x8");                                              // call onCreate($this) → x0 = bool
    emitter.instruction("cbnz x0, __rt_sfau_oncreate_skip");                    // truthy result → proceed with the attach
    // onCreate returned false → release the just-created instance and
    // bail. The obj had refcount 1 from __rt_new_by_name; decref drops
    // it back to zero and frees the object.
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload obj pointer for decref
    emitter.instruction("bl __rt_decref_any");                                  // release the rejected instance
    emitter.instruction("b __rt_sfau_fail");                                    // continue at target label
    emitter.label("__rt_sfau_oncreate_skip");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload obj — onCreate's return value (or absence) replaced it

    // -- record state per direction bit --
    emitter.instruction("ldr x4, [sp, #0]");                                    // reload fd
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload mode
    emitter.instruction("ldr x6, [sp, #24]");                                   // reload resolved id (u8 in the low byte)
    abi::emit_symbol_address(emitter, "x10", "_user_filter_instances");

    // mode & 1 → read direction
    emitter.instruction("tst x5, #1");                                          // STREAM_FILTER_READ bit set?
    emitter.instruction("b.eq __rt_sfau_skip_read");                            // skip read-direction state otherwise
    emitter.instruction("add x11, x4, x4");                                     // fd*2 — read slot
    emitter.instruction("str x0, [x10, x11, lsl #3]");                          // _user_filter_instances[fd*2] = obj
    abi::emit_symbol_address(emitter, "x12", "_stream_read_filters");
    emitter.instruction("strb w6, [x12, x4]");                                  // _stream_read_filters[fd] = id (low 8 bits)
    emitter.label("__rt_sfau_skip_read");

    // mode & 2 → write direction
    emitter.instruction("tst x5, #2");                                          // STREAM_FILTER_WRITE bit set?
    emitter.instruction("b.eq __rt_sfau_skip_write");                           // skip write-direction state otherwise
    emitter.instruction("add x11, x4, x4");                                     // fd*2
    emitter.instruction("add x11, x11, #1");                                    // fd*2 + 1 — write slot
    emitter.instruction("str x0, [x10, x11, lsl #3]");                          // _user_filter_instances[fd*2 + 1] = obj
    abi::emit_symbol_address(emitter, "x12", "_stream_write_filters");
    emitter.instruction("strb w6, [x12, x4]");                                  // _stream_write_filters[fd] = id (low 8 bits)
    emitter.label("__rt_sfau_skip_write");

    emitter.instruction("mov x0, #1");                                          // success
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_sfau_fail_release_params");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload boxed params that were never transferred
    emitter.instruction("bl __rt_decref_any");                                  // release params before reporting attach failure
    emitter.label("__rt_sfau_fail");
    emitter.instruction("mov x0, #0");                                          // failure: unknown name or instantiation failed
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream filter attach user.
fn emit_stream_filter_attach_user_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_filter_attach_user ---");
    emitter.label_global("__rt_stream_filter_attach_user");

    // rbp-relative frame:
    //   [rbp - 8]  fd
    //   [rbp - 16] mode
    //   [rbp - 24] params Mixed*
    //   [rbp - 32] resolved id
    //   [rbp - 40] obj_ptr stash
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save fd
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save mode
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // save boxed filter params

    // -- resolve filter name → id --
    emitter.instruction("mov rdi, rsi");                                        // move name_ptr into the resolver's first arg
    emitter.instruction("mov rsi, rdx");                                        // move name_len into the resolver's second arg
    emitter.instruction("call __rt_resolve_user_filter_id");                    // rax = id or 0
    emitter.instruction("test rax, rax");                                       // unknown filter name?
    emitter.instruction("jz __rt_sfau_fail_release_params_x86");                // fail and release params without touching state
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save resolved id

    // -- look up class_name in the registry slot --
    emitter.instruction("mov r9, rax");                                         // copy id into a scratch register
    emitter.instruction("sub r9, 128");                                         // slot_index = id - USER_FILTER_ID_BASE
    emitter.instruction("shl r9, 5");                                           // slot offset = slot_index * 32
    abi::emit_symbol_address(emitter, "r10", "_user_filter_registry");          // registry base
    emitter.instruction("add r10, r9");                                         // entry pointer = base + offset
    emitter.instruction("mov rax, QWORD PTR [r10 + 16]");                       // class_name pointer (loaded into the new_by_name name-ptr reg)
    emitter.instruction("mov rdx, QWORD PTR [r10 + 24]");                       // class_name length
    emitter.instruction("call __rt_new_by_name");                               // rax = obj or 0
    emitter.instruction("test rax, rax");                                       // class missing?
    emitter.instruction("jz __rt_sfau_fail_release_params_x86");                // fail and release params without touching state
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the obj pointer

    // -- seed php_user_filter::$params before onCreate() observes $this --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // class_id at the head of the obj
    abi::emit_symbol_address(emitter, "r11", "_user_filter_vtable_ptrs");       // load runtime data address
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-filter vtable
    emitter.instruction("mov r9, QWORD PTR [r11 + 32]");                        // slot 4 = params property byte offset
    emitter.instruction("test r9, r9");                                         // does the filter class expose params?
    emitter.instruction("jz __rt_sfau_params_no_slot_x86");                     // no params slot → release the unused boxed value
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload boxed params for transfer into the object
    emitter.instruction("mov QWORD PTR [rax + r9], r10");                       // store params in the inherited public property slot
    emitter.instruction("mov QWORD PTR [rax + r9 + 8], 0");                     // clear the high word for the Mixed pointer slot
    emitter.instruction("jmp __rt_sfau_params_done_x86");                       // skip unused-params release
    emitter.label("__rt_sfau_params_no_slot_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload boxed params that no object slot will own
    emitter.instruction("call __rt_decref_any");                                // release params when the filter class lacks the slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore obj pointer after the release call
    emitter.label("__rt_sfau_params_done_x86");

    // -- onCreate() lifecycle hook (vtable slot 1) — see ARM64 path. --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // class_id at the head of the obj
    abi::emit_symbol_address(emitter, "r11", "_user_filter_vtable_ptrs");       // load runtime data address
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-filter vtable
    emitter.instruction("mov r11, QWORD PTR [r11 + 8]");                        // slot 1 = onCreate
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_sfau_oncreate_skip_x86");                      // method absent → skip the hook
    emitter.instruction("mov rdi, rax");                                        // $this in SysV first arg
    emitter.instruction("call r11");                                            // call onCreate($this) → rax = bool
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jnz __rt_sfau_oncreate_skip_x86");                     // truthy result → proceed
    // onCreate returned false → release the obj and bail without
    // touching any registry state.
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload obj pointer
    emitter.instruction("call __rt_decref_any");                                // call runtime helper
    emitter.instruction("jmp __rt_sfau_fail_x86");                              // continue at target label
    emitter.label("__rt_sfau_oncreate_skip_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload obj — onCreate call replaced it

    // -- record state per direction bit --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload fd
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload mode
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload resolved id (u8 in the low byte)
    abi::emit_symbol_address(emitter, "r9", "_user_filter_instances");          // instances table base

    // mode & 1 → read direction
    emitter.instruction("test rsi, 1");                                         // STREAM_FILTER_READ bit set?
    emitter.instruction("jz __rt_sfau_skip_read_x86");                          // skip read-direction state otherwise
    emitter.instruction("mov r10, rdi");                                        // fd
    emitter.instruction("add r10, r10");                                        // fd*2 — read slot
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rax");                   // _user_filter_instances[fd*2] = obj
    abi::emit_symbol_address(emitter, "r11", "_stream_read_filters");           // load runtime data address
    emitter.instruction("mov BYTE PTR [r11 + rdi], dl");                        // _stream_read_filters[fd] = id (low 8 bits)
    emitter.label("__rt_sfau_skip_read_x86");

    // mode & 2 → write direction
    emitter.instruction("test rsi, 2");                                         // STREAM_FILTER_WRITE bit set?
    emitter.instruction("jz __rt_sfau_skip_write_x86");                         // skip write-direction state otherwise
    emitter.instruction("mov r10, rdi");                                        // fd
    emitter.instruction("add r10, r10");                                        // fd*2
    emitter.instruction("add r10, 1");                                          // fd*2 + 1 — write slot
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rax");                   // _user_filter_instances[fd*2 + 1] = obj
    abi::emit_symbol_address(emitter, "r11", "_stream_write_filters");          // load runtime data address
    emitter.instruction("mov BYTE PTR [r11 + rdi], dl");                        // _stream_write_filters[fd] = id (low 8 bits)
    emitter.label("__rt_sfau_skip_write_x86");

    emitter.instruction("mov eax, 1");                                          // success
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_sfau_fail_release_params_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload boxed params that were never transferred
    emitter.instruction("call __rt_decref_any");                                // release params before reporting attach failure
    emitter.label("__rt_sfau_fail_x86");
    emitter.instruction("xor eax, eax");                                        // failure
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
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

/// `__rt_user_filter_release_fd`: for an fd that's about to be closed
/// or filter-removed, run `onClose()` on every attached user-filter
/// instance and clear the per-direction registry. Called by both
/// `fclose()` and `stream_filter_remove()` so they share one code path.
///
/// Input:  AArch64 x0 = fd; x86_64 rdi = fd.
/// Output: nothing (returns to caller). x0/rax preserved.
pub fn emit_user_filter_release_fd(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_filter_release_fd_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_filter_release_fd ---");
    emitter.label_global("__rt_user_filter_release_fd");

    // Frame: 32 bytes — saved fd + x29/x30.
    emitter.instruction("sub sp, sp, #32");                                     // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save fd across the onClose calls

    for dir in 0..2usize {
        let skip = format!("__rt_ufrf_dir{}_skip", dir);
        let no_method = format!("__rt_ufrf_dir{}_no_method", dir);
        emitter.instruction("ldr x0, [sp, #0]");                                // fd
        emitter.instruction("add x10, x0, x0");                                 // fd*2
        if dir == 1 {
            emitter.instruction("add x10, x10, #1");                            // + read/write direction
        }
        abi::emit_symbol_address(emitter, "x9", "_user_filter_instances");
        emitter.instruction("ldr x11, [x9, x10, lsl #3]");                      // obj at instances[slot]
        emitter.instruction(&format!("cbz x11, {}", skip));                     // no instance attached → skip
        // Look up onClose method (vtable slot 2 = offset 16).
        emitter.instruction("ldr x12, [x11]");                                  // class_id at obj head
        abi::emit_symbol_address(emitter, "x13", "_user_filter_vtable_ptrs");
        emitter.instruction("ldr x13, [x13, x12, lsl #3]");                     // per-class vtable
        emitter.instruction("ldr x14, [x13, #16]");                             // slot 2 = onClose method ptr
        emitter.instruction(&format!("cbz x14, {}", no_method));                // method absent → just clear the slot
        emitter.instruction("mov x0, x11");                                     // $this for the call
        emitter.instruction("blr x14");                                         // call onClose($this) — return discarded
        emitter.label(&no_method);
        // Clear the instance slot now that onClose has run (or was absent).
        emitter.instruction("ldr x0, [sp, #0]");                                // reload fd (clobbered by the call)
        emitter.instruction("add x10, x0, x0");                                 // advance runtime pointer or counter
        if dir == 1 {
            emitter.instruction("add x10, x10, #1");                            // advance runtime pointer or counter
        }
        abi::emit_symbol_address(emitter, "x9", "_user_filter_instances");
        emitter.instruction("str xzr, [x9, x10, lsl #3]");                      // store runtime value
        emitter.label(&skip);
    }

    emitter.instruction("ldr x0, [sp, #0]");                                    // restore caller's x0 = fd
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for user filter release fd.
fn emit_user_filter_release_fd_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_filter_release_fd ---");
    emitter.label_global("__rt_user_filter_release_fd");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate runtime stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save fd

    for dir in 0..2usize {
        let skip = format!("__rt_ufrf_dir{}_skip_x86", dir);
        let no_method = format!("__rt_ufrf_dir{}_no_method_x86", dir);
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // fd
        emitter.instruction("mov r10, rdi");                                    // move runtime value between registers
        emitter.instruction("add r10, r10");                                    // fd*2
        if dir == 1 {
            emitter.instruction("add r10, 1");                                  // advance runtime pointer or counter
        }
        abi::emit_symbol_address(emitter, "r9", "_user_filter_instances");      // load runtime data address
        emitter.instruction("mov r11, QWORD PTR [r9 + r10 * 8]");               // obj
        emitter.instruction("test r11, r11");                                   // check whether the runtime value is zero
        emitter.instruction(&format!("jz {}", skip));                           // branch when the checked value is zero or equal
        emitter.instruction("mov r12, QWORD PTR [r11]");                        // class_id
        abi::emit_symbol_address(emitter, "r13", "_user_filter_vtable_ptrs");   // load runtime data address
        emitter.instruction("mov r13, QWORD PTR [r13 + r12 * 8]");              // vtable
        emitter.instruction("mov r14, QWORD PTR [r13 + 16]");                   // slot 2 = onClose
        emitter.instruction("test r14, r14");                                   // check whether the runtime value is zero
        emitter.instruction(&format!("jz {}", no_method));                      // branch when the checked value is zero or equal
        emitter.instruction("mov rdi, r11");                                    // $this
        emitter.instruction("call r14");                                        // call external helper
        emitter.label(&no_method);
        // Clear the slot.
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // prepare SysV call argument
        emitter.instruction("mov r10, rdi");                                    // move runtime value between registers
        emitter.instruction("add r10, r10");                                    // advance runtime pointer or counter
        if dir == 1 {
            emitter.instruction("add r10, 1");                                  // advance runtime pointer or counter
        }
        abi::emit_symbol_address(emitter, "r9", "_user_filter_instances");      // load runtime data address
        emitter.instruction("mov QWORD PTR [r9 + r10 * 8], 0");                 // store runtime value
        emitter.label(&skip);
    }

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // preserve fd
    emitter.instruction("add rsp, 16");                                         // release runtime stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
