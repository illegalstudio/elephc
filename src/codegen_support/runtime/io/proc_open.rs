//! Purpose:
//! Emits the `__rt_proc_open` runtime helper, the C1b pipe-only `fork`/`pipe`/
//! `execve` implementation for macOS-aarch64, Linux-aarch64, and Linux-x86_64.
//! Windows-x86_64 gets the real C1c implementation (`CreatePipe`/`CreateProcessW`)
//! including native `null`, `redirect`, stream-resource, and file descriptors.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Pipe-only: descriptors must be `["pipe", mode]`. Pipe direction follows
//!   php-src: a string mode whose first byte is `w` makes the child write; every
//!   other string mode (including `r`, `rb`, and the empty string) makes the
//!   child read. PHP weakly coerces scalar non-string modes to strings; their
//!   decimal/boolean/null spellings cannot start with `w`, so they also make
//!   the child read without an unnecessary temporary string allocation.
//!   Any other descriptor shape makes
//!   `proc_open` return `-1` (documented C1b limitation; `["file",...]` and
//!   friends are a follow-up). The descriptor count is bounded at 8. The
//!   descriptor_spec array may be either indexed (kind 2) or hash (kind 3);
//!   `__rt_array_get_mixed_key` is representation-agnostic and handles both.
//! - ABI (set by the EIR lowerer, never changed here): AArch64 `x0` =
//!   descriptor_spec, `x1`/`x2` = command ptr/len, `x3` = pipes, `x4`/`x5` = cwd,
//!   `x6` = environment block and `x7` = packed length/direct flags. x86_64 uses
//!   `rdi` through `r9` plus two stack arguments. Returns the raw child pid (`>= 0`) on
//!   success or `-1` on failure; the lowerer boxes `>= 0` as `Mixed(resource,
//!   kind=5)` and `< 0` as `Mixed(false)`.
//! - `$pipes` may be promoted/reallocated on every target when a sparse
//!   descriptor key is written. The final container pointer is consequently
//!   returned in the paired secondary result register (`x1`/`rdx`) and the EIR
//!   backend writes it through the original by-reference local. Pipe resources
//!   retain their real integer descriptor keys rather than being appended or
//!   reindexed.
//! - Raw syscalls are emitted directly (not via `emitter.syscall()`/`map_syscall`)
//!   so this helper stays self-contained. Linux `svc` does not set flags, so an
//!   explicit `cmp x0, #0` precedes every conditional branch on AArch64 Linux.
//! - macOS fork caveat: the raw `fork` syscall (2) returns the child pid in BOTH
//!   the parent and the child process (not 0 in the child). Parent/child are
//!   distinguished by comparing `getpid()` before and after fork — a different
//!   pid means the child. Linux's `clone` returns 0 in the child as expected.
//! - macOS close caveat: `SYS_close` is syscall 6 on macOS (NOT 3, which is
//!   `SYS_read`). Using 3 would call `read()` on the pipe fd and block forever.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

/// Emits `__rt_proc_open`: pipe-only `proc_open` returning the child pid.
///
/// Input ABI: AArch64 `x0` = descriptor_spec, `x1`/`x2` = command ptr/len,
/// `x3` = pipes array ptr; x86_64 `rdi`/`rsi`/`rdx`/`rcx`. Output: the raw child
/// pid (`>= 0`) on success, or `-1` on failure (the lowerer does the boxing).
/// Every target returns the final `$pipes` container in the paired secondary
/// result register.
///
/// Target dispatch: AArch64 (macOS + Linux) shares one emitter that branches on
/// `emitter.platform` for syscall numbers/mechanisms; Linux-x86_64 gets its own
/// System V AMD64 variant; Windows-x86_64 gets its own MSx64 `CreatePipe`/
/// `CreateProcessW` variant.
pub fn emit_proc_open(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_proc_open_aarch64(emitter),
        Arch::X86_64 => {
            if emitter.target.platform == Platform::Linux {
                emit_proc_open_linux_x86_64(emitter);
            } else {
                emit_proc_open_win32_x86_64(emitter);
                emit_proc_open_socketpair_win(emitter);
            }
        }
    }
}

/// Emits the AArch64 `__rt_proc_open` runtime (macOS and Linux).
///
/// 496-byte frame layout (offsets from `sp`):
/// `[0]` x29, `[8]` x30, `[16]` desc, `[24]` cmd_ptr, `[32]` cmd_len,
/// `[40]` pipes, `[48]` n, `[56]` pipe_count, `[64]` i, `[72]` sub_box,
/// `[80]` sub_ptr (reused for `is_read` after the mode read), `[88]` m0,
/// `[96]` m1, `[104]`/`[112]` pipe_fds, `[120..184)` parent_fd,
/// `[184..248)` child_fd, `[248..312)` is_pipe, `[312]` path_buf,
/// `[320]` argv0, `[328]` argv1, `[336]` cmd_cstr, `[344..376)` argv,
/// `[376]` lit_pipe, `[384]` lit_r, `[392]` pid, `[400]` cleanup_j,
/// `[408]` parent_pid (macOS fork parent/child disambiguation),
/// `[416..480)` descriptor_index[8], `[480]` hash cursor, `[488]` spec kind.
fn emit_proc_open_aarch64(emitter: &mut Emitter) {
    let is_macos = emitter.target.platform == Platform::MacOS;
    emitter.blank();
    emitter.comment("--- runtime: proc_open (C1b pipe-only) ---");
    emitter.label_global("__rt_proc_open");

    // -- prologue: 496-byte bookkeeping frame --
    emitter.instruction("sub sp, sp, #496");                                    // reserve the proc_open frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the descriptor_spec array pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the command string pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save the command string length
    emitter.instruction("str x3, [sp, #40]");                                   // save the pipes array pointer

    // -- validate descriptor_spec: non-null + indexed kind + 1 <= n <= 8 --
    emitter.instruction("cbz x0, __rt_proc_open_fail");                         // a null descriptor_spec is unrecoverable
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed array kind metadata
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low-byte storage kind
    emitter.instruction("cmp x9, #2");                                          // kind 2 = indexed array?
    emitter.instruction("b.eq __rt_proc_open_kind_ok");                         // accept indexed-array descriptor_spec
    emitter.instruction("cmp x9, #3");                                          // kind 3 = hash storage?
    emitter.instruction("b.ne __rt_proc_open_fail");                            // non-array descriptor_spec is unsupported
    emitter.label("__rt_proc_open_kind_ok");
    emitter.instruction("ldr x9, [x0]");                                        // read the descriptor_spec length
    emitter.instruction("cmp x9, #0");                                          // an empty descriptor spec is invalid
    emitter.instruction("b.eq __rt_proc_open_fail");                            // bail out on an empty descriptor spec
    emitter.instruction("cmp x9, #8");                                          // C1b bounds the descriptor count at 8
    emitter.instruction("b.gt __rt_proc_open_fail");                            // refuse an over-long descriptor spec
    emitter.instruction("str x9, [sp, #48]");                                   // save the descriptor count n
    emitter.instruction("ldr x9, [x0, #-8]");                                   // reload descriptor storage metadata after the count read
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the indexed/hash kind for position iteration
    emitter.instruction("str x9, [sp, #488]");                                  // retain descriptor storage kind across runtime helper calls
    emitter.instruction("str xzr, [sp, #480]");                                 // start the hash insertion-order cursor at its head

    // -- zero the is_pipe bookkeeping array so cleanup is safe before any pipe opens --
    emitter.instruction("str xzr, [sp, #248]");                                 // is_pipe[0] = 0
    emitter.instruction("str xzr, [sp, #256]");                                 // is_pipe[1] = 0
    emitter.instruction("str xzr, [sp, #264]");                                 // is_pipe[2] = 0
    emitter.instruction("str xzr, [sp, #272]");                                 // is_pipe[3] = 0
    emitter.instruction("str xzr, [sp, #280]");                                 // is_pipe[4] = 0
    emitter.instruction("str xzr, [sp, #288]");                                 // is_pipe[5] = 0
    emitter.instruction("str xzr, [sp, #296]");                                 // is_pipe[6] = 0
    emitter.instruction("str xzr, [sp, #304]");                                 // is_pipe[7] = 0

    // -- main descriptor loop: for i = 0 .. n-1, open one pipe per descriptor --
    emitter.instruction("str xzr, [sp, #64]");                                  // i = 0
    emitter.label("__rt_proc_open_loop_test");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the loop index
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload n
    emitter.instruction("cmp x9, x10");                                         // i < n?
    emitter.instruction("b.ge __rt_proc_open_fork");                            // descriptor loop complete -> fork

    // -- resolve position i to its real integer descriptor key, then read it --
    emitter.instruction("ldr x9, [sp, #488]");                                  // reload descriptor storage kind
    emitter.instruction("cmp x9, #3");                                          // descriptor spec is associative hash storage?
    emitter.instruction("b.ne __rt_proc_open_indexed_key");                     // indexed positions are their integer keys
    emitter.instruction("ldr x0, [sp, #16]");                                   // hash descriptor-spec pointer
    emitter.instruction("ldr x1, [sp, #480]");                                  // insertion-order hash cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    emitter.instruction("str x0, [sp, #480]");                                  // preserve the returned hash cursor
    emitter.instruction("cmn x2, #1");                                          // only integer descriptor keys are valid
    emitter.instruction("b.ne __rt_proc_open_fail");                            // string keys cannot name child descriptors
    emitter.instruction("str x1, [sp, #400]");                                  // retain the actual descriptor integer key for this loop position
    emitter.instruction("b __rt_proc_open_descriptor_key_ready");               // skip indexed key synthesis
    emitter.label("__rt_proc_open_indexed_key");
    emitter.instruction("ldr x9, [sp, #64]");                                   // indexed position is the descriptor integer key
    emitter.instruction("str x9, [sp, #400]");                                  // retain the descriptor integer key for this loop position
    emitter.label("__rt_proc_open_descriptor_key_ready");
    emitter.instruction("ldr x0, [sp, #16]");                                   // descriptor_spec array pointer
    emitter.instruction("ldr x1, [sp, #400]");                                  // key = actual descriptor integer key
    emitter.instruction("mov x2, #-1");                                         // int-key sentinel
    emitter.instruction("mov x3, #0");                                          // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("str x0, [sp, #72]");                                   // save the owned sub_box across unbox

    // -- unbox sub_box: expect an indexed array (runtime tag 4) --
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // x0=tag, x1=lo (sub ptr), x2=hi
    emitter.instruction("cmp x0, #4");                                          // tag 4 = indexed array?
    emitter.instruction("b.ne __rt_proc_open_cleanup_sub");                     // non-array descriptor -> cleanup + fail
    emitter.instruction("str x1, [sp, #80]");                                   // save the sub-array pointer for element reads

    // -- read sub[0] (the descriptor type string) as an owned box --
    emitter.instruction("ldr x0, [sp, #80]");                                   // sub-array pointer
    emitter.instruction("mov x1, #0");                                          // key = 0
    emitter.instruction("mov x2, #-1");                                         // int-key sentinel
    emitter.instruction("mov x3, #0");                                          // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("str x0, [sp, #88]");                                   // save m0 (descriptor type string box)

    // -- unbox m0: expect a string (runtime tag 1) --
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // x0=tag, x1=ptr, x2=len
    emitter.instruction("cmp x0, #1");                                          // tag 1 = string?
    emitter.instruction("b.ne __rt_proc_open_cleanup_m0");                      // non-string descriptor type -> cleanup + fail

    // -- build the literal "pipe" on the stack and compare against sub[0] --
    emitter.instruction("mov w9, #0x70");                                       // 'p'
    emitter.instruction("strb w9, [sp, #376]");                                 // lit_pipe[0] = 'p'
    emitter.instruction("mov w9, #0x69");                                       // 'i'
    emitter.instruction("strb w9, [sp, #377]");                                 // lit_pipe[1] = 'i'
    emitter.instruction("mov w9, #0x70");                                       // 'p'
    emitter.instruction("strb w9, [sp, #378]");                                 // lit_pipe[2] = 'p'
    emitter.instruction("mov w9, #0x65");                                       // 'e'
    emitter.instruction("strb w9, [sp, #379]");                                 // lit_pipe[3] = 'e'
    // __rt_str_eq(ptr_a, len_a, ptr_b, len_b): x1/x2 already hold ptr/len from unbox
    emitter.instruction("add x3, sp, #376");                                    // ptr_b = &lit_pipe
    emitter.instruction("mov x4, #4");                                          // len_b = 4
    abi::emit_call_label(emitter, "__rt_str_eq");                               // x0 = 1 if "pipe" else 0
    emitter.instruction("cbz x0, __rt_proc_open_cleanup_m0");                   // non-pipe descriptor unsupported in C1b -> cleanup + fail

    // -- read sub[1] (the mode string) as an owned box --
    emitter.instruction("ldr x0, [sp, #80]");                                   // sub-array pointer
    emitter.instruction("mov x1, #1");                                          // key = 1
    emitter.instruction("mov x2, #-1");                                         // int-key sentinel
    emitter.instruction("mov x3, #0");                                          // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("str x0, [sp, #96]");                                   // save m1 (mode string box)

    // -- unbox m1: strings select their first byte; PHP scalar coercions read --
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // x0=tag, x1=ptr, x2=len
    emitter.instruction("cmp x0, #1");                                          // tag 1 = string?
    emitter.instruction("b.eq __rt_proc_open_string_mode");                     // strings retain php-src's first-byte direction rule
    emitter.instruction("cmp x0, #0");                                          // tag 0 = integer scalar?
    emitter.instruction("b.eq __rt_proc_open_scalar_mode_read");                // decimal integer text cannot begin with `w`
    emitter.instruction("cmp x0, #2");                                          // tag 2 = float scalar?
    emitter.instruction("b.eq __rt_proc_open_scalar_mode_read");                // float text cannot begin with `w`
    emitter.instruction("cmp x0, #3");                                          // tag 3 = boolean scalar?
    emitter.instruction("b.eq __rt_proc_open_scalar_mode_read");                // boolean text cannot begin with `w`
    emitter.instruction("cmp x0, #8");                                          // tag 8 = null scalar?
    emitter.instruction("b.ne __rt_proc_open_cleanup_m1");                      // non-scalar mode remains unsupported by this runtime
    emitter.label("__rt_proc_open_scalar_mode_read");
    emitter.instruction("mov x9, #1");                                          // PHP scalar-to-string coercion produces a non-write-leading mode
    emitter.instruction("b __rt_proc_open_mode_direction_ready");               // release owned boxes using the common direction path

    // -- match php-src: only a leading "w" changes the pipe direction --
    emitter.label("__rt_proc_open_string_mode");
    emitter.instruction("cbz x2, __rt_proc_open_empty_mode_read");              // an empty mode follows php-src's non-write/read direction
    emitter.instruction("ldrb w9, [x1]");                                       // load the mode's first byte
    emitter.instruction("cmp w9, #0x77");                                       // first byte is 'w'?
    emitter.instruction("cset x9, ne");                                         // is_read = first byte is not 'w'
    emitter.instruction("b __rt_proc_open_mode_direction_ready");               // keep the computed non-empty mode direction
    emitter.label("__rt_proc_open_empty_mode_read");
    emitter.instruction("mov x9, #1");                                          // empty modes also make the child read
    emitter.label("__rt_proc_open_mode_direction_ready");
    emitter.instruction("str x9, [sp, #80]");                                   // save is_read in the sub_ptr slot (no longer needed)

    // -- release the three owned boxes now that is_read is known --
    emitter.instruction("ldr x0, [sp, #88]");                                   // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // drop the caller's ref on m0
    emitter.instruction("ldr x0, [sp, #96]");                                   // m1
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // drop the caller's ref on m1
    emitter.instruction("ldr x0, [sp, #72]");                                   // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // drop the caller's ref on sub_box

    // -- open a pipe pair: macOS pipe (fds in x0/x1), Linux pipe2 (fds in buffer) --
    if is_macos {
        // macOS pipe syscall returns read_fd in x0, write_fd in x1 (no buffer arg).
        emitter.instruction("mov x16, #42");                                    // macOS pipe syscall number
        emitter.instruction("svc #0x80");                                       // x0 = read_fd, x1 = write_fd
        emitter.instruction("cmp x0, #0");                                      // x0 < 0 means error (-errno)
        emitter.instruction("b.lt __rt_proc_open_cleanup");                     // pipe open failed -> cleanup opened pipes + fail
        emitter.instruction("str w0, [sp, #104]");                              // save read_end as 32-bit int
        emitter.instruction("str w1, [sp, #108]");                              // save write_end as 32-bit int
    } else {
        // Linux pipe2 writes fds to the buffer and returns 0/-1.
        emitter.instruction("add x0, sp, #104");                                // &pipe_fds[0]
        emitter.instruction("mov x1, #0");                                      // pipe2 flags = 0
        emitter.instruction("mov x8, #59");                                     // Linux pipe2 syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
        emitter.instruction("cmp x0, #0");                                      // pipe2 retval < 0 means failure
        emitter.instruction("b.lt __rt_proc_open_cleanup");                     // pipe open failed -> cleanup opened pipes + fail
    }

    // -- record the read/write ends per mode: child gets the mode end, parent the other --
    emitter.instruction("ldr w10, [sp, #104]");                                 // read_end = pipe_fds[0] (32-bit int)
    emitter.instruction("ldr w11, [sp, #108]");                                 // write_end = pipe_fds[1] (32-bit int)
    emitter.instruction("ldr x12, [sp, #64]");                                  // i (descriptor index)
    emitter.instruction("add x13, sp, #184");                                   // base of child_fd
    emitter.instruction("add x13, x13, x12, lsl #3");                           // &child_fd[i]
    emitter.instruction("add x14, sp, #120");                                   // base of parent_fd
    emitter.instruction("add x14, x14, x12, lsl #3");                           // &parent_fd[i]
    emitter.instruction("ldr x9, [sp, #80]");                                   // reload is_read
    emitter.instruction("cbz x9, __rt_proc_open_write_mode");                   // a write-leading mode gives the child write_end
    emitter.instruction("str x10, [x13]");                                      // non-write-leading mode gives the child read_end
    emitter.instruction("str x11, [x14]");                                      // non-write-leading mode gives the parent write_end
    emitter.instruction("b __rt_proc_open_pipe_recorded");                      // skip the write-mode assignment
    emitter.label("__rt_proc_open_write_mode");
    emitter.instruction("str x11, [x13]");                                      // write-leading mode gives the child write_end
    emitter.instruction("str x10, [x14]");                                      // write-leading mode gives the parent read_end
    emitter.label("__rt_proc_open_pipe_recorded");
    emitter.instruction("mov x9, #1");                                          // is_pipe sentinel
    emitter.instruction("add x13, sp, #248");                                   // base of is_pipe
    emitter.instruction("add x13, x13, x12, lsl #3");                           // &is_pipe[i]
    emitter.instruction("str x9, [x13]");                                       // is_pipe[i] = 1
    emitter.instruction("add x13, sp, #416");                                   // base of descriptor_index bookkeeping
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload loop position i
    emitter.instruction("ldr x10, [sp, #400]");                                 // reload the real descriptor integer key
    emitter.instruction("str x10, [x13, x9, lsl #3]");                          // descriptor_index[i] = real key for child/output phases

    // -- advance the loop index --
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload i
    emitter.instruction("add x9, x9, #1");                                      // i += 1
    emitter.instruction("str x9, [sp, #64]");                                   // persist the loop index
    emitter.instruction("b __rt_proc_open_loop_test");                          // continue the descriptor loop

    // -- fork/clone: macOS fork=2, Linux clone(SIGCHLD) = 220 --
    // macOS caveat: the raw fork syscall returns the child pid in BOTH the
    // parent and the child (not 0 in the child). We distinguish parent from
    // child by comparing getpid() before and after fork. Linux's clone returns
    // 0 in the child as expected.
    emitter.label("__rt_proc_open_fork");
    if is_macos {
        emitter.instruction("mov x16, #20");                                    // getpid before fork
        emitter.instruction("svc #0x80");                                       // x0 = parent pid
        emitter.instruction("str x0, [sp, #408]");                              // save parent pid for disambiguation
        emitter.instruction("mov x16, #2");                                     // macOS fork syscall number
        emitter.instruction("svc #0x80");                                       // x0 = child pid (both processes)
        emitter.instruction("str x0, [sp, #392]");                              // save fork retval (pid or -1)
        emitter.instruction("cmp x0, #0");                                      // fork retval < 0 means failure
        emitter.instruction("b.lt __rt_proc_open_cleanup");                     // fork failed -> close all opened pipes + fail
        emitter.instruction("mov x16, #20");                                    // getpid after fork
        emitter.instruction("svc #0x80");                                       // x0 = current pid
        emitter.instruction("ldr x10, [sp, #408]");                             // reload the saved parent pid
        emitter.instruction("cmp x0, x10");                                     // current pid == parent pid?
        emitter.instruction("b.ne __rt_proc_open_child");                       // different pid -> child branch
        // parent: child pid already saved at [sp, #392]
    } else {
        emitter.instruction("mov x0, #17");                                     // clone flags = SIGCHLD
        emitter.instruction("mov x1, #0");                                      // child_stack = 0 (fork semantics)
        emitter.instruction("mov x2, #0");                                      // ptid = 0
        emitter.instruction("mov x3, #0");                                      // ctid = 0
        emitter.instruction("mov x4, #0");                                      // tls = 0
        emitter.instruction("mov x8, #220");                                    // Linux clone syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
        emitter.instruction("cmp x0, #0");                                      // fork retval < 0 means failure
        emitter.instruction("b.lt __rt_proc_open_cleanup");                     // fork failed -> close all opened pipes + fail
        emitter.instruction("b.eq __rt_proc_open_child");                       // pid == 0 -> child branch
        emitter.instruction("str x0, [sp, #392]");                              // parent: save the child pid
    }

    // -- parent: close every child end, then push the parent ends into $pipes --
    emitter.instruction("str xzr, [sp, #64]");                                  // j = 0
    emitter.label("__rt_proc_open_parent_close_test");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload n
    emitter.instruction("cmp x9, x10");                                         // j < n?
    emitter.instruction("b.ge __rt_proc_open_parent_push");                     // all child ends closed -> push phase
    emitter.instruction("add x11, sp, #248");                                   // base of is_pipe
    emitter.instruction("ldr x12, [x11, x9, lsl #3]");                          // is_pipe[j]
    emitter.instruction("cbz x12, __rt_proc_open_parent_close_next");           // not a pipe -> skip
    emitter.instruction("add x11, sp, #184");                                   // base of child_fd
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // child_fd[j]
    if is_macos {
        emitter.instruction("mov x16, #6");                                     // macOS close syscall number (SYS_close=6)
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap
    } else {
        emitter.instruction("mov x8, #57");                                     // Linux close syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
    }
    emitter.label("__rt_proc_open_parent_close_next");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j
    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("str x9, [sp, #64]");                                   // persist j
    emitter.instruction("b __rt_proc_open_parent_close_test");                  // continue closing child ends

    // -- parent publish phase: write each parent end at its descriptor key --
    emitter.label("__rt_proc_open_parent_push");
    emitter.instruction("str xzr, [sp, #64]");                                  // j = 0
    emitter.instruction("str xzr, [sp, #56]");                                  // pipe_count = 0
    emitter.label("__rt_proc_open_parent_push_test");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload n
    emitter.instruction("cmp x9, x10");                                         // j < n?
    emitter.instruction("b.ge __rt_proc_open_done");                            // all pipes pushed -> return pid
    emitter.instruction("add x11, sp, #248");                                   // base of is_pipe
    emitter.instruction("ldr x12, [x11, x9, lsl #3]");                          // is_pipe[j]
    emitter.instruction("cbz x12, __rt_proc_open_parent_push_next");            // not a pipe -> skip
    emitter.instruction("add x11, sp, #120");                                   // base of parent_fd
    emitter.instruction("ldr x1, [x11, x9, lsl #3]");                           // parent_fd[j] (resource handle lo)
    emitter.instruction("mov x0, #9");                                          // tag 9 = resource
    emitter.instruction("mov x2, #1");                                          // hi = kind 1 (native stream fd)
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = res_box (owned, refcount 1)
    emitter.instruction("str x0, [sp, #72]");                                   // save the owned resource box for the keyed container write
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j after the allocating Mixed helper clobbered caller-saved registers
    emitter.instruction("add x11, sp, #416");                                   // base of descriptor_index bookkeeping
    emitter.instruction("ldr x1, [x11, x9, lsl #3]");                           // real integer descriptor key for this parent pipe
    emitter.instruction("mov x0, #0");                                          // tag 0 = integer key Mixed cell
    emitter.instruction("mov x2, #0");                                          // integer key has no high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // allocate the borrowed key cell used by the generic setter
    emitter.instruction("str x0, [sp, #88]");                                   // retain the key cell across the container write
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the current pipes array/hash pointer
    emitter.instruction("ldr x1, [sp, #88]");                                   // pass the integer descriptor key cell
    emitter.instruction("ldr x2, [sp, #72]");                                   // transfer the resource box into the destination container
    abi::emit_call_label(emitter, "__rt_array_set_mixed_key");                  // set resource at the real descriptor key, returning promoted/reallocated container
    emitter.instruction("str x0, [sp, #40]");                                   // persist the potentially promoted/reallocated pipes container
    emitter.instruction("ldr x0, [sp, #88]");                                   // reload the borrowed key cell after the setter read it
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the caller-owned key box; the setter never owns keys
    emitter.label("__rt_proc_open_parent_push_next");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j
    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("str x9, [sp, #64]");                                   // persist j
    emitter.instruction("b __rt_proc_open_parent_push_test");                   // continue pushing parent ends

    // -- child: dup2 each child end onto descriptor fd i, close the parent ends, execve --
    emitter.label("__rt_proc_open_child");
    emitter.instruction("str xzr, [sp, #64]");                                  // j = 0
    emitter.label("__rt_proc_open_child_dup_test");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload n
    emitter.instruction("cmp x9, x10");                                         // j < n?
    emitter.instruction("b.ge __rt_proc_open_child_close_phase");               // all dups done -> close parent ends
    emitter.instruction("add x11, sp, #248");                                   // base of is_pipe
    emitter.instruction("ldr x12, [x11, x9, lsl #3]");                          // is_pipe[j]
    emitter.instruction("cbz x12, __rt_proc_open_child_dup_next");              // not a pipe -> skip
    emitter.instruction("add x11, sp, #184");                                   // base of child_fd
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // child_fd[j] (old fd)
    emitter.instruction("add x11, sp, #416");                                   // base of descriptor_index bookkeeping
    emitter.instruction("ldr x1, [x11, x9, lsl #3]");                           // new fd = real descriptor integer key
    if is_macos {
        emitter.instruction("mov x16, #90");                                    // macOS dup2 syscall number
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap
    } else {
        emitter.instruction("mov x2, #0");                                      // dup3 flags = 0
        emitter.instruction("mov x8, #24");                                     // Linux dup3 syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
    }
    // -- if child_fd[j] != j, close the now-redundant original child end --
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j (preserved across svc in x9)
    emitter.instruction("add x11, sp, #184");                                   // base of child_fd
    emitter.instruction("ldr x13, [x11, x9, lsl #3]");                          // reload child_fd[j]
    emitter.instruction("add x11, sp, #416");                                   // base of descriptor_index bookkeeping
    emitter.instruction("ldr x10, [x11, x9, lsl #3]");                          // reload the real descriptor integer key
    emitter.instruction("cmp x13, x10");                                        // child_fd[j] == descriptor key?
    emitter.instruction("b.eq __rt_proc_open_child_dup_next");                  // equal -> no close needed (fd is already the dup target)
    emitter.instruction("mov x0, x13");                                         // close(child_fd[j])
    if is_macos {
        emitter.instruction("mov x16, #6");                                     // macOS close syscall number (SYS_close=6)
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap
    } else {
        emitter.instruction("mov x8, #57");                                     // Linux close syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
    }
    emitter.label("__rt_proc_open_child_dup_next");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j
    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("str x9, [sp, #64]");                                   // persist j
    emitter.instruction("b __rt_proc_open_child_dup_test");                     // continue the dup loop

    // -- child: close every parent end so the child does not hold the parent's pipe side --
    emitter.label("__rt_proc_open_child_close_phase");
    emitter.instruction("str xzr, [sp, #64]");                                  // j = 0
    emitter.label("__rt_proc_open_child_close_test");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload n
    emitter.instruction("cmp x9, x10");                                         // j < n?
    emitter.instruction("b.ge __rt_proc_open_child_exec");                      // all parent ends closed -> execve
    emitter.instruction("add x11, sp, #248");                                   // base of is_pipe
    emitter.instruction("ldr x12, [x11, x9, lsl #3]");                          // is_pipe[j]
    emitter.instruction("cbz x12, __rt_proc_open_child_close_next");            // not a pipe -> skip
    emitter.instruction("add x11, sp, #120");                                   // base of parent_fd
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // parent_fd[j]
    if is_macos {
        emitter.instruction("mov x16, #6");                                     // macOS close syscall number (SYS_close=6)
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap
    } else {
        emitter.instruction("mov x8, #57");                                     // Linux close syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
    }
    emitter.label("__rt_proc_open_child_close_next");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload j
    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("str x9, [sp, #64]");                                   // persist j
    emitter.instruction("b __rt_proc_open_child_close_test");                   // continue closing parent ends

    // -- child: build the execve payload and exec /bin/sh -c <command> --
    emitter.label("__rt_proc_open_child_exec");
    emitter.instruction("ldr x1, [sp, #24]");                                   // command pointer into __rt_cstr input
    emitter.instruction("ldr x2, [sp, #32]");                                   // command length into __rt_cstr input
    abi::emit_call_label(emitter, "__rt_cstr");                                 // x0 = null-terminated command string
    emitter.instruction("str x0, [sp, #336]");                                  // save cmd_cstr for argv[2]
    // -- store "/bin/sh\0" into path_buf --
    emitter.instruction("mov w9, #0x2f");                                       // '/'
    emitter.instruction("strb w9, [sp, #312]");                                 // path_buf[0] = '/'
    emitter.instruction("mov w9, #0x62");                                       // 'b'
    emitter.instruction("strb w9, [sp, #313]");                                 // path_buf[1] = 'b'
    emitter.instruction("mov w9, #0x69");                                       // 'i'
    emitter.instruction("strb w9, [sp, #314]");                                 // path_buf[2] = 'i'
    emitter.instruction("mov w9, #0x6e");                                       // 'n'
    emitter.instruction("strb w9, [sp, #315]");                                 // path_buf[3] = 'n'
    emitter.instruction("mov w9, #0x2f");                                       // '/'
    emitter.instruction("strb w9, [sp, #316]");                                 // path_buf[4] = '/'
    emitter.instruction("mov w9, #0x73");                                       // 's'
    emitter.instruction("strb w9, [sp, #317]");                                 // path_buf[5] = 's'
    emitter.instruction("mov w9, #0x68");                                       // 'h'
    emitter.instruction("strb w9, [sp, #318]");                                 // path_buf[6] = 'h'
    emitter.instruction("strb wzr, [sp, #319]");                                // path_buf[7] = NUL
    // -- store "sh\0" into argv0 --
    emitter.instruction("mov w9, #0x73");                                       // 's'
    emitter.instruction("strb w9, [sp, #320]");                                 // argv0[0] = 's'
    emitter.instruction("mov w9, #0x68");                                       // 'h'
    emitter.instruction("strb w9, [sp, #321]");                                 // argv0[1] = 'h'
    emitter.instruction("strb wzr, [sp, #322]");                                // argv0[2] = NUL
    // -- store "-c\0" into argv1 --
    emitter.instruction("mov w9, #0x2d");                                       // '-'
    emitter.instruction("strb w9, [sp, #328]");                                 // argv1[0] = '-'
    emitter.instruction("mov w9, #0x63");                                       // 'c'
    emitter.instruction("strb w9, [sp, #329]");                                 // argv1[1] = 'c'
    emitter.instruction("strb wzr, [sp, #330]");                                // argv1[2] = NUL
    // -- build argv[4] = { &argv0, &argv1, cmd_cstr, NULL } --
    emitter.instruction("add x9, sp, #320");                                    // &argv0
    emitter.instruction("str x9, [sp, #344]");                                  // argv[0] = &argv0
    emitter.instruction("add x9, sp, #328");                                    // &argv1
    emitter.instruction("str x9, [sp, #352]");                                  // argv[1] = &argv1
    emitter.instruction("ldr x9, [sp, #336]");                                  // cmd_cstr
    emitter.instruction("str x9, [sp, #360]");                                  // argv[2] = cmd_cstr
    emitter.instruction("str xzr, [sp, #368]");                                 // argv[3] = NULL
    // -- execve(path_buf, argv, NULL) --
    emitter.instruction("add x0, sp, #312");                                    // path = &path_buf
    emitter.instruction("add x1, sp, #344");                                    // argv = &argv[0]
    emitter.instruction("mov x2, #0");                                          // envp = NULL
    if is_macos {
        emitter.instruction("mov x16, #59");                                    // macOS execve syscall number
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap
    } else {
        emitter.instruction("mov x8, #221");                                    // Linux execve syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
    }
    // -- execve returned (failure): exit the child with status 127 --
    emitter.instruction("mov x0, #127");                                        // child exit status for execve failure
    if is_macos {
        emitter.instruction("mov x16, #1");                                     // macOS exit syscall number
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap (does not return)
    } else {
        emitter.instruction("mov x8, #93");                                     // Linux exit syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap (does not return)
    }
    emitter.instruction("b __rt_proc_open_fail");                               // defensive fallthrough (never reached)

    // -- cleanup: close every parent_fd[j] and child_fd[j] for j < i where is_pipe[j] --
    emitter.label("__rt_proc_open_cleanup_sub");
    emitter.instruction("ldr x0, [sp, #72]");                                   // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release sub_box before cleanup
    emitter.instruction("b __rt_proc_open_cleanup");                            // proceed to close opened pipes
    emitter.label("__rt_proc_open_cleanup_m0");
    emitter.instruction("ldr x0, [sp, #88]");                                   // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release m0
    emitter.instruction("ldr x0, [sp, #72]");                                   // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release sub_box
    emitter.instruction("b __rt_proc_open_cleanup");                            // proceed to close opened pipes
    emitter.label("__rt_proc_open_cleanup_m1");
    emitter.instruction("ldr x0, [sp, #96]");                                   // m1
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release m1
    emitter.instruction("ldr x0, [sp, #88]");                                   // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release m0
    emitter.instruction("ldr x0, [sp, #72]");                                   // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release sub_box
    emitter.instruction("b __rt_proc_open_cleanup");                            // proceed to close opened pipes
    emitter.label("__rt_proc_open_cleanup");
    emitter.instruction("str xzr, [sp, #400]");                                 // cleanup index j = 0
    emitter.label("__rt_proc_open_cleanup_test");
    emitter.instruction("ldr x9, [sp, #400]");                                  // reload cleanup j
    emitter.instruction("ldr x10, [sp, #64]");                                  // bound = i (count of opened pipes)
    emitter.instruction("cmp x9, x10");                                         // j < i?
    emitter.instruction("b.ge __rt_proc_open_fail");                            // all opened pipes closed -> fail
    emitter.instruction("add x11, sp, #248");                                   // base of is_pipe
    emitter.instruction("ldr x12, [x11, x9, lsl #3]");                          // is_pipe[j]
    emitter.instruction("cbz x12, __rt_proc_open_cleanup_next");                // not a pipe -> skip
    emitter.instruction("add x11, sp, #120");                                   // base of parent_fd
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // parent_fd[j]
    if is_macos {
        emitter.instruction("mov x16, #6");                                     // macOS close syscall number (SYS_close=6)
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap
    } else {
        emitter.instruction("mov x8, #57");                                     // Linux close syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
    }
    emitter.instruction("ldr x9, [sp, #400]");                                  // reload cleanup j (svc preserves x9)
    emitter.instruction("add x11, sp, #184");                                   // base of child_fd
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // child_fd[j]
    if is_macos {
        emitter.instruction("mov x16, #6");                                     // macOS close syscall number (SYS_close=6)
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap
    } else {
        emitter.instruction("mov x8, #57");                                     // Linux close syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
    }
    emitter.label("__rt_proc_open_cleanup_next");
    emitter.instruction("ldr x9, [sp, #400]");                                  // reload cleanup j
    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("str x9, [sp, #400]");                                  // persist cleanup j
    emitter.instruction("b __rt_proc_open_cleanup_test");                       // continue cleanup

    // -- success: return the child pid --
    emitter.label("__rt_proc_open_done");
    emitter.instruction("ldr x0, [sp, #392]");                                  // reload the saved child pid
    emitter.instruction("ldr x1, [sp, #40]");                                   // return the final pipes array/hash pointer beside the pid
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #496");                                    // release the frame
    emitter.instruction("ret");                                                 // return the pid (lowerer boxes it)

    // -- failure: return -1 (lowerer boxes as PHP false) --
    emitter.label("__rt_proc_open_fail");
    emitter.instruction("mov x0, #-1");                                         // report proc_open failure
    emitter.instruction("ldr x1, [sp, #40]");                                   // return the original/current pipes container on failure
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #496");                                    // release the frame
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// Emits the Linux-x86_64 `__rt_proc_open` runtime (System V AMD64).
///
/// 480-byte `rbp`-relative frame. The kernel clobbers `rcx`/`r11` on syscall,
/// so loop counters live in frame slots and are reloaded after every trap.
/// Helper ABIs: `array_get_mixed_key(rdi/rsi/rdx/rcx -> rax)`,
/// `mixed_unbox(rax -> rax=tag, rdi=lo, rdx=hi)`, `str_eq(rdi/rsi/rdx/rcx -> rax)`,
/// `mixed_from_value(rax/rdi/rsi -> rax)`, `push_refcounted(rdi/rsi -> rax)`,
/// `decref_mixed(rax)`, `cstr(rax=ptr, rdx=len -> rax)`.
fn emit_proc_open_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc_open (C1b pipe-only) ---");
    emitter.label_global("__rt_proc_open");

    // -- prologue: rbp frame, 480 bytes (16-byte aligned) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 480");                                        // reserve the proc_open frame
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the descriptor_spec array pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the command string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the command string length
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the pipes array pointer

    // -- validate descriptor_spec: non-null + indexed kind + 1 <= n <= 8 --
    emitter.instruction("test rdi, rdi");                                       // null descriptor_spec?
    emitter.instruction("jz __rt_proc_open_fail_x86");                          // a null descriptor_spec is unrecoverable
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load the packed array kind metadata
    emitter.instruction("and r9, 0xff");                                        // isolate the low-byte storage kind
    emitter.instruction("cmp r9, 2");                                           // kind 2 = indexed array?
    emitter.instruction("je __rt_proc_open_kind_ok_x86");                       // accept indexed-array descriptor_spec
    emitter.instruction("cmp r9, 3");                                           // kind 3 = hash storage?
    emitter.instruction("jne __rt_proc_open_fail_x86");                         // non-array descriptor_spec is unsupported
    emitter.label("__rt_proc_open_kind_ok_x86");
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // read the descriptor_spec length
    emitter.instruction("test r9, r9");                                         // an empty descriptor spec is invalid
    emitter.instruction("jz __rt_proc_open_fail_x86");                          // bail out on an empty descriptor spec
    emitter.instruction("cmp r9, 8");                                           // C1b bounds the descriptor count at 8
    emitter.instruction("ja __rt_proc_open_fail_x86");                          // refuse an over-long descriptor spec
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the descriptor count n
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // reload descriptor storage metadata after the count read
    emitter.instruction("and r9, 0xff");                                        // isolate indexed/hash storage kind for position iteration
    emitter.instruction("mov QWORD PTR [rbp - 480], r9");                       // retain descriptor storage kind across runtime helper calls
    emitter.instruction("mov QWORD PTR [rbp - 472], 0");                        // start the hash insertion-order cursor at its head

    // -- zero the is_pipe bookkeeping array so cleanup is safe before any pipe opens --
    emitter.instruction("mov QWORD PTR [rbp - 400], 0");                        // is_pipe[0] = 0
    emitter.instruction("mov QWORD PTR [rbp - 392], 0");                        // is_pipe[1] = 0
    emitter.instruction("mov QWORD PTR [rbp - 384], 0");                        // is_pipe[2] = 0
    emitter.instruction("mov QWORD PTR [rbp - 376], 0");                        // is_pipe[3] = 0
    emitter.instruction("mov QWORD PTR [rbp - 368], 0");                        // is_pipe[4] = 0
    emitter.instruction("mov QWORD PTR [rbp - 360], 0");                        // is_pipe[5] = 0
    emitter.instruction("mov QWORD PTR [rbp - 352], 0");                        // is_pipe[6] = 0
    emitter.instruction("mov QWORD PTR [rbp - 344], 0");                        // is_pipe[7] = 0

    // -- main descriptor loop: for i = 0 .. n-1, open one pipe per descriptor --
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // i = 0
    emitter.label("__rt_proc_open_loop_test_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the loop index
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload n
    emitter.instruction("cmp r9, r10");                                         // i < n?
    emitter.instruction("jae __rt_proc_open_fork_x86");                         // descriptor loop complete -> fork

    // -- resolve position i to its real integer descriptor key, then read it --
    emitter.instruction("cmp QWORD PTR [rbp - 480], 3");                        // descriptor spec is associative hash storage?
    emitter.instruction("jne __rt_proc_open_indexed_key_x86");                  // indexed positions are their integer keys
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // hash descriptor-spec pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 472]");                      // insertion-order hash cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    emitter.instruction("mov QWORD PTR [rbp - 472], rax");                      // preserve the returned hash cursor
    emitter.instruction("cmp rdx, -1");                                         // only integer descriptor keys are valid
    emitter.instruction("jne __rt_proc_open_fail_x86");                         // string keys cannot name child descriptors
    emitter.instruction("mov QWORD PTR [rbp - 128], rdi");                      // retain the actual descriptor integer key for this loop position
    emitter.instruction("jmp __rt_proc_open_descriptor_key_ready_x86");         // skip indexed key synthesis
    emitter.label("__rt_proc_open_indexed_key_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // indexed position is its descriptor integer key
    emitter.instruction("mov QWORD PTR [rbp - 128], r9");                       // retain the descriptor integer key for this loop position
    emitter.label("__rt_proc_open_descriptor_key_ready_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // descriptor_spec array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 128]");                      // key = actual descriptor integer key
    emitter.instruction("mov rdx, -1");                                         // int-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the owned sub_box across unbox

    // -- unbox sub_box: expect an indexed array (runtime tag 4) --
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // rax=tag, rdi=lo (sub ptr), rdx=hi
    emitter.instruction("cmp rax, 4");                                          // tag 4 = indexed array?
    emitter.instruction("jne __rt_proc_open_cleanup_sub_x86");                  // non-array descriptor -> cleanup + fail
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save the sub-array pointer for element reads

    // -- read sub[0] (the descriptor type string) as an owned box --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // sub-array pointer
    emitter.instruction("xor esi, esi");                                        // key = 0
    emitter.instruction("mov rdx, -1");                                         // int-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save m0 (descriptor type string box)

    // -- unbox m0: expect a string (runtime tag 1) --
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // rax=tag, rdi=ptr, rdx=len
    emitter.instruction("cmp rax, 1");                                          // tag 1 = string?
    emitter.instruction("jne __rt_proc_open_cleanup_m0_x86");                   // non-string descriptor type -> cleanup + fail

    // -- build the literal "pipe" on the stack and compare against sub[0] --
    emitter.instruction("mov BYTE PTR [rbp - 136], 0x70");                      // lit_pipe[0] = 'p'
    emitter.instruction("mov BYTE PTR [rbp - 135], 0x69");                      // lit_pipe[1] = 'i'
    emitter.instruction("mov BYTE PTR [rbp - 134], 0x70");                      // lit_pipe[2] = 'p'
    emitter.instruction("mov BYTE PTR [rbp - 133], 0x65");                      // lit_pipe[3] = 'e'
    // __rt_str_eq(ptr_a, len_a, ptr_b, len_b): rdi=ptr (from unbox), rsi=len (move from rdx)
    emitter.instruction("mov rsi, rdx");                                        // len_a = string length from unbox
    emitter.instruction("lea rdx, [rbp - 136]");                                // ptr_b = &lit_pipe
    emitter.instruction("mov rcx, 4");                                          // len_b = 4
    abi::emit_call_label(emitter, "__rt_str_eq");                               // rax = 1 if "pipe" else 0
    emitter.instruction("test rax, rax");                                       // non-pipe descriptor unsupported in C1b?
    emitter.instruction("jz __rt_proc_open_cleanup_m0_x86");                    // non-pipe descriptor -> cleanup + fail

    // -- read sub[1] (the mode string) as an owned box --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // sub-array pointer
    emitter.instruction("mov esi, 1");                                          // key = 1
    emitter.instruction("mov rdx, -1");                                         // int-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // save m1 (mode string box)

    // -- unbox m1: strings select their first byte; PHP scalar coercions read --
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // rax=tag, rdi=ptr, rdx=len
    emitter.instruction("cmp rax, 1");                                          // tag 1 = string?
    emitter.instruction("je __rt_proc_open_string_mode_x86");                   // strings retain php-src's first-byte direction rule
    emitter.instruction("cmp rax, 0");                                          // tag 0 = integer scalar?
    emitter.instruction("je __rt_proc_open_scalar_mode_read_x86");              // decimal integer text cannot begin with `w`
    emitter.instruction("cmp rax, 2");                                          // tag 2 = float scalar?
    emitter.instruction("je __rt_proc_open_scalar_mode_read_x86");              // float text cannot begin with `w`
    emitter.instruction("cmp rax, 3");                                          // tag 3 = boolean scalar?
    emitter.instruction("je __rt_proc_open_scalar_mode_read_x86");              // boolean text cannot begin with `w`
    emitter.instruction("cmp rax, 8");                                          // tag 8 = null scalar?
    emitter.instruction("jne __rt_proc_open_cleanup_m1_x86");                   // non-scalar mode remains unsupported by this runtime
    emitter.label("__rt_proc_open_scalar_mode_read_x86");
    emitter.instruction("mov eax, 1");                                          // PHP scalar-to-string coercion produces a non-write-leading mode
    emitter.instruction("jmp __rt_proc_open_mode_direction_ready_x86");         // release owned boxes using the common direction path

    // -- match php-src: only a leading "w" changes the pipe direction --
    emitter.label("__rt_proc_open_string_mode_x86");
    emitter.instruction("test rdx, rdx");                                       // mode string is empty?
    emitter.instruction("jz __rt_proc_open_empty_mode_read_x86");               // empty modes use php-src's non-write/read direction
    emitter.instruction("cmp BYTE PTR [rdi], 0x77");                            // first byte is 'w'?
    emitter.instruction("setne al");                                            // is_read = first byte is not 'w'
    emitter.instruction("movzx rax, al");                                       // widen the direction flag for its stack slot
    emitter.instruction("jmp __rt_proc_open_mode_direction_ready_x86");         // retain the computed non-empty mode direction
    emitter.label("__rt_proc_open_empty_mode_read_x86");
    emitter.instruction("mov eax, 1");                                          // empty modes also make the child read
    emitter.label("__rt_proc_open_mode_direction_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save is_read in the sub_ptr slot (no longer needed)

    // -- release the three owned boxes now that is_read is known --
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // drop the caller's ref on m0
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // m1
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // drop the caller's ref on m1
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // drop the caller's ref on sub_box

    // -- open a pipe pair: pipe2(&pipe_fds, 0) --
    emitter.instruction("lea rdi, [rbp - 104]");                                // &pipe_fds[0]
    emitter.instruction("xor esi, esi");                                        // pipe2 flags = 0
    emitter.instruction("mov eax, 293");                                        // Linux x86_64 pipe2 syscall number
    emitter.instruction("syscall");                                             // Linux x86_64 trap (clobbers rcx/r11)
    emitter.instruction("test rax, rax");                                       // pipe2 retval < 0 means failure
    emitter.instruction("js __rt_proc_open_cleanup_x86");                       // pipe open failed -> cleanup opened pipes + fail

    // -- record the read/write ends per mode: child gets the mode end, parent the other --
    emitter.instruction("mov r10d, DWORD PTR [rbp - 104]");                     // read_end = pipe_fds[0] (32-bit int)
    emitter.instruction("mov r11d, DWORD PTR [rbp - 100]");                     // write_end = pipe_fds[1] (32-bit int)
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // i (descriptor index)
    emitter.instruction("mov r8, QWORD PTR [rbp - 80]");                        // reload is_read
    emitter.instruction("test r8, r8");                                         // a write-leading mode gives the child write_end
    emitter.instruction("jz __rt_proc_open_write_mode_x86");                    // is_read == 0 -> write mode
    emitter.instruction("lea rax, [rbp - 336]");                                // base of child_fd
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r10");                   // non-write-leading mode gives the child read_end
    emitter.instruction("lea rax, [rbp - 272]");                                // base of parent_fd
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r11");                   // non-write-leading mode gives the parent write_end
    emitter.instruction("jmp __rt_proc_open_pipe_recorded_x86");                // skip the write-mode assignment
    emitter.label("__rt_proc_open_write_mode_x86");
    emitter.instruction("lea rax, [rbp - 336]");                                // base of child_fd
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r11");                   // write-leading mode gives the child write_end
    emitter.instruction("lea rax, [rbp - 272]");                                // base of parent_fd
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r10");                   // write-leading mode gives the parent read_end
    emitter.label("__rt_proc_open_pipe_recorded_x86");
    emitter.instruction("lea rax, [rbp - 400]");                                // base of is_pipe
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], 1");                     // is_pipe[i] = 1
    emitter.instruction("lea rax, [rbp - 464]");                                // base of descriptor_index bookkeeping
    emitter.instruction("mov r10, QWORD PTR [rbp - 128]");                      // reload the real descriptor integer key
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r10");                   // descriptor_index[i] = real key for child/output phases

    // -- advance the loop index --
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload i
    emitter.instruction("inc r9");                                              // i += 1
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // persist the loop index
    emitter.instruction("jmp __rt_proc_open_loop_test_x86");                    // continue the descriptor loop

    // -- fork: Linux x86_64 fork = 57 (no args) --
    emitter.label("__rt_proc_open_fork_x86");
    emitter.instruction("mov eax, 57");                                         // Linux x86_64 fork syscall number
    emitter.instruction("syscall");                                             // Linux x86_64 trap (clobbers rcx/r11)
    emitter.instruction("test rax, rax");                                       // fork retval < 0 means failure
    emitter.instruction("js __rt_proc_open_cleanup_x86");                       // fork failed -> close all opened pipes + fail
    emitter.instruction("jz __rt_proc_open_child_x86");                         // pid == 0 -> child branch
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // parent: save the child pid

    // -- parent: close every child end, then push the parent ends into $pipes --
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // j = 0
    emitter.label("__rt_proc_open_parent_close_test_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload n
    emitter.instruction("cmp r9, r10");                                         // j < n?
    emitter.instruction("jae __rt_proc_open_parent_push_x86");                  // all child ends closed -> push phase
    emitter.instruction("lea r11, [rbp - 400]");                                // base of is_pipe
    emitter.instruction("mov r8, QWORD PTR [r11 + r9 * 8]");                    // is_pipe[j]
    emitter.instruction("test r8, r8");                                         // not a pipe?
    emitter.instruction("jz __rt_proc_open_parent_close_next_x86");             // skip non-pipe descriptors
    emitter.instruction("lea r11, [rbp - 336]");                                // base of child_fd
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // child_fd[j]
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 close syscall number
    emitter.instruction("syscall");                                             // close the child end (clobbers rcx/r11)
    emitter.label("__rt_proc_open_parent_close_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("inc r9");                                              // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // persist j
    emitter.instruction("jmp __rt_proc_open_parent_close_test_x86");            // continue closing child ends

    // -- parent publish phase: write each parent end at its descriptor key --
    emitter.label("__rt_proc_open_parent_push_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // j = 0
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // pipe_count = 0
    emitter.label("__rt_proc_open_parent_push_test_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload n
    emitter.instruction("cmp r9, r10");                                         // j < n?
    emitter.instruction("jae __rt_proc_open_done_x86");                         // all pipes pushed -> return pid
    emitter.instruction("lea r11, [rbp - 400]");                                // base of is_pipe
    emitter.instruction("mov r8, QWORD PTR [r11 + r9 * 8]");                    // is_pipe[j]
    emitter.instruction("test r8, r8");                                         // not a pipe?
    emitter.instruction("jz __rt_proc_open_parent_push_next_x86");              // skip non-pipe descriptors
    emitter.instruction("lea r11, [rbp - 272]");                                // base of parent_fd
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // parent_fd[j] (resource handle lo)
    emitter.instruction("mov rax, 9");                                          // tag 9 = resource
    emitter.instruction("mov rsi, 1");                                          // hi = kind 1 (native stream fd)
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // rax = res_box (owned, refcount 1)
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the owned resource box for the keyed container write
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j after the allocating Mixed helper clobbered caller-saved registers
    emitter.instruction("lea r11, [rbp - 464]");                                // base of descriptor_index bookkeeping
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // real integer descriptor key for this parent pipe
    emitter.instruction("xor esi, esi");                                        // integer key has no high payload word
    emitter.instruction("xor eax, eax");                                        // tag 0 = integer key Mixed cell
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // allocate the borrowed key cell used by the generic setter
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // retain the key cell across the container write
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the current pipes array/hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // pass the integer descriptor key cell
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // transfer the resource box into the destination container
    abi::emit_call_label(emitter, "__rt_array_set_mixed_key");                  // set resource at the real descriptor key, returning promoted/reallocated container
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the potentially promoted/reallocated pipes container
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the borrowed key cell after the setter read it
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the caller-owned key box; the setter never owns keys
    emitter.label("__rt_proc_open_parent_push_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("inc r9");                                              // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // persist j
    emitter.instruction("jmp __rt_proc_open_parent_push_test_x86");             // continue pushing parent ends

    // -- child: dup2 each child end onto descriptor fd i, close the parent ends, execve --
    emitter.label("__rt_proc_open_child_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // j = 0
    emitter.label("__rt_proc_open_child_dup_test_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload n
    emitter.instruction("cmp r9, r10");                                         // j < n?
    emitter.instruction("jae __rt_proc_open_child_close_phase_x86");            // all dups done -> close parent ends
    emitter.instruction("lea r11, [rbp - 400]");                                // base of is_pipe
    emitter.instruction("mov r8, QWORD PTR [r11 + r9 * 8]");                    // is_pipe[j]
    emitter.instruction("test r8, r8");                                         // not a pipe?
    emitter.instruction("jz __rt_proc_open_child_dup_next_x86");                // skip non-pipe descriptors
    emitter.instruction("lea r11, [rbp - 336]");                                // base of child_fd
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // child_fd[j] (old fd)
    emitter.instruction("lea r11, [rbp - 464]");                                // base of descriptor_index bookkeeping
    emitter.instruction("mov rsi, QWORD PTR [r11 + r9 * 8]");                   // new fd = real descriptor integer key
    emitter.instruction("mov eax, 33");                                         // Linux x86_64 dup2 syscall number
    emitter.instruction("syscall");                                             // dup2 (clobbers rcx/r11)
    // -- if child_fd[j] != j, close the now-redundant original child end --
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j (rcx clobbered by syscall)
    emitter.instruction("lea r11, [rbp - 336]");                                // base of child_fd
    emitter.instruction("mov r8, QWORD PTR [r11 + r9 * 8]");                    // reload child_fd[j]
    emitter.instruction("lea r11, [rbp - 464]");                                // base of descriptor_index bookkeeping
    emitter.instruction("mov r10, QWORD PTR [r11 + r9 * 8]");                   // reload the real descriptor integer key
    emitter.instruction("cmp r8, r10");                                         // child_fd[j] == descriptor key?
    emitter.instruction("je __rt_proc_open_child_dup_next_x86");                // equal -> no close needed (fd is already the dup target)
    emitter.instruction("mov rdi, r8");                                         // close(child_fd[j])
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 close syscall number
    emitter.instruction("syscall");                                             // close the redundant child end
    emitter.label("__rt_proc_open_child_dup_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("inc r9");                                              // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // persist j
    emitter.instruction("jmp __rt_proc_open_child_dup_test_x86");               // continue the dup loop

    // -- child: close every parent end so the child does not hold the parent's pipe side --
    emitter.label("__rt_proc_open_child_close_phase_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // j = 0
    emitter.label("__rt_proc_open_child_close_test_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload n
    emitter.instruction("cmp r9, r10");                                         // j < n?
    emitter.instruction("jae __rt_proc_open_child_exec_x86");                   // all parent ends closed -> execve
    emitter.instruction("lea r11, [rbp - 400]");                                // base of is_pipe
    emitter.instruction("mov r8, QWORD PTR [r11 + r9 * 8]");                    // is_pipe[j]
    emitter.instruction("test r8, r8");                                         // not a pipe?
    emitter.instruction("jz __rt_proc_open_child_close_next_x86");              // skip non-pipe descriptors
    emitter.instruction("lea r11, [rbp - 272]");                                // base of parent_fd
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // parent_fd[j]
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 close syscall number
    emitter.instruction("syscall");                                             // close the parent end
    emitter.label("__rt_proc_open_child_close_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("inc r9");                                              // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // persist j
    emitter.instruction("jmp __rt_proc_open_child_close_test_x86");             // continue closing parent ends

    // -- child: build the execve payload and exec /bin/sh -c <command> --
    emitter.label("__rt_proc_open_child_exec_x86");
    // __rt_cstr takes ptr in rax and len in rdx; command lives in rsi/rdx currently
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // command pointer into __rt_cstr input
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // command length into __rt_cstr input
    abi::emit_call_label(emitter, "__rt_cstr");                                 // rax = null-terminated command string
    emitter.instruction("mov QWORD PTR [rbp - 176], rax");                      // save cmd_cstr for argv[2]
    // -- store "/bin/sh\0" into path_buf --
    emitter.instruction("mov BYTE PTR [rbp - 152], 0x2f");                      // path_buf[0] = '/'
    emitter.instruction("mov BYTE PTR [rbp - 151], 0x62");                      // path_buf[1] = 'b'
    emitter.instruction("mov BYTE PTR [rbp - 150], 0x69");                      // path_buf[2] = 'i'
    emitter.instruction("mov BYTE PTR [rbp - 149], 0x6e");                      // path_buf[3] = 'n'
    emitter.instruction("mov BYTE PTR [rbp - 148], 0x2f");                      // path_buf[4] = '/'
    emitter.instruction("mov BYTE PTR [rbp - 147], 0x73");                      // path_buf[5] = 's'
    emitter.instruction("mov BYTE PTR [rbp - 146], 0x68");                      // path_buf[6] = 'h'
    emitter.instruction("mov BYTE PTR [rbp - 145], 0");                         // path_buf[7] = NUL
    // -- store "sh\0" into argv0 --
    emitter.instruction("mov BYTE PTR [rbp - 160], 0x73");                      // argv0[0] = 's'
    emitter.instruction("mov BYTE PTR [rbp - 159], 0x68");                      // argv0[1] = 'h'
    emitter.instruction("mov BYTE PTR [rbp - 158], 0");                         // argv0[2] = NUL
    // -- store "-c\0" into argv1 --
    emitter.instruction("mov BYTE PTR [rbp - 168], 0x2d");                      // argv1[0] = '-'
    emitter.instruction("mov BYTE PTR [rbp - 167], 0x63");                      // argv1[1] = 'c'
    emitter.instruction("mov BYTE PTR [rbp - 166], 0");                         // argv1[2] = NUL
    // -- build argv[4] = { &argv0, &argv1, cmd_cstr, NULL } --
    emitter.instruction("lea r9, [rbp - 160]");                                 // &argv0
    emitter.instruction("mov QWORD PTR [rbp - 208], r9");                       // argv[0] = &argv0
    emitter.instruction("lea r9, [rbp - 168]");                                 // &argv1
    emitter.instruction("mov QWORD PTR [rbp - 200], r9");                       // argv[1] = &argv1
    emitter.instruction("mov r9, QWORD PTR [rbp - 176]");                       // cmd_cstr
    emitter.instruction("mov QWORD PTR [rbp - 192], r9");                       // argv[2] = cmd_cstr
    emitter.instruction("mov QWORD PTR [rbp - 184], 0");                        // argv[3] = NULL
    // -- execve(path_buf, argv, NULL) --
    emitter.instruction("lea rdi, [rbp - 152]");                                // path = &path_buf
    emitter.instruction("lea rsi, [rbp - 208]");                                // argv = &argv[0]
    emitter.instruction("xor edx, edx");                                        // envp = NULL
    emitter.instruction("mov eax, 59");                                         // Linux x86_64 execve syscall number
    emitter.instruction("syscall");                                             // execve (does not return on success)
    // -- execve returned (failure): exit the child with status 127 --
    emitter.instruction("mov edi, 127");                                        // child exit status for execve failure
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 exit syscall number
    emitter.instruction("syscall");                                             // exit the child (does not return)
    emitter.instruction("jmp __rt_proc_open_fail_x86");                         // defensive fallthrough (never reached)

    // -- cleanup: release owned boxes for the mid-loop failure paths --
    emitter.label("__rt_proc_open_cleanup_sub_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release sub_box before cleanup
    emitter.instruction("jmp __rt_proc_open_cleanup_x86");                      // proceed to close opened pipes
    emitter.label("__rt_proc_open_cleanup_m0_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release m0
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release sub_box
    emitter.instruction("jmp __rt_proc_open_cleanup_x86");                      // proceed to close opened pipes
    emitter.label("__rt_proc_open_cleanup_m1_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // m1
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release m1
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release m0
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release sub_box
    emitter.instruction("jmp __rt_proc_open_cleanup_x86");                      // proceed to close opened pipes
    emitter.label("__rt_proc_open_cleanup_x86");
    emitter.instruction("mov QWORD PTR [rbp - 128], 0");                        // cleanup index j = 0
    emitter.label("__rt_proc_open_cleanup_test_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 128]");                       // reload cleanup j
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // bound = i (count of opened pipes)
    emitter.instruction("cmp r9, r10");                                         // j < i?
    emitter.instruction("jae __rt_proc_open_fail_x86");                         // all opened pipes closed -> fail
    emitter.instruction("lea r11, [rbp - 400]");                                // base of is_pipe
    emitter.instruction("mov r8, QWORD PTR [r11 + r9 * 8]");                    // is_pipe[j]
    emitter.instruction("test r8, r8");                                         // not a pipe?
    emitter.instruction("jz __rt_proc_open_cleanup_next_x86");                  // skip non-pipe descriptors
    emitter.instruction("lea r11, [rbp - 272]");                                // base of parent_fd
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // parent_fd[j]
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 close syscall number
    emitter.instruction("syscall");                                             // close parent_fd[j]
    emitter.instruction("mov r9, QWORD PTR [rbp - 128]");                       // reload cleanup j (rcx clobbered)
    emitter.instruction("lea r11, [rbp - 336]");                                // base of child_fd
    emitter.instruction("mov rdi, QWORD PTR [r11 + r9 * 8]");                   // child_fd[j]
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 close syscall number
    emitter.instruction("syscall");                                             // close child_fd[j]
    emitter.label("__rt_proc_open_cleanup_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 128]");                       // reload cleanup j
    emitter.instruction("inc r9");                                              // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 128], r9");                       // persist cleanup j
    emitter.instruction("jmp __rt_proc_open_cleanup_test_x86");                 // continue cleanup

    // -- success: return the child pid --
    emitter.label("__rt_proc_open_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 120]");                      // reload the saved child pid
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the final pipes array/hash pointer beside the pid
    emitter.instruction("add rsp, 480");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the pid (lowerer boxes it)

    // -- failure: return -1 (lowerer boxes as PHP false) --
    emitter.label("__rt_proc_open_fail_x86");
    emitter.instruction("mov rax, -1");                                         // report proc_open failure
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the original/current pipes container on failure
    emitter.instruction("add rsp, 480");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// Emits the Windows-x86_64 `__rt_proc_open` runtime (real C1c: `CreatePipe` +
/// `CreateProcessW`, MSx64 ABI).
///
/// 896-byte `rbp`-relative frame (persistent data in `[16, 800)`, the last 96
/// bytes `[800, 896)` are outgoing-call shadow space + stack args, addressed
/// via `[rsp + K]`). Offsets from `rbp`: `[16]` desc, `[24]` cmd_ptr, `[32]`
/// cmd_len, `[40]` pipes, `[48]` n, `[56]` scratch (cmdbuf total size late in
/// the function; unused earlier), `[64]` i/j (reused per phase), `[72]`
/// sub_box, `[80]` sub_ptr (reused for `is_read`), `[88]` m0, `[96]` m1,
/// `[104]`/`[112]` CreatePipe read_end/write_end, `[120]` lit_pipe, `[128]`
/// lit_r, `[136]` cmdbuf pointer, `[144]`/`[152]`/`[160]` NUL handles for
/// stdin/stdout/stderr, `[168, 232)` parent_handle\[8\] (element `j` at
/// `rbp - 224 + 8*j`; raw HANDLEs until publication, then CRT fds),
/// `[232, 296)` child_handle\[8\] (element `j` at `rbp - 288 + 8*j`),
/// `[296, 360)` pipe_state\[8\] (element `j` at `rbp - 352 + 8*j`; bit 0
/// means pipe open, bit 1 means parent-readable, bit 2 means CRT-adopted,
/// bit 3 means raw Winsock socket pair, and bit 4 means child-only handle),
/// `[360, 384)` SECURITY_ATTRIBUTES, `[384, 488)`
/// STARTUPINFOW, `[488, 512)` PROCESS_INFORMATION, `[512, 520)` a 4-byte
/// "NUL\0" literal (reused as the cleanup-loop counter, slot name
/// `cleanup_j`, once the descriptor loop and NUL redirection are behind us),
/// `[528]` cwd pointer, `[536]` cwd length, `[544]` owned narrow cwd staging,
/// `[552]` owned UTF-16 cwd, `[616]` saved process error mode, and `[624]`
/// temporary `GetLastError()` result. The transported env/options pointers occupy
/// SysV stack arguments 7/8; options use the five low bits of the packed flag
/// word and environment length begins at bit five. `[728, 792)` retains the
/// eight real descriptor integer keys, `[792]` is the associative iterator
/// cursor, `[800]` is the descriptor storage kind, and `[704]`/`[712]` retain
/// a temporary NUL-terminated UTF-8 file path and its allocation size. Keeping those slots
/// below the environment state prevents descriptor key 7 from aliasing the
/// owned UTF-16 environment pointer.
///
/// Descriptor-spec parsing validates real integer keys and accepts `pipe`,
/// `null`, `redirect`, `file`, and stream-resource descriptors. `pipe` owns a
/// parent endpoint that is later converted to a CRT fd and published in `$pipes`;
/// all other descriptors only own the inheritable child handle and therefore
/// close it immediately after `CreateProcessW` succeeds. File paths are strictly
/// converted from UTF-8 and opened with `CreateFileW`; stream resources use
/// `_get_osfhandle` followed by `DuplicateHandle` so the caller retains ownership.
/// `socket` uses a private AF_INET loopback pair so the public AF_UNIX
/// `socketpair` API remains unsupported; `pty` remains outside this helper.
/// Parsing (validate / unbox / `"pipe"`/`"r"` compare /
/// decref) is a byte-for-byte copy of `emit_proc_open_linux_x86_64`'s SysV
/// internal-helper calls (`rdi`/`rsi`/`rdx`/`rcx`), since those helpers are
/// emitted once for x86_64 and shared by every platform (see
/// `array_get_mixed_key.rs`, `mixed_unbox.rs`, `str_eq.rs`,
/// `mixed_from_value.rs`, `array_push_refcounted.rs`, `decref_mixed.rs`).
/// Only the pipe/spawn mechanism differs: `CreatePipe` replaces `pipe2`,
/// `STARTUPINFOW.hStd*` replaces `dup2`, and `CreateProcessW` replaces
/// `fork`/`execve`. No register is ever trusted to survive a `call` (SysV
/// helper or Win32 API): every value is reloaded from its `rbp`-relative slot
/// immediately afterward, since Win32 volatile registers (`rax`/`rcx`/`rdx`/
/// `r8`-`r11`) differ from the SysV internal-helper convention.
///
/// Mode mapping mirrors php-src: a leading `w` (the child writes, e.g.
/// stdout/stderr) is the reverse; every other first byte (including `r`/`rb`)
/// puts the pipe's read end in the child and the write end in the parent. Only descriptor
/// indices 0/1/2 are wired into `STARTUPINFOW`; indices `>= 3` still get a
/// real, inheritable pipe end pushed into `$pipes`, but the child has no
/// numbered-fd convention to receive it on Windows (documented C1c
/// limitation, consistent with C1b's own descriptor-count bound).
fn emit_proc_open_win32_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc_open (C1c: CreatePipe/CreateProcessW) ---");
    emitter.label_global("__rt_proc_open");

    // -- prologue: rbp frame, 896 bytes (persistent 800B + 96B call scratch) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 896");                                        // reserve persistent state, descriptor keys, and MSx64 call scratch
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the descriptor_spec array pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the command string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the command string length
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the pipes array pointer
    emitter.instruction("mov QWORD PTR [rbp - 528], r8");                       // save optional cwd string pointer
    emitter.instruction("mov QWORD PTR [rbp - 536], r9");                       // save optional cwd string length
    emitter.instruction("mov QWORD PTR [rbp - 544], 0");                        // cwd narrow staging buffer = NULL
    emitter.instruction("mov QWORD PTR [rbp - 552], 0");                        // cwd UTF-16 buffer = NULL
    emitter.instruction("mov rax, QWORD PTR [rbp + 16]");                       // load arg7: optional UTF-8 environment block pointer
    emitter.instruction("mov QWORD PTR [rbp - 560], rax");                      // preserve the environment block pointer
    emitter.instruction("mov rax, QWORD PTR [rbp + 24]");                       // load arg8: packed environment length/direct flag
    emitter.instruction("mov QWORD PTR [rbp - 568], rax");                      // preserve the packed proc_open flags
    emitter.instruction("mov QWORD PTR [rbp - 576], 0");                        // owned UTF-16 environment block = none yet
    emitter.instruction("mov QWORD PTR [rbp - 584], 0");                        // command-line prefix length = direct by default
    emitter.instruction("mov QWORD PTR [rbp - 592], 0");                        // command-line suffix length = direct by default
    emitter.instruction("mov QWORD PTR [rbp - 616], 0");                        // saved SetErrorMode value when suppression is enabled

    // -- reject an invalid dynamic options marker before acquiring resources --
    emitter.instruction("mov rax, QWORD PTR [rbp - 568]");                      // reload the packed marshalling flags
    emitter.instruction("test rax, rax");                                       // inspect the invalid-options sign bit
    emitter.instruction("js __rt_proc_open_fail_win");                          // runtime option validation already published errno

    // -- validate descriptor_spec: non-null + indexed kind + 1 <= n <= 8 --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the descriptor_spec array pointer
    emitter.instruction("test rdi, rdi");                                       // null descriptor_spec?
    emitter.instruction("jz __rt_proc_open_fail_win");                          // a null descriptor_spec is unrecoverable
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load the packed array kind metadata
    emitter.instruction("and r9, 0xff");                                        // isolate the low-byte storage kind
    emitter.instruction("cmp r9, 2");                                           // kind 2 = indexed array?
    emitter.instruction("je __rt_proc_open_kind_ok_win");                       // accept indexed-array descriptor_spec
    emitter.instruction("cmp r9, 3");                                           // kind 3 = hash storage?
    emitter.instruction("jne __rt_proc_open_fail_win");                         // non-array descriptor_spec is unsupported
    emitter.label("__rt_proc_open_kind_ok_win");
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // read the descriptor_spec length
    emitter.instruction("test r9, r9");                                         // an empty descriptor spec is invalid
    emitter.instruction("jz __rt_proc_open_fail_win");                          // bail out on an empty descriptor spec
    emitter.instruction("cmp r9, 8");                                           // C1c bounds the descriptor count at 8
    emitter.instruction("ja __rt_proc_open_fail_win");                          // refuse an over-long descriptor spec
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the descriptor count n
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // reload descriptor storage metadata after reading n
    emitter.instruction("and r9, 0xff");                                        // isolate indexed/hash storage kind for position iteration
    emitter.instruction("mov QWORD PTR [rbp - 800], r9");                       // retain descriptor storage kind across runtime helper calls
    emitter.instruction("mov QWORD PTR [rbp - 792], 0");                        // begin associative iteration at its insertion-order cursor

    // -- zero the is_pipe bookkeeping array so cleanup is safe before any pipe opens --
    emitter.instruction("mov QWORD PTR [rbp - 296], 0");                        // is_pipe[0] = 0
    emitter.instruction("mov QWORD PTR [rbp - 304], 0");                        // is_pipe[1] = 0
    emitter.instruction("mov QWORD PTR [rbp - 312], 0");                        // is_pipe[2] = 0
    emitter.instruction("mov QWORD PTR [rbp - 320], 0");                        // is_pipe[3] = 0
    emitter.instruction("mov QWORD PTR [rbp - 328], 0");                        // is_pipe[4] = 0
    emitter.instruction("mov QWORD PTR [rbp - 336], 0");                        // is_pipe[5] = 0
    emitter.instruction("mov QWORD PTR [rbp - 344], 0");                        // is_pipe[6] = 0
    emitter.instruction("mov QWORD PTR [rbp - 352], 0");                        // is_pipe[7] = 0

    // -- zero the NUL-handle and cmdbuf slots so failure cleanup can safely skip them --
    emitter.instruction("mov QWORD PTR [rbp - 144], 0");                        // nul_handle[0] (stdin) = none yet
    emitter.instruction("mov QWORD PTR [rbp - 152], 0");                        // nul_handle[1] (stdout) = none yet
    emitter.instruction("mov QWORD PTR [rbp - 160], 0");                        // nul_handle[2] (stderr) = none yet
    emitter.instruction("mov QWORD PTR [rbp - 136], 0");                        // cmdbuf = NULL (not yet allocated)
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // wide_cmdbuf = NULL (not yet converted)

    // -- build the reusable heritable SECURITY_ATTRIBUTES (sa): 24 bytes --
    emitter.instruction("mov QWORD PTR [rbp - 384], 24");                       // sa.nLength = sizeof(SECURITY_ATTRIBUTES)
    emitter.instruction("mov QWORD PTR [rbp - 376], 0");                        // sa.lpSecurityDescriptor = NULL
    emitter.instruction("mov QWORD PTR [rbp - 368], 1");                        // sa.bInheritHandle = TRUE (pipe ends are born inheritable)

    // -- zero-init STARTUPINFOW (104 bytes: 13 QWORDs), then set cb and dwFlags --
    emitter.instruction("mov QWORD PTR [rbp - 488], 0");                        // si bytes [0, 8) = 0 (cb + reserved)
    emitter.instruction("mov QWORD PTR [rbp - 392], 0");                        // si bytes [96, 104) = 0
    emitter.instruction("mov QWORD PTR [rbp - 400], 0");                        // si bytes [88, 96) = 0
    emitter.instruction("mov QWORD PTR [rbp - 408], 0");                        // si bytes [80, 88) = 0
    emitter.instruction("mov QWORD PTR [rbp - 416], 0");                        // si bytes [72, 80) = 0
    emitter.instruction("mov QWORD PTR [rbp - 424], 0");                        // si bytes [64, 72) = 0
    emitter.instruction("mov QWORD PTR [rbp - 432], 0");                        // si bytes [56, 64) = 0 (includes dwFlags)
    emitter.instruction("mov QWORD PTR [rbp - 440], 0");                        // si bytes [48, 56) = 0
    emitter.instruction("mov QWORD PTR [rbp - 448], 0");                        // si bytes [40, 48) = 0
    emitter.instruction("mov QWORD PTR [rbp - 456], 0");                        // si bytes [32, 40) = 0
    emitter.instruction("mov QWORD PTR [rbp - 464], 0");                        // si bytes [24, 32) = 0
    emitter.instruction("mov QWORD PTR [rbp - 472], 0");                        // si bytes [16, 24) = 0
    emitter.instruction("mov QWORD PTR [rbp - 480], 0");                        // si bytes [8, 16) = 0
    emitter.instruction("mov DWORD PTR [rbp - 488], 104");                      // si.cb = sizeof(STARTUPINFOW)
    emitter.instruction("mov DWORD PTR [rbp - 428], 0x100");                    // si.dwFlags = STARTF_USESTDHANDLES

    // -- main descriptor loop: for i = 0 .. n-1, open one pipe per descriptor --
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // i = 0
    emitter.label("__rt_proc_open_loop_test_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the loop index
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload n
    emitter.instruction("cmp r9, r10");                                         // i < n?
    emitter.instruction("jae __rt_proc_open_nul_fill_win");                     // descriptor loop complete -> fill unwired std handles

    // -- resolve position i to its real integer descriptor key, then read it --
    emitter.instruction("cmp QWORD PTR [rbp - 800], 3");                        // descriptor spec uses associative hash storage?
    emitter.instruction("jne __rt_proc_open_indexed_key_win");                  // packed positions are their integer keys
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // associative descriptor-spec pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 792]");                      // insertion-order hash cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    emitter.instruction("mov QWORD PTR [rbp - 792], rax");                      // preserve the returned hash cursor
    emitter.instruction("cmp rdx, -1");                                         // only integer descriptor keys are supported by proc_open
    emitter.instruction("jne __rt_proc_open_fail_win");                         // reject string descriptor keys before opening a pipe
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload position after the iterator call
    emitter.instruction("lea r11, [rbp - 784]");                                // base of real descriptor-key bookkeeping
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rdi");                  // retain this actual integer descriptor key
    emitter.instruction("jmp __rt_proc_open_descriptor_key_ready_win");         // skip packed-key synthesis
    emitter.label("__rt_proc_open_indexed_key_win");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // packed position is its descriptor key
    emitter.instruction("lea r11, [rbp - 784]");                                // base of real descriptor-key bookkeeping
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], r10");                  // retain the packed integer descriptor key
    emitter.label("__rt_proc_open_descriptor_key_ready_win");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload position for the keyed mixed lookup
    emitter.instruction("lea r11, [rbp - 784]");                                // base of real descriptor-key bookkeeping
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10 * 8]");                  // key = actual descriptor integer key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // descriptor_spec array pointer
    emitter.instruction("mov rdx, -1");                                         // int-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the owned sub_box across unbox

    // -- unbox sub_box: an indexed descriptor array or a stream resource --
    abi::emit_call_label(emitter, "__rt_mixed_unbox"); // rax=tag, rdi=lo (sub ptr), rdx=hi
    emitter.instruction("cmp rax, 9");                                          // tag 9 = resource descriptor?
    emitter.instruction("je __rt_proc_open_resource_descriptor_win");           // duplicate a stream resource for the child
    emitter.instruction("cmp rax, 4");                                          // tag 4 = indexed array?
    emitter.instruction("jne __rt_proc_open_cleanup_sub_win");                  // non-array descriptor -> cleanup + fail
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save the sub-array pointer for element reads

    // -- read sub[0] (the descriptor type string) as an owned box --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // sub-array pointer
    emitter.instruction("xor esi, esi");                                        // key = 0
    emitter.instruction("mov rdx, -1");                                         // int-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save m0 (descriptor type string box)

    // -- unbox m0: expect a string (runtime tag 1) --
    abi::emit_call_label(emitter, "__rt_mixed_unbox"); // rax=tag, rdi=ptr, rdx=len
    emitter.instruction("cmp rax, 1");                                          // tag 1 = string?
    emitter.instruction("jne __rt_proc_open_cleanup_m0_win");                   // non-string descriptor type -> cleanup + fail

    // -- build the literal "pipe" on the stack and compare against sub[0] --
    emitter.instruction("mov BYTE PTR [rbp - 120], 0x70");                      // lit_pipe[0] = 'p'
    emitter.instruction("mov BYTE PTR [rbp - 119], 0x69");                      // lit_pipe[1] = 'i'
    emitter.instruction("mov BYTE PTR [rbp - 118], 0x70");                      // lit_pipe[2] = 'p'
    emitter.instruction("mov BYTE PTR [rbp - 117], 0x65");                      // lit_pipe[3] = 'e'
    // __rt_str_eq(ptr_a, len_a, ptr_b, len_b): rdi=ptr (from unbox), rsi=len (move from rdx)
    emitter.instruction("mov rsi, rdx");                                        // len_a = string length from unbox
    emitter.instruction("lea rdx, [rbp - 120]");                                // ptr_b = &lit_pipe
    emitter.instruction("mov rcx, 4");                                          // len_b = 4
    abi::emit_call_label(emitter, "__rt_str_eq"); // rax = 1 if "pipe" else 0
    emitter.instruction("test rax, rax");                                       // did the type match pipe?
    emitter.instruction("jz __rt_proc_open_nonpipe_descriptor_win");            // dispatch another PHP descriptor kind

    // -- read sub[1] (the mode string) as an owned box --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // sub-array pointer
    emitter.instruction("mov esi, 1");                                          // key = 1
    emitter.instruction("mov rdx, -1");                                         // int-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-key warnings
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // save m1 (mode string box)

    // -- unbox m1: strings select their first byte; PHP scalar coercions read --
    abi::emit_call_label(emitter, "__rt_mixed_unbox"); // rax=tag, rdi=ptr, rdx=len
    emitter.instruction("cmp rax, 1");                                          // tag 1 = string?
    emitter.instruction("je __rt_proc_open_string_mode_win");                   // strings retain php-src's first-byte direction rule
    emitter.instruction("cmp rax, 0");                                          // tag 0 = integer scalar?
    emitter.instruction("je __rt_proc_open_scalar_mode_read_win");              // decimal integer text cannot begin with `w`
    emitter.instruction("cmp rax, 2");                                          // tag 2 = float scalar?
    emitter.instruction("je __rt_proc_open_scalar_mode_read_win");              // float text cannot begin with `w`
    emitter.instruction("cmp rax, 3");                                          // tag 3 = boolean scalar?
    emitter.instruction("je __rt_proc_open_scalar_mode_read_win");              // boolean text cannot begin with `w`
    emitter.instruction("cmp rax, 8");                                          // tag 8 = null scalar?
    emitter.instruction("jne __rt_proc_open_cleanup_m1_win");                   // non-scalar mode remains unsupported by this runtime
    emitter.label("__rt_proc_open_scalar_mode_read_win");
    emitter.instruction("mov eax, 1");                                          // PHP scalar-to-string coercion produces a non-write-leading mode
    emitter.instruction("jmp __rt_proc_open_mode_direction_ready_win");         // release owned boxes using the common direction path

    // -- match php-src: only a leading "w" changes the pipe direction --
    emitter.label("__rt_proc_open_string_mode_win");
    emitter.instruction("test rdx, rdx");                                       // mode string is empty?
    emitter.instruction("jz __rt_proc_open_empty_mode_read_win");               // empty modes use php-src's non-write/read direction
    emitter.instruction("cmp BYTE PTR [rdi], 0x77");                            // first byte is 'w'?
    emitter.instruction("setne al");                                            // is_read = first byte is not 'w'
    emitter.instruction("movzx rax, al");                                       // widen the direction flag for its stack slot
    emitter.instruction("jmp __rt_proc_open_mode_direction_ready_win");         // retain the computed non-empty mode direction
    emitter.label("__rt_proc_open_empty_mode_read_win");
    emitter.instruction("mov eax, 1");                                          // empty modes also make the child read
    emitter.label("__rt_proc_open_mode_direction_ready_win");
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save is_read in the sub_ptr slot (no longer needed)

    // -- release the three owned boxes now that is_read is known --
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // drop the caller's ref on m0
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // m1
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // drop the caller's ref on m1
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // drop the caller's ref on sub_box

    // -- open a pipe pair: CreatePipe(&read_end, &write_end, &sa, 0) --
    emitter.instruction("lea rcx, [rbp - 104]");                                // &read_end (out param)
    emitter.instruction("lea rdx, [rbp - 112]");                                // &write_end (out param)
    emitter.instruction("lea r8, [rbp - 384]");                                 // &sa (heritable security attributes)
    emitter.instruction("xor r9d, r9d");                                        // nSize = 0 (default pipe buffer size)
    emitter.instruction("call CreatePipe");                                     // BOOL in eax; fills read_end/write_end
    emitter.instruction("test eax, eax");                                       // CreatePipe failed?
    emitter.instruction("jz __rt_proc_open_capture_cleanup_win");               // capture errno, then cleanup opened pipes + fail

    // -- record pipe ends: only a write-leading mode makes the child write --
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                      // read_end
    emitter.instruction("mov r11, QWORD PTR [rbp - 112]");                      // write_end
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // i (descriptor index)
    emitter.instruction("mov r8, QWORD PTR [rbp - 80]");                        // reload is_read
    emitter.instruction("test r8, r8");                                         // a write-leading mode gives the child write_end
    emitter.instruction("jz __rt_proc_open_write_mode_win");                    // is_read == 0 -> write mode
    emitter.instruction("lea rax, [rbp - 288]");                                // base of child_handle
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r10");                   // non-write-leading mode gives the child read_end
    emitter.instruction("lea rax, [rbp - 224]");                                // base of parent_handle
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r11");                   // non-write-leading mode gives the parent write_end
    emitter.instruction("jmp __rt_proc_open_pipe_recorded_win");                // skip the write-mode assignment
    emitter.label("__rt_proc_open_write_mode_win");
    emitter.instruction("lea rax, [rbp - 288]");                                // base of child_handle
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r11");                   // write-leading mode gives the child write_end
    emitter.instruction("lea rax, [rbp - 224]");                                // base of parent_handle
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r10");                   // write-leading mode gives the parent read_end
    emitter.label("__rt_proc_open_pipe_recorded_win");

    // -- make the parent end non-inheritable (only the child end must be inherited) --
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload i
    emitter.instruction("lea rax, [rbp - 224]");                                // base of parent_handle
    emitter.instruction("mov rcx, QWORD PTR [rax + r9 * 8]");                   // parent_handle[i]
    emitter.instruction("mov rdx, 1");                                          // HANDLE_FLAG_INHERIT
    emitter.instruction("xor r8d, r8d");                                        // dwFlags = 0 (clear inherit -> non-inheritable)
    emitter.instruction("call SetHandleInformation");                           // the parent keeps a non-inheritable end
    emitter.instruction("test eax, eax");                                       // did clearing HANDLE_FLAG_INHERIT succeed?
    emitter.instruction("jnz __rt_proc_open_parent_inherit_ok_win");            // yes -> continue wiring the child handle
    emitter.instruction("inc QWORD PTR [rbp - 64]");                            // include this newly opened pair in the cleanup bound
    emitter.instruction("jmp __rt_proc_open_capture_cleanup_win");              // preserve GetLastError before closing handles
    emitter.label("__rt_proc_open_parent_inherit_ok_win");

    // -- mark the pipe before sharing standard-handle wiring with other descriptors --
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload i
    emitter.instruction("mov r10, 1");                                          // bit 0 = pipe owns a parent endpoint
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // did the child receive the write end (parent reads)?
    emitter.instruction("jne __rt_proc_open_pipe_state_ready_win");             // child-read mode leaves the parent write-only
    emitter.instruction("or r10, 2");                                           // bit 1 = parent owns the readable end
    emitter.label("__rt_proc_open_pipe_state_ready_win");
    emitter.instruction("lea rax, [rbp - 352]");                                // base of descriptor state
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r10");                   // record pipe ownership and parent direction
    emitter.instruction("jmp __rt_proc_open_std_wire_win");                     // wire the child handle into STARTUPINFOW

    // -- wire the child end into STARTUPINFOW for stdin/stdout/stderr (key < 3 only) --
    emitter.label("__rt_proc_open_std_wire_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload bookkeeping position i
    emitter.instruction("lea rax, [rbp - 288]");                                // base of child_handle
    emitter.instruction("mov r10, QWORD PTR [rax + r9 * 8]");                   // child_handle[i]
    emitter.instruction("lea rax, [rbp - 784]");                                // base of real descriptor-key bookkeeping
    emitter.instruction("mov r9, QWORD PTR [rax + r9 * 8]");                    // reload the actual child descriptor key
    emitter.instruction("cmp r9, 0");                                           // fd 0 (stdin)?
    emitter.instruction("jne __rt_proc_open_std_check1_win");                   // skip the stdin slot when the descriptor index is not zero
    emitter.instruction("mov QWORD PTR [rbp - 408], r10");                      // si.hStdInput = child_handle[0]
    emitter.instruction("jmp __rt_proc_open_std_done_win");                     // finish after assigning the child stdin handle
    emitter.label("__rt_proc_open_std_check1_win");
    emitter.instruction("cmp r9, 1");                                           // fd 1 (stdout)?
    emitter.instruction("jne __rt_proc_open_std_check2_win");                   // skip the stdout slot when the descriptor index is not one
    emitter.instruction("mov QWORD PTR [rbp - 400], r10");                      // si.hStdOutput = child_handle[1]
    emitter.instruction("jmp __rt_proc_open_std_done_win");                     // finish after assigning the child stdout handle
    emitter.label("__rt_proc_open_std_check2_win");
    emitter.instruction("cmp r9, 2");                                           // fd 2 (stderr)?
    emitter.instruction("jne __rt_proc_open_std_done_win");                     // fd >= 3: no standard slot (documented C1c limitation)
    emitter.instruction("mov QWORD PTR [rbp - 392], r10");                      // si.hStdError = child_handle[2]
    emitter.label("__rt_proc_open_std_done_win");

    // -- advance the loop index --
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload i
    emitter.instruction("inc r9");                                              // i += 1
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // persist the loop index
    emitter.instruction("jmp __rt_proc_open_loop_test_win");                    // continue the descriptor loop

    // -- non-pipe descriptors: dispatch PHP's null/redirect/file forms --
    emitter.label("__rt_proc_open_nonpipe_descriptor_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the descriptor-type mixed box
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // rdi/rsi = descriptor type bytes and length
    emitter.instruction("cmp rax, 1");                                          // descriptor type remained a string?
    emitter.instruction("jne __rt_proc_open_cleanup_m0_win");                   // malformed type cannot name a descriptor source
    emitter.instruction("cmp rdx, 4");                                          // null/file are both four-byte names
    emitter.instruction("jne __rt_proc_open_nonpipe_nonshort_win");             // socket/redirect use longer names
    emitter.instruction("cmp DWORD PTR [rdi], 0x6c6c756e");                     // "null" in little-endian byte order?
    emitter.instruction("je __rt_proc_open_null_descriptor_win");               // create an inheritable NUL device handle
    emitter.instruction("cmp DWORD PTR [rdi], 0x656c6966");                     // "file" in little-endian byte order?
    emitter.instruction("je __rt_proc_open_file_descriptor_win");               // open a strict UTF-8 path for the child
    emitter.instruction("jmp __rt_proc_open_cleanup_m0_win");                   // unsupported descriptor string
    emitter.label("__rt_proc_open_nonpipe_nonshort_win");
    emitter.instruction("cmp rdx, 6");                                          // socket has six bytes
    emitter.instruction("jne __rt_proc_open_nonpipe_long_win");                 // redirect is the remaining supported name
    emitter.instruction("cmp DWORD PTR [rdi], 0x6b636f73");                     // first half of "socket"
    emitter.instruction("jne __rt_proc_open_cleanup_m0_win");                   // unsupported six-byte descriptor
    emitter.instruction("cmp WORD PTR [rdi + 4], 0x7465");                      // second half of "socket"
    emitter.instruction("jne __rt_proc_open_cleanup_m0_win");                   // unsupported six-byte descriptor
    emitter.instruction("jmp __rt_proc_open_socket_descriptor_win");            // construct private loopback stream pair
    emitter.label("__rt_proc_open_nonpipe_long_win");
    emitter.instruction("cmp rdx, 8");                                          // redirect has eight bytes
    emitter.instruction("jne __rt_proc_open_cleanup_m0_win");                   // unsupported descriptor string
    emitter.instruction("cmp DWORD PTR [rdi], 0x69646572");                     // first half of "redirect"
    emitter.instruction("jne __rt_proc_open_cleanup_m0_win");                   // not a redirect descriptor
    emitter.instruction("cmp DWORD PTR [rdi + 4], 0x74636572");                 // second half of "redirect"
    emitter.instruction("jne __rt_proc_open_cleanup_m0_win");                   // not a redirect descriptor
    emitter.instruction("jmp __rt_proc_open_redirect_descriptor_win");          // duplicate an earlier child descriptor

    // -- descriptor `null`: CreateFileW(NUL) with inheritable attributes --
    emitter.label("__rt_proc_open_null_descriptor_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // descriptor type box is no longer needed
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the type box
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor array box is no longer needed
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the descriptor array box
    emitter.instruction("movabs rax, 0x0000004c0055004e");                      // materialize UTF-16 "NUL\\0"
    emitter.instruction("mov QWORD PTR [rbp - 520], rax");                      // retain the device path through CreateFileW
    emitter.instruction("lea rcx, [rbp - 520]");                                // lpFileName = NUL device
    emitter.instruction("mov rdx, 0xC0000000");                                 // allow child reads and writes against NUL
    emitter.instruction("mov r8, 3");                                           // share reads and writes like php-src streams
    emitter.instruction("lea r9, [rbp - 384]");                                 // inheritable SECURITY_ATTRIBUTES
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // OPEN_EXISTING for the NUL device
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x80");                      // FILE_ATTRIBUTE_NORMAL
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // no template handle
    emitter.instruction("call CreateFileW");                                    // open the inherited NUL endpoint
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je __rt_proc_open_capture_cleanup_win");               // preserve native failure before cleanup
    emitter.instruction("mov QWORD PTR [rbp - 656], rax");                      // retain the child-only NUL handle
    emitter.instruction("jmp __rt_proc_open_child_handle_ready_win");           // record and wire the child handle

    // -- descriptor `socket`: private loopback pair, never the public AF_UNIX API --
    emitter.label("__rt_proc_open_socket_descriptor_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // descriptor type box is no longer needed
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the socket name box
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor array box is no longer needed
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release descriptor array box
    emitter.instruction("lea rdi, [rbp - 104]");                                // parent raw SOCKET scratch output
    emitter.instruction("lea rsi, [rbp - 112]");                                // child raw SOCKET scratch output
    emitter.instruction("call __rt_proc_open_socketpair_win");                  // build a connected AF_INET loopback pair
    emitter.instruction("test rax, rax");                                       // private pair construction succeeded?
    emitter.instruction("jnz __rt_proc_open_cleanup_win");                      // helper already published Winsock errno
    emitter.instruction("mov rcx, -1");                                         // source process = current process pseudo-handle
    emitter.instruction("mov rdx, QWORD PTR [rbp - 104]");                      // inherited parent SOCKET from the private pair
    emitter.instruction("mov r8, -1");                                          // target process = current process pseudo-handle
    emitter.instruction("lea r9, [rbp - 656]");                                 // receive the non-inheritable parent duplicate
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // desired access ignored with DUPLICATE_SAME_ACCESS
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // parent duplicate must not be inherited
    emitter.instruction("mov QWORD PTR [rsp + 48], 3");                         // DUPLICATE_CLOSE_SOURCE | DUPLICATE_SAME_ACCESS
    emitter.instruction("call DuplicateHandle");                                // match php-src's make_descriptor_cloexec(parent)
    emitter.instruction("test eax, eax");                                       // did parent cloexec duplication succeed?
    emitter.instruction("jz __rt_proc_open_socket_parent_duplicate_failed_win"); // source closed even on failure; release child separately
    emitter.instruction("mov rax, QWORD PTR [rbp - 656]");                      // non-inheritable parent SOCKET duplicate
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // replace the closed original parent endpoint
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // descriptor position
    emitter.instruction("mov r11, QWORD PTR [rbp - 104]");                      // parent raw SOCKET
    emitter.instruction("lea r10, [rbp - 224]");                                // parent SOCKET table
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], r11");                   // store parent SOCKET at descriptor position
    emitter.instruction("mov r11, QWORD PTR [rbp - 112]");                      // child raw SOCKET
    emitter.instruction("lea r10, [rbp - 288]");                                // child SOCKET table
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], r11");                   // store child SOCKET at descriptor position
    emitter.instruction("lea r10, [rbp - 352]");                                // descriptor state table
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], 8");                     // bit 3 = raw SOCKET parent/child pair
    emitter.instruction("jmp __rt_proc_open_std_wire_win");                     // assign key 0/1/2 into STARTUPINFOW
    emitter.label("__rt_proc_open_socket_parent_duplicate_failed_win");
    emitter.instruction("call GetLastError");                                   // preserve DuplicateHandle failure before closing the child endpoint
    emitter.instruction("mov QWORD PTR [rbp - 624], rax");                      // source parent is already closed by DUPLICATE_CLOSE_SOURCE
    emitter.instruction("mov rcx, QWORD PTR [rbp - 112]");                      // child endpoint was not registered in the descriptor tables
    emitter.instruction("call closesocket");                                    // release the remaining socket without generic double-close
    emitter.instruction("jmp __rt_proc_open_create_failure_mode_restored_win"); // translate the saved native error and release prior descriptors

    // -- descriptor resource: duplicate its CRT stream handle for the child --
    emitter.label("__rt_proc_open_resource_descriptor_win");
    emitter.instruction("cmp rdx, 1");                                          // kind 1 = native stream resource?
    emitter.instruction("jne __rt_proc_open_cleanup_sub_win");                  // process/other resources are not descriptor streams
    emitter.instruction("mov QWORD PTR [rbp - 632], rdi");                      // retain the stream fd across decref
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor resource box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // caller keeps the original stream resource alive
    emitter.instruction("mov ecx, DWORD PTR [rbp - 632]");                      // CRT fd passed with the MSx64 ABI
    emitter.instruction("call _get_osfhandle");                                 // recover the owned source HANDLE without consuming it
    emitter.instruction("cmp rax, -1");                                         // invalid CRT descriptor?
    emitter.instruction("je __rt_proc_open_resource_invalid_fd_win");           // report deterministic EBADF instead of stale GetLastError
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // source HANDLE for DuplicateHandle
    emitter.instruction("jmp __rt_proc_open_duplicate_child_handle_win");       // create an inheritable child-owned copy

    // -- descriptor redirect: resolve an earlier real descriptor key --
    emitter.label("__rt_proc_open_redirect_descriptor_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // descriptor type box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the already-matched redirect name
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // descriptor sub-array
    emitter.instruction("mov esi, 1");                                          // redirect target lives at sub[1]
    emitter.instruction("mov rdx, -1");                                         // integer-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress a missing-target warning
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");                  // own the redirect target box
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // retain the target box across unbox
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // rdi = target descriptor number
    emitter.instruction("cmp rax, 0");                                          // redirect target must be an integer
    emitter.instruction("jne __rt_proc_open_redirect_release_fail_win");        // malformed target -> release boxes and fail
    emitter.instruction("mov QWORD PTR [rbp - 632], rdi");                      // retain target key while releasing boxes
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // redirect target box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the target box
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor array box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the descriptor array box
    emitter.instruction("mov QWORD PTR [rbp - 640], 0");                        // search position = 0
    emitter.label("__rt_proc_open_redirect_find_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 640]");                       // candidate prior descriptor position
    emitter.instruction("cmp r9, QWORD PTR [rbp - 64]");                        // only descriptors already processed are legal targets
    emitter.instruction("jae __rt_proc_open_redirect_std_fallback_win");        // otherwise PHP permits std handles 0/1/2
    emitter.instruction("lea r10, [rbp - 784]");                                // base of actual descriptor keys
    emitter.instruction("mov r11, QWORD PTR [r10 + r9 * 8]");                   // candidate key
    emitter.instruction("cmp r11, QWORD PTR [rbp - 632]");                      // target key matches this prior entry?
    emitter.instruction("je __rt_proc_open_redirect_found_win");                // duplicate the matching child handle
    emitter.instruction("inc r9");                                              // inspect the next prior descriptor
    emitter.instruction("mov QWORD PTR [rbp - 640], r9");                       // persist the search cursor
    emitter.instruction("jmp __rt_proc_open_redirect_find_win");                // continue lookup
    emitter.label("__rt_proc_open_redirect_found_win");
    emitter.instruction("lea r10, [rbp - 288]");                                // base of earlier child handles
    emitter.instruction("mov rax, QWORD PTR [r10 + r9 * 8]");                   // source HANDLE to duplicate
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // preserve source across DuplicateHandle setup
    emitter.instruction("jmp __rt_proc_open_duplicate_child_handle_win");       // inherit an independent child copy
    emitter.label("__rt_proc_open_redirect_std_fallback_win");
    emitter.instruction("cmp QWORD PTR [rbp - 632], 2");                        // fallback is defined only for standard descriptors 0/1/2
    emitter.instruction("ja __rt_proc_open_cleanup_win");                       // unknown/forward descriptor target fails cleanly
    emitter.instruction("mov ecx, -10");                                        // STD_INPUT_HANDLE for descriptor zero
    emitter.instruction("sub ecx, DWORD PTR [rbp - 632]");                      // 1/2 map to STD_OUTPUT_HANDLE/STD_ERROR_HANDLE
    emitter.instruction("call GetStdHandle");                                   // recover the inherited parent standard HANDLE
    emitter.instruction("test rax, rax");                                       // absent standard handle?
    emitter.instruction("jz __rt_proc_open_capture_cleanup_win");               // map the Win32 failure before cleanup
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je __rt_proc_open_capture_cleanup_win");               // map the Win32 failure before cleanup
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // source HANDLE remains owned by the process
    emitter.instruction("jmp __rt_proc_open_duplicate_child_handle_win");       // duplicate an inheritable child copy

    // -- descriptor file: parse PHP fopen-style mode and open a strict UTF-8 path --
    emitter.label("__rt_proc_open_file_descriptor_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // descriptor type box is no longer needed
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the matched file name
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // no mode box exists until sub[2] lookup succeeds
    emitter.instruction("mov QWORD PTR [rbp - 704], 0");                        // no narrow path staging buffer exists yet
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // descriptor sub-array
    emitter.instruction("mov esi, 1");                                          // file path lives at sub[1]
    emitter.instruction("mov rdx, -1");                                         // integer-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-path warning
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");                  // own the path box
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // retain path box across mode lookup
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // rdi/rdx = UTF-8 path bytes/length
    emitter.instruction("cmp rax, 1");                                          // path must be a string
    emitter.instruction("jne __rt_proc_open_file_release_fail_win");            // reject a non-string path
    emitter.instruction("mov QWORD PTR [rbp - 632], rdi");                      // save path bytes through mode parsing
    emitter.instruction("mov QWORD PTR [rbp - 640], rdx");                      // save path length through mode parsing
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // descriptor sub-array
    emitter.instruction("mov esi, 2");                                          // fopen mode lives at sub[2]
    emitter.instruction("mov rdx, -1");                                         // integer-key sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-mode warning
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");                  // own the mode box
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // retain mode box across unbox
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // rdi/rdx = mode bytes/length
    emitter.instruction("cmp rax, 1");                                          // mode must be a string
    emitter.instruction("jne __rt_proc_open_file_release_fail_win");            // reject a non-string mode
    emitter.instruction("test rdx, rdx");                                       // an empty fopen mode is invalid
    emitter.instruction("jz __rt_proc_open_file_release_fail_win");             // fail before opening a file
    emitter.instruction("mov QWORD PTR [rbp - 656], rdi");                      // retain mode bytes for plus scanning
    emitter.instruction("mov QWORD PTR [rbp - 664], rdx");                      // retain mode length for plus scanning
    emitter.instruction("mov rax, 0x80000000");                                 // materialize GENERIC_READ without an invalid memory immediate
    emitter.instruction("mov QWORD PTR [rbp - 672], rax");                      // default `r` access = GENERIC_READ
    emitter.instruction("mov QWORD PTR [rbp - 680], 3");                        // default `r` disposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rbp - 688], 0");                        // append seek is disabled by default
    emitter.instruction("cmp BYTE PTR [rdi], 0x72");                            // mode starts with `r`?
    emitter.instruction("je __rt_proc_open_file_mode_ready_win");               // keep read/open-existing defaults
    emitter.instruction("mov rax, 0x40000000");                                 // materialize GENERIC_WRITE without an invalid memory immediate
    emitter.instruction("mov QWORD PTR [rbp - 672], rax");                      // non-read modes start with GENERIC_WRITE
    emitter.instruction("cmp BYTE PTR [rdi], 0x77");                            // mode starts with `w`?
    emitter.instruction("je __rt_proc_open_file_mode_w_win");                   // CREATE_ALWAYS
    emitter.instruction("cmp BYTE PTR [rdi], 0x61");                            // mode starts with `a`?
    emitter.instruction("je __rt_proc_open_file_mode_a_win");                   // OPEN_ALWAYS then seek end
    emitter.instruction("cmp BYTE PTR [rdi], 0x78");                            // mode starts with `x`?
    emitter.instruction("je __rt_proc_open_file_mode_x_win");                   // CREATE_NEW
    emitter.instruction("cmp BYTE PTR [rdi], 0x63");                            // mode starts with `c`?
    emitter.instruction("jne __rt_proc_open_file_release_fail_win");            // unsupported PHP fopen mode
    emitter.instruction("mov QWORD PTR [rbp - 680], 4");                        // `c` = OPEN_ALWAYS without truncation
    emitter.instruction("jmp __rt_proc_open_file_mode_ready_win");              // scan optional plus marker
    emitter.label("__rt_proc_open_file_mode_w_win");
    emitter.instruction("mov QWORD PTR [rbp - 680], 2");                        // `w` = CREATE_ALWAYS
    emitter.instruction("jmp __rt_proc_open_file_mode_ready_win");              // scan optional plus marker
    emitter.label("__rt_proc_open_file_mode_a_win");
    emitter.instruction("mov QWORD PTR [rbp - 680], 4");                        // `a` = OPEN_ALWAYS
    emitter.instruction("mov QWORD PTR [rbp - 688], 1");                        // seek to end before child execution
    emitter.instruction("jmp __rt_proc_open_file_mode_ready_win");              // scan optional plus marker
    emitter.label("__rt_proc_open_file_mode_x_win");
    emitter.instruction("mov QWORD PTR [rbp - 680], 1");                        // `x` = CREATE_NEW
    emitter.label("__rt_proc_open_file_mode_ready_win");
    emitter.instruction("mov QWORD PTR [rbp - 696], 0");                        // mode scan index = 0
    emitter.label("__rt_proc_open_file_plus_scan_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 696]");                       // reload mode scan index
    emitter.instruction("cmp r9, QWORD PTR [rbp - 664]");                       // consumed every mode byte?
    emitter.instruction("jae __rt_proc_open_file_plus_done_win");               // no plus marker present
    emitter.instruction("mov r10, QWORD PTR [rbp - 656]");                      // mode byte pointer
    emitter.instruction("cmp BYTE PTR [r10 + r9], 0x2b");                       // this byte is `+`?
    emitter.instruction("je __rt_proc_open_file_plus_seen_win");                // enable read/write access
    emitter.instruction("inc r9");                                              // advance to the next mode byte
    emitter.instruction("mov QWORD PTR [rbp - 696], r9");                       // persist mode scan index
    emitter.instruction("jmp __rt_proc_open_file_plus_scan_win");               // continue scanning `b`/`t`/`+` suffixes
    emitter.label("__rt_proc_open_file_plus_seen_win");
    emitter.instruction("mov rax, 0xC0000000");                                 // materialize GENERIC_READ | GENERIC_WRITE
    emitter.instruction("mov QWORD PTR [rbp - 672], rax");                      // `+` requests GENERIC_READ | GENERIC_WRITE
    emitter.label("__rt_proc_open_file_plus_done_win");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 632]");                      // counted UTF-8 path bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 640]");                      // counted UTF-8 path length
    emitter.instruction("xor eax, eax");                                        // scan byte = NUL
    emitter.instruction("repne scasb");                                         // reject a path that CreateFileW would silently truncate
    emitter.instruction("je __rt_proc_open_file_release_eilseq_win");           // embedded NUL is not a representable Windows path
    emitter.instruction("mov rax, QWORD PTR [rbp - 640]");                      // counted UTF-8 path length
    emitter.instruction("inc rax");                                             // reserve the required NUL terminator
    emitter.instruction("jz __rt_proc_open_file_release_nomem_win");            // allocation-size overflow is ENOMEM
    emitter.instruction("mov QWORD PTR [rbp - 712], rax");                      // retain narrow path allocation size
    emitter.instruction("call __rt_heap_alloc");                                // allocate writable NUL-terminated path staging
    emitter.instruction("mov QWORD PTR [rbp - 704], rax");                      // retain narrow staging through conversion
    emitter.instruction("test rax, rax");                                       // path staging allocation succeeded?
    emitter.instruction("jz __rt_proc_open_file_release_nomem_win");            // release boxes and report ENOMEM
    emitter.instruction("mov rdi, rax");                                        // destination narrow path cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 632]");                      // source counted UTF-8 path bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 640]");                      // copy the complete path byte sequence
    emitter.instruction("cld");                                                 // copy forward into the owned staging buffer
    emitter.instruction("rep movsb");                                           // materialize the counted path without spare-byte assumptions
    emitter.instruction("mov BYTE PTR [rdi], 0");                               // append the NUL required by the shared conversion helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 704]");                      // NUL-terminated UTF-8 path staging
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // strict path conversion for CreateFileW
    emitter.instruction("mov QWORD PTR [rbp - 648], rax");                      // retain owned wide path through CreateFileW
    emitter.instruction("test rax, rax");                                       // strict conversion or allocation succeeded?
    emitter.instruction("jz __rt_proc_open_file_release_eilseq_win");           // path cannot be represented as UTF-16
    emitter.instruction("mov rax, QWORD PTR [rbp - 704]");                      // owned NUL-terminated path staging
    emitter.instruction("call __rt_heap_free");                                 // release narrow staging after strict conversion
    emitter.instruction("mov QWORD PTR [rbp - 704], 0");                        // prevent failure cleanup from freeing it twice
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // mode mixed box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release mode after parsing it
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // path mixed box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release path after conversion
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor array box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release descriptor array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 648]");                      // strict UTF-16 file path
    emitter.instruction("mov rdx, QWORD PTR [rbp - 672]");                      // parsed desired access
    emitter.instruction("mov r8, 3");                                           // FILE_SHARE_READ | FILE_SHARE_WRITE
    emitter.instruction("lea r9, [rbp - 384]");                                 // inherited child handle attributes
    emitter.instruction("mov rax, QWORD PTR [rbp - 680]");                      // parsed creation disposition
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // dwCreationDisposition
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x80");                      // FILE_ATTRIBUTE_NORMAL
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // no template handle
    emitter.instruction("call CreateFileW");                                    // open the child-only file handle
    emitter.instruction("mov QWORD PTR [rbp - 656], rax");                      // preserve CreateFileW result through path cleanup
    emitter.instruction("cmp rax, -1");                                         // file open succeeded?
    emitter.instruction("je __rt_proc_open_file_create_failed_win");            // capture error before releasing the wide path
    emitter.instruction("mov rax, QWORD PTR [rbp - 648]");                      // owned strict UTF-16 path
    emitter.instruction("call __rt_heap_free");                                 // release the path after successful open
    emitter.instruction("cmp QWORD PTR [rbp - 688], 0");                        // append mode requested?
    emitter.instruction("je __rt_proc_open_child_handle_ready_win");            // no seek needed before standard-handle wiring
    emitter.instruction("mov rcx, QWORD PTR [rbp - 656]");                      // file handle for SetFilePointerEx
    emitter.instruction("xor edx, edx");                                        // zero distance value
    emitter.instruction("xor r8d, r8d");                                        // caller does not need the resulting position
    emitter.instruction("mov r9d, 2");                                          // FILE_END
    emitter.instruction("call SetFilePointerEx");                               // place append-mode file at EOF
    emitter.instruction("test eax, eax");                                       // append positioning succeeded?
    emitter.instruction("jz __rt_proc_open_file_append_failed_win");            // capture and close this unregistered child handle
    emitter.instruction("jmp __rt_proc_open_child_handle_ready_win");           // record the child-only file handle

    // -- duplicate an existing child handle while preserving the source owner --
    emitter.label("__rt_proc_open_duplicate_child_handle_win");
    emitter.instruction("mov rcx, -1");                                         // source process = current process pseudo-handle
    emitter.instruction("mov rdx, QWORD PTR [rbp - 104]");                      // source HANDLE (never consumed)
    emitter.instruction("mov r8, -1");                                          // target process = current process pseudo-handle
    emitter.instruction("lea r9, [rbp - 656]");                                 // out: independently owned child HANDLE
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // desired access ignored with DUPLICATE_SAME_ACCESS
    emitter.instruction("mov QWORD PTR [rsp + 40], 1");                         // inherited by the child at CreateProcessW
    emitter.instruction("mov QWORD PTR [rsp + 48], 2");                         // DUPLICATE_SAME_ACCESS
    emitter.instruction("call DuplicateHandle");                                // create the inheritable child-only copy
    emitter.instruction("test eax, eax");                                       // duplication succeeded?
    emitter.instruction("jz __rt_proc_open_capture_cleanup_win");               // preserve native failure before cleanup
    emitter.instruction("jmp __rt_proc_open_child_handle_ready_win");           // record and wire the duplicate

    // -- record a child-only descriptor and share standard-handle wiring --
    emitter.label("__rt_proc_open_child_handle_ready_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // current descriptor position
    emitter.instruction("lea r10, [rbp - 288]");                                // base of child handle table
    emitter.instruction("mov rax, QWORD PTR [rbp - 656]");                      // acquired child-only HANDLE
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], rax");                   // store child HANDLE for post-spawn close
    emitter.instruction("lea r10, [rbp - 352]");                                // base of descriptor state table
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], 16");                    // bit 4 = child-only non-pipe descriptor
    emitter.instruction("jmp __rt_proc_open_std_wire_win");                     // apply real descriptor key 0/1/2 to STARTUPINFOW

    // -- malformed redirect target: the type box was already released --
    emitter.label("__rt_proc_open_redirect_release_fail_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // owned malformed target box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the target box exactly once
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor array box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the descriptor array box
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // prior descriptors remain owned by generic cleanup

    // -- file-only early failures release every owned parsed descriptor box --
    emitter.label("__rt_proc_open_file_release_fail_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // optional mode box
    emitter.instruction("test rax, rax");                                       // was mode lookup reached?
    emitter.instruction("jz __rt_proc_open_file_release_path_win");             // no mode box to release
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the mode box
    emitter.label("__rt_proc_open_file_release_path_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // path box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the path box
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor array box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the descriptor array box
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // prior descriptors are still the only owned handles
    emitter.label("__rt_proc_open_file_release_eilseq_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 704]");                      // optional narrow staging after failed conversion
    emitter.instruction("test rax, rax");                                       // was path staging allocated?
    emitter.instruction("jz __rt_proc_open_file_release_eilseq_boxes_win");     // no staging buffer to release
    emitter.instruction("call __rt_heap_free");                                 // release failed-conversion path staging
    emitter.label("__rt_proc_open_file_release_eilseq_boxes_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // mode box remains owned after failed conversion
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the mode box
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // path box remains owned after failed conversion
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the path box
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor array box remains owned
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the descriptor array box
    emitter.instruction("jmp __rt_proc_open_invalid_utf8_win");                 // publish EILSEQ for an invalid file path
    emitter.label("__rt_proc_open_file_release_nomem_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // path box remains owned after allocation failure
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the path box
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // mode box remains owned after allocation failure
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the mode box
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // descriptor array box remains owned
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the descriptor array box
    emitter.instruction("jmp __rt_proc_open_alloc_fail_win");                   // publish ENOMEM and release prior descriptors
    emitter.label("__rt_proc_open_file_create_failed_win");
    emitter.instruction("call GetLastError");                                   // capture CreateFileW failure before freeing the path
    emitter.instruction("mov QWORD PTR [rbp - 624], rax");                      // preserve native error code
    emitter.instruction("mov rax, QWORD PTR [rbp - 648]");                      // owned strict UTF-16 file path
    emitter.instruction("call __rt_heap_free");                                 // release path after failed open
    emitter.instruction("mov rax, QWORD PTR [rbp - 624]");                      // restore the native error code
    emitter.instruction("call __rt_win32_errno_from_code");                     // publish portable errno before generic cleanup
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // release prior descriptors only
    emitter.label("__rt_proc_open_file_append_failed_win");
    emitter.instruction("call GetLastError");                                   // capture seek failure before closing the file handle
    emitter.instruction("mov QWORD PTR [rbp - 624], rax");                      // preserve native error code
    emitter.instruction("mov rcx, QWORD PTR [rbp - 656]");                      // unregistered child-only file handle
    emitter.instruction("call CloseHandle");                                    // avoid leaking the failed append descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 624]");                      // restore native error code
    emitter.instruction("call __rt_win32_errno_from_code");                     // publish portable errno before cleanup
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // prior descriptors are still the only owned handles
    emitter.label("__rt_proc_open_resource_invalid_fd_win");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 9");                 // EBADF: stream resource has no CRT HANDLE
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // previous descriptors remain the only owned handles

    // -- fill any unwired std handle (0/1/2) with a redirect to NUL --
    emitter.label("__rt_proc_open_nul_fill_win");
    emitter.instruction("movabs rax, 0x0000004c0055004e");                      // materialize UTF-16 "NUL\0" device path
    emitter.instruction("mov QWORD PTR [rbp - 520], rax");                      // store the four WCHARs in the frame

    emitter.instruction("cmp QWORD PTR [rbp - 408], 0");                        // si.hStdInput already wired?
    emitter.instruction("jne __rt_proc_open_nul_stdout_win");                   // wired -> skip
    emitter.instruction("lea rcx, [rbp - 520]");                                // "NUL"
    emitter.instruction("mov rdx, 0xC0000000");                                 // GENERIC_READ | GENERIC_WRITE
    emitter.instruction("mov r8, 3");                                           // FILE_SHARE_READ | FILE_SHARE_WRITE
    emitter.instruction("lea r9, [rbp - 384]");                                 // &sa (heritable)
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x80");                      // dwFlagsAndAttributes = FILE_ATTRIBUTE_NORMAL
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open Unicode NUL for the missing stdin redirect
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je __rt_proc_open_capture_cleanup_win");               // capture the native error and release prior handles
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // nul_handle[0] = NUL handle
    emitter.instruction("mov QWORD PTR [rbp - 408], rax");                      // si.hStdInput = NUL handle
    emitter.label("__rt_proc_open_nul_stdout_win");
    emitter.instruction("cmp QWORD PTR [rbp - 400], 0");                        // si.hStdOutput already wired?
    emitter.instruction("jne __rt_proc_open_nul_stderr_win");                   // wired -> skip
    emitter.instruction("lea rcx, [rbp - 520]");                                // "NUL"
    emitter.instruction("mov rdx, 0xC0000000");                                 // GENERIC_READ | GENERIC_WRITE
    emitter.instruction("mov r8, 3");                                           // FILE_SHARE_READ | FILE_SHARE_WRITE
    emitter.instruction("lea r9, [rbp - 384]");                                 // &sa (heritable)
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x80");                      // dwFlagsAndAttributes = FILE_ATTRIBUTE_NORMAL
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open Unicode NUL for the missing stdout redirect
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je __rt_proc_open_capture_cleanup_win");               // capture the native error and release prior handles
    emitter.instruction("mov QWORD PTR [rbp - 152], rax");                      // nul_handle[1] = NUL handle
    emitter.instruction("mov QWORD PTR [rbp - 400], rax");                      // si.hStdOutput = NUL handle
    emitter.label("__rt_proc_open_nul_stderr_win");
    emitter.instruction("cmp QWORD PTR [rbp - 392], 0");                        // si.hStdError already wired?
    emitter.instruction("jne __rt_proc_open_cmdline_win");                      // wired -> skip
    emitter.instruction("lea rcx, [rbp - 520]");                                // "NUL"
    emitter.instruction("mov rdx, 0xC0000000");                                 // GENERIC_READ | GENERIC_WRITE
    emitter.instruction("mov r8, 3");                                           // FILE_SHARE_READ | FILE_SHARE_WRITE
    emitter.instruction("lea r9, [rbp - 384]");                                 // &sa (heritable)
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // dwCreationDisposition = OPEN_EXISTING
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x80");                      // dwFlagsAndAttributes = FILE_ATTRIBUTE_NORMAL
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // hTemplateFile = NULL
    emitter.instruction("call CreateFileW");                                    // open Unicode NUL for the missing stderr redirect
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je __rt_proc_open_capture_cleanup_win");               // capture the native error and release prior handles
    emitter.instruction("mov QWORD PTR [rbp - 160], rax");                      // nul_handle[2] = NUL handle
    emitter.instruction("mov QWORD PTR [rbp - 392], rax");                      // si.hStdError = NUL handle

    // -- build a direct command line or a cmd.exe shell staging buffer --
    emitter.label("__rt_proc_open_cmdline_win");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // scan the counted PHP string for embedded NUL bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // command byte length
    emitter.instruction("xor eax, eax");                                        // search byte = NUL
    emitter.instruction("repne scasb");                                         // Windows command lines cannot represent embedded NUL
    emitter.instruction("je __rt_proc_open_invalid_utf8_win");                  // reject truncating an embedded-NUL PHP string
    emitter.instruction("mov r9, QWORD PTR [rbp - 568]");                       // load packed environment/direct flags
    emitter.instruction("test r9, 1");                                          // direct array command or bypass_shell?
    emitter.instruction("jnz __rt_proc_open_cmd_size_win");                     // direct mode has no shell wrapper
    emitter.instruction("mov QWORD PTR [rbp - 584], 15");                       // shell prefix byte count
    emitter.instruction("mov QWORD PTR [rbp - 592], 1");                        // shell closing-quote byte count
    emitter.label("__rt_proc_open_cmd_size_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // start with command bytes
    emitter.instruction("add r9, QWORD PTR [rbp - 584]");                       // include the optional shell prefix
    emitter.instruction("add r9, QWORD PTR [rbp - 592]");                       // include the optional closing quote
    emitter.instruction("inc r9");                                              // include the NUL terminator
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // stash total size across the heap calls
    emitter.instruction("call GetProcessHeap");                                 // rax = default process heap
    emitter.instruction("mov rcx, rax");                                        // heap handle (arg1)
    emitter.instruction("xor edx, edx");                                        // dwFlags = 0 (arg2)
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // dwBytes (arg3), reloaded after GetProcessHeap
    emitter.instruction("call HeapAlloc");                                      // rax = cmdbuf pointer
    emitter.instruction("mov QWORD PTR [rbp - 136], rax");                      // save cmdbuf: CreateProcessW input + cleanup key
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // total-size scratch is now the wide_cmdbuf slot
    emitter.instruction("test rax, rax");                                       // did the narrow staging allocation succeed?
    emitter.instruction("jz __rt_proc_open_alloc_fail_win");                    // no -> publish ENOMEM and clean up pipes

    // -- write the optional literal "cmd.exe /s /c \"" prefix --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 136]");                      // cmdbuf pointer
    emitter.instruction("cmp QWORD PTR [rbp - 584], 0");                        // direct mode has no prefix
    emitter.instruction("je __rt_proc_open_copy_command_win");                  // skip the shell literal in direct mode
    emitter.instruction("mov BYTE PTR [rdi + 0], 0x63");                        // 'c'
    emitter.instruction("mov BYTE PTR [rdi + 1], 0x6d");                        // 'm'
    emitter.instruction("mov BYTE PTR [rdi + 2], 0x64");                        // 'd'
    emitter.instruction("mov BYTE PTR [rdi + 3], 0x2e");                        // '.'
    emitter.instruction("mov BYTE PTR [rdi + 4], 0x65");                        // 'e'
    emitter.instruction("mov BYTE PTR [rdi + 5], 0x78");                        // 'x'
    emitter.instruction("mov BYTE PTR [rdi + 6], 0x65");                        // 'e'
    emitter.instruction("mov BYTE PTR [rdi + 7], 0x20");                        // ' '
    emitter.instruction("mov BYTE PTR [rdi + 8], 0x2f");                        // '/'
    emitter.instruction("mov BYTE PTR [rdi + 9], 0x73");                        // 's'
    emitter.instruction("mov BYTE PTR [rdi + 10], 0x20");                       // ' '
    emitter.instruction("mov BYTE PTR [rdi + 11], 0x2f");                       // '/'
    emitter.instruction("mov BYTE PTR [rdi + 12], 0x63");                       // 'c'
    emitter.instruction("mov BYTE PTR [rdi + 13], 0x20");                       // ' '
    emitter.instruction("mov BYTE PTR [rdi + 14], 0x22");                       // '"' (opening quote around the command)

    // -- copy the raw command bytes after the selected prefix --
    emitter.label("__rt_proc_open_copy_command_win");
    emitter.instruction("cld");                                                 // ensure forward direction for the string copy
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // command pointer
    emitter.instruction("add rdi, QWORD PTR [rbp - 584]");                      // advance past the selected prefix
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // command length
    emitter.instruction("rep movsb");                                           // copy the command bytes; rdi now points past them

    // -- append the optional shell quote and mandatory NUL terminator --
    emitter.instruction("cmp QWORD PTR [rbp - 592], 0");                        // shell mode needs a closing quote
    emitter.instruction("je __rt_proc_open_terminate_command_win");             // direct mode terminates immediately
    emitter.instruction("mov BYTE PTR [rdi], 0x22");                            // append the shell command's closing quote
    emitter.instruction("inc rdi");                                             // advance past the closing quote
    emitter.label("__rt_proc_open_terminate_command_win");
    emitter.instruction("mov BYTE PTR [rdi], 0");                               // NUL terminator

    // -- strictly convert the complete writable command line to UTF-16 --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 136]");                      // NUL-terminated UTF-8 command line
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // strict MB_ERR_INVALID_CHARS conversion to owned WCHAR buffer
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve wide_cmdbuf for spawn and cleanup
    emitter.instruction("test rax, rax");                                       // invalid UTF-8 or allocation failure?
    emitter.instruction("jz __rt_proc_open_invalid_utf8_win");                  // fail without passing a truncated/invalid command to Windows

    // -- materialize an optional counted cwd string as strict UTF-16 --
    emitter.instruction("cmp QWORD PTR [rbp - 528], 0");                        // was a non-null cwd supplied?
    emitter.instruction("je __rt_proc_open_cwd_ready_win");                     // null -> inherit the parent current directory
    emitter.instruction("mov rdi, QWORD PTR [rbp - 528]");                      // scan the counted cwd for embedded NUL bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 536]");                      // cwd byte length
    emitter.instruction("xor eax, eax");                                        // search byte = NUL
    emitter.instruction("repne scasb");                                         // Win32 current-directory strings cannot contain embedded NUL
    emitter.instruction("je __rt_proc_open_invalid_utf8_win");                  // reject a cwd that would be truncated
    emitter.instruction("mov r9, QWORD PTR [rbp - 536]");                       // cwd byte length
    emitter.instruction("inc r9");                                              // include the trailing NUL in the staging allocation
    emitter.instruction("mov QWORD PTR [rbp - 544], r9");                       // temporarily preserve allocation size
    emitter.instruction("call GetProcessHeap");                                 // rax = default process heap
    emitter.instruction("mov rcx, rax");                                        // heap handle
    emitter.instruction("xor edx, edx");                                        // allocation flags = 0
    emitter.instruction("mov r8, QWORD PTR [rbp - 544]");                       // cwd staging allocation size
    emitter.instruction("call HeapAlloc");                                      // allocate writable NUL-terminated cwd staging
    emitter.instruction("mov QWORD PTR [rbp - 544], rax");                      // save cwd narrow staging pointer
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz __rt_proc_open_alloc_fail_win");                    // no -> ENOMEM + cleanup
    emitter.instruction("mov rdi, rax");                                        // destination cwd staging cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 528]");                      // source counted cwd bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 536]");                      // source byte length
    emitter.instruction("cld");                                                 // copy forward
    emitter.instruction("rep movsb");                                           // copy the complete cwd
    emitter.instruction("mov BYTE PTR [rdi], 0");                               // append NUL terminator
    emitter.instruction("mov rdi, QWORD PTR [rbp - 544]");                      // NUL-terminated UTF-8 cwd
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // strict cwd conversion
    emitter.instruction("mov QWORD PTR [rbp - 552], rax");                      // preserve owned UTF-16 cwd
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz __rt_proc_open_invalid_utf8_win");                  // invalid UTF-8 -> EILSEQ + cleanup
    emitter.label("__rt_proc_open_cwd_ready_win");

    // -- strictly convert the counted double-NUL UTF-8 environment block --
    emitter.instruction("cmp QWORD PTR [rbp - 560], 0");                        // was a custom environment supplied?
    emitter.instruction("je __rt_proc_open_environment_ready_win");             // null means inherit the parent environment
    emitter.instruction("mov rax, QWORD PTR [rbp - 568]");                      // packed byte length/direct flag
    emitter.instruction("shr rax, 5");                                          // recover the complete UTF-8 block byte length
    emitter.instruction("test rax, rax");                                       // the block must include its final double NUL
    emitter.instruction("jz __rt_proc_open_invalid_utf8_win");                  // reject an inconsistent internal descriptor
    emitter.instruction("mov QWORD PTR [rbp - 600], rax");                      // preserve source byte length across API calls
    emitter.instruction("mov ecx, 65001");                                      // CodePage = CP_UTF8
    emitter.instruction("mov edx, 8");                                          // dwFlags = MB_ERR_INVALID_CHARS
    emitter.instruction("mov r8, QWORD PTR [rbp - 560]");                       // source counted UTF-8 block
    emitter.instruction("mov r9d, eax");                                        // source byte count, including embedded NULs
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // sizing pass has no destination buffer
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // sizing pass destination capacity = 0
    emitter.instruction("call MultiByteToWideChar");                            // eax = required UTF-16 code units
    emitter.instruction("test eax, eax");                                       // strict sizing conversion succeeded?
    emitter.instruction("jz __rt_proc_open_invalid_utf8_win");                  // malformed UTF-8 cannot reach CreateProcessW
    emitter.instruction("mov QWORD PTR [rbp - 608], rax");                      // preserve required UTF-16 code-unit count
    emitter.instruction("mov rax, QWORD PTR [rbp - 608]");                      // required UTF-16 code units
    emitter.instruction("shl rax, 1");                                          // convert code units to allocation bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate the owned UTF-16 environment block
    emitter.instruction("mov QWORD PTR [rbp - 576], rax");                      // retain the block for spawn and cleanup
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz __rt_proc_open_alloc_fail_win");                    // publish ENOMEM and release prior resources
    emitter.instruction("mov ecx, 65001");                                      // CodePage = CP_UTF8
    emitter.instruction("mov edx, 8");                                          // dwFlags = MB_ERR_INVALID_CHARS
    emitter.instruction("mov r8, QWORD PTR [rbp - 560]");                       // source double-NUL UTF-8 block
    emitter.instruction("mov r9, QWORD PTR [rbp - 600]");                       // complete source byte count
    emitter.instruction("mov rax, QWORD PTR [rbp - 576]");                      // destination UTF-16 block
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // lpWideCharStr
    emitter.instruction("mov rax, QWORD PTR [rbp - 608]");                      // destination capacity in code units
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // cchWideChar
    emitter.instruction("call MultiByteToWideChar");                            // preserve embedded separators and final double NUL
    emitter.instruction("test eax, eax");                                       // strict conversion succeeded?
    emitter.instruction("jz __rt_proc_open_invalid_utf8_win");                  // fail before process creation on malformed input
    emitter.label("__rt_proc_open_environment_ready_win");

    // -- Enable php-src's optional error suppression only around CreateProcessW --
    emitter.instruction("mov rax, QWORD PTR [rbp - 568]");                      // packed Windows proc_open options
    emitter.instruction("test rax, 2");                                         // suppress_errors enabled?
    emitter.instruction("jz __rt_proc_open_error_mode_ready_win");              // default behavior preserves the current process mode
    emitter.instruction("mov rcx, 3");                                          // SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX
    emitter.instruction("call SetErrorMode");                                   // suppress modal Windows critical-error dialogs
    emitter.instruction("mov QWORD PTR [rbp - 616], rax");                      // preserve the caller's error mode for restoration
    emitter.label("__rt_proc_open_error_mode_ready_win");

    // -- CreateProcessW with direct/shell command and optional Unicode environment --
    emitter.instruction("xor ecx, ecx");                                        // lpApplicationName = NULL
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // lpCommandLine = wide_cmdbuf (must be writable)
    emitter.instruction("xor r8d, r8d");                                        // lpProcessAttributes = NULL
    emitter.instruction("xor r9d, r9d");                                        // lpThreadAttributes = NULL
    emitter.instruction("mov QWORD PTR [rsp + 32], 1");                         // bInheritHandles = TRUE
    emitter.instruction("mov rax, 0x20");                                       // NORMAL_PRIORITY_CLASS matches php-src's default
    emitter.instruction("test QWORD PTR [rbp - 568], 8");                       // create_process_group requested?
    emitter.instruction("jz __rt_proc_open_no_process_group_win");              // leave default group inheritance unchanged
    emitter.instruction("or rax, 0x200");                                       // add CREATE_NEW_PROCESS_GROUP
    emitter.label("__rt_proc_open_no_process_group_win");
    emitter.instruction("test QWORD PTR [rbp - 568], 16");                      // create_new_console requested?
    emitter.instruction("jz __rt_proc_open_no_new_console_win");                // retain the parent's console by default
    emitter.instruction("or rax, 0x10");                                        // add CREATE_NEW_CONSOLE
    emitter.label("__rt_proc_open_no_new_console_win");
    emitter.instruction("cmp QWORD PTR [rbp - 576], 0");                        // custom environment present?
    emitter.instruction("je __rt_proc_open_creation_flags_ready_win");          // inherited environment needs no Unicode flag
    emitter.instruction("or rax, 0x400");                                       // add CREATE_UNICODE_ENVIRONMENT
    emitter.label("__rt_proc_open_creation_flags_ready_win");
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // dwCreationFlags
    emitter.instruction("mov rax, QWORD PTR [rbp - 576]");                      // optional UTF-16 environment block
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // lpEnvironment = custom block or inherit
    emitter.instruction("mov rax, QWORD PTR [rbp - 552]");                      // optional strict UTF-16 cwd (or NULL)
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // lpCurrentDirectory = cwd or inherit parent's
    emitter.instruction("lea rax, [rbp - 488]");                                // &si
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // lpStartupInfo = &si
    emitter.instruction("lea rax, [rbp - 512]");                                // &pi
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // lpProcessInformation = &pi
    emitter.instruction("call CreateProcessW");                                 // spawn the child process
    emitter.instruction("test eax, eax");                                       // CreateProcessW failed?
    emitter.instruction("jz __rt_proc_open_create_failed_win");                 // preserve spawn errno before restoring the caller mode
    emitter.instruction("test QWORD PTR [rbp - 568], 2");                       // did this call enable suppress_errors?
    emitter.instruction("jz __rt_proc_open_error_mode_restored_win");           // no mode restoration is required
    emitter.instruction("mov rcx, QWORD PTR [rbp - 616]");                      // saved caller error mode
    emitter.instruction("call SetErrorMode");                                   // restore the caller's process-wide error-mode policy
    emitter.label("__rt_proc_open_error_mode_restored_win");

    // -- success: the thread handle is never used, close it immediately --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 504]");                      // pi.hThread
    emitter.instruction("call CloseHandle");                                    // close the unused thread handle

    // -- close every child end: the child inherited its own copies at spawn time --
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // j = 0
    emitter.label("__rt_proc_open_close_child_test_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload n
    emitter.instruction("cmp r9, r10");                                         // j < n?
    emitter.instruction("jae __rt_proc_open_close_nul_win");                    // all child ends closed -> close NUL handles
    emitter.instruction("lea rax, [rbp - 352]");                                // base of is_pipe
    emitter.instruction("mov r11, QWORD PTR [rax + r9 * 8]");                   // is_pipe[j]
    emitter.instruction("test r11, r11");                                       // descriptor owns a child handle?
    emitter.instruction("jz __rt_proc_open_close_child_next_win");              // skip descriptors with no child handle
    emitter.instruction("lea rax, [rbp - 288]");                                // base of child_handle
    emitter.instruction("mov rcx, QWORD PTR [rax + r9 * 8]");                   // child_handle[j]
    emitter.instruction("test r11, 8");                                         // is the child endpoint a raw Winsock SOCKET?
    emitter.instruction("jnz __rt_proc_open_close_child_socket_win");           // SOCKETs are not Win32 kernel HANDLEs for CloseHandle
    emitter.instruction("call CloseHandle");                                    // close the parent's copy of the child end
    emitter.instruction("jmp __rt_proc_open_close_child_next_win");             // continue after closing the child handle
    emitter.label("__rt_proc_open_close_child_socket_win");
    emitter.instruction("call closesocket");                                    // release the inherited child SOCKET in the parent
    emitter.label("__rt_proc_open_close_child_next_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("inc r9");                                              // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // persist j
    emitter.instruction("jmp __rt_proc_open_close_child_test_win");             // continue closing child ends

    // -- close any NUL handles opened for unwired std slots --
    emitter.label("__rt_proc_open_close_nul_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 144]");                      // nul_handle[0]
    emitter.instruction("test rcx, rcx");                                       // was stdin redirected to NUL?
    emitter.instruction("jz __rt_proc_open_close_nul1_win");                    // not opened -> skip
    emitter.instruction("call CloseHandle");                                    // close the stdin NUL handle
    emitter.label("__rt_proc_open_close_nul1_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 152]");                      // nul_handle[1]
    emitter.instruction("test rcx, rcx");                                       // was stdout redirected to NUL?
    emitter.instruction("jz __rt_proc_open_close_nul2_win");                    // not opened -> skip
    emitter.instruction("call CloseHandle");                                    // close the stdout NUL handle
    emitter.label("__rt_proc_open_close_nul2_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 160]");                      // nul_handle[2]
    emitter.instruction("test rcx, rcx");                                       // was stderr redirected to NUL?
    emitter.instruction("jz __rt_proc_open_free_cmdbuf_win");                   // not opened -> skip
    emitter.instruction("call CloseHandle");                                    // close the stderr NUL handle

    // -- free both command buffers: CreateProcessW has already consumed them --
    emitter.label("__rt_proc_open_free_cmdbuf_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // owned UTF-16 command buffer
    emitter.instruction("call __rt_heap_free");                                 // release the strict-conversion result
    emitter.instruction("call GetProcessHeap");                                 // rax = default process heap
    emitter.instruction("mov rcx, rax");                                        // heap handle (arg1)
    emitter.instruction("xor edx, edx");                                        // dwFlags = 0 (arg2)
    emitter.instruction("mov r8, QWORD PTR [rbp - 136]");                       // lpMem = cmdbuf (arg3)
    emitter.instruction("call HeapFree");                                       // release the command-line buffer
    emitter.instruction("mov rax, QWORD PTR [rbp - 576]");                      // optional owned UTF-16 environment block
    emitter.instruction("test rax, rax");                                       // was a custom environment converted?
    emitter.instruction("jz __rt_proc_open_free_cwd_win");                      // no -> continue with cwd cleanup
    emitter.instruction("call __rt_heap_free");                                 // release the converted environment block
    emitter.label("__rt_proc_open_free_cwd_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 552]");                      // optional owned UTF-16 cwd
    emitter.instruction("test rax, rax");                                       // was cwd converted?
    emitter.instruction("jz __rt_proc_open_free_cwd_narrow_win");               // no -> skip the wide free
    emitter.instruction("call __rt_heap_free");                                 // release UTF-16 cwd
    emitter.label("__rt_proc_open_free_cwd_narrow_win");
    emitter.instruction("cmp QWORD PTR [rbp - 544], 0");                        // was cwd narrow staging allocated?
    emitter.instruction("je __rt_proc_open_push_begin_win");                    // no -> proceed to publish pipes
    emitter.instruction("call GetProcessHeap");                                 // rax = process heap
    emitter.instruction("mov rcx, rax");                                        // heap handle
    emitter.instruction("xor edx, edx");                                        // free flags = 0
    emitter.instruction("mov r8, QWORD PTR [rbp - 544]");                       // cwd narrow staging pointer
    emitter.instruction("call HeapFree");                                       // release cwd staging buffer

    // -- adopt every parent HANDLE as a CRT fd, then publish it at its real key --
    // `_open_osfhandle` owns the HANDLE on success. Never call CloseHandle on an
    // adopted entry: kind-1 resource release reaches the Windows close shim,
    // which calls `_close` and transfers the final HANDLE release to the CRT.
    emitter.label("__rt_proc_open_push_begin_win");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // j = 0
    emitter.label("__rt_proc_open_push_test_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload n
    emitter.instruction("cmp r9, r10");                                         // j < n?
    emitter.instruction("jae __rt_proc_open_done_win");                         // all pipes pushed -> return hProcess
    emitter.instruction("lea rax, [rbp - 352]");                                // base of is_pipe
    emitter.instruction("mov r11, QWORD PTR [rax + r9 * 8]");                   // is_pipe[j]
    emitter.instruction("test r11, 1");                                         // descriptor owns a parent pipe endpoint?
    emitter.instruction("jnz __rt_proc_open_parent_pipe_publish_win");          // CRT-adopt ordinary pipe handles only
    emitter.instruction("test r11, 8");                                         // descriptor owns a raw parent Winsock SOCKET?
    emitter.instruction("jz __rt_proc_open_push_next_win");                     // child-only descriptors are never published in $pipes
    emitter.instruction("lea rax, [rbp - 224]");                                // base of parent_handle
    emitter.instruction("mov rax, QWORD PTR [rax + r9 * 8]");                   // raw parent SOCKET remains outside the CRT table
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // preserve raw socket for resource boxing
    emitter.instruction("jmp __rt_proc_open_parent_fd_box_win");                // publish kind-1 resource without _open_osfhandle
    emitter.label("__rt_proc_open_parent_pipe_publish_win");
    emitter.instruction("lea rax, [rbp - 224]");                                // base of parent_handle
    emitter.instruction("mov rax, QWORD PTR [rax + r9 * 8]");                   // raw parent HANDLE awaiting CRT adoption
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // retain the raw HANDLE across _open_osfhandle
    emitter.instruction("mov edx, 0x8000");                                     // _O_BINARY is mandatory for PHP stream bytes
    emitter.instruction("test r11, 2");                                         // does the parent own the readable pipe end?
    emitter.instruction("jnz __rt_proc_open_parent_fd_flags_ready_win");        // readable ends use CRT O_RDONLY (zero)
    emitter.instruction("or edx, 1");                                           // write ends use CRT O_WRONLY
    emitter.label("__rt_proc_open_parent_fd_flags_ready_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // raw HANDLE ownership transfers to the CRT on success
    emitter.instruction("call _open_osfhandle");                                // convert the parent pipe HANDLE into a CRT descriptor
    emitter.instruction("cmp eax, -1");                                         // did CRT descriptor allocation fail before adopting the HANDLE?
    emitter.instruction("je __rt_proc_open_parent_fd_fail_win");                // close raw/adopted peers without touching child handles twice
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // preserve the CRT fd across optional status-cache insertion
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j after the CRT call
    emitter.instruction("lea rax, [rbp - 224]");                                // base of parent_handle
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // newly adopted CRT descriptor
    emitter.instruction("mov QWORD PTR [rax + r9 * 8], r10");                   // replace raw HANDLE with its CRT fd
    emitter.instruction("lea rax, [rbp - 352]");                                // base of pipe_state
    emitter.instruction("or QWORD PTR [rax + r9 * 8], 4");                      // bit 2 = parent HANDLE ownership moved into CRT fd table
    // php-src defaults `blocking_pipes` to false. Cache O_NONBLOCK only for
    // parent-readable pipe ends; `__rt_sys_read` turns that flag into a
    // PeekNamedPipe availability check before it ever enters ReadFile.
    emitter.instruction("test QWORD PTR [rbp - 568], 4");                       // explicit blocking_pipes=true?
    emitter.instruction("jnz __rt_proc_open_parent_fd_box_win");                // true retains normal blocking CRT stream semantics
    emitter.instruction("test QWORD PTR [rax + r9 * 8], 2");                    // is this parent endpoint readable?
    emitter.instruction("jz __rt_proc_open_parent_fd_box_win");                 // write-only endpoint has no ReadFile blocking path
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // cache key = parent CRT fd
    emitter.instruction("mov rsi, 0x800");                                      // O_NONBLOCK visible through F_GETFL and read shim
    emitter.instruction("call __rt_win_fd_status_upsert");                      // retain nonblocking status until close releases this fd
    emitter.label("__rt_proc_open_parent_fd_box_win");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // kind-1 resource low payload = CRT fd, never a raw HANDLE
    emitter.instruction("mov rax, 9");                                          // tag 9 = resource
    emitter.instruction("mov rsi, 1");                                          // hi = kind 1 (native stream fd closed by resource release)
    abi::emit_call_label(emitter, "__rt_mixed_from_value"); // rax = res_box (owned, refcount 1)
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save res_box across the push
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j after the allocating resource-box helper
    emitter.instruction("lea r10, [rbp - 784]");                                // base of real descriptor-key bookkeeping
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // actual integer descriptor key for this parent pipe
    emitter.instruction("xor esi, esi");                                        // integer keys have no high payload word
    emitter.instruction("xor eax, eax");                                        // tag 0 = integer key Mixed cell
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // allocate the borrowed key cell used by the generic setter
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // retain the key cell across the container write
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the current pipes array/hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // pass the integer descriptor key cell
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // transfer the resource box into the destination container
    abi::emit_call_label(emitter, "__rt_array_set_mixed_key");                  // set resource at the real descriptor key, returning promoted/reallocated container
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the potentially promoted/reallocated pipes container
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the borrowed key cell after the setter read it
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the caller-owned key box; the setter never owns keys
    emitter.label("__rt_proc_open_push_next_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload j
    emitter.instruction("inc r9");                                              // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // persist j
    emitter.instruction("jmp __rt_proc_open_push_test_win");                    // continue pushing parent ends

    // -- failed CRT adoption after CreateProcessW: child ends/buffers are already released --
    emitter.label("__rt_proc_open_parent_fd_fail_win");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 24");                // EMFILE: CRT descriptor table could not adopt a parent pipe HANDLE
    emitter.instruction("mov QWORD PTR [rbp - 520], 0");                        // cleanup index j = 0
    emitter.label("__rt_proc_open_parent_fd_cleanup_test_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 520]");                       // reload cleanup j
    emitter.instruction("cmp r9, QWORD PTR [rbp - 48]");                        // processed every descriptor?
    emitter.instruction("jae __rt_proc_open_parent_fd_cleanup_process_win");    // remaining resource is the unreturned process handle
    emitter.instruction("lea rax, [rbp - 352]");                                // base of pipe_state
    emitter.instruction("mov r11, QWORD PTR [rax + r9 * 8]");                   // state for this descriptor
    emitter.instruction("test r11, 8");                                         // did a raw socket parent endpoint survive child-end cleanup?
    emitter.instruction("jnz __rt_proc_open_parent_fd_cleanup_socket_win");     // raw Winsock sockets are never CRT-adopted
    emitter.instruction("test r11, 1");                                         // was a pipe opened here?
    emitter.instruction("jz __rt_proc_open_parent_fd_cleanup_next_win");        // no ownership to release
    emitter.instruction("lea rax, [rbp - 224]");                                // base of parent_handle / adopted fd slots
    emitter.instruction("mov r10, QWORD PTR [rax + r9 * 8]");                   // raw HANDLE or CRT fd, selected by adoption bit
    emitter.instruction("test r11, 4");                                         // did _open_osfhandle take ownership?
    emitter.instruction("jz __rt_proc_open_parent_fd_cleanup_raw_win");         // raw HANDLE still belongs to this helper
    emitter.instruction("mov rdi, r10");                                        // adopted CRT descriptor for the runtime close shim
    emitter.instruction("call __rt_sys_close");                                 // clears status cache and lets _close release the HANDLE exactly once
    emitter.instruction("jmp __rt_proc_open_parent_fd_cleanup_next_win");       // do not CloseHandle an adopted descriptor
    emitter.label("__rt_proc_open_parent_fd_cleanup_raw_win");
    emitter.instruction("mov rcx, r10");                                        // raw HANDLE that CRT never adopted
    emitter.instruction("call CloseHandle");                                    // release the remaining parent handle directly
    emitter.instruction("jmp __rt_proc_open_parent_fd_cleanup_next_win");       // avoid treating a pipe HANDLE as a SOCKET
    emitter.label("__rt_proc_open_parent_fd_cleanup_socket_win");
    emitter.instruction("lea rax, [rbp - 224]");                                // base of published raw parent SOCKETs
    emitter.instruction("mov rcx, QWORD PTR [rax + r9 * 8]");                   // raw parent SOCKET whose child peer was already closed
    emitter.instruction("call closesocket");                                    // release the published parent socket during late adoption rollback
    emitter.label("__rt_proc_open_parent_fd_cleanup_next_win");
    emitter.instruction("inc QWORD PTR [rbp - 520]");                           // advance cleanup index
    emitter.instruction("jmp __rt_proc_open_parent_fd_cleanup_test_win");       // close the next parent endpoint
    emitter.label("__rt_proc_open_parent_fd_cleanup_process_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 512]");                      // process HANDLE was never returned to a kind-5 resource
    emitter.instruction("test rcx, rcx");                                       // CreateProcessW initializes it on success, retain defensive guard
    emitter.instruction("jz __rt_proc_open_fail_win");                          // no process handle needs release
    emitter.instruction("call CloseHandle");                                    // release unreturned process HANDLE without terminating the child
    emitter.instruction("jmp __rt_proc_open_fail_win");                         // report the failed proc_open call as PHP false

    // -- success: return the child process handle --
    emitter.label("__rt_proc_open_done_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 512]");                      // reload pi.hProcess
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the final pipes array/hash pointer beside the process handle
    emitter.instruction("add rsp, 896");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return hProcess (lowerer boxes it as a kind-5 resource)

    // -- cleanup: release owned boxes for the mid-loop failure paths --
    emitter.label("__rt_proc_open_cleanup_sub_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release sub_box before cleanup
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // proceed to close opened handles
    emitter.label("__rt_proc_open_cleanup_m0_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release m0
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release sub_box
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // proceed to close opened handles
    emitter.label("__rt_proc_open_cleanup_m1_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // m1
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release m1
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // m0
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release m0
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // sub_box
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release sub_box
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // proceed to close opened handles

    // -- preserve CreateProcessW failure before restoring suppress_errors mode --
    emitter.label("__rt_proc_open_create_failed_win");
    emitter.instruction("call GetLastError");                                   // capture the failing process-creation error immediately
    emitter.instruction("mov QWORD PTR [rbp - 624], rax");                      // preserve the native code across SetErrorMode
    emitter.instruction("test QWORD PTR [rbp - 568], 2");                       // was suppress_errors enabled for this spawn?
    emitter.instruction("jz __rt_proc_open_create_failure_mode_restored_win");  // no process mode needs restoring
    emitter.instruction("mov rcx, QWORD PTR [rbp - 616]");                      // saved caller error mode
    emitter.instruction("call SetErrorMode");                                   // restore the caller's process-wide error-mode policy
    emitter.label("__rt_proc_open_create_failure_mode_restored_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 624]");                      // restore the captured native error code
    emitter.instruction("call __rt_win32_errno_from_code");                     // publish the matching portable errno
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // release handles and report PHP false

    // -- capture a native failure before cleanup calls can overwrite GetLastError --
    emitter.label("__rt_proc_open_capture_cleanup_win");
    emitter.instruction("call GetLastError");                                   // fetch the failing CreatePipe/File/Process/handle operation's code
    emitter.instruction("call __rt_win32_errno_from_code");                     // publish the translated POSIX errno
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // release every resource acquired so far

    // -- reject unrepresentable command strings with EILSEQ --
    emitter.label("__rt_proc_open_invalid_utf8_win");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 84");                // EILSEQ: embedded NUL or invalid UTF-8 command
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // release pipes and any allocated command buffer

    // -- report command-buffer allocation failure as ENOMEM --
    emitter.label("__rt_proc_open_alloc_fail_win");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 12");                // ENOMEM: command-line staging allocation failed
    emitter.instruction("jmp __rt_proc_open_cleanup_win");                      // release the already-created pipe handles

    // -- cleanup: close every opened child handle and pipe parent endpoint, then NUL handles + cmdbuf --
    // Reached from a mid-loop failure (bound = i, the count already processed)
    // or a CreateProcessW failure (bound = n, since the loop finished first).
    // Reuses the nul_path slot [rbp - 520] as the cleanup loop counter: by the
    // time cleanup runs, the "NUL" literal is no longer needed either way.
    emitter.label("__rt_proc_open_cleanup_win");
    emitter.instruction("mov QWORD PTR [rbp - 520], 0");                        // cleanup index j = 0
    emitter.label("__rt_proc_open_cleanup_test_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 520]");                       // reload cleanup j
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // bound = i (descriptors processed so far)
    emitter.instruction("cmp r9, r10");                                         // j < bound?
    emitter.instruction("jae __rt_proc_open_cleanup_nul_win");                  // all opened pipes closed -> close NUL handles
    emitter.instruction("lea rax, [rbp - 352]");                                // base of is_pipe
    emitter.instruction("mov r11, QWORD PTR [rax + r9 * 8]");                   // is_pipe[j]
    emitter.instruction("test r11, r11");                                       // descriptor acquired any child handle?
    emitter.instruction("jz __rt_proc_open_cleanup_next_win");                  // skip descriptors that never acquired ownership
    emitter.instruction("test r11, 8");                                         // does this descriptor own a raw Winsock socket pair?
    emitter.instruction("jnz __rt_proc_open_cleanup_socket_win");               // sockets require closesocket rather than CloseHandle
    emitter.instruction("test r11, 1");                                         // pipe owns a parent endpoint too?
    emitter.instruction("jz __rt_proc_open_cleanup_child_only_win");            // child-only descriptor has no parent handle to close
    emitter.instruction("lea rax, [rbp - 224]");                                // base of parent_handle
    emitter.instruction("mov rcx, QWORD PTR [rax + r9 * 8]");                   // parent_handle[j]
    emitter.instruction("call CloseHandle");                                    // close the parent end
    emitter.label("__rt_proc_open_cleanup_child_only_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 520]");                       // reload cleanup j (volatile regs clobbered)
    emitter.instruction("lea rax, [rbp - 288]");                                // base of child_handle
    emitter.instruction("mov rcx, QWORD PTR [rax + r9 * 8]");                   // child_handle[j]
    emitter.instruction("call CloseHandle");                                    // close the child end
    emitter.instruction("jmp __rt_proc_open_cleanup_next_win");                 // continue after releasing ordinary handles
    emitter.label("__rt_proc_open_cleanup_socket_win");
    emitter.instruction("lea rax, [rbp - 224]");                                // base of raw parent SOCKETs
    emitter.instruction("mov rcx, QWORD PTR [rax + r9 * 8]");                   // parent SOCKET for this descriptor
    emitter.instruction("call closesocket");                                    // release the parent's raw socket endpoint
    emitter.instruction("mov r9, QWORD PTR [rbp - 520]");                       // reload cleanup j after Winsock call
    emitter.instruction("lea rax, [rbp - 288]");                                // base of raw child SOCKETs
    emitter.instruction("mov rcx, QWORD PTR [rax + r9 * 8]");                   // child SOCKET for this descriptor
    emitter.instruction("call closesocket");                                    // release the child's raw socket endpoint
    emitter.label("__rt_proc_open_cleanup_next_win");
    emitter.instruction("mov r9, QWORD PTR [rbp - 520]");                       // reload cleanup j
    emitter.instruction("inc r9");                                              // j += 1
    emitter.instruction("mov QWORD PTR [rbp - 520], r9");                       // persist cleanup j
    emitter.instruction("jmp __rt_proc_open_cleanup_test_win");                 // continue cleanup

    emitter.label("__rt_proc_open_cleanup_nul_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 144]");                      // nul_handle[0]
    emitter.instruction("test rcx, rcx");                                       // was stdin redirected to NUL?
    emitter.instruction("jz __rt_proc_open_cleanup_nul1_win");                  // not opened -> skip
    emitter.instruction("call CloseHandle");                                    // close the stdin NUL handle
    emitter.label("__rt_proc_open_cleanup_nul1_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 152]");                      // nul_handle[1]
    emitter.instruction("test rcx, rcx");                                       // was stdout redirected to NUL?
    emitter.instruction("jz __rt_proc_open_cleanup_nul2_win");                  // not opened -> skip
    emitter.instruction("call CloseHandle");                                    // close the stdout NUL handle
    emitter.label("__rt_proc_open_cleanup_nul2_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 160]");                      // nul_handle[2]
    emitter.instruction("test rcx, rcx");                                       // was stderr redirected to NUL?
    emitter.instruction("jz __rt_proc_open_cleanup_cmdbuf_win");                // not opened -> skip
    emitter.instruction("call CloseHandle");                                    // close the stderr NUL handle

    emitter.label("__rt_proc_open_cleanup_cmdbuf_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 576]");                      // optional wide environment pointer
    emitter.instruction("test rax, rax");                                       // was the environment converted?
    emitter.instruction("jz __rt_proc_open_cleanup_cwd_wide_win");              // no -> continue cwd cleanup
    emitter.instruction("call __rt_heap_free");                                 // release the UTF-16 environment block
    emitter.label("__rt_proc_open_cleanup_cwd_wide_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 552]");                      // optional wide cwd pointer
    emitter.instruction("test rax, rax");                                       // was cwd converted?
    emitter.instruction("jz __rt_proc_open_cleanup_cwd_narrow_win");            // no -> skip wide cwd free
    emitter.instruction("call __rt_heap_free");                                 // release UTF-16 cwd
    emitter.label("__rt_proc_open_cleanup_cwd_narrow_win");
    emitter.instruction("cmp QWORD PTR [rbp - 544], 0");                        // was cwd narrow staging allocated?
    emitter.instruction("je __rt_proc_open_cleanup_wide_cmdbuf_win");           // no -> continue command cleanup
    emitter.instruction("call GetProcessHeap");                                 // rax = process heap
    emitter.instruction("mov rcx, rax");                                        // heap handle
    emitter.instruction("xor edx, edx");                                        // free flags = 0
    emitter.instruction("mov r8, QWORD PTR [rbp - 544]");                       // cwd narrow staging pointer
    emitter.instruction("call HeapFree");                                       // release cwd staging
    emitter.label("__rt_proc_open_cleanup_wide_cmdbuf_win");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // wide_cmdbuf pointer (0 if conversion never succeeded)
    emitter.instruction("test rax, rax");                                       // was the UTF-16 buffer allocated?
    emitter.instruction("jz __rt_proc_open_cleanup_narrow_cmdbuf_win");         // no -> continue with the narrow staging buffer
    emitter.instruction("call __rt_heap_free");                                 // release the strict UTF-16 conversion result
    emitter.label("__rt_proc_open_cleanup_narrow_cmdbuf_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 136]");                      // cmdbuf pointer (0 if never allocated)
    emitter.instruction("test rcx, rcx");                                       // was cmdbuf allocated?
    emitter.instruction("jz __rt_proc_open_fail_win");                          // not allocated -> nothing to free
    emitter.instruction("call GetProcessHeap");                                 // rax = default process heap
    emitter.instruction("mov rcx, rax");                                        // heap handle (arg1)
    emitter.instruction("xor edx, edx");                                        // dwFlags = 0 (arg2)
    emitter.instruction("mov r8, QWORD PTR [rbp - 136]");                       // lpMem = cmdbuf (arg3)
    emitter.instruction("call HeapFree");                                       // release the command-line buffer

    // -- failure: return -1 (lowerer boxes as PHP false) --
    emitter.label("__rt_proc_open_fail_win");
    emitter.instruction("mov rax, -1");                                         // report proc_open failure
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the original/current pipes container on failure
    emitter.instruction("add rsp, 896");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// Emits a private AF_INET loopback socket-pair constructor for Windows
/// `proc_open(["socket"])`. This is intentionally distinct from the public
/// AF_UNIX `socketpair` shim, which remains unsupported on PHP Windows.
///
/// SysV input: `rdi` = parent SOCKET out pointer, `rsi` = child SOCKET out
/// pointer. Returns `0` or `-1` with the Winsock failure mapped to errno.
fn emit_proc_open_socketpair_win(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc_open private loopback socket pair ---");
    emitter.label_global("__rt_proc_open_socketpair_win");
    emitter.instruction("sub rsp, 120");                                        // shadow space, sockaddr_in, sockets, outputs, and saved WSA error
    emitter.instruction("mov QWORD PTR [rsp + 96], rdi");                       // retain parent output pointer
    emitter.instruction("mov QWORD PTR [rsp + 104], rsi");                      // retain child output pointer
    emitter.instruction("mov QWORD PTR [rsp + 56], -1");                        // listener = INVALID_SOCKET
    emitter.instruction("mov QWORD PTR [rsp + 64], -1");                        // child client = INVALID_SOCKET
    emitter.instruction("mov QWORD PTR [rsp + 72], -1");                        // parent accepted = INVALID_SOCKET
    emitter.instruction("mov WORD PTR [rsp + 32], 2");                          // sockaddr_in.sin_family = AF_INET
    emitter.instruction("mov WORD PTR [rsp + 34], 0");                          // request an ephemeral loopback port
    emitter.instruction("mov DWORD PTR [rsp + 36], 0x0100007f");                // sin_addr = network-order 127.0.0.1
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // clear sin_zero padding
    emitter.instruction("mov DWORD PTR [rsp + 48], 16");                        // sizeof(sockaddr_in)
    emitter.instruction("mov ecx, 2");                                          // AF_INET
    emitter.instruction("mov edx, 1");                                          // SOCK_STREAM
    emitter.instruction("mov r8d, 6");                                          // IPPROTO_TCP
    emitter.instruction("call socket");                                         // create loopback listener
    emitter.instruction("cmp rax, -1");                                         // INVALID_SOCKET?
    emitter.instruction("je __rt_proc_open_socketpair_fail_win");               // capture WSA error and clean up
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // retain listener
    emitter.instruction("mov rcx, rax");                                        // listener SOCKET
    emitter.instruction("lea rdx, [rsp + 32]");                                 // loopback sockaddr_in
    emitter.instruction("mov r8d, 16");                                         // sockaddr length
    emitter.instruction("call bind");                                           // bind ephemeral loopback listener
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je __rt_proc_open_socketpair_fail_win");               // clean listener on bind failure
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // listener SOCKET
    emitter.instruction("mov edx, 1");                                          // backlog one connection
    emitter.instruction("call listen");                                         // begin accepting the private pair
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je __rt_proc_open_socketpair_fail_win");               // clean listener on listen failure
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // listener SOCKET
    emitter.instruction("lea rdx, [rsp + 32]");                                 // receive bound loopback address
    emitter.instruction("lea r8, [rsp + 48]");                                  // receive sockaddr length
    emitter.instruction("call getsockname");                                    // discover ephemeral port
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je __rt_proc_open_socketpair_fail_win");               // clean listener on getsockname failure
    emitter.instruction("mov ecx, 2");                                          // AF_INET
    emitter.instruction("mov edx, 1");                                          // SOCK_STREAM
    emitter.instruction("mov r8d, 6");                                          // IPPROTO_TCP
    emitter.instruction("xor r9d, r9d");                                        // lpProtocolInfo = NULL
    emitter.instruction("sub rsp, 48");                                         // isolated shadow and stack arguments preserve the outer sockaddr
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // g = 0
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // dwFlags = 0: child never uses overlapped I/O
    emitter.instruction("call WSASocketW");                                     // match php-src's non-overlapped child endpoint
    emitter.instruction("add rsp, 48");                                         // restore the socket-pair frame and bound sockaddr
    emitter.instruction("cmp rax, -1");                                         // INVALID_SOCKET?
    emitter.instruction("je __rt_proc_open_socketpair_fail_win");               // clean listener on socket failure
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // retain child client SOCKET
    emitter.instruction("mov rcx, rax");                                        // child client SOCKET
    emitter.instruction("lea rdx, [rsp + 32]");                                 // listener endpoint
    emitter.instruction("mov r8d, 16");                                         // sockaddr length
    emitter.instruction("call connect");                                        // connect parent endpoint to listener
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je __rt_proc_open_socketpair_fail_win");               // clean client/listener on connect failure
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // listener SOCKET
    emitter.instruction("xor edx, edx");                                        // caller does not need peer address
    emitter.instruction("xor r8d, r8d");                                        // caller does not need peer length
    emitter.instruction("call accept");                                         // accept child endpoint
    emitter.instruction("cmp rax, -1");                                         // INVALID_SOCKET?
    emitter.instruction("je __rt_proc_open_socketpair_fail_win");               // clean client/listener on accept failure
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // retain parent accepted SOCKET
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // listener is no longer required
    emitter.instruction("call closesocket");                                    // release listener before exposing endpoints
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // parent accepted SOCKET
    emitter.instruction("mov r9, QWORD PTR [rsp + 96]");                        // parent output pointer
    emitter.instruction("mov QWORD PTR [r9], rax");                             // publish parent raw SOCKET
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // child non-overlapped SOCKET
    emitter.instruction("mov r9, QWORD PTR [rsp + 104]");                       // child output pointer
    emitter.instruction("mov QWORD PTR [r9], rax");                             // publish child raw SOCKET
    emitter.instruction("xor eax, eax");                                        // pair construction succeeded
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return success
    emitter.label("__rt_proc_open_socketpair_fail_win");
    emitter.instruction("call WSAGetLastError");                                // capture Winsock error before close calls
    emitter.instruction("mov DWORD PTR [rsp + 88], eax");                       // preserve WSA error code
    for (offset, suffix) in [(72, "accepted"), (64, "client"), (56, "listener")] {
        emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {offset}]"));   // candidate SOCKET for cleanup
        emitter.instruction("cmp rcx, -1");                                     // endpoint was allocated?
        emitter.instruction(&format!("je __rt_proc_open_socketpair_skip_{suffix}_win")); // skip invalid endpoint
        emitter.instruction("call closesocket");                                // release allocated SOCKET
        emitter.label(&format!("__rt_proc_open_socketpair_skip_{suffix}_win"));
    }
    emitter.instruction("mov eax, DWORD PTR [rsp + 88]");                       // restore captured WSA error
    emitter.instruction("call __rt_win32_errno_from_code");                     // map Winsock error to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish mapped errno
    emitter.instruction("mov rax, -1");                                         // report pair construction failure
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return failure
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Structural tests for the Windows `proc_open` process-spawn emitter.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These assertions cover Unicode boundaries, handle cleanup, and errno
    //!   capture that compile-only PE tests cannot observe at runtime.

    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies Windows spawning uses strict UTF-16 conversion and only Wide
    /// Win32 entry points for the command line, file descriptors, and synthetic
    /// NUL handles.
    #[test]
    fn windows_proc_open_uses_strict_wide_process_apis() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_open_win32_x86_64(&mut emitter);
        let asm = emitter.output();
        assert_eq!(asm.matches("call __rt_win_utf8_to_utf16").count(), 3);
        assert!(asm.contains("call CreateProcessW"));
        assert!(asm.contains("call CreateFileW"));
        assert!(!asm.contains("CreateProcessA"));
        assert!(!asm.contains("CreateFileA"));
        assert!(asm.contains("repne scasb"), "embedded NUL must be rejected");
        assert!(asm.contains("mov QWORD PTR [rsp + 56], rax"));
        assert!(asm.contains("[rip + __rt_errno], 84"), "invalid UTF-8 must publish EILSEQ");
    }

    /// Verifies native failures preserve errno before cleanup and every owned
    /// process/pipe/command-line resource has an explicit release path.
    #[test]
    fn windows_proc_open_checks_inheritance_and_cleans_every_resource() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_open_win32_x86_64(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("call SetHandleInformation"));
        assert!(asm.contains("test eax, eax"));
        assert!(asm.contains("__rt_proc_open_capture_cleanup_win:"));
        assert!(asm.contains("call GetLastError"));
        assert!(asm.contains("call __rt_win32_errno_from_code"));
        assert!(asm.contains("call __rt_heap_free"), "wide command buffer must be freed");
        assert!(asm.contains("call HeapFree"), "narrow staging buffer must be freed");
        assert!(asm.matches("call CloseHandle").count() >= 8);
        assert!(asm.contains("__rt_proc_open_scalar_mode_read_win:"));
        assert!(asm.contains("__rt_proc_open_string_mode_win:"));
    }

    /// Verifies every native proc_open emitter accepts PHP-string-coercible
    /// scalar pipe modes through its shared non-write-leading direction path.
    #[test]
    fn proc_open_scalar_pipe_modes_follow_php_read_direction_on_every_target() {
        for (target, scalar_label, string_label) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "__rt_proc_open_scalar_mode_read:",
                "__rt_proc_open_string_mode:",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "__rt_proc_open_scalar_mode_read_x86:",
                "__rt_proc_open_string_mode_x86:",
            ),
            (
                Target::new(Platform::Windows, Arch::X86_64),
                "__rt_proc_open_scalar_mode_read_win:",
                "__rt_proc_open_string_mode_win:",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_proc_open(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(scalar_label), "missing scalar pipe-mode path for {target:?}");
            assert!(asm.contains(string_label), "missing string pipe-mode path for {target:?}");
        }
    }

    /// Verifies direct execution and custom environments use the packed runtime
    /// flags, counted strict conversion, and CreateProcessW Unicode contract.
    #[test]
    fn windows_proc_open_marshals_direct_commands_and_environment() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_open_win32_x86_64(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("test r9, 1"));
        assert!(asm.contains("__rt_proc_open_copy_command_win:"));
        assert!(asm.contains("shr rax, 5"));
        assert!(asm.matches("call MultiByteToWideChar").count() >= 2);
        assert!(asm.contains("or rax, 0x400"));
        assert!(asm.contains("or rax, 0x200"));
        assert!(asm.contains("or rax, 0x10"));
        assert!(asm.contains("__rt_proc_open_create_failure_mode_restored_win"));
        assert!(asm.contains("mov QWORD PTR [rsp + 48], rax"));
        assert!(asm.matches("mov rax, QWORD PTR [rbp - 576]").count() >= 3);
    }

    /// Verifies parent pipe handles become CRT descriptors before the kind-1
    /// resources are published, and that default pipe reads acquire the
    /// cached O_NONBLOCK state consumed by the Win32 ReadFile shim.
    #[test]
    fn windows_proc_open_adopts_parent_handles_and_sets_default_nonblocking_reads() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_open_win32_x86_64(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("call _open_osfhandle"));
        assert!(asm.contains("cmp eax, -1"));
        assert!(asm.contains("mov edx, 0x8000"));
        assert!(asm.contains("or edx, 1"));
        assert!(asm.contains("or QWORD PTR [rax + r9 * 8], 4"));
        assert!(asm.contains("test QWORD PTR [rbp - 568], 4"));
        assert!(asm.contains("mov rsi, 0x800"));
        assert!(asm.contains("call __rt_win_fd_status_upsert"));
        assert!(asm.contains("call __rt_sys_close"));
        assert!(asm.contains("__rt_proc_open_parent_fd_cleanup_raw_win:"));
    }

    /// Verifies sparse associative descriptor specs retain integer keys through
    /// process setup, publish resources with the keyed setter, and return the
    /// promoted `$pipes` container in the x86_64 secondary result register.
    #[test]
    fn windows_proc_open_preserves_sparse_descriptor_keys_and_pipes_result() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_open_win32_x86_64(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("call __rt_hash_iter_next"));
        assert!(asm.contains("__rt_proc_open_indexed_key_win:"));
        assert!(asm.contains("lea r11, [rbp - 784]"));
        assert!(asm.contains("call __rt_array_set_mixed_key"));
        assert!(asm.contains("mov QWORD PTR [rbp - 40], rax"));
        assert!(asm.contains("mov rdx, QWORD PTR [rbp - 40]"));
        assert!(!asm.contains("call __rt_array_push_refcounted"));
    }

    /// Verifies Windows descriptor dispatch owns only child handles for
    /// `null`, `redirect`, stream-resource, and file sources. Pipe endpoints
    /// remain the sole values published to the caller's `$pipes` array.
    #[test]
    fn windows_proc_open_supports_child_only_descriptor_sources() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_open_win32_x86_64(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_proc_open_null_descriptor_win:"));
        assert!(asm.contains("__rt_proc_open_redirect_descriptor_win:"));
        assert!(asm.contains("__rt_proc_open_resource_descriptor_win:"));
        assert!(asm.contains("__rt_proc_open_file_descriptor_win:"));
        assert!(asm.contains("call DuplicateHandle"));
        assert!(asm.contains("call GetStdHandle"));
        assert!(asm.contains("call _get_osfhandle"));
        assert!(asm.contains("call SetFilePointerEx"));
        assert!(asm.contains("repne scasb"));
        assert!(asm.contains("[rip + __rt_errno], 9"));
        assert!(asm.contains("mov QWORD PTR [r10 + r9 * 8], 16"));
        assert!(asm.contains("test r11, 1"));
        assert!(asm.contains("__rt_proc_open_cleanup_child_only_win:"));
    }

    /// Verifies Windows `proc_open(["socket"])` owns a private TCP loopback
    /// pair, publishes only the raw parent endpoint, and never re-enables the
    /// public AF_UNIX socketpair API.
    #[test]
    fn windows_proc_open_supports_private_socket_descriptors() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_open_win32_x86_64(&mut emitter);
        emit_proc_open_socketpair_win(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_proc_open_socket_descriptor_win:"));
        assert!(asm.contains("__rt_proc_open_socketpair_win:"));
        assert!(asm.contains("call bind"));
        assert!(asm.contains("call listen"));
        assert!(asm.contains("call WSASocketW"));
        assert!(asm.contains("call connect"));
        assert!(asm.contains("call accept"));
        assert!(asm.contains("test r11, 8"));
        assert!(asm.contains("call closesocket"));
        assert!(asm.contains("call __rt_win32_errno_from_code"));
        assert!(asm.contains("__rt_proc_open_parent_fd_cleanup_socket_win:"));
        let socket_descriptor = asm
            .split("__rt_proc_open_socket_descriptor_win:")
            .nth(1)
            .and_then(|tail| tail.split("__rt_proc_open_resource_descriptor_win:").next())
            .expect("socket descriptor emitter section");
        assert!(!socket_descriptor.contains("call SetHandleInformation"));
        assert_eq!(socket_descriptor.matches("call DuplicateHandle").count(), 1);
        assert!(socket_descriptor.contains("mov QWORD PTR [rsp + 48], 3"));
        assert!(!asm.contains("call __rt_sys_socketpair"));
    }
}
