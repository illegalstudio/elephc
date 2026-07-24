//! Purpose:
//! Emits user-assembly helpers that let libelephc-magician run native
//! constructors after allocating AOT objects by class name.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - The cacheable runtime object can allocate by name, but only user assembly
//!   knows constructor symbols and parameter ABI shapes.
//! - Classes without constructors are treated as successful no-ops, matching PHP.
//! - Constructors are bridged for scalar/Mixed/array/object arguments, including
//!   generated variadic array slots and supported scalar/Mixed by-reference parameters.
//! - Non-public constructors are accepted when the active eval class scope
//!   satisfies PHP visibility.

use std::collections::BTreeMap;

use crate::codegen::abi;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Function, LocalKind, Module};
use crate::names::{method_symbol, php_symbol_key};
use crate::parser::ast::{ExprKind, Visibility};
use crate::types::{ClassInfo, FunctionSig, PhpType};

use super::eval_ref_arg_helpers::{
    EvalRefArgSlot, eval_abi_param_types_for_refs, eval_arg_temp_slot_size,
    eval_normalized_ref_params, eval_ref_arg_slots, eval_signature_ref_params_supported,
    emit_aarch64_write_back_ref_args, emit_x86_64_write_back_ref_args,
};
use super::eval_callable_helpers::EvalCallableDescriptorSupport;

const BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES: &[&str] = &[
    "Error",
    "TypeError",
    "ValueError",
    "UnhandledMatchError",
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
    "ReflectionException",
    "JsonException",
    "FiberError",
];
const CONSTRUCTOR_HELPER_BASE_FRAME_SIZE: usize = 80;
const CONSTRUCTOR_HELPER_HANDLER_OFFSET: usize = CONSTRUCTOR_HELPER_BASE_FRAME_SIZE;
const CONSTRUCTOR_HELPER_FRAME_SIZE: usize =
    CONSTRUCTOR_HELPER_BASE_FRAME_SIZE + TRY_HANDLER_SLOT_SIZE;
const X86_64_CONSTRUCTOR_CONTEXT_FRAME_OFFSET: usize = 64;

/// Constructor metadata needed by the eval constructor bridge.
#[derive(Clone)]
struct EvalConstructorSlot {
    class_id: u64,
    class_name: String,
    impl_class: String,
    visibility: Visibility,
    allowed_scopes: Vec<String>,
    params: Vec<PhpType>,
    ref_params: Vec<bool>,
    supported: bool,
    runtime_helper: Option<&'static str>,
    zero_default_first_arg: bool,
}

/// Emits eval constructor helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_constructor_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    callable_support: &EvalCallableDescriptorSupport,
) {
    if !module_uses_eval(module) {
        return;
    }
    let slots = collect_eval_constructor_slots(module);
    let builtin_throwable_class_ids = collect_builtin_throwable_constructor_class_ids(module);
    emit_constructor_helper(
        module,
        emitter,
        data,
        &slots,
        &builtin_throwable_class_ids,
        callable_support,
    );
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

/// Collects AOT and runtime-backed constructors in stable class-id order.
fn collect_eval_constructor_slots(module: &Module) -> Vec<EvalConstructorSlot> {
    let emitted_methods = super::eir_class_method_keys(module);
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_constructor_slot(module, class_name, class_info, &emitted_methods, &mut slots);
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

/// Adds one constructor slot for a class when the constructor has emitted code or a runtime helper.
fn collect_class_constructor_slot(
    module: &Module,
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
    let runtime_helper = eval_runtime_backed_constructor_helper(class_name, &method_key);
    if runtime_helper.is_none()
        && !emitted_methods.contains(&(impl_class.to_string(), method_key.clone(), false))
    {
        return;
    }
    let visibility = constructor_visibility(class_info, &method_key);
    let supported =
        constructor_visibility_supported(visibility) && constructor_signature_supported(sig);
    let params = if supported {
        sig.params.iter().map(|(_, ty)| ty.codegen_repr()).collect()
    } else {
        Vec::new()
    };
    let ref_params = if supported {
        eval_normalized_ref_params(sig.params.len(), &sig.ref_params)
    } else {
        Vec::new()
    };
    slots.push(EvalConstructorSlot {
        class_id: class_info.class_id,
        class_name: class_name.to_string(),
        impl_class: impl_class.to_string(),
        visibility: visibility.clone(),
        allowed_scopes: visibility_scope_names(module, impl_class, visibility),
        params,
        ref_params,
        supported,
        runtime_helper,
        zero_default_first_arg: constructor_uses_zero_default_first_arg(
            class_name,
            &method_key,
            sig,
            runtime_helper,
        ),
    });
}

/// Returns a normal-ABI runtime helper for builtin constructors that eval can bridge.
fn eval_runtime_backed_constructor_helper(
    class_name: &str,
    method_key: &str,
) -> Option<&'static str> {
    if class_name.trim_start_matches('\\') != "SplFixedArray" || method_key != "__construct" {
        return None;
    }
    IntrinsicCall::instance_method(class_name, method_key)?.runtime_helper()
}

/// Returns true when the first constructor parameter has PHP's builtin zero default.
fn constructor_uses_zero_default_first_arg(
    class_name: &str,
    method_key: &str,
    sig: &FunctionSig,
    runtime_helper: Option<&'static str>,
) -> bool {
    runtime_helper.is_some()
        && class_name.trim_start_matches('\\') == "SplFixedArray"
        && method_key == "__construct"
        && matches!(sig.params.first().map(|(_, ty)| ty.codegen_repr()), Some(PhpType::Int))
        && matches!(
            sig.defaults.first().and_then(Option::as_ref).map(|expr| &expr.kind),
            Some(ExprKind::IntLiteral(0))
        )
}

/// Returns the declared constructor visibility, defaulting to public metadata.
fn constructor_visibility<'a>(class_info: &'a ClassInfo, method_key: &str) -> &'a Visibility {
    class_info
        .method_visibilities
        .get(method_key)
        .unwrap_or(&Visibility::Public)
}

/// Returns true when the eval constructor bridge can enforce this visibility.
fn constructor_visibility_supported(visibility: &Visibility) -> bool {
    matches!(
        visibility,
        Visibility::Public | Visibility::Protected | Visibility::Private
    )
}

/// Returns true for constructor signatures supported by this eval bridge slice.
fn constructor_signature_supported(sig: &FunctionSig) -> bool {
    eval_signature_ref_params_supported(sig)
        && sig
            .params
            .iter()
            .all(|(_, ty)| constructor_param_supported(ty))
}

/// Returns true for one constructor argument type supported by the bridge.
fn constructor_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}

/// Emits `__elephc_eval_value_construct_object(Mixed*, MixedArray*, scope, scope_len, ctx) -> bool`.
fn emit_constructor_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalConstructorSlot],
    builtin_throwable_class_ids: &[u64],
    callable_support: &EvalCallableDescriptorSupport,
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user constructor call ---");
    label_c_global(module, emitter, "__elephc_eval_value_construct_object");
    match module.target.arch {
        Arch::AArch64 => {
            emit_constructor_aarch64(
                module,
                emitter,
                data,
                slots,
                builtin_throwable_class_ids,
                callable_support,
            )
        }
        Arch::X86_64 => {
            emit_constructor_x86_64(
                module,
                emitter,
                data,
                slots,
                builtin_throwable_class_ids,
                callable_support,
            )
        }
    }
    emit_take_pending_throwable_helper(module, emitter);
}

/// Emits the ARM64 constructor helper body.
fn emit_constructor_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalConstructorSlot],
    builtin_throwable_class_ids: &[u64],
    callable_support: &EvalCallableDescriptorSupport,
) {
    let success_label = "__elephc_eval_value_construct_success";
    let fail_label = "__elephc_eval_value_construct_fail";
    let done_label = "__elephc_eval_value_construct_done";
    emitter.instruction(&format!("sub sp, sp, #{}", CONSTRUCTOR_HELPER_FRAME_SIZE)); //reserve helper frame plus a boundary exception handler
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x2, [sp, #0]");                                    // save the active eval class-scope pointer
    emitter.instruction("str x3, [sp, #8]");                                    // save the active eval class-scope length
    emitter.instruction("str x1, [sp, #24]");                                   // save the boxed eval argument array
    emitter.instruction("str x4, [sp, #64]");                                   // save the active eval context for callable descriptors
    emitter.instruction(&format!("cbz x0, {}", success_label));                 // a null object pointer means there is nothing to construct
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", success_label));                    // non-object values have no constructor to run
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for constructor calls
    emit_aarch64_builtin_throwable_constructor_dispatch(
        module,
        emitter,
        data,
        builtin_throwable_class_ids,
        fail_label,
        success_label,
        callable_support,
    );
    emit_aarch64_constructor_dispatch(
        module,
        emitter,
        data,
        slots,
        fail_label,
        success_label,
        callable_support,
    );
    emitter.instruction(&format!("b {}", success_label));                       // no constructor metadata matched this class id
    emitter.label(fail_label);
    emitter.instruction("mov x0, #0");                                          // report constructor dispatch failure to Rust
    emitter.instruction(&format!("b {}", done_label));                          // skip the success result after a failure
    emitter.label(success_label);
    emitter.instruction("mov x0, #1");                                          // report successful construction or no-op
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction(&format!("add sp, sp, #{}", CONSTRUCTOR_HELPER_FRAME_SIZE)); //release the constructor helper frame and boundary handler
    emitter.instruction("ret");                                                 // return the constructor status flag to Rust
}

/// Emits the x86_64 constructor helper body.
fn emit_constructor_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalConstructorSlot],
    builtin_throwable_class_ids: &[u64],
    callable_support: &EvalCallableDescriptorSupport,
) {
    let success_label = "__elephc_eval_value_construct_success_x";
    let fail_label = "__elephc_eval_value_construct_fail_x";
    let done_label = "__elephc_eval_value_construct_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction(&format!("sub rsp, {}", CONSTRUCTOR_HELPER_FRAME_SIZE)); //reserve aligned slots plus a boundary exception handler
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the active eval class-scope length
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the boxed eval argument array
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save the active eval context for callable descriptors
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
        data,
        builtin_throwable_class_ids,
        fail_label,
        success_label,
        callable_support,
    );
    emit_x86_64_constructor_dispatch(
        module,
        emitter,
        data,
        slots,
        fail_label,
        success_label,
        callable_support,
    );
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

/// Emits an ARM64 boundary handler so native constructor throws return to magician.
fn emit_aarch64_constructor_exception_boundary_push(emitter: &mut Emitter, escape_label: &str) {
    let handler_offset = CONSTRUCTOR_HELPER_HANDLER_OFFSET - 48;
    emitter.comment("push eval constructor exception boundary");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("str x10, [x29, #{}]", handler_offset));       // save the previous native exception-handler head
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("str x10, [x29, #{}]", handler_offset + 8));   // preserve the caller activation frame across constructor unwinding
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!(
        "str x10, [x29, #{}]",
        handler_offset + TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));                                                                         // save diagnostic suppression depth for restoration
    emitter.instruction(&format!("add x10, x29, #{}", handler_offset));         // compute the boundary handler record address
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!(
        "add x0, x29, #{}",
        handler_offset + TRY_HANDLER_JMP_BUF_OFFSET
    ));                                                                         // pass the boundary jmp_buf to setjmp
    emitter.bl_c("setjmp");                                                     // snapshot the bridge stack before entering native constructors
    emitter.instruction(&format!("cbnz x0, {}", escape_label));                 // non-zero setjmp result means a constructor Throwable escaped
}

/// Emits an ARM64 boundary pop that preserves the constructor status in x0.
fn emit_aarch64_constructor_exception_boundary_pop(emitter: &mut Emitter) {
    let handler_offset = CONSTRUCTOR_HELPER_HANDLER_OFFSET - 48;
    emitter.comment("pop eval constructor exception boundary");
    emitter.instruction(&format!("ldr x10, [x29, #{}]", handler_offset));       // reload the previous native exception-handler head
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!(
        "ldr x10, [x29, #{}]",
        handler_offset + TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));                                                                         // reload the saved diagnostic suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
}

/// Emits an x86_64 boundary handler so native constructor throws return to magician.
fn emit_x86_64_constructor_exception_boundary_push(emitter: &mut Emitter, escape_label: &str) {
    let handler_base = CONSTRUCTOR_HELPER_FRAME_SIZE;
    emitter.comment("push eval constructor exception boundary");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", handler_base)); //save the previous native exception-handler head
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", handler_base - 8)); //preserve the caller activation frame across constructor unwinding
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!(
        "mov QWORD PTR [rbp - {}], r10",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));                                                                          // save diagnostic suppression depth for restoration
    emitter.instruction(&format!("lea r10, [rbp - {}]", handler_base));         // compute the boundary handler record address
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!(
        "lea rdi, [rbp - {}]",
        handler_base - TRY_HANDLER_JMP_BUF_OFFSET
    ));                                                                          // pass the boundary jmp_buf to setjmp
    emitter.bl_c("setjmp");                                                      // snapshot the bridge stack before entering native constructors
    emitter.instruction("test eax, eax");                                       // did control arrive through longjmp?
    emitter.instruction(&format!("jne {}", escape_label));                      // non-zero setjmp result means a constructor Throwable escaped
}

/// Emits an x86_64 boundary pop that preserves the constructor status in rax.
fn emit_x86_64_constructor_exception_boundary_pop(emitter: &mut Emitter) {
    let handler_base = CONSTRUCTOR_HELPER_FRAME_SIZE;
    emitter.comment("pop eval constructor exception boundary");
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", handler_base)); //reload the previous native exception-handler head
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rbp - {}]",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));                                                                          // reload the saved diagnostic suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
}

/// Emits a C helper that transfers `_exc_value` ownership to magician.
fn emit_take_pending_throwable_helper(module: &Module, emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- eval bridge: take pending throwable ---");
    label_c_global(module, emitter, "__elephc_eval_value_take_pending_throwable");
    match module.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("ret");                                         // return the pending Throwable pointer to magician
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("ret");                                         // return the pending Throwable pointer to magician
        }
    }
}

/// Emits ARM64 dispatch for compact builtin Throwable constructors.
fn emit_aarch64_builtin_throwable_constructor_dispatch(
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
fn emit_x86_64_builtin_throwable_constructor_dispatch(
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
    emit_aarch64_cast_eval_arg(
        module,
        emitter,
        &PhpType::Str,
        "__elephc_eval_builtin_throwable_message",
        fail_label,
        data,
        callable_support,
    );
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
    emit_x86_64_cast_eval_arg(
        module,
        emitter,
        &PhpType::Str,
        "__elephc_eval_builtin_throwable_message_x",
        fail_label,
        data,
        callable_support,
    );
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

/// Stores the nullable third Throwable constructor argument on ARM64.
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
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object after retaining previous
    emitter.instruction("str x0, [x9, #40]");                                   // store the retained previous object pointer
    emitter.instruction(&format!("b {}", success_label));                       // builtin Throwable construction completed
}

/// Stores the nullable third Throwable constructor argument on x86_64.
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
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object after retaining previous
    emitter.instruction("mov QWORD PTR [r11 + 40], rax");                       // store the retained previous object pointer
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

/// Writes ARM64 empty-message, zero-code, and null-previous Throwable defaults.
fn emit_aarch64_default_builtin_throwable_fields(emitter: &mut Emitter) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for default initialization
    emitter.instruction("str xzr, [x9, #8]");                                   // default the message pointer to an empty string payload
    emitter.instruction("str xzr, [x9, #16]");                                  // default the message length to zero
    emitter.instruction("str xzr, [x9, #24]");                                  // default the exception code to zero
    emitter.instruction("str xzr, [x9, #40]");                                  // default the previous Throwable pointer to null
}

/// Writes x86_64 empty-message, zero-code, and null-previous Throwable defaults.
fn emit_x86_64_default_builtin_throwable_fields(emitter: &mut Emitter) {
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for default initialization
    emitter.instruction("mov QWORD PTR [r11 + 8], 0");                          // default the message pointer to an empty string payload
    emitter.instruction("mov QWORD PTR [r11 + 16], 0");                         // default the message length to zero
    emitter.instruction("mov QWORD PTR [r11 + 24], 0");                         // default the exception code to zero
    emitter.instruction("mov QWORD PTR [r11 + 40], 0");                         // default the previous Throwable pointer to null
}

/// Emits ARM64 class-id dispatch for supported constructor bodies.
fn emit_aarch64_constructor_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalConstructorSlot],
    fail_label: &str,
    success_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let next_label = format!("__elephc_eval_constructor_next_{}", class_id);
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the unboxed object pointer before this class test
        emitter.instruction("ldr x9, [x9]");                                    // load the receiver class id for constructor dispatch
        abi::emit_load_int_immediate(emitter, "x10", class_id as i64);
        emitter.instruction("cmp x9, x10");                                     // compare receiver class id against this constructor class
        emitter.instruction(&format!("b.ne {}", next_label));                   // try the next constructor class when ids differ
        for slot in class_slots {
            emit_aarch64_constructor_body(
                module,
                emitter,
                data,
                slot,
                fail_label,
                success_label,
                callable_support,
            );
        }
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-id dispatch for supported constructor bodies.
fn emit_x86_64_constructor_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalConstructorSlot],
    fail_label: &str,
    success_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let next_label = format!("__elephc_eval_constructor_next_{}_x", class_id);
        emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                   // reload the unboxed object pointer before this class test
        emitter.instruction("mov r11, QWORD PTR [r11]");                        // load the receiver class id for constructor dispatch
        abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
        emitter.instruction("cmp r11, r10");                                    // compare receiver class id against this constructor class
        emitter.instruction(&format!("jne {}", next_label));                    // try the next constructor class when ids differ
        for slot in class_slots {
            emit_x86_64_constructor_body(
                module,
                emitter,
                data,
                slot,
                fail_label,
                success_label,
                callable_support,
            );
        }
        emitter.label(&next_label);
    }
}

/// Emits one ARM64 constructor body or failure branch for an unsupported constructor.
fn emit_aarch64_constructor_body(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalConstructorSlot,
    fail_label: &str,
    success_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    if !slot.supported {
        emitter.instruction(&format!("b {}", fail_label));                      // reject constructors outside this bridge's supported ABI slice
        return;
    }
    if !matches!(slot.visibility, Visibility::Public) {
        let scope_ok_label = constructor_scope_ok_label(module, slot);
        emit_aarch64_constructor_scope_check(emitter, data, slot, &scope_ok_label, fail_label);
        emitter.label(&scope_ok_label);
    }
    emit_aarch64_validate_constructor_arg_count(module, emitter, slot, fail_label);
    let body_label = constructor_body_label(module, slot);
    let prep_fail_label = format!("{}_prep_fail", body_label);
    let (arg_temp_bytes, ref_slots) =
        emit_aarch64_prepare_constructor_args(
            module,
            emitter,
            data,
            slot,
            &prep_fail_label,
            callable_support,
        );
    let escape_label = format!("{}_escape", body_label);
    emit_aarch64_constructor_exception_boundary_push(emitter, &escape_label);
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    let overflow_bytes =
        materialize_constructor_args(module, emitter, &receiver_ty, &slot.params, &slot.ref_params);
    let caller_stack_pad_bytes = abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(emitter, caller_stack_pad_bytes);
    let callee = slot
        .runtime_helper
        .map(str::to_string)
        .unwrap_or_else(|| method_symbol(&slot.impl_class, "__construct"));
    abi::emit_call_label(emitter, &callee);
    abi::emit_release_temporary_stack(emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(emitter, overflow_bytes);
    emit_aarch64_write_back_ref_args(
        emitter,
        &ref_slots,
        0,
        &body_label,
    );
    abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
    emit_aarch64_constructor_exception_boundary_pop(emitter);
    emitter.instruction(&format!("b {}", success_label));                       // constructor returned normally
    emitter.label(&escape_label);
    abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
    let escape_writeback_label = format!("{}_throw", body_label);
    emit_aarch64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
    abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
    emit_aarch64_constructor_exception_boundary_pop(emitter);
    emitter.instruction(&format!("b {}", fail_label));                          // return failure after preserving by-reference writes
    emitter.label(&prep_fail_label);
    emit_aarch64_constructor_prep_fail_cleanup(emitter, fail_label);
}

/// Emits one x86_64 constructor body or failure branch for an unsupported constructor.
fn emit_x86_64_constructor_body(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalConstructorSlot,
    fail_label: &str,
    success_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    if !slot.supported {
        emitter.instruction(&format!("jmp {}", fail_label));                    // reject constructors outside this bridge's supported ABI slice
        return;
    }
    if !matches!(slot.visibility, Visibility::Public) {
        let scope_ok_label = constructor_scope_ok_label(module, slot);
        emit_x86_64_constructor_scope_check(emitter, data, slot, &scope_ok_label, fail_label);
        emitter.label(&scope_ok_label);
    }
    emit_x86_64_validate_constructor_arg_count(module, emitter, slot, fail_label);
    let body_label = constructor_body_label(module, slot);
    let prep_fail_label = format!("{}_prep_fail_x", body_label);
    let (arg_temp_bytes, ref_slots) =
        emit_x86_64_prepare_constructor_args(
            module,
            emitter,
            data,
            slot,
            &prep_fail_label,
            callable_support,
        );
    let escape_label = format!("{}_escape_x", body_label);
    emit_x86_64_constructor_exception_boundary_push(emitter, &escape_label);
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    let overflow_bytes =
        materialize_constructor_args(module, emitter, &receiver_ty, &slot.params, &slot.ref_params);
    let caller_stack_pad_bytes = abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(emitter, caller_stack_pad_bytes);
    let callee = slot
        .runtime_helper
        .map(str::to_string)
        .unwrap_or_else(|| method_symbol(&slot.impl_class, "__construct"));
    abi::emit_call_label(emitter, &callee);
    abi::emit_release_temporary_stack(emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(emitter, overflow_bytes);
    emit_x86_64_write_back_ref_args(
        emitter,
        &ref_slots,
        0,
        &body_label,
    );
    abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
    emit_x86_64_constructor_exception_boundary_pop(emitter);
    emitter.instruction(&format!("jmp {}", success_label));                     // constructor returned normally
    emitter.label(&escape_label);
    abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
    let escape_writeback_label = format!("{}_throw", body_label);
    emit_x86_64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
    abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
    emit_x86_64_constructor_exception_boundary_pop(emitter);
    emitter.instruction(&format!("jmp {}", fail_label));                        // return failure after preserving by-reference writes
    emitter.label(&prep_fail_label);
    emit_x86_64_constructor_prep_fail_cleanup(emitter, fail_label);
}

/// Restores an ARM64 constructor-helper frame before reporting an argument-prep fatal.
fn emit_aarch64_constructor_prep_fail_cleanup(emitter: &mut Emitter, fail_label: &str) {
    emitter.instruction("sub sp, x29, #48");                                    // restore the helper frame base after argument staging failed
    emitter.instruction(&format!("b {}", fail_label));                          // report the argument-prep failure through the shared fail path
}

/// Restores an x86_64 constructor-helper frame before reporting an argument-prep fatal.
fn emit_x86_64_constructor_prep_fail_cleanup(emitter: &mut Emitter, fail_label: &str) {
    emitter.instruction("mov rsp, rbp");                                        // restore the helper frame base after argument staging failed
    emitter.instruction(&format!("jmp {}", fail_label));                        // report the argument-prep failure through the shared fail path
}

/// Emits ARM64 visibility checks for a protected/private constructor bridge hit.
fn emit_aarch64_constructor_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalConstructorSlot,
    success_label: &str,
    fail_label: &str,
) {
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the active eval class-scope pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the active eval class-scope length
    emitter.instruction(&format!("cbz x1, {}", fail_label));                    // reject scoped constructor access outside a class scope
    for scope_name in &slot.allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction("ldr x1, [sp, #0]");                                // reload the active eval class-scope pointer
        emitter.instruction("ldr x2, [sp, #8]");                                // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "x3", &label);
        abi::emit_load_int_immediate(emitter, "x4", len as i64);
        emitter.instruction("bl __rt_strcasecmp");                              // compare current eval scope with an allowed class
        emitter.instruction(&format!("cbz x0, {}", success_label));             // run the constructor when scoped visibility is satisfied
    }
    emitter.instruction(&format!("b {}", fail_label));                          // reject constructor access from unrelated classes
}

/// Emits x86_64 visibility checks for a protected/private constructor bridge hit.
fn emit_x86_64_constructor_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalConstructorSlot,
    success_label: &str,
    fail_label: &str,
) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the active eval class-scope pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the active eval class-scope length
    emitter.instruction("test rdi, rdi");                                       // check whether eval is executing inside a class scope
    emitter.instruction(&format!("jz {}", fail_label));                         // reject scoped constructor access outside a class scope
    for scope_name in &slot.allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                   // reload the active eval class-scope pointer
        emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                   // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "rdx", &label);
        abi::emit_load_int_immediate(emitter, "rcx", len as i64);
        emitter.instruction("call __rt_strcasecmp");                            // compare current eval scope with an allowed class
        emitter.instruction("test rax, rax");                                   // check whether the current scope matched
        emitter.instruction(&format!("je {}", success_label));                  // run the constructor when scoped visibility is satisfied
    }
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject constructor access from unrelated classes
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
    emitter.instruction("str x0, [sp, #32]");                                   // save the supplied constructor argument count
    abi::emit_load_int_immediate(emitter, "x9", slot.params.len() as i64);
    emitter.instruction("cmp x0, x9");                                          // compare supplied eval argument count with the constructor signature
    if slot.zero_default_first_arg {
        let ok_label = format!("{}_argc_ok", constructor_body_label(module, slot));
        emitter.instruction(&format!("b.eq {}", ok_label));                     // explicit argument count matches the constructor signature
        emitter.instruction("cmp x0, #0");                                      // did eval omit the optional builtin constructor argument?
        emitter.instruction(&format!("b.eq {}", ok_label));                     // accept the builtin zero-default constructor call
        emitter.instruction(&format!("b {}", fail_label));                      // reject constructor dispatch when arity differs
        emitter.label(&ok_label);
    } else {
        emitter.instruction(&format!("b.ne {}", fail_label));                   // reject constructor dispatch when arity differs
    }
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
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the supplied constructor argument count
    abi::emit_load_int_immediate(emitter, "r10", slot.params.len() as i64);
    emitter.instruction("cmp rax, r10");                                        // compare supplied eval argument count with the constructor signature
    if slot.zero_default_first_arg {
        let ok_label = format!("{}_argc_ok", constructor_body_label(module, slot));
        emitter.instruction(&format!("je {}", ok_label));                       // explicit argument count matches the constructor signature
        emitter.instruction("test rax, rax");                                   // did eval omit the optional builtin constructor argument?
        emitter.instruction(&format!("je {}", ok_label));                       // accept the builtin zero-default constructor call
        emitter.instruction(&format!("jmp {}", fail_label));                    // reject constructor dispatch when arity differs
        emitter.label(&ok_label);
    } else {
        emitter.instruction(&format!("jne {}", fail_label));                    // reject constructor dispatch when arity differs
    }
}

/// Prepares ARM64 constructor argument temporaries for the supported argument shapes.
fn emit_aarch64_prepare_constructor_args(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalConstructorSlot,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> (usize, Vec<EvalRefArgSlot>) {
    let body_label = constructor_body_label(module, slot);
    let ref_slots = emit_aarch64_constructor_ref_arg_cells(
        module,
        emitter,
        data,
        &slot.params,
        &slot.ref_params,
        &body_label,
        fail_label,
        callable_support,
    );
    let visible_abi_params = eval_abi_param_types_for_refs(&slot.params, &slot.ref_params);
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    emitter.instruction("ldr x0, [x29, #-32]");                                 // load the unboxed receiver as the first constructor argument
    abi::emit_push_result_value(emitter, &receiver_ty);
    let mut arg_temp_bytes = eval_arg_temp_slot_size(&receiver_ty);
    for (index, param_ty) in slot.params.iter().enumerate() {
        if slot.zero_default_first_arg && index == 0 {
            let default_label = format!("{}_arg_{}_default", body_label, index);
            let done_label = format!("{}_arg_{}_done", body_label, index);
            emitter.instruction("ldr x9, [sp, #32]");                           // reload argc before selecting the optional constructor default
            emitter.instruction(&format!("cbz x9, {}", default_label));         // omitted SplFixedArray size uses PHP's zero default
            emit_aarch64_load_eval_arg(module, emitter, index);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            emit_aarch64_cast_eval_arg(
                module,
                emitter,
                param_ty,
                &label_prefix,
                fail_label,
                data,
                callable_support,
            );
            abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
            emitter.instruction(&format!("b {}", done_label));                  // skip default materialization after an explicit argument
            emitter.label(&default_label);
            abi::emit_load_int_immediate(emitter, "x0", 0);
            abi::emit_push_result_value(emitter, &PhpType::Int);
            emitter.label(&done_label);
        } else if let Some(ref_slot) = ref_slots.iter().find(|ref_slot| ref_slot.param_index == index) {
            abi::emit_temporary_stack_address(
                emitter,
                abi::int_result_reg(emitter),
                arg_temp_bytes + ref_slot.raw_offset,
            );
            abi::emit_push_result_value(emitter, &PhpType::Int);
        } else {
            emit_aarch64_load_eval_arg(module, emitter, index);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            emit_aarch64_cast_eval_arg(
                module,
                emitter,
                param_ty,
                &label_prefix,
                fail_label,
                data,
                callable_support,
            );
            abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
        }
        arg_temp_bytes += eval_arg_temp_slot_size(&visible_abi_params[index]);
    }
    (arg_temp_bytes, ref_slots)
}

/// Prepares x86_64 constructor argument temporaries for the supported argument shapes.
fn emit_x86_64_prepare_constructor_args(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalConstructorSlot,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> (usize, Vec<EvalRefArgSlot>) {
    let body_label = constructor_body_label(module, slot);
    let ref_slots = emit_x86_64_constructor_ref_arg_cells(
        module,
        emitter,
        data,
        &slot.params,
        &slot.ref_params,
        &body_label,
        fail_label,
        callable_support,
    );
    let visible_abi_params = eval_abi_param_types_for_refs(&slot.params, &slot.ref_params);
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the unboxed receiver as the first constructor argument
    abi::emit_push_result_value(emitter, &receiver_ty);
    let mut arg_temp_bytes = eval_arg_temp_slot_size(&receiver_ty);
    for (index, param_ty) in slot.params.iter().enumerate() {
        if slot.zero_default_first_arg && index == 0 {
            let default_label = format!("{}_arg_{}_default", body_label, index);
            let done_label = format!("{}_arg_{}_done", body_label, index);
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload argc before selecting the optional constructor default
            emitter.instruction("test r10, r10");                               // did eval pass an explicit constructor argument?
            emitter.instruction(&format!("jz {}", default_label));              // omitted SplFixedArray size uses PHP's zero default
            emit_x86_64_load_eval_arg(module, emitter, index);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            emit_x86_64_cast_eval_arg(
                module,
                emitter,
                param_ty,
                &label_prefix,
                fail_label,
                data,
                callable_support,
            );
            abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
            emitter.instruction(&format!("jmp {}", done_label));                // skip default materialization after an explicit argument
            emitter.label(&default_label);
            abi::emit_load_int_immediate(emitter, "rax", 0);
            abi::emit_push_result_value(emitter, &PhpType::Int);
            emitter.label(&done_label);
        } else if let Some(ref_slot) = ref_slots.iter().find(|ref_slot| ref_slot.param_index == index) {
            abi::emit_temporary_stack_address(
                emitter,
                abi::int_result_reg(emitter),
                arg_temp_bytes + ref_slot.raw_offset,
            );
            abi::emit_push_result_value(emitter, &PhpType::Int);
        } else {
            emit_x86_64_load_eval_arg(module, emitter, index);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            emit_x86_64_cast_eval_arg(
                module,
                emitter,
                param_ty,
                &label_prefix,
                fail_label,
                data,
                callable_support,
            );
            abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
        }
        arg_temp_bytes += eval_arg_temp_slot_size(&visible_abi_params[index]);
    }
    (arg_temp_bytes, ref_slots)
}

/// Materializes the pushed receiver and eval arguments into the target method ABI.
fn materialize_constructor_args(
    module: &Module,
    emitter: &mut Emitter,
    receiver_ty: &PhpType,
    params: &[PhpType],
    ref_params: &[bool],
) -> usize {
    let mut arg_types = Vec::with_capacity(params.len() + 1);
    arg_types.push(receiver_ty.clone());
    arg_types.extend(eval_abi_param_types_for_refs(params, ref_params));
    let assignments = abi::build_outgoing_arg_assignments_for_target(module.target, &arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

/// Prepares ARM64 stack cells for eval-supplied by-reference constructor arguments.
fn emit_aarch64_constructor_ref_arg_cells(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    param_types: &[PhpType],
    ref_params: &[bool],
    label_prefix: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> Vec<EvalRefArgSlot> {
    let ref_slots = eval_ref_arg_slots(param_types, ref_params, true);
    for slot in &ref_slots {
        emit_aarch64_load_eval_arg(module, emitter, slot.param_index);
        emitter.instruction("ldr x0, [x29, #-16]");                             // reload the original eval Mixed cell for by-reference writeback
        abi::emit_push_result_value(emitter, &PhpType::Mixed);
        if matches!(slot.param_ty.codegen_repr(), PhpType::Mixed) {
            emitter.instruction("ldr x0, [x29, #-16]");                         // seed the mutable by-reference Mixed slot with the original cell
            abi::emit_push_result_value(emitter, &PhpType::Mixed);
        } else {
            let arg_label = format!("{}_ref_arg_{}", label_prefix, slot.param_index);
            emit_aarch64_cast_eval_arg(
                module,
                emitter,
                &slot.param_ty,
                &arg_label,
                fail_label,
                data,
                callable_support,
            );
            abi::emit_push_result_value(emitter, &slot.param_ty);
        }
    }
    ref_slots
}

/// Prepares x86_64 stack cells for eval-supplied by-reference constructor arguments.
fn emit_x86_64_constructor_ref_arg_cells(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    param_types: &[PhpType],
    ref_params: &[bool],
    label_prefix: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> Vec<EvalRefArgSlot> {
    let ref_slots = eval_ref_arg_slots(param_types, ref_params, true);
    for slot in &ref_slots {
        emit_x86_64_load_eval_arg(module, emitter, slot.param_index);
        emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                   // reload the original eval Mixed cell for by-reference writeback
        abi::emit_push_result_value(emitter, &PhpType::Mixed);
        if matches!(slot.param_ty.codegen_repr(), PhpType::Mixed) {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // seed the mutable by-reference Mixed slot with the original cell
            abi::emit_push_result_value(emitter, &PhpType::Mixed);
        } else {
            let arg_label = format!("{}_ref_arg_{}", label_prefix, slot.param_index);
            emit_x86_64_cast_eval_arg(
                module,
                emitter,
                &slot.param_ty,
                &arg_label,
                fail_label,
                data,
                callable_support,
            );
            abi::emit_push_result_value(emitter, &slot.param_ty);
        }
    }
    ref_slots
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
fn emit_aarch64_cast_eval_arg(
    module: &Module,
    emitter: &mut Emitter,
    param_ty: &PhpType,
    label_prefix: &str,
    fail_label: &str,
    data: &mut DataSection,
    callable_support: &EvalCallableDescriptorSupport,
) {
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
        PhpType::Callable => {
            super::eval_callable_helpers::emit_aarch64_cast_eval_callable_arg(
                module,
                emitter,
                data,
                callable_support,
                label_prefix,
                fail_label,
            );
        }
        PhpType::TaggedScalar => {
            emit_aarch64_cast_eval_tagged_scalar_arg(emitter, label_prefix);
        }
        PhpType::Mixed => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for a Mixed constructor parameter
        }
        PhpType::Object(_) => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for object unboxing
            emitter.instruction("bl __rt_mixed_unbox");                         // expose the eval object payload for the constructor ABI
            emitter.instruction("cmp x0, #6");                                  // runtime tag 6 means the eval argument is an object
            emitter.instruction(&format!("b.ne {}", fail_label));               // reject malformed non-object constructor arguments
            emitter.instruction("mov x0, x1");                                  // move the unboxed object payload into the result register
            abi::emit_incref_if_refcounted(emitter, &param_ty.codegen_repr());
        }
        PhpType::Array(_) => {
            emit_aarch64_cast_eval_array_arg(emitter, param_ty, 4, fail_label);
        }
        PhpType::AssocArray { .. } => {
            emit_aarch64_cast_eval_array_arg(emitter, param_ty, 5, fail_label);
        }
        PhpType::Iterable => {
            emit_aarch64_cast_eval_iterable_arg(module, emitter, param_ty, label_prefix, fail_label);
        }
        _ => {}
    }
}

/// Coerces one ARM64 eval argument into the inline nullable-int tagged-scalar ABI pair.
fn emit_aarch64_cast_eval_tagged_scalar_arg(emitter: &mut Emitter, label_prefix: &str) {
    let null_label = format!("{}_tagged_scalar_null", label_prefix);
    let done_label = format!("{}_tagged_scalar_done", label_prefix);
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for nullable-int inspection
    emitter.instruction("str x0, [sp, #-16]!");                                 // preserve the boxed eval argument across tag inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the concrete eval argument tag and payload words
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means the nullable-int argument is null
    emitter.instruction(&format!("b.eq {}", null_label));                       // materialize a tagged null for null eval arguments
    emitter.instruction("ldr x0, [sp]");                                        // reload the boxed eval argument for integer coercion
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the non-null eval argument to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("b {}", done_label));                          // skip the null materialization path after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    emitter.instruction("add sp, sp, #16");                                     // discard the preserved boxed eval argument
}

/// Validates and unboxes one ARM64 array-typed eval argument for native constructors.
fn emit_aarch64_cast_eval_array_arg(
    emitter: &mut Emitter,
    param_ty: &PhpType,
    expected_tag: i64,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for array unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the eval array payload for the constructor ABI
    abi::emit_load_int_immediate(emitter, "x9", expected_tag);
    emitter.instruction("cmp x0, x9");                                          // compare the eval payload tag with the expected array ABI
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject array payloads with an incompatible ABI shape
    emitter.instruction("mov x0, x1");                                          // move the unboxed array payload into the result register
    abi::emit_incref_if_refcounted(emitter, &param_ty.codegen_repr());
}

/// Validates and unboxes one ARM64 iterable-typed eval argument for native constructors.
fn emit_aarch64_cast_eval_iterable_arg(
    module: &Module,
    emitter: &mut Emitter,
    param_ty: &PhpType,
    label_prefix: &str,
    fail_label: &str,
) {
    let payload_ok = format!("{}_iterable_payload", label_prefix);
    let object_case = format!("{}_iterable_object", label_prefix);
    let object_ok = format!("{}_iterable_object_ok", label_prefix);
    let done = format!("{}_iterable_done", label_prefix);
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for iterable unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the concrete iterable payload tag and pointer
    emitter.instruction("cmp x0, #4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("b.eq {}", payload_ok));                       // indexed arrays satisfy iterable parameters
    emitter.instruction("cmp x0, #5");                                          // runtime tag 5 means associative array
    emitter.instruction(&format!("b.eq {}", payload_ok));                       // associative arrays satisfy iterable parameters
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means object
    emitter.instruction(&format!("b.eq {}", object_case));                      // object values need Traversable interface validation
    emitter.instruction(&format!("b {}", fail_label));                          // reject scalar values for iterable parameters
    emitter.label(&payload_ok);
    emitter.instruction("mov x0, x1");                                          // move the array payload into the result register
    emitter.instruction(&format!("b {}", done));                                // skip object-specific interface validation
    emitter.label(&object_case);
    emit_aarch64_validate_iterable_object(module, emitter, &object_ok, fail_label);
    emitter.label(&object_ok);
    emitter.instruction("ldr x0, [sp], #16");                                   // restore the iterable object pointer as the result
    emitter.label(&done);
    abi::emit_incref_if_refcounted(emitter, &param_ty.codegen_repr());
}

/// Validates the ARM64 object payload saved in `x1` against Traversable interfaces.
fn emit_aarch64_validate_iterable_object(
    module: &Module,
    emitter: &mut Emitter,
    object_ok: &str,
    fail_label: &str,
) {
    let interface_ids = traversable_interface_ids(module);
    if interface_ids.is_empty() {
        emitter.instruction(&format!("b {}", fail_label));                      // reject objects when no Traversable interface metadata exists
        return;
    }
    emitter.instruction("str x1, [sp, #-16]!");                                 // preserve the object payload across Traversable checks
    for interface_id in interface_ids {
        emitter.instruction("ldr x0, [sp]");                                    // reload the object pointer as matcher argument 1
        abi::emit_load_int_immediate(emitter, "x1", interface_id as i64);
        abi::emit_load_int_immediate(emitter, "x2", 1);
        abi::emit_call_label(emitter, "__rt_exception_matches");
        emitter.instruction("cmp x0, #0");                                      // test whether the object implements this Traversable interface
        emitter.instruction(&format!("b.ne {}", object_ok));                    // matching Iterator metadata accepts the object
    }
    emitter.instruction("add sp, sp, #16");                                     // discard the rejected object payload
    emitter.instruction(&format!("b {}", fail_label));                          // reject non-Traversable objects for iterable parameters
}

/// Casts one boxed eval argument into x86_64 result registers for temporary staging.
fn emit_x86_64_cast_eval_arg(
    module: &Module,
    emitter: &mut Emitter,
    param_ty: &PhpType,
    label_prefix: &str,
    fail_label: &str,
    data: &mut DataSection,
    callable_support: &EvalCallableDescriptorSupport,
) {
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
        PhpType::Callable => {
            super::eval_callable_helpers::emit_x86_64_cast_eval_callable_arg(
                module,
                emitter,
                data,
                callable_support,
                label_prefix,
                fail_label,
                X86_64_CONSTRUCTOR_CONTEXT_FRAME_OFFSET,
            );
        }
        PhpType::TaggedScalar => {
            emit_x86_64_cast_eval_tagged_scalar_arg(emitter, label_prefix);
        }
        PhpType::Mixed => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for a Mixed constructor parameter
        }
        PhpType::Object(_) => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for object unboxing
            emitter.instruction("call __rt_mixed_unbox");                       // expose the eval object payload for the constructor ABI
            emitter.instruction("cmp rax, 6");                                  // runtime tag 6 means the eval argument is an object
            emitter.instruction(&format!("jne {}", fail_label));                // reject malformed non-object constructor arguments
            emitter.instruction("mov rax, rdi");                                // move the unboxed object payload into the result register
            abi::emit_incref_if_refcounted(emitter, &param_ty.codegen_repr());
        }
        PhpType::Array(_) => {
            emit_x86_64_cast_eval_array_arg(emitter, param_ty, 4, fail_label);
        }
        PhpType::AssocArray { .. } => {
            emit_x86_64_cast_eval_array_arg(emitter, param_ty, 5, fail_label);
        }
        PhpType::Iterable => {
            emit_x86_64_cast_eval_iterable_arg(module, emitter, param_ty, label_prefix, fail_label);
        }
        _ => {}
    }
}

/// Coerces one x86_64 eval argument into the inline nullable-int tagged-scalar ABI pair.
fn emit_x86_64_cast_eval_tagged_scalar_arg(emitter: &mut Emitter, label_prefix: &str) {
    let null_label = format!("{}_tagged_scalar_null", label_prefix);
    let done_label = format!("{}_tagged_scalar_done", label_prefix);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for nullable-int inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the concrete eval argument tag and payload words
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means the nullable-int argument is null
    emitter.instruction(&format!("je {}", null_label));                         // materialize a tagged null for null eval arguments
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for integer coercion
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the non-null eval argument to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("jmp {}", done_label));                        // skip the null materialization path after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
}

/// Validates and unboxes one x86_64 array-typed eval argument for native constructors.
fn emit_x86_64_cast_eval_array_arg(
    emitter: &mut Emitter,
    param_ty: &PhpType,
    expected_tag: i64,
    fail_label: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for array unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the eval array payload for the constructor ABI
    abi::emit_load_int_immediate(emitter, "r10", expected_tag);
    emitter.instruction("cmp rax, r10");                                        // compare the eval payload tag with the expected array ABI
    emitter.instruction(&format!("jne {}", fail_label));                        // reject array payloads with an incompatible ABI shape
    emitter.instruction("mov rax, rdi");                                        // move the unboxed array payload into the result register
    abi::emit_incref_if_refcounted(emitter, &param_ty.codegen_repr());
}

/// Validates and unboxes one x86_64 iterable-typed eval argument for native constructors.
fn emit_x86_64_cast_eval_iterable_arg(
    module: &Module,
    emitter: &mut Emitter,
    param_ty: &PhpType,
    label_prefix: &str,
    fail_label: &str,
) {
    let payload_ok = format!("{}_iterable_payload", label_prefix);
    let object_case = format!("{}_iterable_object", label_prefix);
    let object_ok = format!("{}_iterable_object_ok", label_prefix);
    let done = format!("{}_iterable_done", label_prefix);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for iterable unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the concrete iterable payload tag and pointer
    emitter.instruction("cmp rax, 4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("je {}", payload_ok));                         // indexed arrays satisfy iterable parameters
    emitter.instruction("cmp rax, 5");                                          // runtime tag 5 means associative array
    emitter.instruction(&format!("je {}", payload_ok));                         // associative arrays satisfy iterable parameters
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means object
    emitter.instruction(&format!("je {}", object_case));                        // object values need Traversable interface validation
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject scalar values for iterable parameters
    emitter.label(&payload_ok);
    emitter.instruction("mov rax, rdi");                                        // move the array payload into the result register
    emitter.instruction(&format!("jmp {}", done));                              // skip object-specific interface validation
    emitter.label(&object_case);
    emit_x86_64_validate_iterable_object(module, emitter, &object_ok, fail_label);
    emitter.label(&object_ok);
    abi::emit_pop_reg(emitter, "rax");
    emitter.label(&done);
    abi::emit_incref_if_refcounted(emitter, &param_ty.codegen_repr());
}

/// Validates the x86_64 object payload saved in `rdi` against Traversable interfaces.
fn emit_x86_64_validate_iterable_object(
    module: &Module,
    emitter: &mut Emitter,
    object_ok: &str,
    fail_label: &str,
) {
    let interface_ids = traversable_interface_ids(module);
    if interface_ids.is_empty() {
        emitter.instruction(&format!("jmp {}", fail_label));                    // reject objects when no Traversable interface metadata exists
        return;
    }
    abi::emit_push_reg(emitter, "rdi");
    for interface_id in interface_ids {
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // reload the object pointer as matcher argument 1
        abi::emit_load_int_immediate(emitter, "rsi", interface_id as i64);
        abi::emit_load_int_immediate(emitter, "rdx", 1);
        abi::emit_call_label(emitter, "__rt_exception_matches");
        emitter.instruction("test rax, rax");                                   // test whether the object implements this Traversable interface
        emitter.instruction(&format!("jne {}", object_ok));                     // matching Iterator metadata accepts the object
    }
    abi::emit_pop_reg(emitter, "r10");
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject non-Traversable objects for iterable parameters
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

/// Returns a label-safe constructor body prefix for bridge-local writeback branches.
fn constructor_body_label(module: &Module, slot: &EvalConstructorSlot) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!("__elephc_eval_constructor_{}{}", slot.class_id, suffix)
}

/// Returns a platform-safe label for a successful scoped constructor access check.
fn constructor_scope_ok_label(module: &Module, slot: &EvalConstructorSlot) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!("__elephc_eval_constructor_{}_scope_ok{}", slot.class_id, suffix)
}

/// Returns runtime interface ids for object values accepted by PHP iterable parameters.
fn traversable_interface_ids(module: &Module) -> Vec<u64> {
    ["Iterator", "IteratorAggregate"]
        .into_iter()
        .filter_map(|name| module.interface_infos.get(name).map(|info| info.interface_id))
        .collect()
}

/// Returns class scopes that satisfy one constructor visibility for a declaring class.
fn visibility_scope_names(
    module: &Module,
    declaring_class: &str,
    visibility: &Visibility,
) -> Vec<String> {
    match visibility {
        Visibility::Public => Vec::new(),
        Visibility::Private => vec![declaring_class.to_string()],
        Visibility::Protected => related_class_scope_names(module, declaring_class),
    }
}

/// Returns AOT classes in the same inheritance line as `declaring_class`.
fn related_class_scope_names(module: &Module, declaring_class: &str) -> Vec<String> {
    let mut scopes = module
        .class_infos
        .keys()
        .filter(|class_name| {
            is_same_or_descendant(module, class_name, declaring_class)
                || is_same_or_descendant(module, declaring_class, class_name)
        })
        .cloned()
        .collect::<Vec<_>>();
    scopes.sort_by(|left, right| {
        class_id_for_scope(module, left)
            .cmp(&class_id_for_scope(module, right))
            .then_with(|| left.cmp(right))
    });
    scopes
}

/// Returns true when `class_name` is `ancestor` or descends from it.
fn is_same_or_descendant(module: &Module, class_name: &str, ancestor: &str) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Returns the deterministic class id used to order generated scope checks.
fn class_id_for_scope(module: &Module, class_name: &str) -> u64 {
    module
        .class_infos
        .get(class_name)
        .map(|class_info| class_info.class_id)
        .unwrap_or(u64::MAX)
}

/// Emits a platform-C global label for a user assembly helper.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    emitter.label_global(&module.target.extern_symbol(name));
}
