//! Purpose:
//! Emits wrapper functions that adapt interface dispatch entries to concrete method implementations.
//! Materializes receiver and argument forwarding for interface vtable calls.
//!
//! Called from:
//! - `crate::codegen_support::generate()` after class metadata collection
//!
//! Key details:
//! - Wrapper ABI order must match both interface slots and concrete method codegen signatures.

use std::collections::{HashMap, HashSet};

use crate::codegen_support::emit::Emitter;
use crate::names::{interface_method_wrapper_symbol, method_symbol};
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

use super::{abi, platform};
use super::value_boxing::emit_box_current_value_as_mixed;

/// Emits return wrappers for interface methods whose implementation returns a concrete type
/// but the interface signature declares `Mixed`. Wrappers box the concrete return value so the
/// interface dispatcher can handle heterogeneous return types.
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
) {
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
        );
    }
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
) {
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
            if !interface_method_needs_return_wrapper(
                interface_info,
                method_name,
                impl_class,
                classes,
            ) {
                continue;
            }

            let wrapper = interface_method_wrapper_symbol(
                class_info.class_id,
                interface_info.interface_id,
                method_name,
            );
            let implementation = method_symbol(impl_class, method_name);

            emitter.raw(".align 2");
            emitter.label_global(&wrapper);
            emitter.comment(&format!(
                "interface return wrapper {} implements {}::{}",
                class_name, interface_name, method_name
            ));
            match emitter.target.arch {
                platform::Arch::AArch64 => {
                    emitter.instruction("str x30, [sp, #-16]!");                // preserve the interface dispatch return address across nested calls
                    abi::emit_call_label(emitter, &implementation);
                    emit_box_current_value_as_mixed(
                        emitter,
                        &actual_sig.return_type.codegen_repr(),
                    );
                    emitter.instruction("ldr x30, [sp], #16");                  // restore the interface dispatch return address after boxing
                    emitter.instruction("ret");                                 // return the normalized mixed value to the interface caller
                }
                platform::Arch::X86_64 => {
                    emitter.instruction("sub rsp, 8");                          // align the SysV stack before the wrapper's nested calls
                    abi::emit_call_label(emitter, &implementation);
                    emit_box_current_value_as_mixed(
                        emitter,
                        &actual_sig.return_type.codegen_repr(),
                    );
                    emitter.instruction("add rsp, 8");                          // release the alignment padding before returning to the caller
                    emitter.instruction("ret");                                 // return the normalized mixed value to the interface caller
                }
            }
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
        && !matches!(actual_sig.return_type.codegen_repr(), PhpType::Mixed)
}
