//! Purpose:
//! Boxes persistent native callable descriptors and contains their invocation below Rust.
//!
//! Called from:
//! - Magician runtime value hooks for native callables and Reflection lazy factories.
//!
//! Key details:
//! - Native default factories carry no captures or eval-context pointers.
//! - Borrowed descriptor boxing follows the ordinary Callable ownership contract.
//! - The versioned invocation adapter contains native exceptions below Rust and publishes an
//!   owned result only when the native exception cell is empty.

use super::*;

/// Emits the C ABI that boxes a borrowed persistent native factory as an owned Mixed callable.
pub(super) fn emit(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_callable_from_raw");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter),
        abi::int_arg_reg_name(emitter.target, 0));
    let done = "__elephc_eval_value_callable_from_raw_done";
    abi::emit_branch_if_int_result_zero(emitter, done);
    crate::codegen_support::value_boxing::emit_box_current_value_as_mixed(
        emitter, &crate::types::PhpType::Callable,
    );
    emitter.label(done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
    emit_invocation_boundary(emitter);
    super::super::exceptions::emit_protected_status(
        emitter, "__rt_eval_invoke_native_callable_v2", invoke_body,
    );
}

/// Preserves an inherited native exception independently from this call's status and result.
fn emit_invocation_boundary(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_invoke_native_callable_v2");
    abi::emit_frame_prologue(emitter, 96);
    for index in 0..4 {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), 8 + index * 8);
    }
    let result = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, 48);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    for index in 0..4 {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), 8 + index * 8);
    }
    abi::emit_call_label(emitter, "__rt_eval_invoke_native_callable_v2");
    abi::store_at_offset(emitter, result, 56);
    let thrown = "__rt_eval_native_callable_boundary_thrown";
    let returned = "__rt_eval_native_callable_boundary_returned";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #2");                                  // only a newly caught throw replaces the inherited exception owner
            emitter.instruction(&format!("b.eq {thrown}"));                     // chain the parked exception without exposing a native unwind to Rust
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 2");                                  // success and unsupported preserve the inherited native exception
            emitter.instruction(&format!("je {thrown}"));                       // a new throw keeps its own pending-object identity
        }
    }
    abi::load_at_offset(emitter, result, 48);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_jump(emitter, returned);
    emitter.label(thrown);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, 64);
    abi::load_at_offset(emitter, result, 48);
    abi::store_at_offset(emitter, result, 72);
    abi::emit_frame_slot_address(emitter, result, 72);
    abi::emit_unary_cleanup_preserving_exception(
        emitter, "__rt_eval_native_callable_chain_previous", result,
    );
    emitter.label(returned);
    abi::load_at_offset(emitter, result, 56);
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
    emit_previous_chain_callback(emitter);
}

/// Consumes one parked predecessor under the shared exception-preserving cleanup boundary.
fn emit_previous_chain_callback(emitter: &mut Emitter) {
    emitter.label_global("__rt_eval_native_callable_chain_previous");
    abi::emit_frame_prologue(emitter, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x1, [x0]");                                // consume the parked inherited exception owner as the previous-chain argument
            emitter.instruction("str xzr, [x0]");                               // clear transferred ownership before the chaining helper can execute cleanup
            emitter.instruction("ldr x0, [x0, #8]");                            // borrow the new exception retained by the cleanup boundary
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, rax");                                // preserve the native unary input while materializing the two chain arguments
            emitter.instruction("mov rdi, QWORD PTR [r10]");                    // consume the inherited exception owner exactly once
            emitter.instruction("mov QWORD PTR [r10], 0");                      // retire the staging owner before entering exception-chain cleanup
            emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                // borrow the new exception preserved outside this callback
        }
    }
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Calls `(descriptor, args, invocation_scope)` and reports 0 success, 1 unsupported or 2 throw.
///
/// The fourth retained C argument addresses a nullable owned output slot, initialized before any
/// operation that can throw. The callback and argument container remain borrowed from Rust.
fn invoke_body(emitter: &mut Emitter) {
    let unsupported = "__rt_eval_invoke_native_callable_v2_unsupported";
    let pending = "__rt_eval_invoke_native_callable_v2_pending";
    let done = "__rt_eval_invoke_native_callable_v2_body_done";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x10, [sp, #248]");                         // recover the caller-owned output slot retained by the native boundary
            emitter.instruction("str xzr, [x10]");                              // a failed invocation must never publish an owned result
            emitter.instruction("ldr x0, [sp, #224]");                          // borrow the boxed callable retained by Rust throughout this invocation
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp + 248]");              // recover the C output pointer before decoding the callback
            emitter.instruction("mov QWORD PTR [r10], 0");                      // leave failed or unsupported invocations with no output owner
            emitter.instruction("mov rax, QWORD PTR [rsp + 224]");              // adapt the borrowed boxed callback to the native unbox convention
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #10");                                 // only a native Callable box carries a descriptor address
            emitter.instruction(&format!("b.ne {unsupported}"));                // other callable representations need interpreter normalization
            emitter.instruction(&format!("cbz x1, {unsupported}"));             // a missing descriptor cannot expose an invocation entry
            emitter.instruction("ldr x9, [x1, #56]");                           // select the uniform descriptor invoker rather than its typed native entry
            emitter.instruction(&format!("cbz x9, {unsupported}"));             // optional invokers report an unsupported bridge explicitly
            emitter.instruction("mov x0, x1");                                  // argument zero is the descriptor, including its persistent captures
            emitter.instruction("ldr x1, [sp, #232]");                          // argument one contains only the PHP arguments supplied by the caller
            emitter.instruction("ldr x2, [sp, #240]");                          // argument two preserves call-site visibility and strict scalar scope
            emitter.instruction("blr x9");                                      // contain all native throws inside the complete protected callback frame
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 10");                                 // the unboxed low word denotes a descriptor only for Callable values
            emitter.instruction(&format!("jne {unsupported}"));                 // ordinary strings and arrays remain the interpreter's responsibility
            emitter.instruction("test rdi, rdi");                               // the native unboxer returns the descriptor in its low-payload register
            emitter.instruction(&format!("jz {unsupported}"));                  // reject an absent descriptor before reading its side records
            emitter.instruction("mov r10, QWORD PTR [rdi + 56]");               // preserve the uniform invoker while materializing its three ABI arguments
            emitter.instruction("test r10, r10");                               // some descriptor forms have no dynamic invocation adapter
            emitter.instruction(&format!("jz {unsupported}"));                  // report unsupported without unwinding into Rust
            emitter.instruction("mov rsi, QWORD PTR [rsp + 232]");              // pass the borrowed supplied-argument container after the descriptor in rdi
            emitter.instruction("mov rdx, QWORD PTR [rsp + 240]");              // forward invocation scope through the third SysV argument
            emitter.instruction("call r10");                                    // invoke through the protected descriptor boundary
        }
    }
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_value", 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {scratch}, {pending}"));         // eval-boundary invokers can return normally while retaining a pending throw
            emitter.instruction("ldr x10, [sp, #248]");                         // publish only a successful owned Mixed result
            emitter.instruction("str x0, [x10]");                               // transfer the result into the caller's output owner
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {scratch}, {scratch}"));         // distinguish a genuine success from an invoker's caught native throw
            emitter.instruction(&format!("jnz {pending}"));                     // pending Throwable takes precedence over an apparent returned value
            emitter.instruction("mov r10, QWORD PTR [rsp + 248]");              // recover the caller's output pointer after native execution
            emitter.instruction("mov QWORD PTR [r10], rax");                    // transfer exactly one successful result owner to Rust
        }
    }
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_jump(emitter, done);
    emitter.label(pending);
    abi::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_mixed", abi::int_result_reg(emitter));
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 2);
    abi::emit_jump(emitter, done);
    emitter.label(unsupported);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    emitter.label(done);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every target forwards the third invoker argument and checks pending throws before output.
    #[test]
    fn eval_native_callable_invocation_is_protected_and_preserves_scope_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains(&target.extern_symbol("__elephc_eval_invoke_native_callable_v2")), "{name}");
            let (scope, call, publish) = if target.arch == Arch::AArch64 {
                ("ldr x2, [sp, #240]", "blr x9", "str x0, [x10]")
            } else {
                ("mov rdx, QWORD PTR [rsp + 240]", "call r10", "mov QWORD PTR [r10], rax")
            };
            assert!(assembly.find(scope).unwrap() < assembly.find(call).unwrap(), "{name}");
            let body = assembly.split_once(scope).unwrap().1;
            assert!(body.find("_exc_value").unwrap() < body.find(publish).unwrap(), "{name}");
            assert!(body.contains("__rt_cleanup_preserve_exception"), "{name}: abandon interrupted result safely");
        }
    }

    /// All five targets box the descriptor without depending on an eval-context callback.
    #[test]
    fn eval_native_callable_factory_emits_for_all_supported_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains(&target.extern_symbol("__elephc_eval_value_callable_from_raw")), "{name}");
            assert!(assembly.contains("__rt_mixed_from_value"), "{name}");
            assert!(!assembly.contains("_elephc_eval_wrap_callback_fn"), "{name}");
            assert!(!assembly.contains("__rt_callable_descriptor_release"), "{name}: boxing borrows the persistent descriptor");
        }
    }
}
