//! Purpose:
//! Emits the Windows SAPI code-page, console, and control-event runtime shims.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform`.
//!
//! Key details:
//! - Helpers use the internal SysV-shaped call boundary and adapt Win32 calls to MS x64 shadow
//!   space. They are meaningful only for the Windows x86_64 target.
//! - Console callbacks retain callable descriptors, queue CTRL events on the
//!   Win32 handler thread, and invoke PHP only at generated-code safe points.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

pub(crate) const SAPI_CP_INPUT_CODEPAGE_ERROR: &str =
    "sapi_windows_cp_conv(): Argument #1 ($in_codepage) must be a valid codepage";
pub(crate) const SAPI_CP_OUTPUT_CODEPAGE_ERROR: &str =
    "sapi_windows_cp_conv(): Argument #2 ($out_codepage) must be a valid codepage";
pub(crate) const SAPI_CP_SET_WARNING_PREFIX: &str =
    "Warning: sapi_windows_cp_set(): Failed to switch to codepage ";

/// Emits all Windows SAPI helpers. Non-Windows targets receive no labels because their builtin
/// target support contract rejects these functions before code generation.
pub(crate) fn emit_sapi_windows(emitter: &mut Emitter) {
    if emitter.platform != Platform::Windows || emitter.target.arch != Arch::X86_64 {
        emitter.label_global("__rt_sapi_windows_ctrl_dispatch");
        emitter.instruction("ret");                                             // keep the shared PCNTL safe-point wrapper linkable off Windows
        return;
    }
    emit_cp_valid(emitter);
    emit_cp_set(emitter);
    emit_cp_get(emitter);
    emit_cp_is_utf8(emitter);
    emit_vt100(emitter);
    emit_set_ctrl_handler(emitter);
    emit_generate_ctrl_event(emitter);
    emit_cp_conv(emitter);
    emit_ctrl_handler_trampoline(emitter);
    emit_ctrl_descriptor_invoker(emitter);
    emit_ctrl_dispatch(emitter);
}

/// Emits the shared php-src codepage-table membership predicate for numeric selectors.
fn emit_cp_valid(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_cp_valid");
    emitter.instruction("mov r10d, edi");                                       // preserve the candidate codepage identifier
    emitter.instruction("xor r11d, r11d");                                      // begin at the first shared table entry
    emitter.instruction("lea r9, [rip + __rt_sapi_windows_codepage_ids]");      // load the shared table base
    emitter.label(".Lsapi_cp_valid_loop");
    emitter.instruction("cmp r11, QWORD PTR [rip + __rt_sapi_windows_codepage_count]"); // detect table end
    emitter.instruction("jae .Lsapi_cp_valid_false");                           // reject identifiers absent from php-src's table
    emitter.instruction("cmp r10d, DWORD PTR [r9 + r11*4]");                    // compare one table entry
    emitter.instruction("je .Lsapi_cp_valid_true");                             // accept a PHP-supported codepage
    emitter.instruction("inc r11");                                             // advance to the next table entry
    emitter.instruction("jmp .Lsapi_cp_valid_loop");                            // continue scanning
    emitter.label(".Lsapi_cp_valid_true");
    emitter.instruction("mov eax, 1");                                          // return true for a table member
    emitter.instruction("ret");                                                 // finish the predicate
    emitter.label(".Lsapi_cp_valid_false");
    emitter.instruction("xor eax, eax");                                        // return false for an unsupported codepage
    emitter.instruction("ret");                                                 // finish the predicate
}

/// Emits fixed writable storage for the retained callback descriptor and one queued CTRL event.
pub(crate) fn emit_sapi_windows_data() -> String {
    let mut output = String::from(
        ".globl __rt_sapi_windows_ctrl_descriptor\n__rt_sapi_windows_ctrl_descriptor:\n    .quad 0\n.globl __rt_sapi_windows_ctrl_event\n__rt_sapi_windows_ctrl_event:\n    .quad 0\n.globl __rt_sapi_windows_ctrl_pending\n__rt_sapi_windows_ctrl_pending:\n    .quad 0\n.globl __rt_sapi_windows_ctrl_installed\n__rt_sapi_windows_ctrl_installed:\n    .quad 0\n",
    );
    output.push_str(".globl __rt_sapi_windows_current_codepage\n__rt_sapi_windows_current_codepage:\n    .long 0\n    .long 0\n");
    output.push_str(".globl __rt_sapi_windows_codepage_ids\n__rt_sapi_windows_codepage_ids:\n");
    for entry in elephc_builtin_contract::windows_codepages::WINDOWS_CODEPAGES {
        output.push_str(&format!("    .long {}\n", entry.id));
    }
    output.push_str(&format!(
        ".globl __rt_sapi_windows_codepage_count\n__rt_sapi_windows_codepage_count:\n    .quad {}\n",
        elephc_builtin_contract::windows_codepages::WINDOWS_CODEPAGES.len()
    ));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Verifies the Windows SAPI runtime labels and their Win32 API dependencies.
    #[test]
    fn windows_sapi_runtime_surface_is_emitted() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_sapi_windows(&mut emitter);
        let asm = emitter.output();
        for label in [
            "__rt_sapi_windows_vt100_support",
            "__rt_sapi_windows_cp_set",
            "__rt_sapi_windows_cp_get",
            "__rt_sapi_windows_cp_conv",
            "__rt_sapi_windows_cp_is_utf8",
            "__rt_sapi_windows_set_ctrl_handler",
            "__rt_sapi_windows_generate_ctrl_event",
            "__rt_sapi_windows_ctrl_handler",
            "__rt_sapi_windows_invoke_descriptor",
            "__rt_sapi_windows_ctrl_dispatch",
        ] {
            assert!(asm.contains(label), "missing {label}");
        }
        assert!(asm.contains("call GetConsoleMode"));
        assert!(asm.contains("call SetConsoleMode"));
        assert!(asm.contains("call MultiByteToWideChar"));
        assert!(asm.contains("call WideCharToMultiByte"));
        assert!(asm.matches("call __rt_sapi_windows_cp_valid").count() >= 3);
        assert!(asm.contains("cmp edi, 65001"));
        assert!(asm.contains("cmp edi, 54936"));
        assert!(asm.contains("mov edi, DWORD PTR [rsp + 80]"));
        assert!(asm.contains("__rt_sapi_windows_current_codepage"));
        assert!(asm.contains("call __rt_sapi_windows_cp_get"));
        assert!(asm.contains("mov edx, 128"));
        assert!(asm.contains("_sapi_cp_input_codepage_error"));
        assert!(asm.contains("_sapi_cp_output_codepage_error"));
        assert!(asm.contains("_diag_sapi_cp_set_failed_prefix"));
        assert!(asm.contains("call __rt_diag_warning"));
        assert!(asm.contains("call GenerateConsoleCtrlEvent"));
        assert!(asm.contains("__rt_sapi_windows_ctrl_pending"));
        assert!(asm.contains("__rt_callable_descriptor_release"));
        assert!(asm.contains("mov QWORD PTR [rsp + 32], rax"));
        assert!(asm.contains("mov edx, DWORD PTR [rsp + 44]"));
        assert_eq!(asm.matches("call __rt_heap_alloc").count(), 2);
        assert!(asm.matches("call __rt_heap_free").count() >= 3);
        assert!(!asm.contains("call HeapAlloc"));
        assert!(!asm.contains("call GetProcessHeap"));
        assert!(asm.contains("mov QWORD PTR [rsp + 56], rax"));
    }
}

fn emit_cp_set(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_cp_set");
    emitter.instruction("sub rsp, 40");                                         // reserve warning scratch and Win32 shadow space
    emitter.instruction("mov DWORD PTR [rsp + 32], edi");                       // preserve the requested code page for validation and warning text
    emitter.instruction("mov edi, DWORD PTR [rsp + 32]");                       // pass the code page to the shared PHP table predicate
    emitter.instruction("call __rt_sapi_windows_cp_valid");                     // enforce php-src's numeric codepage table before Win32 mutation
    emitter.instruction("test eax, eax");                                       // test the shared table membership result
    emitter.instruction("jz .Lsapi_cp_set_false");                              // invalid codepages return false with the caller warning path
    emitter.instruction("mov ecx, DWORD PTR [rsp + 32]");                       // reload the code page for the input-console setter
    emitter.instruction("call SetConsoleCP");                                   // set both process console code pages and return Win32 success
    emitter.instruction("test eax, eax");                                       // set both process console code pages and return Win32 success
    emitter.instruction("jz .Lsapi_cp_set_false");                              // set both process console code pages and return Win32 success
    emitter.instruction("mov ecx, DWORD PTR [rsp + 32]");                       // reload the code page for the output-console setter
    emitter.instruction("call SetConsoleOutputCP");                             // set both process console code pages and return Win32 success
    emitter.instruction("test eax, eax");                                       // normalize the output-console setter result
    emitter.instruction("jz .Lsapi_cp_set_false");                              // do not publish a partially applied code page
    emitter.instruction("mov eax, DWORD PTR [rsp + 32]");                       // publish the active PHP code page after both setters succeeded
    emitter.instruction("mov DWORD PTR [rip + __rt_sapi_windows_current_codepage], eax"); // retain the process-local PHP code page
    emitter.instruction("mov eax, 1");                                          // return PHP true for a successful code-page switch
    emitter.instruction("add rsp, 40");                                         // set both process console code pages and return Win32 success
    emitter.instruction("ret");                                                 // set both process console code pages and return Win32 success
    emitter.label(".Lsapi_cp_set_false");
    abi::emit_symbol_address(emitter, "rdi", "_diag_sapi_cp_set_failed_prefix"); // address php-src-compatible warning prefix
    emitter.instruction(&format!("mov esi, {}", SAPI_CP_SET_WARNING_PREFIX.len())); // pass the exact warning-prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the warning prefix
    emitter.instruction("mov eax, DWORD PTR [rsp + 32]");                       // reload the rejected code-page identifier
    abi::emit_call_label(emitter, "__rt_itoa");                                // format the identifier in decimal
    emitter.instruction("mov rdi, rax");                                        // pass the formatted identifier bytes
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted identifier length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the identifier
    abi::emit_symbol_address(emitter, "rdi", "_uncaught_exc_nl");             // reuse the shared newline byte
    emitter.instruction("mov esi, 1");                                          // warning terminates with one newline
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // finish the warning through the shared channel
    emitter.instruction("xor eax, eax");                                        // set both process console code pages and return Win32 success
    emitter.instruction("add rsp, 40");                                         // set both process console code pages and return Win32 success
    emitter.instruction("ret");                                                 // set both process console code pages and return Win32 success
}

fn emit_cp_get(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_cp_get");
    emitter.instruction("sub rsp, 40");                                         // select the active, ANSI, or OEM code page
    emitter.instruction("cmp edi, 1");                                          // select the active, ANSI, or OEM code page
    emitter.instruction("je .Lsapi_cp_get_ansi");                               // select the active, ANSI, or OEM code page
    emitter.instruction("cmp edi, 2");                                          // select the active, ANSI, or OEM code page
    emitter.instruction("je .Lsapi_cp_get_oem");                                // select the active, ANSI, or OEM code page
    // php-src returns its tracked current code page for the default selector.  Keep a lazy
    // process-local copy so a headless Windows worker (where GetConsoleOutputCP() is zero) still
    // reports the ACP initially and observes a later sapi_windows_cp_set().
    emitter.instruction("mov eax, DWORD PTR [rip + __rt_sapi_windows_current_codepage]"); // load the process-local PHP code page
    emitter.instruction("test eax, eax");                                       // use the tracked current page when it was initialized or changed
    emitter.instruction("jnz .Lsapi_cp_get_done");                              // return the tracked current page
    emitter.instruction("call GetACP");                                         // initialize the current page like php_win32_cp_setup()
    emitter.instruction("mov DWORD PTR [rip + __rt_sapi_windows_current_codepage], eax"); // cache the initial ANSI code page
    emitter.instruction("jmp .Lsapi_cp_get_done");                              // select the active code page
    emitter.label(".Lsapi_cp_get_ansi");
    emitter.instruction("call GetACP");                                         // select the active, ANSI, or OEM code page
    emitter.instruction("jmp .Lsapi_cp_get_done");                              // select the active, ANSI, or OEM code page
    emitter.label(".Lsapi_cp_get_oem");
    emitter.instruction("call GetOEMCP");                                       // select the active, ANSI, or OEM code page
    emitter.label(".Lsapi_cp_get_done");
    emitter.instruction("add rsp, 40");                                         // select the active, ANSI, or OEM code page
    emitter.instruction("ret");                                                 // select the active, ANSI, or OEM code page
}

fn emit_cp_is_utf8(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_cp_is_utf8");
    emitter.instruction("sub rsp, 40");                                         // compare the active console code page with UTF-8
    emitter.instruction("xor edi, edi");                                        // request the default tracked code page from the shared getter
    emitter.instruction("call __rt_sapi_windows_cp_get");                       // query php-src's tracked current code page
    emitter.instruction("cmp eax, 65001");                                      // compare the active console code page with UTF-8
    emitter.instruction("sete al");                                             // compare the active console code page with UTF-8
    emitter.instruction("movzx eax, al");                                       // compare the active console code page with UTF-8
    emitter.instruction("add rsp, 40");                                         // compare the active console code page with UTF-8
    emitter.instruction("ret");                                                 // compare the active console code page with UTF-8
}

fn emit_vt100(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_vt100_support");
    emitter.instruction("sub rsp, 56");                                         // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("mov ecx, edi");                                        // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("call _get_osfhandle");                                 // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("cmp rax, -1");                                         // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("je .Lsapi_vt_false");                                  // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("mov rcx, rax");                                        // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("lea rdx, [rsp + 40]");                                 // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("call GetConsoleMode");                                 // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("test eax, eax");                                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("jz .Lsapi_vt_false");                                  // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("mov eax, DWORD PTR [rsp + 40]");                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("test esi, esi");                                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("js .Lsapi_vt_query");                                  // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("test esi, esi");                                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("jz .Lsapi_vt_disable");                                // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("or eax, 4");                                           // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("jmp .Lsapi_vt_set");                                   // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.label(".Lsapi_vt_disable");
    emitter.instruction("and eax, -5");                                         // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.label(".Lsapi_vt_set");
    emitter.instruction("mov DWORD PTR [rsp + 44], eax");                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("mov edx, DWORD PTR [rsp + 44]");                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("call SetConsoleMode");                                 // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("test eax, eax");                                       // report the setter's Win32 BOOL result
    emitter.instruction("setne al");                                            // normalize SetConsoleMode success to PHP bool
    emitter.instruction("movzx eax, al");                                       // widen the normalized setter result
    emitter.instruction("add rsp, 56");                                         // release the VT100 setter frame
    emitter.instruction("ret");                                                 // return the setter result
    emitter.label(".Lsapi_vt_query");
    emitter.instruction("mov eax, DWORD PTR [rsp + 40]");                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("and eax, 4");                                          // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("setne al");                                            // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("movzx eax, al");                                       // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("add rsp, 56");                                         // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("ret");                                                 // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.label(".Lsapi_vt_false");
    emitter.instruction("xor eax, eax");                                        // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("add rsp, 56");                                         // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
    emitter.instruction("ret");                                                 // query or update ENABLE_VIRTUAL_TERMINAL_PROCESSING
}

fn emit_set_ctrl_handler(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_set_ctrl_handler");
    emitter.instruction("sub rsp, 56");                                         // reserve MS x64 shadow space and stable descriptor slots
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // retain the requested descriptor across Win32 calls
    emitter.instruction("test rdi, rdi");                                       // select the null-handler add/remove form
    emitter.instruction("jz .Lsapi_ctrl_handler_null");                         // null handler removes the installed PHP callback
    emitter.instruction("test esi, esi");                                       // distinguish registration from removal
    emitter.instruction("jz .Lsapi_ctrl_handler_reset");                        // add=false removes the callback trampoline after reset
    emitter.label(".Lsapi_ctrl_handler_reset");
    emitter.instruction("xor ecx, ecx");                                        // clear any previous PHP control handler first
    emitter.instruction("xor edx, edx");                                        // SetConsoleCtrlHandler(NULL, FALSE) matches php-src
    emitter.instruction("call SetConsoleCtrlHandler");                          // reset the native handler chain
    emitter.instruction("test eax, eax");                                       // require the reset to succeed
    emitter.instruction("jz .Lsapi_ctrl_handler_fail");                         // do not publish ownership after a failed reset
    emitter.instruction("lea rcx, [rip + __rt_sapi_windows_ctrl_handler]");     // pass the native callback entry to Win32
    emitter.instruction("mov edx, esi");                                        // request callback installation or removal
    emitter.instruction("call SetConsoleCtrlHandler");                          // install/remove the callback after reset
    emitter.instruction("test eax, eax");                                       // check native installation status
    emitter.instruction("jz .Lsapi_ctrl_handler_fail");                         // never return true before both operations succeed
    emitter.instruction("cmp esi, 0");                                          // removal does not retain a new descriptor
    emitter.instruction("je .Lsapi_ctrl_handler_clear");                        // clear the old descriptor after removal
    emitter.instruction("mov r10, QWORD PTR [rip + __rt_sapi_windows_ctrl_descriptor]"); // load the previous retained descriptor
    emitter.instruction("mov QWORD PTR [rsp + 40], r10");                       // preserve the old descriptor across retain calls
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // restore the new descriptor for the retain helper
    emitter.instruction("call __rt_incref");                                    // retain the handler for the process-global slot
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // reload the descriptor after the retain call
    emitter.instruction("mov QWORD PTR [rip + __rt_sapi_windows_ctrl_descriptor], rax"); // publish the new descriptor
    emitter.instruction("mov QWORD PTR [rip + __rt_sapi_windows_ctrl_installed], 1"); // record native installation
    emitter.instruction("mov r10, QWORD PTR [rsp + 40]");                       // reload the old descriptor after publishing the new one
    emitter.instruction("test r10, r10");                                       // check whether an older handler needs release
    emitter.instruction("jz .Lsapi_ctrl_handler_done");                         // no previous descriptor to release
    emitter.instruction("mov rax, r10");                                        // pass the old descriptor to the release helper
    emitter.instruction("call __rt_callable_descriptor_release");               // release the replaced descriptor
    emitter.instruction("jmp .Lsapi_ctrl_handler_done");                        // return native success
    emitter.label(".Lsapi_ctrl_handler_null");
    emitter.instruction("xor ecx, ecx");                                        // NULL handler selects the process-wide default handler
    emitter.instruction("mov edx, esi");                                        // preserve PHP's add/remove flag
    emitter.instruction("call SetConsoleCtrlHandler");                          // apply the requested default-handler operation
    emitter.instruction("test eax, eax");                                       // check native operation status
    emitter.instruction("jz .Lsapi_ctrl_handler_fail");                         // retain the old descriptor on failure
    emitter.label(".Lsapi_ctrl_handler_clear");
    emitter.instruction("mov r10, QWORD PTR [rip + __rt_sapi_windows_ctrl_descriptor]"); // load the retained descriptor
    emitter.instruction("mov QWORD PTR [rip + __rt_sapi_windows_ctrl_descriptor], 0"); // clear the process-global callable slot
    emitter.instruction("mov QWORD PTR [rip + __rt_sapi_windows_ctrl_installed], 0"); // clear native installation state
    emitter.instruction("test r10, r10");                                       // check whether a descriptor needs release
    emitter.instruction("jz .Lsapi_ctrl_handler_done");                         // no descriptor remains
    emitter.instruction("mov rax, r10");                                        // pass the old descriptor to the release helper
    emitter.instruction("call __rt_callable_descriptor_release");               // release the removed descriptor
    emitter.label(".Lsapi_ctrl_handler_done");
    emitter.instruction("mov eax, 1");                                          // report successful install/remove
    emitter.instruction("add rsp, 56");                                         // release the Win32 call frame
    emitter.instruction("ret");                                                 // return PHP true only after native success
    emitter.label(".Lsapi_ctrl_handler_fail");
    emitter.instruction("xor eax, eax");                                        // report native installation/removal failure
    emitter.instruction("add rsp, 56");                                         // release the Win32 call frame
    emitter.instruction("ret");                                                 // return PHP false without changing ownership
}

/// Emits the C-ABI Win32 callback. It only records CTRL+C and CTRL+BREAK; PHP never runs on the
/// foreign control-handler thread.
fn emit_ctrl_handler_trampoline(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_ctrl_handler");
    emitter.instruction("cmp ecx, 0");                                          // accept CTRL_C_EVENT
    emitter.instruction("je .Lsapi_ctrl_handler_queue");                        // queue accepted control events
    emitter.instruction("cmp ecx, 1");                                          // test CTRL_BREAK_EVENT
    emitter.instruction("jne .Lsapi_ctrl_handler_ignore");                      // ignore unrelated Windows control events
    emitter.label(".Lsapi_ctrl_handler_queue");
    emitter.instruction("mov QWORD PTR [rip + __rt_sapi_windows_ctrl_event], rcx"); // retain the event code atomically enough for the safe point
    emitter.instruction("mov QWORD PTR [rip + __rt_sapi_windows_ctrl_pending], 1"); // publish one pending event
    emitter.instruction("mov eax, 1");                                          // report that PHP owns this control event
    emitter.instruction("ret");                                                 // return from the Win32 callback
    emitter.label(".Lsapi_ctrl_handler_ignore");
    emitter.instruction("xor eax, eax");                                        // let Windows continue unrelated control handling
    emitter.instruction("ret");                                                 // return FALSE to the Win32 dispatcher
}

/// Emits the one-argument descriptor adapter used by the safe-point dispatcher.
fn emit_ctrl_descriptor_invoker(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_invoke_descriptor");
    emitter.instruction("push rbp");                                            // preserve the callback adapter frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable local offsets
    emitter.instruction("sub rsp, 64");                                         // reserve descriptor, argument, and owner slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the retained descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the CTRL event integer
    emitter.instruction("mov rdi, rsi");                                        // box the event as the PHP callback argument
    emitter.instruction("xor esi, esi");                                        // clear the Mixed high payload word
    emitter.instruction("xor eax, eax");                                        // Mixed tag = Int
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed event
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // retain the boxed event owner
    emitter.instruction("mov edi, 1");                                          // allocate one callback argument
    emitter.instruction("mov esi, 8");                                          // array element type = refcounted Mixed
    emitter.instruction("call __rt_array_new");                                 // allocate the argument array
    emitter.instruction("mov rdi, rax");                                        // pass the array to the append helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the boxed event
    emitter.instruction("call __rt_array_push_refcounted");                     // retain the event in the array
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the possibly relocated array
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the local event owner
    emitter.instruction("call __rt_decref_any");                                // leave the array as its sole owner
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // box the callback argument array
    emitter.instruction("xor esi, esi");                                        // clear the array high payload word
    emitter.instruction("mov eax, 4");                                          // Mixed tag = indexed array
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed argument array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the boxed argument owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the raw argument array
    emitter.instruction("call __rt_decref_any");                                // transfer ownership to its Mixed box
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the callable descriptor
    emitter.instruction("mov r11, QWORD PTR [r10 + 56]");                       // load its uniform invocation entry point
    emitter.instruction("test r11, r11");                                       // check that the descriptor is invokable
    emitter.instruction("jz .Lsapi_ctrl_invoke_cleanup");                       // tolerate missing invoker metadata
    emitter.instruction("mov rdi, r10");                                        // invocation arg0 = descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // invocation arg1 = boxed argument array
    emitter.emit_platform_callback_call("r11", 2);                            // invoke PHP through the established callback ABI
    emitter.instruction("test rax, rax");                                       // check for an owned callback result
    emitter.instruction("jz .Lsapi_ctrl_invoke_cleanup");                       // null/void results need no release
    emitter.instruction("call __rt_decref_any");                                // release the ignored result
    emitter.label(".Lsapi_ctrl_invoke_cleanup");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed argument owner
    emitter.instruction("call __rt_decref_any");                                // release the callback argument array
    emitter.instruction("leave");                                               // release adapter storage
    emitter.instruction("ret");                                                 // return to the safe-point dispatcher
}

/// Emits the normal safe-point drain for one queued console control event.
fn emit_ctrl_dispatch(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_ctrl_dispatch");
    emitter.instruction("cmp QWORD PTR [rip + __rt_sapi_windows_ctrl_pending], 0"); // test whether a control event is queued
    emitter.instruction("je .Lsapi_ctrl_dispatch_done");                        // skip when no event is pending
    emitter.instruction("mov QWORD PTR [rip + __rt_sapi_windows_ctrl_pending], 0"); // claim the queued event
    emitter.instruction("mov rdi, QWORD PTR [rip + __rt_sapi_windows_ctrl_descriptor]"); // load the retained callable descriptor
    emitter.instruction("test rdi, rdi");                                       // tolerate a removed handler
    emitter.instruction("jz .Lsapi_ctrl_dispatch_done");                        // skip dispatch after removal
    emitter.instruction("mov rsi, QWORD PTR [rip + __rt_sapi_windows_ctrl_event]"); // load the queued CTRL event
    emitter.instruction("call __rt_sapi_windows_invoke_descriptor");            // invoke the callback on the PHP safe-point thread
    emitter.label(".Lsapi_ctrl_dispatch_done");
    emitter.instruction("ret");                                                 // resume generated PHP execution
}

fn emit_generate_ctrl_event(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_generate_ctrl_event");
    emitter.instruction("sub rsp, 40");                                         // reserve MS x64 shadow space for the control-event sequence
    emitter.instruction("xor ecx, ecx");                                        // temporarily disable the process default handler
    emitter.instruction("mov edx, 1");                                          // request temporary handler disablement
    emitter.instruction("call SetConsoleCtrlHandler");                          // avoid self-termination while generating the event
    emitter.instruction("test eax, eax");                                       // check temporary disablement
    emitter.instruction("jz .Lsapi_ctrl_event_fail");                           // fail before generating an unsafe event
    emitter.instruction("mov ecx, edi");                                        // pass the requested control event kind
    emitter.instruction("mov edx, esi");                                        // pass the requested process-group id
    emitter.instruction("call GenerateConsoleCtrlEvent");                       // generate the control event
    emitter.instruction("mov DWORD PTR [rsp + 32], eax");                       // retain the native event result across handler restoration
    emitter.instruction("cmp QWORD PTR [rip + __rt_sapi_windows_ctrl_installed], 0"); // check whether PHP callback restoration is needed
    emitter.instruction("je .Lsapi_ctrl_event_result");                         // no callback was installed
    emitter.instruction("lea rcx, [rip + __rt_sapi_windows_ctrl_handler]");     // restore the retained PHP callback trampoline
    emitter.instruction("mov edx, 1");                                          // request callback restoration
    emitter.instruction("call SetConsoleCtrlHandler");                          // restore callback ownership after event generation
    emitter.instruction("test eax, eax");                                       // check callback restoration
    emitter.instruction("jz .Lsapi_ctrl_event_fail");                           // restoration failure makes the operation false
    emitter.label(".Lsapi_ctrl_event_result");
    emitter.instruction("mov eax, DWORD PTR [rsp + 32]");                       // restore the event result
    emitter.instruction("add rsp, 40");                                         // release the Win32 call frame
    emitter.instruction("ret");                                                 // return PHP bool
    emitter.label(".Lsapi_ctrl_event_fail");
    emitter.instruction("xor eax, eax");                                        // report generation or restoration failure
    emitter.instruction("add rsp, 40");                                         // release the Win32 call frame
    emitter.instruction("ret");                                                 // return PHP false
}

fn emit_cp_conv(emitter: &mut Emitter) {
    emitter.label_global("__rt_sapi_windows_cp_conv");
    // SysV entry: edi=input codepage, esi=output codepage, r8=subject pointer, r9=length.
    // The conversion path is deliberately strict: invalid pages or failed Win32 conversions
    // return the PHP null pair rather than exposing a partially initialized buffer.
    emitter.instruction("sub rsp, 120");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov DWORD PTR [rsp + 80], edi");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov DWORD PTR [rsp + 84], esi");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 88], r8");                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 96], r9");                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("call __rt_sapi_windows_cp_valid");                     // validate the requested input code page against php-src's table
    emitter.instruction("test eax, eax");                                       // did Windows recognize the input page?
    emitter.instruction("jz .Lsapi_cp_conv_input_invalid");                     // throw PHP's argument #1 ValueError
    emitter.instruction("mov edi, DWORD PTR [rsp + 84]");                       // validate the requested output code page
    emitter.instruction("call __rt_sapi_windows_cp_valid");                     // ask the shared table before allocating conversion buffers
    emitter.instruction("test eax, eax");                                       // did Windows recognize the output page?
    emitter.instruction("jz .Lsapi_cp_conv_output_invalid");                    // throw PHP's argument #2 ValueError
    // The validation call above deliberately receives the output selector in EDI.  Restore the
    // input selector before choosing the input flags and before the first Win32 conversion; the
    // two selectors are independent PHP arguments (UTF-8 -> CP1252 otherwise used CP1252 as the
    // input encoding).
    emitter.instruction("mov edi, DWORD PTR [rsp + 80]");                       // restore the input selector after validating the output selector
    emitter.instruction("mov DWORD PTR [rsp + 116], 0");                        // default input conversion flags for legacy code pages
    emitter.instruction("cmp edi, 65001");                                      // does the input use strict UTF-8 conversion?
    emitter.instruction("je .Lsapi_cp_conv_input_strict");                      // enable invalid-sequence rejection for UTF-8
    emitter.instruction("cmp edi, 54936");                                      // does the input use strict GB18030 conversion?
    emitter.instruction("jne .Lsapi_cp_conv_input_flags_ready");                // other pages use their zero flag from php-src's table
    emitter.label(".Lsapi_cp_conv_input_strict");
    emitter.instruction("mov DWORD PTR [rsp + 116], 8");                        // MB_ERR_INVALID_CHARS from php-src's code-page catalog
    emitter.label(".Lsapi_cp_conv_input_flags_ready");
    emitter.instruction("mov ecx, edi");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov edx, DWORD PTR [rsp + 116]");                      // apply the selected input conversion flags
    emitter.instruction("mov r8, QWORD PTR [rsp + 88]");                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov r9d, DWORD PTR [rsp + 96]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("xor eax, eax");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("call MultiByteToWideChar");                            // convert subject bytes through Win32 code-page APIs
    emitter.instruction("test eax, eax");                                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("jz .Lsapi_cp_conv_fail");                              // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov DWORD PTR [rsp + 104], eax");                      // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov eax, DWORD PTR [rsp + 104]");                      // reload the required temporary WCHAR count
    emitter.instruction("shl rax, 1");                                          // convert WCHAR capacity to owned heap bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate the temporary through elephc ownership
    emitter.instruction("test rax, rax");                                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("jz .Lsapi_cp_conv_fail");                              // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov ecx, DWORD PTR [rsp + 80]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov edx, DWORD PTR [rsp + 116]");                      // preserve strict input conversion on the real pass
    emitter.instruction("mov r8, QWORD PTR [rsp + 88]");                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov r9d, DWORD PTR [rsp + 96]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov eax, DWORD PTR [rsp + 104]");                      // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov DWORD PTR [rsp + 40], eax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("call MultiByteToWideChar");                            // convert subject bytes through Win32 code-page APIs
    emitter.instruction("test eax, eax");                                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("jz .Lsapi_cp_conv_free_wide");                         // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov DWORD PTR [rsp + 108], eax");                      // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov ecx, DWORD PTR [rsp + 84]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("xor edx, edx");                                        // default output flags for legacy code pages
    emitter.instruction("cmp ecx, 65001");                                      // does the output use strict UTF-8 conversion?
    emitter.instruction("je .Lsapi_cp_conv_output_query_strict");               // select WC_ERR_INVALID_CHARS for UTF-8
    emitter.instruction("cmp ecx, 54936");                                      // does the output use strict GB18030 conversion?
    emitter.instruction("jne .Lsapi_cp_conv_output_query_ready");               // other pages retain zero flags
    emitter.label(".Lsapi_cp_conv_output_query_strict");
    emitter.instruction("mov edx, 128");                                        // WC_ERR_INVALID_CHARS from php-src's code-page catalog
    emitter.label(".Lsapi_cp_conv_output_query_ready");
    emitter.instruction("mov r8, QWORD PTR [rsp + 72]");                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov r9d, DWORD PTR [rsp + 108]");                      // convert subject bytes through Win32 code-page APIs
    emitter.instruction("xor eax, eax");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // query does not supply a default replacement character
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // caller does not request the used-default flag
    emitter.instruction("call WideCharToMultiByte");                            // convert subject bytes through Win32 code-page APIs
    emitter.instruction("test eax, eax");                                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("jz .Lsapi_cp_conv_free_wide");                         // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov DWORD PTR [rsp + 112], eax");                      // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov eax, DWORD PTR [rsp + 112]");                      // reload the required result byte count
    emitter.instruction("call __rt_heap_alloc");                                // return storage must obey elephc string ownership
    emitter.instruction("test rax, rax");                                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("jz .Lsapi_cp_conv_free_wide");                         // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov ecx, DWORD PTR [rsp + 84]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("xor edx, edx");                                        // default output flags for legacy code pages
    emitter.instruction("cmp ecx, 65001");                                      // does the output use strict UTF-8 conversion?
    emitter.instruction("je .Lsapi_cp_conv_output_write_strict");               // select WC_ERR_INVALID_CHARS for UTF-8
    emitter.instruction("cmp ecx, 54936");                                      // does the output use strict GB18030 conversion?
    emitter.instruction("jne .Lsapi_cp_conv_output_write_ready");               // other pages retain zero flags
    emitter.label(".Lsapi_cp_conv_output_write_strict");
    emitter.instruction("mov edx, 128");                                        // WC_ERR_INVALID_CHARS from php-src's code-page catalog
    emitter.label(".Lsapi_cp_conv_output_write_ready");
    emitter.instruction("mov r8, QWORD PTR [rsp + 72]");                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov r9d, DWORD PTR [rsp + 108]");                      // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov eax, DWORD PTR [rsp + 112]");                      // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov DWORD PTR [rsp + 40], eax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("xor eax, eax");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // caller does not request the used-default flag
    emitter.instruction("call WideCharToMultiByte");                            // convert subject bytes through Win32 code-page APIs
    emitter.instruction("test eax, eax");                                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("jz .Lsapi_cp_conv_free_result");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov rdx, rax");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("jmp .Lsapi_cp_conv_free_wide_return");                 // convert subject bytes through Win32 code-page APIs
    emitter.label(".Lsapi_cp_conv_free_result");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("call __rt_heap_free");                                 // release the failed owned result allocation
    emitter.label(".Lsapi_cp_conv_free_wide");
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("test rax, rax");                                       // convert subject bytes through Win32 code-page APIs
    emitter.instruction("jz .Lsapi_cp_conv_fail");                              // convert subject bytes through Win32 code-page APIs
    emitter.instruction("call __rt_heap_free");                                 // release the failed owned WCHAR allocation
    emitter.label(".Lsapi_cp_conv_fail");
    emitter.instruction("xor eax, eax");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("xor edx, edx");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("add rsp, 120");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("ret");                                                 // convert subject bytes through Win32 code-page APIs
    emitter.label(".Lsapi_cp_conv_input_invalid");
    emitter.instruction("add rsp, 120");                                        // release conversion locals before throwing
    crate::codegen_support::runtime::arrays::value_error::emit_throw_value_error_x86_64(
        emitter,
        "_sapi_cp_input_codepage_error",
        SAPI_CP_INPUT_CODEPAGE_ERROR.len(),
    );
    emitter.label(".Lsapi_cp_conv_output_invalid");
    emitter.instruction("add rsp, 120");                                        // release conversion locals before throwing
    crate::codegen_support::runtime::arrays::value_error::emit_throw_value_error_x86_64(
        emitter,
        "_sapi_cp_output_codepage_error",
        SAPI_CP_OUTPUT_CODEPAGE_ERROR.len(),
    );
    emitter.label(".Lsapi_cp_conv_free_wide_return");
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // reload the owned WCHAR temporary
    emitter.instruction("call __rt_heap_free");                                 // release the intermediate conversion buffer
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // restore the owned converted-string pointer
    emitter.instruction("mov edx, DWORD PTR [rsp + 112]");                      // restore the converted-string byte length
    emitter.instruction("add rsp, 120");                                        // convert subject bytes through Win32 code-page APIs
    emitter.instruction("ret");                                                 // convert subject bytes through Win32 code-page APIs
}
