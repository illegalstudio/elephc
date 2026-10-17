//! Purpose:
//! Objects: the standard handlers (`std_object_handlers`), creation
//! (`object_init_ex`, `zend_objects_new`), properties, cloning, and release.
//!
//! Called from:
//! - Extension code (custom objects copy `std_object_handlers` and embed a
//!   `zend_object`), `crate::exception`, `crate::convert`, `crate::host`.
//!
//! Key details:
//! - Every property is an entry of the object's properties table, keyed by
//!   its plain name (see `crate::class` for why there are no slots).
//! - Releasing follows PHP's order: the destructor hook (`dtor_obj`) once,
//!   then `free_obj`, then the allocation, found by stepping back
//!   `handlers->offset` bytes from the embedded `zend_object`.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;

use crate::abi::*;
use crate::alloc::{alloc, free};
use crate::global::Global;
use crate::hash;
use crate::string::{self, bytes};
use crate::zval::{copy, copy_value, type_of, zval_ptr_dtor};

static NEXT_HANDLE: Global<u32> = Global::new(1);

#[no_mangle]
pub static std_object_handlers: zend_object_handlers = zend_object_handlers {
    offset: 0,
    free_obj: Some(zend_object_std_dtor),
    dtor_obj: Some(zend_objects_destroy_object),
    clone_obj: Some(zend_objects_clone_obj),
    clone_obj_with: Some(zend_std_clone_obj_with),
    read_property: Some(zend_std_read_property),
    write_property: Some(zend_std_write_property),
    read_dimension: Some(zend_std_read_dimension),
    write_dimension: Some(zend_std_write_dimension),
    get_property_ptr_ptr: Some(zend_std_get_property_ptr_ptr),
    has_property: Some(zend_std_has_property),
    unset_property: Some(zend_std_unset_property),
    has_dimension: Some(zend_std_has_dimension),
    unset_dimension: Some(zend_std_unset_dimension),
    get_properties: Some(zend_std_get_properties),
    get_method: Some(zend_std_get_method),
    get_constructor: Some(zend_std_get_constructor),
    get_class_name: Some(zend_std_get_class_name),
    cast_object: Some(zend_std_cast_object_tostring),
    count_elements: None,
    get_debug_info: Some(zend_std_get_debug_info),
    get_closure: Some(zend_std_get_closure),
    get_gc: Some(zend_std_get_gc),
    do_operation: None,
    compare: Some(zend_std_compare_objects),
    get_properties_for: None,
};

/// Bytes `zend_object_alloc` adds past the struct for property slots: always
/// one zval less than the embedded `zend_object` already holds, since this
/// engine keeps no slots.
pub unsafe fn properties_size(ce: *const zend_class_entry) -> isize {
    let count = if ce.is_null() { 0 } else { (*ce).default_properties_count as isize };
    let guard = if !ce.is_null() && (*ce).ce_flags & ZEND_ACC_USE_GUARDS != 0 { 0 } else { 1 };
    std::mem::size_of::<zval>() as isize * (count - guard)
}

/// `zend_object_std_init`: initialises the standard header of an object for class `ce`: refcount 1,
/// object type info, class and default handlers.
#[no_mangle]
pub unsafe extern "C" fn zend_object_std_init(object: *mut zend_object, ce: *mut zend_class_entry) {
    (*object).gc.refcount = 1;
    (*object).gc.u.type_info = GC_OBJECT;
    (*object).ce = ce;
    (*object).handlers = if ce.is_null() || (*ce).default_object_handlers.is_null() {
        &std_object_handlers
    } else {
        (*ce).default_object_handlers
    };
    (*object).properties = ptr::null_mut();
    let handle = NEXT_HANDLE.get();
    (*object).handle = *handle;
    *handle += 1;
}

/// `zend_objects_new`: allocates an object for `ce` with room for its property slots and
/// initialises its standard header.
#[no_mangle]
pub unsafe extern "C" fn zend_objects_new(ce: *mut zend_class_entry) -> *mut zend_object {
    let size = (std::mem::size_of::<zend_object>() as isize + properties_size(ce)).max(std::mem::size_of::<zend_object>() as isize - 16);
    let object = alloc(size as usize).cast::<zend_object>();
    zend_object_std_init(object, ce);
    object
}

/// The properties table, created on first use.
pub unsafe fn properties(object: *mut zend_object) -> *mut HashTable {
    if (*object).properties.is_null() {
        (*object).properties = hash::_zend_new_array(0);
    }
    (*object).properties
}

/// The table `get_properties` reports, through the object's own handler.
#[no_mangle]
pub unsafe extern "C" fn zend_elephc_object_properties(object: *mut zend_object) -> *mut HashTable {
    match (*(*object).handlers).get_properties {
        Some(get) => get(object),
        None => properties(object),
    }
}

/// `object_properties_init`: fills an object's properties with the declared defaults of its class
/// and parents, in lineage order.
#[no_mangle]
pub unsafe extern "C" fn object_properties_init(object: *mut zend_object, class_type: *mut zend_class_entry) {
    for ce in crate::class::lineage(class_type) {
        let Some(list) = crate::class::declared(ce) else { continue };
        for declared in list {
            if declared.default.u1.v.r#type == IS_UNDEF {
                continue;
            }
            let mut value = zval::UNDEF;
            copy(&mut value, &declared.default);
            hash::zend_hash_update(properties(object), declared.name, &mut value);
        }
    }
}

/// `object_properties_init_ex`: loads an object's properties from a table of values.
#[no_mangle]
pub unsafe extern "C" fn object_properties_init_ex(object: *mut zend_object, properties_table: *mut HashTable) {
    object_properties_load(object, properties_table);
}

/// `object_properties_load`: copies every entry of `properties_table` into the object's properties;
/// NULL is a no-op.
#[no_mangle]
pub unsafe extern "C" fn object_properties_load(object: *mut zend_object, properties_table: *mut HashTable) {
    if properties_table.is_null() {
        return;
    }
    for idx in 0..(*properties_table).nNumUsed {
        let p = hash::bucket(properties_table, idx);
        if type_of(&(*p).val) == IS_UNDEF {
            continue;
        }
        let mut value = zval::UNDEF;
        copy(&mut value, &(*p).val);
        if (*p).key.is_null() {
            let key = string::new((*p).h.to_string().as_bytes());
            hash::zend_hash_update(properties(object), key, &mut value);
            string::zend_string_release(key);
        } else {
            hash::zend_hash_update(properties(object), (*p).key, &mut value);
        }
    }
}

/// Returns the message of the Error PHP raises when `ce` cannot be instantiated (interface, trait
/// or abstract class; enums are allowed while the engine is active), or None when instantiation is
/// allowed.
unsafe fn instantiation_error(ce: *mut zend_class_entry) -> Option<String> {
    let flags = (*ce).ce_flags;
    let kind = if flags & ZEND_ACC_INTERFACE != 0 {
        "interface"
    } else if flags & ZEND_ACC_TRAIT != 0 {
        "trait"
    } else if flags & ZEND_ACC_ENUM != 0 && crate::startup::eg().active {
        return None;
    } else if flags & (ZEND_ACC_IMPLICIT_ABSTRACT_CLASS | ZEND_ACC_EXPLICIT_ABSTRACT_CLASS) != 0 {
        "abstract class"
    } else {
        return None;
    };
    Some(format!("Cannot instantiate {kind} {}", string::text((*ce).name)))
}

/// `object_init_ex`: creates an object of class `ce` in `arg` through its create handler and
/// initialises its default properties. A class that cannot be instantiated throws Error, sets `arg`
/// to null and returns FAILURE.
#[no_mangle]
pub unsafe extern "C" fn object_init_ex(arg: *mut zval, ce: *mut zend_class_entry) -> zend_result {
    if let Some(message) = instantiation_error(ce) {
        crate::exception::throw_error(crate::exception::zend_ce_error, &message);
        (*arg).u1.type_info = IS_NULL as u32;
        return FAILURE;
    }
    let object = match (*ce).create_u.create_object {
        Some(create) => create(ce),
        None => {
            let object = zend_objects_new(ce);
            object_properties_init(object, ce);
            object
        }
    };
    crate::zval::set_obj(arg, object);
    SUCCESS
}

/// `object_and_properties_init`: `object_init_ex` followed by loading the given properties table
/// into the new object.
#[no_mangle]
pub unsafe extern "C" fn object_and_properties_init(arg: *mut zval, ce: *mut zend_class_entry, properties_table: *mut HashTable) -> zend_result {
    let status = object_init_ex(arg, ce);
    if status == SUCCESS && !properties_table.is_null() {
        object_properties_load((*arg).value.obj, properties_table);
    }
    status
}

/// `object_init`: creates a `stdClass` object in `arg`.
#[no_mangle]
pub unsafe extern "C" fn object_init(arg: *mut zval) {
    object_init_ex(arg, crate::class::zend_standard_class_def);
}

/// Releases an object whose count reached zero.
#[no_mangle]
pub unsafe extern "C" fn zend_objects_store_del(object: *mut zend_object) {
    let flags = (*object).gc.u.type_info;
    if flags & IS_OBJ_DESTRUCTOR_CALLED == 0 {
        (*object).gc.u.type_info |= IS_OBJ_DESTRUCTOR_CALLED;
        if let Some(dtor) = (*(*object).handlers).dtor_obj {
            (*object).gc.refcount += 1;
            dtor(object);
            (*object).gc.refcount -= 1;
        }
    }
    if (*object).gc.refcount != 0 {
        return;
    }
    if (*object).gc.u.type_info & IS_OBJ_FREE_CALLED == 0 {
        (*object).gc.u.type_info |= IS_OBJ_FREE_CALLED;
        (*object).gc.refcount = 1;
        let handlers = (*object).handlers;
        if let Some(free_obj) = (*handlers).free_obj {
            free_obj(object);
        }
        let start = object.cast::<u8>().sub((*handlers).offset as usize);
        free(start.cast());
    }
}

/// `zend_objects_store_put`: no-op, because objects are not tracked in an object store here.
#[no_mangle]
pub unsafe extern "C" fn zend_objects_store_put(_object: *mut zend_object) {}

/// `zend_object_release`: drops one reference to an object and destroys it when the count reaches
/// zero.
#[no_mangle]
pub unsafe extern "C" fn zend_object_release(object: *mut zend_object) {
    (*object).gc.refcount -= 1;
    if (*object).gc.refcount == 0 {
        zend_objects_store_del(object);
    }
}

/// `zend_object_std_dtor`: releases the property table of an object, the standard part of freeing
/// it.
#[no_mangle]
pub unsafe extern "C" fn zend_object_std_dtor(object: *mut zend_object) {
    let table = (*object).properties;
    if !table.is_null() {
        (*object).properties = ptr::null_mut();
        hash::zend_array_release(table);
    }
}

/// Calls the class's `__destruct` (an internal method) if it has one.
#[no_mangle]
pub unsafe extern "C" fn zend_objects_destroy_object(object: *mut zend_object) {
    let destructor = (*(*object).ce).destructor;
    if destructor.is_null() {
        return;
    }
    let mut retval = zval::UNDEF;
    crate::api::zend_call_known_function(destructor, object, (*object).ce, &mut retval, 0, ptr::null_mut(), ptr::null_mut());
    zval_ptr_dtor(&mut retval);
}

/// `zend_objects_clone_members`: copies the properties of `old_object` into `new_object`, as the
/// standard clone does.
#[no_mangle]
pub unsafe extern "C" fn zend_objects_clone_members(new_object: *mut zend_object, old_object: *mut zend_object) {
    let old = (*old_object).properties;
    if !old.is_null() {
        let copy = hash::zend_array_dup(old);
        if !(*new_object).properties.is_null() {
            hash::zend_array_release((*new_object).properties);
        }
        (*new_object).properties = copy;
    }
    let clone = (*(*new_object).ce).clone;
    if !clone.is_null() {
        let mut retval = zval::UNDEF;
        crate::api::zend_call_known_function(clone, new_object, (*new_object).ce, &mut retval, 0, ptr::null_mut(), ptr::null_mut());
        zval_ptr_dtor(&mut retval);
    }
}

/// `zend_objects_clone_obj`: creates a new object of the same class with copies of the properties.
#[no_mangle]
pub unsafe extern "C" fn zend_objects_clone_obj(old_object: *mut zend_object) -> *mut zend_object {
    let new_object = zend_objects_new((*old_object).ce);
    zend_objects_clone_members(new_object, old_object);
    new_object
}

/// `zend_std_clone_obj_with`: clones an object through its clone handler (or the standard clone)
/// and then applies the given property overrides.
#[no_mangle]
pub unsafe extern "C" fn zend_std_clone_obj_with(
    old_object: *mut zend_object,
    _scope: *const zend_class_entry,
    properties_table: *const HashTable,
) -> *mut zend_object {
    let clone = match (*(*old_object).handlers).clone_obj {
        Some(clone) => clone(old_object),
        None => zend_objects_clone_obj(old_object),
    };
    if !properties_table.is_null() {
        object_properties_load(clone, properties_table as *mut HashTable);
    }
    clone
}

// ------------------------------------------------------------ properties

/// Finds a property of an object by name in its property table, resolving indirect slots; NULL when
/// absent.
pub unsafe fn find_property(object: *mut zend_object, name: &[u8]) -> *mut zval {
    let table = zend_elephc_object_properties(object);
    hash::zend_hash_str_find_ind(table, name.as_ptr().cast(), name.len())
}

/// Sets a property, taking `value`'s reference.
pub unsafe fn set_property(object: *mut zend_object, name: &[u8], value: *mut zval) {
    let table = properties(object);
    hash::zend_hash_str_update(table, name.as_ptr().cast(), name.len(), value);
}

/// `zend_std_read_property`: reads a property from the object's property table; a missing one
/// raises an "Undefined property" warning (except for isset/unset style reads) and yields the
/// engine's uninitialized zval.
#[no_mangle]
pub unsafe extern "C" fn zend_std_read_property(
    object: *mut zend_object,
    name: *mut zend_string,
    r#type: c_int,
    _cache_slot: *mut *mut c_void,
    _rv: *mut zval,
) -> *mut zval {
    let found = hash::zend_hash_find_ind(zend_elephc_object_properties(object), name);
    if !found.is_null() {
        return found;
    }
    if r#type != BP_VAR_IS && r#type != BP_VAR_UNSET {
        crate::error::warning(&format!(
            "Undefined property: {}::${}",
            string::text((*(*object).ce).name),
            string::text(name)
        ));
    }
    &mut crate::startup::eg().uninitialized_zval
}

/// `zend_std_write_property`: stores a copy of `value` under the property name, creating the
/// property if needed, and returns its slot.
#[no_mangle]
pub unsafe extern "C" fn zend_std_write_property(
    object: *mut zend_object,
    name: *mut zend_string,
    value: *mut zval,
    _cache_slot: *mut *mut c_void,
) -> *mut zval {
    let mut stored = zval::UNDEF;
    copy(&mut stored, value);
    hash::zend_hash_update_ind(properties(object), name, &mut stored)
}

/// `zend_std_get_property_ptr_ptr`: returns the property's slot for in-place access, creating it
/// for write-style accesses and returning NULL for plain or isset-style reads of a missing
/// property.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_property_ptr_ptr(
    object: *mut zend_object,
    name: *mut zend_string,
    r#type: c_int,
    _cache_slot: *mut *mut c_void,
) -> *mut zval {
    let table = properties(object);
    let found = hash::zend_hash_find_ind(table, name);
    if !found.is_null() || r#type == BP_VAR_R || r#type == BP_VAR_IS {
        return found;
    }
    let mut null = zval::UNDEF;
    null.u1.type_info = IS_NULL as u32;
    hash::zend_hash_add(table, name, &mut null)
}

/// `zend_std_has_property`: whether a property exists (and, depending on `has_set_exists`, is set
/// or non-empty).
#[no_mangle]
pub unsafe extern "C" fn zend_std_has_property(
    object: *mut zend_object,
    name: *mut zend_string,
    has_set_exists: c_int,
    _cache_slot: *mut *mut c_void,
) -> c_int {
    let found = hash::zend_hash_find_ind(zend_elephc_object_properties(object), name);
    if found.is_null() {
        return 0;
    }
    let value = crate::zval::deref(found);
    (match has_set_exists {
        ZEND_PROPERTY_EXISTS => true,
        ZEND_PROPERTY_NOT_EMPTY => crate::convert::zend_is_true(value),
        _ => type_of(value) != IS_NULL,
    }) as c_int
}

/// `zend_std_unset_property`: removes a property from the object's property table.
#[no_mangle]
pub unsafe extern "C" fn zend_std_unset_property(object: *mut zend_object, name: *mut zend_string, _cache_slot: *mut *mut c_void) {
    if !(*object).properties.is_null() {
        hash::zend_hash_del((*object).properties, name);
    }
}

/// Throws Error "Cannot use object of type X as array" for an object without array access.
unsafe fn not_an_array(object: *mut zend_object) {
    crate::exception::throw_error(
        crate::exception::zend_ce_error,
        &format!("Cannot use object of type {} as array", string::text((*(*object).ce).name)),
    );
}

/// `zend_std_read_dimension`: array-style read on a plain object; throws Error and returns NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_std_read_dimension(object: *mut zend_object, _offset: *mut zval, _type: c_int, _rv: *mut zval) -> *mut zval {
    not_an_array(object);
    ptr::null_mut()
}

/// `zend_std_write_dimension`: array-style write on a plain object; throws Error.
#[no_mangle]
pub unsafe extern "C" fn zend_std_write_dimension(object: *mut zend_object, _offset: *mut zval, _value: *mut zval) {
    not_an_array(object);
}

/// `zend_std_has_dimension`: array-style isset on a plain object; throws Error and returns 0.
#[no_mangle]
pub unsafe extern "C" fn zend_std_has_dimension(object: *mut zend_object, _offset: *mut zval, _check_empty: c_int) -> c_int {
    not_an_array(object);
    0
}

/// `zend_std_unset_dimension`: array-style unset on a plain object; throws Error.
#[no_mangle]
pub unsafe extern "C" fn zend_std_unset_dimension(object: *mut zend_object, _offset: *mut zval) {
    not_an_array(object);
}

/// `zend_std_get_properties`: the object's property table.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_properties(object: *mut zend_object) -> *mut HashTable {
    properties(object)
}

/// `rebuild_object_properties`: the object's property table; nothing needs rebuilding here.
#[no_mangle]
pub unsafe extern "C" fn rebuild_object_properties(object: *mut zend_object) -> *mut HashTable {
    properties(object)
}

/// `zend_std_get_debug_info`: the property table used for debug output; never a temporary.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_debug_info(object: *mut zend_object, is_temp: *mut c_int) -> *mut HashTable {
    *is_temp = 0;
    zend_elephc_object_properties(object)
}

/// `zend_std_get_gc`: reports no extra GC roots and returns the object's property table.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_gc(object: *mut zend_object, table: *mut *mut zval, n: *mut c_int) -> *mut HashTable {
    *table = ptr::null_mut();
    *n = 0;
    (*object).properties
}

/// `zend_std_get_properties_for`: the property table for a given purpose (debug, serialisation,
/// var_export and so on), using the debug-info handler for the debug purpose, and returns it with
/// an added reference unless it is immutable.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_properties_for(object: *mut zend_object, purpose: zend_prop_purpose) -> *mut HashTable {
    let table = if purpose == ZEND_PROP_PURPOSE_DEBUG {
        let mut is_temp = 0;
        match (*(*object).handlers).get_debug_info {
            Some(debug) => {
                let table = debug(object, &mut is_temp);
                if is_temp != 0 {
                    return table;
                }
                table
            }
            None => zend_elephc_object_properties(object),
        }
    } else {
        zend_elephc_object_properties(object)
    };
    if !table.is_null() && (*table).gc.u.type_info & GC_IMMUTABLE == 0 {
        (*table).gc.refcount += 1;
    }
    table
}

/// `zend_get_properties_for`: the property table of an object zval for a purpose, through its
/// `get_properties_for` handler or the standard one.
#[no_mangle]
pub unsafe extern "C" fn zend_get_properties_for(obj: *mut zval, purpose: zend_prop_purpose) -> *mut HashTable {
    let object = (*obj).value.obj;
    match (*(*object).handlers).get_properties_for {
        Some(get) => get(object, purpose),
        None => zend_std_get_properties_for(object, purpose),
    }
}

/// `zend_std_get_method`: finds a method of the object's class by case-insensitive name; NULL when
/// absent.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_method(obj_ptr: *mut *mut zend_object, method_name: *mut zend_string, _key: *const zval) -> *mut zend_function {
    hash::zend_hash_find_ptr_lc(&(*(**obj_ptr).ce).function_table, method_name).cast()
}

/// `zend_std_get_static_method`: finds a method of class `ce` by case-insensitive name; NULL when
/// absent.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_static_method(ce: *mut zend_class_entry, function_name: *mut zend_string, _key: *const zval) -> *mut zend_function {
    hash::zend_hash_find_ptr_lc(&(*ce).function_table, function_name).cast()
}

/// `zend_std_get_constructor`: the constructor of the object's class, possibly NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_constructor(object: *mut zend_object) -> *mut zend_function {
    (*(*object).ce).constructor
}

/// `zend_std_get_class_name`: a new reference to the name of the object's class.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_class_name(object: *const zend_object) -> *mut zend_string {
    string::zend_string_copy((*(*object).ce).name)
}

/// `zend_std_compare_objects`: orders two values at least one of which is an object: identical
/// objects are equal, objects of different classes are unordered (1), same-class objects compare by
/// their properties, and an object against a non-object orders as greater.
#[no_mangle]
pub unsafe extern "C" fn zend_std_compare_objects(o1: *mut zval, o2: *mut zval) -> c_int {
    if type_of(o1) != IS_OBJECT || type_of(o2) != IS_OBJECT {
        let (object, other) = if type_of(o1) == IS_OBJECT { (o1, o2) } else { (o2, o1) };
        let sign = if type_of(o1) == IS_OBJECT { 1 } else { -1 };
        let _ = other;
        let _ = object;
        return sign;
    }
    let (a, b) = ((*o1).value.obj, (*o2).value.obj);
    if a == b {
        return 0;
    }
    if (*a).ce != (*b).ce {
        return 1;
    }
    hash::zend_hash_compare(
        zend_elephc_object_properties(a),
        zend_elephc_object_properties(b),
        Some(compare_values),
        false,
    )
}

/// Comparison callback handing two property zvals to `zend_compare`.
unsafe extern "C" fn compare_values(a: *const c_void, b: *const c_void) -> c_int {
    crate::convert::zend_compare(a as *mut zval, b as *mut zval)
}

/// `zend_objects_not_comparable`: compare handler for non-comparable objects; always reports them
/// unordered (1).
#[no_mangle]
pub unsafe extern "C" fn zend_objects_not_comparable(_o1: *mut zval, _o2: *mut zval) -> c_int {
    1
}

/// `(string)$object` through `__toString` (an internal method here).
#[no_mangle]
pub unsafe extern "C" fn zend_std_cast_object_tostring(object: *mut zend_object, writeobj: *mut zval, r#type: c_int) -> zend_result {
    match r#type as u8 {
        IS_STRING => {
            let tostring = (*(*object).ce).__tostring;
            if tostring.is_null() {
                return FAILURE;
            }
            let mut retval = zval::UNDEF;
            crate::api::zend_call_known_function(tostring, object, (*object).ce, &mut retval, 0, ptr::null_mut(), ptr::null_mut());
            if type_of(&retval) == IS_STRING {
                copy_value(writeobj, &retval);
                SUCCESS
            } else {
                zval_ptr_dtor(&mut retval);
                FAILURE
            }
        }
        _IS_BOOL => {
            (*writeobj).u1.type_info = IS_TRUE as u32;
            SUCCESS
        }
        _ => FAILURE,
    }
}

/// `zend_std_get_closure`: closure handler for a plain object; always FAILURE because it is not
/// callable.
#[no_mangle]
pub unsafe extern "C" fn zend_std_get_closure(
    _obj: *mut zend_object,
    _ce_ptr: *mut *mut zend_class_entry,
    _fptr_ptr: *mut *mut zend_function,
    _obj_ptr: *mut *mut zend_object,
    _check_only: bool,
) -> zend_result {
    FAILURE
}

// ------------------------------------------------------------ property helpers

/// Updates a property by name from a zval, through the object's `write_property` handler (or the
/// standard write), consuming the temporary name string.
unsafe fn update(object: *mut zend_object, name: *const c_char, len: usize, mut value: zval) {
    let key = string::zend_string_init(name, len, false);
    match (*(*object).handlers).write_property {
        Some(write) => {
            write(object, key, &mut value, ptr::null_mut());
        }
        None => {
            zend_std_write_property(object, key, &mut value, ptr::null_mut());
        }
    }
    string::zend_string_release(key);
    zval_ptr_dtor(&mut value);
}

/// `zend_update_property_ex`: sets a property by `zend_string` name through the object's write
/// handler.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_ex(_scope: *mut zend_class_entry, object: *mut zend_object, name: *mut zend_string, value: *mut zval) {
    match (*(*object).handlers).write_property {
        Some(write) => {
            write(object, name, value, ptr::null_mut());
        }
        None => {
            zend_std_write_property(object, name, value, ptr::null_mut());
        }
    }
}

/// `zend_update_property`: sets a property by pointer and length name to the given zval.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: *mut zval) {
    let key = string::zend_string_init(name, name_length, false);
    zend_update_property_ex(scope, object, key, value);
    string::zend_string_release(key);
}

/// `zend_update_property_null`: sets a property to null.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_null(_scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize) {
    update(object, name, name_length, crate::zval::null());
}

/// `zend_update_property_bool`: sets a property to a boolean (non-zero is true).
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_bool(_scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: zend_long) {
    let mut zv = zval::UNDEF;
    crate::zval::set_bool(&mut zv, value != 0);
    update(object, name, name_length, zv);
}

/// `zend_update_property_long`: sets a property to an integer.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_long(_scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: zend_long) {
    update(object, name, name_length, crate::zval::long(value));
}

/// `zend_update_property_double`: sets a property to a float.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_double(_scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: f64) {
    let mut zv = zval::UNDEF;
    crate::zval::set_double(&mut zv, value);
    update(object, name, name_length, zv);
}

/// `zend_update_property_str`: sets a property to a string, taking a reference to the given
/// `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_str(_scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: *mut zend_string) {
    let mut zv = zval::UNDEF;
    crate::zval::set_str_copy(&mut zv, value);
    update(object, name, name_length, zv);
}

/// `zend_update_property_string`: sets a property to a copy of a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_string(_scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: *const c_char) {
    let mut zv = zval::UNDEF;
    crate::zval::set_bytes(&mut zv, CStr::from_ptr(value).to_bytes());
    update(object, name, name_length, zv);
}

/// `zend_update_property_stringl`: sets a property to a copy of a string given by pointer and
/// length.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_stringl(
    _scope: *mut zend_class_entry,
    object: *mut zend_object,
    name: *const c_char,
    name_length: usize,
    value: *const c_char,
    value_length: usize,
) {
    let mut zv = zval::UNDEF;
    crate::zval::set_str(&mut zv, string::zend_string_init(value, value_length, false));
    update(object, name, name_length, zv);
}

/// `zend_unset_property`: removes a property by name through the object's unset handler.
#[no_mangle]
pub unsafe extern "C" fn zend_unset_property(_scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize) {
    let key = string::zend_string_init(name, name_length, false);
    if let Some(unset) = (*(*object).handlers).unset_property {
        unset(object, key, ptr::null_mut());
    }
    string::zend_string_release(key);
}

/// `zend_read_property_ex`: reads a property by `zend_string` name through the object's read
/// handler; `silent` makes it an isset-style read that raises no undefined-property diagnostics,
/// and `rv` is scratch storage for the result.
#[no_mangle]
pub unsafe extern "C" fn zend_read_property_ex(
    _scope: *mut zend_class_entry,
    object: *mut zend_object,
    name: *mut zend_string,
    silent: bool,
    rv: *mut zval,
) -> *mut zval {
    let r#type = if silent { BP_VAR_IS } else { BP_VAR_R };
    match (*(*object).handlers).read_property {
        Some(read) => read(object, name, r#type, ptr::null_mut(), rv),
        None => zend_std_read_property(object, name, r#type, ptr::null_mut(), rv),
    }
}

/// `zend_read_property`: reads a property by pointer and length name; see `zend_read_property_ex`.
#[no_mangle]
pub unsafe extern "C" fn zend_read_property(
    scope: *mut zend_class_entry,
    object: *mut zend_object,
    name: *const c_char,
    name_length: usize,
    silent: bool,
    rv: *mut zval,
) -> *mut zval {
    let key = string::zend_string_init(name, name_length, false);
    let value = zend_read_property_ex(scope, object, key, silent, rv);
    string::zend_string_release(key);
    value
}

/// The object's class name, for messages.
pub unsafe fn class_name(object: *const zend_object) -> String {
    string::text((*(*object).ce).name)
}

/// The visible name of a property key: mangled private/protected names
/// (`\0Class\0name`) lose their prefix.
pub fn visible_name(key: &[u8]) -> &[u8] {
    if key.first() == Some(&0) {
        if let Some(end) = key[1..].iter().position(|&b| b == 0) {
            return &key[end + 2..];
        }
    }
    key
}

/// The bytes of a `zend_string` property key, valid for as long as the string lives.
pub unsafe fn property_bytes(key: *const zend_string) -> &'static [u8] {
    bytes(key)
}
