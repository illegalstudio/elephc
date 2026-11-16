//! Purpose:
//! Emits immutable native property validators and physical binding rows from typed EIR callbacks.
//!
//! Called from:
//! - Final user assembly emission when native reference validation is selected.
//!
//! Key details:
//! - Descriptors start with the native two-argument callback entry, followed by declaration identity.
//! - Instance rows name physical byte offsets, preserving hidden parent-private storage.
//! - Borrowed cells and static declarations have descriptors but no owned-instance binding row.

use std::collections::BTreeMap;

use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, IrHeapKind, IrType, Module, RuntimeCallTarget};

/// Names immutable metadata for one physical declaration independently of a live receiver.
fn descriptor_symbol(class_id: u64, slot: u32, is_static: bool) -> String {
    let storage = if is_static { "static" } else { "instance" };
    format!("_native_reference_declaration_{class_id}_{slot}_{storage}")
}

/// Emits only selected native callback metadata, rejecting entries with a PHP argument ABI.
pub(in crate::codegen) fn emit_native_reference_metadata(module: &Module) -> Result<String> {
    if !module.required_runtime_features.native_reference_validation { return Ok(String::new()); }
    let mut descriptors = BTreeMap::new();
    let mut rows: BTreeMap<u64, Vec<(usize, String)>> = BTreeMap::new();
    for function in module.functions.iter().filter(|function| function.flags.is_native_reference_validator) {
        let boxed = IrType::Heap(IrHeapKind::Mixed);
        if function.signature.is_some() || function.params.len() != 2
            || function.params[0].ir_type != IrType::I64 || function.params[1].ir_type != boxed
            || function.params.iter().any(|parameter| parameter.by_ref || parameter.variadic)
            || function.flags.is_main || function.flags.is_method || function.flags.is_closure
            || function.flags.is_fiber_wrapper || function.flags.is_callback_wrapper
            || function.flags.is_runtime_callable_invoker || function.flags.by_ref_return
            || function.return_type != boxed
        {
            return Err(CodegenIrError::invalid_module("native reference callback has a PHP calling convention"));
        }
        let declarations = function.instructions.iter().filter_map(|inst| match &inst.immediate {
            Some(Immediate::RuntimeCall(RuntimeCallTarget::ReferencePropertyCoerce { class_id, slot, is_static })) =>
                Some((*class_id, *slot, *is_static)),
            _ => None,
        }).collect::<Vec<_>>();
        let [declaration] = declarations.as_slice() else {
            return Err(CodegenIrError::invalid_module("native reference callback must identify exactly one declaration"));
        };
        let (class_id, slot, is_static) = *declaration;
        let info = module.class_infos.values().find(|info| info.class_id == class_id)
            .ok_or_else(|| CodegenIrError::invalid_module("native reference callback has an unknown class"))?;
        let properties = if is_static { &info.static_properties } else { &info.properties };
        if properties.get(slot as usize).is_none() {
            return Err(CodegenIrError::invalid_module("native reference callback has an unknown property"));
        }
        let symbol = descriptor_symbol(class_id, slot, is_static);
        if descriptors.insert((class_id, slot, is_static),
            (symbol.clone(), crate::names::function_symbol(&function.name))).is_some()
        {
            return Err(CodegenIrError::invalid_module("duplicate native property validator"));
        }
        if !is_static && info.property_slot_owns_reference(slot as usize) {
            rows.entry(class_id).or_default().push((8 + slot as usize * 16, symbol));
        }
    }
    let class_count = module.class_infos.values().map(|info| info.class_id).max()
        .map_or(Some(0), |id| id.checked_add(1))
        .ok_or_else(|| CodegenIrError::invalid_module("native reference class table overflow"))?;
    let mut output = String::from(".data\n");
    emit_words(&mut output, "_native_reference_source_class_count", &[class_count.to_string()]);
    let pointers = (0..class_count).map(|id| if rows.contains_key(&id) {
        format!("_native_reference_source_rows_{id}")
    } else { "0".into() }).collect::<Vec<_>>();
    emit_words(&mut output, "_native_reference_source_class_ptrs", &pointers);
    for ((class_id, slot, is_static), (symbol, entry)) in descriptors {
        emit_words(&mut output, &symbol,
            &[entry, class_id.to_string(), slot.to_string(), u8::from(is_static).to_string()]);
    }
    for (class_id, mut bindings) in rows {
        bindings.sort_by_key(|(offset, _)| *offset);
        let mut words = vec![bindings.len().to_string()];
        for (offset, descriptor) in bindings { words.extend([offset.to_string(), descriptor]); }
        emit_words(&mut output, &format!("_native_reference_source_rows_{class_id}"), &words);
    }
    Ok(output)
}

/// Keeps pointer and declaration records eight-byte aligned on every supported object format.
fn emit_words(output: &mut String, symbol: &str, words: &[String]) {
    output.push_str(&format!(".p2align 3\n.globl {symbol}\n{symbol}:\n"));
    for word in words { output.push_str(&format!("    .quad {word}\n")); }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hidden physical fields and static descriptors preserve distinct native callback identities.
    #[test]
    fn native_reference_metadata_covers_physical_slots_on_every_target() {
        let source = r#"<?php
class NativeBindingRowsBase {
    private int $value = 1;
    public function &baseReference(): int { return $this->value; }
}
class NativeBindingRowsChild extends NativeBindingRowsBase {
    public string $value = 'child';
    public static string $label = 'label';
}
class NativeBindingRowsBorrowed {
    public function __construct(public int &$value) {}
}
$value = 1; $borrowed = new NativeBindingRowsBorrowed($value);
$object = new NativeBindingRowsChild();
$a =& $object->baseReference(); $b =& $object->value;
echo $a, $b, $borrowed->value, NativeBindingRowsChild::$label;
"#;
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut module = crate::ir_lower::tests::lower_source_at_for_target(
                source, std::path::Path::new("main.php"), std::path::Path::new("."),
                crate::codegen::platform::Target::parse(target).unwrap(),
            );
            assert!(emit_native_reference_metadata(&module).unwrap().is_empty());
            crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
            let child = &module.class_infos["NativeBindingRowsChild"];
            let borrowed = &module.class_infos["NativeBindingRowsBorrowed"];
            assert!(!borrowed.property_slot_owns_reference(0));
            let output = emit_native_reference_metadata(&module).unwrap();
            let label = format!("_native_reference_source_rows_{}:\n", child.class_id);
            let row = output.split_once(&label).unwrap().1;
            assert!(row.starts_with(&format!("    .quad 2\n    .quad 8\n    .quad {}\n    .quad 24\n    .quad {}\n",
                descriptor_symbol(child.class_id, 0, false), descriptor_symbol(child.class_id, 1, false))),
                "{target}: hidden and visible property bindings remain physically distinct");
            assert!(output.contains(&format!("{}:\n", descriptor_symbol(child.class_id, 0, true))),
                "{target}: static declarations retain their own descriptor namespace");
            assert!(!output.contains(&format!("_native_reference_source_rows_{}:", borrowed.class_id)),
                "{target}: borrowed frames cannot publish owned native cell bindings");
            let callback = module.functions.iter_mut().find(|function|
                function.flags.is_native_reference_validator).unwrap();
            callback.params.pop();
            assert!(emit_native_reference_metadata(&module).is_err(),
                "{target}: descriptor entries cannot silently acquire PHP calling conventions");
        }
    }
}
