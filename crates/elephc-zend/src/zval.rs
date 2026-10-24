//! Purpose:
//! zval lifetimes: copying, reference counting, releasing (`zval_ptr_dtor`),
//! references, and the small constructors the engine uses everywhere.
//!
//! Called from:
//! - Extension code (`zval_ptr_dtor`, `ZVAL_DUP`, `ZVAL_NEW_REF`), and every
//!   engine module.
//!
//! Key details:
//! - Releasing follows PHP: a refcounted value is freed when its count drops
//!   to zero (strings, arrays, objects through their handlers, resources
//!   through their destructor, references with their inner value).
//! - There is no cycle collector: cyclic garbage an extension builds is
//!   never reclaimed, as in a PHP request ending before the collector runs.

use std::ptr;

use crate::abi::*;
use crate::alloc::{alloc_struct, free};
use crate::string::{is_interned, zend_string_release};

/// The type byte of a zval (`IS_*`), without following references.
#[inline]
pub unsafe fn type_of(zv: *const zval) -> u8 {
    (*zv).u1.v.r#type
}

/// Whether a zval's payload is reference-counted (its type flags are non-zero).
#[inline]
pub unsafe fn is_refcounted(zv: *const zval) -> bool {
    (*zv).u1.v.type_flags != 0
}

/// `ZVAL_COPY_VALUE`: the payload and the type word, not `u2`.
#[inline]
pub unsafe fn copy_value(dst: *mut zval, src: *const zval) {
    (*dst).value = (*src).value;
    (*dst).u1.type_info = (*src).u1.type_info;
}

/// `ZVAL_COPY`: a copy holding its own reference.
#[inline]
pub unsafe fn copy(dst: *mut zval, src: *const zval) {
    copy_value(dst, src);
    if is_refcounted(dst) {
        (*(*dst).value.counted).gc.refcount += 1;
    }
}

/// `ZVAL_COPY_DEREF`.
pub unsafe fn copy_deref(dst: *mut zval, src: *const zval) {
    let mut src = src;
    if type_of(src) == IS_REFERENCE {
        src = &(*(*src).value.r#ref).val;
    }
    copy(dst, src);
}

/// `ZVAL_NULL`: makes the zval null.
pub unsafe fn set_null(zv: *mut zval) {
    (*zv).u1.type_info = IS_NULL as u32;
}

/// `ZVAL_BOOL`: makes the zval true or false.
pub unsafe fn set_bool(zv: *mut zval, value: bool) {
    (*zv).u1.type_info = if value { IS_TRUE } else { IS_FALSE } as u32;
}

/// `ZVAL_LONG`: makes the zval an integer.
pub unsafe fn set_long(zv: *mut zval, value: zend_long) {
    (*zv).value.lval = value;
    (*zv).u1.type_info = IS_LONG as u32;
}

/// `ZVAL_DOUBLE`: makes the zval a float.
pub unsafe fn set_double(zv: *mut zval, value: f64) {
    (*zv).value.dval = value;
    (*zv).u1.type_info = IS_DOUBLE as u32;
}

/// `ZVAL_STR`: takes the caller's reference.
pub unsafe fn set_str(zv: *mut zval, s: *mut zend_string) {
    (*zv).value.str = s;
    (*zv).u1.type_info = if is_interned(s) { IS_INTERNED_STRING_EX } else { IS_STRING_EX };
}

/// `ZVAL_STR_COPY`: adds a reference.
pub unsafe fn set_str_copy(zv: *mut zval, s: *mut zend_string) {
    if !is_interned(s) {
        (*s).gc.refcount += 1;
    }
    set_str(zv, s);
}

/// Makes the zval a new string holding a copy of the given bytes; the zval owns the new string.
pub unsafe fn set_bytes(zv: *mut zval, data: &[u8]) {
    set_str(zv, crate::string::new(data));
}

/// `ZVAL_ARR`: makes the zval an array, taking the caller's reference to the table.
pub unsafe fn set_arr(zv: *mut zval, arr: *mut zend_array) {
    (*zv).value.arr = arr;
    (*zv).u1.type_info = IS_ARRAY_EX;
}

/// `ZVAL_OBJ`: makes the zval an object, taking the caller's reference.
pub unsafe fn set_obj(zv: *mut zval, obj: *mut zend_object) {
    (*zv).value.obj = obj;
    (*zv).u1.type_info = IS_OBJECT_EX;
}

/// `ZVAL_RES`: makes the zval a resource, taking the caller's reference.
pub unsafe fn set_res(zv: *mut zval, res: *mut zend_resource) {
    (*zv).value.res = res;
    (*zv).u1.type_info = IS_RESOURCE_EX;
}

/// `ZVAL_PTR`: makes the zval an opaque pointer (`IS_PTR`), which is never reference-counted.
pub unsafe fn set_ptr(zv: *mut zval, p: *mut std::ffi::c_void) {
    (*zv).value.ptr = p;
    (*zv).u1.type_info = IS_PTR as u32;
}

/// Follows a reference to the value inside it.
pub unsafe fn deref(zv: *mut zval) -> *mut zval {
    if type_of(zv) == IS_REFERENCE {
        &mut (*(*zv).value.r#ref).val
    } else {
        zv
    }
}

/// Follows references and indirections to the value itself.
pub unsafe fn resolve(zv: *mut zval) -> *mut zval {
    let mut zv = deref(zv);
    if type_of(zv) == IS_INDIRECT {
        zv = deref((*zv).value.zv);
    }
    zv
}

/// Frees a refcounted value whose count reached zero.
#[no_mangle]
pub unsafe extern "C" fn rc_dtor_func(p: *mut zend_refcounted) {
    match ((*p).gc.u.type_info & GC_TYPE_MASK) as u8 {
        IS_STRING => free(p.cast()),
        IS_ARRAY => crate::hash::zend_array_destroy(p.cast()),
        IS_OBJECT => crate::object::zend_objects_store_del(p.cast()),
        IS_RESOURCE => crate::api::resource_release(p.cast()),
        IS_REFERENCE => {
            let reference = p.cast::<zend_reference>();
            zval_ptr_dtor(&mut (*reference).val);
            free(p.cast());
        }
        _ => {}
    }
}

/// Releases one reference to the value held by `zval_ptr`.
#[no_mangle]
pub unsafe extern "C" fn zval_ptr_dtor(zval_ptr: *mut zval) {
    if !is_refcounted(zval_ptr) {
        return;
    }
    let counted = (*zval_ptr).value.counted;
    if (*counted).gc.u.type_info & GC_IMMUTABLE != 0 {
        return;
    }
    (*counted).gc.refcount -= 1;
    if (*counted).gc.refcount == 0 {
        rc_dtor_func(counted);
    }
}

/// `zval_internal_ptr_dtor`: destroys a zval's value; same as `zval_ptr_dtor` here.
#[no_mangle]
pub unsafe extern "C" fn zval_internal_ptr_dtor(zval_ptr: *mut zval) {
    zval_ptr_dtor(zval_ptr);
}

/// `zval_ptr_dtor_str`: releases the string held by a zval if it is a reference-counted string.
#[no_mangle]
pub unsafe extern "C" fn zval_ptr_dtor_str(zval_ptr: *mut zval) {
    if type_of(zval_ptr) == IS_STRING && is_refcounted(zval_ptr) {
        zend_string_release((*zval_ptr).value.str);
    }
}

/// `zval_dtor`: destroys a zval's value; same as `zval_ptr_dtor`.
#[no_mangle]
pub unsafe extern "C" fn zval_dtor(zval_ptr: *mut zval) {
    zval_ptr_dtor(zval_ptr);
}

/// `zval_ptr_safe_dtor`: destroys a zval's value; same as `zval_ptr_dtor`.
#[no_mangle]
pub unsafe extern "C" fn zval_ptr_safe_dtor(zval_ptr: *mut zval) {
    zval_ptr_dtor(zval_ptr);
}

/// Adds a reference, collapsing a reference held only here into its value,
/// as `zval_add_ref` does for values copied out of a table.
#[no_mangle]
pub unsafe extern "C" fn zval_add_ref(p: *mut zval) {
    if !is_refcounted(p) {
        return;
    }
    if type_of(p) == IS_REFERENCE && (*(*p).value.r#ref).gc.refcount == 1 {
        let reference = (*p).value.r#ref;
        copy(p, &(*reference).val);
    } else {
        (*(*p).value.counted).gc.refcount += 1;
    }
}

/// Gives `zvalue` a private copy of an array or string it shares.
#[no_mangle]
pub unsafe extern "C" fn zval_copy_ctor_func(zvalue: *mut zval) {
    match type_of(zvalue) {
        IS_ARRAY => {
            let source = (*zvalue).value.arr;
            set_arr(zvalue, crate::hash::zend_array_dup(source));
        }
        IS_STRING => {
            let source = (*zvalue).value.str;
            set_str(zvalue, crate::string::new(crate::string::bytes(source)));
        }
        _ => {}
    }
}

/// `ZVAL_DUP`: arrays are duplicated, everything else copied.
#[no_mangle]
pub unsafe extern "C" fn zend_elephc_zval_dup(target: *mut zval, source: *const zval) {
    if type_of(source) == IS_ARRAY {
        set_arr(target, crate::hash::zend_array_dup((*source).value.arr));
    } else {
        copy(target, source);
    }
}

/// A new reference holding a copy of `value` (or UNDEF), refcount 1. The
/// value's own reference moves into it.
#[no_mangle]
pub unsafe extern "C" fn zend_elephc_new_reference(value: *const zval) -> *mut zend_reference {
    let reference = alloc_struct::<zend_reference>();
    (*reference).gc.refcount = 1;
    (*reference).gc.u.type_info = GC_REFERENCE;
    if value.is_null() {
        (*reference).val = zval::UNDEF;
    } else {
        copy_value(&mut (*reference).val, value);
    }
    (*reference).sources.ptr = ptr::null_mut();
    reference
}

/// A typed reference never exists here, so assigning through one is plain.
#[no_mangle]
pub unsafe extern "C" fn zend_verify_prop_assignable_by_ref(
    _prop_info: *const zend_property_info,
    _orig_val: *mut zval,
    _strict: bool,
) -> bool {
    true
}

/// `zend_try_assign_typed_ref_bool`: stores a boolean in a reference, releasing its old value; type
/// constraints are not enforced so it always succeeds.
#[no_mangle]
pub unsafe extern "C" fn zend_try_assign_typed_ref_bool(reference: *mut zend_reference, val: bool) -> bool {
    zval_ptr_dtor(&mut (*reference).val);
    set_bool(&mut (*reference).val, val);
    true
}

/// `zend_try_assign_typed_ref_long`: stores an integer in a reference, releasing its old value; it
/// always succeeds.
#[no_mangle]
pub unsafe extern "C" fn zend_try_assign_typed_ref_long(reference: *mut zend_reference, lval: zend_long) -> bool {
    zval_ptr_dtor(&mut (*reference).val);
    set_long(&mut (*reference).val, lval);
    true
}

/// `zend_try_assign_typed_ref`: stores a value in a reference, releasing its old value and taking
/// the value as is; it always succeeds.
#[no_mangle]
pub unsafe extern "C" fn zend_try_assign_typed_ref(reference: *mut zend_reference, zv: *mut zval) -> bool {
    zval_ptr_dtor(&mut (*reference).val);
    copy_value(&mut (*reference).val, zv);
    true
}

/// A zval of each scalar kind, for building values in Rust.
pub fn long(value: zend_long) -> zval {
    let mut zv = zval::UNDEF;
    unsafe { set_long(&mut zv, value) };
    zv
}

/// A null zval, for building values in Rust.
pub fn null() -> zval {
    let mut zv = zval::UNDEF;
    unsafe { set_null(&mut zv) };
    zv
}

/// Makes `zv` (dereferenced) an empty array of `size` slots, releasing what
/// it held; returns the zval written.
#[no_mangle]
pub unsafe extern "C" fn zend_try_array_init_size(zv: *mut zval, size: u32) -> *mut zval {
    let target = deref(zv);
    zval_ptr_dtor(target);
    set_arr(target, crate::hash::_zend_new_array(size));
    target
}

/// `zend_try_array_init`: makes the zval (dereferenced) an empty array, releasing what it held, and
/// returns the zval written.
#[no_mangle]
pub unsafe extern "C" fn zend_try_array_init(zv: *mut zval) -> *mut zval {
    zend_try_array_init_size(zv, 0)
}
