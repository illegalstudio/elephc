//! Purpose:
//! The ABI functions defined in C (`src/c/variadic.c`) rather than Rust: the
//! variadic ones, which stable Rust cannot define, and the bailout frame,
//! which needs `sigsetjmp`.
//!
//! Called from:
//! - `build.rs`, which declares every function here in the generated header;
//!   the engine calls a few of them (`elephc_zend_protected_call`, the
//!   formatting helpers).
//!
//! Key details:
//! - The C side formats the message and hands it to a Rust entry point
//!   (`crate::error::elephc_zend_report` and friends), so behaviour lives in
//!   Rust and C only unpacks `...`.

#![allow(non_camel_case_types, improper_ctypes)]

use std::ffi::{c_char, c_int, c_void};

use super::engine::*;
use super::runtime::php_stream_wrapper;
use super::scalars::*;
use super::values::*;

extern "C" {
    // Errors and warnings.
    /// `zend_error`: printf-formats the message and reports a diagnostic of level `type` (warning,
    /// notice, deprecation or fatal) through the Rust error reporter; a fatal level bails out.
    pub fn zend_error(r#type: c_int, format: *const c_char, ...);
    /// `zend_error_unchecked`: same as `zend_error`; the variant without compiler format checking.
    pub fn zend_error_unchecked(r#type: c_int, format: *const c_char, ...);
    /// `zend_error_at`: like `zend_error` with an explicit file name and line number, which are
    /// ignored.
    pub fn zend_error_at(r#type: c_int, filename: *mut zend_string, lineno: u32, format: *const c_char, ...);
    /// `zend_error_noreturn`: formats and reports a diagnostic and never returns; a non-fatal level
    /// is promoted to `E_ERROR` so the call bails out.
    pub fn zend_error_noreturn(r#type: c_int, format: *const c_char, ...) -> !;
    /// `zend_error_at_noreturn`: `zend_error_noreturn` with an explicit file name and line number,
    /// which are ignored.
    pub fn zend_error_at_noreturn(r#type: c_int, filename: *mut zend_string, lineno: u32, format: *const c_char, ...) -> !;
    /// `php_error_docref`: formats and reports a function-prefixed diagnostic ("f(): message"); the
    /// docref is ignored.
    pub fn php_error_docref(docref: *const c_char, r#type: c_int, format: *const c_char, ...);
    /// `php_error_docref1`: `php_error_docref` for a call with one named parameter; the docref and
    /// parameter are ignored.
    pub fn php_error_docref1(docref: *const c_char, param1: *const c_char, r#type: c_int, format: *const c_char, ...);
    /// `php_error_docref2`: `php_error_docref` for a call with two named parameters; the docref and
    /// parameters are ignored.
    pub fn php_error_docref2(
        docref: *const c_char,
        param1: *const c_char,
        param2: *const c_char,
        r#type: c_int,
        format: *const c_char,
        ...
    );
    /// `php_verror`: the `va_list` form of the docref diagnostics; formats the message and reports
    /// it with the active function's prefix.
    pub fn php_verror(docref: *const c_char, params: *const c_char, r#type: c_int, format: *const c_char, args: va_list);

    // Exceptions with a formatted message.
    /// `zend_throw_error`: formats a message and throws it as an exception of class `exception_ce`
    /// (Error when NULL).
    pub fn zend_throw_error(exception_ce: *mut zend_class_entry, format: *const c_char, ...);
    /// `zend_type_error`: formats a message and throws it as a TypeError.
    pub fn zend_type_error(format: *const c_char, ...);
    /// `zend_value_error`: formats a message and throws it as a ValueError.
    pub fn zend_value_error(format: *const c_char, ...);
    /// `zend_argument_count_error`: formats a message and throws it as an ArgumentCountError.
    pub fn zend_argument_count_error(format: *const c_char, ...);
    /// `zend_argument_error`: formats a message and throws it as an exception of class `error_ce`
    /// prefixed with the "f(): Argument #N ($name)" description of argument `arg_num`.
    pub fn zend_argument_error(error_ce: *mut zend_class_entry, arg_num: u32, format: *const c_char, ...);
    /// `zend_argument_type_error`: `zend_argument_error` with a TypeError.
    pub fn zend_argument_type_error(arg_num: u32, format: *const c_char, ...);
    /// `zend_argument_value_error`: `zend_argument_error` with a ValueError.
    pub fn zend_argument_value_error(arg_num: u32, format: *const c_char, ...);
    /// `zend_argument_error_variadic`: the `va_list` form of `zend_argument_error`; formats the
    /// message and throws it with the argument prefix.
    pub fn zend_argument_error_variadic(error_ce: *mut zend_class_entry, arg_num: u32, format: *const c_char, va: va_list);
    /// `zend_throw_exception_ex`: formats a message and throws it as an exception of class
    /// `exception_ce` with the given code; returns the exception object.
    pub fn zend_throw_exception_ex(
        exception_ce: *mut zend_class_entry,
        code: zend_long,
        format: *const c_char,
        ...
    ) -> *mut zend_object;

    // Formatting.
    /// `zend_spprintf`: formats into a newly allocated NUL-terminated buffer stored in `*message`
    /// (truncated to `max_len` bytes when non-zero); returns the length. The caller frees the
    /// buffer with `efree`.
    pub fn zend_spprintf(message: *mut *mut c_char, max_len: usize, format: *const c_char, ...) -> usize;
    /// `zend_spprintf_unchecked`: same as `zend_spprintf`; the variant without compiler format
    /// checking.
    pub fn zend_spprintf_unchecked(message: *mut *mut c_char, max_len: usize, format: *const c_char, ...) -> usize;
    /// `zend_vspprintf`: the `va_list` form of `zend_spprintf`.
    pub fn zend_vspprintf(pbuf: *mut *mut c_char, max_len: usize, format: *const c_char, ap: va_list) -> usize;
    /// `zend_strpprintf`: formats into a new `zend_string` (truncated to `max_len` bytes when
    /// non-zero) that the caller releases.
    pub fn zend_strpprintf(max_len: usize, format: *const c_char, ...) -> *mut zend_string;
    /// `zend_strpprintf_unchecked`: same as `zend_strpprintf`; the variant without compiler format
    /// checking.
    pub fn zend_strpprintf_unchecked(max_len: usize, format: *const c_char, ...) -> *mut zend_string;
    /// `zend_vstrpprintf`: the `va_list` form of `zend_strpprintf`.
    pub fn zend_vstrpprintf(max_len: usize, format: *const c_char, ap: va_list) -> *mut zend_string;
    /// `ap_php_snprintf`: `snprintf` into `buf` of size `len`; returns the length the full output
    /// would have had.
    pub fn ap_php_snprintf(buf: *mut c_char, len: usize, format: *const c_char, ...) -> c_int;
    /// `ap_php_vsnprintf`: the `va_list` form of `ap_php_snprintf`.
    pub fn ap_php_vsnprintf(buf: *mut c_char, len: usize, format: *const c_char, ap: va_list) -> c_int;
    /// `ap_php_slprintf`: like `ap_php_snprintf` but returns the length actually written (at most
    /// `len - 1`).
    pub fn ap_php_slprintf(buf: *mut c_char, len: usize, format: *const c_char, ...) -> c_int;
    /// `ap_php_vslprintf`: the `va_list` form of `ap_php_slprintf`.
    pub fn ap_php_vslprintf(buf: *mut c_char, len: usize, format: *const c_char, ap: va_list) -> c_int;
    /// `php_printf`: formats and writes to the program's standard output through `zend_write`;
    /// returns the byte count written.
    pub fn php_printf(format: *const c_char, ...) -> usize;
    /// `php_printf_unchecked`: same as `php_printf`; the variant without compiler format checking.
    pub fn php_printf_unchecked(format: *const c_char, ...) -> usize;
    /// `smart_str_append_printf`: formats and appends the text to a `smart_str`.
    pub fn smart_str_append_printf(dest: *mut smart_str, format: *const c_char, ...);
    /// `smart_string_append_printf`: formats and appends the text to a `smart_string`.
    pub fn smart_string_append_printf(dest: *mut smart_string, format: *const c_char, ...);

    // Argument parsing with a type spec.
    /// `zend_parse_parameters`: parses the active call's arguments against a type spec (`l`, `s`,
    /// `a`, `|` and so on), writing each value through the following pointer arguments. Returns
    /// SUCCESS, or FAILURE after throwing the argument error. An unknown spec letter is an
    /// unsupported-feature fatal error.
    pub fn zend_parse_parameters(num_args: u32, type_spec: *const c_char, ...) -> zend_result;
    /// `zend_parse_parameters_ex`: `zend_parse_parameters` with flags such as
    /// `ZEND_PARSE_PARAMS_QUIET`.
    pub fn zend_parse_parameters_ex(flags: c_int, num_args: u32, type_spec: *const c_char, ...) -> zend_result;
    /// `zend_parse_method_parameters`: `zend_parse_parameters` for a method; a leading `O` in the
    /// spec binds `$this` and checks its class.
    pub fn zend_parse_method_parameters(num_args: u32, this_ptr: *mut zval, type_spec: *const c_char, ...) -> zend_result;
    /// `zend_parse_method_parameters_ex`: `zend_parse_method_parameters` with flags.
    pub fn zend_parse_method_parameters_ex(
        flags: c_int,
        num_args: u32,
        this_ptr: *mut zval,
        type_spec: *const c_char,
        ...
    ) -> zend_result;
    /// `zend_parse_parameter`: parses a single argument value against a spec into the following
    /// pointer arguments.
    pub fn zend_parse_parameter(flags: c_int, arg_num: u32, arg: *mut zval, spec: *const c_char, ...) -> zend_result;

    // Variadic registrations and calls.
    /// `zend_class_implements`: makes a class implement the `num_interfaces` interface class
    /// entries passed as variadic arguments (at most 64).
    pub fn zend_class_implements(class_entry: *mut zend_class_entry, num_interfaces: c_int, ...);
    /// `zend_fcall_info_argn`: sets call arguments from variadic zvals; unsupported here, because a
    /// hosted extension cannot call back into PHP code, and it stops the program.
    pub fn zend_fcall_info_argn(fci: *mut zend_fcall_info, argc: u32, ...);
    /// `zend_hash_apply_with_arguments`: applies a callback with extra variadic arguments to every
    /// element of a table; unsupported here and stops the program.
    pub fn zend_hash_apply_with_arguments(ht: *mut HashTable, apply_func: apply_func_args_t, num_args: c_int, ...);
    /// `zend_set_hash_symbol`: stores a symbol into variadic symbol tables; unsupported here
    /// because PHP symbol tables do not exist, and it stops the program.
    pub fn zend_set_hash_symbol(
        symbol: *mut zval,
        name: *const c_char,
        name_length: usize,
        is_ref: bool,
        num_symbol_tables: c_int,
        ...
    ) -> zend_result;
    /// `php_stream_wrapper_log_error`: logs an error for a stream wrapper; unsupported here because
    /// PHP streams are not provided, and it stops the program.
    pub fn php_stream_wrapper_log_error(wrapper: *const php_stream_wrapper, options: c_int, fmt: *const c_char, ...);

    // phpinfo(): nothing to print in a compiled program.
    /// `php_info_print_table_header`: phpinfo table header; does nothing in a compiled program.
    pub fn php_info_print_table_header(num_cols: c_int, ...);
    /// `php_info_print_table_row`: phpinfo table row; does nothing in a compiled program.
    pub fn php_info_print_table_row(num_cols: c_int, ...);
    /// `php_info_print_table_row_ex`: phpinfo table row with a CSS class; does nothing in a
    /// compiled program.
    pub fn php_info_print_table_row_ex(num_cols: c_int, css: *const c_char, ...);

    /// Runs `handler(execute_data, return_value)` inside a bailout frame
    /// installed in `*slot` (`EG(bailout)`). Returns 0, or 1 when the call
    /// bailed out (a fatal error, already reported).
    /// c: hidden
    pub fn elephc_zend_protected_call(
        handler: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        execute_data: *mut c_void,
        return_value: *mut c_void,
        slot: *mut *mut sigjmp_buf,
    ) -> c_int;
    /// Writes to the program's stdout (Elephc's `__rt_stdout_write`).
    /// c: hidden
    pub fn elephc_zend_stdout_write(str: *const c_char, len: usize);
    /// Whether the program is inside `@`.
    /// c: hidden
    pub fn elephc_zend_diag_suppressed() -> c_int;
    /// Jumps to a bailout frame.
    /// c: hidden
    pub fn elephc_zend_longjmp(target: *mut sigjmp_buf) -> !;
}
