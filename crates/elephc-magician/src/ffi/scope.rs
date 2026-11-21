//! Purpose:
//! Exports materialized eval activation-scope handle operations.
//! Scope entries carry runtime cell pointers plus dirty, ownership, unset, and
//! by-reference metadata used by generated code around eval barriers.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_scope_*` symbols.
//!
//! Key details:
//! - Owned runtime cells are released when a scope is freed or overwritten.
//! - Global aliases store source-target names rather than copying values.

use super::util::{
    abi_name_to_string, release_owned_scope_cells, release_scope_cell, scope_entry_abi_flags,
};
use crate::abi::{ElephcEvalScope, SCOPE_FLAG_GLOBAL_ALIAS, SCOPE_FLAG_NATIVE_DETACHED,
    SCOPE_FLAG_NATIVE_SNAPSHOT, SCOPE_FLAG_OWNED};
use crate::errors::EvalStatus;
use crate::scope::ScopeCellOwnership;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Installs or clears the type of a native reference snapshot before interpreted assignments.
///
/// # Safety
/// The scope must be live and the supplied name and optional type must be readable UTF-8 ranges.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_native_type_v1(
    scope: *mut ElephcEvalScope, name_ptr: *const u8, name_len: u64,
    type_ptr: *const u8, type_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        let Some(scope) = scope.as_mut() else { return EvalStatus::RuntimeFatal.code(); };
        let Ok(name) = abi_name_to_string(name_ptr, name_len) else { return EvalStatus::RuntimeFatal.code(); };
        let ty = if type_len == 0 { None } else {
            let Some(ty) = super::native_methods::native_callable_type_from_abi(
                type_ptr, type_len, super::native_methods::NativeCallableTypePosition::LocalBinding,
            ) else { return EvalStatus::RuntimeFatal.code(); };
            Some(ty)
        };
        scope.set_native_storage_type(name, ty);
        EvalStatus::Ok.code()
    }).unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Publishes a checked native producer type separately from physical snapshot constraints.
///
/// # Safety
/// The scope is live and the name and type are readable UTF-8 ranges for their supplied lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_binding_type_v1(
    scope: *mut ElephcEvalScope, name_ptr: *const u8, name_len: u64,
    type_ptr: *const u8, type_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        let Some(scope) = scope.as_mut() else { return EvalStatus::RuntimeFatal.code(); };
        let Ok(name) = abi_name_to_string(name_ptr, name_len) else { return EvalStatus::RuntimeFatal.code(); };
        let Some(ty) = super::native_methods::native_callable_type_from_abi(
            type_ptr, type_len, super::native_methods::NativeCallableTypePosition::LocalBinding,
        ) else { return EvalStatus::RuntimeFatal.code(); };
        scope.set_native_binding_type(name, ty);
        EvalStatus::Ok.code()
    }).unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Stages checked logical evidence independently from physical storage or inferred local constraints.
///
/// # Safety
/// The scope and both UTF-8 ranges remain live for this call.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_source_type_v1(
    scope: *mut ElephcEvalScope, name_ptr: *const u8, name_len: u64,
    type_ptr: *const u8, type_len: u64, flags: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        let Some(scope) = scope.as_mut() else { return EvalStatus::RuntimeFatal.code(); };
        let Ok(name) = abi_name_to_string(name_ptr, name_len) else { return EvalStatus::RuntimeFatal.code(); };
        let Some(ty) = super::native_methods::native_callable_type_from_abi(
            type_ptr, type_len, super::native_methods::NativeCallableTypePosition::LocalBinding,
        ) else { return EvalStatus::RuntimeFatal.code(); };
        use elephc_semantics::native_local_contract::{SOURCE_DECLARED, SOURCE_ARRAY_CONTENTS, SOURCE_FLAGS};
        if flags & !SOURCE_FLAGS != 0 { return EvalStatus::RuntimeFatal.code(); }
        scope.prepare_native_source(name, ty, flags & SOURCE_DECLARED != 0, flags & SOURCE_ARRAY_CONTENTS != 0);
        EvalStatus::Ok.code()
    }).unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Copies a producer contract onto independent canonical storage without running PHP.
///
/// # Safety
/// Scope and name ranges are live; reference is the fresh destination's canonical PHPRef.
/// The caller keeps that allocation alive until its weak contract registration completes.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_copy_binding_type_v1(
    scope: *mut ElephcEvalScope,
    metadata: *const elephc_semantics::native_local_contract::BindingCopyV1,
    reference: *mut RuntimeCell,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        let Some(scope) = scope.as_mut() else { return EvalStatus::RuntimeFatal.code(); };
        let Some(metadata) = metadata.as_ref() else { return EvalStatus::RuntimeFatal.code(); };
        if metadata.version != elephc_semantics::native_local_contract::ABI_VERSION { return EvalStatus::RuntimeFatal.code(); }
        if reference.is_null() { return EvalStatus::RuntimeFatal.code(); }
        let Ok(source) = abi_name_to_string(metadata.source_ptr, metadata.source_len as u64) else { return EvalStatus::RuntimeFatal.code(); };
        let Ok(target) = abi_name_to_string(metadata.target_ptr, metadata.target_len as u64) else { return EvalStatus::RuntimeFatal.code(); };
        let declared = match copied_local_type(scope, &source, &target, metadata.descriptor.as_ref()) {
            Ok(declared) => declared,
            Err(status) => return status.code(),
        };
        scope.infer_local_type(target, declared.clone());
        if !declared.variants().contains(&crate::eval_ir::EvalParameterTypeVariant::Mixed) {
            let reference = RuntimeCellHandle::from_raw(reference);
            for pointer in crate::context::live_native_reference_contexts() {
                if let Some(context) = pointer.as_mut() {
                    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals) {
                        context.bind_local_reference_types(reference, &[declared.clone()]);
                    }
                }
            }
        }
        EvalStatus::Ok.code()
    }).unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Selects one original producer independently from mutable cell representation.
///
/// # Safety
/// A supplied descriptor is immutable and its type byte range remains readable for this call.
unsafe fn copied_local_type(scope: &ElephcEvalScope, source: &str, target: &str,
    descriptor: Option<&elephc_semantics::native_local_contract::LocalTypeDescriptorV1>,
) -> Result<crate::eval_ir::EvalParameterType, EvalStatus> {
    if descriptor.is_some_and(|descriptor|
        descriptor.version != elephc_semantics::native_local_contract::ABI_VERSION) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(declared) = scope.local_type(source).or_else(|| scope.inferred_local_type(source))
        .or_else(|| scope.native_binding_type(source)).or_else(|| scope.native_binding_type(target)) {
        return Ok(declared.clone());
    }
    match descriptor {
        Some(descriptor) => super::native_methods::native_callable_type_from_abi(
            descriptor.type_ptr, descriptor.type_len as u64,
            super::native_methods::NativeCallableTypePosition::LocalBinding,
        ).ok_or(EvalStatus::RuntimeFatal),
        None => Ok(crate::eval_ir::EvalParameterType::new(
            vec![crate::eval_ir::EvalParameterTypeVariant::Mixed], false)),
    }
}

/// Allocates a materialized activation scope handle for generated code.
#[no_mangle]
pub extern "C" fn __elephc_eval_scope_new() -> *mut ElephcEvalScope {
    Box::into_raw(Box::new(ElephcEvalScope::new()))
}

/// Frees an activation scope completely and transfers any accumulated Throwable to native code.
///
/// # Safety
/// `scope` must be null or a pointer returned by `__elephc_eval_scope_new`
/// that has not already been freed.
#[export_name = "__elephc_eval_scope_free_v2"]
pub unsafe extern "C" fn __elephc_eval_scope_free(scope: *mut ElephcEvalScope) -> *mut RuntimeCell {
    if !scope.is_null() {
        let mut scope = Box::from_raw(scope);
        let thrown = release_owned_scope_cells(&mut scope);
        drop(scope);
        return thrown.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
    }
    std::ptr::null_mut()
}

/// Stores a named runtime cell in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`; names must be UTF-8 variable names.
/// `throwable` must point to writable pointer storage for an owned escaping exception.
#[export_name = "__elephc_eval_scope_set_v2"]
pub unsafe extern "C" fn __elephc_eval_scope_set(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    cell: *mut RuntimeCell,
    flags: u32,
    throwable: *mut *mut RuntimeCell,
) -> i32 {
    if throwable.is_null() { return EvalStatus::RuntimeFatal.code(); }
    *throwable = std::ptr::null_mut();
    if cell.is_null() {
        return __elephc_eval_scope_unset(scope, name_ptr, name_len, throwable);
    }
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let ownership = if flags & SCOPE_FLAG_OWNED != 0 {
        ScopeCellOwnership::Owned
    } else {
        ScopeCellOwnership::Borrowed
    };
    #[cfg(not(test))]
    if flags & crate::abi::SCOPE_FLAG_ASSIGN_REFERENCE != 0 && flags & SCOPE_FLAG_NATIVE_SNAPSHOT == 0 {
        if let Some((status, thrown)) = crate::runtime_hooks::scope_assignment::assign_existing_reference(
            scope, &name, RuntimeCellHandle::from_raw(cell), ownership,
        ) {
            *throwable = thrown.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
            return status.code();
        }
    }
    let replaced = scope.set_from_aot(name.clone(), RuntimeCellHandle::from_raw(cell), ownership);
    if flags & SCOPE_FLAG_NATIVE_SNAPSHOT != 0 {
        scope.mark_name_clean(&name);
        scope.clear_native_name_detachment(&name);
    }
    if let Some(replaced) = replaced {
        if let Some(thrown) = release_scope_cell(replaced) {
            *throwable = thrown.as_ptr();
            return EvalStatus::UncaughtThrowable.code();
        }
    }
    EvalStatus::Ok.code()
}

/// Looks up a named runtime cell in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. Output pointers may be null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_get(
    scope: *const ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    out_cell: *mut *mut RuntimeCell,
    out_flags: *mut u32,
) -> i32 {
    let Some(scope) = scope.as_ref() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let entry = scope.entry(&name);
    if !out_cell.is_null() {
        *out_cell = entry
            .filter(|entry| entry.flags().is_visible())
            .map(|entry| entry.cell().as_ptr())
            .unwrap_or(std::ptr::null_mut());
    }
    if !out_flags.is_null() {
        *out_flags = entry.map(scope_entry_abi_flags).unwrap_or(0);
        if scope.native_name_detached(&name) { *out_flags |= SCOPE_FLAG_NATIVE_DETACHED; }
        if scope.is_global_alias(&name) { *out_flags |= SCOPE_FLAG_GLOBAL_ALIAS; }
    }
    EvalStatus::Ok.code()
}

/// Marks a named runtime cell as unset in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`; names must be UTF-8 variable names.
/// `throwable` must point to writable pointer storage for an owned escaping exception.
#[export_name = "__elephc_eval_scope_unset_v2"]
pub unsafe extern "C" fn __elephc_eval_scope_unset(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    throwable: *mut *mut RuntimeCell,
) -> i32 {
    if throwable.is_null() { return EvalStatus::RuntimeFatal.code(); }
    *throwable = std::ptr::null_mut();
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let replaced = scope.unset(name.clone());
    #[cfg(not(test))]
    let thrown = {
        let pending = crate::runtime_hooks::scope_assignment::release_origins(scope, Some(&name));
        crate::runtime_hooks::release::release_native_cells(replaced, pending)
    };
    #[cfg(test)]
    let thrown = replaced.and_then(release_scope_cell);
    if let Some(thrown) = thrown {
        *throwable = thrown.as_ptr();
        return EvalStatus::UncaughtThrowable.code();
    }
    EvalStatus::Ok.code()
}

/// Marks a local eval-scope variable as an alias of a program-global variable.
///
/// # Safety
/// `scope` must be a valid eval scope handle. Name pointers must be readable
/// for their matching lengths when the length is greater than zero.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_mark_global_alias(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    global_name_ptr: *const u8,
    global_name_len: u64,
) -> i32 {
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(global_name) = abi_name_to_string(global_name_ptr, global_name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    scope.mark_global_alias_to(name, global_name);
    EvalStatus::Ok.code()
}

/// Clears dirty flags for every entry in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle allocated by the eval bridge.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_clear_dirty(scope: *mut ElephcEvalScope) -> i32 {
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    scope.mark_all_clean();
    EvalStatus::Ok.code()
}

#[cfg(test)]
mod copy_origin_tests {
    use super::*;
    use crate::eval_ir::{EvalParameterType, EvalParameterTypeVariant};
    use elephc_semantics::native_local_contract::{LocalTypeDescriptorV1, ABI_VERSION};

    /// Pure native descriptors retain null/union contracts while explicit Mixed sources stay open.
    #[test]
    fn copied_scope_producer_types_preserve_null_unions_and_open_origins() {
        let mut scope = ElephcEvalScope::new();
        for spelling in ["null", "int|float", "string"] {
            let descriptor = LocalTypeDescriptorV1 { callback: 0, class_id: 0, property_slot: 0,
                is_static: 0, version: ABI_VERSION, type_ptr: spelling.as_ptr(), type_len: spelling.len() };
            let copied = unsafe { copied_local_type(&scope, "source", "copy", Some(&descriptor)) }.unwrap();
            let expected = super::super::native_methods::native_callable_type_from_abi(
                spelling.as_ptr(), spelling.len() as u64,
                super::super::native_methods::NativeCallableTypePosition::LocalBinding,
            ).unwrap();
            assert_eq!(copied, expected);
        }
        let closed = EvalParameterType::new(vec![EvalParameterTypeVariant::Float], false);
        scope.infer_local_type("source".into(), closed.clone());
        let copied = unsafe { copied_local_type(&scope, "source", "copy", None) }.unwrap();
        scope.infer_local_type("copy".into(), copied);
        scope.unset("source");
        scope.infer_local_type("source".into(), EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false));
        assert_eq!(unsafe { copied_local_type(&scope, "copy", "second", None) }.unwrap(), closed);
        let open = EvalParameterType::new(vec![EvalParameterTypeVariant::Mixed], false);
        scope.infer_local_type("open".into(), open.clone());
        let text = "float";
        let mut descriptor = LocalTypeDescriptorV1 { callback: 0, class_id: 0, property_slot: 0,
            is_static: 0, version: ABI_VERSION, type_ptr: text.as_ptr(), type_len: text.len() };
        assert_eq!(unsafe { copied_local_type(&scope, "open", "new", Some(&descriptor)) }.unwrap(), open);
        descriptor.version = 0;
        assert_eq!(unsafe { copied_local_type(&scope, "open", "new", Some(&descriptor)) }, Err(EvalStatus::RuntimeFatal));
    }
}
