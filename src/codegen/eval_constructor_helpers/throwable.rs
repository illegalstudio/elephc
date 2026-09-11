//! Purpose:
//! Initializes compact native Throwable objects constructed through eval.
//!
//! Called from:
//! - The generated constructor bridge's class dispatch on every supported target.
//!
//! Key details:
//! - The private argument array owns borrowed parameter cells throughout construction.
//! - Replacing message and previous fields releases their class-initializer owners.
//! - Previous links use the shared writer so compact and ordinary object layouts stay consistent.

use super::*;

/// Emits ARM64 dispatch for compact builtin Throwable constructors.
pub(super) fn emit_aarch64_builtin_throwable_constructor_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_ids: &[u64],
    fail_label: &str,
    success_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for class_id in class_ids {
        let next_label = format!("__elephc_eval_builtin_throwable_next_{}", class_id);
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the unboxed object pointer before this builtin class test
        emitter.instruction("ldr x9, [x9]");                                    // load the receiver class id for builtin constructor dispatch
        abi::emit_load_int_immediate(emitter, "x10", *class_id as i64);
        emitter.instruction("cmp x9, x10");                                     // compare receiver class id against this builtin Throwable class
        emitter.instruction(&format!("b.ne {}", next_label));                   // try the next builtin Throwable class when ids differ
        emit_aarch64_builtin_throwable_constructor_body(
            module,
            emitter,
            data,
            fail_label,
            success_label,
            callable_support,
        );
        emitter.label(&next_label);
    }
}

/// Emits x86_64 dispatch for compact builtin Throwable constructors.
pub(super) fn emit_x86_64_builtin_throwable_constructor_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_ids: &[u64],
    fail_label: &str,
    success_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for class_id in class_ids {
        let next_label = format!("__elephc_eval_builtin_throwable_next_{}_x", class_id);
        emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                   // reload the unboxed object pointer before this builtin class test
        emitter.instruction("mov r11, QWORD PTR [r11]");                        // load the receiver class id for builtin constructor dispatch
        abi::emit_load_int_immediate(emitter, "r10", *class_id as i64);
        emitter.instruction("cmp r11, r10");                                    // compare receiver class id against this builtin Throwable class
        emitter.instruction(&format!("jne {}", next_label));                    // try the next builtin Throwable class when ids differ
        emit_x86_64_builtin_throwable_constructor_body(
            module,
            emitter,
            data,
            fail_label,
            success_label,
            callable_support,
        );
        emitter.label(&next_label);
    }
}

/// Initializes the compact Throwable payload for eval-created ARM64 builtin exceptions.
fn emit_aarch64_builtin_throwable_constructor_body(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    fail_label: &str,
    success_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    emit_aarch64_validate_builtin_throwable_arg_count(module, emitter, fail_label);
    emit_aarch64_default_builtin_throwable_fields(emitter);
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload constructor argc before testing the message argument
    emitter.instruction("cmp x9, #0");                                          // did the eval call pass a message argument?
    emitter.instruction(&format!("b.eq {}", success_label));                    // keep the empty Throwable defaults when no message was supplied
    emit_aarch64_load_eval_arg(module, emitter, 0);
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for message initialization
    emitter.instruction("str x1, [x9, #8]");                                    // store the message pointer in the compact Throwable payload
    emitter.instruction("str x2, [x9, #16]");                                   // store the message length in the compact Throwable payload
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload constructor argc before testing the code argument
    emitter.instruction("cmp x9, #1");                                          // did the eval call pass a code argument?
    emitter.instruction(&format!("b.le {}", success_label));                    // keep code zero when only the message was supplied
    emit_aarch64_load_eval_arg(module, emitter, 1);
    emit_aarch64_cast_eval_arg(
        module,
        emitter,
        &PhpType::Int,
        "__elephc_eval_builtin_throwable_code",
        fail_label,
        data,
        callable_support,
    );
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for code initialization
    emitter.instruction("str x0, [x9, #24]");                                   // store the integer exception code
    emit_aarch64_builtin_throwable_previous_arg(
        module,
        emitter,
        fail_label,
        success_label,
    );
}

/// Initializes the compact Throwable payload for eval-created x86_64 builtin exceptions.
fn emit_x86_64_builtin_throwable_constructor_body(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    fail_label: &str,
    success_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    emit_x86_64_validate_builtin_throwable_arg_count(module, emitter, fail_label);
    emit_x86_64_default_builtin_throwable_fields(emitter);
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload constructor argc before testing the message argument
    emitter.instruction("cmp r11, 0");                                          // did the eval call pass a message argument?
    emitter.instruction(&format!("je {}", success_label));                      // keep the empty Throwable defaults when no message was supplied
    emit_x86_64_load_eval_arg(module, emitter, 0);
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for message initialization
    emitter.instruction("mov QWORD PTR [r11 + 8], rax");                        // store the message pointer in the compact Throwable payload
    emitter.instruction("mov QWORD PTR [r11 + 16], rdx");                       // store the message length in the compact Throwable payload
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload constructor argc before testing the code argument
    emitter.instruction("cmp r11, 1");                                          // did the eval call pass a code argument?
    emitter.instruction(&format!("jle {}", success_label));                     // keep code zero when only the message was supplied
    emit_x86_64_load_eval_arg(module, emitter, 1);
    emit_x86_64_cast_eval_arg(
        module,
        emitter,
        &PhpType::Int,
        "__elephc_eval_builtin_throwable_code_x",
        fail_label,
        data,
        callable_support,
    );
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for code initialization
    emitter.instruction("mov QWORD PTR [r11 + 24], rax");                       // store the integer exception code
    emit_x86_64_builtin_throwable_previous_arg(
        module,
        emitter,
        fail_label,
        success_label,
    );
}

/// Transfers the nullable third constructor argument through the shared AArch64 previous-slot writer.
fn emit_aarch64_builtin_throwable_previous_arg(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    success_label: &str,
) {
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload argc before testing the previous argument
    emitter.instruction("cmp x9, #2");                                          // did eval supply the normalized previous argument?
    emitter.instruction(&format!("b.le {}", success_label));                    // keep null when the legacy bridge omitted previous
    emit_aarch64_load_eval_arg(module, emitter, 2);
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed previous argument for inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the nullable previous payload
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means the previous argument is null
    emitter.instruction(&format!("b.eq {}", success_label));                    // keep the default raw null previous pointer
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the previous argument is an object
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject malformed non-object previous arguments
    emitter.instruction("mov x0, x1");                                          // move the previous object payload into the retain ABI
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov x1, x0");                                          // transfer the retained previous owner into the matching storage layout
    emitter.instruction("ldr x0, [sp, #16]");                                   // recover the constructor receiver for the shared previous-slot writer
    abi::emit_call_label(emitter, "__rt_throwable_append_previous");
    emitter.instruction(&format!("b {}", success_label));                       // builtin Throwable construction completed
}

/// Transfers the nullable third constructor argument through the shared SysV previous-slot writer.
fn emit_x86_64_builtin_throwable_previous_arg(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    success_label: &str,
) {
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload argc before testing the previous argument
    emitter.instruction("cmp r11, 2");                                          // did eval supply the normalized previous argument?
    emitter.instruction(&format!("jle {}", success_label));                     // keep null when the legacy bridge omitted previous
    emit_x86_64_load_eval_arg(module, emitter, 2);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed previous argument for inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the nullable previous payload
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means the previous argument is null
    emitter.instruction(&format!("je {}", success_label));                      // keep the default raw null previous pointer
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the previous argument is an object
    emitter.instruction(&format!("jne {}", fail_label));                        // reject malformed non-object previous arguments
    emitter.instruction("mov rax, rdi");                                        // move the previous object payload into the retain ABI
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov rsi, rax");                                        // transfer the retained previous owner into the matching storage layout
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // recover the constructor receiver for the shared previous-slot writer
    abi::emit_call_label(emitter, "__rt_throwable_append_previous");
    emitter.instruction(&format!("jmp {}", success_label));                     // builtin Throwable construction completed
}

/// Emits ARM64 arity validation for compact builtin Throwable constructors.
fn emit_aarch64_validate_builtin_throwable_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the eval argument array for builtin Throwable arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    emitter.instruction("str x0, [sp, #40]");                                   // preserve argc outside the eval argument scratch slot
    emitter.instruction("cmp x0, #3");                                          // compact Throwable initialization supports message/code/previous
    emitter.instruction(&format!("b.gt {}", fail_label));                       // reject excess builtin Throwable arguments from eval
}

/// Emits x86_64 arity validation for compact builtin Throwable constructors.
fn emit_x86_64_validate_builtin_throwable_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the eval argument array for builtin Throwable arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save constructor argc for message/code initialization
    emitter.instruction("cmp rax, 3");                                          // compact Throwable initialization supports message/code/previous
    emitter.instruction(&format!("jg {}", fail_label));                         // reject excess builtin Throwable arguments from eval
}

/// Releases prior message/previous owners before installing the empty Throwable defaults.
fn emit_aarch64_default_builtin_throwable_fields(emitter: &mut Emitter) {
    for offset in [8, 40] {
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the compact Throwable receiver across owner release
        emitter.instruction(&format!("ldr x0, [x9, #{}]", offset));             // take the previous field owner, including class initializer storage
        emitter.instruction(&format!("str xzr, [x9, #{}]", offset));            // detach the field before release can execute a destructor
        abi::emit_call_label(emitter, "__rt_decref_any");
    }
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable for scalar defaults
    emitter.instruction("str xzr, [x9, #16]");                                  // default the message length to zero
    emitter.instruction("str xzr, [x9, #24]");                                  // default the exception code to zero
}

/// Releases prior message/previous owners before installing the empty Throwable defaults.
fn emit_x86_64_default_builtin_throwable_fields(emitter: &mut Emitter) {
    for offset in [8, 40] {
        emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                   // reload the compact Throwable receiver across owner release
        emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", offset)); // take the previous field owner, including class initializer storage
        emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", offset));   // detach the field before release can execute a destructor
        abi::emit_call_label(emitter, "__rt_decref_any");
    }
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable for scalar defaults
    emitter.instruction("mov QWORD PTR [r11 + 16], 0");                         // default the message length to zero
    emitter.instruction("mov QWORD PTR [r11 + 24], 0");                         // default the exception code to zero
}
