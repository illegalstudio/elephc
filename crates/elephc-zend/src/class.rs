//! Purpose:
//! Classes and interfaces: registration from an extension's
//! `INIT_CLASS_ENTRY`, inheritance, interface implementation, lookup,
//! declared properties and class constants, attributes, enums.
//!
//! Called from:
//! - Extension MINIT code (`zend_register_internal_class_ex`,
//!   `zend_declare_typed_property`...), `crate::exception` for the built-in
//!   hierarchy, `crate::object` for instantiation.
//!
//! Key details:
//! - A registered class is a heap copy of the extension's stack entry, owned
//!   by `CG(class_table)` under its lowercase name, with its methods in its own
//!   `function_table`.
//! - Declared properties live outside the object layout: every property of an
//!   object is an entry of its properties table (`default_properties_count`
//!   stays 0, so `zend_object_alloc` reserves no slots), and the defaults
//!   recorded here seed that table when the object is created. Visibility is
//!   recorded but not enforced: hosted code is the extension itself.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;

use crate::abi::*;
use crate::alloc::{alloc_struct, erealloc};
use crate::global::Global;
use crate::hash;
use crate::startup::cg;
use crate::string::{self, bytes, intern};
use crate::zval::{copy, copy_value, zval_ptr_dtor};

/// A declared property's default, in declaration order per class.
pub struct DeclaredProperty {
    pub name: *mut zend_string,
    pub default: zval,
    pub flags: u32,
}

static DECLARED: Global<Option<HashMap<usize, Vec<DeclaredProperty>>>> = Global::new(None);
static STATICS: Global<Option<HashMap<(usize, Vec<u8>), *mut zval>>> = Global::new(None);

// Built-in classes extensions name directly; set by `crate::exception`.
#[no_mangle]
pub static mut zend_standard_class_def: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_closure: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_traversable: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_aggregate: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_iterator: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_arrayaccess: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_serializable: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_countable: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_stringable: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_internal_iterator: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_unit_enum: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_backed_enum: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_allow_dynamic_properties: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_sensitive_parameter: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_attribute: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut php_json_serializable_ce: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut php_json_exception_ce: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut php_ce_incomplete_class: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_Countable: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_ArrayIterator: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_RecursiveIterator: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_SeekableIterator: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_OuterIterator: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_SplFixedArray: *mut zend_class_entry = ptr::null_mut();

/// Returns the instance properties declared on `ce` through the C API (name, default value, flags),
/// or `None` if none were declared.
pub unsafe fn declared(ce: *const zend_class_entry) -> Option<&'static Vec<DeclaredProperty>> {
    DECLARED.get().get_or_insert_with(HashMap::new).get(&(ce as usize))
}

/// Returns the mutable declared-property list for `ce`, creating an empty one on first use.
unsafe fn declared_mut(ce: *const zend_class_entry) -> &'static mut Vec<DeclaredProperty> {
    DECLARED.get().get_or_insert_with(HashMap::new).entry(ce as usize).or_default()
}

/// The class chain from the root ancestor down to `ce`.
pub unsafe fn lineage(ce: *const zend_class_entry) -> Vec<*const zend_class_entry> {
    let mut chain = Vec::new();
    let mut current = ce;
    while !current.is_null() {
        chain.push(current);
        current = (*current).parent_u.parent;
    }
    chain.reverse();
    chain
}

/// Prepares a class entry: initialises its function, property-info and constants tables, sets
/// refcount 1, clears the default property tables, and installs the standard object handlers if
/// none are set. The `nullify_handlers` flag is ignored.
#[no_mangle]
pub unsafe extern "C" fn zend_initialize_class_data(ce: *mut zend_class_entry, _nullify_handlers: bool) {
    hash::init(&mut (*ce).function_table, 8, None, true);
    hash::init(&mut (*ce).properties_info, 8, None, true);
    hash::init(&mut (*ce).constants_table, 8, None, true);
    (*ce).refcount = 1;
    (*ce).default_properties_count = 0;
    (*ce).default_static_members_count = 0;
    (*ce).default_properties_table = ptr::null_mut();
    (*ce).default_static_members_table = ptr::null_mut();
    (*ce).properties_info_table = ptr::null_mut();
    if (*ce).default_object_handlers.is_null() {
        (*ce).default_object_handlers = &crate::object::std_object_handlers;
    }
}

/// Returns the class-table key for a name: the bytes without a leading backslash, ASCII-lowercased.
unsafe fn class_key(name: *const zend_string) -> Vec<u8> {
    let bytes = bytes(name);
    let bytes = bytes.strip_prefix(b"\\").unwrap_or(bytes);
    bytes.to_ascii_lowercase()
}

/// Copies an extension's class entry into the engine and registers it.
unsafe fn register(class_entry: *mut zend_class_entry, parent: *mut zend_class_entry, flags: u32) -> *mut zend_class_entry {
    let ce = alloc_struct::<zend_class_entry>();
    ptr::copy_nonoverlapping(class_entry, ce, 1);
    let builtin = (*ce).info.internal.builtin_functions;
    zend_initialize_class_data(ce, false);
    (*ce).r#type = ZEND_INTERNAL_CLASS as c_char;
    (*ce).ce_flags |= flags | ZEND_ACC_LINKED | ZEND_ACC_RESOLVED_PARENT | ZEND_ACC_RESOLVED_INTERFACES | ZEND_ACC_CONSTANTS_UPDATED;
    (*ce).info.internal.module = crate::module::current();
    (*ce).parent_u.parent = ptr::null_mut();
    (*ce).num_interfaces = 0;
    (*ce).interfaces_u.interfaces = ptr::null_mut();
    if !builtin.is_null() {
        crate::module::zend_register_functions(ce, builtin, &mut (*ce).function_table, MODULE_PERSISTENT);
    }
    if !parent.is_null() {
        zend_do_inheritance_ex(ce, parent, false);
    }
    let key = class_key((*ce).name);
    if hash::zend_hash_str_add_ptr(cg().class_table, key.as_ptr().cast(), key.len(), ce.cast()).is_null() {
        crate::error::report(
            E_CORE_WARNING,
            format!("Cannot declare class {}, because the name is already in use", string::text((*ce).name)).as_bytes(),
        );
    }
    ce
}

/// Registers an extension's class entry as a new internal class with no parent; returns the
/// engine's copy.
#[no_mangle]
pub unsafe extern "C" fn zend_register_internal_class(class_entry: *mut zend_class_entry) -> *mut zend_class_entry {
    register(class_entry, ptr::null_mut(), 0)
}

/// Registers an extension's class entry as an internal class extending `parent_ce` (inheriting its
/// methods, properties, constants, interfaces and hooks); returns the engine's copy.
#[no_mangle]
pub unsafe extern "C" fn zend_register_internal_class_ex(
    class_entry: *mut zend_class_entry,
    parent_ce: *mut zend_class_entry,
) -> *mut zend_class_entry {
    register(class_entry, parent_ce, 0)
}

/// Like `zend_register_internal_class_ex` with extra `ZEND_ACC_*` class flags set on the new class.
#[no_mangle]
pub unsafe extern "C" fn zend_register_internal_class_with_flags(
    class_entry: *mut zend_class_entry,
    parent_ce: *mut zend_class_entry,
    flags: u32,
) -> *mut zend_class_entry {
    register(class_entry, parent_ce, flags)
}

/// Registers an extension's class entry as an internal interface (`ZEND_ACC_INTERFACE`).
#[no_mangle]
pub unsafe extern "C" fn zend_register_internal_interface(orig_class_entry: *mut zend_class_entry) -> *mut zend_class_entry {
    register(orig_class_entry, ptr::null_mut(), ZEND_ACC_INTERFACE)
}

/// Makes `ce` a subclass of `parent_ce`: methods, properties, constants and
/// interfaces it does not redefine, and the parent's object hooks.
#[no_mangle]
pub unsafe extern "C" fn zend_do_inheritance_ex(ce: *mut zend_class_entry, parent_ce: *mut zend_class_entry, _checked: bool) {
    (*ce).parent_u.parent = parent_ce;
    if (*ce).create_u.create_object.is_none() {
        (*ce).create_u.create_object = (*parent_ce).create_u.create_object;
    }
    if (*ce).default_object_handlers == &crate::object::std_object_handlers as *const _ {
        (*ce).default_object_handlers = (*parent_ce).default_object_handlers;
    }
    if (*ce).get_iterator.is_none() {
        (*ce).get_iterator = (*parent_ce).get_iterator;
    }
    if (*ce).serialize.is_none() {
        (*ce).serialize = (*parent_ce).serialize;
    }
    if (*ce).unserialize.is_none() {
        (*ce).unserialize = (*parent_ce).unserialize;
    }
    if (*parent_ce).ce_flags & ZEND_ACC_ALLOW_DYNAMIC_PROPERTIES != 0 {
        (*ce).ce_flags |= ZEND_ACC_ALLOW_DYNAMIC_PROPERTIES;
    }
    if (*parent_ce).ce_flags & ZEND_ACC_NOT_SERIALIZABLE != 0 {
        (*ce).ce_flags |= ZEND_ACC_NOT_SERIALIZABLE;
    }
    let parent_functions = &mut (*parent_ce).function_table as *mut HashTable;
    for idx in 0..(*parent_functions).nNumUsed {
        let p = hash::bucket(parent_functions, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF || (*p).key.is_null() {
            continue;
        }
        if hash::zend_hash_find(&(*ce).function_table, (*p).key).is_null() {
            hash::zend_hash_add_ptr(&mut (*ce).function_table, (*p).key, (*p).val.value.ptr);
        }
    }
    macro_rules! inherit_magic {
        ($($field:ident),*) => { $( if (*ce).$field.is_null() { (*ce).$field = (*parent_ce).$field; } )* };
    }
    inherit_magic!(constructor, destructor, clone, __get, __set, __unset, __isset, __call, __callstatic, __tostring, __debugInfo, __serialize, __unserialize);
    for table in [&mut (*parent_ce).properties_info as *mut HashTable, &mut (*parent_ce).constants_table] {
        let constants = table != &mut (*parent_ce).properties_info as *mut HashTable;
        let own = if constants {
            &mut (*ce).constants_table as *mut HashTable
        } else {
            &mut (*ce).properties_info as *mut HashTable
        };
        for idx in 0..(*table).nNumUsed {
            let p = hash::bucket(table, idx);
            if (*p).val.u1.v.r#type == IS_UNDEF || (*p).key.is_null() {
                continue;
            }
            // php-src's `do_inherit_class_constant`: a private constant stays the parent's, so a
            // child may declare its own of that name.
            if constants {
                let constant = (*p).val.value.ptr.cast::<zend_class_constant>();
                if (*constant).value.u2.constant_flags & ZEND_ACC_PRIVATE != 0 {
                    continue;
                }
            }
            if hash::zend_hash_find(own, (*p).key).is_null() {
                hash::zend_hash_add_ptr(own, (*p).key, (*p).val.value.ptr);
            }
        }
    }
    for i in 0..(*parent_ce).num_interfaces {
        add_interface(ce, *(*parent_ce).interfaces_u.interfaces.add(i as usize));
    }
}

/// Makes `ce` inherit from `parent_ce`; see `zend_do_inheritance_ex`.
#[no_mangle]
pub unsafe extern "C" fn zend_do_inheritance(ce: *mut zend_class_entry, parent_ce: *mut zend_class_entry) {
    zend_do_inheritance_ex(ce, parent_ce, false);
}

/// True when `iface` is in the class's own flattened interface list (not found through parents).
unsafe fn implements_directly(ce: *const zend_class_entry, iface: *const zend_class_entry) -> bool {
    (0..(*ce).num_interfaces).any(|i| *(*ce).interfaces_u.interfaces.add(i as usize) as *const _ == iface)
}

/// Adds an interface to a class if not already present, along with the interface's parent
/// interfaces and constants (not overriding existing ones), then calls the interface's
/// `interface_gets_implemented` hook.
unsafe fn add_interface(ce: *mut zend_class_entry, iface: *mut zend_class_entry) {
    if iface.is_null() || implements_directly(ce, iface) {
        return;
    }
    let count = (*ce).num_interfaces as usize;
    let list = erealloc(
        (*ce).interfaces_u.interfaces.cast::<c_void>(),
        (count + 1) * std::mem::size_of::<*mut zend_class_entry>(),
    )
    .cast::<*mut zend_class_entry>();
    *list.add(count) = iface;
    (*ce).interfaces_u.interfaces = list;
    (*ce).num_interfaces += 1;
    for i in 0..(*iface).num_interfaces {
        add_interface(ce, *(*iface).interfaces_u.interfaces.add(i as usize));
    }
    let constants = &mut (*iface).constants_table as *mut HashTable;
    for idx in 0..(*constants).nNumUsed {
        let p = hash::bucket(constants, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF || (*p).key.is_null() {
            continue;
        }
        if hash::zend_hash_find(&(*ce).constants_table, (*p).key).is_null() {
            hash::zend_hash_add_ptr(&mut (*ce).constants_table, (*p).key, (*p).val.value.ptr);
        }
    }
    if let Some(hook) = (*iface).create_u.interface_gets_implemented {
        if (*iface).ce_flags & ZEND_ACC_INTERFACE != 0 {
            hook(iface, ce);
        }
    }
}

/// `zend_class_implements` after the C side gathered its arguments.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_class_implements_list(ce: *mut zend_class_entry, count: c_int, interfaces: *mut *mut zend_class_entry) {
    for i in 0..count.max(0) {
        add_interface(ce, *interfaces.add(i as usize));
    }
}

/// Makes `ce` implement `iface` (adds it, its parent interfaces and its constants, and runs the
/// implemented hook).
#[no_mangle]
pub unsafe extern "C" fn zend_do_implement_interface(ce: *mut zend_class_entry, iface: *mut zend_class_entry) {
    add_interface(ce, iface);
}

/// True when `instance_ce` is `ce`, a subclass of it, or (for an interface `ce`) implements it,
/// walking the parent chain; false if either pointer is null.
#[no_mangle]
pub unsafe extern "C" fn instanceof_function_slow(instance_ce: *const zend_class_entry, ce: *const zend_class_entry) -> bool {
    if instance_ce.is_null() || ce.is_null() {
        return false;
    }
    let interface = (*ce).ce_flags & ZEND_ACC_INTERFACE != 0;
    let mut current = instance_ce;
    while !current.is_null() {
        if current == ce || (interface && implements_directly(current, ce)) {
            return true;
        }
        current = (*current).parent_u.parent;
    }
    false
}

/// `instanceof` test between two class entries: a pointer-equality fast path, then
/// `instanceof_function_slow`.
#[no_mangle]
pub unsafe extern "C" fn instanceof_function(instance_ce: *const zend_class_entry, ce: *const zend_class_entry) -> bool {
    instance_ce == ce || instanceof_function_slow(instance_ce, ce)
}

/// True when `class_ce` is, extends or implements `interface_ce`.
#[no_mangle]
pub unsafe extern "C" fn zend_class_implements_interface(class_ce: *const zend_class_entry, interface_ce: *const zend_class_entry) -> bool {
    instanceof_function_slow(class_ce, interface_ce)
}

/// The class registered under `name` (case-insensitive, leading `\` ignored).
pub unsafe fn lookup(name: &[u8]) -> *mut zend_class_entry {
    let name = name.strip_prefix(b"\\").unwrap_or(name);
    let lower = name.to_ascii_lowercase();
    hash::zend_hash_str_find_ptr(cg().class_table, lower.as_ptr().cast(), lower.len()).cast()
}

/// Finds a class by name in the class table, case-insensitively and ignoring a leading backslash;
/// null if unknown (no autoloading).
#[no_mangle]
pub unsafe extern "C" fn zend_lookup_class(name: *mut zend_string) -> *mut zend_class_entry {
    lookup(bytes(name))
}

/// Like `zend_lookup_class`, using the supplied lowercase name `lcname` when given; the flags are
/// ignored.
#[no_mangle]
pub unsafe extern "C" fn zend_lookup_class_ex(name: *mut zend_string, lcname: *mut zend_string, _flags: u32) -> *mut zend_class_entry {
    if lcname.is_null() {
        lookup(bytes(name))
    } else {
        lookup(bytes(lcname))
    }
}

/// Looks a class up by name and, unless `ZEND_FETCH_CLASS_SILENT` is set or an exception is already
/// pending, throws `Error: Class "X" not found` when it is missing.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_class_by_name(
    class_name: *mut zend_string,
    lcname: *mut zend_string,
    fetch_type: u32,
) -> *mut zend_class_entry {
    let ce = zend_lookup_class_ex(class_name, lcname, fetch_type);
    if ce.is_null() && fetch_type & ZEND_FETCH_CLASS_SILENT == 0 && crate::startup::eg().exception.is_null() {
        let kind = match fetch_type & ZEND_FETCH_CLASS_MASK {
            ZEND_FETCH_CLASS_INTERFACE => "Interface",
            ZEND_FETCH_CLASS_TRAIT => "Trait",
            _ => "Class",
        };
        crate::exception::throw_error(
            crate::exception::zend_ce_error,
            &format!("{kind} \"{}\" not found", string::text(class_name)),
        );
    }
    ce
}

/// php-src's `zend_get_class_fetch_type`: the fetch type a class name spells, `self`, `parent` or
/// `static` (case-insensitive, whole name), otherwise the default.
pub fn class_fetch_type(name: &[u8]) -> u32 {
    if name.eq_ignore_ascii_case(b"self") {
        ZEND_FETCH_CLASS_SELF
    } else if name.eq_ignore_ascii_case(b"parent") {
        ZEND_FETCH_CLASS_PARENT
    } else if name.eq_ignore_ascii_case(b"static") {
        ZEND_FETCH_CLASS_STATIC
    } else {
        ZEND_FETCH_CLASS_DEFAULT
    }
}

/// Resolves a class reference by fetch type, as php-src does: `self` gives the executing scope,
/// `parent` its parent, `static` the called scope (late static binding), `ZEND_FETCH_CLASS_AUTO`
/// decides among them by the name itself, anything else the named class via
/// `zend_fetch_class_by_name`. A missing scope is an Error with `ZEND_FETCH_CLASS_EXCEPTION`, a
/// fatal error otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_class(class_name: *mut zend_string, fetch_type: u32) -> *mut zend_class_entry {
    let missing = |message: &str| -> *mut zend_class_entry {
        if fetch_type & ZEND_FETCH_CLASS_EXCEPTION != 0 {
            crate::exception::throw_error(crate::exception::zend_ce_error, message);
        } else {
            crate::error::fatal(message);
        }
        ptr::null_mut()
    };
    let mut sub_type = fetch_type & ZEND_FETCH_CLASS_MASK;
    if sub_type == ZEND_FETCH_CLASS_AUTO && !class_name.is_null() {
        sub_type = class_fetch_type(bytes(class_name));
    }
    match sub_type {
        ZEND_FETCH_CLASS_SELF => {
            let scope = crate::api::zend_get_executed_scope();
            if scope.is_null() {
                return missing("Cannot access \"self\" when no class scope is active");
            }
            scope
        }
        ZEND_FETCH_CLASS_PARENT => {
            let scope = crate::api::zend_get_executed_scope();
            if scope.is_null() {
                return missing("Cannot access \"parent\" when no class scope is active");
            }
            if (*scope).parent_u.parent.is_null() {
                return missing("Cannot access \"parent\" when current class scope has no parent");
            }
            (*scope).parent_u.parent
        }
        ZEND_FETCH_CLASS_STATIC => {
            let called = crate::api::zend_get_called_scope(crate::startup::eg().current_execute_data);
            if called.is_null() {
                return missing("Cannot access \"static\" when no class scope is active");
            }
            called
        }
        _ => zend_fetch_class_by_name(class_name, ptr::null_mut(), fetch_type),
    }
}

/// Returns the class entry of an object.
#[no_mangle]
pub unsafe extern "C" fn zend_get_class_entry(object: *const zend_object) -> *mut zend_class_entry {
    (*object).ce
}

/// Registers a lowercased alias name for a class in the class table; `FAILURE` if the name is
/// already taken.
#[no_mangle]
pub unsafe extern "C" fn zend_register_class_alias_ex(
    name: *const c_char,
    name_len: usize,
    ce: *mut zend_class_entry,
    _persistent: bool,
) -> zend_result {
    let lower: Vec<u8> = std::slice::from_raw_parts(name.cast::<u8>(), name_len).to_ascii_lowercase();
    if hash::zend_hash_str_add_ptr(cg().class_table, lower.as_ptr().cast(), lower.len(), ce.cast()).is_null() {
        FAILURE
    } else {
        SUCCESS
    }
}

/// Returns the word for a class entry's kind (`class`, `interface`, `trait` or `enum`), capitalised
/// when `upper_case` is true.
#[no_mangle]
pub unsafe extern "C" fn zend_get_object_type_case(ce: *const zend_class_entry, upper_case: bool) -> *const c_char {
    let flags = (*ce).ce_flags;
    match (flags & ZEND_ACC_TRAIT != 0, flags & ZEND_ACC_INTERFACE != 0, flags & ZEND_ACC_ENUM != 0, upper_case) {
        (true, _, _, false) => c"trait".as_ptr(),
        (true, _, _, true) => c"Trait".as_ptr(),
        (_, true, _, false) => c"interface".as_ptr(),
        (_, true, _, true) => c"Interface".as_ptr(),
        (_, _, true, false) => c"enum".as_ptr(),
        (_, _, true, true) => c"Enum".as_ptr(),
        (_, _, _, false) => c"class".as_ptr(),
        (_, _, _, true) => c"Class".as_ptr(),
    }
}

/// True when every byte of the name is alphanumeric, `_`, `\` or a high (non-ASCII) byte.
#[no_mangle]
pub unsafe extern "C" fn zend_is_valid_class_name(name: *mut zend_string) -> bool {
    bytes(name).iter().all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'\\' || c >= 0x80)
}

// ------------------------------------------------------------ properties

/// Declares a property on a class with a type: records its `zend_property_info` (public by
/// default), and stores either a static slot holding the initial value or an instance default (null
/// when untyped and no default). Returns the property info.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_typed_property(
    ce: *mut zend_class_entry,
    name: *mut zend_string,
    property: *mut zval,
    access_type: c_int,
    doc_comment: *mut zend_string,
    r#type: zend_type,
) -> *mut zend_property_info {
    let mut access = access_type as u32;
    if access & ZEND_ACC_PPP_MASK == 0 {
        access |= ZEND_ACC_PUBLIC;
    }
    let name = intern(bytes(name));
    let info = alloc_struct::<zend_property_info>();
    (*info).flags = access;
    (*info).name = name;
    (*info).doc_comment = doc_comment;
    (*info).ce = ce;
    (*info).r#type = r#type;
    hash::zend_hash_update_ptr(&mut (*ce).properties_info, name, info.cast());
    if access & ZEND_ACC_STATIC != 0 {
        let slot = crate::alloc::alloc_struct::<zval>();
        if !property.is_null() {
            copy_value(slot, property);
        } else {
            (*slot).u1.type_info = IS_NULL as u32;
        }
        // A redeclaration replaces the previous default, which php-src releases.
        if let Some(previous) = STATICS.get().get_or_insert_with(HashMap::new).insert((ce as usize, bytes(name).to_vec()), slot) {
            zval_ptr_dtor(previous);
            crate::alloc::free(previous.cast());
        }
        return info;
    }
    let mut default = zval::UNDEF;
    if !property.is_null() && (*property).u1.v.r#type != IS_UNDEF {
        copy_value(&mut default, property);
    } else if r#type.type_mask & _ZEND_TYPE_MASK == 0 {
        default.u1.type_info = IS_NULL as u32;
    }
    let list = declared_mut(ce);
    list.retain_mut(|declared| {
        if bytes(declared.name) != bytes(name) {
            return true;
        }
        zval_ptr_dtor(&mut declared.default);
        false
    });
    list.push(DeclaredProperty { name, default, flags: access });
    info
}

/// Declares an untyped property from a `zend_string` name, default value, access flags and doc
/// comment.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_property_ex(
    ce: *mut zend_class_entry,
    name: *mut zend_string,
    property: *mut zval,
    access_type: c_int,
    doc_comment: *mut zend_string,
) {
    zend_declare_typed_property(ce, name, property, access_type, doc_comment, zend_type { ptr: ptr::null_mut(), type_mask: 0 });
}

/// Interns a C-string property name and declares the property with the given value and access
/// flags.
unsafe fn declare_named(ce: *mut zend_class_entry, name: *const c_char, len: usize, mut value: zval, access_type: c_int) {
    let name = string::zend_string_init_interned(name, len, true);
    zend_declare_property_ex(ce, name, &mut value, access_type, ptr::null_mut());
}

/// Declares a property with a default copied from a zval, using a C-string name.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_property(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, property: *mut zval, access_type: c_int) {
    let mut value = zval::UNDEF;
    copy_value(&mut value, property);
    declare_named(ce, name, name_length, value, access_type);
}

/// Declares a property whose default is `null`.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_property_null(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, access_type: c_int) {
    declare_named(ce, name, name_length, crate::zval::null(), access_type);
}

/// Declares a boolean property default (non-zero is true).
#[no_mangle]
pub unsafe extern "C" fn zend_declare_property_bool(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, value: zend_long, access_type: c_int) {
    let mut zv = zval::UNDEF;
    crate::zval::set_bool(&mut zv, value != 0);
    declare_named(ce, name, name_length, zv, access_type);
}

/// Declares an integer property default.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_property_long(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, value: zend_long, access_type: c_int) {
    declare_named(ce, name, name_length, crate::zval::long(value), access_type);
}

/// Declares a float property default.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_property_double(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, value: f64, access_type: c_int) {
    let mut zv = zval::UNDEF;
    crate::zval::set_double(&mut zv, value);
    declare_named(ce, name, name_length, zv, access_type);
}

/// Declares a string property default from a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_property_string(
    ce: *mut zend_class_entry,
    name: *const c_char,
    name_length: usize,
    value: *const c_char,
    access_type: c_int,
) {
    zend_declare_property_stringl(ce, name, name_length, value, CStr::from_ptr(value).to_bytes().len(), access_type);
}

/// Declares a string property default from a buffer and length (stored as an interned string).
#[no_mangle]
pub unsafe extern "C" fn zend_declare_property_stringl(
    ce: *mut zend_class_entry,
    name: *const c_char,
    name_length: usize,
    value: *const c_char,
    value_len: usize,
    access_type: c_int,
) {
    let mut zv = zval::UNDEF;
    crate::zval::set_str(&mut zv, string::zend_string_init_interned(value, value_len, true));
    declare_named(ce, name, name_length, zv, access_type);
}

/// Returns the declared property info for a member name in the class (`properties_info` lookup), or
/// null; the silent flag is ignored.
#[no_mangle]
pub unsafe extern "C" fn zend_get_property_info(ce: *const zend_class_entry, member: *mut zend_string, _silent: c_int) -> *mut zend_property_info {
    hash::zend_hash_find_ptr(&(*ce).properties_info, member).cast()
}

/// Always returns null: typed-property info is not tracked per object slot.
#[no_mangle]
pub unsafe extern "C" fn zend_get_property_info_for_slot(_obj: *mut zend_object, _slot: *mut zval) -> *mut zend_property_info {
    ptr::null_mut()
}

/// Always returns null: typed-property info is not tracked per object slot.
#[no_mangle]
pub unsafe extern "C" fn zend_get_typed_property_info_for_slot(_obj: *mut zend_object, _slot: *mut zval) -> *mut zend_property_info {
    ptr::null_mut()
}

/// Finds the storage for a static property by name, searching the class and then its parents; null
/// if undeclared.
unsafe fn static_slot(scope: *mut zend_class_entry, name: &[u8]) -> *mut zval {
    let statics = STATICS.get().get_or_insert_with(HashMap::new);
    let mut current = scope;
    while !current.is_null() {
        if let Some(slot) = statics.get(&(current as usize, name.to_vec())) {
            return *slot;
        }
        current = (*current).parent_u.parent;
    }
    ptr::null_mut()
}

/// Returns the zval slot of a static property, throwing `Error: Access to undeclared static
/// property` when missing unless `silent`.
#[no_mangle]
pub unsafe extern "C" fn zend_read_static_property(scope: *mut zend_class_entry, name: *const c_char, name_length: usize, silent: bool) -> *mut zval {
    let name = std::slice::from_raw_parts(name.cast::<u8>(), name_length);
    let slot = static_slot(scope, name);
    if slot.is_null() && !silent {
        crate::exception::throw_error(
            crate::exception::zend_ce_error,
            &format!("Access to undeclared static property {}::${}", string::text((*scope).name), String::from_utf8_lossy(name)),
        );
    }
    slot
}

/// Assigns a value to an existing static property, releasing the old value and taking a reference
/// on the new one; `FAILURE` if the property is undeclared.
#[no_mangle]
pub unsafe extern "C" fn zend_update_static_property(scope: *mut zend_class_entry, name: *const c_char, name_length: usize, value: *mut zval) -> zend_result {
    let slot = zend_read_static_property(scope, name, name_length, false);
    if slot.is_null() {
        return FAILURE;
    }
    crate::zval::zval_ptr_dtor(slot);
    copy(slot, value);
    SUCCESS
}

/// Assigns an integer to a static property; see `zend_update_static_property`.
#[no_mangle]
pub unsafe extern "C" fn zend_update_static_property_long(scope: *mut zend_class_entry, name: *const c_char, name_length: usize, value: zend_long) -> zend_result {
    let mut zv = crate::zval::long(value);
    zend_update_static_property(scope, name, name_length, &mut zv)
}

/// No-op returning `SUCCESS`: class constants are already evaluated when declared.
#[no_mangle]
pub unsafe extern "C" fn zend_update_class_constants(_class_type: *mut zend_class_entry) -> zend_result {
    SUCCESS
}

/// No-op returning `SUCCESS`: constant expressions never need late evaluation here.
#[no_mangle]
pub unsafe extern "C" fn zval_update_constant_ex(_pp: *mut zval, _scope: *mut zend_class_entry) -> zend_result {
    SUCCESS
}

/// No-op returning `SUCCESS`: constant expressions never need late evaluation here.
#[no_mangle]
pub unsafe extern "C" fn zval_update_constant(_pp: *mut zval) -> zend_result {
    SUCCESS
}

// ------------------------------------------------------------ class constants

/// Declares a class constant with a type: allocates its `zend_class_constant` (public by default,
/// storing the flags), and registers it; a duplicate name is php-src's fatal `Cannot redefine
/// class constant`. Returns the constant.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_typed_class_constant(
    ce: *mut zend_class_entry,
    name: *mut zend_string,
    value: *mut zval,
    access_type: c_int,
    doc_comment: *mut zend_string,
    r#type: zend_type,
) -> *mut zend_class_constant {
    let constant = alloc_struct::<zend_class_constant>();
    copy_value(&mut (*constant).value, value);
    let mut flags = access_type as u32;
    if flags & ZEND_ACC_PPP_MASK == 0 {
        flags |= ZEND_ACC_PUBLIC;
    }
    (*constant).value.u2.constant_flags = flags;
    (*constant).doc_comment = doc_comment;
    (*constant).ce = ce;
    (*constant).r#type = r#type;
    let name = intern(bytes(name));
    if hash::zend_hash_add_ptr(&mut (*ce).constants_table, name, constant.cast()).is_null() {
        // php-src raises `E_CORE_ERROR` here: a duplicate never returns an orphan constant.
        crate::error::report(
            E_CORE_ERROR,
            format!("Cannot redefine class constant {}::{}", string::text((*ce).name), string::text(name)).as_bytes(),
        );
    }
    constant
}

/// Declares an untyped class constant from a `zend_string` name, value, access flags and doc
/// comment.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_class_constant_ex(
    ce: *mut zend_class_entry,
    name: *mut zend_string,
    value: *mut zval,
    access_type: c_int,
    doc_comment: *mut zend_string,
) -> *mut zend_class_constant {
    zend_declare_typed_class_constant(ce, name, value, access_type, doc_comment, zend_type { ptr: ptr::null_mut(), type_mask: 0 })
}

/// Interns a C-string constant name and declares it as a public class constant with the given
/// value.
unsafe fn declare_constant(ce: *mut zend_class_entry, name: *const c_char, len: usize, mut value: zval) {
    let name = string::zend_string_init_interned(name, len, true);
    zend_declare_class_constant_ex(ce, name, &mut value, ZEND_ACC_PUBLIC as c_int, ptr::null_mut());
}

/// Declares a public class constant whose value is copied from a zval.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_class_constant(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, value: *mut zval) {
    let mut zv = zval::UNDEF;
    copy_value(&mut zv, value);
    declare_constant(ce, name, name_length, zv);
}

/// Declares a public class constant with value `null`.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_class_constant_null(ce: *mut zend_class_entry, name: *const c_char, name_length: usize) {
    declare_constant(ce, name, name_length, crate::zval::null());
}

/// Declares a public integer class constant.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_class_constant_long(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, value: zend_long) {
    declare_constant(ce, name, name_length, crate::zval::long(value));
}

/// Declares a public boolean class constant.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_class_constant_bool(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, value: bool) {
    let mut zv = zval::UNDEF;
    crate::zval::set_bool(&mut zv, value);
    declare_constant(ce, name, name_length, zv);
}

/// Declares a public float class constant.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_class_constant_double(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, value: f64) {
    let mut zv = zval::UNDEF;
    crate::zval::set_double(&mut zv, value);
    declare_constant(ce, name, name_length, zv);
}

/// Declares a public string class constant from a buffer and length.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_class_constant_stringl(
    ce: *mut zend_class_entry,
    name: *const c_char,
    name_length: usize,
    value: *const c_char,
    value_length: usize,
) {
    let mut zv = zval::UNDEF;
    crate::zval::set_str(&mut zv, string::zend_string_init_interned(value, value_length, true));
    declare_constant(ce, name, name_length, zv);
}

/// Declares a public string class constant from a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn zend_declare_class_constant_string(ce: *mut zend_class_entry, name: *const c_char, name_length: usize, value: *const c_char) {
    zend_declare_class_constant_stringl(ce, name, name_length, value, CStr::from_ptr(value).to_bytes().len());
}

/// Whether code running in `scope` may read `constant`, as php-src's `zend_verify_const_access`:
/// a public constant is always readable, a private one only from the class that declared it, a
/// protected one from the declaring class's family (`zend_check_protected`).
pub unsafe fn constant_accessible(constant: *const zend_class_constant, scope: *const zend_class_entry) -> bool {
    let flags = (*constant).value.u2.constant_flags;
    if flags & ZEND_ACC_PUBLIC != 0 {
        true
    } else if flags & ZEND_ACC_PRIVATE != 0 {
        (*constant).ce as *const zend_class_entry == scope
    } else {
        check_protected((*constant).ce, scope)
    }
}

/// The word PHP's messages use for a class constant's visibility.
pub unsafe fn constant_visibility(constant: *const zend_class_constant) -> &'static str {
    let flags = (*constant).value.u2.constant_flags;
    if flags & ZEND_ACC_PRIVATE != 0 {
        "private"
    } else if flags & ZEND_ACC_PROTECTED != 0 {
        "protected"
    } else {
        "public"
    }
}

/// php-src's `zend_check_protected`: whether `scope` is `ce`, one of its parents, or a class
/// `ce` descends from.
pub unsafe fn check_protected(ce: *const zend_class_entry, scope: *const zend_class_entry) -> bool {
    if scope.is_null() {
        return false;
    }
    let mut parent = ce;
    while !parent.is_null() {
        if parent == scope {
            return true;
        }
        parent = (*parent).parent_u.parent;
    }
    let mut current = scope;
    while !current.is_null() {
        if current == ce {
            return true;
        }
        current = (*current).parent_u.parent;
    }
    false
}

/// Finds class constant `constant_name` of `ce` for code running in `scope`, as php-src's
/// `zend_get_class_constant_ex` does once it holds the class: an undefined constant throws
/// `Undefined constant C::X`, a private or protected one outside its scope throws
/// `Cannot access private constant C::X`, both unless `ZEND_FETCH_CLASS_SILENT` is set. NULL on
/// failure. `class_text` is the name the messages print.
pub unsafe fn read_class_constant(
    ce: *mut zend_class_entry,
    class_text: &str,
    constant_name: &[u8],
    scope: *mut zend_class_entry,
    flags: u32,
) -> *mut zval {
    let constant =
        hash::zend_hash_str_find_ptr(&(*ce).constants_table, constant_name.as_ptr().cast(), constant_name.len())
            .cast::<zend_class_constant>();
    let name = String::from_utf8_lossy(constant_name);
    if constant.is_null() {
        if flags & ZEND_FETCH_CLASS_SILENT == 0 {
            crate::exception::throw_error(crate::exception::zend_ce_error, &format!("Undefined constant {class_text}::{name}"));
        }
        return ptr::null_mut();
    }
    if !constant_accessible(constant, scope) {
        if flags & ZEND_FETCH_CLASS_SILENT == 0 {
            crate::exception::throw_error(
                crate::exception::zend_ce_error,
                &format!("Cannot access {} constant {class_text}::{name}", constant_visibility(constant)),
            );
        }
        return ptr::null_mut();
    }
    &mut (*constant).value
}

/// Fetches the class named `class_name` through `zend_fetch_class` with `flags` (so
/// `ZEND_FETCH_CLASS_AUTO` resolves `self`, `parent` and `static`) and returns the value slot of
/// its constant for code running in `scope`, or the executing scope when `scope` is NULL; see
/// `read_class_constant` for the failures. NULL when the class is unknown.
#[no_mangle]
pub unsafe extern "C" fn zend_get_class_constant_ex(
    class_name: *mut zend_string,
    constant_name: *mut zend_string,
    scope: *mut zend_class_entry,
    flags: u32,
) -> *mut zval {
    let scope = if scope.is_null() { crate::api::zend_get_executed_scope() } else { scope };
    let ce = zend_fetch_class(class_name, flags);
    if ce.is_null() {
        return ptr::null_mut();
    }
    read_class_constant(ce, &string::text(class_name), bytes(constant_name), scope, flags)
}

// ------------------------------------------------------------ attributes

/// Creates an attribute record (name, lowercased name, flags, argument slots initialised to
/// `UNDEF`, position info) and appends it to `*attributes`, creating that table if needed. Returns
/// the attribute for the caller to fill in its arguments.
#[no_mangle]
pub unsafe extern "C" fn zend_add_attribute(
    attributes: *mut *mut HashTable,
    name: *mut zend_string,
    argc: u32,
    flags: u32,
    offset: u32,
    lineno: u32,
) -> *mut zend_attribute {
    let size = std::mem::size_of::<zend_attribute>() + (argc.max(1) as usize - 1) * std::mem::size_of::<zend_attribute_arg>();
    let attribute = crate::alloc::alloc_zeroed(size).cast::<zend_attribute>();
    (*attribute).name = string::zend_string_copy(name);
    (*attribute).lcname = string::zend_string_tolower(name);
    (*attribute).flags = flags;
    (*attribute).lineno = lineno;
    (*attribute).offset = offset;
    (*attribute).argc = argc;
    for i in 0..argc as usize {
        let arg = (*attribute).args.as_mut_ptr().add(i);
        (*arg).value.u1.type_info = IS_UNDEF as u32;
    }
    if (*attributes).is_null() {
        let table = alloc_struct::<HashTable>();
        hash::init(table, 8, None, true);
        *attributes = table;
    }
    hash::zend_hash_next_index_insert_ptr(*attributes, attribute.cast());
    attribute
}

/// Adds a persistent attribute with `argc` arguments to a class.
#[no_mangle]
pub unsafe extern "C" fn zend_add_class_attribute(ce: *mut zend_class_entry, name: *mut zend_string, argc: u32) -> *mut zend_attribute {
    zend_add_attribute(&mut (*ce).attributes, name, argc, ZEND_ATTRIBUTE_PERSISTENT, 0, 0)
}

/// Adds a persistent attribute with `argc` arguments to a function or method.
#[no_mangle]
pub unsafe extern "C" fn zend_add_function_attribute(func: *mut zend_function, name: *mut zend_string, argc: u32) -> *mut zend_attribute {
    zend_add_attribute(&mut (*func).common.attributes, name, argc, ZEND_ATTRIBUTE_PERSISTENT, 0, 0)
}

/// Adds a persistent attribute to a function's parameter at zero-based `offset` (stored 1-based).
#[no_mangle]
pub unsafe extern "C" fn zend_add_parameter_attribute(func: *mut zend_function, offset: u32, name: *mut zend_string, argc: u32) -> *mut zend_attribute {
    zend_add_attribute(&mut (*func).common.attributes, name, argc, ZEND_ATTRIBUTE_PERSISTENT, offset + 1, 0)
}

/// Adds a persistent attribute to a declared property.
#[no_mangle]
pub unsafe extern "C" fn zend_add_property_attribute(
    _ce: *mut zend_class_entry,
    info: *mut zend_property_info,
    name: *mut zend_string,
    argc: u32,
) -> *mut zend_attribute {
    zend_add_attribute(&mut (*info).attributes, name, argc, ZEND_ATTRIBUTE_PERSISTENT, 0, 0)
}

/// Adds a persistent attribute to a class constant.
#[no_mangle]
pub unsafe extern "C" fn zend_add_class_constant_attribute(
    _ce: *mut zend_class_entry,
    c: *mut zend_class_constant,
    name: *mut zend_string,
    argc: u32,
) -> *mut zend_attribute {
    zend_add_attribute(&mut (*c).attributes, name, argc, ZEND_ATTRIBUTE_PERSISTENT, 0, 0)
}

/// Finds an attribute by name (ASCII case-insensitive) in an attribute table; null if absent or the
/// table is null.
#[no_mangle]
pub unsafe extern "C" fn zend_get_attribute_str(attributes: *mut HashTable, str: *const c_char, len: usize) -> *mut zend_attribute {
    if attributes.is_null() {
        return ptr::null_mut();
    }
    let wanted = std::slice::from_raw_parts(str.cast::<u8>(), len).to_ascii_lowercase();
    for idx in 0..(*attributes).nNumUsed {
        let p = hash::bucket(attributes, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF {
            continue;
        }
        let attribute = (*p).val.value.ptr.cast::<zend_attribute>();
        if bytes((*attribute).lcname) == wanted.as_slice() {
            return attribute;
        }
    }
    ptr::null_mut()
}

/// No-op: `#[SensitiveParameter]` marking has no effect on a compiled program.
#[no_mangle]
pub unsafe extern "C" fn zend_mark_function_parameter_as_sensitive(_table: *const HashTable, _func_name: *const c_char, _parameter: u32) {}

// ------------------------------------------------------------ enums

/// Registers an internal enum class (final, implementing `UnitEnum`, and `BackedEnum` when `type`
/// is not `IS_UNDEF`) with the given methods and backing type.
#[no_mangle]
pub unsafe extern "C" fn zend_register_internal_enum(
    name: *const c_char,
    r#type: u8,
    functions: *const zend_function_entry,
) -> *mut zend_class_entry {
    let mut entry: zend_class_entry = std::mem::zeroed();
    entry.name = intern(CStr::from_ptr(name).to_bytes());
    entry.default_object_handlers = &crate::object::std_object_handlers;
    entry.info.internal.builtin_functions = functions;
    let ce = register(&mut entry, ptr::null_mut(), ZEND_ACC_ENUM | ZEND_ACC_FINAL);
    (*ce).enum_backing_type = r#type as u32;
    add_interface(ce, zend_ce_unit_enum);
    if r#type != IS_UNDEF {
        add_interface(ce, zend_ce_backed_enum);
    }
    ce
}

/// Declares an enum case: creates the case object with `name` (and `value` for backed enums) and
/// registers it as a public class constant flagged as a case.
#[no_mangle]
pub unsafe extern "C" fn zend_enum_add_case(ce: *mut zend_class_entry, case_name: *mut zend_string, value: *mut zval) {
    let mut object = zval::UNDEF;
    crate::object::init_object(&mut object, ce);
    let mut name = zval::UNDEF;
    crate::zval::set_str_copy(&mut name, case_name);
    crate::object::set_property(object.value.obj, b"name", &mut name);
    if !value.is_null() && (*value).u1.v.r#type != IS_UNDEF {
        let mut backing = zval::UNDEF;
        copy(&mut backing, value);
        crate::object::set_property(object.value.obj, b"value", &mut backing);
    }
    let constant = zend_declare_class_constant_ex(ce, case_name, &mut object, ZEND_ACC_PUBLIC as c_int, ptr::null_mut());
    (*constant).value.u2.constant_flags |= ZEND_CLASS_CONST_IS_CASE;
}

/// C-string-name form of `zend_enum_add_case`.
#[no_mangle]
pub unsafe extern "C" fn zend_enum_add_case_cstr(ce: *mut zend_class_entry, name: *const c_char, value: *mut zval) {
    zend_enum_add_case(ce, intern(CStr::from_ptr(name).to_bytes()), value);
}

/// Returns the case object for a case name, or null if the constant is missing or not an object.
#[no_mangle]
pub unsafe extern "C" fn zend_enum_get_case(ce: *mut zend_class_entry, name: *mut zend_string) -> *mut zend_object {
    let constant = hash::zend_hash_find_ptr(&(*ce).constants_table, name).cast::<zend_class_constant>();
    if constant.is_null() || (*constant).value.u1.v.r#type != IS_OBJECT {
        return ptr::null_mut();
    }
    (*constant).value.value.obj
}

/// C-string-name form of `zend_enum_get_case`.
#[no_mangle]
pub unsafe extern "C" fn zend_enum_get_case_cstr(ce: *mut zend_class_entry, name: *const c_char) -> *mut zend_object {
    zend_enum_get_case(ce, intern(CStr::from_ptr(name).to_bytes()))
}

/// Returns the `name` property slot of an enum case object.
#[no_mangle]
pub unsafe extern "C" fn zend_enum_fetch_case_name(zobj: *mut zend_object) -> *mut zval {
    crate::object::find_property(zobj, b"name")
}

/// Returns the `value` property slot of a backed enum case object.
#[no_mangle]
pub unsafe extern "C" fn zend_enum_fetch_case_value(zobj: *mut zend_object) -> *mut zval {
    crate::object::find_property(zobj, b"value")
}

/// Creates an enum case object of class `ce` in `result`, setting its `name` and (when given)
/// `value` properties; returns the object.
#[no_mangle]
pub unsafe extern "C" fn zend_enum_new(
    result: *mut zval,
    ce: *mut zend_class_entry,
    case_name: *mut zend_string,
    backing_value_zv: *mut zval,
) -> *mut zend_object {
    crate::object::init_object(result, ce);
    let mut name = zval::UNDEF;
    crate::zval::set_str_copy(&mut name, case_name);
    crate::object::set_property((*result).value.obj, b"name", &mut name);
    if !backing_value_zv.is_null() {
        let mut backing = zval::UNDEF;
        copy(&mut backing, backing_value_zv);
        crate::object::set_property((*result).value.obj, b"value", &mut backing);
    }
    (*result).value.obj
}
