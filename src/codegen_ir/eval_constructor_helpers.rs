//! Purpose:
//! Emits user-assembly helpers that let libelephc-eval run public native
//! constructors after allocating AOT objects by class name.
//!
//! Called from:
//! - `crate::codegen_ir::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - The cacheable runtime object can allocate by name, but only user assembly
//!   knows constructor symbols and parameter ABI shapes.
//! - Classes without constructors are treated as successful no-ops, matching PHP.
//! - Constructors are bridged for fixed non-by-ref scalar/Mixed arguments.

use std::collections::BTreeMap;

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::{Function, LocalKind, Module};
use crate::names::{method_symbol, php_symbol_key};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, FunctionSig, PhpType};

const MAX_EVAL_CONSTRUCTOR_ARGS: usize = 8;
const BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES: &[&str] = &[
    "Error",
    "TypeError",
    "ValueError",
    "Exception",
    "LogicException",
    "BadFunctionCallException",
    "BadMethodCallException",
    "DomainException",
    "InvalidArgumentException",
    "LengthException",
    "OutOfRangeException",
    "RuntimeException",
    "OutOfBoundsException",
    "OverflowException",
    "RangeException",
    "UnderflowException",
    "UnexpectedValueException",
    "JsonException",
    "FiberError",
];

/// Constructor metadata needed by the eval constructor bridge.
#[derive(Clone)]
struct EvalConstructorSlot {
    class_id: u64,
    class_name: String,
    impl_class: String,
    params: Vec<PhpType>,
    supported: bool,
}

/// Emits eval constructor helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_constructor_helpers(module: &Module, emitter: &mut Emitter) {
    if !module_uses_eval(module) {
        return;
    }
    let slots = collect_eval_constructor_slots(module);
    let builtin_throwable_class_ids = collect_builtin_throwable_constructor_class_ids(module);
    emit_constructor_helper(module, emitter, &slots, &builtin_throwable_class_ids);
}

/// Returns true when the EIR module contains a function that can call eval.
fn module_uses_eval(module: &Module) -> bool {
    all_module_functions(module).any(function_uses_eval)
}

/// Iterates every EIR function body emitted or inspected by the backend.
fn all_module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Returns true when a function has hidden eval state locals.
fn function_uses_eval(function: &Function) -> bool {
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
}

/// Collects AOT constructors backed by emitted EIR symbols in stable class-id order.
fn collect_eval_constructor_slots(module: &Module) -> Vec<EvalConstructorSlot> {
    let emitted_methods = super::eir_class_method_keys(module);
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_constructor_slot(
            class_name,
            class_info,
            &emitted_methods,
            &mut slots,
        );
    }
    slots
}

/// Collects compact builtin Throwable class ids that eval can initialize directly.
fn collect_builtin_throwable_constructor_class_ids(module: &Module) -> Vec<u64> {
    let mut class_ids = BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES
        .iter()
        .filter_map(|class_name| module.class_infos.get(*class_name))
        .map(|class_info| class_info.class_id)
        .collect::<Vec<_>>();
    class_ids.sort_unstable();
    class_ids.dedup();
    class_ids
}

/// Adds one constructor slot for a class when the constructor has emitted code.
fn collect_class_constructor_slot(
    class_name: &str,
    class_info: &ClassInfo,
    emitted_methods: &std::collections::HashSet<(String, String, bool)>,
    slots: &mut Vec<EvalConstructorSlot>,
) {
    let method_key = php_symbol_key("__construct");
    let Some(sig) = class_info.methods.get(&method_key) else {
        return;
    };
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    if !emitted_methods.contains(&(impl_class.to_string(), method_key.clone(), false)) {
        return;
    }
    let supported = constructor_is_public(class_info, &method_key)
        && constructor_signature_supported(sig);
    let params = if supported {
        sig.params.iter().map(|(_, ty)| ty.codegen_repr()).collect()
    } else {
        Vec::new()
    };
    slots.push(EvalConstructorSlot {
        class_id: class_info.class_id,
        class_name: class_name.to_string(),
        impl_class: impl_class.to_string(),
        params,
        supported,
    });
}

/// Returns true when the constructor is publicly visible to runtime eval.
fn constructor_is_public(class_info: &ClassInfo, method_key: &str) -> bool {
    class_info
        .method_visibilities
        .get(method_key)
        .is_none_or(|visibility| matches!(visibility, Visibility::Public))
}

/// Returns true for constructor signatures supported by this eval bridge slice.
fn constructor_signature_supported(sig: &FunctionSig) -> bool {
    sig.params.len() <= MAX_EVAL_CONSTRUCTOR_ARGS
        && sig.variadic.is_none()
        && sig.ref_params.iter().all(|is_ref| !*is_ref)
        && sig
            .params
            .iter()
            .all(|(_, ty)| constructor_param_supported(ty))
}

/// Returns true for one constructor argument type supported by the bridge.
fn constructor_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str | PhpType::Mixed
    )
}

/// Emits `__elephc_eval_value_construct_object(Mixed*, MixedArray*) -> bool`.
fn emit_constructor_helper(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalConstructorSlot],
    builtin_throwable_class_ids: &[u64],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user constructor call ---");
    label_c_global(module, emitter, "__elephc_eval_value_construct_object");
    match module.target.arch {
        Arch::AArch64 => {
            emit_constructor_aarch64(module, emitter, slots, builtin_throwable_class_ids)
        }
        Arch::X86_64 => emit_constructor_x86_64(module, emitter, slots, builtin_throwable_class_ids),
    }
}

/// Emits the ARM64 constructor helper body.
fn emit_constructor_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalConstructorSlot],
    builtin_throwable_class_ids: &[u64],
) {
    let success_label = "__elephc_eval_value_construct_success";
    let fail_label = "__elephc_eval_value_construct_fail";
    let done_label = "__elephc_eval_value_construct_done";
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for args, object, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the boxed eval argument array
    emitter.instruction(&format!("cbz x0, {}", success_label));                 // a null object pointer means there is nothing to construct
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", success_label));                    // non-object values have no constructor to run
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for constructor calls
    emit_aarch64_builtin_throwable_constructor_dispatch(
        module,
        emitter,
        builtin_throwable_class_ids,
        fail_label,
        success_label,
    );
    emit_aarch64_constructor_dispatch(module, emitter, slots, fail_label, success_label);
    emitter.instruction(&format!("b {}", success_label));                       // no constructor metadata matched this class id
    emitter.label(fail_label);
    emitter.instruction("mov x0, #0");                                          // report constructor dispatch failure to Rust
    emitter.instruction(&format!("b {}", done_label));                          // skip the success result after a failure
    emitter.label(success_label);
    emitter.instruction("mov x0, #1");                                          // report successful construction or no-op
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release the constructor helper frame
    emitter.instruction("ret");                                                 // return the constructor status flag to Rust
}

/// Emits the x86_64 constructor helper body.
fn emit_constructor_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalConstructorSlot],
    builtin_throwable_class_ids: &[u64],
) {
    let success_label = "__elephc_eval_value_construct_success_x";
    let fail_label = "__elephc_eval_value_construct_fail_x";
    let done_label = "__elephc_eval_value_construct_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for object, args, and temp values
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the boxed eval argument array
    emitter.instruction("test rdi, rdi");                                       // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", success_label));                      // a null object pointer means there is nothing to construct
    emitter.instruction("mov rax, rdi");                                        // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", success_label));                     // non-object values have no constructor to run
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the unboxed object pointer for constructor calls
    emit_x86_64_builtin_throwable_constructor_dispatch(
        module,
        emitter,
        builtin_throwable_class_ids,
        fail_label,
        success_label,
    );
    emit_x86_64_constructor_dispatch(module, emitter, slots, fail_label, success_label);
    emitter.instruction(&format!("jmp {}", success_label));                     // no constructor metadata matched this class id
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // report constructor dispatch failure to Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // skip the success result after a failure
    emitter.label(success_label);
    emitter.instruction("mov eax, 1");                                          // report successful construction or no-op
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the constructor status flag to Rust
}

/// Emits ARM64 dispatch for compact builtin Throwable constructors.
fn emit_aarch64_builtin_throwable_constructor_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    class_ids: &[u64],
    fail_label: &str,
    success_label: &str,
) {
    for class_id in class_ids {
        let next_label = format!("__elephc_eval_builtin_throwable_next_{}", class_id);
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the unboxed object pointer before this builtin class test
        emitter.instruction("ldr x9, [x9]");                                    // load the receiver class id for builtin constructor dispatch
        abi::emit_load_int_immediate(emitter, "x10", *class_id as i64);
        emitter.instruction("cmp x9, x10");                                     // compare receiver class id against this builtin Throwable class
        emitter.instruction(&format!("b.ne {}", next_label));                   // try the next builtin Throwable class when ids differ
        emit_aarch64_builtin_throwable_constructor_body(module, emitter, fail_label, success_label);
        emitter.label(&next_label);
    }
}

/// Emits x86_64 dispatch for compact builtin Throwable constructors.
fn emit_x86_64_builtin_throwable_constructor_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    class_ids: &[u64],
    fail_label: &str,
    success_label: &str,
) {
    for class_id in class_ids {
        let next_label = format!("__elephc_eval_builtin_throwable_next_{}_x", class_id);
        emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                   // reload the unboxed object pointer before this builtin class test
        emitter.instruction("mov r11, QWORD PTR [r11]");                        // load the receiver class id for builtin constructor dispatch
        abi::emit_load_int_immediate(emitter, "r10", *class_id as i64);
        emitter.instruction("cmp r11, r10");                                    // compare receiver class id against this builtin Throwable class
        emitter.instruction(&format!("jne {}", next_label));                    // try the next builtin Throwable class when ids differ
        emit_x86_64_builtin_throwable_constructor_body(module, emitter, fail_label, success_label);
        emitter.label(&next_label);
    }
}

/// Initializes the compact Throwable payload for eval-created ARM64 builtin exceptions.
fn emit_aarch64_builtin_throwable_constructor_body(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    success_label: &str,
) {
    emit_aarch64_validate_builtin_throwable_arg_count(module, emitter, fail_label);
    emit_aarch64_default_builtin_throwable_fields(emitter);
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload constructor argc before testing the message argument
    emitter.instruction("cmp x9, #0");                                          // did the eval call pass a message argument?
    emitter.instruction(&format!("b.eq {}", success_label));                    // keep the empty Throwable defaults when no message was supplied
    emit_aarch64_load_eval_arg(module, emitter, 0);
    emit_aarch64_cast_eval_arg(emitter, &PhpType::Str);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for message initialization
    emitter.instruction("str x1, [x9, #8]");                                    // store the message pointer in the compact Throwable payload
    emitter.instruction("str x2, [x9, #16]");                                   // store the message length in the compact Throwable payload
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload constructor argc before testing the code argument
    emitter.instruction("cmp x9, #1");                                          // did the eval call pass a code argument?
    emitter.instruction(&format!("b.le {}", success_label));                    // keep code zero when only the message was supplied
    emit_aarch64_load_eval_arg(module, emitter, 1);
    emit_aarch64_cast_eval_arg(emitter, &PhpType::Int);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for code initialization
    emitter.instruction("str x0, [x9, #24]");                                   // store the integer exception code
    emitter.instruction(&format!("b {}", success_label));                       // builtin Throwable construction completed
}

/// Initializes the compact Throwable payload for eval-created x86_64 builtin exceptions.
fn emit_x86_64_builtin_throwable_constructor_body(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    success_label: &str,
) {
    emit_x86_64_validate_builtin_throwable_arg_count(module, emitter, fail_label);
    emit_x86_64_default_builtin_throwable_fields(emitter);
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload constructor argc before testing the message argument
    emitter.instruction("cmp r11, 0");                                          // did the eval call pass a message argument?
    emitter.instruction(&format!("je {}", success_label));                      // keep the empty Throwable defaults when no message was supplied
    emit_x86_64_load_eval_arg(module, emitter, 0);
    emit_x86_64_cast_eval_arg(emitter, &PhpType::Str);
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for message initialization
    emitter.instruction("mov QWORD PTR [r11 + 8], rax");                        // store the message pointer in the compact Throwable payload
    emitter.instruction("mov QWORD PTR [r11 + 16], rdx");                       // store the message length in the compact Throwable payload
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload constructor argc before testing the code argument
    emitter.instruction("cmp r11, 1");                                          // did the eval call pass a code argument?
    emitter.instruction(&format!("jle {}", success_label));                     // keep code zero when only the message was supplied
    emit_x86_64_load_eval_arg(module, emitter, 1);
    emit_x86_64_cast_eval_arg(emitter, &PhpType::Int);
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for code initialization
    emitter.instruction("mov QWORD PTR [r11 + 24], rax");                       // store the integer exception code
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
    emitter.instruction("str x0, [sp, #32]");                                   // save constructor argc for message/code initialization
    emitter.instruction("cmp x0, #2");                                          // compact Throwable initialization supports message and code arguments
    emitter.instruction(&format!("b.gt {}", fail_label));                       // reject unsupported previous-Throwable arguments from eval
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
    emitter.instruction("cmp rax, 2");                                          // compact Throwable initialization supports message and code arguments
    emitter.instruction(&format!("jg {}", fail_label));                         // reject unsupported previous-Throwable arguments from eval
}

/// Writes ARM64 empty-message and zero-code defaults into the compact Throwable payload.
fn emit_aarch64_default_builtin_throwable_fields(emitter: &mut Emitter) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for default initialization
    emitter.instruction("str xzr, [x9, #8]");                                   // default the message pointer to an empty string payload
    emitter.instruction("str xzr, [x9, #16]");                                  // default the message length to zero
    emitter.instruction("str xzr, [x9, #24]");                                  // default the exception code to zero
}

/// Writes x86_64 empty-message and zero-code defaults into the compact Throwable payload.
fn emit_x86_64_default_builtin_throwable_fields(emitter: &mut Emitter) {
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for default initialization
    emitter.instruction("mov QWORD PTR [r11 + 8], 0");                          // default the message pointer to an empty string payload
    emitter.instruction("mov QWORD PTR [r11 + 16], 0");                         // default the message length to zero
    emitter.instruction("mov QWORD PTR [r11 + 24], 0");                         // default the exception code to zero
}

/// Emits ARM64 class-id dispatch for supported constructor bodies.
fn emit_aarch64_constructor_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalConstructorSlot],
    fail_label: &str,
    success_label: &str,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let next_label = format!("__elephc_eval_constructor_next_{}", class_id);
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the unboxed object pointer before this class test
        emitter.instruction("ldr x9, [x9]");                                    // load the receiver class id for constructor dispatch
        abi::emit_load_int_immediate(emitter, "x10", class_id as i64);
        emitter.instruction("cmp x9, x10");                                     // compare receiver class id against this constructor class
        emitter.instruction(&format!("b.ne {}", next_label));                   // try the next constructor class when ids differ
        for slot in class_slots {
            emit_aarch64_constructor_body(module, emitter, slot, fail_label, success_label);
        }
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-id dispatch for supported constructor bodies.
fn emit_x86_64_constructor_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalConstructorSlot],
    fail_label: &str,
    success_label: &str,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let next_label = format!("__elephc_eval_constructor_next_{}_x", class_id);
        emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                   // reload the unboxed object pointer before this class test
        emitter.instruction("mov r11, QWORD PTR [r11]");                        // load the receiver class id for constructor dispatch
        abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
        emitter.instruction("cmp r11, r10");                                    // compare receiver class id against this constructor class
        emitter.instruction(&format!("jne {}", next_label));                    // try the next constructor class when ids differ
        for slot in class_slots {
            emit_x86_64_constructor_body(module, emitter, slot, fail_label, success_label);
        }
        emitter.label(&next_label);
    }
}

/// Emits one ARM64 constructor body or failure branch for an unsupported constructor.
fn emit_aarch64_constructor_body(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalConstructorSlot,
    fail_label: &str,
    success_label: &str,
) {
    if !slot.supported {
        emitter.instruction(&format!("b {}", fail_label));                      // reject constructors outside this bridge's supported ABI slice
        return;
    }
    emit_aarch64_validate_constructor_arg_count(module, emitter, slot, fail_label);
    let overflow_bytes = emit_aarch64_prepare_constructor_args(module, emitter, slot);
    let caller_stack_pad_bytes = abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(emitter, caller_stack_pad_bytes);
    abi::emit_call_label(emitter, &method_symbol(&slot.impl_class, "__construct"));
    abi::emit_release_temporary_stack(emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(emitter, overflow_bytes);
    emitter.instruction(&format!("b {}", success_label));                       // constructor returned normally
}

/// Emits one x86_64 constructor body or failure branch for an unsupported constructor.
fn emit_x86_64_constructor_body(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalConstructorSlot,
    fail_label: &str,
    success_label: &str,
) {
    if !slot.supported {
        emitter.instruction(&format!("jmp {}", fail_label));                    // reject constructors outside this bridge's supported ABI slice
        return;
    }
    emit_x86_64_validate_constructor_arg_count(module, emitter, slot, fail_label);
    let overflow_bytes = emit_x86_64_prepare_constructor_args(module, emitter, slot);
    let caller_stack_pad_bytes = abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(emitter, caller_stack_pad_bytes);
    abi::emit_call_label(emitter, &method_symbol(&slot.impl_class, "__construct"));
    abi::emit_release_temporary_stack(emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(emitter, overflow_bytes);
    emitter.instruction(&format!("jmp {}", success_label));                     // constructor returned normally
}

/// Emits ARM64 arity validation for one constructor body.
fn emit_aarch64_validate_constructor_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalConstructorSlot,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the eval argument array for arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    abi::emit_load_int_immediate(emitter, "x9", slot.params.len() as i64);
    emitter.instruction("cmp x0, x9");                                          // compare supplied eval argument count with the constructor signature
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject constructor dispatch when arity differs
}

/// Emits x86_64 arity validation for one constructor body.
fn emit_x86_64_validate_constructor_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalConstructorSlot,
    fail_label: &str,
) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the eval argument array for arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    abi::emit_load_int_immediate(emitter, "r10", slot.params.len() as i64);
    emitter.instruction("cmp rax, r10");                                        // compare supplied eval argument count with the constructor signature
    emitter.instruction(&format!("jne {}", fail_label));                        // reject constructor dispatch when arity differs
}

/// Prepares ARM64 constructor ABI registers for the supported argument shapes.
fn emit_aarch64_prepare_constructor_args(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalConstructorSlot,
) -> usize {
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the unboxed receiver as the first constructor argument
    abi::emit_push_result_value(emitter, &receiver_ty);
    for (index, param_ty) in slot.params.iter().enumerate() {
        emit_aarch64_load_eval_arg(module, emitter, index);
        emit_aarch64_cast_eval_arg(emitter, param_ty);
        abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
    }
    materialize_constructor_args(module, emitter, &receiver_ty, &slot.params)
}

/// Prepares x86_64 constructor ABI registers for the supported argument shapes.
fn emit_x86_64_prepare_constructor_args(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalConstructorSlot,
) -> usize {
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the unboxed receiver as the first constructor argument
    abi::emit_push_result_value(emitter, &receiver_ty);
    for (index, param_ty) in slot.params.iter().enumerate() {
        emit_x86_64_load_eval_arg(module, emitter, index);
        emit_x86_64_cast_eval_arg(emitter, param_ty);
        abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
    }
    materialize_constructor_args(module, emitter, &receiver_ty, &slot.params)
}

/// Materializes the pushed receiver and eval arguments into the target method ABI.
fn materialize_constructor_args(
    module: &Module,
    emitter: &mut Emitter,
    receiver_ty: &PhpType,
    params: &[PhpType],
) -> usize {
    let mut arg_types = Vec::with_capacity(params.len() + 1);
    arg_types.push(receiver_ty.clone());
    arg_types.extend(params.iter().map(|param| param.codegen_repr()));
    let assignments = abi::build_outgoing_arg_assignments_for_target(module.target, &arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

/// Loads one eval argument into an ARM64 spill slot as a boxed Mixed cell.
fn emit_aarch64_load_eval_arg(module: &Module, emitter: &mut Emitter, index: usize) {
    let value_int_symbol = module.target.extern_symbol("__elephc_eval_value_int");
    let array_get_symbol = module.target.extern_symbol("__elephc_eval_value_array_get");
    abi::emit_load_int_immediate(emitter, "x0", index as i64);
    abi::emit_call_label(emitter, &value_int_symbol);
    emitter.instruction("str x0, [x29, #-16]");                                 // save the boxed index while loading from the argument array
    emitter.instruction("ldr x1, [x29, #-16]");                                 // pass the boxed index to the eval array reader
    emitter.instruction("ldr x0, [x29, #-24]");                                 // pass the eval argument array to the reader
    abi::emit_call_label(emitter, &array_get_symbol);
    emitter.instruction("str x0, [x29, #-16]");                                 // save the boxed eval argument for coercion
}

/// Loads one eval argument into an x86_64 spill slot as a boxed Mixed cell.
fn emit_x86_64_load_eval_arg(module: &Module, emitter: &mut Emitter, index: usize) {
    let value_int_symbol = module.target.extern_symbol("__elephc_eval_value_int");
    let array_get_symbol = module.target.extern_symbol("__elephc_eval_value_array_get");
    abi::emit_load_int_immediate(emitter, "rdi", index as i64);
    abi::emit_call_label(emitter, &value_int_symbol);
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the boxed index while loading from the argument array
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the boxed index to the eval array reader
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the eval argument array to the reader
    abi::emit_call_label(emitter, &array_get_symbol);
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the boxed eval argument for coercion
}

/// Casts one boxed eval argument into ARM64 result registers for temporary staging.
fn emit_aarch64_cast_eval_arg(emitter: &mut Emitter, param_ty: &PhpType) {
    match param_ty.codegen_repr() {
        PhpType::Int => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for integer coercion
            emitter.instruction("bl __rt_mixed_cast_int");                      // coerce the eval argument to a PHP int
        }
        PhpType::Bool => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for boolean coercion
            emitter.instruction("bl __rt_mixed_cast_bool");                     // coerce the eval argument to a PHP bool
        }
        PhpType::Float => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for float coercion
            emitter.instruction("bl __rt_mixed_cast_float");                    // coerce the eval argument to a PHP float in d0
        }
        PhpType::Str => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for string coercion
            emitter.instruction("bl __rt_mixed_cast_string");                   // coerce the eval argument to a PHP string pair in x1/x2
        }
        PhpType::Mixed => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for a Mixed constructor parameter
        }
        _ => {}
    }
}

/// Casts one boxed eval argument into x86_64 result registers for temporary staging.
fn emit_x86_64_cast_eval_arg(emitter: &mut Emitter, param_ty: &PhpType) {
    match param_ty.codegen_repr() {
        PhpType::Int => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for integer coercion
            emitter.instruction("call __rt_mixed_cast_int");                    // coerce the eval argument to a PHP int
        }
        PhpType::Bool => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for boolean coercion
            emitter.instruction("call __rt_mixed_cast_bool");                   // coerce the eval argument to a PHP bool
        }
        PhpType::Float => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for float coercion
            emitter.instruction("call __rt_mixed_cast_float");                  // coerce the eval argument to a PHP float in xmm0
        }
        PhpType::Str => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for string coercion
            emitter.instruction("call __rt_mixed_cast_string");                 // coerce the eval argument to a PHP string pair
        }
        PhpType::Mixed => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for a Mixed constructor parameter
        }
        _ => {}
    }
}

/// Groups constructor slots by class id while preserving sorted class order.
fn grouped_slots(slots: &[EvalConstructorSlot]) -> BTreeMap<u64, Vec<&EvalConstructorSlot>> {
    let mut grouped = BTreeMap::new();
    for slot in slots {
        grouped
            .entry(slot.class_id)
            .or_insert_with(Vec::new)
            .push(slot);
    }
    grouped
}

/// Emits a platform-C global label for a user assembly helper.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    emitter.label_global(&module.target.extern_symbol(name));
}
