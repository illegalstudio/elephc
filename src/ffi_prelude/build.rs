//! Purpose:
//! Builds the FFI prelude declarations as synthetic AST. This file is transcribed from the
//! PHP form in `super::fragments` and then reviewed; production never parses that text.
//!
//! Called from:
//! - `crate::ffi_prelude::ffi_declarations`.
//!
//! Key details:
//! - One helper per declaration keeps each builder frame small.
//! - The transcription is the source of truth for node shapes; hand edits must keep the
//!   parse-parity test in `ffi_prelude` green.

use crate::parser::ast::{BinOp, CType, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// `elephc_ffi_cdef` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cdef() -> Stmt {
    extern_fn("elephc_ffi_cdef", "elephc_ffi")
        .param("code", CType::Str)
        .param("library", CType::NullableStr)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_load` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_load() -> Stmt {
    extern_fn("elephc_ffi_load", "elephc_ffi")
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_scope` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_scope() -> Stmt {
    extern_fn("elephc_ffi_scope", "elephc_ffi")
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_error_kind` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_error_kind() -> Stmt {
    extern_fn("elephc_ffi_error_kind", "elephc_ffi")
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_error_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_error_ptr() -> Stmt {
    extern_fn("elephc_ffi_error_ptr", "elephc_ffi")
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_error_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_error_len() -> Stmt {
    extern_fn("elephc_ffi_error_len", "elephc_ffi")
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_clear_error` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_clear_error() -> Stmt {
    extern_fn("elephc_ffi_clear_error", "elephc_ffi")
        .returns(CType::Void)
        .build()
}

/// `elephc_ffi_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type() -> Stmt {
    extern_fn("elephc_ffi_type", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_name` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_name() -> Stmt {
    extern_fn("elephc_ffi_type_name", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_type_name_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_name_len() -> Stmt {
    extern_fn("elephc_ffi_type_name_len", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_kind` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_kind() -> Stmt {
    extern_fn("elephc_ffi_type_kind", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_size` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_size() -> Stmt {
    extern_fn("elephc_ffi_type_size", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_align` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_align() -> Stmt {
    extern_fn("elephc_ffi_type_align", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_attributes` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_attributes() -> Stmt {
    extern_fn("elephc_ffi_type_attributes", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_enum_kind` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_enum_kind() -> Stmt {
    extern_fn("elephc_ffi_type_enum_kind", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_array_element` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_array_element() -> Stmt {
    extern_fn("elephc_ffi_type_array_element", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_array_length` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_array_length() -> Stmt {
    extern_fn("elephc_ffi_type_array_length", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_pointer_to` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_pointer_to() -> Stmt {
    extern_fn("elephc_ffi_type_pointer_to", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_pointer_pointee` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_pointer_pointee() -> Stmt {
    extern_fn("elephc_ffi_type_pointer_pointee", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_array_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_array_type() -> Stmt {
    extern_fn("elephc_ffi_array_type", "elephc_ffi")
        .param("element", CType::Int)
        .param("length", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_struct_field_count` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_struct_field_count() -> Stmt {
    extern_fn("elephc_ffi_type_struct_field_count", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_struct_field_name` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_struct_field_name() -> Stmt {
    extern_fn("elephc_ffi_type_struct_field_name", "elephc_ffi")
        .param("type", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_type_struct_field_name_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_struct_field_name_len() -> Stmt {
    extern_fn("elephc_ffi_type_struct_field_name_len", "elephc_ffi")
        .param("type", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_struct_field_offset` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_struct_field_offset() -> Stmt {
    extern_fn("elephc_ffi_type_struct_field_offset", "elephc_ffi")
        .param("type", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_struct_field_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_struct_field_type() -> Stmt {
    extern_fn("elephc_ffi_type_struct_field_type", "elephc_ffi")
        .param("type", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_func_return` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_func_return() -> Stmt {
    extern_fn("elephc_ffi_type_func_return", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_func_param_count` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_func_param_count() -> Stmt {
    extern_fn("elephc_ffi_type_func_param_count", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_func_param` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_func_param() -> Stmt {
    extern_fn("elephc_ffi_type_func_param", "elephc_ffi")
        .param("type", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_type_func_abi` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_type_func_abi() -> Stmt {
    extern_fn("elephc_ffi_type_func_abi", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_new` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_new() -> Stmt {
    extern_fn("elephc_ffi_new", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("owned", CType::Bool)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_new_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_new_type() -> Stmt {
    extern_fn("elephc_ffi_new_type", "elephc_ffi")
        .param("context", CType::Int)
        .param("type", CType::Int)
        .param("owned", CType::Bool)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_new_any` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_new_any() -> Stmt {
    extern_fn("elephc_ffi_new_any", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("type", CType::Int)
        .param("owned", CType::Bool)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_set_persistent` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_set_persistent() -> Stmt {
    extern_fn("elephc_ffi_set_persistent", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_cast` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cast() -> Stmt {
    extern_fn("elephc_ffi_cast", "elephc_ffi")
        .param("type", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_cast_name` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cast_name() -> Stmt {
    extern_fn("elephc_ffi_cast_name", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_cast_any` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cast_any() -> Stmt {
    extern_fn("elephc_ffi_cast_any", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("type", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_typeof` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_typeof() -> Stmt {
    extern_fn("elephc_ffi_typeof", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_cdata_type_name` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cdata_type_name() -> Stmt {
    extern_fn("elephc_ffi_cdata_type_name", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_cdata_type_name_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cdata_type_name_len() -> Stmt {
    extern_fn("elephc_ffi_cdata_type_name_len", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_sizeof_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_sizeof_cdata() -> Stmt {
    extern_fn("elephc_ffi_sizeof_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_alignof_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_alignof_cdata() -> Stmt {
    extern_fn("elephc_ffi_alignof_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_sizeof_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_sizeof_type() -> Stmt {
    extern_fn("elephc_ffi_sizeof_type", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_alignof_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_alignof_type() -> Stmt {
    extern_fn("elephc_ffi_alignof_type", "elephc_ffi")
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_addr` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_addr() -> Stmt {
    extern_fn("elephc_ffi_addr", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_addr_owned` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_addr_owned() -> Stmt {
    extern_fn("elephc_ffi_addr_owned", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_cdata_is_callable` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cdata_is_callable() -> Stmt {
    extern_fn("elephc_ffi_cdata_is_callable", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_free` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_free() -> Stmt {
    extern_fn("elephc_ffi_free", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_release_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_release_cdata() -> Stmt {
    extern_fn("elephc_ffi_release_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_ffi_cdata_clone` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cdata_clone() -> Stmt {
    extern_fn("elephc_ffi_cdata_clone", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_is_null` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_is_null() -> Stmt {
    extern_fn("elephc_ffi_is_null", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_is_freed` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_is_freed() -> Stmt {
    extern_fn("elephc_ffi_is_freed", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_pointer_value` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_pointer_value() -> Stmt {
    extern_fn("elephc_ffi_pointer_value", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::IntWidth(crate::parser::ast::CIntWidth::U64))
        .build()
}

/// `elephc_ffi_memcpy` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memcpy() -> Stmt {
    extern_fn("elephc_ffi_memcpy", "elephc_ffi")
        .param("to", CType::Int)
        .param("from", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_memcpy_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memcpy_string() -> Stmt {
    extern_fn("elephc_ffi_memcpy_string", "elephc_ffi")
        .param("to", CType::Int)
        .param("from", CType::Str)
        .param("from_len", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_memcmp` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memcmp() -> Stmt {
    extern_fn("elephc_ffi_memcmp", "elephc_ffi")
        .param("left", CType::Int)
        .param("right", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_memcmp_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memcmp_string() -> Stmt {
    extern_fn("elephc_ffi_memcmp_string", "elephc_ffi")
        .param("left", CType::Int)
        .param("right", CType::Str)
        .param("right_len", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_memset` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memset() -> Stmt {
    extern_fn("elephc_ffi_memset", "elephc_ffi")
        .param("ptr", CType::Int)
        .param("value", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_string() -> Stmt {
    extern_fn("elephc_ffi_string", "elephc_ffi")
        .param("ptr", CType::Int)
        .param("size", CType::Int)
        .param("has_size", CType::Bool)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_string_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_string_len() -> Stmt {
    extern_fn("elephc_ffi_string_len", "elephc_ffi")
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_kind` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_kind() -> Stmt {
    extern_fn("elephc_ffi_field_kind", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_int() -> Stmt {
    extern_fn("elephc_ffi_field_int", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_float() -> Stmt {
    extern_fn("elephc_ffi_field_float", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Float)
        .build()
}

/// `elephc_ffi_field_bool` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_bool() -> Stmt {
    extern_fn("elephc_ffi_field_bool", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_char` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_char() -> Stmt {
    extern_fn("elephc_ffi_field_char", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_cdata() -> Stmt {
    extern_fn("elephc_ffi_field_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_pointer_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_pointer_type() -> Stmt {
    extern_fn("elephc_ffi_field_pointer_type", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_is_null` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_is_null() -> Stmt {
    extern_fn("elephc_ffi_field_is_null", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_addr` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_addr() -> Stmt {
    extern_fn("elephc_ffi_field_addr", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_type() -> Stmt {
    extern_fn("elephc_ffi_field_type", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_size` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_size() -> Stmt {
    extern_fn("elephc_ffi_field_size", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_write_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_int() -> Stmt {
    extern_fn("elephc_ffi_field_write_int", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_write_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_float() -> Stmt {
    extern_fn("elephc_ffi_field_write_float", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Float)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_write_bool` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_bool() -> Stmt {
    extern_fn("elephc_ffi_field_write_bool", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Bool)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_write_char` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_char() -> Stmt {
    extern_fn("elephc_ffi_field_write_char", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_write_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_cdata() -> Stmt {
    extern_fn("elephc_ffi_field_write_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_write_pointer` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_pointer() -> Stmt {
    extern_fn("elephc_ffi_field_write_pointer", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_write_null` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_null() -> Stmt {
    extern_fn("elephc_ffi_field_write_null", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_write_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_string() -> Stmt {
    extern_fn("elephc_ffi_field_write_string", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_field_count` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_count() -> Stmt {
    extern_fn("elephc_ffi_field_count", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_name_at` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_name_at() -> Stmt {
    extern_fn("elephc_ffi_field_name_at", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_field_name_at_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_name_at_len() -> Stmt {
    extern_fn("elephc_ffi_field_name_at_len", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_kind` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_kind() -> Stmt {
    extern_fn("elephc_ffi_offset_kind", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_int() -> Stmt {
    extern_fn("elephc_ffi_offset_int", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_float() -> Stmt {
    extern_fn("elephc_ffi_offset_float", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Float)
        .build()
}

/// `elephc_ffi_offset_bool` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_bool() -> Stmt {
    extern_fn("elephc_ffi_offset_bool", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_char` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_char() -> Stmt {
    extern_fn("elephc_ffi_offset_char", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_cdata() -> Stmt {
    extern_fn("elephc_ffi_offset_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_pointer_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_pointer_type() -> Stmt {
    extern_fn("elephc_ffi_offset_pointer_type", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_is_null` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_is_null() -> Stmt {
    extern_fn("elephc_ffi_offset_is_null", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_addr` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_addr() -> Stmt {
    extern_fn("elephc_ffi_offset_addr", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_type() -> Stmt {
    extern_fn("elephc_ffi_offset_type", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_size` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_size() -> Stmt {
    extern_fn("elephc_ffi_offset_size", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_write_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_int() -> Stmt {
    extern_fn("elephc_ffi_offset_write_int", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_write_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_float() -> Stmt {
    extern_fn("elephc_ffi_offset_write_float", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("input", CType::Float)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_write_bool` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_bool() -> Stmt {
    extern_fn("elephc_ffi_offset_write_bool", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("input", CType::Bool)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_write_char` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_char() -> Stmt {
    extern_fn("elephc_ffi_offset_write_char", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("input", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_write_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_cdata() -> Stmt {
    extern_fn("elephc_ffi_offset_write_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_write_pointer` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_pointer() -> Stmt {
    extern_fn("elephc_ffi_offset_write_pointer", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_write_null` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_null() -> Stmt {
    extern_fn("elephc_ffi_offset_write_null", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_write_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_string() -> Stmt {
    extern_fn("elephc_ffi_offset_write_string", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("input", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_array_length` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_array_length() -> Stmt {
    extern_fn("elephc_ffi_array_length", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_iter_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_iter_cdata() -> Stmt {
    extern_fn("elephc_ffi_iter_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_free_offset` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_free_offset() -> Stmt {
    extern_fn("elephc_ffi_free_offset", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_free_field` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_free_field() -> Stmt {
    extern_fn("elephc_ffi_free_field", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_offset_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_string() -> Stmt {
    extern_fn("elephc_ffi_offset_string", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("size", CType::Int)
        .param("has_size", CType::Bool)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_field_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_string() -> Stmt {
    extern_fn("elephc_ffi_field_string", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("size", CType::Int)
        .param("has_size", CType::Bool)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_memset_offset` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memset_offset() -> Stmt {
    extern_fn("elephc_ffi_memset_offset", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("byte", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_memset_field` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memset_field() -> Stmt {
    extern_fn("elephc_ffi_memset_field", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("byte", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_memcpy_offset` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memcpy_offset() -> Stmt {
    extern_fn("elephc_ffi_memcpy_offset", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .param("source", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_memcpy_field` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_memcpy_field() -> Stmt {
    extern_fn("elephc_ffi_memcpy_field", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .param("source", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_global_kind` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_kind() -> Stmt {
    extern_fn("elephc_ffi_global_kind", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_global_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_int() -> Stmt {
    extern_fn("elephc_ffi_global_int", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_global_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_float() -> Stmt {
    extern_fn("elephc_ffi_global_float", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Float)
        .build()
}

/// `elephc_ffi_global_bool` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_bool() -> Stmt {
    extern_fn("elephc_ffi_global_bool", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_global_char` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_char() -> Stmt {
    extern_fn("elephc_ffi_global_char", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_global_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_cdata() -> Stmt {
    extern_fn("elephc_ffi_global_cdata", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_global_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_string() -> Stmt {
    extern_fn("elephc_ffi_global_string", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_global_write_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_write_int() -> Stmt {
    extern_fn("elephc_ffi_global_write_int", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_global_write_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_write_float() -> Stmt {
    extern_fn("elephc_ffi_global_write_float", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Float)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_global_write_bool` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_write_bool() -> Stmt {
    extern_fn("elephc_ffi_global_write_bool", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Bool)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_global_write_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_write_cdata() -> Stmt {
    extern_fn("elephc_ffi_global_write_cdata", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_global_write_pointer` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_write_pointer() -> Stmt {
    extern_fn("elephc_ffi_global_write_pointer", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("input", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_global_write_null` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_write_null() -> Stmt {
    extern_fn("elephc_ffi_global_write_null", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_cdata_address` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cdata_address() -> Stmt {
    extern_fn("elephc_ffi_cdata_address", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_address` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_address() -> Stmt {
    extern_fn("elephc_ffi_offset_address", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_global_address` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_address() -> Stmt {
    extern_fn("elephc_ffi_global_address", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_global_pointer_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_pointer_type() -> Stmt {
    extern_fn("elephc_ffi_global_pointer_type", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_cdata_pointer` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cdata_pointer() -> Stmt {
    extern_fn("elephc_ffi_cdata_pointer", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_global_write_closure` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_global_write_closure() -> Stmt {
    extern_fn("elephc_ffi_global_write_closure", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_field_write_closure` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_field_write_closure() -> Stmt {
    extern_fn("elephc_ffi_field_write_closure", "elephc_ffi")
        .param("value", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_offset_write_closure` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_offset_write_closure() -> Stmt {
    extern_fn("elephc_ffi_offset_write_closure", "elephc_ffi")
        .param("value", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_cdata_write_closure` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_cdata_write_closure() -> Stmt {
    extern_fn("elephc_ffi_cdata_write_closure", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_set_dispatcher_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_set_dispatcher_int() -> Stmt {
    extern_fn("elephc_ffi_set_dispatcher_int", "elephc_ffi")
        .param("dispatcher", CType::Callable)
        .returns(CType::Void)
        .build()
}

/// `elephc_ffi_set_dispatcher_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_set_dispatcher_float() -> Stmt {
    extern_fn("elephc_ffi_set_dispatcher_float", "elephc_ffi")
        .param("dispatcher", CType::Callable)
        .returns(CType::Void)
        .build()
}

/// `elephc_ffi_set_dispatcher_void` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_set_dispatcher_void() -> Stmt {
    extern_fn("elephc_ffi_set_dispatcher_void", "elephc_ffi")
        .param("dispatcher", CType::Callable)
        .returns(CType::Void)
        .build()
}

/// `elephc_ffi_set_dispatcher_aggregate` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_set_dispatcher_aggregate() -> Stmt {
    extern_fn("elephc_ffi_set_dispatcher_aggregate", "elephc_ffi")
        .param("dispatcher", CType::Callable)
        .returns(CType::Void)
        .build()
}

/// `elephc_ffi_stage_aggregate` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_stage_aggregate() -> Stmt {
    extern_fn("elephc_ffi_stage_aggregate", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_callback_arg_count` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_arg_count() -> Stmt {
    extern_fn("elephc_ffi_callback_arg_count", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_callback_arg_kind` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_arg_kind() -> Stmt {
    extern_fn("elephc_ffi_callback_arg_kind", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_callback_arg_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_arg_int() -> Stmt {
    extern_fn("elephc_ffi_callback_arg_int", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_callback_arg_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_arg_float() -> Stmt {
    extern_fn("elephc_ffi_callback_arg_float", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Float)
        .build()
}

/// `elephc_ffi_callback_arg_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_arg_string() -> Stmt {
    extern_fn("elephc_ffi_callback_arg_string", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_callback_arg_string_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_arg_string_len() -> Stmt {
    extern_fn("elephc_ffi_callback_arg_string_len", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_callback_arg_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_arg_cdata() -> Stmt {
    extern_fn("elephc_ffi_callback_arg_cdata", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_callback_name` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_name() -> Stmt {
    extern_fn("elephc_ffi_callback_name", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_callback_name_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_name_len() -> Stmt {
    extern_fn("elephc_ffi_callback_name_len", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_callback_active_name` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_active_name() -> Stmt {
    extern_fn("elephc_ffi_callback_active_name", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_callback_active_name_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_active_name_len() -> Stmt {
    extern_fn("elephc_ffi_callback_active_name_len", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_callback_abort` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_abort() -> Stmt {
    extern_fn("elephc_ffi_callback_abort", "elephc_ffi")
        .returns(CType::Void)
        .build()
}

/// `elephc_ffi_function_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_function_cdata() -> Stmt {
    extern_fn("elephc_ffi_function_cdata", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_begin` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_begin() -> Stmt {
    extern_fn("elephc_ffi_call_begin", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_begin_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_begin_cdata() -> Stmt {
    extern_fn("elephc_ffi_call_begin_cdata", "elephc_ffi")
        .param("value", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_arg_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_arg_int() -> Stmt {
    extern_fn("elephc_ffi_call_arg_int", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_call_arg_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_arg_float() -> Stmt {
    extern_fn("elephc_ffi_call_arg_float", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .param("value", CType::Float)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_call_arg_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_arg_string() -> Stmt {
    extern_fn("elephc_ffi_call_arg_string", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .param("value", CType::Str)
        .param("len", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_call_arg_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_arg_cdata() -> Stmt {
    extern_fn("elephc_ffi_call_arg_cdata", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_call_param_is_function_pointer` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_param_is_function_pointer() -> Stmt {
    extern_fn("elephc_ffi_call_param_is_function_pointer", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_call_arg_callback` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_arg_callback() -> Stmt {
    extern_fn("elephc_ffi_call_arg_callback", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_arg_pointer` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_arg_pointer() -> Stmt {
    extern_fn("elephc_ffi_call_arg_pointer", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .param("value", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_callback_code` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_callback_code() -> Stmt {
    extern_fn("elephc_ffi_callback_code", "elephc_ffi")
        .param("id", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_arg_null` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_arg_null() -> Stmt {
    extern_fn("elephc_ffi_call_arg_null", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_call_arg_bool` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_arg_bool() -> Stmt {
    extern_fn("elephc_ffi_call_arg_bool", "elephc_ffi")
        .param("call", CType::Int)
        .param("index", CType::Int)
        .param("value", CType::Bool)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_call_invoke` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_invoke() -> Stmt {
    extern_fn("elephc_ffi_call_invoke", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_result_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_result_int() -> Stmt {
    extern_fn("elephc_ffi_call_result_int", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_result_float` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_result_float() -> Stmt {
    extern_fn("elephc_ffi_call_result_float", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Float)
        .build()
}

/// `elephc_ffi_call_result_bool` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_result_bool() -> Stmt {
    extern_fn("elephc_ffi_call_result_bool", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_call_result_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_result_cdata() -> Stmt {
    extern_fn("elephc_ffi_call_result_cdata", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_result_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_result_string() -> Stmt {
    extern_fn("elephc_ffi_call_result_string", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_call_result_string_len` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_result_string_len() -> Stmt {
    extern_fn("elephc_ffi_call_result_string_len", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_call_release` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_call_release() -> Stmt {
    extern_fn("elephc_ffi_call_release", "elephc_ffi")
        .param("call", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_ffi_binary_kind` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_binary_kind() -> Stmt {
    extern_fn("elephc_ffi_binary_kind", "elephc_ffi")
        .param("left", CType::Int)
        .param("left_kind", CType::Int)
        .param("right", CType::Int)
        .param("right_kind", CType::Int)
        .param("op", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_binary_int` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_binary_int() -> Stmt {
    extern_fn("elephc_ffi_binary_int", "elephc_ffi")
        .param("left", CType::Int)
        .param("left_kind", CType::Int)
        .param("right", CType::Int)
        .param("right_kind", CType::Int)
        .param("op", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_binary_cdata` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_binary_cdata() -> Stmt {
    extern_fn("elephc_ffi_binary_cdata", "elephc_ffi")
        .param("left", CType::Int)
        .param("left_kind", CType::Int)
        .param("right", CType::Int)
        .param("right_kind", CType::Int)
        .param("op", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_binary_compare` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_binary_compare() -> Stmt {
    extern_fn("elephc_ffi_binary_compare", "elephc_ffi")
        .param("left", CType::Int)
        .param("left_kind", CType::Int)
        .param("right", CType::Int)
        .param("right_kind", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_last_place_type` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_type() -> Stmt {
    extern_fn("elephc_ffi_last_place_type", "elephc_ffi")
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_last_place_addr` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_addr() -> Stmt {
    extern_fn("elephc_ffi_last_place_addr", "elephc_ffi")
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_last_place_size` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_size() -> Stmt {
    extern_fn("elephc_ffi_last_place_size", "elephc_ffi")
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_last_place_align` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_align() -> Stmt {
    extern_fn("elephc_ffi_last_place_align", "elephc_ffi")
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_last_place_string` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_string() -> Stmt {
    extern_fn("elephc_ffi_last_place_string", "elephc_ffi")
        .param("size", CType::Int)
        .param("has_size", CType::Bool)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_ffi_last_place_memset` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_memset() -> Stmt {
    extern_fn("elephc_ffi_last_place_memset", "elephc_ffi")
        .param("byte", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_last_place_free` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_free() -> Stmt {
    extern_fn("elephc_ffi_last_place_free", "elephc_ffi")
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_last_place_memcmp` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_memcmp() -> Stmt {
    extern_fn("elephc_ffi_last_place_memcmp", "elephc_ffi")
        .param("other", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_ffi_last_place_memcpy` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_memcpy() -> Stmt {
    extern_fn("elephc_ffi_last_place_memcpy", "elephc_ffi")
        .param("destination", CType::Int)
        .param("size", CType::Int)
        .returns(CType::Bool)
        .build()
}

/// `elephc_ffi_last_place_cast` — transcribed from the PHP form.
fn decl_extern_elephc_ffi_last_place_cast() -> Stmt {
    extern_fn("elephc_ffi_last_place_cast", "elephc_ffi")
        .param("context", CType::Int)
        .param("name", CType::Str)
        .param("type", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `__elephc_ffi_type_handle` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_type_handle() -> Stmt {
    function("__elephc_ffi_type_handle")
        .param("type", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_instance_of(e_var("type"), "\\FFI\\CType"),
                vec![
                    s_return(e_method_call(e_var("type"), "__elephc_raw", vec![])),
                ],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// `__elephc_ffi_type_name` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_type_name() -> Stmt {
    function("__elephc_ffi_type_name")
        .param("type", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_string", vec![e_var("type")]),
                vec![
                    s_return(e_var("type")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_ffi_field_value` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_field_value() -> Stmt {
    function("__elephc_ffi_field_value")
        .param("handle", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .param("kind", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)),
                vec![
                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_call("elephc_ffi_field_cdata", vec![e_var("handle"), e_var("name")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)),
                vec![
                    s_return(e_call("chr", vec![e_call("elephc_ffi_field_char", vec![e_var("handle"), e_var("name")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                vec![
                    s_return(e_call("elephc_ffi_field_bool", vec![e_var("handle"), e_var("name")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_return(e_call("elephc_ffi_field_float", vec![e_var("handle"), e_var("name")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_ffi_field_int", vec![e_var("handle"), e_var("name")])),
        ])
        .build()
}

/// `__elephc_ffi_offset_value` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_offset_value() -> Stmt {
    function("__elephc_ffi_offset_value")
        .param("handle", TypeExpr::Int)
        .param("index", TypeExpr::Int)
        .param("kind", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)),
                vec![
                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_call("elephc_ffi_offset_cdata", vec![e_var("handle"), e_var("index")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)),
                vec![
                    s_return(e_call("chr", vec![e_call("elephc_ffi_offset_char", vec![e_var("handle"), e_var("index")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                vec![
                    s_return(e_call("elephc_ffi_offset_bool", vec![e_var("handle"), e_var("index")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_return(e_call("elephc_ffi_offset_float", vec![e_var("handle"), e_var("index")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_ffi_offset_int", vec![e_var("handle"), e_var("index")])),
        ])
        .build()
}

/// `__elephc_ffi_add_arg` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_add_arg() -> Stmt {
    function("__elephc_ffi_add_arg")
        .param("call", TypeExpr::Int)
        .param("index", TypeExpr::Int)
        .param("arg", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_call("elephc_ffi_call_param_is_function_pointer", vec![e_var("call"), e_var("index")]),
                vec![
                    s_if(
                        e_call("is_null", vec![e_var("arg")]),
                        vec![
                            s_expr(e_call("elephc_ffi_call_arg_null", vec![e_var("call"), e_var("index")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("arg"), "\\FFI\\CData"),
                        vec![
                            s_expr(e_call("elephc_ffi_call_arg_cdata", vec![e_var("call"), e_var("index"), e_method_call(e_var("arg"), "__elephc_raw", vec![])])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("is_callable", vec![e_var("arg")]),
                        vec![
                            s_expr(e_static_call("\\FFI", "__elephc_ensure_dispatchers", vec![])),
                            s_assign("id", e_call("elephc_ffi_call_arg_callback", vec![e_var("call"), e_var("index")])),
                            s_if(
                                e_binop(e_var("id"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_static_call("\\FFI", "__elephc_register_callback_closure", vec![e_var("id"), e_var("arg")])),
                            s_expr(e_call("elephc_ffi_call_arg_pointer", vec![e_var("call"), e_var("index"), e_call("elephc_ffi_callback_code", vec![e_var("id")])])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("is_string", vec![e_var("arg")]),
                        vec![
                            s_throw(e_new_fq("FFI\\Exception", vec![e_binop(e_binop(e_str("Attempt to assign an invalid callback, function \""), BinOp::Concat, e_var("arg")), BinOp::Concat, e_str("\" not found or invalid function name"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to assign an invalid callback, no array or string given")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_null", vec![e_var("arg")]),
                vec![
                    s_expr(e_call("elephc_ffi_call_arg_null", vec![e_var("call"), e_var("index")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("arg")]),
                vec![
                    s_expr(e_call("elephc_ffi_call_arg_bool", vec![e_var("call"), e_var("index"), e_var("arg")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_int", vec![e_var("arg")]),
                vec![
                    s_expr(e_call("elephc_ffi_call_arg_int", vec![e_var("call"), e_var("index"), e_var("arg")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("arg")]),
                vec![
                    s_expr(e_call("elephc_ffi_call_arg_float", vec![e_var("call"), e_var("index"), e_var("arg")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("arg")]),
                vec![
                    s_expr(e_call("elephc_ffi_call_arg_string", vec![e_var("call"), e_var("index"), e_var("arg"), e_call("strlen", vec![e_var("arg")])])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("arg"), "\\FFI\\CData"),
                vec![
                    s_expr(e_call("elephc_ffi_call_arg_cdata", vec![e_var("call"), e_var("index"), e_method_call(e_var("arg"), "__elephc_raw", vec![])])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("arg"), "\\Closure"),
                vec![
                    s_throw(e_new_fq("FFI\\Exception", vec![e_str("Passing incompatible argument for a C parameter, found PHP 'Closure'")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_object", vec![e_var("arg")]),
                vec![
                    s_expr(e_call("elephc_ffi_call_arg_cdata", vec![e_var("call"), e_var("index"), e_method_call(e_var("arg"), "__elephc_raw", vec![])])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_throw(e_new_fq("FFI\\Exception", vec![e_binop(e_str("Unsupported FFI argument type: "), BinOp::Concat, e_call("gettype", vec![e_var("arg")]))])),
        ])
        .build()
}

/// `__elephc_ffi_operand_value` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_operand_value() -> Stmt {
    function("__elephc_ffi_operand_value")
        .param("value", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_instance_of(e_var("value"), "\\FFI\\CData"),
                vec![
                    s_return(e_method_call(e_var("value"), "__elephc_raw", vec![])),
                ],
                vec![],
                None,
            ),
            s_return(e_cast(CastType::Int, e_var("value"))),
        ])
        .build()
}

/// `__elephc_ffi_operand_kind` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_operand_kind() -> Stmt {
    function("__elephc_ffi_operand_kind")
        .param("value", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_instance_of(e_var("value"), "\\FFI\\CData"),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// `__elephc_ffi_operand_type` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_operand_type() -> Stmt {
    function("__elephc_ffi_operand_type")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_instance_of(e_var("value"), "\\FFI\\CData"),
                vec![
                    s_return(e_str("FFI\\CData")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_str("null")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![
                    s_return(e_str("bool")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![
                    s_return(e_str("int")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("value")]),
                vec![
                    s_return(e_str("float")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_return(e_str("string")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_return(e_str("array")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![
                    s_return(e_call("get_class", vec![e_var("value")])),
                ],
                vec![],
                None,
            ),
            s_return(e_str("resource")),
        ])
        .build()
}

/// `__elephc_ffi_unsupported_operands` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_unsupported_operands() -> Stmt {
    function("__elephc_ffi_unsupported_operands")
        .param("left", t_mixed())
        .param("right", t_mixed())
        .param("operator", TypeExpr::Str)
        .param_default("object_first", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("left_type", e_call("__elephc_ffi_operand_type", vec![e_var("left")])),
            s_assign("right_type", e_call("__elephc_ffi_operand_type", vec![e_var("right")])),
            s_if(
                e_binop(e_binop(e_var("object_first"), BinOp::And, e_not(e_instance_of(e_var("left"), "\\FFI\\CData"))), BinOp::And, e_instance_of(e_var("right"), "\\FFI\\CData")),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Unsupported operand types: "), BinOp::Concat, e_var("right_type")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("operator")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("left_type"))])),
                ],
                vec![],
                None,
            ),
            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Unsupported operand types: "), BinOp::Concat, e_var("left_type")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("operator")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("right_type"))])),
        ])
        .build()
}

/// `__elephc_ffi_finish_call` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_finish_call() -> Stmt {
    function("__elephc_ffi_finish_call")
        .param("call", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("kind", e_call("elephc_ffi_call_invoke", vec![e_var("call")])),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(7)),
                vec![
                    s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_assign("result", e_call("elephc_ffi_call_result_int", vec![e_var("call")])),
                ],
                vec![
                (e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)), vec![
                    s_assign("result", e_call("elephc_ffi_call_result_float", vec![e_var("call")])),
                ]),
                (e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), vec![
                    s_assign("result", e_call("elephc_ffi_call_result_bool", vec![e_var("call")])),
                ]),
                (e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)), vec![
                    s_assign("result", e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_call("elephc_ffi_call_result_cdata", vec![e_var("call")])])),
                ]),
                (e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)), vec![
                    s_assign("ptr", e_call("elephc_ffi_call_result_string", vec![e_var("call")])),
                    s_assign("len", e_call("elephc_ffi_call_result_string_len", vec![e_var("call")])),
                    s_assign("result", e_call("ptr_read_string", vec![e_var("ptr"), e_var("len")])),
                ]),
                (e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)), vec![
                    s_assign("result", e_call("chr", vec![e_call("elephc_ffi_call_result_int", vec![e_var("call")])])),
                ]),
            ],
                Some(vec![
                s_assign("result", e_null()),
            ]),
            ),
            s_expr(e_call("elephc_ffi_call_release", vec![e_var("call")])),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_ffi_callback_dispatch_int` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_callback_dispatch_int() -> Stmt {
    function("__elephc_ffi_callback_dispatch_int")
        .param("id", TypeExpr::Int)
        .param("call", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_static_call("\\FFI", "__elephc_callback_invoke_int", vec![e_var("id"), e_var("call")])),
        ])
        .build()
}

/// `__elephc_ffi_callback_dispatch_float` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_callback_dispatch_float() -> Stmt {
    function("__elephc_ffi_callback_dispatch_float")
        .param("id", TypeExpr::Int)
        .param("call", TypeExpr::Int)
        .returns(TypeExpr::Float)
        .body(vec![
            s_return(e_static_call("\\FFI", "__elephc_callback_invoke_float", vec![e_var("id"), e_var("call")])),
        ])
        .build()
}

/// `__elephc_ffi_callback_dispatch_void` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_callback_dispatch_void() -> Stmt {
    function("__elephc_ffi_callback_dispatch_void")
        .param("id", TypeExpr::Int)
        .param("call", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_static_call("\\FFI", "__elephc_callback_invoke_void", vec![e_var("id"), e_var("call")])),
        ])
        .build()
}

/// `__elephc_ffi_callback_dispatch_aggregate` — transcribed from the PHP form.
fn decl_fn_elephc_ffi_callback_dispatch_aggregate() -> Stmt {
    function("__elephc_ffi_callback_dispatch_aggregate")
        .param("id", TypeExpr::Int)
        .param("call", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_static_call("\\FFI", "__elephc_callback_invoke_aggregate", vec![e_var("id"), e_var("call")])),
        ])
        .build()
}

/// `FFI` — transcribed from the PHP form.
fn decl_class_ffi() -> Stmt {
    class("FFI")
        .final_()
        .implements("\\IteratorAggregate")
        .constant("__BIGGEST_ALIGNMENT__", e_int(16))
        .private_prop("__elephc_context", TypeExpr::Int, None)
        .private_static_prop("__elephc_closures", t_array(), Some(e_array(vec![])))
        .private_static_prop("__elephc_callback_closures", t_array(), Some(e_array(vec![])))
        .private_static_prop("__elephc_dispatchers_ready", TypeExpr::Bool, Some(e_bool(false)))
        .private_static_prop("__elephc_enabled", TypeExpr::Bool, Some(e_bool(true)))
        .method(
            method("__elephc_validate_api")
                .private()
                .static_()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_not(e_self_static_prop("__elephc_enabled")),
                        vec![
                            s_throw(e_new_fq("FFI\\Exception", vec![e_str("FFI API is restricted by \"ffi.enable\" configuration directive")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_register_closure")
                .static_()
                .param("address", TypeExpr::Int)
                .param("closure", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("closures", e_self_static_prop("__elephc_closures")),
                    s_array_assign("closures", e_var("address"), e_var("closure")),
                    s_self_static_prop_assign("__elephc_closures", e_var("closures")),
                ]),
        )
        .method(
            method("__elephc_has_closure")
                .static_()
                .param("address", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("closures", e_self_static_prop("__elephc_closures")),
                    s_return(e_call("isset", vec![e_index(e_var("closures"), e_var("address"))])),
                ]),
        )
        .method(
            method("__elephc_closure")
                .static_()
                .param("address", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_assign("closures", e_self_static_prop("__elephc_closures")),
                    s_return(e_index(e_var("closures"), e_var("address"))),
                ]),
        )
        .method(
            method("__elephc_register_callback_closure")
                .static_()
                .param("id", TypeExpr::Int)
                .param("closure", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("closures", e_self_static_prop("__elephc_callback_closures")),
                    s_array_assign("closures", e_var("id"), e_var("closure")),
                    s_self_static_prop_assign("__elephc_callback_closures", e_var("closures")),
                ]),
        )
        .method(
            method("__elephc_invoke_array")
                .static_()
                .param("callback", t_class("\\FFI\\CData"))
                .param("args", t_array())
                .returns(t_mixed())
                .body(vec![
                    s_return(e_method_call(e_var("callback"), "__elephc_invoke_array", vec![e_var("args")])),
                ]),
        )
        .method(
            method("__elephc_set_iter_by_ref")
                .static_()
                .param("data", t_class("\\FFI\\CData"))
                .param("by_ref", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_var("data"), "__elephc_set_iter_by_ref", vec![e_var("by_ref")])),
                ]),
        )
        .method(
            method("__elephc_ensure_dispatchers")
                .static_()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_self_static_prop("__elephc_dispatchers_ready"),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_self_static_prop_assign("__elephc_dispatchers_ready", e_bool(true)),
                    s_expr(e_call("elephc_ffi_set_dispatcher_int", vec![e_str("__elephc_ffi_callback_dispatch_int")])),
                    s_expr(e_call("elephc_ffi_set_dispatcher_float", vec![e_str("__elephc_ffi_callback_dispatch_float")])),
                    s_expr(e_call("elephc_ffi_set_dispatcher_void", vec![e_str("__elephc_ffi_callback_dispatch_void")])),
                    s_expr(e_call("elephc_ffi_set_dispatcher_aggregate", vec![e_str("__elephc_ffi_callback_dispatch_aggregate")])),
                ]),
        )
        .method(
            method("__elephc_callback_invoke")
                .static_()
                .param("id", TypeExpr::Int)
                .param("call", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_assign("closures", e_self_static_prop("__elephc_callback_closures")),
                    s_assign("closure", e_null_coalesce(e_index(e_var("closures"), e_var("id")), e_null())),
                    s_if(
                        e_not(e_call("is_callable", vec![e_var("closure")])),
                        vec![
                            s_throw(e_new_fq("FFI\\Exception", vec![e_str("Cannot call callback")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("args", e_array(vec![])),
                    s_assign("count", e_call("elephc_ffi_callback_arg_count", vec![e_var("call")])),
                    s_for(Some(s_assign("index", e_int(0))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("index"))), vec![
                        s_assign("kind", e_call("elephc_ffi_callback_arg_kind", vec![e_var("call"), e_var("index")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_array_push("args", e_call("elephc_ffi_callback_arg_int", vec![e_var("call"), e_var("index")])),
                            ],
                            vec![
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)), vec![
                                s_array_push("args", e_call("elephc_ffi_callback_arg_float", vec![e_var("call"), e_var("index")])),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), vec![
                                s_array_push("args", e_binop(e_call("elephc_ffi_callback_arg_int", vec![e_var("call"), e_var("index")]), BinOp::StrictNotEq, e_int(0))),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)), vec![
                                s_array_push("args", e_call("chr", vec![e_call("elephc_ffi_callback_arg_int", vec![e_var("call"), e_var("index")])])),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)), vec![
                                s_assign("string", e_call("elephc_ffi_callback_arg_string", vec![e_var("call"), e_var("index")])),
                                s_assign("string_len", e_call("elephc_ffi_callback_arg_string_len", vec![e_var("call"), e_var("index")])),
                                s_array_push("args", e_call("ptr_read_string", vec![e_var("string"), e_var("string_len")])),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)), vec![
                                s_array_push("args", e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_call("elephc_ffi_callback_arg_cdata", vec![e_var("call"), e_var("index")])])),
                            ]),
                        ],
                            Some(vec![
                            s_array_push("args", e_null()),
                        ]),
                        ),
                    ]),
                    s_return(e_closure_call("closure", vec![e_spread(e_var("args"))])),
                ]),
        )
        .method(
            method("__elephc_callback_invoke_int")
                .static_()
                .param("id", TypeExpr::Int)
                .param("call", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_try(vec![
                        s_assign("result", e_self_call("__elephc_callback_invoke", vec![e_var("id"), e_var("call")])),
                    ], vec![
                        (vec!["\\Throwable"], Some("error"), vec![
                            s_expr(e_self_call("__elephc_callback_fatal", vec![e_var("call"), e_var("error")])),
                            s_return(e_int(0)),
                        ]),
                    ], None),
                    s_if(
                        e_instance_of(e_var("result"), "\\FFI\\CData"),
                        vec![
                            s_return(e_call("elephc_ffi_cdata_pointer", vec![e_method_call(e_var("result"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("is_string", vec![e_var("result")]), BinOp::And, e_binop(e_call("strlen", vec![e_var("result")]), BinOp::StrictEq, e_int(1))),
                        vec![
                            s_return(e_call("ord", vec![e_var("result")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_cast(CastType::Int, e_var("result"))),
                ]),
        )
        .method(
            method("__elephc_callback_invoke_float")
                .static_()
                .param("id", TypeExpr::Int)
                .param("call", TypeExpr::Int)
                .returns(TypeExpr::Float)
                .body(vec![
                    s_try(vec![
                        s_assign("result", e_self_call("__elephc_callback_invoke", vec![e_var("id"), e_var("call")])),
                    ], vec![
                        (vec!["\\Throwable"], Some("error"), vec![
                            s_expr(e_self_call("__elephc_callback_fatal", vec![e_var("call"), e_var("error")])),
                            s_return(e_float(0.0)),
                        ]),
                    ], None),
                    s_return(e_cast(CastType::Float, e_var("result"))),
                ]),
        )
        .method(
            method("__elephc_callback_invoke_void")
                .static_()
                .param("id", TypeExpr::Int)
                .param("call", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_try(vec![
                        s_expr(e_self_call("__elephc_callback_invoke", vec![e_var("id"), e_var("call")])),
                    ], vec![
                        (vec!["\\Throwable"], Some("error"), vec![
                            s_expr(e_self_call("__elephc_callback_fatal", vec![e_var("call"), e_var("error")])),
                        ]),
                    ], None),
                ]),
        )
        .method(
            method("__elephc_callback_invoke_aggregate")
                .static_()
                .param("id", TypeExpr::Int)
                .param("call", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_try(vec![
                        s_assign("result", e_self_call("__elephc_callback_invoke", vec![e_var("id"), e_var("call")])),
                    ], vec![
                        (vec!["\\Throwable"], Some("error"), vec![
                            s_expr(e_self_call("__elephc_callback_fatal", vec![e_var("call"), e_var("error")])),
                            s_return(e_int(0)),
                        ]),
                    ], None),
                    s_if(
                        e_not(e_instance_of(e_var("result"), "\\FFI\\CData")),
                        vec![
                            s_expr(e_self_call("__elephc_callback_fatal", vec![e_var("call"), e_new_fq("TypeError", vec![e_str("Cannot call callback")])])),
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("elephc_ffi_stage_aggregate", vec![e_method_call(e_var("result"), "__elephc_raw", vec![])])),
                ]),
        )
        .method(
            method("__elephc_callback_fatal")
                .private()
                .static_()
                .param("call", TypeExpr::Int)
                .param("error", t_class("\\Throwable"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("name_len", e_call("elephc_ffi_callback_active_name_len", vec![e_var("call")])),
                    s_if(
                        e_binop(e_var("name_len"), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("name_ptr", e_call("elephc_ffi_callback_active_name", vec![e_var("call")])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("name_ptr", e_call("elephc_ffi_callback_name", vec![e_var("call")])),
                        s_assign("name_len", e_call("elephc_ffi_callback_name_len", vec![e_var("call")])),
                    ]),
                    ),
                    s_assign("name", e_call("ptr_read_string", vec![e_var("name_ptr"), e_var("name_len")])),
                    s_assign("file", e_method_call(e_var("error"), "getFile", vec![])),
                    s_assign("line", e_method_call(e_var("error"), "getLine", vec![])),
                    s_echo(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Warning: Uncaught "), BinOp::Concat, e_call("get_class", vec![e_var("error")])), BinOp::Concat, e_str(": ")), BinOp::Concat, e_method_call(e_var("error"), "getMessage", vec![])), BinOp::Concat, e_str(" in ")), BinOp::Concat, e_var("file")), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("line")), BinOp::Concat, e_str("\n"))),
                    s_echo(e_str("Stack trace:\n")),
                    s_echo(e_binop(e_binop(e_binop(e_binop(e_str("#0 "), BinOp::Concat, e_var("file")), BinOp::Concat, e_str("(")), BinOp::Concat, e_var("line")), BinOp::Concat, e_str("): {closure}()\n"))),
                    s_echo(e_binop(e_binop(e_binop(e_binop(e_str("#1 "), BinOp::Concat, e_var("file")), BinOp::Concat, e_str("(0): FFI->")), BinOp::Concat, e_var("name")), BinOp::Concat, e_str("()\n"))),
                    s_echo(e_str("#2 {main}\n")),
                    s_echo(e_binop(e_binop(e_binop(e_binop(e_str("  thrown in "), BinOp::Concat, e_var("file")), BinOp::Concat, e_str(" on line ")), BinOp::Concat, e_var("line")), BinOp::Concat, e_str("\n"))),
                    s_echo(e_str("\n")),
                    s_echo(e_binop(e_binop(e_binop(e_binop(e_str("Fatal error: Throwing from FFI callbacks is not allowed in "), BinOp::Concat, e_var("file")), BinOp::Concat, e_str(" on line ")), BinOp::Concat, e_var("line")), BinOp::Concat, e_str("\n"))),
                    s_expr(e_call("elephc_ffi_callback_abort", vec![])),
                ]),
        )
        .method(
            method("__construct")
                .private()
                .param("context", TypeExpr::Int)
                .body(vec![
                    s_prop_assign(e_this(), "__elephc_context", e_var("context")),
                ]),
        )
        .method(
            method("getIterator")
                .returns(t_class("\\Iterator"))
                .body(vec![
                    s_return(e_new_fq("ArrayIterator", vec![e_array(vec![])])),
                ]),
        )
        .method(
            method("__elephc_wrap")
                .static_()
                .param("context", TypeExpr::Int)
                .returns(t_class("self"))
                .body(vec![
                    s_return(e_new_self(vec![e_var("context")])),
                ]),
        )
        .method(
            method("__elephc_throw")
                .static_()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("kind", e_call("elephc_ffi_error_kind", vec![])),
                    s_assign("len", e_call("elephc_ffi_error_len", vec![])),
                    s_assign("message", e_call("ptr_read_string", vec![e_call("elephc_ffi_error_ptr", vec![]), e_var("len")])),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_throw(e_new_fq("FFI\\ParserException", vec![e_var("message")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_var("message")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new_fq("FFI\\Exception", vec![e_var("message")])),
                ]),
        )
        .method(
            method("cdef")
                .static_()
                .param_default("code", t_mixed(), e_null())
                .param_default("library", t_mixed(), e_null())
                .returns(t_class("FFI"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_assign("code", e_cast(CastType::String, e_null_coalesce(e_var("code"), e_str("")))),
                    s_assign("library", e_ternary(e_binop(e_var("library"), BinOp::StrictEq, e_null()), e_null(), e_cast(CastType::String, e_var("library")))),
                    s_assign("context", e_call("elephc_ffi_cdef", vec![e_var("code"), e_var("library")])),
                    s_if(
                        e_binop(e_var("context"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_assign("kind", e_call("elephc_ffi_error_kind", vec![])),
                            s_assign("len", e_call("elephc_ffi_error_len", vec![])),
                            s_assign("message", e_call("ptr_read_string", vec![e_call("elephc_ffi_error_ptr", vec![]), e_var("len")])),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                                vec![
                                    s_throw(e_new_fq("FFI\\ParserException", vec![e_var("message")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                                vec![
                                    s_throw(e_new_fq("TypeError", vec![e_var("message")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new_fq("FFI\\Exception", vec![e_var("message")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_self_call("__elephc_wrap", vec![e_var("context")])),
                ]),
        )
        .method(
            method("load")
                .static_()
                .param("filename", TypeExpr::Str)
                .returns(t_class("FFI"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_assign("context", e_call("elephc_ffi_load", vec![e_var("filename")])),
                    s_if(
                        e_binop(e_var("context"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_self_call("__elephc_wrap", vec![e_var("context")])),
                ]),
        )
        .method(
            method("scope")
                .static_()
                .param("name", TypeExpr::Str)
                .returns(t_class("FFI"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_assign("context", e_call("elephc_ffi_scope", vec![e_var("name")])),
                    s_if(
                        e_binop(e_var("context"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_self_call("__elephc_wrap", vec![e_var("context")])),
                ]),
        )
        .method(
            method("new")
                .static_()
                .param("type", t_union(vec![t_class("FFI\\CType"), TypeExpr::Str]))
                .param_default("owned", TypeExpr::Bool, e_bool(true))
                .param_default("persistent", TypeExpr::Bool, e_bool(false))
                .returns(t_class("FFI\\CData"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_assign("handle", e_call("__elephc_ffi_type_handle", vec![e_var("type")])),
                    s_assign("name", e_call("__elephc_ffi_type_name", vec![e_var("type")])),
                    s_assign("data", e_call("elephc_ffi_new_any", vec![e_int(0), e_var("name"), e_var("handle"), e_var("owned")])),
                    s_if(
                        e_binop(e_var("data"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("persistent"), BinOp::And, e_not(e_call("elephc_ffi_set_persistent", vec![e_var("data")]))),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_var("data")])),
                ]),
        )
        .method(
            method("free")
                .static_()
                .param("ptr", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CData"),
                        vec![
                            s_assign("ok", e_call("elephc_ffi_free", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("ok", e_call("elephc_ffi_last_place_free", vec![])),
                    ]),
                    ),
                    s_if(
                        e_not(e_var("ok")),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_is_function_pointer_type")
                .static_()
                .param("pointee_type", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_ffi_type_kind", vec![e_var("pointee_type")]), BinOp::StrictEq, e_int(16))),
                ]),
        )
        .method(
            method("__elephc_type_label")
                .private()
                .static_()
                .param("value", t_mixed())
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("label", e_call("gettype", vec![e_var("value")])),
                    s_if(
                        e_binop(e_var("label"), BinOp::StrictEq, e_str("NULL")),
                        vec![
                            s_return(e_str("null")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("label"), BinOp::StrictEq, e_str("object")),
                        vec![
                            s_return(e_call("get_class", vec![e_var("value")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("label")),
                ]),
        )
        .method(
            method("__elephc_cdata_raw")
                .private()
                .static_()
                .param("value", t_mixed())
                .param("method", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_instance_of(e_var("value"), "\\FFI\\CData"),
                        vec![
                            s_return(e_method_call(e_var("value"), "__elephc_raw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("FFI::"), BinOp::Concat, e_var("method")), BinOp::Concat, e_str("(): Argument #")), BinOp::Concat, e_var("argument")), BinOp::Concat, e_str(" ($")), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(") must be of type FFI\\CData, ")), BinOp::Concat, e_self_call("__elephc_type_label", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
                ]),
        )
        .method(
            method("cast")
                .static_()
                .param("type", t_mixed())
                .param("ptr", t_mixed())
                .returns(t_class("FFI\\CData"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_assign("handle", e_call("__elephc_ffi_type_handle", vec![e_var("type")])),
                    s_assign("name", e_call("__elephc_ffi_type_name", vec![e_var("type")])),
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CData"),
                        vec![
                            s_assign("data", e_call("elephc_ffi_cast_any", vec![e_int(0), e_var("name"), e_var("handle"), e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("data", e_call("elephc_ffi_last_place_cast", vec![e_int(0), e_var("name"), e_var("handle")])),
                    ]),
                    ),
                    s_if(
                        e_binop(e_var("data"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_var("data")])),
                ]),
        )
        .method(
            method("type")
                .static_()
                .param("type", TypeExpr::Str)
                .returns(t_class("FFI\\CType"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_assign("handle", e_call("elephc_ffi_type", vec![e_int(0), e_var("type")])),
                    s_if(
                        e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("\\FFI\\CType", "__elephc_wrap", vec![e_var("handle")])),
                ]),
        )
        .method(
            method("typeof")
                .static_()
                .param("ptr", t_mixed())
                .returns(t_class("FFI\\CType"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CData"),
                        vec![
                            s_assign("handle", e_call("elephc_ffi_typeof", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("handle", e_call("elephc_ffi_last_place_type", vec![])),
                    ]),
                    ),
                    s_if(
                        e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("FFI::typeof(): Argument #1 ($ptr) must be of type FFI\\CData, "), BinOp::Concat, e_self_call("__elephc_type_label", vec![e_var("ptr")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("\\FFI\\CType", "__elephc_wrap", vec![e_var("handle")])),
                ]),
        )
        .method(
            method("arrayType")
                .static_()
                .param("type", t_class("FFI\\CType"))
                .param("dimensions", t_array())
                .returns(t_class("FFI\\CType"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_assign("handle", e_method_call(e_var("type"), "__elephc_raw", vec![])),
                    s_foreach(e_var("dimensions"), None, "dimension", vec![
                        s_assign("handle", e_call("elephc_ffi_array_type", vec![e_var("handle"), e_cast(CastType::Int, e_var("dimension"))])),
                    ]),
                    s_return(e_static_call("\\FFI\\CType", "__elephc_wrap", vec![e_var("handle")])),
                ]),
        )
        .method(
            method("addr")
                .static_()
                .param("ptr", t_mixed())
                .returns(t_class("FFI\\CData"))
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CData"),
                        vec![
                            s_assign("data", e_call("elephc_ffi_addr", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                            s_if(
                                e_binop(e_var("data"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_expr(e_self_call("__elephc_throw", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("data", e_call("elephc_ffi_last_place_addr", vec![])),
                        s_if(
                            e_binop(e_var("data"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("FFI::addr(): Argument #1 ($ptr) must be of type FFI\\CData, "), BinOp::Concat, e_self_call("__elephc_type_label", vec![e_var("ptr")])), BinOp::Concat, e_str(" given"))])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    ),
                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_var("data")])),
                ]),
        )
        .method(
            method("__elephc_addr_temp")
                .static_()
                .param("ptr", t_mixed())
                .returns(t_class("\\FFI\\CData"))
                .body(vec![
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CData"),
                        vec![
                            s_assign("data", e_call("elephc_ffi_addr_owned", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                            s_if(
                                e_binop(e_var("data"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_expr(e_self_call("__elephc_throw", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_var("data")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_self_call("addr", vec![e_var("ptr")])),
                ]),
        )
        .method(
            method("__elephc_is_callable")
                .static_()
                .param("value", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_instance_of(e_var("value"), "\\FFI\\CData"),
                        vec![
                            s_return(e_call("elephc_ffi_cdata_is_callable", vec![e_method_call(e_var("value"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("sizeof")
                .static_()
                .param("ptr", t_mixed())
                .returns(TypeExpr::Int)
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CType"),
                        vec![
                            s_return(e_call("elephc_ffi_sizeof_type", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CData"),
                        vec![
                            s_return(e_call("elephc_ffi_sizeof_cdata", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("size", e_call("elephc_ffi_last_place_size", vec![])),
                    s_if(
                        e_binop(e_var("size"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("FFI::sizeof(): Argument #1 ($ptr) must be of type FFI\\CData|FFI\\CType, "), BinOp::Concat, e_self_call("__elephc_type_label", vec![e_var("ptr")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("size")),
                ]),
        )
        .method(
            method("alignof")
                .static_()
                .param("ptr", t_mixed())
                .returns(TypeExpr::Int)
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CType"),
                        vec![
                            s_return(e_call("elephc_ffi_alignof_type", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("ptr"), "\\FFI\\CData"),
                        vec![
                            s_return(e_call("elephc_ffi_alignof_cdata", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("align", e_call("elephc_ffi_last_place_align", vec![])),
                    s_if(
                        e_binop(e_var("align"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("FFI::alignof(): Argument #1 ($ptr) must be of type FFI\\CData|FFI\\CType, "), BinOp::Concat, e_self_call("__elephc_type_label", vec![e_var("ptr")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("align")),
                ]),
        )
        .method(
            method("memcpy")
                .static_()
                .param("to", t_mixed())
                .param("from", t_mixed())
                .param("size", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_not(e_instance_of(e_var("to"), "\\FFI\\CData")),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("FFI::memcpy(): Argument #1 ($to) must be of type FFI\\CData, "), BinOp::Concat, e_self_call("__elephc_type_label", vec![e_var("to")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("is_string", vec![e_var("from")]),
                        vec![
                            s_assign("ok", e_call("elephc_ffi_memcpy_string", vec![e_self_call("__elephc_cdata_raw", vec![e_var("to"), e_str("memcpy"), e_str("1"), e_str("to")]), e_var("from"), e_call("strlen", vec![e_var("from")]), e_var("size")])),
                        ],
                        vec![
                        (e_instance_of(e_var("from"), "\\FFI\\CData"), vec![
                            s_assign("ok", e_call("elephc_ffi_memcpy", vec![e_self_call("__elephc_cdata_raw", vec![e_var("to"), e_str("memcpy"), e_str("1"), e_str("to")]), e_method_call(e_var("from"), "__elephc_raw", vec![]), e_var("size")])),
                        ]),
                    ],
                        Some(vec![
                        s_assign("ok", e_call("elephc_ffi_last_place_memcpy", vec![e_self_call("__elephc_cdata_raw", vec![e_var("to"), e_str("memcpy"), e_str("1"), e_str("to")]), e_var("size")])),
                    ]),
                    ),
                    s_if(
                        e_not(e_var("ok")),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("memcmp")
                .static_()
                .param("ptr1", t_mixed())
                .param("ptr2", t_mixed())
                .param("size", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_call("is_string", vec![e_var("ptr2")]),
                        vec![
                            s_assign("left", e_ternary(e_call("is_string", vec![e_var("ptr1")]), e_int(0), e_self_call("__elephc_cdata_raw", vec![e_var("ptr1"), e_str("memcmp"), e_str("1"), e_str("ptr1")]))),
                            s_if(
                                e_binop(e_var("left"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_throw(e_new_fq("FFI\\Exception", vec![e_str("FFI::memcmp() does not accept two strings")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("elephc_ffi_memcmp_string", vec![e_var("left"), e_var("ptr2"), e_call("strlen", vec![e_var("ptr2")]), e_var("size")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("left", e_ternary(e_call("is_string", vec![e_var("ptr1")]), e_int(0), e_self_call("__elephc_cdata_raw", vec![e_var("ptr1"), e_str("memcmp"), e_str("1"), e_str("ptr1")]))),
                    s_if(
                        e_binop(e_var("left"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new_fq("FFI\\Exception", vec![e_str("FFI::memcmp() does not accept a string as the first argument")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("ptr2"), "\\FFI\\CData"),
                        vec![
                            s_return(e_call("elephc_ffi_memcmp", vec![e_var("left"), e_method_call(e_var("ptr2"), "__elephc_raw", vec![]), e_var("size")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("elephc_ffi_last_place_memcmp", vec![e_var("left"), e_var("size")])),
                ]),
        )
        .method(
            method("memset")
                .static_()
                .param("ptr", t_mixed())
                .param("value", TypeExpr::Int)
                .param("size", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_assign("target", e_ternary(e_instance_of(e_var("ptr"), "\\FFI\\CData"), e_method_call(e_var("ptr"), "__elephc_raw", vec![]), e_int(0))),
                    s_assign("ok", e_ternary(e_binop(e_var("target"), BinOp::StrictNotEq, e_int(0)), e_call("elephc_ffi_memset", vec![e_var("target"), e_var("value"), e_var("size")]), e_call("elephc_ffi_last_place_memset", vec![e_var("value"), e_var("size")]))),
                    s_if(
                        e_not(e_var("ok")),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("string")
                .static_()
                .param("ptr", t_mixed())
                .param_default("size", t_nullable(TypeExpr::Int), e_null())
                .returns(TypeExpr::Str)
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_call("is_string", vec![e_var("ptr")]),
                        vec![
                            s_return(e_var("ptr")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("has_size", e_binop(e_var("size"), BinOp::StrictNotEq, e_null())),
                    s_assign("actual", e_ternary(e_var("has_size"), e_cast(CastType::Int, e_var("size")), e_int(0))),
                    s_assign("start", e_ternary(e_instance_of(e_var("ptr"), "\\FFI\\CData"), e_call("elephc_ffi_string", vec![e_method_call(e_var("ptr"), "__elephc_raw", vec![]), e_var("actual"), e_var("has_size")]), e_call("elephc_ffi_last_place_string", vec![e_var("actual"), e_var("has_size")]))),
                    s_return(e_call("ptr_read_string", vec![e_var("start"), e_call("elephc_ffi_string_len", vec![])])),
                ]),
        )
        .method(
            method("isNull")
                .static_()
                .param("ptr", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_self_call("__elephc_validate_api", vec![])),
                    s_if(
                        e_not(e_instance_of(e_var("ptr"), "\\FFI\\CData")),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("FFI::isNull(): Argument #1 ($ptr) must be of type FFI\\CData, "), BinOp::Concat, e_self_call("__elephc_type_label", vec![e_var("ptr")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("elephc_ffi_is_null", vec![e_self_call("__elephc_cdata_raw", vec![e_var("ptr"), e_str("isNull"), e_str("1"), e_str("ptr")])])),
                ]),
        )
        .method(
            method("__elephc_global_cdata")
                .param("name", TypeExpr::Str)
                .returns(t_class("\\FFI\\CData"))
                .body(vec![
                    s_assign("kind", e_call("elephc_ffi_global_kind", vec![e_this_prop("__elephc_context"), e_var("name")])),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_assign("function", e_call("elephc_ffi_function_cdata", vec![e_this_prop("__elephc_context"), e_var("name")])),
                            s_if(
                                e_binop(e_var("function"), BinOp::StrictNotEq, e_int(0)),
                                vec![
                                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_var("function")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_call("elephc_ffi_global_cdata", vec![e_this_prop("__elephc_context"), e_var("name")])])),
                ]),
        )
        .method(
            method("__elephc_instance_new")
                .param("type", t_mixed())
                .param_default("owned", TypeExpr::Bool, e_bool(true))
                .param_default("persistent", TypeExpr::Bool, e_bool(false))
                .returns(t_class("\\FFI\\CData"))
                .body(vec![
                    s_assign("handle", e_call("__elephc_ffi_type_handle", vec![e_var("type")])),
                    s_assign("type_name", e_call("__elephc_ffi_type_name", vec![e_var("type")])),
                    s_assign("data", e_call("elephc_ffi_new_any", vec![e_this_prop("__elephc_context"), e_var("type_name"), e_var("handle"), e_var("owned")])),
                    s_if(
                        e_binop(e_var("data"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("persistent"), BinOp::And, e_not(e_call("elephc_ffi_set_persistent", vec![e_var("data")]))),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_var("data")])),
                ]),
        )
        .method(
            method("__call")
                .param("name", TypeExpr::Str)
                .param("args", t_array())
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("new")),
                        vec![
                            s_return(e_method_call(e_this(), "__elephc_instance_new", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_str("")), e_cast(CastType::Bool, e_null_coalesce(e_index(e_var("args"), e_int(1)), e_bool(true))), e_cast(CastType::Bool, e_null_coalesce(e_index(e_var("args"), e_int(2)), e_bool(false)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("type")),
                        vec![
                            s_assign("handle", e_call("elephc_ffi_type", vec![e_this_prop("__elephc_context"), e_cast(CastType::String, e_null_coalesce(e_index(e_var("args"), e_int(0)), e_str("")))])),
                            s_if(
                                e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_expr(e_self_call("__elephc_throw", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_static_call("\\FFI\\CType", "__elephc_wrap", vec![e_var("handle")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("cast")),
                        vec![
                            s_assign("type", e_null_coalesce(e_index(e_var("args"), e_int(0)), e_str(""))),
                            s_assign("ptr", e_null_coalesce(e_index(e_var("args"), e_int(1)), e_null())),
                            s_if(
                                e_not(e_instance_of(e_var("ptr"), "\\FFI\\CData")),
                                vec![
                                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_str("FFI::cast(): Argument #2 ("), BinOp::Concat, e_var("ptr")), BinOp::Concat, e_str(") must be of type FFI\\CData, ")), BinOp::Concat, e_self_call("__elephc_type_label", vec![e_var("ptr")])), BinOp::Concat, e_str(" given"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("handle", e_call("__elephc_ffi_type_handle", vec![e_var("type")])),
                            s_assign("type_name", e_call("__elephc_ffi_type_name", vec![e_var("type")])),
                            s_assign("raw", e_self_call("__elephc_cdata_raw", vec![e_var("ptr"), e_str("cast"), e_str("2"), e_str("ptr")])),
                            s_assign("data", e_call("elephc_ffi_cast_any", vec![e_this_prop("__elephc_context"), e_var("type_name"), e_var("handle"), e_var("raw")])),
                            s_if(
                                e_binop(e_var("data"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_expr(e_self_call("__elephc_throw", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_var("data")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("arrayType")),
                        vec![
                            s_assign("handle", e_call("__elephc_ffi_type_handle", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null())])),
                            s_foreach(e_null_coalesce(e_index(e_var("args"), e_int(1)), e_array(vec![])), None, "dimension", vec![
                                s_assign("handle", e_call("elephc_ffi_array_type", vec![e_var("handle"), e_cast(CastType::Int, e_var("dimension"))])),
                            ]),
                            s_return(e_static_call("\\FFI\\CType", "__elephc_wrap", vec![e_var("handle")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("sizeof")),
                        vec![
                            s_return(e_self_call("sizeof", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null())])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("alignof")),
                        vec![
                            s_return(e_self_call("alignof", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null())])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("addr")),
                        vec![
                            s_return(e_self_call("addr", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null())])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("typeof")),
                        vec![
                            s_return(e_self_call("typeof", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null())])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("string")),
                        vec![
                            s_assign("size", e_null_coalesce(e_index(e_var("args"), e_int(1)), e_null())),
                            s_return(e_self_call("string", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null()), e_ternary(e_binop(e_var("size"), BinOp::StrictEq, e_null()), e_null(), e_cast(CastType::Int, e_var("size")))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("isNull")),
                        vec![
                            s_return(e_self_call("isNull", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null())])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("memset")),
                        vec![
                            s_return(e_self_call("memset", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null()), e_cast(CastType::Int, e_null_coalesce(e_index(e_var("args"), e_int(1)), e_int(0))), e_cast(CastType::Int, e_null_coalesce(e_index(e_var("args"), e_int(2)), e_int(0)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("memcpy")),
                        vec![
                            s_return(e_self_call("memcpy", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null()), e_null_coalesce(e_index(e_var("args"), e_int(1)), e_null()), e_cast(CastType::Int, e_null_coalesce(e_index(e_var("args"), e_int(2)), e_int(0)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("memcmp")),
                        vec![
                            s_return(e_self_call("memcmp", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null()), e_null_coalesce(e_index(e_var("args"), e_int(1)), e_null()), e_cast(CastType::Int, e_null_coalesce(e_index(e_var("args"), e_int(2)), e_int(0)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("free")),
                        vec![
                            s_return(e_self_call("free", vec![e_null_coalesce(e_index(e_var("args"), e_int(0)), e_null())])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("call", e_call("elephc_ffi_call_begin", vec![e_this_prop("__elephc_context"), e_var("name")])),
                    s_if(
                        e_binop(e_var("call"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("index", e_int(0)),
                    s_foreach(e_var("args"), None, "arg", vec![
                        s_expr(e_call("__elephc_ffi_add_arg", vec![e_var("call"), e_var("index"), e_var("arg")])),
                        s_expr(e_post_inc("index")),
                    ]),
                    s_return(e_call("__elephc_ffi_finish_call", vec![e_var("call")])),
                ]),
        )
        .method(
            method("__get")
                .param("name", TypeExpr::Str)
                .returns(t_mixed())
                .body(vec![
                    s_assign("kind", e_call("elephc_ffi_global_kind", vec![e_this_prop("__elephc_context"), e_var("name")])),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_assign("function", e_call("elephc_ffi_function_cdata", vec![e_this_prop("__elephc_context"), e_var("name")])),
                            s_if(
                                e_binop(e_var("function"), BinOp::StrictNotEq, e_int(0)),
                                vec![
                                    s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_var("function")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)),
                        vec![
                            s_return(e_static_call("\\FFI\\CData", "__elephc_wrap", vec![e_call("elephc_ffi_global_cdata", vec![e_this_prop("__elephc_context"), e_var("name")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_return(e_call("chr", vec![e_call("elephc_ffi_global_char", vec![e_this_prop("__elephc_context"), e_var("name")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_return(e_call("elephc_ffi_global_bool", vec![e_this_prop("__elephc_context"), e_var("name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_return(e_call("elephc_ffi_global_float", vec![e_this_prop("__elephc_context"), e_var("name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("elephc_ffi_global_int", vec![e_this_prop("__elephc_context"), e_var("name")])),
                ]),
        )
        .method(
            method("__set")
                .param("name", TypeExpr::Str)
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("pointer_type", e_call("elephc_ffi_global_pointer_type", vec![e_this_prop("__elephc_context"), e_var("name")])),
                    s_if(
                        e_self_call("__elephc_is_function_pointer_type", vec![e_var("pointer_type")]),
                        vec![
                            s_if(
                                e_instance_of(e_var("value"), "\\Closure"),
                                vec![
                                    s_expr(e_self_call("__elephc_ensure_dispatchers", vec![])),
                                    s_assign("id", e_call("elephc_ffi_global_write_closure", vec![e_this_prop("__elephc_context"), e_var("name")])),
                                    s_if(
                                        e_binop(e_var("id"), BinOp::StrictEq, e_int(0)),
                                        vec![
                                            s_expr(e_self_call("__elephc_throw", vec![])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_expr(e_self_call("__elephc_register_callback_closure", vec![e_var("id"), e_var("value")])),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_var("value"), "\\FFI\\CData"),
                                vec![
                                    s_if(
                                        e_not(e_call("elephc_ffi_global_write_cdata", vec![e_this_prop("__elephc_context"), e_var("name"), e_method_call(e_var("value"), "__elephc_raw", vec![])])),
                                        vec![
                                            s_expr(e_self_call("__elephc_throw", vec![])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_call("is_null", vec![e_var("value")]),
                                vec![
                                    s_if(
                                        e_not(e_call("elephc_ffi_global_write_null", vec![e_this_prop("__elephc_context"), e_var("name")])),
                                        vec![
                                            s_expr(e_self_call("__elephc_throw", vec![])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to assign an invalid callback, no array or string given")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("value"), "\\Closure"),
                        vec![
                            s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to assign an invalid callback, no array or string given")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("ok", e_bool(false)),
                    s_if(
                        e_instance_of(e_var("value"), "\\FFI\\CData"),
                        vec![
                            s_assign("ok", e_call("elephc_ffi_global_write_cdata", vec![e_this_prop("__elephc_context"), e_var("name"), e_method_call(e_var("value"), "__elephc_raw", vec![])])),
                        ],
                        vec![
                        (e_call("is_null", vec![e_var("value")]), vec![
                            s_assign("ok", e_call("elephc_ffi_global_write_null", vec![e_this_prop("__elephc_context"), e_var("name")])),
                        ]),
                        (e_call("is_bool", vec![e_var("value")]), vec![
                            s_assign("ok", e_call("elephc_ffi_global_write_bool", vec![e_this_prop("__elephc_context"), e_var("name"), e_var("value")])),
                        ]),
                        (e_call("is_float", vec![e_var("value")]), vec![
                            s_assign("ok", e_call("elephc_ffi_global_write_float", vec![e_this_prop("__elephc_context"), e_var("name"), e_var("value")])),
                        ]),
                        (e_call("is_int", vec![e_var("value")]), vec![
                            s_assign("ok", e_call("elephc_ffi_global_write_int", vec![e_this_prop("__elephc_context"), e_var("name"), e_var("value")])),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_not(e_var("ok")),
                        vec![
                            s_expr(e_self_call("__elephc_throw", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .build()
}

/// `bootstrap 1` — transcribed from the PHP form.
fn decl_stmt_bootstrap_1() -> Stmt {
    s_namespace("FFI", vec![
        class("Exception")
            .extends("\\Error")
            .build(),
        class("ParserException")
            .final_()
            .extends("Exception")
            .build(),
        class("CType")
            .final_()
            .constant("TYPE_VOID", e_int(0))
            .constant("TYPE_FLOAT", e_int(1))
            .constant("TYPE_DOUBLE", e_int(2))
            .constant("TYPE_LONGDOUBLE", e_int(3))
            .constant("TYPE_UINT8", e_int(4))
            .constant("TYPE_SINT8", e_int(5))
            .constant("TYPE_UINT16", e_int(6))
            .constant("TYPE_SINT16", e_int(7))
            .constant("TYPE_UINT32", e_int(8))
            .constant("TYPE_SINT32", e_int(9))
            .constant("TYPE_UINT64", e_int(10))
            .constant("TYPE_SINT64", e_int(11))
            .constant("TYPE_ENUM", e_int(12))
            .constant("TYPE_BOOL", e_int(13))
            .constant("TYPE_CHAR", e_int(14))
            .constant("TYPE_POINTER", e_int(15))
            .constant("TYPE_FUNC", e_int(16))
            .constant("TYPE_ARRAY", e_int(17))
            .constant("TYPE_STRUCT", e_int(18))
            .constant("ATTR_CONST", e_int(1))
            .constant("ATTR_INCOMPLETE_TAG", e_int(2))
            .constant("ATTR_VARIADIC", e_int(4))
            .constant("ATTR_INCOMPLETE_ARRAY", e_int(8))
            .constant("ATTR_VLA", e_int(16))
            .constant("ATTR_UNION", e_int(32))
            .constant("ATTR_PACKED", e_int(64))
            .constant("ATTR_MS_STRUCT", e_int(128))
            .constant("ATTR_GCC_STRUCT", e_int(256))
            .constant("ABI_DEFAULT", e_int(0))
            .constant("ABI_CDECL", e_int(1))
            .constant("ABI_FASTCALL", e_int(2))
            .constant("ABI_THISCALL", e_int(3))
            .constant("ABI_STDCALL", e_int(4))
            .constant("ABI_PASCAL", e_int(5))
            .constant("ABI_REGISTER", e_int(6))
            .constant("ABI_MS", e_int(7))
            .constant("ABI_SYSV", e_int(8))
            .constant("ABI_VECTORCALL", e_int(9))
            .private_prop("__elephc_handle", TypeExpr::Int, None)
            .private_prop("__elephc_type_name", TypeExpr::Str, None)
            .private_prop("__elephc_index", TypeExpr::Int, None)
            .private_static_prop("__elephc_freed_debug_calls", TypeExpr::Int, Some(e_int(0)))
            .method(
                method("__construct")
                    .private()
                    .param("handle", TypeExpr::Int)
                    .param("type_name", TypeExpr::Str)
                    .body(vec![
                        s_prop_assign(e_this(), "__elephc_handle", e_var("handle")),
                        s_prop_assign(e_this(), "__elephc_type_name", e_var("type_name")),
                        s_prop_assign(e_this(), "__elephc_index", e_int(0)),
                    ]),
            )
            .method(
                method("__elephc_wrap")
                    .static_()
                    .param("handle", TypeExpr::Int)
                    .returns(t_class("self"))
                    .body(vec![
                        s_assign("ptr", e_call("elephc_ffi_type_name", vec![e_var("handle")])),
                        s_assign("len", e_call("elephc_ffi_type_name_len", vec![e_var("handle")])),
                        s_return(e_new_self(vec![e_var("handle"), e_binop(e_str("FFI\\CType:"), BinOp::Concat, e_call("ptr_read_string", vec![e_var("ptr"), e_var("len")]))])),
                    ]),
            )
            .method(
                method("__elephc_raw")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_this_prop("__elephc_handle")),
                    ]),
            )
            .method(
                method("getName")
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_assign("ptr", e_call("elephc_ffi_type_name", vec![e_this_prop("__elephc_handle")])),
                        s_return(e_call("ptr_read_string", vec![e_var("ptr"), e_call("elephc_ffi_type_name_len", vec![e_this_prop("__elephc_handle")])])),
                    ]),
            )
            .method(
                method("getKind")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_kind", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("getSize")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_size", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("getAlignment")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_align", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("getAttributes")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_attributes", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("getEnumKind")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_enum_kind", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("getArrayElementType")
                    .returns(t_class("CType"))
                    .body(vec![
                        s_return(e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_type_array_element", vec![e_this_prop("__elephc_handle")])])),
                    ]),
            )
            .method(
                method("getArrayLength")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_array_length", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("getPointerType")
                    .returns(t_class("CType"))
                    .body(vec![
                        s_return(e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_type_pointer_pointee", vec![e_this_prop("__elephc_handle")])])),
                    ]),
            )
            .method(
                method("getStructFieldNames")
                    .returns(t_array())
                    .body(vec![
                        s_assign("names", e_array(vec![])),
                        s_assign("count", e_call("elephc_ffi_type_struct_field_count", vec![e_this_prop("__elephc_handle")])),
                        s_for(Some(s_assign("index", e_int(0))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("index"))), vec![
                            s_assign("ptr", e_call("elephc_ffi_type_struct_field_name", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                            s_assign("len", e_call("elephc_ffi_type_struct_field_name_len", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                            s_array_push("names", e_call("ptr_read_string", vec![e_var("ptr"), e_var("len")])),
                        ]),
                        s_return(e_var("names")),
                    ]),
            )
            .method(
                method("getStructFieldOffset")
                    .param("name", TypeExpr::Str)
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_struct_field_offset", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                    ]),
            )
            .method(
                method("getStructFieldType")
                    .param("name", TypeExpr::Str)
                    .returns(t_class("CType"))
                    .body(vec![
                        s_return(e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_type_struct_field_type", vec![e_this_prop("__elephc_handle"), e_var("name")])])),
                    ]),
            )
            .method(
                method("getFuncABI")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_func_abi", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("getFuncReturnType")
                    .returns(t_class("CType"))
                    .body(vec![
                        s_return(e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_type_func_return", vec![e_this_prop("__elephc_handle")])])),
                    ]),
            )
            .method(
                method("getFuncParameterCount")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_call("elephc_ffi_type_func_param_count", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("getFuncParameterType")
                    .param("index", TypeExpr::Int)
                    .returns(t_class("CType"))
                    .body(vec![
                        s_return(e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_type_func_param", vec![e_this_prop("__elephc_handle"), e_var("index")])])),
                    ]),
            )
            .method(
                method("__debugInfo")
                    .returns(t_array())
                    .body(vec![
                        s_return(e_array(vec![])),
                    ]),
            )
            .build(),
        class("CData")
            .final_()
            .implements("\\ArrayAccess")
            .implements("\\Countable")
            .implements("\\Iterator")
            .private_prop("__elephc_handle", TypeExpr::Int, None)
            .private_prop("__elephc_type_name", TypeExpr::Str, None)
            .private_prop("__elephc_index", TypeExpr::Int, None)
            .private_prop("__elephc_iter_by_ref", TypeExpr::Bool, None)
            .method(
                method("__construct")
                    .private()
                    .param("handle", TypeExpr::Int)
                    .param("type_name", TypeExpr::Str)
                    .body(vec![
                        s_prop_assign(e_this(), "__elephc_handle", e_var("handle")),
                        s_prop_assign(e_this(), "__elephc_type_name", e_var("type_name")),
                        s_prop_assign(e_this(), "__elephc_index", e_int(0)),
                        s_prop_assign(e_this(), "__elephc_iter_by_ref", e_bool(false)),
                    ]),
            )
            .method(
                method("__elephc_set_iter_by_ref")
                    .param("by_ref", TypeExpr::Bool)
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_prop_assign(e_this(), "__elephc_iter_by_ref", e_var("by_ref")),
                    ]),
            )
            .method(
                method("__elephc_wrap")
                    .static_()
                    .param("handle", TypeExpr::Int)
                    .returns(t_class("self"))
                    .body(vec![
                        s_assign("ptr", e_call("elephc_ffi_cdata_type_name", vec![e_var("handle")])),
                        s_assign("len", e_call("elephc_ffi_cdata_type_name_len", vec![e_var("handle")])),
                        s_return(e_new_self(vec![e_var("handle"), e_binop(e_str("FFI\\CData:"), BinOp::Concat, e_call("ptr_read_string", vec![e_var("ptr"), e_var("len")]))])),
                    ]),
            )
            .method(
                method("__elephc_raw")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_this_prop("__elephc_handle")),
                    ]),
            )
            .method(
                method("__elephc_cdata")
                    .returns(t_class("CData"))
                    .body(vec![
                        s_return(e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_field_cdata", vec![e_this_prop("__elephc_handle"), e_str("cdata")])])),
                    ]),
            )
            .method(
                method("__destruct")
                    .private()
                    .body(vec![
                        s_if(
                            e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)),
                            vec![
                                s_expr(e_call("elephc_ffi_release_cdata", vec![e_this_prop("__elephc_handle")])),
                                s_prop_assign(e_this(), "__elephc_handle", e_int(0)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("__clone")
                    .body(vec![
                        s_prop_assign(e_this(), "__elephc_handle", e_call("elephc_ffi_cdata_clone", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("__get")
                    .param("name", TypeExpr::Str)
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("kind", e_call("elephc_ffi_field_kind", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("name"), BinOp::StrictEq, e_str("cdata")),
                            vec![
                                s_if(
                                    e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)),
                                    vec![
                                        s_return(e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_field_cdata", vec![e_this_prop("__elephc_handle"), e_var("name")])])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_return(e_call("\\__elephc_ffi_field_value", vec![e_this_prop("__elephc_handle"), e_var("name"), e_var("kind")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_call("\\__elephc_ffi_field_value", vec![e_this_prop("__elephc_handle"), e_var("name"), e_var("kind")])),
                    ]),
            )
            .method(
                method("__set")
                    .param("name", TypeExpr::Str)
                    .param("value", t_mixed())
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("cdata")), BinOp::And, e_instance_of(e_var("value"), "\\Closure")),
                            vec![
                                s_assign("cdata_type", e_call("elephc_ffi_typeof", vec![e_this_prop("__elephc_handle")])),
                                s_assign("pointee_type", e_call("elephc_ffi_type_pointer_pointee", vec![e_var("cdata_type")])),
                                s_if(
                                    e_binop(e_binop(e_call("elephc_ffi_type_kind", vec![e_var("cdata_type")]), BinOp::StrictEq, e_int(15)), BinOp::And, e_binop(e_call("elephc_ffi_type_kind", vec![e_var("pointee_type")]), BinOp::StrictEq, e_int(16))),
                                    vec![
                                        s_expr(e_static_call("\\FFI", "__elephc_ensure_dispatchers", vec![])),
                                        s_assign("id", e_call("elephc_ffi_cdata_write_closure", vec![e_this_prop("__elephc_handle")])),
                                        s_if(
                                            e_binop(e_var("id"), BinOp::StrictEq, e_int(0)),
                                            vec![
                                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_expr(e_static_call("\\FFI", "__elephc_register_callback_closure", vec![e_var("id"), e_var("value")])),
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("field_type", e_call("elephc_ffi_field_pointer_type", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                        s_if(
                            e_static_call("\\FFI", "__elephc_is_function_pointer_type", vec![e_var("field_type")]),
                            vec![
                                s_if(
                                    e_instance_of(e_var("value"), "\\Closure"),
                                    vec![
                                        s_expr(e_static_call("\\FFI", "__elephc_ensure_dispatchers", vec![])),
                                        s_assign("id", e_call("elephc_ffi_field_write_closure", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                                        s_if(
                                            e_binop(e_var("id"), BinOp::StrictEq, e_int(0)),
                                            vec![
                                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_expr(e_static_call("\\FFI", "__elephc_register_callback_closure", vec![e_var("id"), e_var("value")])),
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_instance_of(e_var("value"), "CData"),
                                    vec![
                                        s_if(
                                            e_not(e_call("elephc_ffi_field_write_cdata", vec![e_this_prop("__elephc_handle"), e_var("name"), e_method_call(e_var("value"), "__elephc_raw", vec![])])),
                                            vec![
                                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_call("is_null", vec![e_var("value")]),
                                    vec![
                                        s_if(
                                            e_not(e_call("elephc_ffi_field_write_null", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                                            vec![
                                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to assign an invalid callback, no array or string given")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("kind", e_call("elephc_ffi_field_kind", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_throw(e_new_fq("FFI\\Exception", vec![e_binop(e_binop(e_str("Attempt to assign field '"), BinOp::Concat, e_var("name")), BinOp::Concat, e_str("' of non C struct/union"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("ok", e_bool(false)),
                        s_if(
                            e_instance_of(e_var("value"), "CData"),
                            vec![
                                s_assign("ok", e_call("elephc_ffi_field_write_cdata", vec![e_this_prop("__elephc_handle"), e_var("name"), e_method_call(e_var("value"), "__elephc_raw", vec![])])),
                            ],
                            vec![
                            (e_call("is_null", vec![e_var("value")]), vec![
                                s_assign("ok", e_call("elephc_ffi_field_write_null", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                            ]),
                            (e_call("is_bool", vec![e_var("value")]), vec![
                                s_assign("ok", e_call("elephc_ffi_field_write_bool", vec![e_this_prop("__elephc_handle"), e_var("name"), e_var("value")])),
                            ]),
                            (e_call("is_float", vec![e_var("value")]), vec![
                                s_assign("ok", e_call("elephc_ffi_field_write_float", vec![e_this_prop("__elephc_handle"), e_var("name"), e_var("value")])),
                            ]),
                            (e_call("is_int", vec![e_var("value")]), vec![
                                s_assign("type", e_call("elephc_ffi_field_type", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                                s_if(
                                    e_binop(e_call("elephc_ffi_type_kind", vec![e_var("type")]), BinOp::StrictEq, e_int(15)),
                                    vec![
                                        s_assign("ok", e_call("elephc_ffi_field_write_pointer", vec![e_this_prop("__elephc_handle"), e_var("name"), e_var("value")])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("ok", e_call("elephc_ffi_field_write_int", vec![e_this_prop("__elephc_handle"), e_var("name"), e_var("value")])),
                                ]),
                                ),
                            ]),
                            (e_call("is_string", vec![e_var("value")]), vec![
                                s_assign("ok", e_call("elephc_ffi_field_write_string", vec![e_this_prop("__elephc_handle"), e_var("name"), e_var("value"), e_call("strlen", vec![e_var("value")])])),
                            ]),
                        ],
                            None,
                        ),
                        s_if(
                            e_not(e_var("ok")),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("offsetExists")
                    .param("_offset", t_mixed())
                    .keep_unread_params()
                    .returns(TypeExpr::Bool)
                    .body(vec![
                        s_return(e_bool(true)),
                    ]),
            )
            .method(
                method("offsetGet")
                    .param("offset", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("index", e_cast(CastType::Int, e_var("offset"))),
                        s_assign("kind", e_call("elephc_ffi_offset_kind", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_call("\\__elephc_ffi_offset_value", vec![e_this_prop("__elephc_handle"), e_var("index"), e_var("kind")])),
                    ]),
            )
            .method(
                method("offsetSet")
                    .param("offset", t_mixed())
                    .param("value", t_mixed())
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_call("is_null", vec![e_var("offset")]),
                            vec![
                                s_throw(e_new_fq("FFI\\Exception", vec![e_str("Cannot add next element to object of type FFI\\CData")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("index", e_cast(CastType::Int, e_var("offset"))),
                        s_assign("offset_type", e_call("elephc_ffi_offset_pointer_type", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                        s_if(
                            e_static_call("\\FFI", "__elephc_is_function_pointer_type", vec![e_var("offset_type")]),
                            vec![
                                s_if(
                                    e_instance_of(e_var("value"), "\\Closure"),
                                    vec![
                                        s_expr(e_static_call("\\FFI", "__elephc_ensure_dispatchers", vec![])),
                                        s_assign("id", e_call("elephc_ffi_offset_write_closure", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                                        s_if(
                                            e_binop(e_var("id"), BinOp::StrictEq, e_int(0)),
                                            vec![
                                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_expr(e_static_call("\\FFI", "__elephc_register_callback_closure", vec![e_var("id"), e_var("value")])),
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_instance_of(e_var("value"), "CData"),
                                    vec![
                                        s_if(
                                            e_not(e_call("elephc_ffi_offset_write_cdata", vec![e_this_prop("__elephc_handle"), e_var("index"), e_method_call(e_var("value"), "__elephc_raw", vec![])])),
                                            vec![
                                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_call("is_null", vec![e_var("value")]),
                                    vec![
                                        s_if(
                                            e_not(e_call("elephc_ffi_offset_write_null", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                                            vec![
                                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to assign an invalid callback, no array or string given")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_instance_of(e_var("value"), "\\Closure"),
                            vec![
                                s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to assign an invalid callback, no array or string given")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("kind", e_call("elephc_ffi_offset_kind", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("ok", e_bool(false)),
                        s_if(
                            e_instance_of(e_var("value"), "CData"),
                            vec![
                                s_assign("ok", e_call("elephc_ffi_offset_write_cdata", vec![e_this_prop("__elephc_handle"), e_var("index"), e_method_call(e_var("value"), "__elephc_raw", vec![])])),
                            ],
                            vec![
                            (e_call("is_null", vec![e_var("value")]), vec![
                                s_assign("ok", e_call("elephc_ffi_offset_write_null", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                            ]),
                            (e_call("is_bool", vec![e_var("value")]), vec![
                                s_assign("ok", e_call("elephc_ffi_offset_write_bool", vec![e_this_prop("__elephc_handle"), e_var("index"), e_var("value")])),
                            ]),
                            (e_call("is_float", vec![e_var("value")]), vec![
                                s_assign("ok", e_call("elephc_ffi_offset_write_float", vec![e_this_prop("__elephc_handle"), e_var("index"), e_var("value")])),
                            ]),
                            (e_call("is_int", vec![e_var("value")]), vec![
                                s_assign("type", e_call("elephc_ffi_offset_type", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                                s_if(
                                    e_binop(e_call("elephc_ffi_type_kind", vec![e_var("type")]), BinOp::StrictEq, e_int(15)),
                                    vec![
                                        s_assign("ok", e_call("elephc_ffi_offset_write_pointer", vec![e_this_prop("__elephc_handle"), e_var("index"), e_var("value")])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("ok", e_call("elephc_ffi_offset_write_int", vec![e_this_prop("__elephc_handle"), e_var("index"), e_var("value")])),
                                ]),
                                ),
                            ]),
                            (e_call("is_string", vec![e_var("value")]), vec![
                                s_assign("ok", e_call("elephc_ffi_offset_write_string", vec![e_this_prop("__elephc_handle"), e_var("index"), e_var("value"), e_call("strlen", vec![e_var("value")])])),
                            ]),
                        ],
                            None,
                        ),
                        s_if(
                            e_not(e_var("ok")),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("offsetUnset")
                    .param("_offset", t_mixed())
                    .keep_unread_params()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_throw(e_new_fq("FFI\\Exception", vec![e_str("Cannot unset C array element")])),
                    ]),
            )
            .method(
                method("count")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_assign("type", e_call("elephc_ffi_typeof", vec![e_this_prop("__elephc_handle")])),
                        s_if(
                            e_binop(e_call("elephc_ffi_type_kind", vec![e_var("type")]), BinOp::StrictNotEq, e_int(17)),
                            vec![
                                s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to count() on non C array")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_call("elephc_ffi_array_length", vec![e_this_prop("__elephc_handle")])),
                    ]),
            )
            .method(
                method("current")
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("type", e_call("elephc_ffi_typeof", vec![e_this_prop("__elephc_handle")])),
                        s_if(
                            e_binop(e_call("elephc_ffi_type_kind", vec![e_var("type")]), BinOp::StrictNotEq, e_int(17)),
                            vec![
                                s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to iterate on non C array")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_this_prop("__elephc_iter_by_ref"),
                            vec![
                                s_return(e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_iter_cdata", vec![e_this_prop("__elephc_handle"), e_this_prop("__elephc_index")])])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("kind", e_call("elephc_ffi_offset_kind", vec![e_this_prop("__elephc_handle"), e_this_prop("__elephc_index")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_call("\\__elephc_ffi_offset_value", vec![e_this_prop("__elephc_handle"), e_this_prop("__elephc_index"), e_var("kind")])),
                    ]),
            )
            .method(
                method("key")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_this_prop("__elephc_index")),
                    ]),
            )
            .method(
                method("next")
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_prop_assign(e_this(), "__elephc_index", e_binop(e_this_prop("__elephc_index"), BinOp::Add, e_int(1))),
                    ]),
            )
            .method(
                method("rewind")
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_prop_assign(e_this(), "__elephc_index", e_int(0)),
                    ]),
            )
            .method(
                method("valid")
                    .returns(TypeExpr::Bool)
                    .body(vec![
                        s_assign("type", e_call("elephc_ffi_typeof", vec![e_this_prop("__elephc_handle")])),
                        s_if(
                            e_binop(e_call("elephc_ffi_type_kind", vec![e_var("type")]), BinOp::StrictNotEq, e_int(17)),
                            vec![
                                s_throw(e_new_fq("FFI\\Exception", vec![e_str("Attempt to iterate on non C array")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_binop(e_this_prop("__elephc_index"), BinOp::Lt, e_call("elephc_ffi_array_length", vec![e_this_prop("__elephc_handle")]))),
                    ]),
            )
            .method(
                method("__toString")
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_assign("kind", e_call("elephc_ffi_field_kind", vec![e_this_prop("__elephc_handle"), e_str("cdata")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_return(e_cast(CastType::String, e_call("elephc_ffi_field_int", vec![e_this_prop("__elephc_handle"), e_str("cdata")]))),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                            vec![
                                s_return(e_cast(CastType::String, e_call("elephc_ffi_field_float", vec![e_this_prop("__elephc_handle"), e_str("cdata")]))),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                            vec![
                                s_return(e_ternary(e_call("elephc_ffi_field_bool", vec![e_this_prop("__elephc_handle"), e_str("cdata")]), e_str("1"), e_str(""))),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)),
                            vec![
                                s_return(e_call("chr", vec![e_call("elephc_ffi_field_char", vec![e_this_prop("__elephc_handle"), e_str("cdata")])])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)),
                            vec![
                                s_return(e_str("")),
                            ],
                            vec![],
                            None,
                        ),
                        s_throw(e_new_fq("Error", vec![e_str("Object of class FFI\\CData could not be converted to string")])),
                    ]),
            )
            .method(
                method("__debugInfo")
                    .returns(t_mixed())
                    .body(vec![
                        s_if(
                            e_call("elephc_ffi_is_freed", vec![e_this_prop("__elephc_handle")]),
                            vec![
                                s_return(e_method_call(e_this(), "__elephc_debug_freed", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("type", e_call("elephc_ffi_typeof", vec![e_this_prop("__elephc_handle")])),
                        s_assign("kind", e_call("elephc_ffi_type_kind", vec![e_var("type")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(18)),
                            vec![
                                s_return(e_method_call(e_this(), "__elephc_debug_struct", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(17)),
                            vec![
                                s_return(e_method_call(e_this(), "__elephc_debug_array", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(15)),
                            vec![
                                s_return(e_method_call(e_this(), "__elephc_debug_pointer", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(16)),
                            vec![
                                s_return(e_array(vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_method_call(e_this(), "__elephc_debug_scalar", vec![])),
                    ]),
            )
            .method(
                method("__elephc_debug_freed")
                    .private()
                    .returns(t_mixed())
                    .body(vec![
                        s_echo(e_binop(e_binop(e_binop(e_binop(e_str("object("), BinOp::Concat, e_this_prop("__elephc_type_name")), BinOp::Concat, e_str(")#")), BinOp::Concat, e_call("spl_object_id", vec![e_this()])), BinOp::Concat, e_str(" (0) {\n}\n"))),
                        s_throw(e_new_fq("FFI\\Exception", vec![e_str("Use after free()")])),
                    ]),
            )
            .method(
                method("__elephc_debug_struct")
                    .private()
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("fields", e_array(vec![])),
                        s_assign("count", e_call("elephc_ffi_field_count", vec![e_this_prop("__elephc_handle")])),
                        s_for(Some(s_assign("index", e_int(0))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("index"))), vec![
                            s_assign("ptr", e_call("elephc_ffi_field_name_at", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                            s_assign("len", e_call("elephc_ffi_field_name_at_len", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                            s_assign("name", e_call("ptr_read_string", vec![e_var("ptr"), e_var("len")])),
                            s_assign("field_kind", e_call("elephc_ffi_field_kind", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                            s_assign("value", e_call("\\__elephc_ffi_field_value", vec![e_this_prop("__elephc_handle"), e_var("name"), e_var("field_kind")])),
                            s_if(
                                e_binop(e_var("field_kind"), BinOp::StrictEq, e_int(5)),
                                vec![
                                    s_assign("field_type", e_call("elephc_ffi_field_type", vec![e_this_prop("__elephc_handle"), e_var("name")])),
                                    s_if(
                                        e_binop(e_call("elephc_ffi_type_kind", vec![e_var("field_type")]), BinOp::StrictEq, e_int(15)),
                                        vec![
                                            s_assign("pointer", e_call("elephc_ffi_pointer_value", vec![e_method_call(e_var("value"), "__elephc_raw", vec![])])),
                                            s_if(
                                                e_binop(e_var("pointer"), BinOp::Lt, e_int(4096)),
                                                vec![
                                                    s_assign("value", e_call("sprintf", vec![e_str("0x%x"), e_var("pointer")])),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_array_assign("fields", e_var("name"), e_var("value")),
                        ]),
                        s_return(e_var("fields")),
                    ]),
            )
            .method(
                method("__elephc_debug_array")
                    .private()
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("items", e_array(vec![])),
                        s_assign("count", e_call("elephc_ffi_array_length", vec![e_this_prop("__elephc_handle")])),
                        s_for(Some(s_assign("index", e_int(0))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("index"))), vec![
                            s_assign("element_kind", e_call("elephc_ffi_offset_kind", vec![e_this_prop("__elephc_handle"), e_var("index")])),
                            s_array_assign("items", e_cast(CastType::String, e_var("index")), e_call("\\__elephc_ffi_offset_value", vec![e_this_prop("__elephc_handle"), e_var("index"), e_var("element_kind")])),
                        ]),
                        s_return(e_var("items")),
                    ]),
            )
            .method(
                method("__elephc_debug_pointer")
                    .private()
                    .returns(t_mixed())
                    .body(vec![
                        s_if(
                            e_call("elephc_ffi_is_null", vec![e_this_prop("__elephc_handle")]),
                            vec![
                                s_return(e_array_assoc(vec![(e_str("0"), e_null())])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("element_kind", e_call("elephc_ffi_offset_kind", vec![e_this_prop("__elephc_handle"), e_int(0)])),
                        s_assign("pointer", e_call("elephc_ffi_pointer_value", vec![e_this_prop("__elephc_handle")])),
                        s_if(
                            e_binop(e_binop(e_var("element_kind"), BinOp::StrictEq, e_int(4)), BinOp::And, e_binop(e_var("pointer"), BinOp::Lt, e_int(4096))),
                            vec![
                                s_return(e_array_assoc(vec![(e_str("0"), e_call("sprintf", vec![e_str("0x%x"), e_var("pointer")]))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("element_kind"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_return(e_array_assoc(vec![(e_str("0"), e_self_call("__elephc_wrap", vec![e_call("elephc_ffi_offset_cdata", vec![e_this_prop("__elephc_handle"), e_int(0)])]))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_array_assoc(vec![(e_str("0"), e_call("\\__elephc_ffi_offset_value", vec![e_this_prop("__elephc_handle"), e_int(0), e_var("element_kind")]))])),
                    ]),
            )
            .method(
                method("__elephc_debug_scalar")
                    .private()
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("scalar_kind", e_call("elephc_ffi_field_kind", vec![e_this_prop("__elephc_handle"), e_str("cdata")])),
                        s_return(e_array_assoc(vec![(e_str("cdata"), e_call("\\__elephc_ffi_field_value", vec![e_this_prop("__elephc_handle"), e_str("cdata"), e_var("scalar_kind")]))])),
                    ]),
            )
            .method(
                method("__serialize")
                    .returns(t_array())
                    .body(vec![
                        s_throw(e_new_fq("Exception", vec![e_str("Serialization of 'FFI\\CData' is not allowed")])),
                    ]),
            )
            .method(
                method("__unserialize")
                    .param("_data", t_array())
                    .keep_unread_params()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_throw(e_new_fq("Exception", vec![e_str("Unserialization of 'FFI\\CData' is not allowed")])),
                    ]),
            )
            .method(
                method("__elephc_add")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_class("CData"))
                    .body(vec![
                        s_assign("left_value", e_call("\\__elephc_ffi_operand_value", vec![e_var("left")])),
                        s_assign("left_kind", e_call("\\__elephc_ffi_operand_kind", vec![e_var("left")])),
                        s_assign("right_value", e_call("\\__elephc_ffi_operand_value", vec![e_var("right")])),
                        s_assign("right_kind", e_call("\\__elephc_ffi_operand_kind", vec![e_var("right")])),
                        s_assign("kind", e_call("elephc_ffi_binary_kind", vec![e_var("left_value"), e_var("left_kind"), e_var("right_value"), e_var("right_kind"), e_int(1)])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(2)),
                            vec![
                                s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("+")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("handle", e_call("elephc_ffi_binary_cdata", vec![e_var("left_value"), e_var("left_kind"), e_var("right_value"), e_var("right_kind"), e_int(1)])),
                        s_if(
                            e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_self_call("__elephc_wrap", vec![e_var("handle")])),
                    ]),
            )
            .method(
                method("__elephc_sub")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("left_value", e_call("\\__elephc_ffi_operand_value", vec![e_var("left")])),
                        s_assign("left_kind", e_call("\\__elephc_ffi_operand_kind", vec![e_var("left")])),
                        s_assign("right_value", e_call("\\__elephc_ffi_operand_value", vec![e_var("right")])),
                        s_assign("right_kind", e_call("\\__elephc_ffi_operand_kind", vec![e_var("right")])),
                        s_assign("kind", e_call("elephc_ffi_binary_kind", vec![e_var("left_value"), e_var("left_kind"), e_var("right_value"), e_var("right_kind"), e_int(2)])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                            vec![
                                s_assign("handle", e_call("elephc_ffi_binary_cdata", vec![e_var("left_value"), e_var("left_kind"), e_var("right_value"), e_var("right_kind"), e_int(2)])),
                                s_if(
                                    e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                                    vec![
                                        s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_return(e_self_call("__elephc_wrap", vec![e_var("handle")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_return(e_call("elephc_ffi_binary_int", vec![e_var("left_value"), e_var("left_kind"), e_var("right_value"), e_var("right_kind"), e_int(2)])),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("-")])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_mul")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("*"), e_bool(true)])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_div")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("/")])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_mod")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("%")])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_pow")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("**")])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_bitand")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("&"), e_bool(true)])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_bitor")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("|"), e_bool(true)])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_bitxor")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("^"), e_bool(true)])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_shl")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str("<<")])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_shr")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_call("\\__elephc_ffi_unsupported_operands", vec![e_var("left"), e_var("right"), e_str(">>")])),
                        s_return(e_null()),
                    ]),
            )
            .method(
                method("__elephc_compare")
                    .static_()
                    .param("left", t_mixed())
                    .param("right", t_mixed())
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_assign("left_value", e_call("\\__elephc_ffi_operand_value", vec![e_var("left")])),
                        s_assign("left_kind", e_call("\\__elephc_ffi_operand_kind", vec![e_var("left")])),
                        s_assign("right_value", e_call("\\__elephc_ffi_operand_value", vec![e_var("right")])),
                        s_assign("right_kind", e_call("\\__elephc_ffi_operand_kind", vec![e_var("right")])),
                        s_expr(e_call("elephc_ffi_clear_error", vec![])),
                        s_assign("result", e_call("elephc_ffi_binary_compare", vec![e_var("left_value"), e_var("left_kind"), e_var("right_value"), e_var("right_kind")])),
                        s_if(
                            e_binop(e_call("elephc_ffi_error_kind", vec![]), BinOp::StrictNotEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_var("result")),
                    ]),
            )
            .method(
                method("__elephc_bool")
                    .returns(TypeExpr::Bool)
                    .body(vec![
                        s_return(e_bool(true)),
                    ]),
            )
            .method(
                method("append")
                    .param("_value", t_mixed())
                    .keep_unread_params()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_throw(e_new_fq("FFI\\Exception", vec![e_str("Cannot add next element to object of type FFI\\CData")])),
                    ]),
            )
            .method(
                method("__elephc_invoke_array")
                    .param("args", t_array())
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("call", e_call("elephc_ffi_call_begin_cdata", vec![e_this_prop("__elephc_handle")])),
                        s_if(
                            e_binop(e_var("call"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("index", e_int(0)),
                        s_foreach(e_var("args"), None, "arg", vec![
                            s_expr(e_call("\\__elephc_ffi_add_arg", vec![e_var("call"), e_var("index"), e_var("arg")])),
                            s_expr(e_post_inc("index")),
                        ]),
                        s_return(e_call("\\__elephc_ffi_finish_call", vec![e_var("call")])),
                    ]),
            )
            .method(
                method("__invoke")
                    .param_default("arg0", t_mixed(), e_str("\0__elephc_ffi_unset__"))
                    .param_default("arg1", t_mixed(), e_str("\0__elephc_ffi_unset__"))
                    .param_default("arg2", t_mixed(), e_str("\0__elephc_ffi_unset__"))
                    .param_default("arg3", t_mixed(), e_str("\0__elephc_ffi_unset__"))
                    .param_default("arg4", t_mixed(), e_str("\0__elephc_ffi_unset__"))
                    .param_default("arg5", t_mixed(), e_str("\0__elephc_ffi_unset__"))
                    .variadic("rest", Some(t_mixed()))
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("args", e_array(vec![])),
                        s_if(
                            e_binop(e_var("arg0"), BinOp::StrictNotEq, e_str("\0__elephc_ffi_unset__")),
                            vec![
                                s_array_push("args", e_var("arg0")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("arg1"), BinOp::StrictNotEq, e_str("\0__elephc_ffi_unset__")),
                            vec![
                                s_array_push("args", e_var("arg1")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("arg2"), BinOp::StrictNotEq, e_str("\0__elephc_ffi_unset__")),
                            vec![
                                s_array_push("args", e_var("arg2")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("arg3"), BinOp::StrictNotEq, e_str("\0__elephc_ffi_unset__")),
                            vec![
                                s_array_push("args", e_var("arg3")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("arg4"), BinOp::StrictNotEq, e_str("\0__elephc_ffi_unset__")),
                            vec![
                                s_array_push("args", e_var("arg4")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("arg5"), BinOp::StrictNotEq, e_str("\0__elephc_ffi_unset__")),
                            vec![
                                s_array_push("args", e_var("arg5")),
                            ],
                            vec![],
                            None,
                        ),
                        s_foreach(e_var("rest"), None, "extra", vec![
                            s_array_push("args", e_var("extra")),
                        ]),
                        s_assign("address", e_call("elephc_ffi_cdata_address", vec![e_this_prop("__elephc_handle")])),
                        s_if(
                            e_static_call("\\FFI", "__elephc_has_closure", vec![e_var("address")]),
                            vec![
                                s_assign("closure", e_static_call("\\FFI", "__elephc_closure", vec![e_var("address")])),
                                s_return(e_closure_call("closure", vec![e_spread(e_var("args"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("call", e_call("elephc_ffi_call_begin_cdata", vec![e_this_prop("__elephc_handle")])),
                        s_if(
                            e_binop(e_var("call"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("\\FFI", "__elephc_throw", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("index", e_int(0)),
                        s_foreach(e_var("args"), None, "arg", vec![
                            s_expr(e_call("\\__elephc_ffi_add_arg", vec![e_var("call"), e_var("index"), e_var("arg")])),
                            s_expr(e_post_inc("index")),
                        ]),
                        s_return(e_call("\\__elephc_ffi_finish_call", vec![e_var("call")])),
                    ]),
            )
            .build()
    ])
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn ffi_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_extern_elephc_ffi_cdef(),
            decl_extern_elephc_ffi_load(),
            decl_extern_elephc_ffi_scope(),
            decl_extern_elephc_ffi_error_kind(),
            decl_extern_elephc_ffi_error_ptr(),
            decl_extern_elephc_ffi_error_len(),
            decl_extern_elephc_ffi_clear_error(),
            decl_extern_elephc_ffi_type(),
            decl_extern_elephc_ffi_type_name(),
            decl_extern_elephc_ffi_type_name_len(),
            decl_extern_elephc_ffi_type_kind(),
            decl_extern_elephc_ffi_type_size(),
            decl_extern_elephc_ffi_type_align(),
            decl_extern_elephc_ffi_type_attributes(),
            decl_extern_elephc_ffi_type_enum_kind(),
            decl_extern_elephc_ffi_type_array_element(),
            decl_extern_elephc_ffi_type_array_length(),
            decl_extern_elephc_ffi_type_pointer_to(),
            decl_extern_elephc_ffi_type_pointer_pointee(),
            decl_extern_elephc_ffi_array_type(),
            decl_extern_elephc_ffi_type_struct_field_count(),
            decl_extern_elephc_ffi_type_struct_field_name(),
            decl_extern_elephc_ffi_type_struct_field_name_len(),
            decl_extern_elephc_ffi_type_struct_field_offset(),
            decl_extern_elephc_ffi_type_struct_field_type(),
            decl_extern_elephc_ffi_type_func_return(),
            decl_extern_elephc_ffi_type_func_param_count(),
            decl_extern_elephc_ffi_type_func_param(),
            decl_extern_elephc_ffi_type_func_abi(),
            decl_extern_elephc_ffi_new(),
            decl_extern_elephc_ffi_new_type(),
            decl_extern_elephc_ffi_new_any(),
            decl_extern_elephc_ffi_set_persistent(),
            decl_extern_elephc_ffi_cast(),
            decl_extern_elephc_ffi_cast_name(),
            decl_extern_elephc_ffi_cast_any(),
            decl_extern_elephc_ffi_typeof(),
            decl_extern_elephc_ffi_cdata_type_name(),
            decl_extern_elephc_ffi_cdata_type_name_len(),
            decl_extern_elephc_ffi_sizeof_cdata(),
            decl_extern_elephc_ffi_alignof_cdata(),
            decl_extern_elephc_ffi_sizeof_type(),
            decl_extern_elephc_ffi_alignof_type(),
            decl_extern_elephc_ffi_addr(),
            decl_extern_elephc_ffi_addr_owned(),
            decl_extern_elephc_ffi_cdata_is_callable(),
            decl_extern_elephc_ffi_free(),
            decl_extern_elephc_ffi_release_cdata(),
            decl_extern_elephc_ffi_cdata_clone(),
            decl_extern_elephc_ffi_is_null(),
            decl_extern_elephc_ffi_is_freed(),
            decl_extern_elephc_ffi_pointer_value(),
            decl_extern_elephc_ffi_memcpy(),
            decl_extern_elephc_ffi_memcpy_string(),
            decl_extern_elephc_ffi_memcmp(),
            decl_extern_elephc_ffi_memcmp_string(),
            decl_extern_elephc_ffi_memset(),
            decl_extern_elephc_ffi_string(),
            decl_extern_elephc_ffi_string_len(),
            decl_extern_elephc_ffi_field_kind(),
            decl_extern_elephc_ffi_field_int(),
            decl_extern_elephc_ffi_field_float(),
            decl_extern_elephc_ffi_field_bool(),
            decl_extern_elephc_ffi_field_char(),
            decl_extern_elephc_ffi_field_cdata(),
            decl_extern_elephc_ffi_field_pointer_type(),
            decl_extern_elephc_ffi_field_is_null(),
            decl_extern_elephc_ffi_field_addr(),
            decl_extern_elephc_ffi_field_type(),
            decl_extern_elephc_ffi_field_size(),
            decl_extern_elephc_ffi_field_write_int(),
            decl_extern_elephc_ffi_field_write_float(),
            decl_extern_elephc_ffi_field_write_bool(),
            decl_extern_elephc_ffi_field_write_char(),
            decl_extern_elephc_ffi_field_write_cdata(),
            decl_extern_elephc_ffi_field_write_pointer(),
            decl_extern_elephc_ffi_field_write_null(),
            decl_extern_elephc_ffi_field_write_string(),
            decl_extern_elephc_ffi_field_count(),
            decl_extern_elephc_ffi_field_name_at(),
            decl_extern_elephc_ffi_field_name_at_len(),
            decl_extern_elephc_ffi_offset_kind(),
            decl_extern_elephc_ffi_offset_int(),
            decl_extern_elephc_ffi_offset_float(),
            decl_extern_elephc_ffi_offset_bool(),
            decl_extern_elephc_ffi_offset_char(),
            decl_extern_elephc_ffi_offset_cdata(),
            decl_extern_elephc_ffi_offset_pointer_type(),
            decl_extern_elephc_ffi_offset_is_null(),
            decl_extern_elephc_ffi_offset_addr(),
            decl_extern_elephc_ffi_offset_type(),
            decl_extern_elephc_ffi_offset_size(),
            decl_extern_elephc_ffi_offset_write_int(),
            decl_extern_elephc_ffi_offset_write_float(),
            decl_extern_elephc_ffi_offset_write_bool(),
            decl_extern_elephc_ffi_offset_write_char(),
            decl_extern_elephc_ffi_offset_write_cdata(),
            decl_extern_elephc_ffi_offset_write_pointer(),
            decl_extern_elephc_ffi_offset_write_null(),
            decl_extern_elephc_ffi_offset_write_string(),
            decl_extern_elephc_ffi_array_length(),
            decl_extern_elephc_ffi_iter_cdata(),
            decl_extern_elephc_ffi_free_offset(),
            decl_extern_elephc_ffi_free_field(),
            decl_extern_elephc_ffi_offset_string(),
            decl_extern_elephc_ffi_field_string(),
            decl_extern_elephc_ffi_memset_offset(),
            decl_extern_elephc_ffi_memset_field(),
            decl_extern_elephc_ffi_memcpy_offset(),
            decl_extern_elephc_ffi_memcpy_field(),
            decl_extern_elephc_ffi_global_kind(),
            decl_extern_elephc_ffi_global_int(),
            decl_extern_elephc_ffi_global_float(),
            decl_extern_elephc_ffi_global_bool(),
            decl_extern_elephc_ffi_global_char(),
            decl_extern_elephc_ffi_global_cdata(),
            decl_extern_elephc_ffi_global_string(),
            decl_extern_elephc_ffi_global_write_int(),
            decl_extern_elephc_ffi_global_write_float(),
            decl_extern_elephc_ffi_global_write_bool(),
            decl_extern_elephc_ffi_global_write_cdata(),
            decl_extern_elephc_ffi_global_write_pointer(),
            decl_extern_elephc_ffi_global_write_null(),
            decl_extern_elephc_ffi_cdata_address(),
            decl_extern_elephc_ffi_offset_address(),
            decl_extern_elephc_ffi_global_address(),
            decl_extern_elephc_ffi_global_pointer_type(),
            decl_extern_elephc_ffi_cdata_pointer(),
            decl_extern_elephc_ffi_global_write_closure(),
            decl_extern_elephc_ffi_field_write_closure(),
            decl_extern_elephc_ffi_offset_write_closure(),
            decl_extern_elephc_ffi_cdata_write_closure(),
            decl_extern_elephc_ffi_set_dispatcher_int(),
            decl_extern_elephc_ffi_set_dispatcher_float(),
            decl_extern_elephc_ffi_set_dispatcher_void(),
            decl_extern_elephc_ffi_set_dispatcher_aggregate(),
            decl_extern_elephc_ffi_stage_aggregate(),
            decl_extern_elephc_ffi_callback_arg_count(),
            decl_extern_elephc_ffi_callback_arg_kind(),
            decl_extern_elephc_ffi_callback_arg_int(),
            decl_extern_elephc_ffi_callback_arg_float(),
            decl_extern_elephc_ffi_callback_arg_string(),
            decl_extern_elephc_ffi_callback_arg_string_len(),
            decl_extern_elephc_ffi_callback_arg_cdata(),
            decl_extern_elephc_ffi_callback_name(),
            decl_extern_elephc_ffi_callback_name_len(),
            decl_extern_elephc_ffi_callback_active_name(),
            decl_extern_elephc_ffi_callback_active_name_len(),
            decl_extern_elephc_ffi_callback_abort(),
            decl_extern_elephc_ffi_function_cdata(),
            decl_extern_elephc_ffi_call_begin(),
            decl_extern_elephc_ffi_call_begin_cdata(),
            decl_extern_elephc_ffi_call_arg_int(),
            decl_extern_elephc_ffi_call_arg_float(),
            decl_extern_elephc_ffi_call_arg_string(),
            decl_extern_elephc_ffi_call_arg_cdata(),
            decl_extern_elephc_ffi_call_param_is_function_pointer(),
            decl_extern_elephc_ffi_call_arg_callback(),
            decl_extern_elephc_ffi_call_arg_pointer(),
            decl_extern_elephc_ffi_callback_code(),
            decl_extern_elephc_ffi_call_arg_null(),
            decl_extern_elephc_ffi_call_arg_bool(),
            decl_extern_elephc_ffi_call_invoke(),
            decl_extern_elephc_ffi_call_result_int(),
            decl_extern_elephc_ffi_call_result_float(),
            decl_extern_elephc_ffi_call_result_bool(),
            decl_extern_elephc_ffi_call_result_cdata(),
            decl_extern_elephc_ffi_call_result_string(),
            decl_extern_elephc_ffi_call_result_string_len(),
            decl_extern_elephc_ffi_call_release(),
            decl_extern_elephc_ffi_binary_kind(),
            decl_extern_elephc_ffi_binary_int(),
            decl_extern_elephc_ffi_binary_cdata(),
            decl_extern_elephc_ffi_binary_compare(),
            decl_extern_elephc_ffi_last_place_type(),
            decl_extern_elephc_ffi_last_place_addr(),
            decl_extern_elephc_ffi_last_place_size(),
            decl_extern_elephc_ffi_last_place_align(),
            decl_extern_elephc_ffi_last_place_string(),
            decl_extern_elephc_ffi_last_place_memset(),
            decl_extern_elephc_ffi_last_place_free(),
            decl_extern_elephc_ffi_last_place_memcmp(),
            decl_extern_elephc_ffi_last_place_memcpy(),
            decl_extern_elephc_ffi_last_place_cast(),
            decl_fn_elephc_ffi_type_handle(),
            decl_fn_elephc_ffi_type_name(),
            decl_fn_elephc_ffi_field_value(),
            decl_fn_elephc_ffi_offset_value(),
            decl_fn_elephc_ffi_add_arg(),
            decl_fn_elephc_ffi_operand_value(),
            decl_fn_elephc_ffi_operand_kind(),
            decl_fn_elephc_ffi_operand_type(),
            decl_fn_elephc_ffi_unsupported_operands(),
            decl_fn_elephc_ffi_finish_call(),
            decl_fn_elephc_ffi_callback_dispatch_int(),
            decl_fn_elephc_ffi_callback_dispatch_float(),
            decl_fn_elephc_ffi_callback_dispatch_void(),
            decl_fn_elephc_ffi_callback_dispatch_aggregate(),
            decl_class_ffi(),
            decl_stmt_bootstrap_1(),
        ]
    })
}
