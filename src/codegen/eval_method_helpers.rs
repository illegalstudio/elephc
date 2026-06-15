//! Purpose:
//! Emits user-assembly helpers that let libelephc-eval call public native
//! instance methods on runtime objects known to the current module.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - The cacheable runtime object cannot know user class ids, method symbols,
//!   or return types, so this bridge is emitted into the user assembly.
//! - This method-call slice supports public AOT methods with zero, one, or two
//!   non-by-ref scalar arguments and reports unsupported calls as runtime failure.

use std::collections::BTreeMap;

use crate::codegen::abi;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit_box_current_value_as_mixed;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::{Function, LocalKind, Module};
use crate::names::method_symbol;
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

/// Method metadata needed by eval method-call bridge dispatch.
#[derive(Clone)]
struct EvalMethodSlot {
    class_id: u64,
    class_name: String,
    method: String,
    impl_class: String,
    params: Vec<PhpType>,
    return_ty: PhpType,
}

const MAX_EVAL_METHOD_ARGS: usize = 2;

/// Emits eval method-call helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_method_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    if !module_uses_eval(module) {
        return;
    }
    let slots = collect_eval_method_slots(module);
    emit_method_call_helper(module, emitter, data, &slots);
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
    function
        .locals
        .iter()
        .any(|local| matches!(local.kind, LocalKind::EvalContext | LocalKind::EvalScope))
}

/// Collects public bridge-supported instance methods backed by emitted EIR symbols.
fn collect_eval_method_slots(module: &Module) -> Vec<EvalMethodSlot> {
    let emitted_methods = super::eir_class_method_keys(module);
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_method_slots(class_name, class_info, &emitted_methods, &mut slots);
    }
    slots
}

/// Adds bridge-supported public methods for one class.
fn collect_class_method_slots(
    class_name: &str,
    class_info: &ClassInfo,
    emitted_methods: &std::collections::HashSet<(String, String, bool)>,
    slots: &mut Vec<EvalMethodSlot>,
) {
    let mut methods = class_info.methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method, sig) in methods {
        if !method_is_public(class_info, method)
            || !method_signature_supported(sig)
            || !method_return_supported(&sig.return_type)
        {
            continue;
        }
        let impl_class = class_info
            .method_impl_classes
            .get(method)
            .map(String::as_str)
            .unwrap_or(class_name);
        if !emitted_methods.contains(&(impl_class.to_string(), method.clone(), false)) {
            continue;
        }
        slots.push(EvalMethodSlot {
            class_id: class_info.class_id,
            class_name: class_name.to_string(),
            method: method.clone(),
            impl_class: impl_class.to_string(),
            params: sig
                .params
                .iter()
                .map(|(_, ty)| ty.codegen_repr())
                .collect(),
            return_ty: sig.return_type.codegen_repr(),
        });
    }
}

/// Returns true when a method is publicly visible to runtime eval.
fn method_is_public(class_info: &ClassInfo, method: &str) -> bool {
    class_info
        .method_visibilities
        .get(method)
        .is_none_or(|visibility| matches!(visibility, Visibility::Public))
}

/// Returns true for method signatures supported by this first eval bridge slice.
fn method_signature_supported(sig: &crate::types::FunctionSig) -> bool {
    sig.params.len() <= MAX_EVAL_METHOD_ARGS
        && sig.variadic.is_none()
        && sig.ref_params.iter().all(|is_ref| !*is_ref)
        && sig
            .params
            .iter()
            .all(|(_, ty)| method_param_supported(ty))
}

/// Returns true for an eval-supplied method argument type supported by this bridge.
fn method_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
    )
}

/// Returns true for return storage shapes the bridge can box for eval.
fn method_return_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Void
            | PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Object(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
    )
}

/// Emits `__elephc_eval_value_method_call(Mixed*, name, len, MixedArray*) -> Mixed*`.
fn emit_method_call_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user method call ---");
    label_c_global(module, emitter, "__elephc_eval_value_method_call");
    match module.target.arch {
        Arch::AArch64 => emit_method_call_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_method_call_x86_64(module, emitter, data, slots),
    }
}

/// Emits the ARM64 method-call helper body.
fn emit_method_call_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
) {
    let fail_label = "__elephc_eval_value_method_call_fail";
    let done_label = "__elephc_eval_value_method_call_done";
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for inputs, object, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the requested method-name pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the requested method-name length
    emitter.instruction("str x3, [sp, #24]");                                   // save the boxed eval argument array
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // null Mixed receiver cannot dispatch a method
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", fail_label));                       // non-object receivers cannot dispatch instance methods
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for method calls
    emit_aarch64_method_dispatch(module, emitter, data, slots);
    emitter.instruction(&format!("b {}", fail_label));                          // no supported public method matched the request
    emit_aarch64_method_bodies(module, emitter, slots, done_label, fail_label);
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr");                                         // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed method result to Rust
}

/// Emits the x86_64 method-call helper body.
fn emit_method_call_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
) {
    let fail_label = "__elephc_eval_value_method_call_fail_x";
    let done_label = "__elephc_eval_value_method_call_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for name, length, object, args, and first argument
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the requested method-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the requested method-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the boxed eval argument array
    emitter.instruction("test rdi, rdi");                                       // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", fail_label));                         // null Mixed receiver cannot dispatch a method
    emitter.instruction("mov rax, rdi");                                        // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", fail_label));                        // non-object receivers cannot dispatch instance methods
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the unboxed object pointer for method calls
    emit_x86_64_method_dispatch(module, emitter, data, slots);
    emitter.instruction(&format!("jmp {}", fail_label));                        // no supported public method matched the request
    emit_x86_64_method_bodies(module, emitter, slots, done_label, fail_label);
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed method result to Rust
}

/// Emits ARM64 class-id and method-name dispatch for helper method bodies.
fn emit_aarch64_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let next_label = format!("__elephc_eval_method_next_{}", class_id);
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the unboxed object pointer before this class test
        emitter.instruction("ldr x9, [x9]");                                    // load the receiver class id for method dispatch
        abi::emit_load_int_immediate(emitter, "x10", class_id as i64);
        emitter.instruction("cmp x9, x10");                                     // compare receiver class id against this eval bridge class
        emitter.instruction(&format!("b.ne {}", next_label));                   // try the next class when ids differ
        for slot in class_slots {
            emit_aarch64_method_name_compare(module, emitter, data, slot);
        }
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-id and method-name dispatch for helper method bodies.
fn emit_x86_64_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let next_label = format!("__elephc_eval_method_next_{}_x", class_id);
        emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                   // reload the unboxed object pointer before this class test
        emitter.instruction("mov r11, QWORD PTR [r11]");                        // load the receiver class id for method dispatch
        abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
        emitter.instruction("cmp r11, r10");                                    // compare receiver class id against this eval bridge class
        emitter.instruction(&format!("jne {}", next_label));                    // try the next class when ids differ
        for slot in class_slots {
            emit_x86_64_method_name_compare(module, emitter, data, slot);
        }
        emitter.label(&next_label);
    }
}

/// Emits one ARM64 method-name comparison and branch to the matching body.
fn emit_aarch64_method_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalMethodSlot,
) {
    let (label, len) = data.add_string(slot.method.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested method-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested method-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_str_eq");                                      // compare requested method name with this public method
    emitter.instruction(&format!("cbnz x0, {}", method_body_label(module, slot))); // dispatch to the method body when the names match
}

/// Emits one x86_64 method-name comparison and branch to the matching body.
fn emit_x86_64_method_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalMethodSlot,
) {
    let (label, len) = data.add_string(slot.method.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload requested method-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested method-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_str_eq");                                    // compare requested method name with this public method
    emitter.instruction("test rax, rax");                                       // check whether the method names matched
    emitter.instruction(&format!("jne {}", method_body_label(module, slot)));   // dispatch to the method body when the names match
}

/// Emits ARM64 method-call bodies for every bridge-supported method.
fn emit_aarch64_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalMethodSlot],
    done_label: &str,
    fail_label: &str,
) {
    for slot in slots {
        emitter.label(&method_body_label(module, slot));
        emit_aarch64_validate_method_arg_count(module, emitter, slot, fail_label);
        let overflow_bytes = emit_aarch64_prepare_method_args(module, emitter, slot);
        abi::emit_call_label(emitter, &method_symbol(&slot.impl_class, &slot.method));
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        emit_box_method_result(module, emitter, slot);
        emitter.instruction(&format!("b {}", done_label));                      // return after boxing the native method result
    }
}

/// Emits x86_64 method-call bodies for every bridge-supported method.
fn emit_x86_64_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalMethodSlot],
    done_label: &str,
    fail_label: &str,
) {
    for slot in slots {
        emitter.label(&method_body_label(module, slot));
        emit_x86_64_validate_method_arg_count(module, emitter, slot, fail_label);
        let overflow_bytes = emit_x86_64_prepare_method_args(module, emitter, slot);
        abi::emit_call_label(emitter, &method_symbol(&slot.impl_class, &slot.method));
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        emit_box_method_result(module, emitter, slot);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the native method result
    }
}

/// Emits ARM64 arity validation for one method body.
fn emit_aarch64_validate_method_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalMethodSlot,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the eval argument array for arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    abi::emit_load_int_immediate(emitter, "x9", slot.params.len() as i64);
    emitter.instruction("cmp x0, x9");                                          // compare supplied eval argument count with the method signature
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject method dispatch when arity differs
}

/// Emits x86_64 arity validation for one method body.
fn emit_x86_64_validate_method_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalMethodSlot,
    fail_label: &str,
) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the eval argument array for arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    abi::emit_load_int_immediate(emitter, "r10", slot.params.len() as i64);
    emitter.instruction("cmp rax, r10");                                        // compare supplied eval argument count with the method signature
    emitter.instruction(&format!("jne {}", fail_label));                        // reject method dispatch when arity differs
}

/// Prepares ARM64 method ABI registers for the supported argument shapes.
fn emit_aarch64_prepare_method_args(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalMethodSlot,
) -> usize {
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the unboxed receiver as the first method argument
    abi::emit_push_result_value(emitter, &receiver_ty);
    for (index, param_ty) in slot.params.iter().enumerate() {
        emit_aarch64_load_eval_arg(module, emitter, index);
        emit_aarch64_cast_eval_arg(emitter, param_ty);
        abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
    }
    materialize_method_args(module, emitter, &receiver_ty, &slot.params)
}

/// Prepares x86_64 method ABI registers for the supported argument shapes.
fn emit_x86_64_prepare_method_args(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalMethodSlot,
) -> usize {
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the unboxed receiver as the first method argument
    abi::emit_push_result_value(emitter, &receiver_ty);
    for (index, param_ty) in slot.params.iter().enumerate() {
        emit_x86_64_load_eval_arg(module, emitter, index);
        emit_x86_64_cast_eval_arg(emitter, param_ty);
        abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
    }
    materialize_method_args(module, emitter, &receiver_ty, &slot.params)
}

/// Materializes the pushed receiver and eval arguments into the target method ABI.
fn materialize_method_args(
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
        _ => {}
    }
}

/// Boxes the current native method result as the Mixed cell expected by eval.
fn emit_box_method_result(module: &Module, emitter: &mut Emitter, slot: &EvalMethodSlot) {
    if slot.return_ty.codegen_repr() == PhpType::Void {
        let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
        abi::emit_call_label(emitter, &null_symbol);
    } else {
        emit_box_current_value_as_mixed(emitter, &slot.return_ty);
    }
}

/// Groups method slots by class id while preserving sorted class order.
fn grouped_slots(slots: &[EvalMethodSlot]) -> BTreeMap<u64, Vec<&EvalMethodSlot>> {
    let mut grouped = BTreeMap::new();
    for slot in slots {
        grouped.entry(slot.class_id).or_insert_with(Vec::new).push(slot);
    }
    grouped
}

/// Returns a platform-safe body label for a method slot.
fn method_body_label(module: &Module, slot: &EvalMethodSlot) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!(
        "__elephc_eval_method_{}_{}{}",
        label_fragment(&slot.class_name),
        label_fragment(&slot.method),
        suffix
    )
}

/// Converts arbitrary PHP metadata names into assembly-label-safe fragments.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// Emits a C-visible global label with target-specific symbol mangling.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    let symbol = module.target.extern_symbol(name);
    emitter.label_global(&symbol);
}
