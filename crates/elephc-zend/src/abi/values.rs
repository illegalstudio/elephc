//! Purpose:
//! The value layout of the extension ABI: zval, zend_string, HashTable and its
//! buckets, zend_object, resources and references.
//!
//! Called from:
//! - Every engine module, and `build.rs`, which emits each struct as C.
//!
//! Key details:
//! - These follow PHP 8.5's memory layout field for field, because extension
//!   code reads and writes them directly: it tests a zval's type byte, embeds a
//!   `zend_object` at the end of its own struct, walks a table's buckets, and
//!   (APCu, igbinary) copies a table's hash-and-bucket block by hand.
//! - A HashTable's `arData` points just past its hash slots, exactly as in PHP:
//!   `HT_GET_DATA_ADDR` must find the start of the allocation.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::{c_char, c_void};

use super::engine::{zend_class_entry, zend_function, zend_object_handlers, zend_property_info};
use super::scalars::*;

/// Callback releasing a value removed from a table.
pub type dtor_func_t = Option<unsafe extern "C" fn(pDest: *mut zval)>;
pub type copy_ctor_func_t = Option<unsafe extern "C" fn(pElement: *mut zval)>;

/// One word of a refcounted value's header besides its count.
#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_refcounted_h_u {
    pub type_info: u32,
}

/// The header every refcounted value starts with.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct zend_refcounted_h {
    pub refcount: u32,
    pub u: zend_refcounted_h_u,
}

#[repr(C)]
pub struct zend_refcounted {
    pub gc: zend_refcounted_h,
}

#[repr(C)]
pub struct zend_string {
    pub gc: zend_refcounted_h,
    pub h: zend_ulong,
    pub len: usize,
    pub val: [c_char; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct zend_value_ww {
    pub w1: u32,
    pub w2: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_value {
    pub lval: zend_long,
    pub dval: f64,
    pub counted: *mut zend_refcounted,
    pub str: *mut zend_string,
    pub arr: *mut zend_array,
    pub obj: *mut zend_object,
    pub res: *mut zend_resource,
    pub r#ref: *mut zend_reference,
    pub ast: *mut zend_ast_ref,
    pub zv: *mut zval,
    pub ptr: *mut c_void,
    pub ce: *mut zend_class_entry,
    pub func: *mut zend_function,
    pub ww: zend_value_ww,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zval_u1_v_u {
    pub extra: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct zval_u1_v {
    pub r#type: u8,
    pub type_flags: u8,
    pub u: zval_u1_v_u,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zval_u1 {
    pub type_info: u32,
    pub v: zval_u1_v,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zval_u2 {
    pub next: u32,
    pub cache_slot: u32,
    pub opline_num: u32,
    pub lineno: u32,
    pub num_args: u32,
    pub fe_pos: u32,
    pub fe_iter_idx: u32,
    pub guard: u32,
    pub constant_flags: u32,
    pub extra: u32,
}

/// A PHP value: 16 bytes, the payload then the type word.
/// c: tag _zval_struct
#[repr(C)]
#[derive(Clone, Copy)]
pub struct zval {
    pub value: zend_value,
    pub u1: zval_u1,
    pub u2: zval_u2,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bucket {
    pub val: zval,
    pub h: zend_ulong,
    pub key: *mut zend_string,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct zend_array_u_v {
    pub flags: u8,
    pub _unused: u8,
    pub nIteratorsCount: u8,
    pub _unused2: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_array_u {
    pub v: zend_array_u_v,
    pub flags: u32,
}

/// The three views of a table's storage pointer.
#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_array_data {
    pub arHash: *mut u32,
    pub arData: *mut Bucket,
    pub arPacked: *mut zval,
}

/// A PHP array (and every other hash table).
#[repr(C)]
pub struct zend_array {
    pub gc: zend_refcounted_h,
    pub u: zend_array_u,
    pub nTableMask: u32,
    /// c: anonymous
    pub data: zend_array_data,
    pub nNumUsed: u32,
    pub nNumOfElements: u32,
    pub nTableSize: u32,
    pub nInternalPointer: u32,
    pub nNextFreeElement: zend_long,
    pub pDestructor: dtor_func_t,
}

pub type HashTable = zend_array;

#[repr(C)]
pub struct zend_object {
    pub gc: zend_refcounted_h,
    pub handle: u32,
    pub ce: *mut zend_class_entry,
    pub handlers: *const zend_object_handlers,
    pub properties: *mut HashTable,
    pub properties_table: [zval; 1],
}

#[repr(C)]
pub struct zend_resource {
    pub gc: zend_refcounted_h,
    pub handle: zend_long,
    pub r#type: i32,
    pub ptr: *mut c_void,
}

#[repr(C)]
pub struct zend_property_info_list {
    pub num: usize,
    pub num_allocated: usize,
    pub ptr: [*mut zend_property_info; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union zend_property_info_source_list {
    pub ptr: *mut zend_property_info,
    pub list: usize,
}

#[repr(C)]
pub struct zend_reference {
    pub gc: zend_refcounted_h,
    pub val: zval,
    pub sources: zend_property_info_source_list,
}

#[repr(C)]
pub struct zend_ast_ref {
    pub gc: zend_refcounted_h,
}

/// A key of a table entry, as the apply callbacks receive it.
#[repr(C)]
pub struct zend_hash_key {
    pub h: zend_ulong,
    pub key: *mut zend_string,
}

pub type apply_func_t = Option<unsafe extern "C" fn(pDest: *mut zval) -> i32>;
pub type apply_func_arg_t = Option<unsafe extern "C" fn(pDest: *mut zval, argument: *mut c_void) -> i32>;
pub type apply_func_args_t =
    Option<unsafe extern "C" fn(pDest: *mut zval, num_args: i32, args: va_list, hash_key: *mut zend_hash_key) -> i32>;
pub type bucket_compare_func_t = Option<unsafe extern "C" fn(a: *mut Bucket, b: *mut Bucket) -> i32>;
pub type compare_func_t = Option<unsafe extern "C" fn(a: *const c_void, b: *const c_void) -> i32>;
pub type swap_func_t = Option<unsafe extern "C" fn(a: *mut c_void, b: *mut c_void)>;
pub type sort_func_t =
    Option<unsafe extern "C" fn(base: *mut c_void, nmemb: usize, siz: usize, cmp: compare_func_t, swp: swap_func_t)>;
pub type merge_checker_func_t = Option<
    unsafe extern "C" fn(target_ht: *mut HashTable, source_data: *mut zval, hash_key: *mut zend_hash_key, pParam: *mut c_void) -> bool,
>;

impl zval {
    /// An undefined zval.
    pub const UNDEF: zval = zval {
        value: zend_value { lval: 0 },
        u1: zval_u1 { type_info: IS_UNDEF as u32 },
        u2: zval_u2 { next: 0 },
    };
}
