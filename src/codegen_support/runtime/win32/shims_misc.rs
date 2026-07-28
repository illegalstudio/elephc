//! Win32 shims for the remaining syscall/libc grab-bag: unsupported-syscall
//! fallback, exit, argv init, mmap/munmap/brk, getpid/getrandom, the math/FP
//! family, strtod/strtol/snprintf, ioctl, dup/getuid/kill/uname/sysinfo,
//! execve/futex/mprotect.

use crate::codegen::emit::Emitter;

/// Emits the `__rt_unsupported_syscall` diagnostic helper.
///
/// The Windows syscall->shim transform rewrites any Linux syscall number with
/// no Win32 shim into `mov eax, <N>` + `call __rt_unsupported_syscall` (instead
/// of a silent `int3`). Entered with the Linux syscall number in `eax`, this
/// helper prints `unsupported syscall: <N>\n` to stderr via a manual base-10
/// itoa (no libc) and calls `ExitProcess(70)`. It is emitted unconditionally so
/// the transform always has a target, even if unreferenced in a given build.
pub(super) fn emit_shim_unsupported_syscall(emitter: &mut Emitter) {
    emitter.label_global("__rt_unsupported_syscall");
    emitter.instruction("sub rsp, 120");                                        // frame: shadow(32)+overlapped+written+msg+itoa scratch, 16B aligned
    emitter.instruction("mov r15d, eax");                                       // stash syscall number (eax is clobbered by every Win32 call)
    // -- copy the "unsupported syscall: " prefix into the message buffer --
    emitter.instruction("cld");                                                 // ensure forward direction for the string copy
    emitter.instruction("lea rsi, [rip + __rt_unsup_prefix]");                  // source = constant prefix string
    emitter.instruction("lea rdi, [rsp + 48]");                                 // dest = message buffer start
    emitter.instruction("mov ecx, 21");                                         // prefix length (\"unsupported syscall: \")
    emitter.instruction("rep movsb");                                           // copy the 21 prefix bytes; rdi now points past the prefix
    // -- manual base-10 itoa of the number into scratch (least-significant first) --
    emitter.instruction("mov eax, r15d");                                       // working value = syscall number
    emitter.instruction("lea rsi, [rsp + 112]");                                // rsi = one past the itoa scratch end
    emitter.instruction("mov ecx, 10");                                         // decimal divisor
    emitter.label(".Lunsup_itoa");
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing
    emitter.instruction("div ecx");                                             // eax /= 10, edx = eax % 10
    emitter.instruction("add dl, 48");                                          // convert the remainder to an ASCII digit ('0')
    emitter.instruction("dec rsi");                                             // move one byte toward the front of the scratch
    emitter.instruction("mov [rsi], dl");                                       // store the digit
    emitter.instruction("test eax, eax");                                       // any higher-order digits remaining?
    emitter.instruction("jnz .Lunsup_itoa");                                    // loop until the quotient reaches zero
    // -- append the digits (now in order at [rsi..scratch_end]) after the prefix --
    emitter.instruction("lea rdx, [rsp + 112]");                                // sentinel = itoa scratch end
    emitter.label(".Lunsup_copy");
    emitter.instruction("mov al, [rsi]");                                       // load the next digit
    emitter.instruction("mov [rdi], al");                                       // append it to the message buffer
    emitter.instruction("inc rsi");                                             // advance the source pointer
    emitter.instruction("inc rdi");                                             // advance the destination pointer
    emitter.instruction("cmp rsi, rdx");                                        // reached the end of the digits?
    emitter.instruction("jne .Lunsup_copy");                                    // keep copying digits
    emitter.instruction("mov BYTE PTR [rdi], 10");                              // append a trailing newline
    emitter.instruction("inc rdi");                                             // include the newline in the length
    emitter.instruction("lea rax, [rsp + 48]");                                 // message buffer start
    emitter.instruction("sub rdi, rax");                                        // rdi = total message length
    emitter.instruction("mov r14, rdi");                                        // stash length in a callee-saved reg (survives the calls)
    // -- WriteFile(stderr, &msg, len, &written, NULL) --
    emitter.instruction("mov ecx, -12");                                        // STD_ERROR_HANDLE
    emitter.instruction("call GetStdHandle");                                   // rax = stderr handle
    emitter.instruction("mov rcx, rax");                                        // handle (arg1)
    emitter.instruction("lea rdx, [rsp + 48]");                                 // &msg (arg2)
    emitter.instruction("mov r8, r14");                                         // len (arg3)
    emitter.instruction("lea r9, [rsp + 40]");                                  // &written (arg4), off the arg5 slot
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // lpOverlapped = NULL (arg5 slot)
    emitter.instruction("call WriteFile");                                      // write the diagnostic to stderr
    // -- ExitProcess(70) (never returns) --
    emitter.instruction("mov ecx, 70");                                         // process exit code 70
    emitter.instruction("call ExitProcess");                                    // terminate the process
    // -- constant prefix string in .data --
    emitter.raw(".data");
    emitter.raw("__rt_unsup_prefix:");
    emitter.raw("    .ascii \"unsupported syscall: \"");
    emitter.raw(".text");
    emitter.blank();
}

/// Emits a shim that calls `ExitProcess` with the exit code from rdi.
///
/// Forces 16-byte stack alignment before the call so both the implicit
/// end-of-main exit (enters the shim at rsp = 0 mod 16, after the epilogue's
/// `pop rbp` leaves it at 8) and explicit `exit($n)` reach `ExitProcess`
/// correctly aligned; Wine's process-exit path uses aligned SSE and faults
/// otherwise. The shim never returns, so clobbering rsp with the `and` is safe.
///
/// Alignment arithmetic: `and rsp, -16` forces rsp ≡ 0 mod 16 (the shim can be
/// reached at any alignment, so it must force-align). After that, the prologue
/// `sub rsp, N` MUST have `N ≡ 0 mod 16` so rsp stays ≡ 0 at each `call` site —
/// the opposite rule from shims entered normally (rsp ≡ 8 at entry, which need
/// `N ≡ 8 mod 16`). Using `N ≡ 8` here (e.g. 40) would leave rsp ≡ 8 at the
/// `call __rt_winsock_cleanup` and `call ExitProcess` sites, misaligning the
/// SSE registers Wine's process-exit path reads and crashing with a #GP.
pub(super) fn emit_shim_exit(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_exit");
    emitter.instruction("and rsp, -16");                                        // force rsp ≡ 0 mod 16 before any Win32 call (shim never returns; clobbering rsp is safe)
    emitter.instruction("sub rsp, 48");                                         // shadow(32) + spill(8) + pad(8), 16-byte aligned (and rsp,-16 forces ≡0, so sub must be ≡0 mod 16)
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // spill the exit code (rdi is volatile on MSx64) across the cleanup call
    // -- release process-bootstrap argument storage before terminating --
    emitter.instruction("call __rt_sys_free_argv");                             // release tracked UTF-8 argv strings and their pointer table
    // -- release Winsock resources before terminating --
    emitter.instruction("call __rt_winsock_cleanup");                           // WSACleanup() — safe to call even if WSAStartup was never invoked
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // MSx64 arg1 = exit code (reloaded after the cleanup call)
    emitter.instruction("call ExitProcess");                                    // terminate the process (never returns)
    emitter.blank();
}

/// Emits the non-returning emergency exit used when a generated function
/// prologue detects that its next frame would consume an active Fiber guard.
///
/// This path intentionally skips heap, argv, Winsock, and Fiber cleanup: the
/// current stack is nearly exhausted, so invoking arbitrary cleanup code could
/// consume the remaining emergency reserve and fault before `ExitProcess`.
pub(super) fn emit_fiber_stack_overflow_abort(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_fiber_stack_overflow_abort");
    emitter.instruction("and rsp, -16");                                        // force a known alignment on the still-usable Fiber stack
    emitter.instruction("sub rsp, 32");                                         // reserve only the mandatory MSx64 shadow space
    emitter.instruction("mov ecx, 134");                                        // conventional abort-style nonzero process status
    emitter.instruction("call ExitProcess");                                    // terminate immediately without unsafe low-stack cleanup
    emitter.instruction("ud2");                                                 // preserve non-returning semantics if ExitProcess unexpectedly returns
    emitter.blank();
}

/// Emits the `__rt_sys_init_argv` shim that populates `_global_argc` and
/// `_global_argv` from the Win32 command line on Windows x86_64.
///
/// The shim ignores the width-ambiguous CRT entry registers and asks shell32 to
/// parse `GetCommandLineW` with the native Windows quote/backslash rules. Each
/// returned UTF-16 argument is strictly converted into its own persistent UTF-8
/// allocation; the shell32 array is released with `LocalFree`, while partial
/// failures unwind every argument and the pointer table before publishing a safe
/// empty bootstrap state.
pub(super) fn emit_shim_sys_init_argv(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_init_argv");
    emitter.instruction("sub rsp, 72");                                         // shadow space and argc/argv/loop ownership locals
    emitter.instruction("call GetCommandLineW");                                // retrieve immutable UTF-16 process command line
    emitter.instruction("mov rcx, rax");                                        // lpCmdLine
    emitter.instruction("lea rdx, [rsp + 32]");                                 // pNumArgs
    emitter.instruction("call CommandLineToArgvW");                             // apply native Windows quoting/backslash rules
    emitter.instruction("test rax, rax");                                       // parsing succeeded?
    emitter.instruction("jz .Linit_argv_w_parse_fail");                         // capture shell32 failure
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // LocalFree-owned LPWSTR array
    emitter.instruction("mov eax, DWORD PTR [rsp + 32]");                       // clean zero-extended argc
    emitter.instruction("mov DWORD PTR [rsp + 36], eax");                       // preserve argc separately from shell output
    emitter.instruction("movsxd rax, eax");                                     // widen argc for pointer-table allocation
    emitter.instruction("shl rax, 3");                                          // one UTF-8 pointer per argument
    emitter.instruction("call __rt_heap_alloc");                                // allocate persistent UTF-8 argv table
    emitter.instruction("test rax, rax");                                       // table allocation succeeded?
    emitter.instruction("jz .Linit_argv_w_table_oom");                          // release shell32 array with ENOMEM
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // owned UTF-8 pointer table
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // conversion index
    emitter.label(".Linit_argv_w_loop");
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // current index
    emitter.instruction("mov ecx, DWORD PTR [rsp + 36]");                       // argc
    emitter.instruction("cmp rax, rcx");                                        // converted every argument?
    emitter.instruction("jae .Linit_argv_w_done");                              // publish converted argv
    emitter.instruction("mov r10, QWORD PTR [rsp + 40]");                       // shell32 wide argv table
    emitter.instruction("mov rdi, QWORD PTR [r10 + rax * 8]");                  // LPWSTR argv[index]
    emitter.instruction("mov QWORD PTR [rsp + 64], rdi");                       // preserve wide source across allocation
    emitter.instruction("xor esi, esi");                                        // query UTF-8 size with no destination
    emitter.instruction("xor edx, edx");                                        // destination capacity zero
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // required UTF-8 bytes including NUL
    emitter.instruction("test eax, eax");                                       // strict conversion size query succeeded?
    emitter.instruction("jz .Linit_argv_w_conversion_fail");                    // cleanup completed arguments
    emitter.instruction("mov DWORD PTR [rsp + 32], eax");                       // preserve exact byte capacity
    emitter.instruction("movsxd rax, eax");                                     // allocation size
    emitter.instruction("call __rt_heap_alloc");                                // allocate persistent UTF-8 argument
    emitter.instruction("test rax, rax");                                       // argument allocation succeeded?
    emitter.instruction("jz .Linit_argv_w_arg_alloc_fail");                     // cleanup completed arguments
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // UTF-8 table
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // current index
    emitter.instruction("mov QWORD PTR [r10 + rcx * 8], rax");                  // table[index] owns UTF-8 allocation
    emitter.instruction("mov rdi, QWORD PTR [rsp + 64]");                       // wide source
    emitter.instruction("mov rsi, rax");                                        // UTF-8 destination
    emitter.instruction("mov edx, DWORD PTR [rsp + 32]");                       // exact byte capacity
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // strict conversion including NUL
    emitter.instruction("test eax, eax");                                       // final conversion succeeded?
    emitter.instruction("jz .Linit_argv_w_current_conversion_fail");            // free current and prior arguments
    emitter.instruction("add QWORD PTR [rsp + 56], 1");                         // advance converted count
    emitter.instruction("jmp .Linit_argv_w_loop");                              // convert next native argument
    emitter.label(".Linit_argv_w_done");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");                       // shell32 allocation
    emitter.instruction("call LocalFree");                                      // release UTF-16 argv array after copying
    emitter.instruction("mov eax, DWORD PTR [rsp + 36]");                       // clean argc
    emitter.instruction("mov QWORD PTR [rip + _global_argc], rax");             // publish argc
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // persistent UTF-8 pointer table
    emitter.instruction("mov QWORD PTR [rip + _global_argv], rax");             // publish argv
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return to process bootstrap
    emitter.label(".Linit_argv_w_current_conversion_fail");
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // UTF-8 table
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // current index
    emitter.instruction("mov rax, QWORD PTR [r10 + rcx * 8]");                  // current failed allocation
    emitter.instruction("call __rt_heap_free");                                 // release current argument
    emitter.label(".Linit_argv_w_conversion_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 84");                // EILSEQ
    emitter.instruction("jmp .Linit_argv_w_cleanup");                           // cleanup prior arguments
    emitter.label(".Linit_argv_w_arg_alloc_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.label(".Linit_argv_w_cleanup");
    emitter.instruction("cmp QWORD PTR [rsp + 56], 0");                         // any completed argument allocations?
    emitter.instruction("je .Linit_argv_w_cleanup_table");                      // free pointer table
    emitter.instruction("sub QWORD PTR [rsp + 56], 1");                         // previous completed index
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // UTF-8 table
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // index to free
    emitter.instruction("mov rax, QWORD PTR [r10 + rcx * 8]");                  // owned UTF-8 argument
    emitter.instruction("call __rt_heap_free");                                 // release completed argument
    emitter.instruction("jmp .Linit_argv_w_cleanup");                           // continue reverse cleanup
    emitter.label(".Linit_argv_w_cleanup_table");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // UTF-8 pointer table
    emitter.instruction("call __rt_heap_free");                                 // release failed table
    emitter.instruction("jmp .Linit_argv_w_table_alloc_fail");                  // preserve existing errno
    emitter.label(".Linit_argv_w_table_oom");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.label(".Linit_argv_w_table_alloc_fail");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");                       // shell32 UTF-16 argv allocation
    emitter.instruction("call LocalFree");                                      // release native parsed argv
    emitter.instruction("mov QWORD PTR [rip + _global_argc], 0");               // publish safe empty bootstrap state
    emitter.instruction("mov QWORD PTR [rip + _global_argv], 0");               // no argv table
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return with failure state
    emitter.label(".Linit_argv_w_parse_fail");
    emitter.instruction("call GetLastError");                                   // capture CommandLineToArgvW failure
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native error
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov QWORD PTR [rip + _global_argc], 0");               // safe empty argc
    emitter.instruction("mov QWORD PTR [rip + _global_argv], 0");               // safe empty argv
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return to bootstrap
    emitter.blank();
}

/// Emits the `__rt_sys_free_argv` shim that releases the tracked UTF-8 process
/// arguments allocated by [`emit_shim_sys_init_argv`].
///
/// The globals are cleared after releasing every string and the pointer table,
/// so process-exit paths may call the helper defensively more than once. The
/// cleanup deliberately uses the runtime heap rather than the Win32 process
/// heap because argv bootstrap allocations participate in GC/heap diagnostics.
pub(super) fn emit_shim_sys_free_argv(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_free_argv");
    emitter.instruction("sub rsp, 40");                                         // align nested runtime calls and reserve table/count/index locals
    emitter.instruction("mov rax, QWORD PTR [rip + _global_argv]");             // load the owned UTF-8 pointer table
    emitter.instruction("test rax, rax");                                       // was argv bootstrap initialized successfully?
    emitter.instruction("jz .Lfree_argv_done");                                 // no table means there is nothing to release
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // preserve table across heap-free calls
    emitter.instruction("mov rax, QWORD PTR [rip + _global_argc]");             // load the number of owned argument strings
    emitter.instruction("mov QWORD PTR [rsp + 8], rax");                        // preserve argc across heap-free calls
    emitter.instruction("mov QWORD PTR [rsp + 16], 0");                         // start at argv[0]
    emitter.label(".Lfree_argv_loop");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                       // current argument index
    emitter.instruction("cmp rcx, QWORD PTR [rsp + 8]");                        // have all owned strings been released?
    emitter.instruction("jae .Lfree_argv_table");                               // release the table after its entries
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the still-live pointer table
    emitter.instruction("mov rax, QWORD PTR [r10 + rcx * 8]");                  // load argv[index] for the heap-free ABI
    emitter.instruction("call __rt_heap_free");                                 // release the owned UTF-8 argument string
    emitter.instruction("add QWORD PTR [rsp + 16], 1");                         // advance after the clobbering runtime call
    emitter.instruction("jmp .Lfree_argv_loop");                                // continue through every process argument
    emitter.label(".Lfree_argv_table");
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // load the owned pointer table itself
    emitter.instruction("call __rt_heap_free");                                 // release the UTF-8 pointer table last
    emitter.instruction("mov QWORD PTR [rip + _global_argc], 0");               // make repeated cleanup a no-op
    emitter.instruction("mov QWORD PTR [rip + _global_argv], 0");               // clear the released table pointer
    emitter.label(".Lfree_argv_done");
    emitter.instruction("add rsp, 40");                                         // restore caller stack
    emitter.instruction("ret");                                                 // return to diagnostics or process exit
    emitter.blank();
}

/// Emits the former in-place ANSI parser, retained only for regression reference.
#[allow(dead_code)]
fn emit_shim_sys_init_argv_legacy(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_init_argv");
    emitter.instruction("sub rsp, 40");                                         // shadow space (32) + alignment (8)
    // -- fetch the mutable Win32 command line --
    emitter.instruction("xor eax, eax");                                        // dead legacy fixture intentionally owns no ANSI import
    emitter.instruction("mov rsi, rax");                                        // rsi = cmdline cursor (preserved across Win32 calls)
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // save immutable cmdline START for pass-2 restart (pad slot above the 32-byte shadow; untouched by GetProcessHeap/HeapAlloc)
    // -- pass 1: count whitespace-delimited tokens into rdi (argc) --
    emitter.instruction("xor rdi, rdi");                                        // rdi = argc = 0
    emitter.label(".Linit_argv_count_skip_ws");
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load next command-line byte
    emitter.instruction("test dl, dl");                                         // check for the terminating NUL
    emitter.instruction("jz .Linit_argv_count_done");                           // end of command line -> stop counting
    emitter.instruction("cmp dl, 32");                                          // is it a space?
    emitter.instruction("je .Linit_argv_count_ws_adv");                         // skip the space
    emitter.instruction("cmp dl, 9");                                           // is it a tab?
    emitter.instruction("je .Linit_argv_count_ws_adv");                         // skip the tab
    emitter.instruction("inc rdi");                                             // start a new token -> argc += 1
    emitter.instruction("cmp dl, 34");                                          // does the token open with a double quote?
    emitter.instruction("jne .Linit_argv_count_unquoted");                      // unquoted token -> scan to whitespace
    emitter.instruction("add rsi, 1");                                          // skip the opening double quote
    emitter.label(".Linit_argv_count_quoted_loop");
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load next quoted-token byte
    emitter.instruction("test dl, dl");                                         // check for the terminating NUL (unbalanced quote)
    emitter.instruction("jz .Linit_argv_count_done");                           // unbalanced quote -> treat as end of command line
    emitter.instruction("cmp dl, 34");                                          // look for the closing double quote
    emitter.instruction("je .Linit_argv_count_quoted_end");                     // closing quote found -> token ends
    emitter.instruction("add rsi, 1");                                          // advance within the quoted token
    emitter.instruction("jmp .Linit_argv_count_quoted_loop");                   // continue scanning the quoted token
    emitter.label(".Linit_argv_count_quoted_end");
    emitter.instruction("add rsi, 1");                                          // skip the closing double quote
    emitter.label(".Linit_argv_count_quoted_tail");
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load the byte after the closing quote
    emitter.instruction("test dl, dl");                                         // check for the terminating NUL
    emitter.instruction("jz .Linit_argv_count_done");                           // end of command line -> stop counting
    emitter.instruction("cmp dl, 32");                                          // is it a space?
    emitter.instruction("je .Linit_argv_count_ws_adv");                         // whitespace after the token -> skip it
    emitter.instruction("cmp dl, 9");                                           // is it a tab?
    emitter.instruction("je .Linit_argv_count_ws_adv");                         // whitespace after the token -> skip it
    emitter.instruction("add rsi, 1");                                          // advance past any trailing non-whitespace
    emitter.instruction("jmp .Linit_argv_count_quoted_tail");                   // keep scanning the token tail
    emitter.label(".Linit_argv_count_unquoted");
    emitter.instruction("add rsi, 1");                                          // advance past the first token byte
    emitter.label(".Linit_argv_count_unquoted_loop");
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load next unquoted-token byte
    emitter.instruction("test dl, dl");                                         // check for the terminating NUL
    emitter.instruction("jz .Linit_argv_count_done");                           // end of command line -> stop counting
    emitter.instruction("cmp dl, 32");                                          // is it a space?
    emitter.instruction("je .Linit_argv_count_ws_adv");                         // whitespace -> token ends
    emitter.instruction("cmp dl, 9");                                           // is it a tab?
    emitter.instruction("je .Linit_argv_count_ws_adv");                         // whitespace -> token ends
    emitter.instruction("add rsi, 1");                                          // advance within the unquoted token
    emitter.instruction("jmp .Linit_argv_count_unquoted_loop");                 // continue scanning the unquoted token
    emitter.label(".Linit_argv_count_ws_adv");
    emitter.instruction("add rsi, 1");                                          // advance past one whitespace byte
    emitter.instruction("jmp .Linit_argv_count_skip_ws");                       // resume whitespace skipping / token dispatch
    emitter.label(".Linit_argv_count_done");
    // -- allocate the argv pointer array: HeapAlloc(GetProcessHeap(), 0, argc*8) --
    emitter.instruction("call GetProcessHeap");                                 // rax = default process heap (rdi/rsi preserved)
    emitter.instruction("mov rcx, rax");                                        // rcx = heap handle (MSx64 arg1)
    emitter.instruction("xor rdx, rdx");                                        // rdx = flags = 0 (MSx64 arg2)
    emitter.instruction("mov r8, rdi");                                         // r8 = argc (MSx64 arg3 source)
    emitter.instruction("shl r8, 3");                                           // r8 = argc * 8 bytes (one char* per slot)
    emitter.instruction("call HeapAlloc");                                      // rax = argv pointer array base (rdi/rsi preserved)
    emitter.instruction("mov r8, rax");                                         // r8 = argv array base for pass 2
    // -- pass 2: re-walk the command line, null-terminate tokens, fill argv[i] --
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // reload cmdline START for pass 2 (rsi was consumed as the pass-1 scan cursor)
    emitter.instruction("xor r9, r9");                                          // r9 = token index i = 0
    emitter.label(".Linit_argv_fill_skip_ws");
    emitter.instruction("mov dl, BYTE PTR [rax]");                              // load next command-line byte
    emitter.instruction("test dl, dl");                                         // check for the terminating NUL
    emitter.instruction("jz .Linit_argv_fill_done");                            // end of command line -> stop filling
    emitter.instruction("cmp dl, 32");                                          // is it a space?
    emitter.instruction("je .Linit_argv_fill_ws_adv");                          // skip the space
    emitter.instruction("cmp dl, 9");                                           // is it a tab?
    emitter.instruction("je .Linit_argv_fill_ws_adv");                          // skip the tab
    emitter.instruction("cmp dl, 34");                                          // does the token open with a double quote?
    emitter.instruction("jne .Linit_argv_fill_unquoted");                       // unquoted token -> record start and scan to whitespace
    emitter.instruction("lea r10, [rax + 1]");                                  // r10 = token start (after the opening quote, quotes stripped)
    emitter.instruction("add rax, 1");                                          // advance past the opening double quote
    emitter.label(".Linit_argv_fill_quoted_loop");
    emitter.instruction("mov dl, BYTE PTR [rax]");                              // load next quoted-token byte
    emitter.instruction("test dl, dl");                                         // check for the terminating NUL (unbalanced quote)
    emitter.instruction("jz .Linit_argv_fill_store");                           // unbalanced quote -> store the partial token
    emitter.instruction("cmp dl, 34");                                          // look for the closing double quote
    emitter.instruction("je .Linit_argv_fill_close_quote");                     // closing quote found -> null-terminate here
    emitter.instruction("add rax, 1");                                          // advance within the quoted token
    emitter.instruction("jmp .Linit_argv_fill_quoted_loop");                    // continue scanning the quoted token
    emitter.label(".Linit_argv_fill_close_quote");
    emitter.instruction("mov BYTE PTR [rax], 0");                               // null-terminate at the closing quote position (strip the quote)
    emitter.instruction("add rax, 1");                                          // advance past the now-NUL position
    emitter.instruction("mov QWORD PTR [r8 + r9 * 8], r10");                    // argv[i] = token start pointer
    emitter.instruction("add r9, 1");                                           // i += 1
    emitter.instruction("jmp .Linit_argv_fill_skip_ws");                        // resume after the just-terminated token
    emitter.label(".Linit_argv_fill_unquoted");
    emitter.instruction("mov r10, rax");                                        // r10 = token start (unquoted, no stripping)
    emitter.instruction("add rax, 1");                                          // advance past the first token byte
    emitter.label(".Linit_argv_fill_unquoted_loop");
    emitter.instruction("mov dl, BYTE PTR [rax]");                              // load next unquoted-token byte
    emitter.instruction("test dl, dl");                                         // check for the terminating NUL
    emitter.instruction("jz .Linit_argv_fill_store");                           // end of command line -> store the token
    emitter.instruction("cmp dl, 32");                                          // is it a space?
    emitter.instruction("je .Linit_argv_fill_term_unquoted");                   // whitespace -> null-terminate and store
    emitter.instruction("cmp dl, 9");                                           // is it a tab?
    emitter.instruction("je .Linit_argv_fill_term_unquoted");                   // whitespace -> null-terminate and store
    emitter.instruction("add rax, 1");                                          // advance within the unquoted token
    emitter.instruction("jmp .Linit_argv_fill_unquoted_loop");                  // continue scanning the unquoted token
    emitter.label(".Linit_argv_fill_term_unquoted");
    emitter.instruction("mov BYTE PTR [rax], 0");                               // null-terminate the token at the whitespace position
    emitter.instruction("mov QWORD PTR [r8 + r9 * 8], r10");                    // argv[i] = token start pointer
    emitter.instruction("add r9, 1");                                           // i += 1
    emitter.instruction("add rax, 1");                                          // advance past the now-NUL whitespace
    emitter.instruction("jmp .Linit_argv_fill_skip_ws");                        // resume after the just-terminated token
    emitter.label(".Linit_argv_fill_store");
    emitter.instruction("mov QWORD PTR [r8 + r9 * 8], r10");                    // argv[i] = token start pointer (command line ended at token end)
    emitter.instruction("add r9, 1");                                           // i += 1
    emitter.instruction("jmp .Linit_argv_fill_done");                           // command line exhausted -> stop filling
    emitter.label(".Linit_argv_fill_ws_adv");
    emitter.instruction("add rax, 1");                                          // advance past one whitespace byte
    emitter.instruction("jmp .Linit_argv_fill_skip_ws");                        // resume whitespace skipping / token dispatch
    emitter.label(".Linit_argv_fill_done");
    // -- publish the clean 64-bit argc and the argv array base to the globals --
    emitter.instruction("mov QWORD PTR [rip + _global_argc], rdi");             // _global_argc = argc (clean 64-bit count)
    emitter.instruction("mov QWORD PTR [rip + _global_argv], r8");              // _global_argv = argv pointer array base
    emitter.instruction("add rsp, 40");                                         // restore shadow space
    emitter.instruction("ret");                                                 // return to __elephc_main prologue
    emitter.blank();
}

/// Emits a shim that converts `mmap` to `VirtualAlloc`.
///
/// SysV: rdi=addr, rsi=len, rdx=prot, r10=flags, r8=fd, r9=offset
pub(super) fn emit_shim_mmap(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_mmap");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // base address (NULL = let OS choose)
    emitter.instruction("mov rdx, rsi");                                        // size
    emitter.instruction("mov r8, 0x3000");                                      // MEM_RESERVE | MEM_COMMIT for a protectable standalone mapping
    emitter.instruction("mov r9, 0x04");                                        // PAGE_READWRITE (default)
    emitter.instruction("call VirtualAlloc");                                   // allocate memory
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return base address in rax
    emitter.blank();
}

/// Emits a shim that converts `munmap` to `VirtualFree`.
pub(super) fn emit_shim_munmap(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_munmap");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // base address
    emitter.instruction("xor rdx, rdx");                                        // size = 0 (MEM_RELEASE requires 0)
    emitter.instruction("mov r8, 0x8000");                                      // MEM_RELEASE
    emitter.instruction("call VirtualFree");                                    // free memory
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that provides a simple heap allocation via `HeapAlloc`.
///
/// SysV: rdi=size → returns pointer
pub(super) fn emit_shim_brk(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_brk");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("call GetProcessHeap");                                 // get default heap handle
    emitter.instruction("mov rcx, rax");                                        // heap handle
    emitter.instruction("xor rdx, rdx");                                        // flags = 0
    emitter.instruction("mov r8, rdi");                                         // size
    emitter.instruction("call HeapAlloc");                                      // allocate from heap
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return pointer
    emitter.blank();
}

/// Emits a shim that returns the current process ID.
pub(super) fn emit_shim_getpid(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getpid");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("call GetCurrentProcessId");                            // get PID
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return PID in rax
    emitter.blank();
}

/// Emits a shim that generates random bytes via `BCryptGenRandom`.
///
/// SysV: rdi=buffer, rsi=count. At most `u32::MAX` bytes are filled per call;
/// callers use the getrandom partial-read contract to request the remaining bytes.
/// BCryptGenRandom's signature is `BCryptGenRandom(hAlgorithm, pbBuffer, cbBuffer,
/// dwFlags)`, so it is called with `hAlgorithm = NULL`, `pbBuffer = buffer`,
/// `cbBuffer = count`, and `dwFlags = BCRYPT_USE_SYSTEM_PREFERRED_RNG (2)`. It
/// returns STATUS_SUCCESS (0) on success, a nonzero NTSTATUS on failure. This shim
/// mirrors Linux getrandom's contract: it returns the byte count on success and -1 on
/// error, so callers can treat a negative return as a hard failure.
pub(super) fn emit_shim_getrandom(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getrandom");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + padding
    emitter.instruction("mov rax, 4294967295");                                 // BCryptGenRandom cbBuffer is an unsigned 32-bit length
    emitter.instruction("cmp rsi, rax");                                        // does the request exceed one BCryptGenRandom call?
    emitter.instruction("cmova rsi, rax");                                      // cap this partial fill; the caller loops for the remainder
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // save the actual byte count to return on success
    emitter.instruction("xor rcx, rcx");                                        // hAlgorithm = NULL (use the system-preferred RNG)
    emitter.instruction("mov rdx, rdi");                                        // pbBuffer = caller buffer
    emitter.instruction("mov r8, rsi");                                         // cbBuffer = caller-requested byte count
    emitter.instruction("mov r9, 2");                                           // dwFlags = BCRYPT_USE_SYSTEM_PREFERRED_RNG
    emitter.instruction("call BCryptGenRandom");                                // fill the whole buffer with CSPRNG bytes
    emitter.instruction("test rax, rax");                                       // BCryptGenRandom returns STATUS_SUCCESS (0) on success
    emitter.instruction("jnz .Lgetrandom_fail");                                // any nonzero NTSTATUS → return -1
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // return the byte count on success
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lgetrandom_fail");
    emitter.instruction("mov rax, -1");                                         // return -1 on failure
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Symbols in the libm math/FP family — `pow` and the trig/exp/log cluster —
/// each routed to a dedicated `__rt_sys_<name>` shim on windows-x86_64 (W3c).
/// Unlike the other Class-1 shims above, every one of these functions takes
/// its arguments and returns its result in **floating-point** registers, and
/// that register assignment is IDENTICAL between SysV and MSx64: 1st/2nd FP
/// arg in xmm0/xmm1, result in xmm0, in BOTH ABIs. So none of these shims
/// perform a register shuffle — the sole ABI difference a bare `call <fn>`
/// violates is the MSx64 caller contract of 32-byte shadow space + 16-byte
/// stack alignment at the call site, which every shim below supplies via
/// [`emit_fp_shadow_shim`].
const MATH_FP_SHIM_SYMBOLS: &[&str] = &[
    "pow", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "log",
    "log2", "log10", "atan2", "hypot", "fmod", "round",
];

/// Emits the uniform FP-shadow-space shim body for `symbol`, under the label
/// `__rt_sys_<symbol>`:
/// ```text
/// __rt_sys_<symbol>:
///     sub rsp, 40      ; 32B shadow space + 8B realignment
///     call <symbol>    ; xmm0/xmm1 args, xmm0 result — untouched, identical in both ABIs
///     add rsp, 40
///     ret
/// ```
/// `sub rsp, 40` reserves the mandatory 32-byte MSx64 shadow space plus 8
/// bytes to restore 16-byte alignment at the nested `call`: this shim is
/// entered with `rsp ≡ 8 (mod 16)` (the post-`call` value on entry to any
/// x86_64 function, per the SysV/Win64-shared `call`-pushes-a-return-address
/// convention every caller in this runtime already honors), so after
/// `sub rsp, 40` (also ≡ 0 mod 8, and 40 ≡ 8 mod 16) `rsp` lands exactly on a
/// 16-byte boundary for `call <symbol>`. xmm0/xmm1 (arguments) and xmm0
/// (return value) are never touched — libm functions read/write those exact
/// registers under both ABIs, so no shuffle is needed, only the shadow space.
fn emit_fp_shadow_shim(emitter: &mut Emitter, symbol: &str) {
    emitter.label_global(&format!("__rt_sys_{}", symbol));
    emitter.instruction("sub rsp, 40");                                         // 32B shadow space + 8B realignment
    emitter.instruction(&format!("call {}", symbol));                           // FP args/result in xmm0/xmm1 — untouched, identical in both ABIs
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits all math/libm FP shims (see [`MATH_FP_SHIM_SYMBOLS`] /
/// [`emit_fp_shadow_shim`]).
pub(super) fn emit_shim_math_fp(emitter: &mut Emitter) {
    for symbol in MATH_FP_SHIM_SYMBOLS {
        emit_fp_shadow_shim(emitter, symbol);
    }
}

/// Emits the `__rt_sys_strtod` shim: converts SysV `strtod(s, endptr)` to MSx64 and calls
/// msvcrt `strtod`. SysV: rdi=s, rsi=endptr → MSx64: rcx=s, rdx=endptr. The double return
/// stays in xmm0 (same in both ABIs). The runtime emitters call this shim on Windows-x86_64
/// instead of `call strtod` directly, so the msvcrt import sees MSx64-shaped arguments.
pub(super) fn emit_shim_strtod(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_strtod");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rdx, rsi");                                        // endptr → arg2 (rdx) FIRST, before rdi is moved
    emitter.instruction("mov rcx, rdi");                                        // s → arg1 (rcx)
    emitter.instruction("call strtod");                                         // msvcrt strtod (returns double in xmm0)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the `__rt_sys_strtol` shim: converts SysV `strtol(s, endptr, base)` to MSx64 and
/// calls msvcrt `strtol`. SysV: rdi=s, rsi=endptr, rdx=base → MSx64: rcx=s, rdx=endptr,
/// r8=base. Windows `long` is 32-bit, so the msvcrt return in `eax` is
/// sign-extended before SysV-staged callers consume it as a 64-bit integer.
/// rdx is saved to r8 BEFORE rdx is overwritten, per the socket-shim idiom at
/// `emit_shim_socket_shims`.
pub(super) fn emit_shim_strtol(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_strtol");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // base → arg3 (r8) FIRST, save rdx before overwrite
    emitter.instruction("mov rdx, rsi");                                        // endptr → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // s → arg1 (rcx)
    emitter.instruction("call strtol");                                         // msvcrt strtol (returns long in rax)
    emitter.instruction("cdqe");                                                // sign-extend Windows' 32-bit long into the runtime's 64-bit result
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the `__rt_sys_snprintf` shim: converts the SysV variadic
/// `snprintf(buf, size, fmt, precision, double)` call to the MSx64 variadic ABI and calls
/// MinGW's standards-conforming `__mingw_snprintf`. SysV variadic:
/// rdi=buf, rsi=size, rdx=fmt, rcx=precision, xmm0=double,
/// `al`=1 (vector-register count). MSx64 variadic: rcx=buf, rdx=size,
/// r8=fmt, r9=precision, 5th arg (double) at `[rsp+32]` (the slot above the
/// 32-byte shadow), `al` ignored.
///
/// Register-shuffle hazard: SysV arg4 (precision) is in `rcx`, which is ALSO MSx64 arg1, so
/// precision is saved to `r10` BEFORE `rcx` is overwritten. SysV arg3 (fmt) is in `rdx`, which
/// is ALSO MSx64 arg2, so it is moved to `r8` FIRST (per the socket-shim idiom). The double
/// is spilled to the 5th-arg stack slot via `movsd`; it is NOT passed in a GPR. `al` is left
/// MinGW's formatter is used instead of msvcrt's legacy formatter because PHP
/// preserves the complete binary-double tail for high requested precisions.
/// The int return stays in rax.
pub(super) fn emit_shim_snprintf(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_snprintf");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + 5th-arg slot(8) + pad(16), 16-byte aligned
    emitter.instruction("mov r10, rcx");                                        // SAVE precision (SysV arg4) before rcx is overwritten
    emitter.instruction("mov r8, rdx");                                         // fmt → arg3 (r8) FIRST, save rdx before overwrite
    emitter.instruction("mov rdx, rsi");                                        // size → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // buf → arg1 (rcx)
    emitter.instruction("mov r9, r10");                                         // precision → arg4 (r9)
    emitter.instruction("movsd QWORD PTR [rsp + 32], xmm0");                    // double → 5th arg (stack slot above shadow)
    emitter.instruction("call __mingw_snprintf");                               // render through MinGW's standards-conforming formatter and return the byte count in rax
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the `__rt_sys_snprintf_double` shim: converts a SysV variadic
/// `snprintf(buf, size, fmt, double)` call — exactly ONE variadic argument, a
/// `double`, with NO leading integer precision argument — to the MSx64 variadic
/// ABI and calls MinGW's standards-conforming `__mingw_snprintf`. Used by
/// `__rt_sprintf`'s `%f`/`%e`/`%g`/`%E`/
/// `%G` path (`runtime::strings::sprintf_x86_64`), where the format string
/// already embeds the user's precision/width/flags, unlike `__rt_number_format`,
/// `__rt_ftoa`, and `__rt_json_ftoa`'s
/// `snprintf(buf, size, "%.*e", precision, double)` shape (handled by
/// [`emit_shim_snprintf`]) whose double is logically the 5th argument.
///
/// SysV variadic: rdi=buf, rsi=size, rdx=fmt, xmm0=double, `al`=1.
/// MSx64 variadic: rcx=buf, rdx=size, r8=fmt, and the double is positional
/// argument 4 — under the MSx64 variadic-call convention a floating-point
/// argument in one of the first four positions MUST be duplicated into BOTH its
/// positional integer register (`r9`) and its positional xmm register (`xmm3`):
/// the callee's prologue home-spills `rcx`/`rdx`/`r8`/`r9` into the va_list
/// backing store regardless of type, so a `va_arg(double)` read of position 4
/// pulls from `r9`'s home slot, not `xmm3`, unless both are populated. (This is
/// the "windows garbage output" root cause: [`emit_shim_snprintf`]'s shim
/// treats a caller's `rcx` as a leading precision int and pushes the double to
/// the 5th-argument stack slot at `[rsp+32]`, which msvcrt never reads for a
/// 3-fixed-arg call — it reads uninitialized `r9`/stack garbage instead.)
pub(super) fn emit_shim_snprintf_double(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_snprintf_double");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + pad(8), 16-byte aligned (entry rsp is 8 mod 16)
    emitter.instruction("mov r8, rdx");                                         // fmt → arg3 (r8) FIRST, save rdx before overwrite
    emitter.instruction("mov rdx, rsi");                                        // size → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // buf → arg1 (rcx)
    emitter.instruction("movq r9, xmm0");                                       // double raw bits → arg4 integer register (r9), required for MSx64 variadic va_arg
    emitter.instruction("movaps xmm3, xmm0");                                   // double → arg4 float register (xmm3), duplicate of r9's bits
    emitter.instruction("call __mingw_snprintf");                               // render through MinGW's standards-conforming formatter and return the byte count in rax
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that converts `ioctl` to `ioctlsocket`.
///
/// `ioctlsocket` returns a 32-bit `int` status in `eax` (0 on success, `SOCKET_ERROR` = -1
/// on failure) — reached from PHP's `stream_set_blocking()` via `__rt_sys_fcntl`'s
/// `F_SETFL`/`FIONBIO` delegation (fcntl syscall 72 → `__rt_sys_fcntl` → tail-calls here).
/// `stream_set_blocking.rs:94/114` sign-tests the result (`test rax,rax; js`), so without
/// sign-extension a failure leaves `rax = 0x00000000_FFFFFFFF` (positive) and the failure
/// is missed. `cdqe` restores the 64-bit negative.
pub(super) fn emit_shim_ioctl(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_ioctl");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov r8, rdx");                                         // save argp before rdx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // socket
    emitter.instruction("mov rdx, rsi");                                        // cmd
    emitter.instruction("call ioctlsocket");                                    // ioctl socket
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lioctl_capture_errno");                            // publish Winsock failure
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lioctl_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
}

/// Emits dup/dup2 shims using msvcrt `_dup`/`_dup2`.
///
/// Both `_dup`/`_dup2` return a 32-bit `int` in `eax` (the new fd, or -1 on failure) — the
/// same int-status shape as the Winsock shims, so `cdqe` sign-extends a -1 failure into a
/// 64-bit negative for any sign-testing caller. Note: `opendir_glob.rs`'s `dup(2)` call
/// (the historical justification for this class of fix) is a direct native `call dup`, not
/// routed through this shim or the syscall-number transform, so it is unaffected either way.
pub(super) fn emit_shim_dup_shims(emitter: &mut Emitter) {
    // _dup(fd) → returns new fd or -1
    emitter.label_global("__rt_sys_dup");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("call _dup");                                           // msvcrt _dup
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (-1 failure negative)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return new fd
    emitter.blank();
    // _dup2(fd, fd2) → returns fd2 or -1
    emitter.label_global("__rt_sys_dup2");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // fd
    emitter.instruction("mov rdx, rsi");                                        // fd2
    emitter.instruction("call _dup2");                                          // msvcrt _dup2
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (-1 failure negative)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return new fd
    emitter.blank();
}

/// Emits getuid/getgid (return 0, PHP behavior on Windows) and setuid/setgid/getppid/
/// getpriority/setpriority (return -1, ENOSYS — POSIX-only functions).
pub(super) fn emit_shim_getuid_shims(emitter: &mut Emitter) {
    // getuid/getgid: return 0 on Windows (PHP behavior — no Unix UID/GID)
    for (label, _desc) in &[
        ("__rt_sys_getuid", "getuid"),
        ("__rt_sys_getgid", "getgid"),
    ] {
        emitter.label_global(label);
        emitter.instruction("xor rax, rax");                                    // return 0 (PHP behavior on Windows)
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }
    // setuid/setgid/getppid/getpriority/setpriority: return -1 (ENOSYS)
    for (label, _desc) in &[
        ("__rt_sys_setuid", "setuid"),
        ("__rt_sys_setgid", "setgid"),
        ("__rt_sys_getppid", "getppid"),
        ("__rt_sys_getpriority", "getpriority"),
        ("__rt_sys_setpriority", "setpriority"),
    ] {
        emitter.label_global(label);
        emitter.instruction("mov rax, -1");                                     // return -1 (not supported on Windows)
        emitter.instruction("ret");                                             // return
        emitter.blank();
    }
}

/// Emits a kill shim using OpenProcess+TerminateProcess for SIGKILL, -1 otherwise.
///
/// SysV: rdi=pid, rsi=signal. Windows has no `posix_kill` equivalent for
/// arbitrary signals, so silently reporting success for a signal that was
/// never delivered is worse than a loud failure: both the OpenProcess-failed
/// path and the unsupported-signal path now return -1 (POSIX failure)
/// instead of 0. `posix_kill`/`__rt_sys_kill` has no PHP-reachable caller in
/// this runtime today (grepped: no `posix_kill` lowering, no raw `syscall`
/// site loads `eax` with 62, and no C-symbol `call kill` site exists outside
/// this shim's own delegate) and signal 0 (the POSIX existence-check idiom)
/// has no consumer either, so it is treated the same as any other
/// unsupported signal rather than implemented as an existence check.
pub(super) fn emit_shim_kill(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_kill");
    emitter.instruction("cmp rsi, 9");                                          // sig == SIGKILL?
    emitter.instruction("jne .Lsys_kill_noop");                                 // skip if not SIGKILL
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + handle(8)
    emitter.instruction("mov rcx, 1");                                          // dwDesiredAccess = PROCESS_TERMINATE
    emitter.instruction("xor rdx, rdx");                                        // bInheritHandle = FALSE
    emitter.instruction("mov r8, rdi");                                         // dwProcessId = pid
    emitter.instruction("call OpenProcess");                                    // open process handle
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // save handle
    emitter.instruction("test rax, rax");                                       // check if OpenProcess succeeded
    emitter.instruction("je .Lsys_kill_fail");                                  // jump if failed
    emitter.instruction("mov rcx, rax");                                        // handle
    emitter.instruction("mov rdx, 1");                                          // exit code
    emitter.instruction("call TerminateProcess");                               // terminate process
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // reload handle
    emitter.instruction("call CloseHandle");                                    // close handle
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("xor rax, rax");                                        // return 0 (SIGKILL delivered)
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsys_kill_fail");
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("mov rax, -1");                                         // OpenProcess failed -> return -1
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsys_kill_noop");
    emitter.instruction("mov rax, -1");                                         // return -1 (unsupported signal on Windows; no silent success)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a Win32-backed `uname` shim filling the flat Linux-layout `struct
/// utsname` buffer (`char[5][65]` = 325 bytes, UPWARD: sysname@+0, nodename@+65,
/// release@+130, version@+195, machine@+260 — matches `php_uname.rs`'s
/// `uts_field_len`/`uts_offset`, both 65 bytes/field on Windows).
///
/// SysV: rdi=utsname buffer (MSx64 non-volatile, survives every Win32 call
/// below; copied to rsi up front since rdi is consumed as the `rep stosb`
/// zero-fill destination). Replaces the previous `call uname` stub, which
/// self-recursed: msvcrt exposes no `uname`, so the local `uname` C-symbol
/// delegate (this file) forwarded back into `__rt_sys_uname`, which called
/// `uname` again, forever.
///
/// `sysname`/`nodename`/`machine` are real (literal, `GetComputerNameW`,
/// `GetNativeSystemInfo`). `release`/`version` report a minimal-but-correct
/// fallback ("0.0"/"build 0") rather than the true OS version: the accurate
/// source, `RtlGetVersion`, lives in `ntdll.dll`, which is not part of this
/// target's link set (`src/linker.rs`) and out of scope for a shim-only change.
pub(super) fn emit_shim_uname(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_uname");
    emitter.instruction("sub rsp, 232");                                        // shadow, size, 64-WCHAR hostname, SYSTEM_INFO, saved uts base
    emitter.instruction("mov rsi, rdi");                                        // preserve the utsname buffer base (rsi survives every Win32 call below)
    emitter.instruction("mov QWORD PTR [rsp + 224], rsi");                      // preserve base across UTF conversion argument setup
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 325");                                        // sizeof flat utsname buffer: 5 fields * 65 bytes
    emitter.instruction("rep stosb");                                           // zero the whole utsname buffer before filling any field
    // -- sysname: literal "Windows NT" --
    // x86-64 has no `mov m64, imm64` encoding (only `mov r64, imm64` then
    // store), so the 8-byte "Windows " literal is staged through r10 first —
    // mirrors the heap-kind stamping pattern in tempnam.rs.
    emitter.instruction("mov r10, 0x2073776F646E6957");                         // "Windows "
    emitter.instruction("mov QWORD PTR [rsi], r10");                            // NUL already zeroed by rep stosb
    emitter.instruction("mov WORD PTR [rsi + 8], 0x544E");                      // "NT"
    // -- nodename: GetComputerNameW to a wide temporary, then strict UTF-8 --
    emitter.instruction("mov DWORD PTR [rsp + 32], 64");                        // lpnSize in/out: buffer capacity in TCHARs
    emitter.instruction("lea rcx, [rsp + 40]");                                 // wide hostname temporary
    emitter.instruction("lea rdx, [rsp + 32]");                                 // lpnSize
    emitter.instruction("call GetComputerNameW");                               // retrieve Unicode computer name
    emitter.instruction("test eax, eax");                                       // hostname query succeeded?
    emitter.instruction("jz .Luname_nodename_done");                            // leave zeroed nodename on failure
    emitter.instruction("lea rdi, [rsp + 40]");                                 // UTF-16 hostname source
    emitter.instruction("mov rsi, QWORD PTR [rsp + 224]");                      // utsname buffer base
    emitter.instruction("add rsi, 65");                                         // UTF-8 nodename destination
    emitter.instruction("mov rdx, 65");                                         // nodename field byte capacity
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // strict hostname conversion
    emitter.label(".Luname_nodename_done");
    emitter.instruction("mov rsi, QWORD PTR [rsp + 224]");                      // restore utsname base
    // -- release/version: minimal-but-correct fallback (see docblock) --
    emitter.instruction("mov DWORD PTR [rsi + 130], 0x00302E30");               // release = "0.0"
    emitter.instruction("mov r10, 0x3020646C697562");                           // "build 0" (8-byte literal, staged through r10 — see above)
    emitter.instruction("mov QWORD PTR [rsi + 195], r10");                      // version = "build 0"
    // -- machine: GetNativeSystemInfo(&SYSTEM_INFO) — wProcessorArchitecture is the first WORD --
    emitter.instruction("lea rcx, [rsp + 168]");                                // lpSystemInfo = &SYSTEM_INFO
    emitter.instruction("call GetNativeSystemInfo");                            // fill wProcessorArchitecture at offset 0
    emitter.instruction("movzx eax, WORD PTR [rsp + 168]");                     // wProcessorArchitecture
    emitter.instruction("cmp eax, 9");                                          // PROCESSOR_ARCHITECTURE_AMD64?
    emitter.instruction("je .Luname_machine_amd64");                            // → "AMD64"
    emitter.instruction("cmp eax, 12");                                         // PROCESSOR_ARCHITECTURE_ARM64?
    emitter.instruction("je .Luname_machine_arm64");                            // → "ARM64"
    emitter.instruction("mov DWORD PTR [rsi + 260], 0x00363878");               // fallback machine = "x86" (also PROCESSOR_ARCHITECTURE_INTEL)
    emitter.instruction("jmp .Luname_done");                                    // → done
    emitter.label(".Luname_machine_amd64");
    emitter.instruction("mov DWORD PTR [rsi + 260], 0x36444D41");               // "AMD6"
    emitter.instruction("mov WORD PTR [rsi + 264], 0x0034");                    // "4\0" -> "AMD64"
    emitter.instruction("jmp .Luname_done");                                    // → done
    emitter.label(".Luname_machine_arm64");
    emitter.instruction("mov DWORD PTR [rsi + 260], 0x364D5241");               // "ARM6"
    emitter.instruction("mov WORD PTR [rsi + 264], 0x0034");                    // "4\0" -> "ARM64"
    emitter.label(".Luname_done");
    emitter.instruction("xor eax, eax");                                        // return 0 (success)
    emitter.instruction("add rsp, 232");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a sysinfo shim using GlobalMemoryStatusEx for memory info.
///
/// `GlobalMemoryStatusEx(MEMORYSTATUSEX* lpBuffer)` requires the caller to set
/// `lpBuffer->dwLength` (the first DWORD, offset 0) to `sizeof(MEMORYSTATUSEX)`
/// (64) BEFORE the call; otherwise it fails with `ERROR_INVALID_PARAMETER` on
/// every invocation. The shim initializes `dwLength` and honors the returned
/// BOOL (nonzero = success) instead of unconditionally reporting success,
/// translating it to the POSIX `sysinfo(2)` convention this shim's callers
/// expect: 0 on success, -1 on failure.
pub(super) fn emit_shim_sysinfo(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_sysinfo");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // sysinfo struct pointer
    emitter.instruction("mov DWORD PTR [rcx], 64");                             // initialise MEMORYSTATUSEX.dwLength before GlobalMemoryStatusEx
    emitter.instruction("call GlobalMemoryStatusEx");                           // get memory status; BOOL result in eax
    emitter.instruction("add rsp, 40");                                         // restore stack (before the branch, so both paths are balanced)
    emitter.instruction("test eax, eax");                                       // GlobalMemoryStatusEx returns BOOL (nonzero = success)
    emitter.instruction("jnz .Lsysinfo_ok");                                    // success -> return 0
    emitter.instruction("mov rax, -1");                                         // failure -> return -1
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsysinfo_ok");
    emitter.instruction("xor eax, eax");                                        // return 0 (success)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits an execve shim using msvcrt _execvp.
///
/// SysV: rdi=path, rsi=argv, rdx=envp (ignored on Windows).
pub(super) fn emit_shim_execve(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_execve");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // path
    emitter.instruction("mov rdx, rsi");                                        // argv
    emitter.instruction("call _execvp");                                        // execute program (replaces process)
    emitter.instruction("add rsp, 40");                                         // restore stack (only reached on failure)
    emitter.instruction("ret");                                                 // return -1 on failure (rax from _execvp)
    emitter.blank();
}

/// Emits a loud-failure futex shim for the unsupported Linux-only syscall.
///
/// Returning success here would let raw-syscall callers assume that they slept
/// or woke a waiter when no synchronization happened. Windows runtime paths
/// must use their native synchronization primitives instead.
pub(super) fn emit_shim_futex(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_futex");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 38");                // ENOSYS: no synchronization was performed
    emitter.instruction("mov rax, -1");                                         // report the unsupported operation instead of false success
    emitter.instruction("ret");                                                 // return the POSIX failure sentinel
    emitter.blank();
}

/// Emits an mprotect shim using VirtualProtect.
pub(super) fn emit_shim_mprotect(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_mprotect");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + old_protect(8)
    emitter.instruction("mov r8, 0x04");                                        // default nonzero POSIX protections to PAGE_READWRITE
    emitter.instruction("test edx, edx");                                       // distinguish PROT_NONE from readable/writable mappings
    emitter.instruction("jnz .Lrt_sys_mprotect_protection_ready");              // keep PAGE_READWRITE for every supported nonzero protection mask
    emitter.instruction("mov r8, 0x01");                                        // translate PROT_NONE to PAGE_NOACCESS for guard pages
    emitter.label(".Lrt_sys_mprotect_protection_ready");
    emitter.instruction("mov rcx, rdi");                                        // address
    emitter.instruction("mov rdx, rsi");                                        // size
    emitter.instruction("lea r9, [rsp + 32]");                                  // &old_protect
    emitter.instruction("call VirtualProtect");                                 // change protection
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}
