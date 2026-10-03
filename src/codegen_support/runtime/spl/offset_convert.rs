//! Purpose:
//! Emits `__rt_spl_offset_convert`, which turns an SPL container's boxed offset into the integer
//! index PHP would use, and `__rt_spl_throw_offset_type`, which raises the TypeError naming the
//! offset's type when there is none.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed`, next to the SPL container helpers.
//! - The `SplFixedArray` and `SplDoublyLinkedList` offset helpers in this module's siblings.
//!
//! Key details:
//! - PHP uses two different rules. `SplFixedArray` goes through `spl_offset_convert_to_long`: an
//!   int, a bool, a float (truncated, with the float-to-int diagnostics) or a CANONICAL integer
//!   string (`"1"`, `"-1"`, the same test an array key uses) is an index, anything else is
//!   `TypeError("Cannot access offset of type <type> on SplFixedArray")`. The linked-list family
//!   declares `int $index` and takes weak-mode coercion: any fully numeric string (`" 1"`,
//!   `"1.0"`, `"+1"`, `"1e0"`) is accepted, a string that is not numeric is a TypeError, and a null
//!   means `0` (`offsetSet()` alone gives null its own meaning, an append). A float must be finite
//!   and inside the int range (`INF`, `NAN`, `1e20` are `TypeError(... float given)`, and a numeric
//!   string spelling such a value is `... string given`); a lossy one truncates with PHP's
//!   `Implicit conversion from float[-string] ... to int loses precision` deprecation. PHP
//!   8.4 keeps the `SplDoublyLinkedList::` prefix for `SplStack` and `SplQueue` too.
//! - A type error comes back as the address of a 16-byte `{pointer, length}` name row, the shape of
//!   `_class_name_entries`, so an object names its class and a scalar names its type through one
//!   word, and no reference to the offset outlives the call.
//! - The helper CONSUMES the boxed offset, and releases it before any diagnostic: a deprecation or
//!   warning can run a user error handler that throws, which would strand the box. A lossy
//!   float-string buffers the string into the diagnostic before the box goes.
//! - A stack-local exceptional owner covers numeric parsing and diagnostic-buffer allocation.
//!   Normal paths detach it before releasing the box, preventing double cleanup.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::{emit_throwable_creation_line_unknown, x86_64_heap_kind_word};

#[cfg(test)]
mod tests;

/// `SplFixedArray` offset rules: canonical integer strings only, null is a type error.
pub(crate) const SPL_OFFSET_MODE_FIXED: i64 = 0;
/// Linked-list `int $index` rules: numeric strings coerce, null reads as index 0.
pub(crate) const SPL_OFFSET_MODE_LIST: i64 = 1;
/// Linked-list rules for `offsetSet()`, where a null offset is reported so it can append.
pub(crate) const SPL_OFFSET_MODE_LIST_NULLABLE: i64 = 2;
/// Status for a converted integer index; equal to the int tag the callers already compare against.
pub(crate) const SPL_OFFSET_STATUS_INT: i64 = 0;
/// Status for a null offset under `SPL_OFFSET_MODE_LIST_NULLABLE`; equal to the null tag.
pub(crate) const SPL_OFFSET_STATUS_NULL: i64 = 8;

/// The type names a rejected offset can carry, in `_spl_offset_type_rows` order, as
/// `(data symbol of the name, name length)`.
pub(crate) const SPL_OFFSET_TYPE_ROWS: [(&str, usize); 7] = [
    ("_unser_type_string", 6),
    ("_unser_type_null", 4),
    ("_unser_type_array", 5),
    ("_unser_type_resource", 8),
    ("_sprintf_closure_class_name", 7),
    ("_unser_type_object", 6),
    ("_unser_type_float", 5),
];
const ROW_STRING: i64 = 0;
const ROW_NULL: i64 = 1;
const ROW_ARRAY: i64 = 2;
const ROW_RESOURCE: i64 = 3;
const ROW_CLOSURE: i64 = 4;
const ROW_OBJECT: i64 = 5;
const ROW_FLOAT: i64 = 6;
/// Bit patterns of 2^63 and -2^63, the bounds PHP's `ZEND_DOUBLE_FITS_LONG` checks against.
const POSITIVE_2_63: i64 = 0x43e0000000000000;
const NEGATIVE_2_63: u64 = 0xc3e0000000000000;
/// PHP's float-string precision deprecation, around the string as written.
pub(crate) const SPL_FLOAT_STRING_PREFIX: &str = "Deprecated: Implicit conversion from float-string \"";
pub(crate) const SPL_FLOAT_STRING_SUFFIX: &str = "\" to int loses precision\n";

/// Emits both helpers for the current target.
pub(crate) fn emit_spl_offset_runtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_convert_x86_64(emitter);
        emit_throw_x86_64(emitter);
    } else {
        emit_convert_aarch64(emitter);
        emit_throw_aarch64(emitter);
    }
}

/// Emits `__rt_spl_offset_convert` for ARM64.
///
/// Input: `x0` = owned boxed offset, `x1` = mode. Output: `x0` = status (int, 1 for a type error,
/// or null), `x1` = the integer index or the name-row address. Frame (96 bytes): `[sp]` box,
/// `[sp, #8]` mode, `[sp, #16]` status, `[sp, #24]`/`[sp, #32]` payload words, `[sp, #40]` result,
/// `[sp, #48..80]` owner guard, `[sp, #80]` linkage.
fn emit_convert_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl offset convert ---");
    emitter.label_global("__rt_spl_offset_convert");
    emitter.instruction("sub sp, sp, #96");                                     // reserve conversion slots and the exceptional owner guard
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the conversion frame
    emitter.instruction("stp x0, x1, [sp]");                                    // keep the owned box and the conversion mode
    super::super::exceptions::guards::guard(emitter, 48, 80);
    emitter.instruction("ldr x0, [sp]");                                        // reload the guarded box after registration
    emitter.instruction("bl __rt_mixed_unbox");                                 // read the offset's tag and payload words
    emitter.instruction("stp x1, x2, [sp, #24]");                               // keep the payload across the classification calls
    emitter.instruction("cmp x0, #0");                                          // an int is already an index
    emitter.instruction("b.eq __rt_spl_offset_convert_int");                    // use the int payload as is
    emitter.instruction("cmp x0, #3");                                          // a bool is the index 0 or 1
    emitter.instruction("b.eq __rt_spl_offset_convert_int");                    // its payload is already 0 or 1
    emitter.instruction("cmp x0, #1");                                          // a string needs PHP's string rules
    emitter.instruction("b.eq __rt_spl_offset_convert_string");                 // classify the string
    emitter.instruction("cmp x0, #2");                                          // a float truncates
    emitter.instruction("b.eq __rt_spl_offset_convert_float");                  // convert it after releasing the box
    emitter.instruction("cmp x0, #8");                                          // null depends on the container
    emitter.instruction("b.eq __rt_spl_offset_convert_null");                   // apply the mode's null rule
    emitter.instruction("cmp x0, #6");                                          // an object names its class
    emitter.instruction("b.eq __rt_spl_offset_convert_object");                 // look up the class name row
    emitter.instruction("mov x10, #2");                                         // packed and associative arrays are both "array"
    emitter.instruction("cmp x0, #4");                                          // an indexed array?
    emitter.instruction("b.eq __rt_spl_offset_convert_named");                  // report "array"
    emitter.instruction("cmp x0, #5");                                          // an associative array?
    emitter.instruction("b.eq __rt_spl_offset_convert_named");                  // report "array"
    emitter.instruction(&format!("mov x10, #{}", ROW_RESOURCE));                // a resource
    emitter.instruction("cmp x0, #9");                                          // is the offset a resource?
    emitter.instruction("b.eq __rt_spl_offset_convert_named");                  // report "resource"
    emitter.instruction(&format!("mov x10, #{}", ROW_CLOSURE));                 // callables are Closure objects in PHP
    emitter.instruction("cmp x0, #10");                                         // is the offset a callable?
    emitter.instruction("b.eq __rt_spl_offset_convert_named");                  // report "Closure"
    emitter.instruction(&format!("mov x10, #{}", ROW_OBJECT));                  // anything else is reported as an object
    emitter.instruction("b __rt_spl_offset_convert_named");                     // report the fallback name

    emitter.label("__rt_spl_offset_convert_int");
    emitter.instruction("ldr x9, [sp, #24]");                                   // the integer payload
    emitter.instruction("str x9, [sp, #40]");                                   // is the index
    emitter.instruction(&format!("mov x9, #{}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("str x9, [sp, #16]");                                   // record the status
    emitter.instruction("b __rt_spl_offset_convert_release");                   // release the box and return

    emitter.label("__rt_spl_offset_convert_string");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the mode
    emitter.instruction(&format!("cmp x9, #{}", SPL_OFFSET_MODE_FIXED));        // SplFixedArray takes canonical integer strings only
    emitter.instruction("b.ne __rt_spl_offset_convert_string_coerce");          // the linked lists coerce any numeric string
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // borrow the string pointer and length from the live box
    emitter.instruction("bl __rt_hash_normalize_key");                          // apply the array-key integer-string test
    emitter.instruction("cmn x2, #1");                                          // did it produce an integer key?
    emitter.instruction("b.ne __rt_spl_offset_convert_string_rejected");        // a non-canonical string is a type error
    emitter.instruction("str x1, [sp, #40]");                                   // the integer is the index
    emitter.instruction(&format!("mov x9, #{}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("str x9, [sp, #16]");                                   // record the status
    emitter.instruction("b __rt_spl_offset_convert_release");                   // release the box and return
    emitter.label("__rt_spl_offset_convert_string_coerce");
    emitter.instruction("ldp x0, x1, [sp, #24]");                               // borrow the string pointer and length from the live box
    emitter.instruction("bl __rt_str_numeric_value");                           // parse it as weak-mode int coercion does
    emitter.instruction("cbnz x2, __rt_spl_offset_convert_string_rejected");    // only a fully numeric string coerces
    emitter.instruction("cmp x0, #0");                                          // did it parse as an integer?
    emitter.instruction("b.eq __rt_spl_offset_convert_string_int");             // store the integer
    emitter.instruction("str x1, [sp, #16]");                                   // keep the float's bits
    emitter.instruction("fmov d0, x1");                                         // the float the string spells
    emit_int_range_check_aarch64(emitter, "__rt_spl_offset_convert_string_rejected");
    emitter.instruction("bl __rt_php_float_to_int");                            // truncate toward zero
    emitter.instruction("str x9, [sp, #40]");                                   // the integer is the index
    emitter.instruction("scvtf d1, x9");                                        // rebuild the integer as a float
    emitter.instruction("ldr d0, [sp, #16]");                                   // reload the float the string spells
    emitter.instruction("fcmp d0, d1");                                         // was the conversion exact?
    emitter.instruction("b.eq __rt_spl_offset_convert_string_exact");           // "1.0" converts silently
    abi::emit_symbol_address(emitter, "x1", "_spl_float_string_prefix");
    emitter.instruction(&format!("mov x2, #{}", SPL_FLOAT_STRING_PREFIX.len())); // deprecation prefix length
    emitter.instruction("bl __rt_diag_warning_fragment");                       // start the float-string deprecation
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // the string as written, still borrowed from the box
    emitter.instruction("bl __rt_diag_warning_fragment");                       // quote it in the deprecation
    super::super::exceptions::guards::unguard(emitter, 48, 80);
    emitter.instruction("ldr x0, [sp]");                                        // the owned boxed offset
    emitter.instruction("bl __rt_decref_mixed");                                // release it before the handler can run
    abi::emit_symbol_address(emitter, "x1", "_spl_float_string_suffix");
    emitter.instruction(&format!("mov x2, #{}", SPL_FLOAT_STRING_SUFFIX.len())); // deprecation suffix length
    emitter.instruction("bl __rt_diag_warning");                                // finish the deprecation, which may run the handler
    emitter.instruction("ldr x1, [sp, #40]");                                   // the converted index
    emitter.instruction(&format!("mov x0, #{}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("b __rt_spl_offset_convert_return");                    // the box is already released
    emitter.label("__rt_spl_offset_convert_string_exact");
    emitter.instruction(&format!("mov x9, #{}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("str x9, [sp, #16]");                                   // record the status
    emitter.instruction("b __rt_spl_offset_convert_release");                   // release the box and return
    emitter.label("__rt_spl_offset_convert_string_int");
    emitter.instruction("str x1, [sp, #40]");                                   // the integer is the index
    emitter.instruction(&format!("mov x9, #{}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("str x9, [sp, #16]");                                   // record the status
    emitter.instruction("b __rt_spl_offset_convert_release");                   // release the box and return
    emitter.label("__rt_spl_offset_convert_string_rejected");
    emitter.instruction(&format!("mov x10, #{}", ROW_STRING));                  // report "string"
    emitter.instruction("b __rt_spl_offset_convert_named");                     // record the type error

    emitter.label("__rt_spl_offset_convert_float");
    super::super::exceptions::guards::unguard(emitter, 48, 80);
    emitter.instruction("ldr x0, [sp]");                                        // the owned box holds nothing the float needs
    emitter.instruction("bl __rt_decref_mixed");                                // release it before a diagnostic can run user code
    emitter.instruction("ldr x9, [sp, #24]");                                   // the float's bits
    emitter.instruction("fmov d0, x9");                                         // pass the float
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the mode
    emitter.instruction(&format!("cmp x9, #{}", SPL_OFFSET_MODE_FIXED));        // SplFixedArray converts any float like an array key
    emitter.instruction("b.eq __rt_spl_offset_convert_float_convert");          // with the array-key diagnostics
    emit_int_range_check_aarch64(emitter, "__rt_spl_offset_convert_float_rejected");
    emitter.label("__rt_spl_offset_convert_float_convert");
    emitter.instruction("bl __rt_float_key_to_int");                            // truncate with PHP's float-to-int diagnostics
    emitter.instruction("mov x1, x0");                                          // the converted integer is the index
    emitter.instruction(&format!("mov x0, #{}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("b __rt_spl_offset_convert_return");                    // the box is already released

    emitter.label("__rt_spl_offset_convert_float_rejected");
    abi::emit_symbol_address(emitter, "x1", "_spl_offset_type_rows");
    emitter.instruction(&format!("add x1, x1, #{}", ROW_FLOAT * 16));           // report the float type name
    emitter.instruction("mov x0, #1");                                          // status: type error
    emitter.instruction("b __rt_spl_offset_convert_return");                    // the box is already released

    emitter.label("__rt_spl_offset_convert_null");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the mode
    emitter.instruction(&format!("cmp x9, #{}", SPL_OFFSET_MODE_LIST));         // a linked-list index reads null as 0
    emitter.instruction("b.eq __rt_spl_offset_convert_null_zero");              // use index 0
    emitter.instruction(&format!("cmp x9, #{}", SPL_OFFSET_MODE_LIST_NULLABLE)); // offsetSet() gives null its own meaning
    emitter.instruction("b.eq __rt_spl_offset_convert_null_status");            // report the null
    emitter.instruction(&format!("mov x10, #{}", ROW_NULL));                    // SplFixedArray rejects null
    emitter.instruction("b __rt_spl_offset_convert_named");                     // report "null"
    emitter.label("__rt_spl_offset_convert_null_zero");
    emitter.instruction("str xzr, [sp, #40]");                                  // index 0
    emitter.instruction(&format!("mov x9, #{}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("str x9, [sp, #16]");                                   // record the status
    emitter.instruction("b __rt_spl_offset_convert_release");                   // release the box and return
    emitter.label("__rt_spl_offset_convert_null_status");
    emitter.instruction("str xzr, [sp, #40]");                                  // no index
    emitter.instruction(&format!("mov x9, #{}", SPL_OFFSET_STATUS_NULL));       // status: null
    emitter.instruction("str x9, [sp, #16]");                                   // record the status
    emitter.instruction("b __rt_spl_offset_convert_release");                   // release the box and return

    emitter.label("__rt_spl_offset_convert_object");
    emitter.instruction("ldr x13, [sp, #24]");                                  // the object pointer
    emitter.instruction("ldr x13, [x13]");                                      // its class id, kept outside the symbol helper's x9 scratch
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_name_count", 0);
    emitter.instruction("cmp x13, x10");                                        // is the class id within the dense name table?
    emitter.instruction("b.hs __rt_spl_offset_convert_object_fallback");        // malformed ids use the generic spelling
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
    emitter.instruction("add x11, x11, x13, lsl #4");                           // select the 16-byte class-name row
    emitter.instruction("ldr x12, [x11, #8]");                                  // its byte length
    emitter.instruction("cbz x12, __rt_spl_offset_convert_object_fallback");    // a nameless class reads as "object"
    emitter.instruction("str x11, [sp, #40]");                                  // the class-name row is the result
    emitter.instruction("b __rt_spl_offset_convert_rejected");                  // record the type error
    emitter.label("__rt_spl_offset_convert_object_fallback");
    emitter.instruction(&format!("mov x10, #{}", ROW_OBJECT));                  // report "object"

    emitter.label("__rt_spl_offset_convert_named");
    abi::emit_symbol_address(emitter, "x11", "_spl_offset_type_rows");
    emitter.instruction("add x11, x11, x10, lsl #4");                           // select the type-name row
    emitter.instruction("str x11, [sp, #40]");                                  // the row is the result
    emitter.label("__rt_spl_offset_convert_rejected");
    emitter.instruction("mov x9, #1");                                          // status: type error
    emitter.instruction("str x9, [sp, #16]");                                   // record the status

    emitter.label("__rt_spl_offset_convert_release");
    super::super::exceptions::guards::unguard(emitter, 48, 80);
    emitter.instruction("ldr x0, [sp]");                                        // the owned boxed offset
    emitter.instruction("bl __rt_decref_mixed");                                // this helper consumes it
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the status
    emitter.instruction("ldr x1, [sp, #40]");                                   // and the index or name row
    emitter.label("__rt_spl_offset_convert_return");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the conversion frame
    emitter.instruction("ret");                                                 // return status and value
}

/// Emits `__rt_spl_throw_offset_type` for ARM64. Never returns.
///
/// Input: `x0`/`x1` = message prefix pointer and length, `x2` = name-row address, `x3`/`x4` =
/// message suffix pointer and length. Raises `TypeError("<prefix><name><suffix>")`.
fn emit_throw_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl offset type error ---");
    emitter.label_global("__rt_spl_throw_offset_type");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the suffix and message pairs and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a Throwable-construction frame
    emitter.instruction("stp x3, x4, [sp, #16]");                               // keep the suffix across the first concat
    emitter.instruction("mov x9, x2");                                          // the name row
    emitter.instruction("mov x2, x1");                                          // left operand length: the prefix
    emitter.instruction("mov x1, x0");                                          // left operand pointer: the prefix
    emitter.instruction("ldp x3, x4, [x9]");                                    // right operand: the type or class name
    emitter.instruction("bl __rt_concat");                                      // build `<prefix><name>`
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // right operand: the suffix
    emitter.instruction("bl __rt_concat");                                      // append it
    emitter.instruction("bl __rt_str_persist");                                 // give the Throwable stable message ownership
    emitter.instruction("stp x1, x2, [sp]");                                    // keep the message pair across the allocation
    emitter.instruction("mov x0, #56");                                         // canonical Throwable payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the Throwable object payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 identifies a throwable object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a runtime object
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the Throwable to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "x9", "_spl_type_error_class_id", 0);
    emitter.instruction("str x9, [x0]");                                        // stamp the TypeError class id
    emitter.instruction("ldp x10, x11, [sp]");                                  // recover the persisted message pair
    emitter.instruction("str x10, [x0, #8]");                                   // message pointer
    emitter.instruction("str x11, [x0, #16]");                                  // message byte length
    emitter.instruction("str xzr, [x0, #24]");                                  // code = 0
    emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("str xzr, [x0, #40]");                                  // previous = null
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // publish the Throwable for the unwinder
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("b __rt_throw_current");                                // unwind, or report it uncaught and exit like PHP
}

/// Emits `__rt_spl_offset_convert` for x86_64.
///
/// Input: `rdi` = owned boxed offset, `rsi` = mode. Output: `rax` = status, `rdi` = the integer
/// index or the name-row address. Frame: `[rbp - 8]` box, `[rbp - 16]` mode, `[rbp - 24]` status,
/// `[rbp - 32]`/`[rbp - 40]` payload words, `[rbp - 48]` result, `[rbp - 80..48]` owner guard.
fn emit_convert_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl offset convert ---");
    emitter.label_global("__rt_spl_offset_convert");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the conversion frame
    emitter.instruction("sub rsp, 80");                                         // reserve conversion slots and the exceptional owner guard
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the owned box
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the conversion mode
    emitter.instruction("mov rax, rdi");                                        // publish the consumed box through the native owner convention
    super::super::exceptions::guards::guard(emitter, 48, 80);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the guarded box after registration
    emitter.instruction("call __rt_mixed_unbox");                               // read the offset's tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // keep the payload low word
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // keep the payload high word
    emitter.instruction("cmp rax, 0");                                          // an int is already an index
    emitter.instruction("je __rt_spl_offset_convert_int_x");                    // use the int payload as is
    emitter.instruction("cmp rax, 3");                                          // a bool is the index 0 or 1
    emitter.instruction("je __rt_spl_offset_convert_int_x");                    // its payload is already 0 or 1
    emitter.instruction("cmp rax, 1");                                          // a string needs PHP's string rules
    emitter.instruction("je __rt_spl_offset_convert_string_x");                 // classify the string
    emitter.instruction("cmp rax, 2");                                          // a float truncates
    emitter.instruction("je __rt_spl_offset_convert_float_x");                  // convert it after releasing the box
    emitter.instruction("cmp rax, 8");                                          // null depends on the container
    emitter.instruction("je __rt_spl_offset_convert_null_x");                   // apply the mode's null rule
    emitter.instruction("cmp rax, 6");                                          // an object names its class
    emitter.instruction("je __rt_spl_offset_convert_object_x");                 // look up the class name row
    emitter.instruction(&format!("mov r10, {}", ROW_ARRAY));                    // packed and associative arrays are both "array"
    emitter.instruction("cmp rax, 4");                                          // an indexed array?
    emitter.instruction("je __rt_spl_offset_convert_named_x");                  // report "array"
    emitter.instruction("cmp rax, 5");                                          // an associative array?
    emitter.instruction("je __rt_spl_offset_convert_named_x");                  // report "array"
    emitter.instruction(&format!("mov r10, {}", ROW_RESOURCE));                 // a resource
    emitter.instruction("cmp rax, 9");                                          // is the offset a resource?
    emitter.instruction("je __rt_spl_offset_convert_named_x");                  // report "resource"
    emitter.instruction(&format!("mov r10, {}", ROW_CLOSURE));                  // callables are Closure objects in PHP
    emitter.instruction("cmp rax, 10");                                         // is the offset a callable?
    emitter.instruction("je __rt_spl_offset_convert_named_x");                  // report "Closure"
    emitter.instruction(&format!("mov r10, {}", ROW_OBJECT));                   // anything else is reported as an object
    emitter.instruction("jmp __rt_spl_offset_convert_named_x");                 // report the fallback name

    emitter.label("__rt_spl_offset_convert_int_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // the integer payload
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // is the index
    emitter.instruction(&format!("mov QWORD PTR [rbp - 24], {}", SPL_OFFSET_STATUS_INT)); // status: converted
    emitter.instruction("jmp __rt_spl_offset_convert_release_x");               // release the box and return

    emitter.label("__rt_spl_offset_convert_string_x");
    emitter.instruction(&format!("cmp QWORD PTR [rbp - 16], {}", SPL_OFFSET_MODE_FIXED)); // SplFixedArray takes canonical integer strings only
    emitter.instruction("jne __rt_spl_offset_convert_string_coerce_x");         // the linked lists coerce any numeric string
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // borrow the string pointer from the live box
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // and its length
    emitter.instruction("call __rt_hash_normalize_key");                        // apply the array-key integer-string test
    emitter.instruction("cmp rdx, -1");                                         // did it produce an integer key?
    emitter.instruction("jne __rt_spl_offset_convert_string_rejected_x");       // a non-canonical string is a type error
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // the integer is the index
    emitter.instruction(&format!("mov QWORD PTR [rbp - 24], {}", SPL_OFFSET_STATUS_INT)); // status: converted
    emitter.instruction("jmp __rt_spl_offset_convert_release_x");               // release the box and return
    emitter.label("__rt_spl_offset_convert_string_coerce_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // borrow the string pointer from the live box
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // and its length
    emitter.instruction("call __rt_str_numeric_value");                         // parse it as weak-mode int coercion does
    emitter.instruction("test rdx, rdx");                                       // only a fully numeric string coerces
    emitter.instruction("jnz __rt_spl_offset_convert_string_rejected_x");       // anything else is a type error
    emitter.instruction("cmp rax, 0");                                          // did it parse as an integer?
    emitter.instruction("je __rt_spl_offset_convert_string_int_x");             // store the integer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // keep the float's bits
    emitter.instruction("movq xmm0, rdi");                                      // the float the string spells
    emit_int_range_check_x86_64(emitter, "__rt_spl_offset_convert_string_rejected_x");
    emitter.instruction("call __rt_php_float_to_int");                          // truncate toward zero
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // the integer is the index
    emitter.instruction("cvtsi2sd xmm1, r11");                                  // rebuild the integer as a float
    emitter.instruction("movq xmm0, QWORD PTR [rbp - 24]");                     // reload the float the string spells
    emitter.instruction("ucomisd xmm0, xmm1");                                  // was the conversion exact?
    emitter.instruction("je __rt_spl_offset_convert_string_exact_x");           // "1.0" converts silently
    emitter.instruction("lea rdi, [rip + _spl_float_string_prefix]");           // deprecation prefix
    emitter.instruction(&format!("mov rsi, {}", SPL_FLOAT_STRING_PREFIX.len())); // deprecation prefix length
    emitter.instruction("call __rt_diag_warning_fragment");                     // start the float-string deprecation
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // the string as written, still borrowed from the box
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // and its length
    emitter.instruction("call __rt_diag_warning_fragment");                     // quote it in the deprecation
    super::super::exceptions::guards::unguard(emitter, 48, 80);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the owned boxed offset
    emitter.instruction("call __rt_decref_mixed");                              // release it before the handler can run
    emitter.instruction("lea rdi, [rip + _spl_float_string_suffix]");           // deprecation suffix
    emitter.instruction(&format!("mov rsi, {}", SPL_FLOAT_STRING_SUFFIX.len())); // deprecation suffix length
    emitter.instruction("call __rt_diag_warning");                              // finish the deprecation, which may run the handler
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // the converted index
    emitter.instruction(&format!("mov rax, {}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("jmp __rt_spl_offset_convert_return_x");                // the box is already released
    emitter.label("__rt_spl_offset_convert_string_exact_x");
    emitter.instruction(&format!("mov QWORD PTR [rbp - 24], {}", SPL_OFFSET_STATUS_INT)); // status: converted
    emitter.instruction("jmp __rt_spl_offset_convert_release_x");               // release the box and return
    emitter.label("__rt_spl_offset_convert_string_int_x");
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // the integer is the index
    emitter.instruction(&format!("mov QWORD PTR [rbp - 24], {}", SPL_OFFSET_STATUS_INT)); // status: converted
    emitter.instruction("jmp __rt_spl_offset_convert_release_x");               // release the box and return
    emitter.label("__rt_spl_offset_convert_string_rejected_x");
    emitter.instruction(&format!("mov r10, {}", ROW_STRING));                   // report "string"
    emitter.instruction("jmp __rt_spl_offset_convert_named_x");                 // record the type error

    emitter.label("__rt_spl_offset_convert_float_x");
    super::super::exceptions::guards::unguard(emitter, 48, 80);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the owned box holds nothing the float needs
    emitter.instruction("call __rt_decref_mixed");                              // release it before a diagnostic can run user code
    emitter.instruction("movq xmm0, QWORD PTR [rbp - 32]");                     // pass the float
    emitter.instruction(&format!("cmp QWORD PTR [rbp - 16], {}", SPL_OFFSET_MODE_FIXED)); // SplFixedArray converts any float like an array key
    emitter.instruction("je __rt_spl_offset_convert_float_convert_x");          // with the array-key diagnostics
    emit_int_range_check_x86_64(emitter, "__rt_spl_offset_convert_float_rejected_x");
    emitter.label("__rt_spl_offset_convert_float_convert_x");
    emitter.instruction("call __rt_float_key_to_int");                          // truncate with PHP's float-to-int diagnostics
    emitter.instruction("mov rdi, rax");                                        // the converted integer is the index
    emitter.instruction(&format!("mov rax, {}", SPL_OFFSET_STATUS_INT));        // status: converted
    emitter.instruction("jmp __rt_spl_offset_convert_return_x");                // the box is already released

    emitter.label("__rt_spl_offset_convert_float_rejected_x");
    emitter.instruction("lea rdi, [rip + _spl_offset_type_rows]");              // the type-name rows
    emitter.instruction(&format!("add rdi, {}", ROW_FLOAT * 16));               // report the float type name
    emitter.instruction("mov rax, 1");                                          // status: type error
    emitter.instruction("jmp __rt_spl_offset_convert_return_x");                // the box is already released

    emitter.label("__rt_spl_offset_convert_null_x");
    emitter.instruction(&format!("cmp QWORD PTR [rbp - 16], {}", SPL_OFFSET_MODE_LIST)); // a linked-list index reads null as 0
    emitter.instruction("je __rt_spl_offset_convert_null_zero_x");              // use index 0
    emitter.instruction(&format!("cmp QWORD PTR [rbp - 16], {}", SPL_OFFSET_MODE_LIST_NULLABLE)); // offsetSet() gives null its own meaning
    emitter.instruction("je __rt_spl_offset_convert_null_status_x");            // report the null
    emitter.instruction(&format!("mov r10, {}", ROW_NULL));                     // SplFixedArray rejects null
    emitter.instruction("jmp __rt_spl_offset_convert_named_x");                 // report "null"
    emitter.label("__rt_spl_offset_convert_null_zero_x");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // index 0
    emitter.instruction(&format!("mov QWORD PTR [rbp - 24], {}", SPL_OFFSET_STATUS_INT)); // status: converted
    emitter.instruction("jmp __rt_spl_offset_convert_release_x");               // release the box and return
    emitter.label("__rt_spl_offset_convert_null_status_x");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // no index
    emitter.instruction(&format!("mov QWORD PTR [rbp - 24], {}", SPL_OFFSET_STATUS_NULL)); // status: null
    emitter.instruction("jmp __rt_spl_offset_convert_release_x");               // release the box and return

    emitter.label("__rt_spl_offset_convert_object_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // the object pointer
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // its class id
    emitter.instruction("cmp r11, QWORD PTR [rip + _class_name_count]");        // is the class id within the dense name table?
    emitter.instruction("jae __rt_spl_offset_convert_object_fallback_x");       // malformed ids use the generic spelling
    emitter.instruction("lea r10, [rip + _class_name_entries]");                // dense class-name metadata table
    emitter.instruction("shl r11, 4");                                          // scale the class id to the 16-byte row
    emitter.instruction("add r10, r11");                                        // select the class-name row
    emitter.instruction("cmp QWORD PTR [r10 + 8], 0");                          // is the name non-empty?
    emitter.instruction("je __rt_spl_offset_convert_object_fallback_x");        // a nameless class reads as "object"
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // the class-name row is the result
    emitter.instruction("jmp __rt_spl_offset_convert_rejected_x");              // record the type error
    emitter.label("__rt_spl_offset_convert_object_fallback_x");
    emitter.instruction(&format!("mov r10, {}", ROW_OBJECT));                   // report "object"

    emitter.label("__rt_spl_offset_convert_named_x");
    emitter.instruction("shl r10, 4");                                          // scale the row index to the 16-byte row
    emitter.instruction("lea r11, [rip + _spl_offset_type_rows]");              // the type-name rows
    emitter.instruction("add r11, r10");                                        // select the type-name row
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // the row is the result
    emitter.label("__rt_spl_offset_convert_rejected_x");
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // status: type error

    emitter.label("__rt_spl_offset_convert_release_x");
    super::super::exceptions::guards::unguard(emitter, 48, 80);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the owned boxed offset
    emitter.instruction("call __rt_decref_mixed");                              // this helper consumes it
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the status
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // and the index or name row
    emitter.label("__rt_spl_offset_convert_return_x");
    emitter.instruction("mov rsp, rbp");                                        // release the conversion frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return status and value
}

/// Emits `__rt_spl_throw_offset_type` for x86_64. Never returns.
///
/// Input: `rdi`/`rsi` = message prefix pointer and length, `rdx` = name-row address, `rcx`/`r8`
/// = message suffix pointer and length. Raises `TypeError("<prefix><name><suffix>")`.
fn emit_throw_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl offset type error ---");
    emitter.label_global("__rt_spl_throw_offset_type");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a Throwable-construction frame
    emitter.instruction("sub rsp, 32");                                         // reserve the suffix and message pairs, keeping rsp aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                        // keep the suffix pointer across the first concat
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // keep the suffix length
    emitter.instruction("mov r10, rdx");                                        // the name row
    emitter.instruction("mov rax, rdi");                                        // left operand pointer: the prefix
    emitter.instruction("mov rdx, rsi");                                        // left operand length: the prefix
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // right operand pointer: the type or class name
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // right operand length
    abi::emit_call_label(emitter, "__rt_concat");                               // build `<prefix><name>`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // right operand pointer: the suffix
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // right operand length
    abi::emit_call_label(emitter, "__rt_concat");                               // append it
    abi::emit_call_label(emitter, "__rt_str_persist");                          // give the Throwable stable message ownership
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the message pointer across the allocation
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // keep the message byte length
    emitter.instruction("mov rax, 56");                                         // canonical Throwable payload size
    abi::emit_call_label(emitter, "__rt_heap_alloc");                           // allocate the Throwable object payload (rax = payload)
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(6))); // magic + kind 6 identifies a throwable object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform heap header
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");                // bind the Throwable to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_type_error_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // stamp the TypeError class id
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // recover the message pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // recover the message byte length
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // message pointer
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // message byte length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // code = 0
    emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous = null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the Throwable for the unwinder
    emitter.instruction("mov rsp, rbp");                                        // release the local frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // unwind, or report it uncaught and exit like PHP
}

/// Branches to `rejected` unless the float in `d0` fits PHP's int range: not NaN, below 2^63 and
/// at or above -2^63 (`ZEND_DOUBLE_FITS_LONG`). Leaves `d0` intact; clobbers `x9` and `d1`.
fn emit_int_range_check_aarch64(emitter: &mut Emitter, rejected: &str) {
    emitter.instruction("fcmp d0, d0");                                         // is the float NaN?
    emitter.instruction(&format!("b.vs {rejected}"));                           // NaN is not an int
    abi::emit_load_int_immediate(emitter, "x9", POSITIVE_2_63);
    emitter.instruction("fmov d1, x9");                                         // 2^63, the first float past the int range
    emitter.instruction("fcmp d0, d1");                                         // too large (INF included)?
    emitter.instruction(&format!("b.ge {rejected}"));                           // the float does not fit an int
    abi::emit_load_int_immediate(emitter, "x9", NEGATIVE_2_63 as i64);
    emitter.instruction("fmov d1, x9");                                         // -2^63, the smallest int
    emitter.instruction("fcmp d0, d1");                                         // too small (-INF included)?
    emitter.instruction(&format!("b.lt {rejected}"));                           // the float does not fit an int
}

/// Jumps to `rejected` unless the float in `xmm0` fits PHP's int range: not NaN, below 2^63 and
/// at or above -2^63 (`ZEND_DOUBLE_FITS_LONG`). Leaves `xmm0` intact; clobbers `r10` and `xmm1`.
fn emit_int_range_check_x86_64(emitter: &mut Emitter, rejected: &str) {
    emitter.instruction("ucomisd xmm0, xmm0");                                  // is the float NaN?
    emitter.instruction(&format!("jp {rejected}"));                             // NaN is not an int
    abi::emit_load_int_immediate(emitter, "r10", POSITIVE_2_63);
    emitter.instruction("movq xmm1, r10");                                      // 2^63, the first float past the int range
    emitter.instruction("ucomisd xmm0, xmm1");                                  // too large (INF included)?
    emitter.instruction(&format!("jae {rejected}"));                            // the float does not fit an int
    abi::emit_load_int_immediate(emitter, "r10", NEGATIVE_2_63 as i64);
    emitter.instruction("movq xmm1, r10");                                      // -2^63, the smallest int
    emitter.instruction("ucomisd xmm0, xmm1");                                  // too small (-INF included)?
    emitter.instruction(&format!("jb {rejected}"));                             // the float does not fit an int
}
