//! Purpose:
//! Contains eval output operations behind the shared native PHP exception boundary.
//!
//! Called from:
//! - `super::emit_eval_bridge_runtime()` and Magician's runtime value adapter.
//!
//! Key details:
//! - The request carries all six start arguments and independent result storage.
//! - No PHP exception crosses the Rust caller; invalid action numbers return failure.

use super::*;
use elephc_builtin_contract::output_abi::{OutputAction, OutputRequestV1};
use crate::codegen_support::runtime::exceptions::emit_protected_status;

const RESULT: usize = std::mem::offset_of!(OutputRequestV1, result);
const BYTES: usize = std::mem::offset_of!(OutputRequestV1, bytes);
const LENGTH: usize = std::mem::offset_of!(OutputRequestV1, length);

/// Routes shared boxed ob_* builtins through the same protected request as interpreter adapters.
pub(super) fn emit_boxed_builtin(emitter: &mut Emitter, id: elephc_builtin_contract::RuntimeBuiltinId) {
    use elephc_builtin_contract::RuntimeBuiltinId;
    let (action, flag) = match id {
        RuntimeBuiltinId::ObClean => (OutputAction::Clean, 0),
        RuntimeBuiltinId::ObFlush => (OutputAction::Flush, 0),
        RuntimeBuiltinId::ObEndClean => (OutputAction::End, 0),
        RuntimeBuiltinId::ObEndFlush => (OutputAction::End, 1),
        _ => unreachable!("only output operations use the protected boxed adapter"),
    };
    let failed = format!("__rt_eval_output_builtin_{}_failed", id.as_u32());
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #80");                                 // reserve a complete request beneath the shared dispatch frame
        for offset in (0..80).step_by(8) {
            emitter.instruction(&format!("str xzr, [sp, #{offset}]"));          // initialize every borrowed argument and result ownership field
        }
        emitter.instruction(&format!("mov x9, #{}", action as u64));            // select the typed output action
        emitter.instruction("str x9, [sp]");                                    // publish the request action
        emitter.instruction(&format!("mov x9, #{flag}"));                       // select clean or flush behavior for end operations
        emitter.instruction("str x9, [sp, #8]");                                // retain the first C operation argument
        emitter.instruction("mov x0, sp");                                      // pass the complete request to the protected output entry
        emitter.bl_c("__elephc_eval_output_v1");
        emitter.instruction(&format!("cbnz x0, {failed}"));                     // return pending exceptions before allocating a result box
        emitter.instruction(&format!("ldr x0, [sp, #{RESULT}]"));               // recover the successful PHP boolean result
        emitter.instruction("add sp, sp, #80");                                 // retire borrowed request storage before boxing
        emitter.bl_c("__elephc_eval_value_bool");
        emitter.instruction("b __elephc_runtime_builtin_v1_result");            // transfer the new boolean box through the shared ABI
        emitter.label(&failed);
        emitter.instruction("add sp, sp, #80");                                 // preserve the returned status while discarding request storage
        emitter.instruction("b __elephc_runtime_builtin_v1_done");              // let the interpreter recover the published native Throwable
    } else {
        emitter.instruction("sub rsp, 80");                                     // reserve the aligned output request beneath shared dispatch state
        for offset in (0..80).step_by(8) {
            emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], 0")); // initialize every request argument and output field
        }
        emitter.instruction(&format!("mov QWORD PTR [rsp], {}", action as u64)); // select the typed output action
        emitter.instruction(&format!("mov QWORD PTR [rsp + 8], {flag}"));       // retain the clean or flush selector
        emitter.instruction("mov rdi, rsp");                                    // pass caller-owned request storage to the protected entry
        emitter.bl_c("__elephc_eval_output_v1");
        emitter.instruction("test eax, eax");                                   // inspect success before materializing any result owner
        emitter.instruction(&format!("jnz {failed}"));                          // propagate the native status after Rust receives control again
        emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {RESULT}]"));   // recover the successful operation's PHP truth value
        emitter.instruction("add rsp, 80");                                     // restore the shared dispatcher frame before result allocation
        emitter.bl_c("__elephc_eval_value_bool");
        emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");      // transfer exactly one new boolean box
        emitter.label(&failed);
        emitter.instruction("add rsp, 80");                                     // discard request storage while preserving fatal or pending status
        emitter.instruction("jmp __elephc_runtime_builtin_v1_done_x86");        // return through the versioned shared builtin ABI
    }
}

/// Emits the versioned request entry using the complete native exception-handler record.
pub(super) fn emit(emitter: &mut Emitter) {
    let symbol = emitter.target.extern_symbol("__elephc_eval_output_v1");
    emit_protected_status(emitter, &symbol, body);
}

/// Dispatches the retained request while keeping the native return values separate from status.
fn body(emitter: &mut Emitter) {
    let actions = [OutputAction::Echo, OutputAction::Start, OutputAction::Clean,
        OutputAction::Flush, OutputAction::End, OutputAction::GetEnd];
    request(emitter);
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("cbz x9, __rt_eval_output_invalid");                // reject missing request storage before reading the action
        emitter.instruction("ldr x10, [x9]");                                   // inspect the versioned action number
        for action in actions {
            emitter.instruction(&format!("cmp x10, #{}", action as u64));       // match only one declared output action
            emitter.instruction(&format!("b.eq __rt_eval_output_{}", action as u64)); // enter its native argument adapter
        }
        emitter.label("__rt_eval_output_invalid");
        emitter.instruction("mov x0, #1");                                      // unknown action or missing storage is a fatal ABI status
        emitter.instruction("b __rt_eval_output_done");                         // restore the protected frame before returning failure
    } else {
        emitter.instruction("test r11, r11");                                   // require request storage before reading its action
        emitter.instruction("jz __rt_eval_output_invalid");                     // reject a missing request with no output owner
        emitter.instruction("mov r10, QWORD PTR [r11]");                        // inspect the versioned action number
        for action in actions {
            emitter.instruction(&format!("cmp r10, {}", action as u64));        // match only one declared output action
            emitter.instruction(&format!("je __rt_eval_output_{}", action as u64)); // enter the corresponding native argument adapter
        }
        emitter.label("__rt_eval_output_invalid");
        emitter.instruction("mov eax, 1");                                      // report an invalid action without entering a runtime operation
        emitter.instruction("jmp __rt_eval_output_done");                       // preserve failure through native boundary restoration
    }
    for action in actions {
        emitter.label(&format!("__rt_eval_output_{}", action as u64));
        action_body(emitter, action);
        request(emitter);
        if emitter.target.arch == Arch::AArch64 {
            emitter.instruction(&format!("str x0, [x9, #{RESULT}]"));           // publish the operation result separately from boundary status
            emitter.instruction("mov x0, #0");                                  // successful execution has no pending Throwable
            emitter.instruction("b __rt_eval_output_done");                     // share exception-state restoration across actions
        } else {
            emitter.instruction(&format!("mov QWORD PTR [r11 + {RESULT}], rax")); // publish the operation result without replacing the C status
            emitter.instruction("xor eax, eax");                                // successful execution returns boundary status zero
            emitter.instruction("jmp __rt_eval_output_done");                   // restore the protected caller state once
        }
    }
    emitter.label("__rt_eval_output_done");
}

/// Reloads the request address retained across setjmp and arbitrary PHP callbacks.
fn request(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x9, [sp, #224]");                              // recover the caller-owned request pointer
    } else {
        emitter.instruction("mov r11, QWORD PTR [rsp + 224]");                  // recover request storage after native calls clobber volatile registers
    }
}

/// Loads one borrowed request argument into its target C integer register.
fn argument(emitter: &mut Emitter, index: usize) {
    let offset = std::mem::offset_of!(OutputRequestV1, arguments) + index * 8;
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("ldr x{index}, [x9, #{offset}]"));         // marshal one retained request word into its C argument register
    } else {
        let register = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"][index];
        emitter.instruction(&format!("mov {register}, QWORD PTR [r11 + {offset}]")); // materialize one borrowed C argument without moving its owner
    }
}

/// Calls the existing output operation inside the request's exception boundary.
fn action_body(emitter: &mut Emitter, action: OutputAction) {
    match action {
        OutputAction::Echo => {
            argument(emitter, 0);
            emitter.bl_c("__elephc_eval_value_echo");
        },
        OutputAction::Start => {
            for index in 0..6 { argument(emitter, index); }
            emitter.bl_c("__elephc_eval_ob_start_ex");
        },
        OutputAction::Clean => emitter.bl_c("__elephc_eval_ob_clean"),
        OutputAction::Flush => emitter.bl_c("__elephc_eval_ob_flush"),
        OutputAction::End => {
            argument(emitter, 0);
            emitter.bl_c("__elephc_eval_ob_end");
        },
        OutputAction::GetEnd => {
            if emitter.target.arch == Arch::AArch64 {
                emitter.instruction("ldr x10, [x9, #8]");                       // inspect the get-and-pop flush selector
                emitter.instruction(&format!("add x0, x9, #{BYTES}"));          // provide caller-owned byte-pointer output storage
                emitter.instruction(&format!("add x1, x9, #{LENGTH}"));         // provide caller-owned byte-length output storage
                emitter.instruction("cbz x10, __rt_eval_output_get_clean");     // select the non-flushing get-and-pop operation
            } else {
                emitter.instruction("mov r10, QWORD PTR [r11 + 8]");            // inspect whether surviving bytes should be flushed
                emitter.instruction(&format!("lea rdi, [r11 + {BYTES}]"));      // give the legacy adapter writable byte-pointer storage
                emitter.instruction(&format!("lea rsi, [r11 + {LENGTH}]"));     // give the legacy adapter writable byte-length storage
                emitter.instruction("test r10, r10");                           // distinguish get_clean from get_flush
                emitter.instruction("jz __rt_eval_output_get_clean");           // run the requested non-flushing path
            }
            emitter.bl_c("__elephc_eval_ob_get_flush_pop");
            abi::emit_jump(emitter, "__rt_eval_output_get_done");
            emitter.label("__rt_eval_output_get_clean");
            emitter.bl_c("__elephc_eval_ob_get_clean_pop");
            emitter.label("__rt_eval_output_get_done");
        },
    }
}

#[cfg(test)]
mod tests;
