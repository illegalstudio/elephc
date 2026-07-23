//! Purpose:
//! Emits the `__rt_php_input` runtime helper: backs `file_get_contents('php://input')`
//! in `--web` builds by copying the captured HTTP request body into an owned PHP
//! string. Mirrors the `__rt_stdout_write` web-gating discipline.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The EIR lowering of a literal `file_get_contents("php://input")` calls this label.
//!
//! Key details:
//! - The result follows the `box_owned_string_or_false_result` convention: a string
//!   pointer/length in `x1`/`x2` (AArch64) or `rax`/`rdx` (x86_64); a null pointer
//!   boxes to PHP `false`.
//! - In a `--web` build it reads the body via the bridge getters `elephc_web_body_ptr`
//!   / `elephc_web_body_len` and copies it with `__rt_ptr_read_string` (always emitted
//!   under `--web` because the web prelude's $_POST path uses `ptr_read_string`). In a
//!   non-web build it returns a null pointer (→ `false`) and never references the
//!   web-only bridge symbols, so non-web binaries link without them.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_php_input` runtime helper for the active target.
///
/// When `web` is true, returns the captured request body as an owned PHP string
/// (empty body → null pointer → `false`, a tolerable edge case). When `web` is
/// false, returns a null pointer so `file_get_contents('php://input')` boxes to
/// `false` in a non-web binary without referencing the bridge.
pub fn emit_php_input(emitter: &mut Emitter, web: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_php_input_x86_64(emitter, web);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: php_input (file_get_contents('php://input')) ---");
    emitter.label_global("__rt_php_input");

    emitter.instruction("sub sp, sp, #16");                                     // frame: save the return address and spill the body length across calls
    emitter.instruction("str x30, [sp]");                                       // preserve the caller return address before the nested helper calls

    if web {
        emitter.emit_native_bridge_symbol_call("elephc_web_body_len", 0);      // x0 = request body length (i64) from the bridge
        emitter.instruction("str x0, [sp, #8]");                                // spill the length across the body-pointer call
        emitter.emit_native_bridge_symbol_call("elephc_web_body_ptr", 0);      // x0 = pointer to the request body bytes from the bridge
        emitter.instruction("ldr x1, [sp, #8]");                                // reload the body length into the ptr_read_string length argument
        emitter.instruction("bl __rt_ptr_read_string");                         // copy the body into an owned PHP string (x1=ptr, x2=len out)
    } else {
        emitter.instruction("mov x1, #0");                                      // non-web: null string pointer so the caller boxes PHP false
    }

    emitter.instruction("ldr x30, [sp]");                                       // restore the caller return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 variant of `__rt_php_input`.
fn emit_php_input_x86_64(emitter: &mut Emitter, web: bool) {
    emitter.blank();
    emitter.comment("--- runtime: php_input (file_get_contents('php://input')) ---");
    emitter.label_global("__rt_php_input");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before any nested helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the helper
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned spill slot for the body length

    if web {
        emitter.emit_native_bridge_symbol_call("elephc_web_body_len", 0);      // rax = request body length; Windows still reserves native shadow space
        emitter.instruction("mov QWORD PTR [rbp - 8], rax");                    // spill the length across the body-pointer call
        emitter.emit_native_bridge_symbol_call("elephc_web_body_ptr", 0);      // rax = body pointer; Windows still reserves native shadow space
        emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                    // reload the body length into the ptr_read_string length argument
        emitter.instruction("call __rt_ptr_read_string");                       // copy the body into an owned PHP string (rax=ptr, rdx=len out)
    } else {
        emitter.instruction("xor eax, eax");                                    // non-web: null string pointer so the caller boxes PHP false
    }

    emitter.instruction("add rsp, 16");                                         // release the aligned spill slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies both zero-argument Windows body getters reserve and release
    /// MSx64 shadow space even though they do not need register remapping.
    #[test]
    fn windows_web_body_getters_use_native_shadow_space() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_php_input(&mut emitter, true);
        let asm = emitter.output();
        assert!(asm.contains("lea r11, [rip + elephc_web_body_len]"));
        assert!(asm.contains("lea r11, [rip + elephc_web_body_ptr]"));
        assert_eq!(asm.matches("sub rsp, 32").count(), 2, "{asm}");
        assert_eq!(asm.matches("call r11").count(), 2, "{asm}");
        assert_eq!(asm.matches("add rsp, 32").count(), 2, "{asm}");
        assert!(!asm.contains("call elephc_web_body_"));
    }

    /// Verifies Linux x86 retains direct body-getter calls with no Windows shadow area.
    #[test]
    fn linux_x86_web_body_getters_remain_direct() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_php_input(&mut emitter, true);
        let asm = emitter.output();
        assert!(asm.contains("call elephc_web_body_len"));
        assert!(asm.contains("call elephc_web_body_ptr"));
        assert!(!asm.contains("sub rsp, 32"));
    }

    /// Verifies non-web Windows emission never references optional web bridge symbols.
    #[test]
    fn windows_non_web_input_does_not_reference_bridge() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_php_input(&mut emitter, false);
        let asm = emitter.output();
        assert!(!asm.contains("elephc_web_"));
    }
}
