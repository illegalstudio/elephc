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
use crate::property::{self, Access};
use crate::string::{self, bytes};
use crate::zval::{copy, copy_value, set_double, set_long, type_of, zval_ptr_dtor};

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

/// Returns the message of the Error PHP raises when `ce` cannot be instantiated (interface, trait,
/// enum or abstract class), or None when instantiation is allowed. Enum cases are created through
/// `init_object`, as php-src's `zend_enum_new` bypasses this check.
unsafe fn instantiation_error(ce: *mut zend_class_entry) -> Option<String> {
    let flags = (*ce).ce_flags;
    let kind = if flags & ZEND_ACC_INTERFACE != 0 {
        "interface"
    } else if flags & ZEND_ACC_TRAIT != 0 {
        "trait"
    } else if flags & ZEND_ACC_ENUM != 0 {
        "enum"
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
    init_object(arg, ce);
    SUCCESS
}

/// Creates an object of class `ce` in `arg` through its create handler and initialises its default
/// properties, without asking whether the class may be instantiated (enum cases).
pub unsafe fn init_object(arg: *mut zval, ce: *mut zend_class_entry) {
    let object = match (*ce).create_u.create_object {
        Some(create) => create(ce),
        None => {
            let object = zend_objects_new(ce);
            object_properties_init(object, ce);
            object
        }
    };
    crate::zval::set_obj(arg, object);
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
    release_shadowing(object);
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
    let shadowed = shadowing(old_object, false);
    if !shadowed.is_null() {
        let copy = hash::zend_array_dup(shadowed);
        if let Some(previous) = SHADOWING.get().get_or_insert_with(Default::default).insert(new_object as usize, copy) {
            hash::zend_array_release(previous);
        }
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

/// Each object's dynamic properties named like a private property of an ancestor (`$this->secret`
/// in a subclass of the class declaring `private $secret`). php-src keeps them in the properties
/// table beside the private one, which it keys by its mangled name; this engine keys a declared
/// property by its plain name, so these live in a table of their own, by object address.
static SHADOWING: Global<Option<std::collections::HashMap<usize, *mut HashTable>>> = Global::new(None);

/// `object`'s table of properties shadowing an ancestor's private ones, created when `create`;
/// NULL when it has none.
unsafe fn shadowing(object: *mut zend_object, create: bool) -> *mut HashTable {
    let tables = SHADOWING.get().get_or_insert_with(Default::default);
    match tables.get(&(object as usize)) {
        Some(&table) => table,
        None if create => *tables.entry(object as usize).or_insert_with(|| hash::_zend_new_array(0)),
        None => ptr::null_mut(),
    }
}

/// Releases `object`'s shadowing properties, as it is freed.
unsafe fn release_shadowing(object: *mut zend_object) {
    if let Some(table) = SHADOWING.get().as_mut().and_then(|tables| tables.remove(&(object as usize))) {
        hash::zend_array_release(table);
    }
}

/// The array PHP's `get_object_vars()` (`mangled` false) or `(array)` cast and debug table
/// (`mangled` true) give for `object`, built as php-src's properties table orders it: declared
/// properties in declaration order (the root class's first), skipping one never initialized,
/// then dynamic ones in creation order. Unmangled, only what the program access's scope may see
/// is listed; mangled, every property under php-src's key (`"\0Class\0name"` for a private one,
/// `"\0*\0name"` for a protected one).
pub unsafe fn property_view(object: *mut zend_object, mangled: bool) -> *mut HashTable {
    let result = hash::_zend_new_array(0);
    let table = zend_elephc_object_properties(object);
    let mut declared = std::collections::HashSet::new();
    let add = |key: *mut zend_string, value: *mut zval| {
        let mut copied = zval::UNDEF;
        copy(&mut copied, crate::zval::deref(value));
        if hash::zend_hash_add(result, key, &mut copied).is_null() {
            zval_ptr_dtor(&mut copied);
        }
    };
    for ce in crate::class::lineage((*object).ce) {
        let Some(list) = crate::class::declared(ce) else { continue };
        for property in list {
            let name = property.name;
            if !declared.insert(bytes(name).to_vec()) {
                continue;
            }
            let info = property::info((*object).ce, name);
            if info.is_null() || table.is_null() {
                continue;
            }
            let value = hash::zend_hash_find_ind(table, name);
            if value.is_null() {
                continue;
            }
            if mangled {
                let key = mangled_name(info);
                add(key, value);
                string::zend_string_release(key);
            } else if matches!(property::visibility(object, info), Access::Allowed) {
                add(name, value);
            }
        }
    }
    let dynamic = |source: *mut HashTable| {
        if source.is_null() {
            return;
        }
        for idx in 0..(*source).nNumUsed {
            let bucket = hash::bucket(source, idx);
            if (*bucket).val.u1.v.r#type == IS_UNDEF {
                continue;
            }
            if (*bucket).key.is_null() {
                let mut copied = zval::UNDEF;
                copy(&mut copied, crate::zval::deref(&mut (*bucket).val));
                hash::zend_hash_index_update(result, (*bucket).h, &mut copied);
            } else if !declared.contains(bytes((*bucket).key)) || source != table {
                add((*bucket).key, &mut (*bucket).val);
            }
        }
    };
    dynamic(table);
    dynamic(shadowing(object, false));
    result
}

/// Whether class `ce` (or an ancestor) declares property `name`, in any visibility, static or
/// not, as `property_exists()` asks.
pub unsafe fn class_declares(ce: *mut zend_class_entry, name: *mut zend_string) -> bool {
    let mut current = ce;
    while !current.is_null() {
        if !hash::zend_hash_find_ptr(&(*current).properties_info, name).is_null() {
            return true;
        }
        current = (*current).parent_u.parent;
    }
    false
}

/// Whether `object` has property `name` as `property_exists()` sees it: declared by its class, or
/// held as a dynamic property.
pub unsafe fn object_has_property(object: *mut zend_object, name: *mut zend_string) -> bool {
    if class_declares((*object).ce, name) {
        return true;
    }
    let table = zend_elephc_object_properties(object);
    let shadowed = shadowing(object, false);
    (!table.is_null() && !hash::zend_hash_find_ind(table, name).is_null())
        || (!shadowed.is_null() && !hash::zend_hash_find_ind(shadowed, name).is_null())
}

/// `get_class_vars()` of class `ce` from a program scope whose nearest hosted class is
/// `ancestor` (null for the global scope or a class extending none): the declared instance
/// properties' defaults in declaration order, null for one with no default. A program class is
/// never an engine class, so a private property is never listed and a protected one only from a
/// class in its lineage.
pub unsafe fn class_defaults(ce: *mut zend_class_entry, ancestor: *mut zend_class_entry) -> *mut HashTable {
    let result = hash::_zend_new_array(0);
    let mut seen = std::collections::HashSet::new();
    for class in crate::class::lineage(ce) {
        let Some(list) = crate::class::declared(class) else { continue };
        for property in list {
            if !seen.insert(bytes(property.name).to_vec()) {
                continue;
            }
            let info = property::info(ce, property.name);
            if info.is_null() {
                continue;
            }
            let flags = (*info).flags;
            let visible = if flags & ZEND_ACC_PRIVATE != 0 {
                false
            } else if flags & ZEND_ACC_PROTECTED != 0 {
                !ancestor.is_null() && crate::class::instanceof_function(ancestor, (*info).ce)
            } else {
                true
            };
            if !visible {
                continue;
            }
            let mut value = zval::UNDEF;
            if property.default.u1.v.r#type == IS_UNDEF {
                value.u1.type_info = IS_NULL as u32;
            } else {
                copy(&mut value, &property.default);
            }
            hash::zend_hash_add(result, property.name, &mut value);
        }
    }
    result
}

/// php-src's key for a declared property in a properties table: `"\0Class\0name"` for a
/// private one, `"\0*\0name"` for a protected one, the name for a public one.
unsafe fn mangled_name(info: *mut zend_property_info) -> *mut zend_string {
    let name = bytes((*info).name);
    let mut key = Vec::with_capacity(name.len() + 16);
    if (*info).flags & ZEND_ACC_PRIVATE != 0 {
        key.push(0);
        key.extend_from_slice(bytes((*(*info).ce).name));
        key.push(0);
    } else if (*info).flags & ZEND_ACC_PROTECTED != 0 {
        key.extend_from_slice(b"\0*\0");
    }
    key.extend_from_slice(name);
    string::new(&key)
}

/// Whether the engine answers an access to property `name` of `object` itself, in the program
/// access's scope: php-src's standard handlers call a class's `__get`, `__set`, `__isset` or
/// `__unset` only for a name that scope cannot reach or the object does not hold. A class with
/// its own read handler decides what it holds through its `has_property` handler.
pub unsafe fn answers_directly(object: *mut zend_object, name: *mut zend_string) -> bool {
    let handlers = (*object).handlers;
    let standard = (*handlers).read_property.map(|read| read as *const () as usize) == Some(zend_std_read_property as *const () as usize);
    if !standard {
        return match (*handlers).has_property {
            Some(has) => has(object, name, ZEND_PROPERTY_EXISTS as c_int, ptr::null_mut()) != 0,
            None => false,
        };
    }
    let info = property::info((*object).ce, name);
    match property::visibility(object, info) {
        Access::Allowed => !info.is_null() || !hash::zend_hash_find_ind(zend_elephc_object_properties(object), name).is_null(),
        Access::Undeclared => {
            let table = shadowing(object, false);
            !table.is_null() && !hash::zend_hash_find_ind(table, name).is_null()
        }
        Access::Denied | Access::Probe => false,
    }
}

/// Whether `object` may gain the dynamic property `name` (php-src's `zend_std_write_property`):
/// a class that forbids dynamic properties throws PHP's Error, one that does not allow them
/// deprecates their creation.
unsafe fn may_create_dynamic(object: *mut zend_object, name: *mut zend_string) -> bool {
    let flags = (*(*object).ce).ce_flags;
    let named = || format!("{}::${}", property::display_class(object), string::text(name));
    if flags & ZEND_ACC_NO_DYNAMIC_PROPERTIES != 0 {
        crate::exception::throw_error(crate::exception::zend_ce_error, &format!("Cannot create dynamic property {}", named()));
        return false;
    }
    if flags & ZEND_ACC_ALLOW_DYNAMIC_PROPERTIES == 0 {
        crate::error::deprecated(&format!("Creation of dynamic property {} is deprecated", named()));
    }
    crate::startup::eg().exception.is_null()
}

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

/// Warns that `name` is not a property of `object` (unless the read is isset- or unset-style) and
/// yields the engine's uninitialized zval.
unsafe fn undefined_property(object: *mut zend_object, name: *mut zend_string, r#type: c_int) -> *mut zval {
    if !property::is_silent(r#type) {
        crate::error::warning(&format!("Undefined property: {}::${}", property::display_class(object), string::text(name)));
    }
    &mut crate::startup::eg().uninitialized_zval
}

/// `zend_std_read_property`: reads a property from the object's property table, as the accessing
/// scope may (see `crate::property`). A missing one raises an "Undefined property" warning, or
/// PHP's Error for a typed property never initialized (except for isset/unset style reads), and
/// yields the engine's uninitialized zval.
#[no_mangle]
pub unsafe extern "C" fn zend_std_read_property(
    object: *mut zend_object,
    name: *mut zend_string,
    r#type: c_int,
    _cache_slot: *mut *mut c_void,
    _rv: *mut zval,
) -> *mut zval {
    let info = property::info((*object).ce, name);
    match property::visibility(object, info) {
        Access::Allowed => {}
        Access::Undeclared => {
            let table = shadowing(object, false);
            let found = if table.is_null() { ptr::null_mut() } else { hash::zend_hash_find_ind(table, name) };
            return if found.is_null() { undefined_property(object, name, r#type) } else { found };
        }
        Access::Denied => {
            if r#type != BP_VAR_IS {
                property::denied(object, info);
            }
            return &mut crate::startup::eg().uninitialized_zval;
        }
        Access::Probe => {
            property::need_scope();
            return &mut crate::startup::eg().uninitialized_zval;
        }
    }
    let found = hash::zend_hash_find_ind(zend_elephc_object_properties(object), name);
    if !found.is_null() {
        return found;
    }
    if property::is_typed(info) {
        if !property::is_silent(r#type) {
            property::uninitialized(info);
        }
        return &mut crate::startup::eg().uninitialized_zval;
    }
    undefined_property(object, name, r#type)
}

/// `zend_std_write_property`: stores a copy of `value` under the property name, creating the
/// property if needed, and returns its slot. A declared property is written only as the accessing
/// scope may, a readonly one only once, and a typed one only with a value of its type (converted
/// in weak mode); a refused write throws PHP's error and returns `EG(error_zval)`.
#[no_mangle]
pub unsafe extern "C" fn zend_std_write_property(
    object: *mut zend_object,
    name: *mut zend_string,
    value: *mut zval,
    _cache_slot: *mut *mut c_void,
) -> *mut zval {
    let refused = &mut crate::startup::eg().error_zval as *mut zval;
    let info = property::info((*object).ce, name);
    match property::visibility(object, info) {
        Access::Allowed => {}
        // A parent's private name is a dynamic property of its own to this scope.
        Access::Undeclared => {
            let mut stored = zval::UNDEF;
            copy(&mut stored, value);
            let table = shadowing(object, false);
            if (table.is_null() || hash::zend_hash_find_ind(table, name).is_null()) && !may_create_dynamic(object, name) {
                zval_ptr_dtor(&mut stored);
                return refused;
            }
            return hash::zend_hash_update_ind(shadowing(object, true), name, &mut stored);
        }
        Access::Denied => {
            property::denied(object, info);
            return refused;
        }
        Access::Probe => {
            property::need_scope();
            return refused;
        }
    }
    let table = properties(object);
    if property::is_readonly(info) {
        if !hash::zend_hash_find_ind(table, name).is_null() {
            property::readonly_modified(info, false);
            return refused;
        }
        match property::readonly_scope_allows(object, info, false) {
            Some(true) => {}
            Some(false) => return refused,
            None => {
                property::need_scope();
                return refused;
            }
        }
    }
    let mut stored = zval::UNDEF;
    if property::is_typed(info) {
        copy(&mut stored, crate::zval::deref(value));
        if !property::verify_type(info, &mut stored, property::uses_strict_types(object)) {
            zval_ptr_dtor(&mut stored);
            return refused;
        }
    } else {
        if info.is_null() && hash::zend_hash_find_ind(table, name).is_null() && !may_create_dynamic(object, name) {
            return refused;
        }
        copy(&mut stored, value);
    }
    hash::zend_hash_update_ind(table, name, &mut stored)
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
    // A property the scope may not reach, or whose writes are checked, has no slot to hand out:
    // the caller falls back to the read and write handlers, as php-src's callers do.
    let info = property::info((*object).ce, name);
    if !info.is_null()
        && (!matches!(property::visibility(object, info), Access::Allowed) || property::is_readonly(info) || property::is_typed(info))
    {
        return ptr::null_mut();
    }
    let table = properties(object);
    let found = hash::zend_hash_find_ind(table, name);
    if !found.is_null() || r#type == BP_VAR_R || r#type == BP_VAR_IS {
        return found;
    }
    if info.is_null() && !may_create_dynamic(object, name) {
        return &mut crate::startup::eg().error_zval;
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
    let table = match property::visibility(object, property::info((*object).ce, name)) {
        Access::Allowed => zend_elephc_object_properties(object),
        Access::Probe => {
            property::need_scope();
            return 0;
        }
        Access::Undeclared => shadowing(object, false),
        Access::Denied => return 0,
    };
    if table.is_null() {
        return 0;
    }
    let found = hash::zend_hash_find_ind(table, name);
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

/// `zend_std_unset_property`: removes a property from the object's property table, as the
/// accessing scope may; an initialized readonly property cannot be unset.
#[no_mangle]
pub unsafe extern "C" fn zend_std_unset_property(object: *mut zend_object, name: *mut zend_string, _cache_slot: *mut *mut c_void) {
    let info = property::info((*object).ce, name);
    match property::visibility(object, info) {
        Access::Allowed => {}
        Access::Undeclared => {
            let table = shadowing(object, false);
            if !table.is_null() {
                hash::zend_hash_del(table, name);
            }
            return;
        }
        Access::Denied => return property::denied(object, info),
        Access::Probe => return property::need_scope(),
    }
    if property::is_readonly(info) {
        if !(*object).properties.is_null() && !hash::zend_hash_find_ind((*object).properties, name).is_null() {
            return property::readonly_modified(info, true);
        }
        match property::readonly_scope_allows(object, info, true) {
            Some(_) => return,
            None => return property::need_scope(),
        }
    }
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
/// their properties. Against a non-object, the object is cast to the other operand's type through
/// its `cast_object` handler and the two compared; when the cast fails an int or float target
/// notices and uses 1, any other target orders the object above the value.
#[no_mangle]
pub unsafe extern "C" fn zend_std_compare_objects(o1: *mut zval, o2: *mut zval) -> c_int {
    if type_of(o1) != type_of(o2) {
        let object_lhs = type_of(o1) == IS_OBJECT;
        let (object, value) = if object_lhs { (o1, o2) } else { (o2, o1) };
        let target = match type_of(value) {
            IS_FALSE | IS_TRUE => _IS_BOOL,
            other => other,
        };
        let zobj = (*object).value.obj;
        let mut casted = zval::UNDEF;
        let cast = match (*(*zobj).handlers).cast_object {
            Some(cast) => cast(zobj, &mut casted, c_int::from(target)),
            None => FAILURE,
        };
        if cast == FAILURE {
            if target == IS_LONG || target == IS_DOUBLE {
                let name = if target == IS_LONG { "int" } else { "float" };
                crate::error::report(
                    E_NOTICE,
                    format!("Object of class {} could not be converted to {name}", string::text((*(*zobj).ce).name)).as_bytes(),
                );
                if target == IS_LONG {
                    set_long(&mut casted, 1);
                } else {
                    set_double(&mut casted, 1.0);
                }
            } else {
                return if object_lhs { 1 } else { -1 };
            }
        }
        let result = if object_lhs {
            crate::convert::zend_compare(&mut casted, value)
        } else {
            crate::convert::zend_compare(value, &mut casted)
        };
        zval_ptr_dtor(&mut casted);
        return result;
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

/// Runs `body` with `EG(fake_scope)` set to `scope`, as php-src's property API does, so the
/// handlers check visibility and readonly rules from that class.
unsafe fn in_scope<T>(scope: *mut zend_class_entry, body: impl FnOnce() -> T) -> T {
    let outer = std::mem::replace(&mut crate::startup::eg().fake_scope, scope);
    let result = body();
    crate::startup::eg().fake_scope = outer;
    result
}

/// Updates a property by name from a zval, through the object's `write_property` handler (or the
/// standard write) run in `scope`, consuming the temporary name string.
unsafe fn update(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, len: usize, mut value: zval) {
    let key = string::zend_string_init(name, len, false);
    zend_update_property_ex(scope, object, key, &mut value);
    string::zend_string_release(key);
    zval_ptr_dtor(&mut value);
}

/// `zend_update_property_ex`: sets a property by `zend_string` name through the object's write
/// handler, run in `scope`.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_ex(scope: *mut zend_class_entry, object: *mut zend_object, name: *mut zend_string, value: *mut zval) {
    in_scope(scope, || match (*(*object).handlers).write_property {
        Some(write) => {
            write(object, name, value, ptr::null_mut());
        }
        None => {
            zend_std_write_property(object, name, value, ptr::null_mut());
        }
    })
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
pub unsafe extern "C" fn zend_update_property_null(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize) {
    update(scope, object, name, name_length, crate::zval::null());
}

/// `zend_update_property_bool`: sets a property to a boolean (non-zero is true).
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_bool(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: zend_long) {
    let mut zv = zval::UNDEF;
    crate::zval::set_bool(&mut zv, value != 0);
    update(scope, object, name, name_length, zv);
}

/// `zend_update_property_long`: sets a property to an integer.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_long(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: zend_long) {
    update(scope, object, name, name_length, crate::zval::long(value));
}

/// `zend_update_property_double`: sets a property to a float.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_double(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: f64) {
    let mut zv = zval::UNDEF;
    crate::zval::set_double(&mut zv, value);
    update(scope, object, name, name_length, zv);
}

/// `zend_update_property_str`: sets a property to a string, taking a reference to the given
/// `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_str(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: *mut zend_string) {
    let mut zv = zval::UNDEF;
    crate::zval::set_str_copy(&mut zv, value);
    update(scope, object, name, name_length, zv);
}

/// `zend_update_property_string`: sets a property to a copy of a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_string(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize, value: *const c_char) {
    let mut zv = zval::UNDEF;
    crate::zval::set_bytes(&mut zv, CStr::from_ptr(value).to_bytes());
    update(scope, object, name, name_length, zv);
}

/// `zend_update_property_stringl`: sets a property to a copy of a string given by pointer and
/// length.
#[no_mangle]
pub unsafe extern "C" fn zend_update_property_stringl(
    scope: *mut zend_class_entry,
    object: *mut zend_object,
    name: *const c_char,
    name_length: usize,
    value: *const c_char,
    value_length: usize,
) {
    let mut zv = zval::UNDEF;
    crate::zval::set_str(&mut zv, string::zend_string_init(value, value_length, false));
    update(scope, object, name, name_length, zv);
}

/// `zend_unset_property`: removes a property by name through the object's unset handler, run in
/// `scope`.
#[no_mangle]
pub unsafe extern "C" fn zend_unset_property(scope: *mut zend_class_entry, object: *mut zend_object, name: *const c_char, name_length: usize) {
    let key = string::zend_string_init(name, name_length, false);
    if let Some(unset) = (*(*object).handlers).unset_property {
        in_scope(scope, || unset(object, key, ptr::null_mut()));
    }
    string::zend_string_release(key);
}

/// `zend_read_property_ex`: reads a property by `zend_string` name through the object's read
/// handler, run in `scope`; `silent` makes it an isset-style read that raises no undefined-property
/// diagnostics, and `rv` is scratch storage for the result.
#[no_mangle]
pub unsafe extern "C" fn zend_read_property_ex(
    scope: *mut zend_class_entry,
    object: *mut zend_object,
    name: *mut zend_string,
    silent: bool,
    rv: *mut zval,
) -> *mut zval {
    let r#type = if silent { BP_VAR_IS } else { BP_VAR_R };
    in_scope(scope, || match (*(*object).handlers).read_property {
        Some(read) => read(object, name, r#type, ptr::null_mut(), rv),
        None => zend_std_read_property(object, name, r#type, ptr::null_mut(), rv),
    })
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
