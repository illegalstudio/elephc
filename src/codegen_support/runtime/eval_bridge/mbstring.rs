//! Purpose:
//! Routes boxed eval mbstring calls through the shared PHP invocation coordinator.
//!
//! Called from:
//! - The versioned runtime builtin dispatcher for registered typed mbstring identities.
//!
//! Key details:
//! - Shared Rust planning owns arity, outer coercion, diagnostics, and delayed array snapshots.
//! - The retained eval context reaches protected Stringable and metadata callbacks.
//! - Pending exceptions return a status without unwinding through Magician.
//! - Eval source strictness remains pending; fragments currently use their default weak mode.

use super::*;
use crate::builtins::registry::eval_runtime_builtin_ids;

/// Joins all AArch64 operation labels to one shared boxed-argument invocation path.
pub(super) fn emit_aarch64_mbstring(emitter: &mut Emitter, mbregex: bool) {
    for operation in eval_runtime_builtin_ids().filter(|id| id.is_mbstring()) {
        emitter.label(&format!("__elephc_runtime_builtin_v1_mbstring_{}", operation.as_u32()));
    }
    emitter.instruction("mov x0, x19");                                         // retain the selected typed mbstring operation
    emitter.instruction("mov x1, x20");                                         // pass the original borrowed boxed argument array
    emitter.instruction("mov x2, x21");                                         // preserve omitted arguments through the supplied count
    emitter.instruction("mov x3, #0");                                          // use eval fragment default weak parsing until source profiles are propagated
    emitter.instruction("mov x4, x23");                                         // propagate the active eval context to protected host callbacks
    emit_invoke(emitter, mbregex);
    emitter.instruction("cbnz x1, __elephc_runtime_builtin_v1_mbstring_failed"); // return a pending exception without creating a result cell
    emitter.instruction("bl __rt_mbstring_box_result");                         // box the successful result and consume its native owner
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // transfer one fresh result cell through the versioned dispatcher
    emitter.label("__elephc_runtime_builtin_v1_mbstring_failed");
    emitter.instruction("mov x0, x1");                                          // return the coordinator or materializer failure status
    emitter.instruction("b __elephc_runtime_builtin_v1_done");                  // restore the dispatcher frame without unwinding through Rust
}

/// Joins all SysV operation labels to the same coordinator with the retained incoming context.
pub(super) fn emit_x86_64_mbstring(emitter: &mut Emitter, mbregex: bool) {
    for operation in eval_runtime_builtin_ids().filter(|id| id.is_mbstring()) {
        emitter.label(&format!("__elephc_runtime_builtin_v1_mbstring_{}_x86", operation.as_u32()));
    }
    emitter.instruction("mov edi, ebx");                                        // pass the selected typed mbstring operation
    emitter.instruction("mov rsi, r12");                                        // pass the original borrowed boxed argument array
    emitter.instruction("mov rdx, r13");                                        // retain the exact supplied count for arity and optional arguments
    emitter.instruction("xor ecx, ecx");                                        // use eval fragment default weak parsing until source profiles are propagated
    emitter.instruction("mov r8, r15");                                         // propagate the retained active eval context to protected callbacks
    emit_invoke(emitter, mbregex);
    emitter.instruction("test rdx, rdx");                                       // inspect the non-unwinding invocation status
    emitter.instruction("jnz __elephc_runtime_builtin_v1_mbstring_failed_x86"); // return pending exceptions without successful result ownership
    emitter.instruction("call __rt_mbstring_box_result");                       // box the successful scalar/array and consume its native owner
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // publish the fresh boxed PHP result
    emitter.label("__elephc_runtime_builtin_v1_mbstring_failed_x86");
    emitter.instruction("mov eax, edx");                                        // preserve the coordinator or materializer failure status
    emitter.instruction("jmp __elephc_runtime_builtin_v1_done_x86");            // restore caller linkage without crossing Magician via longjmp
}

/// Selects the V5 query host, optional V4 regex host, or ordinary shared-value coordinator.
fn emit_invoke(emitter: &mut Emitter, mbregex: bool) {
    use elephc_builtin_contract::RuntimeBuiltinId;
    let arm = emitter.target.arch == crate::codegen_support::platform::Arch::AArch64;
    let capture = "__elephc_runtime_builtin_v1_mbstring_capture";
    let ordinary = "__elephc_runtime_builtin_v1_mbstring_values";
    let done = "__elephc_runtime_builtin_v1_mbstring_invoked";
    let not_query = "__elephc_runtime_builtin_v1_mbstring_not_query";
    if arm {
        emitter.instruction(&format!("cmp x19, #{}", RuntimeBuiltinId::MbParseStr.as_u32())); // distinguish query output from ordinary values and regex captures
        emitter.instruction(&format!("b.ne {not_query}"));                      // keep existing operation families on their current adapters
    } else {
        emitter.instruction(&format!("cmp ebx, {}", RuntimeBuiltinId::MbParseStr.as_u32())); // identify the typed query operation without inspecting a PHP name
        emitter.instruction(&format!("jne {not_query}"));                       // preserve ordinary and regex dispatch for every other operation
    }
    let state_bytes = crate::codegen_support::mbstring_query::STATE_BYTES;
    abi::emit_reserve_temporary_stack(emitter, state_bytes);
    crate::codegen_support::mbstring_query::stage(emitter, 0);
    abi::emit_call_label(emitter, "__rt_mbstring_query_invoke");
    abi::emit_release_temporary_stack(emitter, state_bytes);
    abi::emit_jump(emitter, done);
    emitter.label(not_query);
    if arm {
        emitter.instruction(&format!("cmp x19, #{}", RuntimeBuiltinId::MbEreg.as_u32())); // identify the case-sensitive capture operation
        emitter.instruction(&format!("b.eq {capture}"));                        // preserve the original output wrapper for reference calls
        emitter.instruction(&format!("cmp x19, #{}", RuntimeBuiltinId::MbEregi.as_u32())); // identify the case-insensitive capture operation
        emitter.instruction(&format!("b.ne {ordinary}"));                       // keep ordinary values on the existing shared coordinator
    } else {
        emitter.instruction(&format!("cmp ebx, {}", RuntimeBuiltinId::MbEreg.as_u32())); // recognize the typed capture operation
        emitter.instruction(&format!("je {capture}"));                          // route persistent output references through the V4 host
        emitter.instruction(&format!("cmp ebx, {}", RuntimeBuiltinId::MbEregi.as_u32())); // recognize the case-insensitive capture operation
        emitter.instruction(&format!("jne {ordinary}"));                        // preserve the ordinary value invocation path
    }
    emitter.label(capture);
    if mbregex {
        if arm {
            emitter.instruction("sub sp, sp, #16");                             // allocate aligned invocation-local capture state
            emitter.instruction("stp xzr, xzr, [sp]");                          // select untyped publication and clear deferred ownership
            emitter.instruction("mov x5, sp");                                  // pass the sixth C input without changing the evaluated arguments
            emitter.instruction("bl __rt_mbstring_capture_invoke");             // run shared initialization, matching, publication, and protected cleanup
            emitter.instruction("add sp, sp, #16");                             // release capture state after deferred owners have been adopted
            emitter.instruction(&format!("b {done}"));                          // share native status translation and result boxing
        } else {
            emitter.instruction("sub rsp, 16");                                 // reserve aligned invocation-local capture state
            emitter.instruction("mov QWORD PTR [rsp], 0");                      // select untyped reference publication
            emitter.instruction("mov QWORD PTR [rsp + 8], 0");                  // initialize deferred output ownership
            emitter.instruction("mov r9, rsp");                                 // provide the sixth C input alongside the retained eval context
            emitter.instruction("call __rt_mbstring_capture_invoke");           // contain PHP exceptions until the shared coordinator returns
            emitter.instruction("add rsp, 16");                                 // release state without changing the returned status tuple
            emitter.instruction(&format!("jmp {done}"));                        // join ordinary result boxing and pending-exception handling
        }
    } else {
        emitter.instruction(if arm { "b __elephc_runtime_builtin_v1_unsupported" } else { "jmp __elephc_runtime_builtin_v1_unsupported_x86" }); // reject unavailable captures without referencing a missing native helper
    }
    emitter.label(ordinary);
    emitter.instruction(if arm { "bl __rt_mbstring_invoke" } else { "call __rt_mbstring_invoke" }); // retain the existing by-value invocation coordinator
    emitter.label(done);
}
