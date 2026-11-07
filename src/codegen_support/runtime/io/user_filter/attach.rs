//! Purpose:
//! Transfers user-filter params and instance owners safely through attachment callbacks.
//!
//! Called from:
//! - Stream-filter attach lowering through the shared I/O runtime emitter.
//!
//! Key details:
//! - Params and the new instance have independent exceptional owners until publication.
//! - The params property publishes its replacement before its previous default is retired.
//! - Each selected direction owns its instance; lifecycle cleanup deduplicates callbacks.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::exceptions::guards;

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
///         failure. Provisional roots are published before onCreate: refusal
///         rolls them back, while a throw leaves them owned until stream close.
pub fn emit_stream_filter_attach_user(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_filter_attach_user_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_filter_attach_user ---");
    emitter.label_global("__rt_stream_filter_attach_user");

    // Frame (128 bytes):
    //   [sp, #0]  fd
    //   [sp, #8]  mode
    //   [sp, #16] params Mixed*
    //   [sp, #24] resolved id
    //   [sp, #32] obj_ptr stash (success path only)
    //   [sp, #48..80] params owner guard
    //   [sp, #80..112] object owner guard
    //   [sp, #112] saved x29
    //   [sp, #120] saved x30
    emitter.instruction("sub sp, sp, #128");                                    // helper frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save fd
    emitter.instruction("str x3, [sp, #8]");                                    // save mode
    emitter.instruction("str x4, [sp, #16]");                                   // save boxed filter params

    // -- resolve filter name → id --
    emitter.instruction("mov x0, x1");                                          // move name_ptr into the resolver's first arg
    emitter.instruction("mov x1, x2");                                          // move name_len into the resolver's second arg
    emitter.instruction("bl __rt_resolve_user_filter_id");                      // x0 = id (>=128) or 0
    emitter.instruction("cbz x0, __rt_sfau_fail_release_params");               // unknown filter name → fail and release params
    emitter.instruction("str x0, [sp, #24]");                                   // save the resolved id across __rt_new_by_name

    // -- protect transferred params while instantiation can enter PHP --
    emitter.instruction("ldr x0, [sp, #16]");                                   // recover the boxed params owner
    guards::guard(emitter, 48, 80);
    emitter.instruction("ldr x0, [sp, #24]");                                   // restore the resolved filter id

    // -- look up class_name in the registry slot --
    emitter.instruction("sub x9, x0, #128");                                    // slot_index = id - USER_FILTER_ID_BASE
    abi::emit_symbol_address(emitter, "x10", "_user_filter_registry");
    emitter.instruction("add x10, x10, x9, lsl #5");                            // registry entry = base + slot_index * 32
    emitter.instruction("ldr x1, [x10, #16]");                                  // class_name pointer
    emitter.instruction("ldr x2, [x10, #24]");                                  // class_name length
    emitter.instruction("bl __rt_new_by_name");                                 // x0 = obj or 0
    emitter.instruction("cbz x0, __rt_sfau_fail_guarded_params");               // class missing → fail and release params
    emitter.instruction("str x0, [sp, #32]");                                   // save the obj pointer for the instances table

    // -- guard the object while transferring params into its property --
    guards::guard(emitter, 80, 112);
    guards::unguard(emitter, 48, 80);
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the guarded receiver before publishing params

    // -- seed php_user_filter::$params before onCreate() observes $this --
    emitter.instruction("ldr x6, [x0]");                                        // class_id at the head of the obj
    abi::emit_symbol_address(emitter, "x7", "_user_filter_vtable_ptrs");
    emitter.instruction("ldr x7, [x7, x6, lsl #3]");                            // per-class user-filter vtable
    emitter.instruction("ldr x9, [x7, #32]");                                   // slot 4 = params property byte offset
    emitter.instruction("cbz x9, __rt_sfau_params_no_slot");                    // no params slot → release the unused boxed value
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload boxed params for transfer into the object
    emitter.instruction("ldr x12, [x0, x9]");                                   // preserve the initialized property's previous owner
    emitter.instruction("str x10, [x0, x9]");                                   // store params in the inherited public property slot
    emitter.instruction("add x11, x0, x9");                                     // compute the params property slot address
    emitter.instruction("str xzr, [x11, #8]");                                  // clear the high word for the Mixed pointer slot
    emitter.instruction("mov x0, x12");                                         // release the displaced default after publishing its replacement
    emitter.instruction("bl __rt_decref_any");                                  // the object guard owns the replacement if cleanup throws
    emitter.instruction("ldr x0, [sp, #32]");                                   // restore the guarded receiver after property cleanup
    emitter.instruction("b __rt_sfau_params_done");                             // skip unused-params release
    emitter.label("__rt_sfau_params_no_slot");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload boxed params that no object slot will own
    emitter.instruction("bl __rt_decref_any");                                  // release params when the filter class lacks the slot
    emitter.instruction("ldr x0, [sp, #32]");                                   // restore obj pointer after the release call
    emitter.label("__rt_sfau_params_done");

    // -- transfer one owner per published direction before onCreate can throw --
    guards::unguard(emitter, 80, 112);
    emitter.instruction("ldr x0, [sp, #32]");                                   // restore the object through the native retain convention
    emitter.instruction("ldr x9, [sp, #8]");                                    // determine whether both directions share this instance
    emitter.instruction("cmp x9, #3");                                          // STREAM_FILTER_ALL publishes two owning table entries
    emitter.instruction("b.ne __rt_sfau_single_owner");                         // a single direction transfers the initial owner
    emitter.instruction("bl __rt_incref");                                      // retain the second direction's independent reference
    emitter.label("__rt_sfau_single_owner");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload obj , prepare the provisional instance for direction publication

    // -- record state per direction bit --
    emitter.instruction("ldr x4, [sp, #0]");                                    // reload fd
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload mode
    emitter.instruction("ldr x6, [sp, #24]");                                   // reload resolved id (u8 in the low byte)
    abi::emit_symbol_address(emitter, "x10", "_user_filter_instances");

    // mode & 1 → read direction
    emitter.instruction("tst x5, #1");                                          // STREAM_FILTER_READ bit set?
    emitter.instruction("b.eq __rt_sfau_skip_read");                            // skip read-direction state otherwise
    emitter.instruction("add x11, x4, x4");                                     // fd*2 , read slot
    emitter.instruction("str x0, [x10, x11, lsl #3]");                          // _user_filter_instances[fd*2] = obj
    abi::emit_symbol_address(emitter, "x12", "_stream_read_filters");
    emitter.instruction("strb w6, [x12, x4]");                                  // _stream_read_filters[fd] = id (low 8 bits)
    emitter.label("__rt_sfau_skip_read");

    // mode & 2 → write direction
    emitter.instruction("tst x5, #2");                                          // STREAM_FILTER_WRITE bit set?
    emitter.instruction("b.eq __rt_sfau_skip_write");                           // skip write-direction state otherwise
    emitter.instruction("add x11, x4, x4");                                     // fd*2
    emitter.instruction("add x11, x11, #1");                                    // fd*2 + 1 , write slot
    emitter.instruction("str x0, [x10, x11, lsl #3]");                          // _user_filter_instances[fd*2 + 1] = obj
    abi::emit_symbol_address(emitter, "x12", "_stream_write_filters");
    emitter.instruction("strb w6, [x12, x4]");                                  // _stream_write_filters[fd] = id (low 8 bits)
    emitter.label("__rt_sfau_skip_write");

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
    // onCreate returning false rolls back the provisional roots without onClose.
    emitter.instruction("ldr x0, [sp, #0]");                                    // recover the descriptor whose provisional attach was refused
    emitter.instruction("ldr x1, [sp, #32]");                                   // remove only slots belonging to the rejected object
    emitter.instruction("bl __rt_user_filter_reject_fd");                       // retire its owners without invoking onClose
    emitter.instruction("b __rt_sfau_fail");                                    // continue at target label
    emitter.label("__rt_sfau_oncreate_skip");
    emitter.instruction("mov x0, #1");                                          // success
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_sfau_fail_guarded_params");
    guards::unguard(emitter, 48, 80);
    emitter.label("__rt_sfau_fail_release_params");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload boxed params that were never transferred
    emitter.instruction("bl __rt_decref_any");                                  // release params before reporting attach failure
    emitter.label("__rt_sfau_fail");
    emitter.instruction("mov x0, #0");                                          // failure: unknown name or instantiation failed
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the helper frame
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
    //   [rbp - 80..-48] params guard, [rbp - 112..-80] object guard
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // helper frame
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

    // -- protect transferred params across filter construction --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // recover the boxed params owner
    guards::guard(emitter, 48, 80);
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // restore the resolved filter id

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
    emitter.instruction("jz __rt_sfau_fail_guarded_params_x86");                // fail and release guarded params without touching state
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the obj pointer

    // -- retain exceptional object cleanup while publishing params --
    guards::guard(emitter, 80, 112);
    guards::unguard(emitter, 48, 80);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the guarded receiver before publishing params

    // -- seed php_user_filter::$params before onCreate() observes $this --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // class_id at the head of the obj
    abi::emit_symbol_address(emitter, "r11", "_user_filter_vtable_ptrs");       // load runtime data address
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-filter vtable
    emitter.instruction("mov r9, QWORD PTR [r11 + 32]");                        // slot 4 = params property byte offset
    emitter.instruction("test r9, r9");                                         // does the filter class expose params?
    emitter.instruction("jz __rt_sfau_params_no_slot_x86");                     // no params slot → release the unused boxed value
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload boxed params for transfer into the object
    emitter.instruction("mov r11, QWORD PTR [rax + r9]");                       // preserve the initialized property's previous owner
    emitter.instruction("mov QWORD PTR [rax + r9], r10");                       // store params in the inherited public property slot
    emitter.instruction("mov QWORD PTR [rax + r9 + 8], 0");                     // clear the high word for the Mixed pointer slot
    emitter.instruction("mov rax, r11");                                        // release the displaced default after publishing its replacement
    emitter.instruction("call __rt_decref_any");                                // the object guard owns the replacement if cleanup throws
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the guarded receiver after property cleanup
    emitter.instruction("jmp __rt_sfau_params_done_x86");                       // skip unused-params release
    emitter.label("__rt_sfau_params_no_slot_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload boxed params that no object slot will own
    emitter.instruction("call __rt_decref_any");                                // release params when the filter class lacks the slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore obj pointer after the release call
    emitter.label("__rt_sfau_params_done_x86");

    // -- give each selected direction an independent owner before invoking onCreate --
    guards::unguard(emitter, 80, 112);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the object through the native retain convention
    emitter.instruction("cmp QWORD PTR [rbp - 16], 3");                         // STREAM_FILTER_ALL publishes two owning table entries
    emitter.instruction("jne __rt_sfau_single_owner_x86");                      // one direction transfers the original reference
    emitter.instruction("call __rt_incref");                                    // retain a second owner for the other direction
    emitter.label("__rt_sfau_single_owner_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // prepare the provisional instance for direction publication

    // -- record state per direction bit --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload fd
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload mode
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload resolved id (u8 in the low byte)
    abi::emit_symbol_address(emitter, "r9", "_user_filter_instances");          // instances table base

    // mode & 1 → read direction
    emitter.instruction("test rsi, 1");                                         // STREAM_FILTER_READ bit set?
    emitter.instruction("jz __rt_sfau_skip_read_x86");                          // skip read-direction state otherwise
    emitter.instruction("mov r10, rdi");                                        // fd
    emitter.instruction("add r10, r10");                                        // fd*2 , read slot
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rax");                   // _user_filter_instances[fd*2] = obj
    abi::emit_symbol_address(emitter, "r11", "_stream_read_filters");           // load runtime data address
    emitter.instruction("mov BYTE PTR [r11 + rdi], dl");                        // _stream_read_filters[fd] = id (low 8 bits)
    emitter.label("__rt_sfau_skip_read_x86");

    // mode & 2 → write direction
    emitter.instruction("test rsi, 2");                                         // STREAM_FILTER_WRITE bit set?
    emitter.instruction("jz __rt_sfau_skip_write_x86");                         // skip write-direction state otherwise
    emitter.instruction("mov r10, rdi");                                        // fd
    emitter.instruction("add r10, r10");                                        // fd*2
    emitter.instruction("add r10, 1");                                          // fd*2 + 1 , write slot
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rax");                   // _user_filter_instances[fd*2 + 1] = obj
    abi::emit_symbol_address(emitter, "r11", "_stream_write_filters");          // load runtime data address
    emitter.instruction("mov BYTE PTR [r11 + rdi], dl");                        // _stream_write_filters[fd] = id (low 8 bits)
    emitter.label("__rt_sfau_skip_write_x86");

    // -- onCreate() lifecycle hook (vtable slot 1) , see ARM64 path. --
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
    // A refusal rolls back this provisional attach without onClose.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // recover the descriptor with the provisional attach
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // identify exactly the rejected object
    emitter.instruction("call __rt_user_filter_reject_fd");                     // consume its roots without running onClose
    emitter.instruction("jmp __rt_sfau_fail_x86");                              // continue at target label
    emitter.label("__rt_sfau_oncreate_skip_x86");
    emitter.instruction("mov eax, 1");                                          // success
    emitter.instruction("add rsp, 112");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_sfau_fail_guarded_params_x86");
    guards::unguard(emitter, 48, 80);
    emitter.label("__rt_sfau_fail_release_params_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload boxed params that were never transferred
    emitter.instruction("call __rt_decref_any");                                // release params before reporting attach failure
    emitter.label("__rt_sfau_fail_x86");
    emitter.instruction("xor eax, eax");                                        // failure
    emitter.instruction("add rsp, 112");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
