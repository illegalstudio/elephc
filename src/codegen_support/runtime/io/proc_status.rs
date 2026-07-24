//! Purpose:
//! Emits process-status metadata, `proc_get_status`, and `proc_terminate` helpers.
//!
//! Called from:
//! - The process builtin lowerers after `proc_open`, before `proc_close`, and for
//!   direct PHP status/termination calls.
//!
//! Key details:
//! - Windows keeps a separate raw registry mapping a live process HANDLE to the
//!   persisted command string required by PHP's status result.
//! - The Windows status query never waits or closes the HANDLE; `proc_close`
//!   remains the sole consuming operation and unregisters metadata first.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

/// Emits process-status helpers for the selected target.
pub(crate) fn emit_proc_status(emitter: &mut Emitter) {
    emit_proc_terminate(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_registry_aarch64(emitter);
            emit_registry_cached_exit_aarch64(emitter);
            emit_proc_get_status_unix_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_registry_x86_64(emitter);
            if emitter.target.platform == Platform::Windows {
                emit_registry_register_cstr_windows(emitter);
                emit_proc_get_status_windows(emitter);
            } else {
                emit_registry_cached_exit_x86_64(emitter);
                emit_proc_get_status_unix_x86_64(emitter);
            }
        }
    }
}

/// Looks up a normal-exit status harvested by `proc_get_status` on AArch64.
///
/// The helper retains the registry node; `proc_close` uses the returned exit
/// code and then unregisters the node. Signal and stopped observations are
/// never cached by php-src, so the flag is set only for `WIFEXITED`.
fn emit_registry_cached_exit_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_proc_status_cached_exit");
    abi::emit_symbol_address(emitter, "x9", "_proc_status_registry_head");
    emitter.instruction("ldr x10, [x9]");                                       // start status-node lookup
    emitter.label("__rt_proc_status_cached_exit_find");
    emitter.instruction("cbz x10, __rt_proc_status_cached_exit_none");          // no metadata means no cached normal exit
    emitter.instruction("ldr x11, [x10, #8]");                                  // load node process identifier
    emitter.instruction("cmp x11, x0");                                         // does this node belong to the process being closed?
    emitter.instruction("b.eq __rt_proc_status_cached_exit_found");             // inspect the matched node's cached-exit slot
    emitter.instruction("ldr x10, [x10]");                                      // follow the registry linked list
    emitter.instruction("b __rt_proc_status_cached_exit_find");                 // continue lookup
    emitter.label("__rt_proc_status_cached_exit_found");
    emitter.instruction("ldr x1, [x10, #32]");                                  // return the normal-exit cache flag
    emitter.instruction("ldr x2, [x10, #40]");                                  // return the cached normal exit code
    emitter.instruction("ret");                                                 // let proc_close decide whether waiting is still required
    emitter.label("__rt_proc_status_cached_exit_none");
    emitter.instruction("mov x1, #0");                                          // missing metadata cannot supply a cached exit code
    emitter.instruction("mov x2, #-1");                                         // preserve proc_close's ordinary failure sentinel
    emitter.instruction("ret");                                                 // fall back to blocking wait4
}

/// Looks up a normal-exit status harvested by `proc_get_status` on x86_64.
///
/// Returns the unchanged process descriptor in `rax`, cache flag in `rdx`, and
/// cached exit code in `rcx`; it is emitted only on Unix, where `proc_close`
/// must not wait a second time after PHP cached a `WIFEXITED` result.
fn emit_registry_cached_exit_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_proc_status_cached_exit");
    emitter.instruction("mov rax, rdi");                                        // preserve the process descriptor for proc_close
    abi::emit_symbol_address(emitter, "r10", "_proc_status_registry_head");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // start status-node lookup
    emitter.label("__rt_proc_status_cached_exit_find_x86");
    emitter.instruction("test r11, r11");                                       // reached the end of the metadata list?
    emitter.instruction("jz __rt_proc_status_cached_exit_none_x86");            // no node means no cached normal exit
    emitter.instruction("cmp QWORD PTR [r11 + 8], rdi");                        // does this node belong to the process being closed?
    emitter.instruction("je __rt_proc_status_cached_exit_found_x86");           // inspect the matched node cache
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // follow the registry linked list
    emitter.instruction("jmp __rt_proc_status_cached_exit_find_x86");           // continue lookup
    emitter.label("__rt_proc_status_cached_exit_found_x86");
    emitter.instruction("mov rdx, QWORD PTR [r11 + 32]");                       // return the normal-exit cache flag
    emitter.instruction("mov rcx, QWORD PTR [r11 + 40]");                       // return the cached normal exit code
    emitter.instruction("ret");                                                 // let proc_close decide whether waiting is still required
    emitter.label("__rt_proc_status_cached_exit_none_x86");
    emitter.instruction("xor edx, edx");                                        // missing metadata cannot supply a cached exit code
    emitter.instruction("mov rcx, -1");                                         // preserve proc_close's ordinary failure sentinel
    emitter.instruction("ret");                                                 // fall back to blocking wait4
}

/// Emits PHP's `proc_terminate` primitive for every supported target.
fn emit_proc_terminate(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc_terminate ---");
    emitter.label_global("__rt_proc_terminate");
    match emitter.target.arch {
        Arch::AArch64 => {
            if emitter.target.platform == Platform::MacOS {
                emitter.instruction("mov x16, #37");                            // macOS kill syscall number
                emitter.instruction("svc #0x80");                               // send the requested signal to the child PID
                emitter.instruction("cset x0, cc");                             // macOS reports syscall failure through the carry flag
            } else {
                emitter.instruction("mov x8, #129");                            // Linux AArch64 kill syscall number
                emitter.instruction("svc #0");                                  // send the requested signal to the child PID
                emitter.instruction("cmp x0, #0");                              // Linux uses a signed zero-or-negative syscall return
                emitter.instruction("cset x0, eq");                             // materialize PHP true/false from the syscall result
            }
            emitter.instruction("ret");                                         // return without consuming the process resource
        }
        Arch::X86_64 if emitter.target.platform == Platform::Windows => {
            emitter.instruction("sub rsp, 40");                                 // reserve MSx64 shadow space with call-site alignment
            emitter.instruction("mov rcx, rdi");                                // TerminateProcess first argument is the process HANDLE
            emitter.instruction("mov edx, 255");                                // php-src always terminates Windows processes with exit code 255
            emitter.instruction("call TerminateProcess");                       // terminate the retained Windows process HANDLE
            emitter.instruction("test eax, eax");                               // Win32 returns non-zero on success
            emitter.instruction("setne al");                                    // convert Win32 BOOL to the PHP boolean representation
            emitter.instruction("movzx eax, al");                               // clear high result bits after setne
            emitter.instruction("add rsp, 40");                                 // release MSx64 shadow space
            emitter.instruction("ret");                                         // leave handle ownership with proc_close
        }
        Arch::X86_64 => {
            emitter.instruction("mov eax, 62");                                 // Linux x86_64 kill syscall number
            emitter.instruction("syscall");                                     // send rsi signal to rdi child PID
            emitter.instruction("test rax, rax");                               // Linux returns a negative errno on failure
            emitter.instruction("setns al");                                    // true when the syscall result is zero or positive
            emitter.instruction("movzx eax, al");                               // clear high result bits after setns
            emitter.instruction("ret");                                         // return PHP boolean without reaping the child
        }
    }
}

/// Emits the Windows-only metadata registry and status hash builder.
///
/// A registry node is eight words: `next`, `HANDLE`, persisted command pointer
/// and length, plus four reserved status words.  The reserved words make the
/// Unix cached-wait extension ABI-compatible without changing the Windows node
/// allocation shape; Windows itself deliberately reports `cached => false`.
fn emit_registry_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc status registry ---");
    emit_registry_register_windows(emitter);
    emit_registry_unregister_windows(emitter);
}

/// Emits the AArch64 status registry shared by macOS and Linux.
///
/// Nodes have the same eight-word layout as x86_64: next, process PID,
/// persisted command pointer/length, then cached-status slots used by Unix
/// `wait4(WNOHANG|WUNTRACED)` queries.
fn emit_registry_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc status registry ---");
    emitter.label_global("__rt_proc_status_register");
    emitter.instruction("cmn x0, #1");                                          // failed proc_open results have no process metadata
    emitter.instruction("b.eq __rt_proc_status_register_done");                 // leave failure untouched without allocating
    emitter.instruction("sub sp, sp, #64");                                     // reserve process, command, and node spill slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the registry helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // retain the raw child PID
    emitter.instruction("str x1, [sp, #8]");                                    // retain command pointer across persistence
    emitter.instruction("str x2, [sp, #16]");                                   // retain command byte length across persistence
    abi::emit_call_label(emitter, "__rt_str_persist");                          // copy command bytes into registry-owned storage
    emitter.instruction("cbz x1, __rt_proc_status_register_oom");               // a null persisted pointer cannot back status records
    emitter.instruction("str x1, [sp, #24]");                                   // retain owned command pointer
    emitter.instruction("str x2, [sp, #32]");                                   // retain owned command length
    emitter.instruction("mov x0, #64");                                         // allocate the fixed eight-word metadata node
    abi::emit_call_label(emitter, "__rt_heap_alloc");                           // reserve raw registry storage
    emitter.instruction("cbz x0, __rt_proc_status_register_node_oom");          // release command copy when node allocation failed
    abi::emit_symbol_address(emitter, "x9", "_proc_status_registry_head");
    emitter.instruction("ldr x10, [x9]");                                       // load old metadata head
    emitter.instruction("str x10, [x0]");                                       // node.next = old head
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload raw child PID
    emitter.instruction("str x10, [x0, #8]");                                   // node.process = child PID
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload owned command pointer
    emitter.instruction("str x10, [x0, #16]");                                  // node.command_ptr = owned bytes
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload owned command length
    emitter.instruction("str x10, [x0, #24]");                                  // node.command_len = byte length
    emitter.instruction("str xzr, [x0, #32]");                                  // clear cached-exit flag
    emitter.instruction("str xzr, [x0, #40]");                                  // clear cached exit code
    emitter.instruction("str xzr, [x0, #48]");                                  // clear cached termination signal
    emitter.instruction("str xzr, [x0, #56]");                                  // clear cached stop signal
    emitter.instruction("str x0, [x9]");                                        // publish fully initialized metadata node
    emitter.instruction("ldr x0, [sp, #0]");                                    // return original process PID
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release registry frame
    emitter.instruction("ret");                                                 // resume proc_open lowering
    emitter.label("__rt_proc_status_register_node_oom");
    emitter.instruction("ldr x0, [sp, #24]");                                   // load orphaned command copy
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release ownership before falling back
    emitter.label("__rt_proc_status_register_oom");
    emitter.instruction("ldr x0, [sp, #0]");                                    // preserve successful proc_open result
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release registry frame
    emitter.instruction("ret");                                                 // no metadata is preferable to failing proc_open
    emitter.label("__rt_proc_status_register_done");
    emitter.instruction("ret");                                                 // preserve raw failure PID sentinel

    emitter.label_global("__rt_proc_status_unregister");
    emitter.instruction("sub sp, sp, #48");                                     // reserve process, previous, and matched-node slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the unlink helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // retain the child PID for return
    abi::emit_symbol_address(emitter, "x9", "_proc_status_registry_head");
    emitter.instruction("ldr x10, [x9]");                                       // begin list traversal at the registry head
    emitter.instruction("str xzr, [sp, #8]");                                   // previous node = null
    emitter.label("__rt_proc_status_unregister_find");
    emitter.instruction("cbz x10, __rt_proc_status_unregister_done");           // absent metadata is harmless during proc_close
    emitter.instruction("ldr x11, [x10, #8]");                                  // load candidate process PID
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload requested process PID
    emitter.instruction("cmp x11, x12");                                        // does this node match the consumed process?
    emitter.instruction("b.eq __rt_proc_status_unregister_found");              // unlink the matching node
    emitter.instruction("str x10, [sp, #8]");                                   // advance predecessor pointer
    emitter.instruction("ldr x10, [x10]");                                      // follow the next metadata node
    emitter.instruction("b __rt_proc_status_unregister_find");                  // continue the linked-list walk
    emitter.label("__rt_proc_status_unregister_found");
    emitter.instruction("str x10, [sp, #16]");                                  // retain matched node across cleanup calls
    emitter.instruction("ldr x11, [x10]");                                      // load successor before unlinking
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload predecessor pointer
    emitter.instruction("cbnz x12, __rt_proc_status_unregister_link_prev");     // non-head nodes relink through their predecessor
    abi::emit_symbol_address(emitter, "x9", "_proc_status_registry_head");
    emitter.instruction("str x11, [x9]");                                       // remove the matched head node
    emitter.instruction("b __rt_proc_status_unregister_unlinked");              // free detached ownership
    emitter.label("__rt_proc_status_unregister_link_prev");
    emitter.instruction("str x11, [x12]");                                      // bypass the matched node
    emitter.label("__rt_proc_status_unregister_unlinked");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload detached metadata node
    emitter.instruction("ldr x0, [x10, #16]");                                  // load its owned command string
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release command ownership
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload detached raw node
    abi::emit_call_label(emitter, "__rt_heap_free");                            // free registry storage
    emitter.label("__rt_proc_status_unregister_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return process PID to proc_close
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release unlink helper frame
    emitter.instruction("ret");                                                 // continue into wait4
}

/// Emits Unix `proc_get_status` on AArch64 with cached non-blocking wait state.
fn emit_proc_get_status_unix_aarch64(emitter: &mut Emitter) {
    let macos = emitter.target.platform == Platform::MacOS;
    emitter.blank();
    emitter.comment("--- runtime: Unix proc_get_status ---");
    emitter.label_global("__rt_proc_get_status");
    emitter.instruction("sub sp, sp, #144");                                    // reserve lookup, wait-status, and status-hash state
    emitter.instruction("stp x29, x30, [sp, #128]");                            // preserve frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the status-query frame
    emitter.instruction("str x0, [sp, #8]");                                    // retain requested child PID
    abi::emit_symbol_address(emitter, "x9", "_proc_status_registry_head");
    emitter.instruction("ldr x10, [x9]");                                       // start process metadata lookup
    emitter.label("__rt_proc_get_status_find");
    emitter.instruction("cbz x10, __rt_proc_get_status_missing");               // closed/unknown process has no PHP status record
    emitter.instruction("ldr x11, [x10, #8]");                                  // load node process PID
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload requested child PID
    emitter.instruction("cmp x11, x12");                                        // does this metadata node match?
    emitter.instruction("b.eq __rt_proc_get_status_found");                     // retain matching node
    emitter.instruction("ldr x10, [x10]");                                      // follow metadata list
    emitter.instruction("b __rt_proc_get_status_find");                         // continue process lookup
    emitter.label("__rt_proc_get_status_found");
    emitter.instruction("str x10, [sp, #0]");                                   // retain metadata node across wait/hash calls
    emitter.instruction("mov x9, #1");                                          // default running=true before non-blocking wait
    emitter.instruction("str x9, [sp, #24]");                                   // save running flag
    emitter.instruction("str xzr, [sp, #32]");                                  // default signaled=false
    emitter.instruction("str xzr, [sp, #40]");                                  // default stopped=false
    emitter.instruction("mov x9, #-1");                                         // PHP uses -1 while exit status is unknown
    emitter.instruction("str x9, [sp, #48]");                                   // save default exitcode
    emitter.instruction("str xzr, [sp, #56]");                                  // default termsig=0
    emitter.instruction("str xzr, [sp, #64]");                                  // default stopsig=0
    emitter.instruction("str xzr, [sp, #72]");                                  // default cached=false
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload metadata node
    emitter.instruction("ldr x9, [x10, #32]");                                  // inspect cached-exit flag
    emitter.instruction("cbz x9, __rt_proc_get_status_wait");                   // query wait4 until a terminal status is cached
    emitter.instruction("str xzr, [sp, #24]");                                  // cached terminal state is not running
    emitter.instruction("mov x9, #1");                                          // report PHP cached=true after prior terminal observation
    emitter.instruction("str x9, [sp, #72]");                                   // retain cached flag for hash construction
    emitter.instruction("ldr x9, [x10, #40]");                                  // reload cached exit code
    emitter.instruction("str x9, [sp, #48]");                                   // expose cached exit code
    emitter.instruction("str xzr, [sp, #56]");                                  // only normal exits are cached by php-src
    emitter.instruction("b __rt_proc_get_status_hash_unix");                    // build the stable cached record
    emitter.label("__rt_proc_get_status_wait");
    emitter.instruction("ldr x0, [sp, #8]");                                    // wait4 first argument = child PID
    emitter.instruction("add x1, sp, #16");                                     // wait4 second argument = raw-status word
    emitter.instruction("mov x2, #3");                                          // WNOHANG | WUNTRACED
    emitter.instruction("mov x3, #0");                                          // no rusage output
    if macos {
        emitter.instruction("mov x16, #7");                                     // macOS wait4 syscall number
        emitter.instruction("svc #0x80");                                       // observe child state without blocking
        emitter.instruction("b.cs __rt_proc_get_status_no_child");              // Darwin signals wait4 errors through carry, not a negative result
    } else {
        emitter.instruction("mov x8, #260");                                    // Linux AArch64 wait4 syscall number
        emitter.instruction("svc #0");                                          // observe child state without blocking
    }
    emitter.instruction("cmp x0, #0");                                          // distinguish live child, error, and returned PID
    emitter.instruction("b.eq __rt_proc_get_status_hash_unix");                 // zero means WNOHANG found the child still running
    emitter.instruction("b.lt __rt_proc_get_status_no_child");                  // ECHILD/error means process is no longer running
    emitter.instruction("ldr x9, [sp, #16]");                                   // inspect raw wait status for returned child
    emitter.instruction("and x10, x9, #0x7f");                                  // low seven bits classify exit/signal/stop
    emitter.instruction("cbz x10, __rt_proc_get_status_exited");                // zero low bits means normal exit
    emitter.instruction("cmp x10, #0x7f");                                      // 0x7f marks WIFSTOPPED
    emitter.instruction("b.eq __rt_proc_get_status_stopped");                   // stopped children remain running in PHP's record
    emitter.instruction("str xzr, [sp, #24]");                                  // signal termination means no longer running
    emitter.instruction("mov x11, #1");                                         // mark PHP signaled=true
    emitter.instruction("str x11, [sp, #32]");                                  // save signaled flag
    emitter.instruction("str x10, [sp, #56]");                                  // termsig = low status signal number
    emitter.instruction("b __rt_proc_get_status_hash_unix");                    // signal status is intentionally not cached by php-src
    emitter.label("__rt_proc_get_status_exited");
    emitter.instruction("str xzr, [sp, #24]");                                  // normal exit means no longer running
    emitter.instruction("lsr x10, x9, #8");                                     // move exit status into low byte
    emitter.instruction("and x10, x10, #0xff");                                 // retain PHP's 0..255 exit status
    emitter.instruction("str x10, [sp, #48]");                                  // expose completed exit code
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload metadata node
    emitter.instruction("mov x12, #1");                                         // terminal status becomes cacheable after this first observation
    emitter.instruction("str x12, [x11, #32]");                                 // store cached-exit flag
    emitter.instruction("str x10, [x11, #40]");                                 // cache completed exit code
    emitter.instruction("str xzr, [x11, #48]");                                 // normal exit has no termination signal
    emitter.instruction("mov x12, #1");                                         // PHP reports cached=true on the harvesting observation itself
    emitter.instruction("str x12, [sp, #72]");                                  // expose the successful normal-exit cache state
    emitter.instruction("b __rt_proc_get_status_hash_unix");                    // build normal-exit status with cached=true
    emitter.label("__rt_proc_get_status_stopped");
    emitter.instruction("mov x11, #1");                                         // PHP reports stopped=true without clearing running
    emitter.instruction("str x11, [sp, #40]");                                  // retain stopped flag
    emitter.instruction("lsr x10, x9, #8");                                     // move stop signal into low byte
    emitter.instruction("and x10, x10, #0xff");                                 // retain stop signal number
    emitter.instruction("str x10, [sp, #64]");                                  // expose stopsig
    emitter.instruction("b __rt_proc_get_status_hash_unix");                    // stopped state is deliberately not terminal-cached
    emitter.label("__rt_proc_get_status_no_child");
    emitter.instruction("str xzr, [sp, #24]");                                  // an unobservable child cannot remain running
    emitter.label("__rt_proc_get_status_hash_unix");
    emit_proc_status_hash_aarch64(emitter);
    emitter.instruction("ldp x29, x30, [sp, #128]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #144");                                    // release status-query frame
    emitter.instruction("ret");                                                 // return associative hash pointer
    emitter.label("__rt_proc_get_status_missing");
    emitter.instruction("mov x0, #0");                                          // unknown/closed resource maps to PHP false
    emitter.instruction("ldp x29, x30, [sp, #128]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #144");                                    // release status-query frame
    emitter.instruction("ret");                                                 // return null hash pointer
}

/// Builds the AArch64 Unix status hash from the standard query-frame slots.
fn emit_proc_status_hash_aarch64(emitter: &mut Emitter) {
    emitter.instruction("mov x0, #16");                                         // allocate enough buckets for all PHP status fields
    emitter.instruction("mov x1, #7");                                          // status values use heterogeneous Mixed storage
    abi::emit_call_label(emitter, "__rt_hash_new");                             // create result associative hash
    emitter.instruction("str x0, [sp, #80]");                                   // retain potentially-growing hash pointer
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload metadata node for command ownership
    emitter.instruction("ldr x3, [x10, #16]");                                  // value low word = registry-owned command pointer
    emitter.instruction("ldr x4, [x10, #24]");                                  // value high word = command byte length
    emitter.instruction("mov x5, #1");                                          // tag 1 = string
    emit_proc_status_hash_put_aarch64(emitter, "_proc_status_k_command", 7);
    emitter.instruction("ldr x3, [sp, #8]");                                    // value low word = child PID
    emitter.instruction("mov x4, #0");                                          // integers have no high payload word
    emitter.instruction("mov x5, #0");                                          // tag 0 = integer
    emit_proc_status_hash_put_aarch64(emitter, "_proc_status_k_pid", 3);
    emit_proc_status_hash_bool_aarch64(emitter, "_proc_status_k_cached", 6, "[sp, #72]");
    emit_proc_status_hash_bool_aarch64(emitter, "_proc_status_k_running", 7, "[sp, #24]");
    emit_proc_status_hash_bool_aarch64(emitter, "_proc_status_k_signaled", 8, "[sp, #32]");
    emit_proc_status_hash_bool_aarch64(emitter, "_proc_status_k_stopped", 7, "[sp, #40]");
    emitter.instruction("ldr x3, [sp, #48]");                                   // value low word = exitcode or PHP -1 sentinel
    emitter.instruction("mov x4, #0");                                          // integers have no high payload word
    emitter.instruction("mov x5, #0");                                          // tag 0 = integer
    emit_proc_status_hash_put_aarch64(emitter, "_proc_status_k_exitcode", 8);
    emit_proc_status_hash_int_aarch64(emitter, "_proc_status_k_termsig", 7, "[sp, #56]");
    emit_proc_status_hash_int_aarch64(emitter, "_proc_status_k_stopsig", 7, "[sp, #64]");
    emitter.instruction("ldr x0, [sp, #80]");                                   // return completed status hash pointer
}

/// Inserts the staged AArch64 Mixed triple under one static PHP status key.
fn emit_proc_status_hash_put_aarch64(emitter: &mut Emitter, key: &str, len: i64) {
    emitter.instruction("mov x0, x5");                                          // move the concrete value tag into the mixed-box ABI
    emitter.instruction("mov x1, x3");                                          // move the concrete low payload into the mixed-box ABI
    emitter.instruction("mov x2, x4");                                          // move the concrete high payload into the mixed-box ABI
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // give this heterogeneous hash entry its own boxed owner
    emitter.instruction("mov x3, x0");                                          // hash value low word = boxed Mixed cell
    emitter.instruction("mov x4, #0");                                          // boxed Mixed values have no separate high word
    emitter.instruction("mov x5, #7");                                          // entry tag 7 identifies a boxed Mixed cell
    abi::emit_symbol_address(emitter, "x1", key);
    emitter.instruction(&format!("mov x2, #{len}"));                            // pass static status-key byte length
    emitter.instruction("ldr x0, [sp, #80]");                                   // reload current possibly-grown hash
    abi::emit_call_label(emitter, "__rt_hash_set");                             // insert status field
    emitter.instruction("str x0, [sp, #80]");                                   // retain potentially-reallocated hash pointer
}

/// Inserts one boolean AArch64 status slot.
fn emit_proc_status_hash_bool_aarch64(emitter: &mut Emitter, key: &str, len: i64, slot: &str) {
    emitter.instruction(&format!("ldr x3, {slot}"));                            // stage boolean payload from query frame
    emitter.instruction("mov x4, #0");                                          // booleans have no high payload word
    emitter.instruction("mov x5, #3");                                          // tag 3 = boolean
    emit_proc_status_hash_put_aarch64(emitter, key, len);
}

/// Inserts one integer AArch64 status slot.
fn emit_proc_status_hash_int_aarch64(emitter: &mut Emitter, key: &str, len: i64, slot: &str) {
    emitter.instruction(&format!("ldr x3, {slot}"));                            // stage integer payload from query frame
    emitter.instruction("mov x4, #0");                                          // integers have no high payload word
    emitter.instruction("mov x5, #0");                                          // tag 0 = integer
    emit_proc_status_hash_put_aarch64(emitter, key, len);
}

/// Emits Linux-x86_64 `proc_get_status` with cached non-blocking wait state.
fn emit_proc_get_status_unix_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: Unix proc_get_status ---");
    emitter.label_global("__rt_proc_get_status");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish status-query frame
    emitter.instruction("sub rsp, 128");                                        // reserve lookup, wait status, and hash state
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // retain requested child PID
    abi::emit_symbol_address(emitter, "r10", "_proc_status_registry_head");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // start process metadata lookup
    emitter.label("__rt_proc_get_status_find");
    emitter.instruction("test r11, r11");                                       // reached the end of the metadata list?
    emitter.instruction("jz __rt_proc_get_status_missing");                     // closed/unknown process has no PHP status record
    emitter.instruction("cmp QWORD PTR [r11 + 8], rdi");                        // does this metadata node match the requested PID?
    emitter.instruction("je __rt_proc_get_status_found");                       // retain the matching node
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // follow the next metadata node
    emitter.instruction("jmp __rt_proc_get_status_find");                       // continue list traversal
    emitter.label("__rt_proc_get_status_found");
    emitter.instruction("mov QWORD PTR [rbp - 8], r11");                        // retain metadata node across wait/hash calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // default running=true before non-blocking wait
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // default signaled=false
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // default stopped=false
    emitter.instruction("mov QWORD PTR [rbp - 56], -1");                        // PHP exitcode=-1 while status is unknown
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // default termsig=0
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // default stopsig=0
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // default cached=false
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload metadata node
    emitter.instruction("cmp QWORD PTR [r10 + 32], 0");                         // was a terminal wait status cached already?
    emitter.instruction("je __rt_proc_get_status_wait_x86");                    // query wait4 until a terminal observation exists
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // cached terminal state is no longer running
    emitter.instruction("mov QWORD PTR [rbp - 80], 1");                         // subsequent status calls report cached=true
    emitter.instruction("mov rax, QWORD PTR [r10 + 40]");                       // reload cached exit code
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // expose cached exit code
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // only normal exits are cached by php-src
    emitter.instruction("jmp __rt_proc_get_status_hash_unix_x86");              // build stable cached status record
    emitter.label("__rt_proc_get_status_wait_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // wait4 first argument = child PID
    emitter.instruction("lea rsi, [rbp - 24]");                                 // wait4 second argument = raw status word
    emitter.instruction("mov edx, 3");                                          // WNOHANG | WUNTRACED
    emitter.instruction("xor r10d, r10d");                                      // no rusage output
    emitter.instruction("mov eax, 61");                                         // Linux x86_64 wait4 syscall number
    emitter.instruction("syscall");                                             // observe child state without blocking
    emitter.instruction("test rax, rax");                                       // distinguish live child, error, and returned PID
    emitter.instruction("jz __rt_proc_get_status_hash_unix_x86");               // zero means WNOHANG found child still running
    emitter.instruction("js __rt_proc_get_status_no_child_x86");                // ECHILD/error means no running child remains
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // inspect raw wait status for returned child
    emitter.instruction("mov r10, r9");                                         // copy status for low-bit classification
    emitter.instruction("and r10, 0x7f");                                       // low seven bits classify exit/signal/stop
    emitter.instruction("jz __rt_proc_get_status_exited_x86");                  // zero low bits means normal exit
    emitter.instruction("cmp r10, 0x7f");                                       // 0x7f marks WIFSTOPPED
    emitter.instruction("je __rt_proc_get_status_stopped_x86");                 // stopped children remain running in PHP's record
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // signal termination means no longer running
    emitter.instruction("mov QWORD PTR [rbp - 40], 1");                         // mark PHP signaled=true
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // termsig = low status signal number
    emitter.instruction("jmp __rt_proc_get_status_hash_unix_x86");              // signal status is intentionally not cached by php-src
    emitter.label("__rt_proc_get_status_exited_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // normal exit means no longer running
    emitter.instruction("shr r9, 8");                                           // move exit status into low byte
    emitter.instruction("and r9, 0xff");                                        // retain PHP's 0..255 exit status
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // expose completed exit code
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload metadata node
    emitter.instruction("mov QWORD PTR [r11 + 32], 1");                         // cache terminal status after this first observation
    emitter.instruction("mov QWORD PTR [r11 + 40], r9");                        // cache completed exit code
    emitter.instruction("mov QWORD PTR [r11 + 48], 0");                         // normal exit has no termination signal
    emitter.instruction("mov QWORD PTR [rbp - 80], 1");                         // PHP reports cached=true on the harvesting observation itself
    emitter.instruction("jmp __rt_proc_get_status_hash_unix_x86");              // build normal-exit status with cached=true
    emitter.label("__rt_proc_get_status_stopped_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // PHP reports stopped=true without clearing running
    emitter.instruction("shr r9, 8");                                           // move stop signal into low byte
    emitter.instruction("and r9, 0xff");                                        // retain stop signal number
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // expose stopsig
    emitter.instruction("jmp __rt_proc_get_status_hash_unix_x86");              // stopped state is not terminal-cached
    emitter.label("__rt_proc_get_status_no_child_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // an unobservable child cannot remain running
    emitter.label("__rt_proc_get_status_hash_unix_x86");
    emit_proc_status_hash_x86_64(emitter);
    emitter.instruction("add rsp, 128");                                        // release status-query frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return associative hash pointer
    emitter.label("__rt_proc_get_status_missing");
    emitter.instruction("xor eax, eax");                                        // unknown/closed resource maps to PHP false
    emitter.instruction("add rsp, 128");                                        // release status-query frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return null hash pointer
}

/// Builds the x86_64 Unix status hash from the standard query-frame slots.
fn emit_proc_status_hash_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov rdi, 16");                                         // allocate enough buckets for all PHP status fields
    emitter.instruction("mov rsi, 7");                                          // status values use heterogeneous Mixed storage
    emitter.instruction("call __rt_hash_new");                                  // create result associative hash
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // retain potentially-growing hash pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload metadata node for command ownership
    emitter.instruction("mov rcx, QWORD PTR [r10 + 16]");                       // value low word = registry-owned command pointer
    emitter.instruction("mov r8, QWORD PTR [r10 + 24]");                        // value high word = command byte length
    emitter.instruction("mov r9, 1");                                           // tag 1 = string
    emit_proc_status_hash_put_x86_64(emitter, "_proc_status_k_command", 7);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // value low word = child PID
    emitter.instruction("xor r8d, r8d");                                        // integers have no high payload word
    emitter.instruction("xor r9d, r9d");                                        // tag 0 = integer
    emit_proc_status_hash_put_x86_64(emitter, "_proc_status_k_pid", 3);
    emit_proc_status_hash_bool_x86_64(emitter, "_proc_status_k_cached", 6, 80);
    emit_proc_status_hash_bool_x86_64(emitter, "_proc_status_k_running", 7, 32);
    emit_proc_status_hash_bool_x86_64(emitter, "_proc_status_k_signaled", 8, 40);
    emit_proc_status_hash_bool_x86_64(emitter, "_proc_status_k_stopped", 7, 48);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // value low word = exitcode or PHP -1 sentinel
    emitter.instruction("xor r8d, r8d");                                        // integers have no high payload word
    emitter.instruction("xor r9d, r9d");                                        // tag 0 = integer
    emit_proc_status_hash_put_x86_64(emitter, "_proc_status_k_exitcode", 8);
    emit_proc_status_hash_int_x86_64(emitter, "_proc_status_k_termsig", 7, 64);
    emit_proc_status_hash_int_x86_64(emitter, "_proc_status_k_stopsig", 7, 72);
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // return completed status hash pointer
}

/// Inserts the staged x86_64 Mixed triple under one static PHP status key.
fn emit_proc_status_hash_put_x86_64(emitter: &mut Emitter, key: &str, len: i64) {
    emitter.instruction("mov rax, r9");                                         // move the concrete value tag into the mixed-box ABI
    emitter.instruction("mov rdi, rcx");                                        // move the concrete low payload into the mixed-box ABI
    emitter.instruction("mov rsi, r8");                                         // move the concrete high payload into the mixed-box ABI
    emitter.instruction("call __rt_mixed_from_value");                          // give this heterogeneous hash entry its own boxed owner
    emitter.instruction("mov rcx, rax");                                        // hash value low word = boxed Mixed cell
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed values have no separate high word
    emitter.instruction("mov r9, 7");                                           // entry tag 7 identifies a boxed Mixed cell
    abi::emit_symbol_address(emitter, "rsi", key);
    emitter.instruction(&format!("mov rdx, {len}"));                            // pass static status-key byte length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // reload current possibly-grown hash
    emitter.instruction("call __rt_hash_set");                                  // insert status field
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // retain potentially-reallocated hash pointer
}

/// Inserts one boolean x86_64 status slot.
fn emit_proc_status_hash_bool_x86_64(emitter: &mut Emitter, key: &str, len: i64, slot: i64) {
    emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {slot}]"));         // stage boolean payload from query frame
    emitter.instruction("xor r8d, r8d");                                        // booleans have no high payload word
    emitter.instruction("mov r9, 3");                                           // tag 3 = boolean
    emit_proc_status_hash_put_x86_64(emitter, key, len);
}

/// Inserts one integer x86_64 status slot.
fn emit_proc_status_hash_int_x86_64(emitter: &mut Emitter, key: &str, len: i64, slot: i64) {
    emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {slot}]"));         // stage integer payload from query frame
    emitter.instruction("xor r8d, r8d");                                        // integers have no high payload word
    emitter.instruction("xor r9d, r9d");                                        // tag 0 = integer
    emit_proc_status_hash_put_x86_64(emitter, key, len);
}

/// Persists the command associated with a successfully opened Windows process.
///
/// Inputs follow the internal SysV convention: `rdi = HANDLE`, `rsi = command`
/// and `rdx = command length`.  The raw process result is returned in `rax` so
/// the helper can be inserted transparently between `proc_open` and boxing.
fn emit_registry_register_windows(emitter: &mut Emitter) {
    emitter.label_global("__rt_proc_status_register");
    emitter.instruction("cmp rdi, -1");                                         // failed proc_open results have no status metadata
    emitter.instruction("je __rt_proc_status_register_done");                   // preserve failure without allocating registry state
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable registry frame
    emitter.instruction("sub rsp, 48");                                         // reserve process, command, and node spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the raw process HANDLE across allocations
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve command bytes until they are copied
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the exact command byte length
    emitter.instruction("mov rax, rsi");                                        // __rt_str_persist reads the command pointer from rax under the shared x86_64 ABI
    emitter.instruction("call __rt_str_persist");                               // allocate an owned command copy for later status queries
    emitter.instruction("test rax, rax");                                       // did command persistence allocate successfully?
    emitter.instruction("jz __rt_proc_status_register_oom");                    // leave proc_open usable when metadata allocation fails
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // retain persisted command pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // retain persisted command length
    emitter.instruction("mov rax, 64");                                         // eight machine words form one status registry node
    emitter.instruction("call __rt_heap_alloc");                                // allocate raw metadata storage
    emitter.instruction("test rax, rax");                                       // did the node allocation succeed?
    emitter.instruction("jz __rt_proc_status_register_node_oom");               // release the copied command before falling back
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // retain the node pointer while linking it
    abi::emit_symbol_address(emitter, "r10", "_proc_status_registry_head");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the previous registry head
    emitter.instruction("mov QWORD PTR [rax], r11");                            // node.next = previous head
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the process HANDLE
    emitter.instruction("mov QWORD PTR [rax + 8], r11");                        // node.process = HANDLE
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the owned command pointer
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // node.command_ptr = owned string
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the owned command length
    emitter.instruction("mov QWORD PTR [rax + 24], r11");                       // node.command_len = byte length
    emitter.instruction("mov QWORD PTR [rax + 32], 0");                         // clear reserved cached-status flags
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // clear reserved exit-code state
    emitter.instruction("mov QWORD PTR [rax + 48], 0");                         // clear reserved termination-signal state
    emitter.instruction("mov QWORD PTR [rax + 56], 0");                         // clear reserved stop-signal state
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the node only after full initialization
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the original live process HANDLE
    emitter.instruction("add rsp, 48");                                         // release registry spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // resume proc_open result handling
    emitter.label("__rt_proc_status_register_node_oom");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // recover the owned command allocation
    emitter.instruction("call __rt_decref_any");                                // release it because no node can own it
    emitter.label("__rt_proc_status_register_oom");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // preserve successful proc_open despite missing metadata
    emitter.instruction("add rsp, 48");                                         // release registry spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // fall back without status metadata
    emitter.label("__rt_proc_status_register_done");
    emitter.instruction("mov rax, rdi");                                        // return the unchanged proc_open failure sentinel
    emitter.instruction("ret");                                                 // no registry state exists for a failed process
}

/// Registers a NUL-terminated marshalled Windows command line.
fn emit_registry_register_cstr_windows(emitter: &mut Emitter) {
    emitter.label_global("__rt_proc_status_register_cstr");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable C-string scan frame
    emitter.instruction("sub rsp, 16");                                         // retain the process HANDLE and byte cursor
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the raw process HANDLE
    emitter.instruction("xor edx, edx");                                        // begin at byte offset zero
    emitter.label("__rt_proc_status_register_cstr_scan");
    emitter.instruction("cmp BYTE PTR [rsi + rdx], 0");                         // reached the marshalled command terminator?
    emitter.instruction("je __rt_proc_status_register_cstr_ready");             // the cursor is the exact command length
    emitter.instruction("inc rdx");                                             // consume one non-NUL command byte
    emitter.instruction("jmp __rt_proc_status_register_cstr_scan");             // continue the bounded NUL scan
    emitter.label("__rt_proc_status_register_cstr_ready");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the process HANDLE for the normal registry ABI
    emitter.instruction("call __rt_proc_status_register");                      // persist the counted command line
    emitter.instruction("add rsp, 16");                                         // release the C-string scan frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the unchanged process HANDLE
}

/// Removes and frees metadata after `proc_close` has consumed a process HANDLE.
fn emit_registry_unregister_windows(emitter: &mut Emitter) {
    emitter.label_global("__rt_proc_status_unregister");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable unlink frame
    emitter.instruction("sub rsp, 32");                                         // retain process, previous node, and match node
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the process HANDLE for return
    abi::emit_symbol_address(emitter, "r10", "_proc_status_registry_head");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // begin at the linked-list head
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // previous node = null
    emitter.label("__rt_proc_status_unregister_find");
    emitter.instruction("test r11, r11");                                       // reached the end of the metadata list?
    emitter.instruction("jz __rt_proc_status_unregister_done");                 // missing metadata is harmless for proc_close
    emitter.instruction("cmp QWORD PTR [r11 + 8], rdi");                        // is this node associated with the process HANDLE?
    emitter.instruction("je __rt_proc_status_unregister_found");                // unlink the matching node
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // advance the predecessor pointer
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // continue through the linked list
    emitter.instruction("jmp __rt_proc_status_unregister_find");                // inspect the next node
    emitter.label("__rt_proc_status_unregister_found");
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // retain the matched node across cleanup calls
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load its successor before unlinking
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the predecessor pointer
    emitter.instruction("test r10, r10");                                       // did the match occur at the list head?
    emitter.instruction("jnz __rt_proc_status_unregister_link_prev");           // non-head nodes relink through their predecessor
    abi::emit_symbol_address(emitter, "r10", "_proc_status_registry_head");
    emitter.instruction("mov QWORD PTR [r10], rax");                            // remove the head node
    emitter.instruction("jmp __rt_proc_status_unregister_unlinked");            // release node-owned allocations
    emitter.label("__rt_proc_status_unregister_link_prev");
    emitter.instruction("mov QWORD PTR [r10], rax");                            // bypass the matched node
    emitter.label("__rt_proc_status_unregister_unlinked");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the detached metadata node
    emitter.instruction("mov rax, QWORD PTR [r11 + 16]");                       // load its owned command string
    emitter.instruction("call __rt_decref_any");                                // release the command allocation
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the detached raw node
    emitter.instruction("call __rt_heap_free");                                 // return metadata storage to the runtime heap
    emitter.label("__rt_proc_status_unregister_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the original process HANDLE to proc_close
    emitter.instruction("add rsp, 32");                                         // release unlink spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // continue with process wait and CloseHandle
}

/// Builds PHP's Windows `proc_get_status()` associative result without consuming the HANDLE.
fn emit_proc_get_status_windows(emitter: &mut Emitter) {
    emitter.label_global("__rt_proc_get_status");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a status-query frame
    emitter.instruction("sub rsp, 128");                                        // reserve metadata, Win32 output, and hash state with shadow space
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the live process HANDLE
    abi::emit_symbol_address(emitter, "r10", "_proc_status_registry_head");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // begin metadata lookup at the registry head
    emitter.label("__rt_proc_get_status_find");
    emitter.instruction("test r11, r11");                                       // did lookup reach the end of the list?
    emitter.instruction("jz __rt_proc_get_status_missing");                     // unknown/closed resources cannot produce a status record
    emitter.instruction("cmp QWORD PTR [r11 + 8], rdi");                        // does this node own the requested HANDLE?
    emitter.instruction("je __rt_proc_get_status_found");                       // retain the matching metadata node
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // follow the next node
    emitter.instruction("jmp __rt_proc_get_status_find");                       // continue registry lookup
    emitter.label("__rt_proc_get_status_found");
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // retain the metadata node across Win32 calls
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // GetProcessId receives the live process HANDLE
    emitter.instruction("call GetProcessId");                                   // obtain PHP's numeric child pid field
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the PID for hash construction
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // GetExitCodeProcess receives the live process HANDLE
    emitter.instruction("lea rdx, [rbp - 32]");                                 // provide the DWORD exit-code out parameter
    emitter.instruction("call GetExitCodeProcess");                             // query state without waiting or closing
    emitter.instruction("test eax, eax");                                       // did the Win32 status query succeed?
    emitter.instruction("jz __rt_proc_get_status_query_failed");                // use a safe non-running fallback on API failure
    emitter.instruction("mov eax, DWORD PTR [rbp - 32]");                       // load the child exit status DWORD
    emitter.instruction("cmp eax, 259");                                        // STILL_ACTIVE means the child remains running
    emitter.instruction("sete al");                                             // record the PHP running boolean
    emitter.instruction("movzx eax, al");                                       // widen the boolean to one machine word
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the running flag
    emitter.instruction("cmp rax, 0");                                          // did status report a live process?
    emitter.instruction("jne __rt_proc_get_status_exit_ready");                 // live processes report exitcode -1
    emitter.instruction("mov eax, DWORD PTR [rbp - 32]");                       // preserve the completed process exit code
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save completed exit code for the status hash
    emitter.instruction("jmp __rt_proc_get_status_hash");                       // build the PHP status record
    emitter.label("__rt_proc_get_status_exit_ready");
    emitter.instruction("mov QWORD PTR [rbp - 48], -1");                        // running processes expose PHP exitcode -1
    emitter.instruction("jmp __rt_proc_get_status_hash");                       // build the PHP status record
    emitter.label("__rt_proc_get_status_query_failed");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // failed status query cannot report a live process
    emitter.instruction("mov QWORD PTR [rbp - 48], -1");                        // preserve PHP's unknown-exit sentinel
    emitter.label("__rt_proc_get_status_hash");
    emitter.instruction("mov rdi, 16");                                         // allocate enough initial buckets for all nine fields
    emitter.instruction("mov rsi, 7");                                          // status values are heterogeneous boxed Mixed values
    emitter.instruction("call __rt_hash_new");                                  // create the associative status hash
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // retain the possibly-moving hash pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload metadata for the persisted command
    emitter.instruction("mov rcx, QWORD PTR [r11 + 16]");                       // value low word = registry-owned command pointer
    emitter.instruction("mov r8, QWORD PTR [r11 + 24]");                        // value high word = command byte length
    emitter.instruction("mov r9, 1");                                           // runtime tag 1 = string
    emit_hash_put_windows(emitter, "_proc_status_k_command", 7);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // value low word = numeric child PID
    emitter.instruction("xor r8d, r8d");                                        // integer values have no high payload word
    emitter.instruction("xor r9d, r9d");                                        // runtime tag 0 = integer
    emit_hash_put_windows(emitter, "_proc_status_k_pid", 3);
    emit_hash_bool_windows(emitter, "_proc_status_k_cached", 6, 0);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // value low word = queried running boolean
    emitter.instruction("xor r8d, r8d");                                        // boolean values have no high payload word
    emitter.instruction("mov r9, 3");                                           // runtime tag 3 = boolean
    emit_hash_put_windows(emitter, "_proc_status_k_running", 7);
    emit_hash_bool_windows(emitter, "_proc_status_k_signaled", 8, 0);
    emit_hash_bool_windows(emitter, "_proc_status_k_stopped", 7, 0);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // value low word = completed code or PHP -1 sentinel
    emitter.instruction("xor r8d, r8d");                                        // integer values have no high payload word
    emitter.instruction("xor r9d, r9d");                                        // runtime tag 0 = integer
    emit_hash_put_windows(emitter, "_proc_status_k_exitcode", 8);
    emit_hash_int_windows(emitter, "_proc_status_k_termsig", 7, 0);
    emit_hash_int_windows(emitter, "_proc_status_k_stopsig", 7, 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // return the completed associative status hash
    emitter.instruction("add rsp, 128");                                        // release status frame and MSx64 shadow space
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // leave the HANDLE live for later proc_close
    emitter.label("__rt_proc_get_status_missing");
    emitter.instruction("xor eax, eax");                                        // no retained process metadata maps to PHP false
    emitter.instruction("add rsp, 128");                                        // release status frame and MSx64 shadow space
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return null hash pointer to the lowerer
}

/// Boxes the staged concrete value and inserts it under one static status key.
fn emit_hash_put_windows(emitter: &mut Emitter, key: &str, len: i64) {
    emitter.instruction("mov rax, r9");                                         // move the concrete value tag into the mixed-box ABI
    emitter.instruction("mov rdi, rcx");                                        // move the concrete low payload into the mixed-box ABI
    emitter.instruction("mov rsi, r8");                                         // move the concrete high payload into the mixed-box ABI
    emitter.instruction("call __rt_mixed_from_value");                          // give this heterogeneous hash entry its own boxed owner
    emitter.instruction("mov rcx, rax");                                        // hash value low word = boxed Mixed cell
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed values have no separate high word
    emitter.instruction("mov r9, 7");                                           // entry tag 7 identifies a boxed Mixed cell
    abi::emit_symbol_address(emitter, "rsi", key);
    emitter.instruction(&format!("mov rdx, {len}"));                            // pass the static key byte length to hash_set
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the current possibly-grown status hash
    emitter.instruction("call __rt_hash_set");                                  // insert one PHP status field
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // retain the potentially reallocated hash pointer
}

/// Inserts a literal boolean status field.
fn emit_hash_bool_windows(emitter: &mut Emitter, key: &str, len: i64, value: i64) {
    emitter.instruction(&format!("mov rcx, {value}"));                          // stage the boolean payload word
    emitter.instruction("xor r8d, r8d");                                        // booleans have no high payload word
    emitter.instruction("mov r9, 3");                                           // runtime tag 3 = boolean
    emit_hash_put_windows(emitter, key, len);
}

/// Inserts a literal integer status field.
fn emit_hash_int_windows(emitter: &mut Emitter, key: &str, len: i64, value: i64) {
    emitter.instruction(&format!("mov rcx, {value}"));                          // stage the integer payload word
    emitter.instruction("xor r8d, r8d");                                        // integers have no high payload word
    emitter.instruction("xor r9d, r9d");                                        // runtime tag 0 = integer
    emit_hash_put_windows(emitter, key, len);
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Structural tests for process status and termination runtime emission.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Windows checks require HANDLE-preserving API calls and every PHP status key.

    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies Windows status preserves its live HANDLE and builds all PHP fields.
    #[test]
    fn windows_proc_status_uses_live_handle_and_all_php_keys() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_status(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_proc_get_status"));
        assert!(asm.contains("call GetProcessId"));
        assert!(asm.contains("call GetExitCodeProcess"));
        assert!(asm.contains("call __rt_mixed_from_value"));
        assert!(!asm.contains("call CloseHandle"));
        for key in [
            "_proc_status_k_command",
            "_proc_status_k_pid",
            "_proc_status_k_cached",
            "_proc_status_k_running",
            "_proc_status_k_signaled",
            "_proc_status_k_stopped",
            "_proc_status_k_exitcode",
            "_proc_status_k_termsig",
            "_proc_status_k_stopsig",
        ] {
            assert!(asm.contains(key), "missing status key {key}");
        }
    }

    /// Verifies the Windows process-status registry passes command bytes to the
    /// shared string persister through its x86_64 result-register ABI.
    #[test]
    fn windows_proc_status_persists_command_with_string_runtime_abi() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_status(&mut emitter);
        let asm = emitter.output();
        let register = asm
            .split("__rt_proc_status_register:")
            .nth(1)
            .and_then(|section| section.split("__rt_proc_status_register_cstr:").next())
            .expect("Windows status-register emitter must be present");
        assert!(register.contains("mov rax, rsi"));
        assert!(register.contains("call __rt_str_persist"));
        assert!(!register.contains("mov rdi, rsi"));
    }

    /// Verifies each supported process ABI emits an explicit terminate primitive.
    #[test]
    fn proc_terminate_has_target_specific_process_primitives() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
            Target::new(Platform::Windows, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_proc_status(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_proc_terminate"));
        }
    }
}
