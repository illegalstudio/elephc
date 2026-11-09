//! Purpose:
//! Emits wrappers that adapt interface entries to concrete method implementations.
//! Owns source-to-physical argument forwarding and concrete-to-Mixed return boxing.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` after class metadata collection.
//!
//! Key details:
//! - Wrapper ABI order must match both interface slots and concrete method codegen signatures.
//! - One wrapper performs both adaptations when a route needs both.

use std::collections::{HashMap, HashSet};

use crate::codegen_support::emit::Emitter;
use crate::names::{interface_method_wrapper_symbol, method_symbol};
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

use super::source_method_adapters::{self, MethodAbiPlan, MethodKind};

/// Emits wrappers for interface argument or return ABI differences.
///
/// # Arguments
/// * `emitter` - Assembly emitter
/// * `interfaces` - Map of interface name → metadata including method order and signatures
/// * `classes` - Map of class name → metadata including implemented interfaces and method signatures
/// * `emitted_class_names` - Optional filter; only classes in this set receive wrappers
pub(crate) fn emit_interface_return_wrappers(
    emitter: &mut Emitter,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: &HashMap<String, ClassInfo>,
    emitted_class_names: Option<&HashSet<String>>,
) -> Result<(), String> {
    let mut sorted_classes: Vec<(&String, &ClassInfo)> = classes
        .iter()
        .filter(|(class_name, _)| {
            emitted_class_names
                .is_none_or(|declared| declared.contains(*class_name))
        })
        .collect();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);

    for (class_name, class_info) in sorted_classes {
        emit_class_interface_return_wrappers(
            emitter,
            class_name,
            class_info,
            interfaces,
            classes,
        )?;
    }
    Ok(())
}

/// Emits all return wrappers for a single class's interface implementations.
///
/// Iterates over every interface the class declares it implements, then each method in the
/// interface's method order. For each method whose implementation returns a concrete type but the
/// interface declares `Mixed`, emits a wrapper that boxes the return value before returning to the
/// interface caller.
///
/// # Arguments
/// * `emitter` - Assembly emitter
/// * `class_name` - Name of the class receiving wrappers
/// * `class_info` - Class metadata (interfaces, method impl classes, class ID)
/// * `interfaces` - Map of all known interfaces
/// * `classes` - Map of all known classes with their method signatures
fn emit_class_interface_return_wrappers(
    emitter: &mut Emitter,
    class_name: &str,
    class_info: &ClassInfo,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: &HashMap<String, ClassInfo>,
) -> Result<(), String> {
    for interface_name in &class_info.interfaces {
        let Some(interface_info) = interfaces.get(interface_name) else {
            continue;
        };
        for method_name in &interface_info.method_order {
            let Some(impl_class) = class_info.method_impl_classes.get(method_name) else {
                continue;
            };
            let Some(actual_sig) = classes
                .get(impl_class)
                .and_then(|impl_info| impl_info.methods.get(method_name))
            else {
                continue;
            };
            let Some(interface_sig) = interface_info.methods.get(method_name) else {
                continue;
            };
            let needs_return_wrapper = interface_method_needs_return_wrapper(
                interface_info,
                method_name,
                impl_class,
                classes,
            );
            let abi_plan = source_method_adapters::plan_method_abi(interface_sig, actual_sig)
                .map_err(|error| format!(
                    "{error}: {interface_name}::{method_name} implemented by {impl_class}::{method_name}; caller {:?} -> {:?}, physical {:?} -> {:?}",
                    interface_sig.params, interface_sig.return_type,
                    actual_sig.params, actual_sig.return_type,
                ))?;
            if !needs_return_wrapper && abi_plan == MethodAbiPlan::Direct {
                continue;
            }
            if abi_plan == MethodAbiPlan::OptionalDefaults {
                // EIR owns default expressions, argument planning, and exceptional cleanup.
                continue;
            }

            let wrapper = interface_method_wrapper_symbol(
                class_info.class_id,
                interface_info.interface_id,
                method_name,
            );
            let implementation = method_symbol(impl_class, method_name);

            source_method_adapters::emit_method_adapter(
                emitter,
                &wrapper,
                &implementation,
                MethodKind::Instance,
                interface_sig,
                actual_sig,
                needs_return_wrapper,
            )?;
        }
    }
    // `Countable::count()` is declared as int, but `#[ReturnTypeWillChange]` implementors may
    // return any scalar or mixed value. PHP's count() casts that result, so the dense count
    // table needs an adapter that applies the matching int coercion after the implementation.
    if class_info.interfaces.iter().any(|name| name.eq_ignore_ascii_case("Countable")) {
        if let Some(sig) = class_info.methods.get("count") {
            match sig.return_type.codegen_repr() {
                PhpType::Str => emit_count_int_wrapper(emitter, class_name, class_info, "__rt_str_to_int"),
                PhpType::Float => emit_count_int_wrapper(emitter, class_name, class_info, "__rt_php_float_to_int"),
                PhpType::Mixed => emit_count_int_wrapper(emitter, class_name, class_info, "__rt_mixed_cast_int"),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Emits a Countable adapter that calls `count()` and applies PHP's int coercion.
fn emit_count_int_wrapper(
    emitter: &mut Emitter,
    class_name: &str,
    class_info: &ClassInfo,
    conversion: &str,
) {
    let implementation = class_info
        .method_impl_classes
        .get("count")
        .map(|owner| method_symbol(owner, "count"))
        .unwrap_or_else(|| method_symbol(class_name, "count"));
    let wrapper = format!("_count_int_wrap_{}", class_info.class_id);
    // String and mixed results are owned by this adapter; the conversion helper only reads them,
    // so the payload is released after the integer result has been saved.
    let owned_payload = match conversion {
        "__rt_str_to_int" => Some("x1"),
        "__rt_mixed_cast_int" => Some("x0"),
        _ => None,
    };
    emitter.blank();
    emitter.comment("--- Countable::count int-coercion adapter ---");
    emitter.label_global(&wrapper);
    match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-32]!");                   // preserve wrapper return address and result slots
            emitter.instruction("mov x29, sp");                                 // establish the adapter frame
            emitter.instruction(&format!("bl {implementation}"));               // call the typed count()
            if let Some(payload) = owned_payload {
                emitter.instruction(&format!("str {payload}, [sp, #16]"));      // save the owned payload across the coercion
            }
            emitter.instruction(&format!("bl {conversion}"));                   // apply PHP's int coercion
            if conversion == "__rt_php_float_to_int" {
                emitter.instruction("mov x0, x9");                              // move the float conversion result into the int result register
            }
            emitter.instruction("str x0, [sp, #24]");                           // save the coerced integer result
            if owned_payload.is_some() {
                emitter.instruction("ldr x0, [sp, #16]");                       // reload the owned payload
                emitter.instruction("bl __rt_decref_any");                      // release the count() result after coercion
                emitter.instruction("ldr x0, [sp, #24]");                       // restore the coerced integer result
            }
            emitter.instruction("ldp x29, x30, [sp], #32");                     // restore the wrapper frame
            emitter.instruction("ret");                                         // return the integer count
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the wrapper frame
            emitter.instruction("mov rbp, rsp");                                // establish the adapter frame
            emitter.instruction("sub rsp, 32");                                 // reserve slots for the payload and coerced result
            emitter.instruction(&format!("call {implementation}"));             // call the typed count()
            if owned_payload.is_some() {
                emitter.instruction("mov QWORD PTR [rbp - 8], rax");            // save the owned payload across the coercion
            }
            emitter.instruction(&format!("call {conversion}"));                 // apply PHP's int coercion
            if conversion == "__rt_php_float_to_int" {
                emitter.instruction("mov rax, r11");                            // move the float conversion result into the int result register
            }
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // save the coerced integer result
            if owned_payload.is_some() {
                emitter.instruction("mov rax, QWORD PTR [rbp - 8]");            // reload the owned payload
                emitter.instruction("call __rt_decref_any");                    // release the count() result after coercion
                emitter.instruction("mov rax, QWORD PTR [rbp - 16]");           // restore the coerced integer result
            }
            emitter.instruction("mov rsp, rbp");                                // restore the wrapper frame
            emitter.instruction("pop rbp");                                     // restore the caller frame
            emitter.instruction("ret");                                         // return the integer count
        }
    }
}

/// Returns true if an interface method implementation needs a return wrapper.
///
/// A wrapper is required when the interface declares a `Mixed` return type but the concrete
/// implementation returns a narrower type. The wrapper bridges the ABI gap by boxing the concrete
/// value into a `Mixed` cell so the interface vtable dispatch can handle it uniformly.
///
/// # Arguments
/// * `interface_info` - Interface metadata including the declared method signature
/// * `method_name` - Name of the method to check
/// * `impl_class` - Name of the class that implements the method
/// * `classes` - Map of all known classes with their method signatures
fn interface_method_needs_return_wrapper(
    interface_info: &InterfaceInfo,
    method_name: &str,
    impl_class: &str,
    classes: &HashMap<String, ClassInfo>,
) -> bool {
    let Some(interface_sig) = interface_info.methods.get(method_name) else {
        return false;
    };
    let Some(actual_sig) = classes
        .get(impl_class)
        .and_then(|class_info| class_info.methods.get(method_name))
    else {
        return false;
    };

    matches!(interface_sig.return_type.codegen_repr(), PhpType::Mixed)
        && !actual_sig.by_ref_return
        && !matches!(actual_sig.return_type.codegen_repr(), PhpType::Mixed)
}
