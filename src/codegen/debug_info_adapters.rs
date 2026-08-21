//! Purpose:
//! Emits uniform runtime adapters for dynamically dispatched `__debugInfo()` methods.
//! Each adapter converts the compiled method ABI into one owned boxed `Mixed` result.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` before class debug-info tables are emitted.
//!
//! Key details:
//! - The object receiver is borrowed and remains owned by the runtime walker.
//! - Raw array/hash returns are transferred into a fresh Mixed cell; Mixed/union returns
//!   already use that ABI and pass through unchanged.

use std::collections::HashMap;

use crate::codegen::{abi, emit_box_current_owned_value_as_mixed};
use crate::codegen_support::emit::Emitter;
use crate::ir::Module;
use crate::names::{debug_info_adapter_symbol, method_symbol, php_symbol_key};
use crate::types::{ClassInfo, PhpType};

/// Emits one boxed-Mixed `__debugInfo()` adapter for every runtime class that exposes it.
pub(super) fn emit_debug_info_adapters(
    module: &Module,
    classes: &HashMap<String, ClassInfo>,
    emitter: &mut Emitter,
) {
    let method_key = php_symbol_key("__debugInfo");
    let mut sorted_classes = classes.iter().collect::<Vec<_>>();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);

    for (class_name, class_info) in sorted_classes {
        let compiler_projection = class_info.method_impl_classes.contains_key(&method_key)
            && !class_info.methods.contains_key(&method_key);
        let impl_class = if compiler_projection {
            class_name
        } else {
            let Some(impl_class) = class_info.method_impl_classes.get(&method_key) else {
                continue;
            };
            impl_class
        };
        let return_type = if compiler_projection {
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            }
        } else {
            let Some(signature) = module
                .class_infos
                .get(impl_class)
                .and_then(|impl_info| impl_info.methods.get(&method_key))
                .or_else(|| class_info.methods.get(&method_key))
            else {
                continue;
            };
            signature.return_type.codegen_repr()
        };

        let adapter = debug_info_adapter_symbol(class_info.class_id);
        emitter.blank();
        emitter.comment(&format!(
            "--- runtime __debugInfo adapter for class id {} ---",
            class_info.class_id
        ));
        emitter.label_global(&adapter);
        abi::emit_frame_prologue(emitter, 16);
        abi::emit_call_label(emitter, &method_symbol(impl_class, &method_key));
        if !matches!(return_type, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_owned_value_as_mixed(emitter, &return_type);
        }
        abi::emit_frame_restore(emitter, 16);
        abi::emit_return(emitter);
    }
}
