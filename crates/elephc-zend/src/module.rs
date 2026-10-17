//! Purpose:
//! Modules and the functions they register: `zend_register_internal_module`,
//! `zend_startup_module_ex` (globals constructor, then MINIT),
//! `zend_register_functions`, and the modules whose surface the engine itself
//! provides (Core, standard, SPL, hash, json...).
//!
//! Called from:
//! - `crate::host` when an extension is first used, and `crate::class` for
//!   methods.
//!
//! Key details:
//! - A function becomes a `zend_internal_function` in `CG(function_table)`
//!   under its lowercase name; its `arg_info` points at the extension's own
//!   arginfo array (element 1; element 0, the return info, sits before it).
//! - The module being registered or started is "current": classes and
//!   constants it registers record it, as `EG(current_module)` does in PHP.

use std::ffi::{c_char, c_int, CStr};
use std::ptr;

use crate::abi::*;
use crate::alloc::alloc_struct;
use crate::global::Global;
use crate::hash;
use crate::startup::{cg, modules};
use crate::string::{self, intern};

static CURRENT: Global<*mut zend_module_entry> = Global::new(ptr::null_mut());
static NEXT_MODULE_NUMBER: Global<c_int> = Global::new(1);

/// The module whose registration or startup is running.
pub fn current() -> *mut zend_module_entry {
    unsafe { *CURRENT.get() }
}

/// The module number of the module currently registering or starting, or 0 when none is current.
pub fn current_number() -> c_int {
    let module = current();
    if module.is_null() {
        0
    } else {
        unsafe { (*module).module_number }
    }
}

/// Runs `f` with `module` current.
pub fn with_module<T>(module: *mut zend_module_entry, f: impl FnOnce() -> T) -> T {
    unsafe {
        let outer = *CURRENT.get();
        *CURRENT.get() = module;
        let result = f();
        *CURRENT.get() = outer;
        result
    }
}

/// Builds a minimal module entry with the given name for a module whose API the engine itself
/// provides, so hard dependencies on it are satisfied; evaluated at compile time.
const fn provided(name: &'static CStr) -> zend_module_entry {
    zend_module_entry {
        size: std::mem::size_of::<zend_module_entry>() as u16,
        zend_api: ZEND_MODULE_API_NO as u32,
        zend_debug: 0,
        zts: 0,
        ini_entry: ptr::null(),
        deps: ptr::null(),
        name: name.as_ptr(),
        functions: ptr::null(),
        module_startup_func: None,
        module_shutdown_func: None,
        request_startup_func: None,
        request_shutdown_func: None,
        info_func: None,
        version: c"8.5.6".as_ptr(),
        globals_size: 0,
        globals_ptr: ptr::null_mut(),
        globals_ctor: None,
        globals_dtor: None,
        post_deactivate_func: None,
        module_started: 0,
        r#type: 0,
        handle: ptr::null_mut(),
        module_number: 0,
        build_id: c"API20250925,NTS".as_ptr(),
    }
}

/// Modules whose C API the engine provides (or refuses by name): an extension
/// declaring a hard dependency on one of them can load.
const PROVIDED_COUNT: usize = 10;
static mut PROVIDED: [zend_module_entry; PROVIDED_COUNT] = [
    provided(c"Core"),
    provided(c"date"),
    provided(c"hash"),
    provided(c"iconv"),
    provided(c"json"),
    provided(c"pcre"),
    provided(c"random"),
    provided(c"Reflection"),
    provided(c"SPL"),
    provided(c"standard"),
];

/// The Core module, owner of the built-in classes.
pub fn core_module() -> *mut zend_module_entry {
    unsafe { ptr::addr_of_mut!(PROVIDED[0]) }
}

/// Whether `module` is one the engine itself provides, as opposed to a hosted
/// extension.
pub fn is_provided(module: *const zend_module_entry) -> bool {
    unsafe {
        let first = ptr::addr_of!(PROVIDED[0]);
        let end = first.add(PROVIDED_COUNT);
        module >= first && module < end
    }
}

/// Runs `f` with the Core module as the current module.
pub fn with_core_module(f: impl FnOnce()) {
    with_module(core_module(), f);
}

/// Registers each engine-provided module (Core, date, hash, and so on) in the module registry and
/// marks it started.
pub fn register_provided_modules() {
    unsafe {
        for index in 0..PROVIDED_COUNT {
            let module = ptr::addr_of_mut!(PROVIDED[index]);
            if !zend_register_internal_module(module).is_null() {
                (*module).module_started = 1;
            }
        }
    }
}

/// Lowercases a C module name, the key form used in the module registry.
unsafe fn lower_name(name: *const c_char) -> Vec<u8> {
    CStr::from_ptr(name).to_bytes().to_ascii_lowercase()
}

/// `zend_register_module_ex`: registers a module entry in the registry under its lowercased name
/// and gives it a module number; an already loaded name raises an `E_CORE_WARNING` and returns
/// NULL. Returns the registered entry.
#[no_mangle]
pub unsafe extern "C" fn zend_register_module_ex(module: *mut zend_module_entry, module_type: c_int) -> *mut zend_module_entry {
    let lower = lower_name((*module).name);
    if !hash::zend_hash_str_find(modules(), lower.as_ptr().cast(), lower.len()).is_null() {
        crate::error::report(
            E_CORE_WARNING,
            format!("Module \"{}\" is already loaded", CStr::from_ptr((*module).name).to_string_lossy()).as_bytes(),
        );
        return ptr::null_mut();
    }
    let mut dep = (*module).deps;
    while !dep.is_null() && !(*dep).name.is_null() {
        let dep_lower = lower_name((*dep).name);
        let loaded = !hash::zend_hash_str_find(modules(), dep_lower.as_ptr().cast(), dep_lower.len()).is_null();
        match (*dep).r#type as c_int {
            MODULE_DEP_CONFLICTS if loaded => {
                crate::error::report(
                    E_CORE_WARNING,
                    format!(
                        "Cannot load module \"{}\" because conflicting module \"{}\" is already loaded",
                        CStr::from_ptr((*module).name).to_string_lossy(),
                        CStr::from_ptr((*dep).name).to_string_lossy()
                    )
                    .as_bytes(),
                );
                return ptr::null_mut();
            }
            MODULE_DEP_REQUIRED if !loaded => {
                crate::error::report(
                    E_CORE_WARNING,
                    format!(
                        "Cannot load module \"{}\" because required module \"{}\" is not loaded",
                        CStr::from_ptr((*module).name).to_string_lossy(),
                        CStr::from_ptr((*dep).name).to_string_lossy()
                    )
                    .as_bytes(),
                );
                return ptr::null_mut();
            }
            _ => {}
        }
        dep = dep.add(1);
    }
    let number = NEXT_MODULE_NUMBER.get();
    (*module).module_number = *number;
    *number += 1;
    (*module).r#type = module_type as u8;
    (*module).module_started = 0;
    hash::zend_hash_str_add_ptr(modules(), lower.as_ptr().cast(), lower.len(), module.cast());
    if !(*module).functions.is_null() {
        let status = with_module(module, || {
            zend_register_functions(ptr::null_mut(), (*module).functions, ptr::null_mut(), module_type)
        });
        if status != SUCCESS {
            hash::zend_hash_str_del(modules(), lower.as_ptr().cast(), lower.len());
            return ptr::null_mut();
        }
    }
    module
}

/// `zend_register_internal_module`: registers a persistent module entry; see
/// `zend_register_module_ex`.
#[no_mangle]
pub unsafe extern "C" fn zend_register_internal_module(module: *mut zend_module_entry) -> *mut zend_module_entry {
    zend_register_module_ex(module, MODULE_PERSISTENT)
}

/// Runs the module's globals constructor and MINIT, once.
#[no_mangle]
pub unsafe extern "C" fn zend_startup_module_ex(module: *mut zend_module_entry) -> zend_result {
    if (*module).module_started != 0 {
        return SUCCESS;
    }
    (*module).module_started = 1;
    let mut dep = (*module).deps;
    while !dep.is_null() && !(*dep).name.is_null() {
        if (*dep).r#type as c_int == MODULE_DEP_REQUIRED {
            let dep_lower = lower_name((*dep).name);
            let found = hash::zend_hash_str_find_ptr(modules(), dep_lower.as_ptr().cast(), dep_lower.len())
                .cast::<zend_module_entry>();
            if found.is_null() || (*found).module_started == 0 {
                crate::error::report(
                    E_CORE_WARNING,
                    format!(
                        "Cannot load module \"{}\" because required module \"{}\" is not loaded",
                        CStr::from_ptr((*module).name).to_string_lossy(),
                        CStr::from_ptr((*dep).name).to_string_lossy()
                    )
                    .as_bytes(),
                );
                (*module).module_started = 0;
                return FAILURE;
            }
        }
        dep = dep.add(1);
    }
    if (*module).globals_size > 0 && !(*module).globals_ptr.is_null() {
        if let Some(ctor) = (*module).globals_ctor {
            ctor((*module).globals_ptr);
        }
    }
    if let Some(startup) = (*module).module_startup_func {
        let status = with_module(module, || startup((*module).r#type as c_int, (*module).module_number));
        if status != SUCCESS {
            crate::error::report(
                E_CORE_ERROR,
                format!("Unable to start {} module", CStr::from_ptr((*module).name).to_string_lossy()).as_bytes(),
            );
            (*module).module_started = 0;
            return FAILURE;
        }
    }
    SUCCESS
}

/// `zend_startup_module`: registers a module and runs its startup; FAILURE if registration fails.
#[no_mangle]
pub unsafe extern "C" fn zend_startup_module(module: *mut zend_module_entry) -> zend_result {
    let registered = zend_register_internal_module(module);
    if registered.is_null() {
        return FAILURE;
    }
    zend_startup_module_ex(registered)
}

/// `zend_get_module_started`: whether the named module (case-insensitive) is registered and has
/// started.
#[no_mangle]
pub unsafe extern "C" fn zend_get_module_started(module_name: *const c_char) -> bool {
    let lower = lower_name(module_name);
    let module = hash::zend_hash_str_find_ptr(modules(), lower.as_ptr().cast(), lower.len()).cast::<zend_module_entry>();
    !module.is_null() && (*module).module_started != 0
}

/// `zend_hash_find_module`: finds a registered module entry by case-insensitive name, or NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_hash_find_module(name: *const c_char) -> *mut zend_module_entry {
    let lower = lower_name(name);
    hash::zend_hash_str_find_ptr(modules(), lower.as_ptr().cast(), lower.len()).cast()
}

// ------------------------------------------------------------ functions

const MAGIC_METHODS: &[&[u8]] = &[
    b"__construct", b"__destruct", b"__clone", b"__get", b"__set", b"__unset", b"__isset", b"__call",
    b"__callstatic", b"__tostring", b"__debuginfo", b"__serialize", b"__unserialize",
];

/// Stores a magic method (`__construct`, `__get`, `__toString` and so on, given lowercased) in the
/// matching special slot of the class entry.
unsafe fn set_magic(scope: *mut zend_class_entry, lower: &[u8], function: *mut zend_function) {
    let ce = &mut *scope;
    match lower {
        b"__construct" => ce.constructor = function,
        b"__destruct" => ce.destructor = function,
        b"__clone" => ce.clone = function,
        b"__get" => ce.__get = function,
        b"__set" => ce.__set = function,
        b"__unset" => ce.__unset = function,
        b"__isset" => ce.__isset = function,
        b"__call" => ce.__call = function,
        b"__callstatic" => ce.__callstatic = function,
        b"__tostring" => ce.__tostring = function,
        b"__debuginfo" => ce.__debugInfo = function,
        b"__serialize" => ce.__serialize = function,
        b"__unserialize" => ce.__unserialize = function,
        _ => {}
    }
}

/// The by-reference flags of the first 12 parameters, two bits each.
unsafe fn set_arg_flags(function: *mut zend_internal_function) {
    (*function).arg_flags = [0; 3];
    if (*function).arg_info.is_null() {
        return;
    }
    let count = (*function).num_args.min(12);
    for i in 0..count {
        let info = (*function).arg_info.add(i as usize);
        let mode = ((*info).r#type.type_mask >> _ZEND_SEND_MODE_SHIFT) & 3;
        let byte = (i / 4) as usize;
        if byte < 3 {
            (*function).arg_flags[byte] |= (mode as u8) << ((i % 4) * 2);
        }
    }
}

/// Builds one internal function from its entry.
unsafe fn internal_function(entry: *const zend_function_entry, scope: *mut zend_class_entry) -> *mut zend_internal_function {
    let function = alloc_struct::<zend_internal_function>();
    let f = &mut *function;
    f.r#type = ZEND_INTERNAL_FUNCTION;
    f.handler = (*entry).handler;
    f.function_name = intern(CStr::from_ptr((*entry).fname).to_bytes());
    f.scope = scope;
    f.module = current();
    f.fn_flags = (*entry).flags;
    if !scope.is_null() && f.fn_flags & ZEND_ACC_PPP_MASK == 0 {
        f.fn_flags |= ZEND_ACC_PUBLIC;
    }
    if !(*entry).doc_comment.is_null() {
        f.doc_comment = intern(CStr::from_ptr((*entry).doc_comment).to_bytes());
    }
    let info = (*entry).arg_info;
    if info.is_null() {
        f.num_args = 0;
        f.required_num_args = 0;
        f.arg_info = ptr::null_mut();
    } else {
        f.arg_info = info.add(1).cast_mut();
        f.num_args = (*entry).num_args;
        let required = (*info).name as usize;
        f.required_num_args = if required == usize::MAX { f.num_args } else { required as u32 };
        let return_mask = (*info).r#type.type_mask;
        if (return_mask >> _ZEND_SEND_MODE_SHIFT) & 3 != 0 {
            f.fn_flags |= ZEND_ACC_RETURN_REFERENCE;
        }
        if return_mask & _ZEND_TYPE_MASK != 0 {
            f.fn_flags |= ZEND_ACC_HAS_RETURN_TYPE;
        }
        if f.num_args > 0 {
            let last = f.arg_info.add(f.num_args as usize - 1);
            if (*last).r#type.type_mask & _ZEND_IS_VARIADIC_BIT != 0 {
                f.fn_flags |= ZEND_ACC_VARIADIC;
                f.num_args -= 1;
            }
        }
        if f.required_num_args > f.num_args {
            f.required_num_args = f.num_args;
        }
    }
    set_arg_flags(function);
    function
}

/// Registers `functions` in `function_table` (the global one when NULL), as
/// methods of `scope` when it is set. Stops at the first duplicate.
#[no_mangle]
pub unsafe extern "C" fn zend_register_functions(
    scope: *mut zend_class_entry,
    functions: *const zend_function_entry,
    function_table: *mut HashTable,
    _type: c_int,
) -> zend_result {
    let table = if !function_table.is_null() {
        function_table
    } else if !scope.is_null() {
        &mut (*scope).function_table
    } else {
        cg().function_table
    };
    let mut entry = functions;
    while !entry.is_null() && !(*entry).fname.is_null() {
        let function = internal_function(entry, scope);
        let name = string::bytes((*function).function_name).to_ascii_lowercase();
        let added = hash::zend_hash_str_add_ptr(table, name.as_ptr().cast(), name.len(), function.cast());
        if added.is_null() {
            let owner = if scope.is_null() { String::new() } else { format!("{}::", string::text((*scope).name)) };
            crate::error::report(
                E_CORE_WARNING,
                format!(
                    "Function registration failed - duplicate name - {owner}{}",
                    string::text((*function).function_name)
                )
                .as_bytes(),
            );
            return FAILURE;
        }
        if !scope.is_null() && MAGIC_METHODS.contains(&name.as_slice()) {
            set_magic(scope, &name, function.cast());
        }
        entry = entry.add(1);
    }
    SUCCESS
}

/// `zend_unregister_functions`: removes the functions of a `zend_function_entry` array from a
/// function table (the global table when NULL), stopping at the terminator or after `count` entries
/// when `count` is not negative.
#[no_mangle]
pub unsafe extern "C" fn zend_unregister_functions(functions: *const zend_function_entry, count: c_int, function_table: *mut HashTable) {
    let table = if function_table.is_null() { cg().function_table } else { function_table };
    let mut entry = functions;
    let mut seen = 0;
    while !entry.is_null() && !(*entry).fname.is_null() && (count < 0 || seen < count) {
        let name = lower_name((*entry).fname);
        hash::zend_hash_str_del(table, name.as_ptr().cast(), name.len());
        entry = entry.add(1);
        seen += 1;
    }
}

/// `zend_set_function_arg_flags`: computes the by-reference argument flags of an internal function
/// from its argument info.
#[no_mangle]
pub unsafe extern "C" fn zend_set_function_arg_flags(function: *mut zend_function) {
    set_arg_flags(ptr::addr_of_mut!((*function).internal_function));
}

/// `zend_fetch_function`: finds a function by name (case-insensitively) in the global function
/// table, or NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_function(name: *mut zend_string) -> *mut zend_function {
    hash::zend_hash_find_ptr_lc(cg().function_table, name).cast()
}

/// `zend_fetch_function_str`: finds a function by pointer and length name (case-insensitively) in
/// the global function table, or NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_function_str(name: *const c_char, len: usize) -> *mut zend_function {
    hash::zend_hash_str_find_ptr_lc(cg().function_table, name, len).cast()
}

/// `function_add_ref`: no-op, because internal functions are not reference counted.
#[no_mangle]
pub unsafe extern "C" fn function_add_ref(_function: *mut zend_function) {}

/// `zend_get_extension`: always NULL, because the engine loads no Zend extensions.
#[no_mangle]
pub unsafe extern "C" fn zend_get_extension(_extension_name: *const c_char) -> *mut zend_extension {
    ptr::null_mut()
}
