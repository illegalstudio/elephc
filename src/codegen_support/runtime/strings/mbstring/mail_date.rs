//! Purpose:
//! Formats mb_send_mail's Date header through the active PHP timezone runtime.
//!
//! Called from:
//! - The mail-specific V8 mbstring invocation host.
//!
//! Key details:
//! - `__rt_date(-1, "r")` supplies current-time and timezone semantics.
//! - Returned bytes receive an independent owner consumed by the ordinary host release callback.
//! - PHP exceptions are contained before control returns across the Rust boundary.

use super::*;
use crate::codegen_support::platform::{Arch, Platform};
use crate::codegen_support::runtime::exceptions::emit_protected_status;

pub(super) const C_ABI_CALLBACK: &str = "__rt_mbstring_mail_date_cabi";

pub(super) fn emit(emitter: &mut Emitter, eval_bridge: bool, mail: bool) {
    if !mail { return; }
    abi::emit_c_callback_entry(emitter, C_ABI_CALLBACK);
    emitter.instruction(&format!("{} __rt_mbstring_mail_date",
        if emitter.target.arch == Arch::AArch64 { "b" } else { "jmp" }));       // enter the exception-protected SysV-shaped implementation
    emit_protected_status(emitter, "__rt_mbstring_mail_date",
        if eval_bridge { body_with_eval } else { body_aot });
}

fn body_with_eval(emitter: &mut Emitter) { body(emitter, true); }

fn body_aot(emitter: &mut Emitter) { body(emitter, false); }

fn body(emitter: &mut Emitter, eval_bridge: bool) {
    if eval_bridge {
        if emitter.target.arch == Arch::AArch64 {
            emitter.instruction("ldr x0, [sp, #224]");                          // recover the optional eval context
            emitter.instruction("cbz x0, __rt_mbstring_mail_date_aot");         // null context uses the native PHP date runtime
            emitter.instruction("ldr x1, [sp, #232]");                          // pass writable host-string output storage
            emitter.emit_native_bridge_symbol_call("elephc_eval_mbstring_mail_date_v1", 2); // format through ElephcEvalContext.default_timezone
            emitter.instruction("b __rt_mbstring_mail_date_body_return");       // return the provider's contained status
        } else {
            emitter.instruction("mov rdi, QWORD PTR [rsp + 224]");              // recover the optional eval context
            emitter.instruction("test rdi, rdi");                               // distinguish AOT calls from eval runtime dispatch
            emitter.instruction("jz __rt_mbstring_mail_date_aot");              // null context uses the native PHP date runtime
            emitter.instruction("mov rsi, QWORD PTR [rsp + 232]");              // pass writable host-string output storage
            emitter.emit_native_bridge_symbol_call("elephc_eval_mbstring_mail_date_v1", 2); // format through ElephcEvalContext.default_timezone
            emitter.instruction("jmp __rt_mbstring_mail_date_body_return");     // return the provider's contained status
        }
        emitter.label("__rt_mbstring_mail_date_aot");
    }
    if emitter.target.platform == Platform::Windows {
        crate::codegen_support::tz_bridge::publish_elephc_tz_offset_function_pointer(emitter);
    }
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #32");                                 // reserve output, format byte, and length across date formatting
        emitter.instruction("ldr x9, [sp, #264]");                              // recover the protected callback's MbHostString output pointer
        emitter.instruction("str x9, [sp]");                                    // preserve writable output storage across helper calls
        emitter.instruction("mov w10, #114");                                   // RFC 2822 date format token 'r'
        emitter.instruction("strb w10, [sp, #8]");                              // materialize the one-byte format string
        emitter.instruction("mov x0, #-1");                                     // request the current Unix timestamp
        emitter.instruction("add x1, sp, #8");                                  // format pointer
        emitter.instruction("mov x2, #1");                                      // format length
        emitter.instruction("bl __rt_date");                                    // format through active PHP timezone state
        emitter.instruction("str x2, [sp, #16]");                               // retain exact output length across persistence
        emitter.instruction("bl __rt_str_persist");                             // acquire an independent owner for Rust to consume
        emitter.instruction("cbz x1, __rt_mbstring_mail_date_failed");          // allocation failure returns a fatal callback status
        emitter.instruction("ldr x9, [sp]");                                    // recover the output descriptor
        emitter.instruction("ldr x2, [sp, #16]");                               // restore formatted byte length
        emitter.instruction("stp x1, x2, [x9]");                                // publish owned bytes and length
        emitter.instruction("str x1, [x9, #16]");                               // the persisted bytes are their own release owner
        emitter.instruction("mov x0, #0");                                      // report successful formatting
        emitter.instruction("b __rt_mbstring_mail_date_body_done");             // share stack teardown
        emitter.label("__rt_mbstring_mail_date_failed");
        emitter.instruction("mov x0, #1");                                      // report runtime failure without fabricated bytes
        emitter.label("__rt_mbstring_mail_date_body_done");
        emitter.instruction("add sp, sp, #32");                                 // release callback-local scratch
    } else {
        emitter.instruction("sub rsp, 32");                                     // reserve output, format byte, and length across date formatting
        emitter.instruction("mov r10, QWORD PTR [rsp + 264]");                  // recover the protected callback's MbHostString output pointer
        emitter.instruction("mov QWORD PTR [rsp], r10");                        // preserve writable output storage across helper calls
        emitter.instruction("mov BYTE PTR [rsp + 8], 114");                     // materialize RFC 2822 date format token 'r'
        emitter.instruction("mov rax, -1");                                     // request the current Unix timestamp
        emitter.instruction("lea rdi, [rsp + 8]");                              // format pointer
        emitter.instruction("mov esi, 1");                                      // format length
        emitter.instruction("call __rt_date");                                  // format through active PHP timezone state
        emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                   // retain exact output length across persistence
        emitter.instruction("call __rt_str_persist");                           // acquire an independent owner for Rust to consume
        emitter.instruction("test rax, rax");                                   // persistence must return an owned byte pointer
        emitter.instruction("jz __rt_mbstring_mail_date_failed");               // allocation failure returns a fatal callback status
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover the output descriptor
        emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");                   // restore formatted byte length
        emitter.instruction("mov QWORD PTR [r10], rax");                        // publish owned bytes
        emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                    // publish exact byte length
        emitter.instruction("mov QWORD PTR [r10 + 16], rax");                   // the persisted bytes are their own release owner
        emitter.instruction("xor eax, eax");                                    // report successful formatting
        emitter.instruction("jmp __rt_mbstring_mail_date_body_done");           // share stack teardown
        emitter.label("__rt_mbstring_mail_date_failed");
        emitter.instruction("mov eax, 1");                                      // report runtime failure without fabricated bytes
        emitter.label("__rt_mbstring_mail_date_body_done");
        emitter.instruction("add rsp, 32");                                     // release callback-local scratch
    }
    if eval_bridge { emitter.label("__rt_mbstring_mail_date_body_return"); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Separates eval timezone formatting from native PHP date formatting and mail gating.
    #[test]
    fn mail_date_separates_aot_and_eval_timezone_providers() {
        for eval in [false, true] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
            emit(&mut emitter, eval, true);
            let asm = emitter.output();
            assert!(asm.contains("call __rt_date"));
            assert_eq!(asm.contains("elephc_eval_mbstring_mail_date_v1"), eval);
            assert!(asm.contains("call __rt_str_persist"));
        }
        let mut disabled = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit(&mut disabled, true, false);
        assert!(disabled.output().is_empty());

        let mut windows = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit(&mut windows, true, true);
        let asm = windows.output();
        let body = asm.split(&format!("{C_ABI_CALLBACK}:\n")).nth(1)
            .expect("mail Date C ABI callback");
        let entry = body.split(&format!("{C_ABI_CALLBACK}__internal:")).next().unwrap();
        assert!(entry.contains("mov rdi, rcx"));
        assert!(entry.contains("mov rsi, rdx"));
    }

    /// Windows crosses into Magician through the native two-integer bridge ABI.
    #[test]
    fn windows_eval_mail_date_uses_native_bridge_call() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit(&mut emitter, true, true);
        let asm = emitter.output();
        let body = asm.split("__rt_mbstring_mail_date:").nth(1)
            .expect("protected mail Date body");
        assert!(body.contains("lea r11, [rip + elephc_eval_mbstring_mail_date_v1]"));
        assert!(body.contains("sub rsp, 32"));
        assert!(body.contains("mov rdx, rsi"));
        assert!(body.contains("mov rcx, rdi"));
        assert!(body.contains("call r11"));
        assert!(!body.contains("call elephc_eval_mbstring_mail_date_v1"));
    }
}
