//! Purpose:
//! Lowers immutable native property declaration callbacks through ordinary EIR.
//!
//! Called from:
//! - Explicit native-validation capability selection after physical class metadata is complete.
//!
//! Key details:
//! - Each native callback has exactly context and original-RHS C-ABI parameters.
//! - Both parameters are borrowed; one owned boxed candidate is returned.
//! - Callbacks have no PHP argument collectors, scalar-return markers or receiver dependency.

use crate::ir::{
    Builder, Function, FunctionParam, Immediate, IrHeapKind, IrType, LocalKind, Module,
    Op, Ownership, RuntimeCallTarget, Terminator,
};
use crate::types::PhpType;

/// Returns the stable internal name for one physical declaration's native callback.
pub(crate) fn callback_name(class_id: u64, slot: u32, is_static: bool) -> String {
    let storage = if is_static { "static" } else { "instance" };
    format!("__eir_reference_property_{class_id}_{slot}_{storage}")
}

/// Limits the first callback slice to scalar declarations, nullable scalars and their unions.
fn supports_declaration(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
        | PhpType::Void | PhpType::Never => true,
        PhpType::Union(members) => members.iter().all(supports_declaration),
        _ => false,
    }
}

/// Enables native validation and generates callbacks for the module's physical declarations.
///
/// Ordinary lowering does not need these callbacks. In particular, opaque eval must not
/// duplicate every scalar guard in its class catalog before native publishers use them.
/// Repeated selection preserves function identifiers and does not duplicate callbacks.
pub fn enable_native_reference_validation(module: &mut Module) -> Result<(), super::LoweringError> {
    module.required_runtime_features.native_reference_validation = true;
    let mut declarations = Vec::new();
    for info in module.class_infos.values() {
        for (index, (name, ty)) in info.properties.iter().enumerate() {
            if info.property_slot_is_declared(index, name)
                && info.property_slot_is_reference(index, name) && supports_declaration(ty)
            {
                declarations.push((info.class_id, index as u32, false));
            }
        }
        for (index, (name, ty)) in info.static_properties.iter().enumerate() {
            if info.declared_static_properties.contains(name) && supports_declaration(ty) {
                declarations.push((info.class_id, index as u32, true));
            }
        }
    }
    declarations.sort_unstable();
    declarations.dedup();
    for (class_id, slot, is_static) in declarations {
        let name = callback_name(class_id, slot, is_static);
        if !module.functions.iter().any(|function| function.name == name) {
            module.add_function(lower_callback(class_id, slot, is_static));
        }
    }
    crate::ir::validate_module(module)
        .map_err(|error| super::validation_context::describe(module, error))
}

/// Creates a native two-parameter function returning exactly one independently owned candidate.
fn lower_callback(class_id: u64, slot: u32, is_static: bool) -> Function {
    let boxed = IrType::Heap(IrHeapKind::Mixed);
    let mut function = Function::new(callback_name(class_id, slot, is_static), boxed, PhpType::Mixed);
    function.flags.is_internal = true;
    function.flags.is_synthetic = true;
    function.flags.is_native_reference_validator = true;
    for (name, ir_type, php_type) in [
        ("context", IrType::I64, PhpType::Int), ("original", boxed, PhpType::Mixed),
    ] {
        function.params.push(FunctionParam {
            name: name.into(), ir_type, php_type: php_type.clone(), by_ref: false, variadic: false,
        });
        let slot = function.add_local(Some(name.into()), ir_type, php_type, LocalKind::BorrowedTemp);
        function.no_epilogue_cleanup_slots.insert(slot);
    }
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let mut operands = Vec::new();
    for (index, ir_type, php_type) in [
        (0, IrType::I64, PhpType::Int), (1, boxed, PhpType::Mixed),
    ] {
        let ownership = if index == 0 { Ownership::NonHeap } else { Ownership::Borrowed };
        operands.push(builder.emit(
            Op::LoadLocal, Vec::new(),
            Some(Immediate::LocalSlot(crate::ir::LocalSlotId::from_raw(index))),
            ir_type, php_type, ownership,
        ).expect("validator parameter load produces a value"));
    }
    let candidate = builder.emit(
        Op::RuntimeCall, operands,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ReferencePropertyCoerce {
            class_id, slot, is_static,
        })), boxed, PhpType::Mixed, Ownership::Owned,
    ).expect("reference declaration coercion produces a candidate");
    builder.terminate(Terminator::Return { value: Some(candidate) });
    function
}
