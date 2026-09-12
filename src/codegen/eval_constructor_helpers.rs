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
//! - By-value arguments are BORROWED from the caller's argument array, exactly as the eval
//!   method bridge stages them. Only a by-reference slot acquires an owner, because its
//!   writeback is the one path that releases the raw slot again.
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
    emit_aarch64_write_back_ref_args, emit_acquire_mixed_ref_args, emit_x86_64_write_back_ref_args,
};
use super::eval_callable_helpers::EvalCallableDescriptorSupport;
use super::eval_argument_helpers::emit_borrowed_string_arg;

const BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES: &[&str] = &[
    "Error",
    "TypeError",
    "ArgumentCountError",
    "ValueError",
    "ArithmeticError",
    "DivisionByZeroError",
    "AssertionError",
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

/// Whether one staged constructor argument keeps an owner beyond the cast that produced it.
///
/// A BY-VALUE argument is rooted by Magician's normalized argument array for the whole native
/// activation and is released with it, so the bridge only borrows the unboxed payload. This is
/// exactly how the eval method bridge stages the same parameter types, and the generated
/// `__construct` never consumes an argument: the AOT direct-call site retires its own argument
/// temporaries after the call instead.
///
/// A BY-REFERENCE slot is different. `eval_ref_arg_slots` gives constructor slots
/// `raw_refcounted_owned = true`, so writeback releases the raw slot on the changed and the
/// unchanged path alike; that release is only balanced when the staging cast acquired an owner.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConstructorArgOwner {
    /// The argument array keeps the only owner for the duration of the call.
    Borrowed,
    /// The raw by-reference slot owns the staged payload until writeback releases it.
    Owned,
}

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
        sig.params.iter().map(|(_, ty)| super::eval_argument_helpers::bridge_storage_type(ty)).collect()
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
    emitter.instruction(                                                        //reserve helper frame plus a boundary exception handler
        &format!("sub sp, sp, #{}", CONSTRUCTOR_HELPER_FRAME_SIZE)
    );
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
    emitter.instruction(                                                        //release the constructor helper frame and boundary handler
        &format!("add sp, sp, #{}", CONSTRUCTOR_HELPER_FRAME_SIZE)
    );
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
    emitter.instruction(&format!("sub rsp, {}", CONSTRUCTOR_HELPER_FRAME_SIZE));//reserve aligned slots plus a boundary exception handler
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
    emitter.instruction(&format!(                                               // save diagnostic suppression depth for restoration
        "str x10, [x29, #{}]",
        handler_offset + TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    emitter.instruction(&format!("add x10, x29, #{}", handler_offset));         // compute the boundary handler record address
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // pass the boundary jmp_buf to setjmp
        "add x0, x29, #{}",
        handler_offset + TRY_HANDLER_JMP_BUF_OFFSET
    ));
    emitter.bl_c("setjmp");                                                     // snapshot the bridge stack before entering native constructors
    emitter.instruction(&format!("cbnz x0, {}", escape_label));                 // non-zero setjmp result means a constructor Throwable escaped
}

/// Emits an ARM64 boundary pop that preserves the constructor status in x0.
fn emit_aarch64_constructor_exception_boundary_pop(emitter: &mut Emitter) {
    let handler_offset = CONSTRUCTOR_HELPER_HANDLER_OFFSET - 48;
    emitter.comment("pop eval constructor exception boundary");
    emitter.instruction(&format!("ldr x10, [x29, #{}]", handler_offset));       // reload the previous native exception-handler head
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // reload the saved diagnostic suppression depth
        "ldr x10, [x29, #{}]",
        handler_offset + TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
}

/// Emits an x86_64 boundary handler so native constructor throws return to magician.
fn emit_x86_64_constructor_exception_boundary_push(emitter: &mut Emitter, escape_label: &str) {
    let handler_base = CONSTRUCTOR_HELPER_FRAME_SIZE;
    emitter.comment("push eval constructor exception boundary");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(                                                        //save the previous native exception-handler head
        &format!("mov QWORD PTR [rbp - {}], r10", handler_base)
    );
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(                                                        //preserve the caller activation frame across constructor unwinding
        &format!("mov QWORD PTR [rbp - {}], r10", handler_base - 8)
    );
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!(                                               // save diagnostic suppression depth for restoration
        "mov QWORD PTR [rbp - {}], r10",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    emitter.instruction(&format!("lea r10, [rbp - {}]", handler_base));         // compute the boundary handler record address
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // pass the boundary jmp_buf to setjmp
        "lea rdi, [rbp - {}]",
        handler_base - TRY_HANDLER_JMP_BUF_OFFSET
    ));
    emitter.bl_c("setjmp");                                                      // snapshot the bridge stack before entering native constructors
    emitter.instruction("test eax, eax");                                       // did control arrive through longjmp?
    emitter.instruction(&format!("jne {}", escape_label));                      // non-zero setjmp result means a constructor Throwable escaped
}

/// Emits an x86_64 boundary pop that preserves the constructor status in rax.
fn emit_x86_64_constructor_exception_boundary_pop(emitter: &mut Emitter) {
    let handler_base = CONSTRUCTOR_HELPER_FRAME_SIZE;
    emitter.comment("pop eval constructor exception boundary");
    emitter.instruction(                                                        //reload the previous native exception-handler head
        &format!("mov r10, QWORD PTR [rbp - {}]", handler_base)
    );
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // reload the saved diagnostic suppression depth
        "mov r10, QWORD PTR [rbp - {}]",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
}

/// Transfers the pending native exception as one owned box through the versioned eval ABI.
fn emit_take_pending_throwable_helper(module: &Module, emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- eval bridge: take pending throwable ---");
    label_c_global(module, emitter, "__elephc_eval_value_take_pending_throwable_v2");
    let result = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_eval_no_pending_throwable");
    abi::emit_jump(emitter, "__rt_throwable_box_owned");
    emitter.label("__rt_eval_no_pending_throwable");
    abi::emit_return(emitter);
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
    emit_aarch64_load_eval_arg(module, emitter, 0, fail_label);
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
    emit_aarch64_load_eval_arg(module, emitter, 1, fail_label);
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
    emit_x86_64_load_eval_arg(module, emitter, 0, fail_label);
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
    emit_x86_64_load_eval_arg(module, emitter, 1, fail_label);
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

/// Stores the nullable previous argument using the receiver's compact or ordinary ARM64 layout.
fn emit_aarch64_builtin_throwable_previous_arg(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    success_label: &str,
) {
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload argc before testing the previous argument
    emitter.instruction("cmp x9, #2");                                          // did eval supply the normalized previous argument?
    emitter.instruction(&format!("b.le {}", success_label));                    // keep null when the legacy bridge omitted previous
    emit_aarch64_load_eval_arg(module, emitter, 2, fail_label);
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed previous argument for inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the nullable previous payload
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means the previous argument is null
    emitter.instruction(&format!("b.eq {}", success_label));                    // keep the default raw null previous pointer
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the previous argument is an object
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject malformed non-object previous arguments
    emitter.instruction("ldr x9, [sp, #16]");                                   // inspect the actual receiver allocated by AOT or the by-name eval bridge
    emitter.instruction("ldr x10, [x9, #-8]");                                  // recover the heap kind independently of the PHP class id
    emitter.instruction("and x10, x10, #0xff");                                 // discard collector and ownership flags
    emitter.instruction("cmp x10, #6");                                         // only compact Throwable payloads own a raw previous pointer
    emitter.instruction("ldr x0, [x29, #-16]");                                 // ordinary objects retain the nullable property's Mixed argument cell
    emitter.instruction("csel x0, x1, x0, eq");                                 // compact objects instead retain the unboxed previous object
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the receiver after retaining its previous owner
    emitter.instruction("str x0, [x9, #40]");                                   // store the previous owner in the representation its reader and cleanup expect
    emitter.instruction(&format!("b {}", success_label));                       // builtin Throwable construction completed
}

/// Stores the nullable previous argument using the receiver's compact or ordinary x86_64 layout.
fn emit_x86_64_builtin_throwable_previous_arg(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    success_label: &str,
) {
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload argc before testing the previous argument
    emitter.instruction("cmp r11, 2");                                          // did eval supply the normalized previous argument?
    emitter.instruction(&format!("jle {}", success_label));                     // keep null when the legacy bridge omitted previous
    emit_x86_64_load_eval_arg(module, emitter, 2, fail_label);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed previous argument for inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the nullable previous payload
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means the previous argument is null
    emitter.instruction(&format!("je {}", success_label));                      // keep the default raw null previous pointer
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the previous argument is an object
    emitter.instruction(&format!("jne {}", fail_label));                        // reject malformed non-object previous arguments
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // inspect the receiver allocated by AOT or the by-name eval bridge
    emitter.instruction("mov r10, QWORD PTR [r11 - 8]");                        // recover the actual heap layout independently of the PHP class id
    emitter.instruction("and r10, 0xff");                                       // discard the heap marker and collector flags
    emitter.instruction("cmp r10, 6");                                          // compact Throwable payloads own raw previous pointers
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // ordinary objects retain the nullable property's Mixed argument cell
    emitter.instruction("cmove rax, rdi");                                      // compact objects instead retain the unboxed previous object
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the receiver after retaining its previous owner
    emitter.instruction("mov QWORD PTR [r11 + 40], rax");                       // store the previous owner in its reader and cleanup representation
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
    emit_release_builtin_throwable_defaults(emitter);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for default initialization
    emitter.instruction("str xzr, [x9, #8]");                                   // default the message pointer to an empty string payload
    emitter.instruction("str xzr, [x9, #16]");                                  // default the message length to zero
    emitter.instruction("str xzr, [x9, #24]");                                  // default the exception code to zero
    emitter.instruction("str xzr, [x9, #40]");                                  // default the previous Throwable pointer to null
}

/// Writes x86_64 empty-message, zero-code, and null-previous Throwable defaults.
fn emit_x86_64_default_builtin_throwable_fields(emitter: &mut Emitter) {
    emit_release_builtin_throwable_defaults(emitter);
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for default initialization
    emitter.instruction("mov QWORD PTR [r11 + 8], 0");                          // default the message pointer to an empty string payload
    emitter.instruction("mov QWORD PTR [r11 + 16], 0");                         // default the message length to zero
    emitter.instruction("mov QWORD PTR [r11 + 24], 0");                         // default the exception code to zero
    emitter.instruction("mov QWORD PTR [r11 + 40], 0");                         // default the previous Throwable pointer to null
}

/// Releases the default owners installed by by-name allocation before compact Throwable setup.
fn emit_release_builtin_throwable_defaults(emitter: &mut Emitter) {
    let object_frame_offset = match emitter.target.arch {
        Arch::AArch64 => 32,
        Arch::X86_64 => 24,
    };
    let object = abi::symbol_scratch_reg(emitter);
    let value = abi::int_result_reg(emitter);
    for (offset, release) in [(8, "__rt_heap_free_safe"), (40, "__rt_decref_any")] {
        abi::load_at_offset(emitter, object, object_frame_offset);
        abi::emit_load_from_address(emitter, value, object, offset);
        // Detach before release, since a previous exception may run a user destructor.
        abi::emit_store_zero_to_address(emitter, object, offset);
        if offset == 8 {
            abi::emit_store_zero_to_address(emitter, object, 16);
        }
        abi::emit_call_label(emitter, release);
    }
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
    emit_acquire_mixed_ref_args(emitter, &ref_slots, arg_temp_bytes);
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
    emit_acquire_mixed_ref_args(emitter, &ref_slots, arg_temp_bytes);
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
    emitter.instruction("str x0, [x29, #-8]");                                  // save argc frame-relative, clear of the borrowed argument spill
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
            emitter.instruction("ldr x9, [x29, #-8]");                          // reload argc before selecting the optional constructor default
            emitter.instruction(&format!("cbz x9, {}", default_label));         // omitted SplFixedArray size uses PHP's zero default
            emit_aarch64_load_eval_arg(module, emitter, index, fail_label);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            emit_aarch64_cast_eval_arg(
                module,
                emitter,
                param_ty,
                &label_prefix,
                fail_label,
                data,
                callable_support,
                ConstructorArgOwner::Borrowed,
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
            emit_aarch64_load_eval_arg(module, emitter, index, fail_label);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            if !emit_borrowed_string_arg(emitter, param_ty, 16, fail_label) {
                emit_aarch64_cast_eval_arg(
                    module,
                    emitter,
                    param_ty,
                    &label_prefix,
                    fail_label,
                    data,
                    callable_support,
                    ConstructorArgOwner::Borrowed,
                );
            }
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
            emit_x86_64_load_eval_arg(module, emitter, index, fail_label);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            emit_x86_64_cast_eval_arg(
                module,
                emitter,
                param_ty,
                &label_prefix,
                fail_label,
                data,
                callable_support,
                ConstructorArgOwner::Borrowed,
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
            emit_x86_64_load_eval_arg(module, emitter, index, fail_label);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            if !emit_borrowed_string_arg(emitter, param_ty, 40, fail_label) {
                emit_x86_64_cast_eval_arg(
                    module,
                    emitter,
                    param_ty,
                    &label_prefix,
                    fail_label,
                    data,
                    callable_support,
                    ConstructorArgOwner::Borrowed,
                );
            }
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
        emit_aarch64_load_eval_arg(module, emitter, slot.param_index, fail_label);
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
                ConstructorArgOwner::Owned,
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
        emit_x86_64_load_eval_arg(module, emitter, slot.param_index, fail_label);
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
                ConstructorArgOwner::Owned,
            );
            abi::emit_push_result_value(emitter, &slot.param_ty);
        }
    }
    ref_slots
}

/// Borrows one normalized eval argument in an ARM64 spill slot for constructor dispatch.
fn emit_aarch64_load_eval_arg(_module: &Module, emitter: &mut Emitter, index: usize, fail_label: &str) {
    super::eval_argument_helpers::emit_borrowed_argument(emitter, index, 24, 16, fail_label);
}

/// Borrows one normalized eval argument in an x86_64 spill slot for constructor dispatch.
fn emit_x86_64_load_eval_arg(_module: &Module, emitter: &mut Emitter, index: usize, fail_label: &str) {
    super::eval_argument_helpers::emit_borrowed_argument(emitter, index, 32, 40, fail_label);
}

/// Acquires an owner for a staged constructor argument only when the slot owns it.
///
/// `abi::emit_incref_if_refcounted` already selects the active ABI, so both targets share this
/// rule. A borrowed by-value argument keeps exactly one owner, the one Magician's normalized
/// argument array holds across the whole native activation; acquiring a second one here would
/// leak a reference the bridge has no path on which to release it.
fn emit_acquire_staged_constructor_arg(
    emitter: &mut Emitter,
    param_ty: &PhpType,
    owner: ConstructorArgOwner,
) {
    if owner == ConstructorArgOwner::Owned {
        abi::emit_incref_if_refcounted(emitter, &param_ty.codegen_repr());
    }
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
    owner: ConstructorArgOwner,
) {
    if param_ty.is_php_array() {
        emitter.instruction("ldr x0, [x29, #-16]");                             // borrow the boxed argument before checking its PHP array constraint
        super::eval_argument_helpers::emit_require_php_array(emitter, fail_label);
    }
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
            emit_acquire_staged_constructor_arg(emitter, param_ty, owner);
        }
        PhpType::Array(_) => {
            emit_aarch64_cast_eval_array_arg(emitter, param_ty, 4, fail_label, owner);
        }
        PhpType::AssocArray { .. } => {
            emit_aarch64_cast_eval_array_arg(emitter, param_ty, 5, fail_label, owner);
        }
        PhpType::Iterable => {
            emit_aarch64_cast_eval_iterable_arg(
                module, emitter, param_ty, label_prefix, fail_label, owner,
            );
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
    owner: ConstructorArgOwner,
) {
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for array unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the eval array payload for the constructor ABI
    abi::emit_load_int_immediate(emitter, "x9", expected_tag);
    emitter.instruction("cmp x0, x9");                                          // compare the eval payload tag with the expected array ABI
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject array payloads with an incompatible ABI shape
    emitter.instruction("mov x0, x1");                                          // move the unboxed array payload into the result register
    emit_acquire_staged_constructor_arg(emitter, param_ty, owner);
}

/// Validates and unboxes one ARM64 iterable-typed eval argument for native constructors.
fn emit_aarch64_cast_eval_iterable_arg(
    module: &Module,
    emitter: &mut Emitter,
    param_ty: &PhpType,
    label_prefix: &str,
    fail_label: &str,
    owner: ConstructorArgOwner,
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
    emit_acquire_staged_constructor_arg(emitter, param_ty, owner);
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
    owner: ConstructorArgOwner,
) {
    if param_ty.is_php_array() {
        emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                   // borrow the boxed argument before checking its PHP array constraint
        super::eval_argument_helpers::emit_require_php_array(emitter, fail_label);
    }
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
            emit_acquire_staged_constructor_arg(emitter, param_ty, owner);
        }
        PhpType::Array(_) => {
            emit_x86_64_cast_eval_array_arg(emitter, param_ty, 4, fail_label, owner);
        }
        PhpType::AssocArray { .. } => {
            emit_x86_64_cast_eval_array_arg(emitter, param_ty, 5, fail_label, owner);
        }
        PhpType::Iterable => {
            emit_x86_64_cast_eval_iterable_arg(
                module, emitter, param_ty, label_prefix, fail_label, owner,
            );
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
    owner: ConstructorArgOwner,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for array unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the eval array payload for the constructor ABI
    abi::emit_load_int_immediate(emitter, "r10", expected_tag);
    emitter.instruction("cmp rax, r10");                                        // compare the eval payload tag with the expected array ABI
    emitter.instruction(&format!("jne {}", fail_label));                        // reject array payloads with an incompatible ABI shape
    emitter.instruction("mov rax, rdi");                                        // move the unboxed array payload into the result register
    emit_acquire_staged_constructor_arg(emitter, param_ty, owner);
}

/// Validates and unboxes one x86_64 iterable-typed eval argument for native constructors.
fn emit_x86_64_cast_eval_iterable_arg(
    module: &Module,
    emitter: &mut Emitter,
    param_ty: &PhpType,
    label_prefix: &str,
    fail_label: &str,
    owner: ConstructorArgOwner,
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
    emit_acquire_staged_constructor_arg(emitter, param_ty, owner);
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

#[cfg(test)]
mod catalog_tests {
    /// Pending native exceptions transfer an owned box, while an empty slot remains a null result.
    #[test]
    fn pending_throwable_bridge_transfers_boxed_ownership_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen::platform::Target::parse(name).unwrap();
            let module = super::Module::new(target);
            let mut emitter = super::Emitter::new(target);
            super::emit_take_pending_throwable_helper(&module, &mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(&target.extern_symbol("__elephc_eval_value_take_pending_throwable_v2")), "{name}");
            assert_eq!(asm.matches("__rt_throwable_box_owned").count(), 1, "{name}");
            assert!(asm.find("_exc_value").unwrap() < asm.find("__rt_throwable_box_owned").unwrap(), "{name}");
            assert!(asm.contains("__rt_eval_no_pending_throwable:"), "{name}");
        }
    }

    /// Eval construction selects the previous owner from the concrete layout on every target.
    #[test]
    fn throwable_previous_initialization_preserves_ordinary_boxed_storage() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen::platform::Target::parse(name).unwrap();
            let module = super::Module::new(target);
            let mut emitter = super::Emitter::new(target);
            let selection = match target.arch {
                super::Arch::AArch64 => {
                    super::emit_aarch64_builtin_throwable_previous_arg(&module, &mut emitter, "fail", "done");
                    "csel x0, x1, x0, eq"
                }
                super::Arch::X86_64 => {
                    super::emit_x86_64_builtin_throwable_previous_arg(&module, &mut emitter, "fail", "done");
                    "cmove rax, rdi"
                }
            };
            let asm = emitter.output();
            assert!(asm.find(selection).unwrap() < asm.find("__rt_incref").unwrap(), "{name}");
            assert_eq!(asm.matches("__rt_incref").count(), 1, "{name}");
        }
    }

    /// Both native ABIs release default message and previous owners before overwriting the slots.
    #[test]
    fn throwable_default_initialization_releases_displaced_owners_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen::platform::Target::parse(name).unwrap();
            let mut emitter = super::Emitter::new(target);
            match target.arch {
                super::Arch::AArch64 => super::emit_aarch64_default_builtin_throwable_fields(&mut emitter),
                super::Arch::X86_64 => super::emit_x86_64_default_builtin_throwable_fields(&mut emitter),
            }
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_heap_free_safe").count(), 1, "{name}");
            assert_eq!(asm.matches("__rt_decref_any").count(), 1, "{name}");
            let cleared_previous = match target.arch {
                super::Arch::AArch64 => "str xzr, [x9, #40]",
                super::Arch::X86_64 => "mov QWORD PTR [r11 + 40], 0",
            };
            assert!(asm.find(cleared_previous).unwrap() < asm.find("__rt_decref_any").unwrap(), "{name}");
        }
    }

    /// Every throwable this helper can materialize is a catalogued builtin class.
    #[test]
    fn throwable_list_is_a_subset_of_the_class_catalog() {
        for name in super::BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES {
            assert!(
                elephc_builtin_contract::lookup_class(name).is_some(),
                "{name} is not in the shared class catalog"
            );
        }
    }
}

#[cfg(test)]
mod argument_ownership_tests {
    use super::*;
    use crate::codegen::eval_callable_helpers::emit_eval_callable_descriptor_support;
    use crate::codegen::platform::Target;

    const SUPPORTED_TARGETS: &[&str] = &[
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ];

    /// Builds a one-parameter constructor slot in the shape the bridge collects from a class.
    ///
    /// The parameter type mirrors the compiler-generated `__elephc_func_args#gen` collector that
    /// `crate::func_args` appends to every constructor once a program can reach `eval()`, which is
    /// why this leak reproduced for constructors whose PHP source declares no array parameter.
    fn collector_slot(by_ref: bool) -> EvalConstructorSlot {
        EvalConstructorSlot {
            class_id: 7,
            class_name: "Fixture".to_string(),
            impl_class: "Fixture".to_string(),
            visibility: Visibility::Public,
            allowed_scopes: Vec::new(),
            params: vec![PhpType::Array(Box::new(PhpType::Mixed))],
            ref_params: vec![by_ref],
            supported: true,
            runtime_helper: None,
            zero_default_first_arg: false,
        }
    }

    /// Stages one constructor argument and returns the emitted bridge assembly.
    fn staged_argument_asm(target: Target, by_ref: bool) -> String {
        let module = Module::new(target);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let callable_support =
            emit_eval_callable_descriptor_support(&module, &mut emitter, &mut data, false);
        let slot = collector_slot(by_ref);
        match target.arch {
            Arch::AArch64 => {
                emit_aarch64_prepare_constructor_args(
                    &module,
                    &mut emitter,
                    &mut data,
                    &slot,
                    "fail",
                    &callable_support,
                );
            }
            Arch::X86_64 => {
                emit_x86_64_prepare_constructor_args(
                    &module,
                    &mut emitter,
                    &mut data,
                    &slot,
                    "fail",
                    &callable_support,
                );
            }
        }
        emitter.output()
    }

    /// By-value constructor arguments are borrowed from the caller's argument array.
    ///
    /// Magician owns that array for the whole native activation and releases it afterwards, and
    /// the generated `__construct` does not consume its arguments, so the bridge has no path on
    /// which it could retire a second owner. Acquiring one here leaked exactly one reference per
    /// constructor call, which is what the eval method bridge has always avoided.
    #[test]
    fn by_value_constructor_arguments_are_borrowed_on_every_target() {
        for name in SUPPORTED_TARGETS {
            let target = Target::parse(name).unwrap();
            let asm = staged_argument_asm(target, false);
            assert!(asm.contains("__rt_mixed_unbox"), "{name}: {asm}");
            assert!(!asm.contains("__rt_incref"), "{name}: {asm}");
            assert!(!asm.contains("__rt_decref"), "{name}: {asm}");
        }
    }

    /// By-reference constructor slots still acquire the owner their writeback releases.
    ///
    /// `eval_ref_arg_slots` marks constructor slots `raw_refcounted_owned`, so writeback releases
    /// the raw slot on both the changed and the unchanged path. Dropping this acquisition would
    /// turn that release into an over-release.
    #[test]
    fn by_reference_constructor_slots_acquire_one_owner_on_every_target() {
        for name in SUPPORTED_TARGETS {
            let target = Target::parse(name).unwrap();
            let asm = staged_argument_asm(target, true);
            assert_eq!(asm.matches("__rt_incref").count(), 1, "{name}: {asm}");
        }
    }

    /// Constructor by-reference slots own their raw payload while method slots borrow it.
    #[test]
    fn constructor_reference_slots_stay_owned_unlike_method_slots() {
        let params = [PhpType::Array(Box::new(PhpType::Mixed))];
        let slots = eval_ref_arg_slots(&params, &[true], true);
        assert!(slots[0].raw_refcounted_owned);
    }
}

#[cfg(test)]
mod zero_default_argument_tests {
    use super::*;
    use crate::codegen::eval_callable_helpers::emit_eval_callable_descriptor_support;
    use crate::codegen::platform::Target;

    const SUPPORTED_TARGETS: &[&str] = &[
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ];

    /// Builds the `SplFixedArray::__construct` slot shape, the only zero-default bridge slot.
    ///
    /// `collect_class_constructor_slot` derives exactly this shape for `SplFixedArray`: a single
    /// `int` parameter bridged to the runtime helper, with `zero_default_first_arg` set because
    /// PHP declares `__construct(int $size = 0)`.
    fn spl_fixed_array_slot() -> EvalConstructorSlot {
        let runtime_helper = IntrinsicCall::instance_method("SplFixedArray", "__construct")
            .and_then(IntrinsicCall::runtime_helper);
        assert!(runtime_helper.is_some(), "SplFixedArray::__construct lost its runtime helper");
        EvalConstructorSlot {
            class_id: 11,
            class_name: "SplFixedArray".to_string(),
            impl_class: "SplFixedArray".to_string(),
            visibility: Visibility::Public,
            allowed_scopes: Vec::new(),
            params: vec![PhpType::Int],
            ref_params: vec![false],
            supported: true,
            runtime_helper,
            zero_default_first_arg: true,
        }
    }

    /// Emits arity validation followed by argument staging for the zero-default slot.
    fn zero_default_staging_asm(target: Target) -> String {
        let module = Module::new(target);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let callable_support =
            emit_eval_callable_descriptor_support(&module, &mut emitter, &mut data, false);
        let slot = spl_fixed_array_slot();
        match target.arch {
            Arch::AArch64 => {
                emit_aarch64_validate_constructor_arg_count(&module, &mut emitter, &slot, "fail");
                emit_aarch64_prepare_constructor_args(
                    &module,
                    &mut emitter,
                    &mut data,
                    &slot,
                    "fail",
                    &callable_support,
                );
            }
            Arch::X86_64 => {
                emit_x86_64_validate_constructor_arg_count(&module, &mut emitter, &slot, "fail");
                emit_x86_64_prepare_constructor_args(
                    &module,
                    &mut emitter,
                    &mut data,
                    &slot,
                    "fail",
                    &callable_support,
                );
            }
        }
        emitter.output()
    }

    /// The saved argc survives the receiver push that separates its store from its reload.
    ///
    /// Staging pushes the unboxed receiver between the two, which moves SP by one temporary slot
    /// on both ABIs. A stack-pointer-relative reload therefore reads the receiver instead of argc
    /// and `SplFixedArray::__construct()` with no arguments never reaches its zero default. Both
    /// targets must address argc through the helper's frame pointer.
    #[test]
    fn zero_default_argc_is_reloaded_frame_relative_on_every_target() {
        for name in SUPPORTED_TARGETS {
            let target = Target::parse(name).unwrap();
            let asm = zero_default_staging_asm(target);
            let (save, reload, receiver_push) = match target.arch {
                Arch::AArch64 => (
                    "str x0, [x29, #-8]",
                    "ldr x9, [x29, #-8]",
                    "str x0, [sp, #-16]!",
                ),
                Arch::X86_64 => (
                    "mov QWORD PTR [rbp - 8], rax",
                    "mov r10, QWORD PTR [rbp - 8]",
                    "mov QWORD PTR [rsp], rax",
                ),
            };
            assert_eq!(asm.matches(save).count(), 1, "{name}: {asm}");
            assert_eq!(asm.matches(reload).count(), 1, "{name}: {asm}");
            let save_at = asm.find(save).unwrap();
            let push_at = asm.find(receiver_push).unwrap();
            let reload_at = asm.find(reload).unwrap();
            assert!(save_at < push_at, "{name}: {asm}");
            assert!(push_at < reload_at, "{name}: {asm}");
        }
    }

    /// The argc slot never aliases the frame slot `emit_borrowed_argument` spills into.
    ///
    /// On ARM64 the bridge frame places the borrowed argument at `[x29, #-16]`, which is the same
    /// byte as the literal `[sp, #32]` the arity check used to write. Reusing it silently
    /// destroyed argc as soon as the first argument was borrowed.
    #[test]
    fn zero_default_argc_slot_is_disjoint_from_the_borrowed_argument_spill() {
        for name in SUPPORTED_TARGETS {
            let target = Target::parse(name).unwrap();
            let asm = zero_default_staging_asm(target);
            let (argc_slot, borrow_slot) = match target.arch {
                Arch::AArch64 => ("[x29, #-8]", "[x29, #-16]"),
                Arch::X86_64 => ("[rbp - 8]", "[rbp - 40]"),
            };
            assert_ne!(argc_slot, borrow_slot, "{name}");
            assert!(asm.contains(argc_slot), "{name}: {asm}");
            assert!(asm.contains(borrow_slot), "{name}: {asm}");
            if matches!(target.arch, Arch::AArch64) {
                assert!(!asm.contains("[sp, #32]"), "{name}: {asm}");
            }
        }
    }

    /// An omitted `SplFixedArray` size still stages PHP's zero default instead of a borrow.
    #[test]
    fn omitted_spl_fixed_array_size_branches_to_the_zero_default() {
        for name in SUPPORTED_TARGETS {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let slot = spl_fixed_array_slot();
            let body_label = constructor_body_label(&module, &slot);
            let asm = zero_default_staging_asm(target);
            let default_label = format!("{}_arg_0_default", body_label);
            let done_label = format!("{}_arg_0_done", body_label);
            let branch = match target.arch {
                Arch::AArch64 => format!("cbz x9, {}", default_label),
                Arch::X86_64 => format!("jz {}", default_label),
            };
            assert!(asm.contains(&branch), "{name}: {asm}");
            assert!(asm.contains(&format!("{}:", default_label)), "{name}: {asm}");
            assert!(asm.contains(&format!("{}:", done_label)), "{name}: {asm}");
            let default_at = asm.find(&format!("{}:", default_label)).unwrap();
            let done_at = asm.find(&format!("{}:", done_label)).unwrap();
            assert!(default_at < done_at, "{name}: {asm}");
            assert!(
                asm.find(&branch).unwrap() < asm.find("__rt_mixed_cast_int").unwrap(),
                "{name}: {asm}"
            );
        }
    }
}
