//! Purpose:
//! The engine structures of the extension ABI: types and arginfo, functions
//! and call frames, modules, classes, objects and their handlers, constants,
//! INI entries, the engine globals and the string builders.
//!
//! Called from:
//! - Every engine module, and `build.rs`, which emits each struct as C.
//!
//! Key details:
//! - Structures an extension *defines* (zend_function_entry, arginfo,
//!   zend_module_entry, zend_ini_entry_def, zend_object_handlers) keep PHP's
//!   field order, since their initializers are positional.
//! - Structures only this engine fills (zend_class_entry, the executor
//!   globals) carry the fields extensions read; their order is Elephc's.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::{c_char, c_int, c_uchar, c_uint, c_ushort, c_void};

use super::scalars::*;
use super::values::*;

// ------------------------------------------------------------ declarations only

/// c: opaque
#[repr(C)]
pub struct zend_op {
    _private: [u8; 0],
}
/// c: opaque
#[repr(C)]
pub struct zend_op_array {
    _private: [u8; 0],
}
/// c: opaque
#[repr(C)]
pub struct zend_ast {
    _private: [u8; 0],
}
/// c: opaque
#[repr(C)]
pub struct zend_serialize_data {
    _private: [u8; 0],
}
/// c: opaque
#[repr(C)]
pub struct zend_unserialize_data {
    _private: [u8; 0],
}
/// c: opaque
#[repr(C)]
pub struct zend_frameless_function_info {
    _private: [u8; 0],
}

// ------------------------------------------------------------ types

/// A declared type: a mask of `MAY_BE_*` bits and kind bits, plus a class
/// name or type list when the kind bits say so.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct zend_type {
    pub ptr: *mut c_void,
    pub type_mask: u32,
}

#[repr(C)]
pub struct zend_type_list {
    pub num_types: u32,
    pub types: [zend_type; 1],
}

// ------------------------------------------------------------ functions

pub type zif_handler = Option<unsafe extern "C" fn(execute_data: *mut zend_execute_data, return_value: *mut zval)>;

/// Arginfo as an extension writes it: element 0 describes the return value
/// (its `name` holds the required argument count), the rest the parameters.
#[repr(C)]
pub struct zend_internal_arg_info {
    pub name: *const c_char,
    pub r#type: zend_type,
    pub default_value: *const c_char,
}

#[repr(C)]
pub struct zend_arg_info {
    pub name: *mut zend_string,
    pub r#type: zend_type,
    pub default_value: *mut zend_string,
}

#[repr(C)]
pub struct zend_internal_function_info {
    pub required_num_args: usize,
    pub r#type: zend_type,
    pub default_value: *const c_char,
}

#[repr(C)]
pub struct zend_function_entry {
    pub fname: *const c_char,
    pub handler: zif_handler,
    pub arg_info: *const zend_internal_arg_info,
    pub num_args: u32,
    pub flags: u32,
    pub frameless_function_infos: *const zend_frameless_function_info,
    pub doc_comment: *const c_char,
}

/// The fields every function kind shares, in the same place in each.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct zend_function_common {
    pub r#type: u8,
    pub arg_flags: [u8; 3],
    pub fn_flags: u32,
    pub function_name: *mut zend_string,
    pub scope: *mut zend_class_entry,
    pub prototype: *mut zend_function,
    pub num_args: u32,
    pub required_num_args: u32,
    pub arg_info: *mut zend_arg_info,
    pub attributes: *mut HashTable,
    pub T: u32,
    pub run_time_cache__ptr: *mut c_void,
    pub doc_comment: *mut zend_string,
    pub fbc_unused: *mut zend_type,
    pub prop_info: *const zend_property_info,
}

/// A function an extension registered. `arg_info` points at its arginfo
/// array's element 1 (the return info sits just before it).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct zend_internal_function {
    pub r#type: u8,
    pub arg_flags: [u8; 3],
    pub fn_flags: u32,
    pub function_name: *mut zend_string,
    pub scope: *mut zend_class_entry,
    pub prototype: *mut zend_function,
    pub num_args: u32,
    pub required_num_args: u32,
    pub arg_info: *mut zend_internal_arg_info,
    pub attributes: *mut HashTable,
    pub T: u32,
    pub run_time_cache__ptr: *mut c_void,
    pub doc_comment: *mut zend_string,
    pub fbc_unused: *mut zend_type,
    pub prop_info: *const zend_property_info,
    pub handler: zif_handler,
    pub module: *mut zend_module_entry,
    pub frameless_function_infos: *const zend_frameless_function_info,
    pub reserved: [*mut c_void; 6],
}

#[repr(C)]
pub union zend_function {
    pub r#type: u8,
    pub quick_arg_flags: u32,
    pub common: zend_function_common,
    pub internal_function: zend_internal_function,
    pub op_array: zend_function_common,
}

/// A call frame. The arguments follow it in memory, one zval each, from
/// `ZEND_CALL_FRAME_SLOT` zvals past its start.
#[repr(C)]
pub struct zend_execute_data {
    pub opline: *const zend_op,
    pub call: *mut zend_execute_data,
    pub return_value: *mut zval,
    pub func: *mut zend_function,
    pub This: zval,
    pub prev_execute_data: *mut zend_execute_data,
    pub symbol_table: *mut zend_array,
    pub run_time_cache: *mut *mut c_void,
    pub extra_named_params: *mut zend_array,
}

#[repr(C)]
pub struct zend_fcall_info {
    pub size: usize,
    pub function_name: zval,
    pub retval: *mut zval,
    pub params: *mut zval,
    pub object: *mut zend_object,
    pub param_count: u32,
    pub named_params: *mut HashTable,
}

#[repr(C)]
pub struct zend_fcall_info_cache {
    pub function_handler: *mut zend_function,
    pub calling_scope: *mut zend_class_entry,
    pub called_scope: *mut zend_class_entry,
    pub object: *mut zend_object,
    pub closure: *mut zend_object,
}

// ------------------------------------------------------------ modules

#[repr(C)]
pub struct zend_module_dep {
    pub name: *const c_char,
    pub rel: *const c_char,
    pub version: *const c_char,
    pub r#type: c_uchar,
}

pub type module_func_t = Option<unsafe extern "C" fn(r#type: c_int, module_number: c_int) -> zend_result>;

#[repr(C)]
pub struct zend_module_entry {
    pub size: c_ushort,
    pub zend_api: c_uint,
    pub zend_debug: c_uchar,
    pub zts: c_uchar,
    pub ini_entry: *const zend_ini_entry,
    pub deps: *const zend_module_dep,
    pub name: *const c_char,
    pub functions: *const zend_function_entry,
    pub module_startup_func: module_func_t,
    pub module_shutdown_func: module_func_t,
    pub request_startup_func: module_func_t,
    pub request_shutdown_func: module_func_t,
    pub info_func: Option<unsafe extern "C" fn(zend_module: *mut zend_module_entry)>,
    pub version: *const c_char,
    pub globals_size: usize,
    pub globals_ptr: *mut c_void,
    pub globals_ctor: Option<unsafe extern "C" fn(global: *mut c_void)>,
    pub globals_dtor: Option<unsafe extern "C" fn(global: *mut c_void)>,
    pub post_deactivate_func: Option<unsafe extern "C" fn() -> zend_result>,
    pub module_started: c_int,
    pub r#type: c_uchar,
    pub handle: *mut c_void,
    pub module_number: c_int,
    pub build_id: *const c_char,
}

// ------------------------------------------------------------ classes

#[repr(C)]
pub struct zend_class_name {
    pub name: *mut zend_string,
    pub lc_name: *mut zend_string,
}

#[repr(C)]
pub struct zend_class_iterator_funcs {
    pub zf_new_iterator: *mut zend_function,
    pub zf_valid: *mut zend_function,
    pub zf_current: *mut zend_function,
    pub zf_key: *mut zend_function,
    pub zf_next: *mut zend_function,
    pub zf_rewind: *mut zend_function,
}

#[repr(C)]
pub struct zend_class_arrayaccess_funcs {
    pub zf_offsetget: *mut zend_function,
    pub zf_offsetexists: *mut zend_function,
    pub zf_offsetset: *mut zend_function,
    pub zf_offsetunset: *mut zend_function,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_class_entry_parent {
    pub parent: *mut zend_class_entry,
    pub parent_name: *mut zend_string,
}

pub type create_object_t = Option<unsafe extern "C" fn(class_type: *mut zend_class_entry) -> *mut zend_object>;

#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_class_entry_create {
    pub create_object: create_object_t,
    pub interface_gets_implemented:
        Option<unsafe extern "C" fn(iface: *mut zend_class_entry, class_type: *mut zend_class_entry) -> c_int>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_class_entry_interfaces {
    pub interfaces: *mut *mut zend_class_entry,
    pub interface_names: *mut zend_class_name,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct zend_class_entry_info_user {
    pub filename: *mut zend_string,
    pub line_start: u32,
    pub line_end: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct zend_class_entry_info_internal {
    pub builtin_functions: *const zend_function_entry,
    pub module: *mut zend_module_entry,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_class_entry_info {
    pub user: zend_class_entry_info_user,
    pub internal: zend_class_entry_info_internal,
}

/// A class or interface. Extensions fill one on the stack with
/// `INIT_CLASS_ENTRY` and register it; the engine copies it.
#[repr(C)]
pub struct zend_class_entry {
    pub r#type: c_char,
    pub name: *mut zend_string,
    /// c: anonymous
    pub parent_u: zend_class_entry_parent,
    pub refcount: c_int,
    pub ce_flags: u32,
    pub default_properties_count: c_int,
    pub default_static_members_count: c_int,
    pub default_properties_table: *mut zval,
    pub default_static_members_table: *mut zval,
    pub static_members_table__ptr: *mut zval,
    pub function_table: HashTable,
    pub properties_info: HashTable,
    pub constants_table: HashTable,
    pub mutable_data__ptr: *mut c_void,
    pub inheritance_cache: *mut c_void,
    pub properties_info_table: *mut *mut zend_property_info,
    pub constructor: *mut zend_function,
    pub destructor: *mut zend_function,
    pub clone: *mut zend_function,
    pub __get: *mut zend_function,
    pub __set: *mut zend_function,
    pub __unset: *mut zend_function,
    pub __isset: *mut zend_function,
    pub __call: *mut zend_function,
    pub __callstatic: *mut zend_function,
    pub __tostring: *mut zend_function,
    pub __debugInfo: *mut zend_function,
    pub __serialize: *mut zend_function,
    pub __unserialize: *mut zend_function,
    pub default_object_handlers: *const zend_object_handlers,
    pub iterator_funcs_ptr: *mut zend_class_iterator_funcs,
    pub arrayaccess_funcs_ptr: *mut zend_class_arrayaccess_funcs,
    /// c: anonymous
    pub create_u: zend_class_entry_create,
    pub get_iterator: Option<
        unsafe extern "C" fn(ce: *mut zend_class_entry, object: *mut zval, by_ref: c_int) -> *mut zend_object_iterator,
    >,
    pub get_static_method:
        Option<unsafe extern "C" fn(ce: *mut zend_class_entry, method: *mut zend_string) -> *mut zend_function>,
    pub serialize: Option<
        unsafe extern "C" fn(
            object: *mut zval,
            buffer: *mut *mut c_uchar,
            buf_len: *mut usize,
            data: *mut zend_serialize_data,
        ) -> c_int,
    >,
    pub unserialize: Option<
        unsafe extern "C" fn(
            object: *mut zval,
            ce: *mut zend_class_entry,
            buf: *const c_uchar,
            buf_len: usize,
            data: *mut zend_unserialize_data,
        ) -> c_int,
    >,
    pub num_interfaces: u32,
    pub num_traits: u32,
    pub num_hooked_props: u32,
    pub num_hooked_prop_variance_checks: u32,
    /// c: anonymous
    pub interfaces_u: zend_class_entry_interfaces,
    pub trait_names: *mut zend_class_name,
    pub trait_aliases: *mut c_void,
    pub trait_precedences: *mut c_void,
    pub attributes: *mut HashTable,
    pub enum_backing_type: u32,
    pub backed_enum_table: *mut HashTable,
    pub doc_comment: *mut zend_string,
    pub info: zend_class_entry_info,
}

/// A property a class declares.
#[repr(C)]
pub struct zend_property_info {
    pub offset: u32,
    pub flags: u32,
    pub name: *mut zend_string,
    pub doc_comment: *mut zend_string,
    pub attributes: *mut HashTable,
    pub ce: *mut zend_class_entry,
    pub r#type: zend_type,
    pub prototype: *const zend_property_info,
    pub hooks: *mut *mut zend_function,
}

#[repr(C)]
pub struct zend_class_constant {
    pub value: zval,
    pub doc_comment: *mut zend_string,
    pub attributes: *mut HashTable,
    pub ce: *mut zend_class_entry,
    pub r#type: zend_type,
}

#[repr(C)]
pub struct zend_attribute_arg {
    pub name: *mut zend_string,
    pub value: zval,
}

#[repr(C)]
pub struct zend_attribute {
    pub name: *mut zend_string,
    pub lcname: *mut zend_string,
    pub flags: u32,
    pub lineno: u32,
    pub offset: u32,
    pub argc: u32,
    pub args: [zend_attribute_arg; 1],
}

// ------------------------------------------------------------ objects

pub type zend_object_read_property_t = Option<
    unsafe extern "C" fn(
        object: *mut zend_object,
        member: *mut zend_string,
        r#type: c_int,
        cache_slot: *mut *mut c_void,
        rv: *mut zval,
    ) -> *mut zval,
>;
pub type zend_object_read_dimension_t =
    Option<unsafe extern "C" fn(object: *mut zend_object, offset: *mut zval, r#type: c_int, rv: *mut zval) -> *mut zval>;
pub type zend_object_write_property_t = Option<
    unsafe extern "C" fn(
        object: *mut zend_object,
        member: *mut zend_string,
        value: *mut zval,
        cache_slot: *mut *mut c_void,
    ) -> *mut zval,
>;
pub type zend_object_write_dimension_t =
    Option<unsafe extern "C" fn(object: *mut zend_object, offset: *mut zval, value: *mut zval)>;
pub type zend_object_get_property_ptr_ptr_t = Option<
    unsafe extern "C" fn(
        object: *mut zend_object,
        member: *mut zend_string,
        r#type: c_int,
        cache_slot: *mut *mut c_void,
    ) -> *mut zval,
>;
pub type zend_object_has_property_t = Option<
    unsafe extern "C" fn(
        object: *mut zend_object,
        member: *mut zend_string,
        has_set_exists: c_int,
        cache_slot: *mut *mut c_void,
    ) -> c_int,
>;
pub type zend_object_has_dimension_t =
    Option<unsafe extern "C" fn(object: *mut zend_object, member: *mut zval, check_empty: c_int) -> c_int>;
pub type zend_object_unset_property_t =
    Option<unsafe extern "C" fn(object: *mut zend_object, member: *mut zend_string, cache_slot: *mut *mut c_void)>;
pub type zend_object_unset_dimension_t = Option<unsafe extern "C" fn(object: *mut zend_object, offset: *mut zval)>;
pub type zend_object_get_properties_t = Option<unsafe extern "C" fn(object: *mut zend_object) -> *mut HashTable>;
pub type zend_object_get_debug_info_t =
    Option<unsafe extern "C" fn(object: *mut zend_object, is_temp: *mut c_int) -> *mut HashTable>;
pub type zend_object_get_properties_for_t =
    Option<unsafe extern "C" fn(object: *mut zend_object, purpose: zend_prop_purpose) -> *mut zend_array>;
pub type zend_object_get_method_t = Option<
    unsafe extern "C" fn(object: *mut *mut zend_object, method: *mut zend_string, key: *const zval) -> *mut zend_function,
>;
pub type zend_object_get_constructor_t = Option<unsafe extern "C" fn(object: *mut zend_object) -> *mut zend_function>;
pub type zend_object_free_obj_t = Option<unsafe extern "C" fn(object: *mut zend_object)>;
pub type zend_object_dtor_obj_t = Option<unsafe extern "C" fn(object: *mut zend_object)>;
pub type zend_object_clone_obj_t = Option<unsafe extern "C" fn(old_object: *mut zend_object) -> *mut zend_object>;
pub type zend_object_clone_obj_with_t = Option<
    unsafe extern "C" fn(
        old_object: *mut zend_object,
        scope: *const zend_class_entry,
        properties: *const HashTable,
    ) -> *mut zend_object,
>;
pub type zend_object_get_class_name_t = Option<unsafe extern "C" fn(object: *const zend_object) -> *mut zend_string>;
pub type zend_object_compare_t = Option<unsafe extern "C" fn(object1: *mut zval, object2: *mut zval) -> c_int>;
pub type zend_object_cast_t =
    Option<unsafe extern "C" fn(readobj: *mut zend_object, retval: *mut zval, r#type: c_int) -> zend_result>;
pub type zend_object_count_elements_t =
    Option<unsafe extern "C" fn(object: *mut zend_object, count: *mut zend_long) -> zend_result>;
pub type zend_object_get_closure_t = Option<
    unsafe extern "C" fn(
        obj: *mut zend_object,
        ce_ptr: *mut *mut zend_class_entry,
        fptr_ptr: *mut *mut zend_function,
        obj_ptr: *mut *mut zend_object,
        check_only: bool,
    ) -> zend_result,
>;
pub type zend_object_get_gc_t =
    Option<unsafe extern "C" fn(object: *mut zend_object, table: *mut *mut zval, n: *mut c_int) -> *mut HashTable>;
pub type zend_object_do_operation_t =
    Option<unsafe extern "C" fn(opcode: u8, result: *mut zval, op1: *mut zval, op2: *mut zval) -> zend_result>;

/// An object's behaviour. Extensions copy `std_object_handlers`, then replace
/// the hooks they implement and set `offset` to where their `zend_object`
/// sits inside the struct they allocate.
#[repr(C)]
pub struct zend_object_handlers {
    pub offset: c_int,
    pub free_obj: zend_object_free_obj_t,
    pub dtor_obj: zend_object_dtor_obj_t,
    pub clone_obj: zend_object_clone_obj_t,
    pub clone_obj_with: zend_object_clone_obj_with_t,
    pub read_property: zend_object_read_property_t,
    pub write_property: zend_object_write_property_t,
    pub read_dimension: zend_object_read_dimension_t,
    pub write_dimension: zend_object_write_dimension_t,
    pub get_property_ptr_ptr: zend_object_get_property_ptr_ptr_t,
    pub has_property: zend_object_has_property_t,
    pub unset_property: zend_object_unset_property_t,
    pub has_dimension: zend_object_has_dimension_t,
    pub unset_dimension: zend_object_unset_dimension_t,
    pub get_properties: zend_object_get_properties_t,
    pub get_method: zend_object_get_method_t,
    pub get_constructor: zend_object_get_constructor_t,
    pub get_class_name: zend_object_get_class_name_t,
    pub cast_object: zend_object_cast_t,
    pub count_elements: zend_object_count_elements_t,
    pub get_debug_info: zend_object_get_debug_info_t,
    pub get_closure: zend_object_get_closure_t,
    pub get_gc: zend_object_get_gc_t,
    pub do_operation: zend_object_do_operation_t,
    pub compare: zend_object_compare_t,
    pub get_properties_for: zend_object_get_properties_for_t,
}

#[repr(C)]
pub struct zend_object_iterator_funcs {
    pub dtor: Option<unsafe extern "C" fn(iter: *mut zend_object_iterator)>,
    pub valid: Option<unsafe extern "C" fn(iter: *mut zend_object_iterator) -> c_int>,
    pub get_current_data: Option<unsafe extern "C" fn(iter: *mut zend_object_iterator) -> *mut zval>,
    pub get_current_key: Option<unsafe extern "C" fn(iter: *mut zend_object_iterator, key: *mut zval)>,
    pub move_forward: Option<unsafe extern "C" fn(iter: *mut zend_object_iterator)>,
    pub rewind: Option<unsafe extern "C" fn(iter: *mut zend_object_iterator)>,
    pub invalidate_current: Option<unsafe extern "C" fn(iter: *mut zend_object_iterator)>,
    pub get_gc: Option<
        unsafe extern "C" fn(iter: *mut zend_object_iterator, table: *mut *mut zval, n: *mut c_int) -> *mut HashTable,
    >,
}

#[repr(C)]
pub struct zend_object_iterator {
    pub std: zend_object,
    pub data: zval,
    pub funcs: *const zend_object_iterator_funcs,
    pub index: zend_ulong,
}

// ------------------------------------------------------------ constants

#[repr(C)]
pub struct zend_constant {
    pub value: zval,
    pub name: *mut zend_string,
    pub filename: *mut zend_string,
    pub attributes: *mut HashTable,
}

// ------------------------------------------------------------ INI

/// `ZEND_INI_MH`: called when a directive is set; non-zero refuses the value.
pub type zend_ini_mh_t = Option<
    unsafe extern "C" fn(
        entry: *mut zend_ini_entry,
        new_value: *mut zend_string,
        mh_arg1: *mut c_void,
        mh_arg2: *mut c_void,
        mh_arg3: *mut c_void,
        stage: c_int,
    ) -> zend_result,
>;
pub type zend_ini_displayer_t = Option<unsafe extern "C" fn(ini_entry: *mut zend_ini_entry, r#type: c_int)>;

#[repr(C)]
pub struct zend_ini_entry_def {
    pub name: *const c_char,
    pub on_modify: zend_ini_mh_t,
    pub mh_arg1: *mut c_void,
    pub mh_arg2: *mut c_void,
    pub mh_arg3: *mut c_void,
    pub value: *const c_char,
    pub displayer: zend_ini_displayer_t,
    pub value_length: u32,
    pub name_length: u16,
    pub modifiable: u8,
}

#[repr(C)]
pub struct zend_ini_entry {
    pub name: *mut zend_string,
    pub on_modify: zend_ini_mh_t,
    pub mh_arg1: *mut c_void,
    pub mh_arg2: *mut c_void,
    pub mh_arg3: *mut c_void,
    pub value: *mut zend_string,
    pub orig_value: *mut zend_string,
    pub displayer: zend_ini_displayer_t,
    pub module_number: c_int,
    pub modifiable: u8,
    pub orig_modifiable: u8,
    pub modified: u8,
}

// ------------------------------------------------------------ globals

#[repr(C)]
pub struct zend_error_handling {
    pub handling: zend_error_handling_t,
    pub exception: *mut zend_class_entry,
}

/// `EG(...)`: the state of the running program as extensions see it.
#[repr(C)]
pub struct zend_executor_globals {
    pub uninitialized_zval: zval,
    pub error_zval: zval,
    pub symbol_table: HashTable,
    pub function_table: *mut HashTable,
    pub class_table: *mut HashTable,
    pub zend_constants: *mut HashTable,
    pub current_execute_data: *mut zend_execute_data,
    pub fake_scope: *mut zend_class_entry,
    pub precision: zend_long,
    pub exception: *mut zend_object,
    pub prev_exception: *mut zend_object,
    pub opline_before_exception: *const zend_op,
    pub error_reporting: c_int,
    pub exit_status: c_int,
    pub error_handling: zend_error_handling_t,
    pub exception_class: *mut zend_class_entry,
    pub regular_list: HashTable,
    pub persistent_list: HashTable,
    pub ini_directives: *mut HashTable,
    pub modified_ini_directives: *mut HashTable,
    pub user_error_handler: zval,
    pub user_exception_handler: zval,
    pub bailout: *mut sigjmp_buf,
    pub lineno_override: zend_long,
    pub filename_override: *mut zend_string,
    pub active: bool,
    pub flags: u8,
    pub no_extensions: bool,
    pub exception_ignore_args: bool,
    pub capture_warnings_during_sccp: u32,
    pub num_errors: u32,
    pub errors: *mut c_void,
    pub record_errors: bool,
    pub hard_timeout: zend_long,
    pub reserved: [*mut c_void; 6],
}

/// `CG(...)`: the compiler's state, of which only the tables matter here.
#[repr(C)]
pub struct zend_compiler_globals {
    pub function_table: *mut HashTable,
    pub class_table: *mut HashTable,
    pub auto_globals: *mut HashTable,
    pub active_class_entry: *mut zend_class_entry,
    pub compiled_filename: *mut zend_string,
    pub zend_lineno: c_int,
    pub compiler_options: u32,
    pub in_compilation: bool,
    pub arena: *mut c_void,
    pub reserved: [*mut c_void; 6],
}

// ------------------------------------------------------------ string builders

#[repr(C)]
pub struct smart_str {
    pub s: *mut zend_string,
    pub a: usize,
}

#[repr(C)]
pub struct smart_string {
    pub c: *mut c_char,
    pub len: usize,
    pub a: usize,
}

#[repr(C)]
pub struct zend_llist_element {
    pub next: *mut zend_llist_element,
    pub prev: *mut zend_llist_element,
    pub data: [c_char; 1],
}

pub type llist_dtor_func_t = Option<unsafe extern "C" fn(data: *mut c_void)>;

#[repr(C)]
pub struct zend_llist {
    pub head: *mut zend_llist_element,
    pub tail: *mut zend_llist_element,
    pub count: usize,
    pub size: usize,
    pub dtor: llist_dtor_func_t,
    pub persistent: c_uchar,
    pub traverse_ptr: *mut zend_llist_element,
}

// ------------------------------------------------------------ resources

pub type rsrc_dtor_func_t = Option<unsafe extern "C" fn(res: *mut zend_resource)>;

// ------------------------------------------------------------ extensions of the engine

#[repr(C)]
pub struct zend_extension {
    pub name: *mut c_char,
    pub version: *mut c_char,
    pub author: *mut c_char,
    pub URL: *mut c_char,
    pub copyright: *mut c_char,
    pub reserved: [*mut c_void; 16],
}
