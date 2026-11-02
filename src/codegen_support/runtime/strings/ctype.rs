//! Purpose:
//! Emits the shared `ctype_*` runtime helpers: byte predicates, PHP's deprecated integer
//! fallback, and the dynamic deprecation that names a non-string argument's runtime type.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through
//!   `crate::codegen_support::runtime::strings`.
//! - `crate::codegen::lower_inst::builtins::ctype` for the shared ABI entry points.
//!
//! Key details:
//! - `__rt_ctype_string(kind, ptr, len)` scans every byte and rejects the empty string.
//! - `__rt_ctype_int(kind, value)` implements php-src 8.1+'s 0..255, -128..-1, positive
//!   `allow_digits`, and negative `allow_minus` fallback matrix.
//! - `__rt_ctype_check(kind, cell)` unboxes a borrowed Mixed operand and dispatches on its
//!   runtime tag; non-string tags emit the php-src deprecation before answering false.
//! - `__rt_ctype_deprecated` writes a complete `Deprecated: ... in <file> on line <N>` line
//!   through the shared warning dispatcher so handlers, masks, and HTML output stay shared.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Suffix shared by every non-string ctype deprecation.
pub(crate) const DEPRECATION_SUFFIX: &str =
    " will be interpreted as string in the future in ";
/// Separator between the caller file and line number in a deprecation.
pub(crate) const LINE_SEPARATOR: &str = " on line ";
/// Terminal newline of a complete ctype deprecation.
pub(crate) const NEWLINE: &str = "\n";
/// Function-name prefixes indexed by the shared ctype kind number.
pub(crate) const PREFIXES: [&str; 11] = [
    "ctype_alnum(): Argument of type ",
    "ctype_alpha(): Argument of type ",
    "ctype_cntrl(): Argument of type ",
    "ctype_digit(): Argument of type ",
    "ctype_graph(): Argument of type ",
    "ctype_lower(): Argument of type ",
    "ctype_print(): Argument of type ",
    "ctype_punct(): Argument of type ",
    "ctype_space(): Argument of type ",
    "ctype_upper(): Argument of type ",
    "ctype_xdigit(): Argument of type ",
];
/// Runtime type names used by the Mixed dispatch and the static lowering fallback.
pub(crate) const TYPE_INT: &str = "int";
pub(crate) const TYPE_FLOAT: &str = "float";
pub(crate) const TYPE_BOOL: &str = "bool";
pub(crate) const TYPE_NULL: &str = "null";
pub(crate) const TYPE_ARRAY: &str = "array";
pub(crate) const TYPE_RESOURCE: &str = "resource";
pub(crate) const TYPE_CLOSURE: &str = "Closure";
pub(crate) const TYPE_UNKNOWN: &str = "unknown";

/// Emits every ctype runtime helper for the active target.
pub(crate) fn emit_ctype(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ctype_x86_64(emitter);
        return;
    }
    emit_ctype_aarch64(emitter);
}

/// `allow_digits` answer for every ctype kind, indexed by the shared kind number.
const ALLOW_DIGITS: [bool; 11] = [
    true, false, false, true, true, false, true, false, false, false, true,
];
/// `allow_minus` answer for every ctype kind, indexed by the shared kind number.
const ALLOW_MINUS: [bool; 11] = [
    false, false, false, false, true, false, true, false, false, false, false,
];

/// Emits one integer answer (`true` or `false`) and returns from the helper.
fn emit_aarch64_answer(emitter: &mut Emitter, value: bool) {
    emitter.instruction(&format!("mov x0, #{}", i64::from(value)));             // materialize the ctype boolean answer in the result register
    emitter.instruction("ret");                                                 // return the helper's ctype answer
}

/// Emits one integer answer (`true` or `false`) and returns from the x86_64 helper.
fn emit_x86_64_answer(emitter: &mut Emitter, value: bool) {
    emitter.instruction(&format!("mov eax, {}", i64::from(value)));             // materialize the ctype boolean answer in the result register
    emitter.instruction("ret");                                                 // return the helper's ctype answer
}

/// Emits the complete AArch64 ctype helper family.
fn emit_ctype_aarch64(emitter: &mut Emitter) {
    emit_aarch64_string(emitter);
    emit_aarch64_byte(emitter);
    emit_aarch64_int(emitter);
    emit_aarch64_object(emitter);
    emit_aarch64_check(emitter);
    emit_aarch64_deprecated(emitter);
}

/// Emits `__rt_ctype_string(kind, ptr, len)` on AArch64.
fn emit_aarch64_string(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_string ---");
    emitter.label_global("__rt_ctype_string");
    emitter.instruction("stp x19, x20, [sp, #-48]!");                           // preserve the string pointer and length across libc calls
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve the ctype kind and scan index
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller linkage across the classifier calls
    emitter.instruction("mov x19, x1");                                         // retain the PHP string payload pointer
    emitter.instruction("mov x20, x2");                                         // retain the PHP string byte length
    emitter.instruction("mov x21, x0");                                         // retain the ctype kind
    emitter.instruction("mov x22, #0");                                         // start scanning at byte offset zero
    emitter.instruction("cbz x20, __rt_ctype_string_fail");                     // PHP ctype rejects the empty string
    emitter.label("__rt_ctype_string_loop");
    emitter.instruction("cmp x22, x20");                                        // check whether every byte has been examined
    emitter.instruction("b.hs __rt_ctype_string_pass");                         // report success once the scan reaches the string length
    emitter.instruction("ldrb w0, [x19, x22]");                                 // load the current byte from the PHP string payload
    emitter.instruction("mov x1, x21");                                         // pass the selected ctype kind
    emitter.instruction("bl __rt_ctype_byte");                                  // classify the byte through the active libc locale
    emitter.instruction("cbz w0, __rt_ctype_string_fail");                      // reject the string when any byte fails the class
    emitter.instruction("add x22, x22, #1");                                    // advance to the next byte after accepting the current byte
    emitter.instruction("b __rt_ctype_string_loop");                            // continue scanning the remaining bytes
    emitter.label("__rt_ctype_string_fail");
    emitter.instruction("mov x0, #0");                                          // materialize false for an empty string or rejected byte
    emitter.instruction("b __rt_ctype_string_done");                            // skip the success result after a failed predicate
    emitter.label("__rt_ctype_string_pass");
    emitter.instruction("mov x0, #1");                                          // materialize true when every byte satisfies the predicate
    emitter.label("__rt_ctype_string_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller linkage after the scan
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the ctype kind and scan index
    emitter.instruction("ldp x19, x20, [sp], #48");                             // restore the string pointer and length
    emitter.instruction("ret");                                                 // return the ctype boolean answer
}

/// Emits `__rt_ctype_byte(kind, byte)` on AArch64, delegating classification to libc.
fn emit_aarch64_byte(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_byte ---");
    emitter.label_global("__rt_ctype_byte");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve linkage across the libc classifier
    for (kind, symbol) in [
        (0, "isalnum"),
        (1, "isalpha"),
        (2, "iscntrl"),
        (3, "isdigit"),
        (4, "isgraph"),
        (5, "islower"),
        (6, "isprint"),
        (7, "ispunct"),
        (8, "isspace"),
        (9, "isupper"),
        (10, "isxdigit"),
    ] {
        emitter.instruction(&format!("cmp x1, #{kind}"));                       // compare the kind index with this libc classifier
        emitter.instruction(&format!("b.eq __rt_ctype_byte_{symbol}"));         // dispatch to the matching libc predicate
    }
    emitter.instruction("mov w0, #0");                                          // unknown kind indices answer false
    emitter.instruction("b __rt_ctype_byte_done");                              // return the unknown-kind answer
    for (kind, symbol) in [
        (0, "isalnum"),
        (1, "isalpha"),
        (2, "iscntrl"),
        (3, "isdigit"),
        (4, "isgraph"),
        (5, "islower"),
        (6, "isprint"),
        (7, "ispunct"),
        (8, "isspace"),
        (9, "isupper"),
        (10, "isxdigit"),
    ] {
        let _ = kind;
        emitter.label(&format!("__rt_ctype_byte_{symbol}"));
        emitter.bl_c(symbol);                                                    // classify the byte in the process locale
        emitter.instruction("b __rt_ctype_byte_done");                          // return the libc classification
    }
    emitter.label("__rt_ctype_byte_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller linkage
    emitter.instruction("ret");                                                 // return the libc predicate answer
}

/// Emits `__rt_ctype_int(kind, value)` on AArch64.
fn emit_aarch64_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_int ---");
    emitter.label_global("__rt_ctype_int");
    emitter.instruction("cmp x1, #0");                                          // distinguish the non-negative byte window from negative codes
    emitter.instruction("b.lt __rt_ctype_int_negative");                        // handle -1..-128 through PHP's wrap rule
    emitter.instruction("cmp x1, #255");                                        // check the upper bound of the one-byte window
    emitter.instruction("b.gt __rt_ctype_int_high");                            // values above 255 use the allow_digits fallback
    emitter.instruction("b __rt_ctype_int_byte");                               // 0..255 is evaluated as one byte
    emitter.label("__rt_ctype_int_negative");
    emitter.instruction("mov x9, #-128");                                       // materialize the lowest wrapped byte code
    emitter.instruction("cmp x1, x9");                                          // check whether the value is below the wrapped window
    emitter.instruction("b.lt __rt_ctype_int_low");                             // values below -128 use the allow_minus fallback
    emitter.instruction("add x1, x1, #256");                                    // wrap -128..-1 into the 128..255 byte window
    emitter.label("__rt_ctype_int_byte");
    emitter.instruction("sub sp, sp, #32");                                     // reserve linkage and an aligned temporary byte
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller across the shared string scan
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame for the scanner call
    emitter.instruction("strb w1, [sp]");                                       // store the requested character code as one byte
    emitter.instruction("mov x1, sp");                                          // pass the temporary byte as the scanner's string pointer
    emitter.instruction("mov x2, #1");                                          // scan exactly one byte
    emitter.instruction("bl __rt_ctype_string");                                // evaluate the byte predicate shared with string inputs
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller linkage after the scan
    emitter.instruction("add sp, sp, #32");                                     // release the temporary byte and frame
    emitter.instruction("ret");                                                 // return the predicate answer
    emitter.label("__rt_ctype_int_high");
    emit_aarch64_int_fallback(emitter);
    emitter.label("__rt_ctype_int_low");
    emit_aarch64_int_minus_fallback(emitter);
}

/// Emits the AArch64 kind-dependent allow_digits fallback for positive out-of-byte values.
fn emit_aarch64_int_fallback(emitter: &mut Emitter) {
    for kind in 0..11 {
        emitter.instruction(&format!("cmp x0, #{kind}"));                       // compare the kind index with this fallback case
        emitter.instruction(&format!("b.eq __rt_ctype_int_high_{kind}"));       // select this kind's allow_digits answer
    }
    emitter.instruction("b __rt_ctype_int_high_false");                         // unknown kinds answer false
    for (kind, allowed) in ALLOW_DIGITS.iter().enumerate() {
        emitter.label(&format!("__rt_ctype_int_high_{kind}"));
        emit_aarch64_answer(emitter, *allowed);
    }
    emitter.label("__rt_ctype_int_high_false");
    emit_aarch64_answer(emitter, false);
}

/// Emits the AArch64 kind-dependent allow_minus fallback for negative out-of-byte values.
fn emit_aarch64_int_minus_fallback(emitter: &mut Emitter) {
    for kind in 0..11 {
        emitter.instruction(&format!("cmp x0, #{kind}"));                       // compare the kind index with this allow_minus case
        emitter.instruction(&format!("b.eq __rt_ctype_int_low_{kind}"));        // select this kind's allow_minus answer
    }
    emitter.instruction("b __rt_ctype_int_low_false");                          // unknown kinds answer false
    for (kind, allowed) in ALLOW_MINUS.iter().enumerate() {
        emitter.label(&format!("__rt_ctype_int_low_{kind}"));
        emit_aarch64_answer(emitter, *allowed);
    }
    emitter.label("__rt_ctype_int_low_false");
    emit_aarch64_answer(emitter, false);
}

/// Emits `__rt_ctype_check(kind, cell)` on AArch64.
fn emit_aarch64_check(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_check ---");
    emitter.label_global("__rt_ctype_check");
    emitter.instruction("sub sp, sp, #64");                                     // reserve the dispatch frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame linkage across diagnostics
    emitter.instruction("add x29, sp, #32");                                    // establish a stable dispatch frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the ctype kind
    emitter.instruction("str x1, [sp, #48]");                                   // preserve the borrowed Mixed cell
    emitter.instruction("mov x0, x1");                                          // pass the Mixed cell to the unbox helper
    emitter.instruction("bl __rt_mixed_unbox");                                 // read the concrete runtime tag and payload
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the runtime tag
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the payload low word
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the payload high word or string length
    emitter.instruction("cmp x0, #1");                                          // recognize a PHP string payload
    emitter.instruction("b.eq __rt_ctype_check_string");                        // strings scan every byte without a deprecation
    emitter.instruction("cmp x0, #0");                                          // recognize a PHP integer payload
    emitter.instruction("b.eq __rt_ctype_check_int");                           // integers use the deprecated fallback matrix
    emitter.instruction("cmp x0, #2");                                          // recognize a PHP float payload
    emitter.instruction("b.eq __rt_ctype_check_float");                         // floats are named in the deprecation
    emitter.instruction("cmp x0, #3");                                          // recognize a PHP boolean payload
    emitter.instruction("b.eq __rt_ctype_check_bool");                          // booleans are named in the deprecation
    emitter.instruction("cmp x0, #4");                                          // recognize a packed array payload
    emitter.instruction("b.eq __rt_ctype_check_array");                         // arrays are named in the deprecation
    emitter.instruction("cmp x0, #5");                                          // recognize a hash payload
    emitter.instruction("b.eq __rt_ctype_check_array");                         // hashes are arrays for PHP's type name
    emitter.instruction("cmp x0, #6");                                          // recognize an object payload
    emitter.instruction("b.eq __rt_ctype_check_object");                        // objects are named by their class
    emitter.instruction("cmp x0, #8");                                          // recognize the null runtime tag
    emitter.instruction("b.eq __rt_ctype_check_null");                          // null is named in the deprecation
    emitter.instruction("cmp x0, #9");                                          // recognize a resource payload
    emitter.instruction("b.eq __rt_ctype_check_resource");                      // resources are named in the deprecation
    emitter.instruction("cmp x0, #10");                                         // recognize a callable/Closure payload
    emitter.instruction("b.eq __rt_ctype_check_closure");                       // closures are named in the deprecation
    emitter.instruction("b __rt_ctype_check_unknown");                          // every other tag is answered as unknown
    emitter.label("__rt_ctype_check_string");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // restore the string length
    emitter.instruction("bl __rt_ctype_string");                                // scan the string with the shared predicate
    emitter.instruction("b __rt_ctype_check_done");                             // return the predicate answer
    emitter.label("__rt_ctype_check_int");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind
    emit_aarch64_type_deprecation(emitter, TYPE_INT);
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind after the diagnostic
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the integer code after the diagnostic
    emitter.instruction("bl __rt_ctype_int");                                   // apply PHP's deprecated integer fallback matrix
    emitter.instruction("b __rt_ctype_check_done");                             // return the integer answer
    emit_aarch64_named_check_case(emitter, "float", TYPE_FLOAT);
    emit_aarch64_named_check_case(emitter, "bool", TYPE_BOOL);
    emit_aarch64_named_check_case(emitter, "array", TYPE_ARRAY);
    emit_aarch64_named_check_case(emitter, "null", TYPE_NULL);
    emit_aarch64_named_check_case(emitter, "resource", TYPE_RESOURCE);
    emit_aarch64_named_check_case(emitter, "closure", TYPE_CLOSURE);
    emit_aarch64_check_object(emitter);
    emit_aarch64_check_unknown(emitter);
    emitter.label("__rt_ctype_check_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame linkage
    emitter.instruction("add sp, sp, #64");                                     // release the dispatch frame
    emitter.instruction("ret");                                                 // return the ctype boolean answer
}

/// Emits one AArch64 deprecation-only Mixed case and rejoins the dispatch footer.
fn emit_aarch64_named_check_case(emitter: &mut Emitter, label: &str, type_name: &str) {
    emitter.label(&format!("__rt_ctype_check_{label}"));
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind
    emit_aarch64_type_deprecation(emitter, type_name);
    emitter.instruction("mov x0, #0");                                          // non-string, non-integer arguments answer false
    emitter.instruction("b __rt_ctype_check_done");                             // return the false answer
}

/// Emits the AArch64 object case, delegating to the shared object-name helper.
fn emit_aarch64_check_object(emitter: &mut Emitter) {
    emitter.label("__rt_ctype_check_object");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the unboxed object pointer
    emitter.instruction("bl __rt_ctype_object");                                // report the concrete class name
    emitter.instruction("b __rt_ctype_check_done");                             // return the false answer
}

/// Emits `__rt_ctype_object(kind, object)` on AArch64.
fn emit_aarch64_object(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_object ---");
    emitter.label_global("__rt_ctype_object");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the object-name frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve linkage across the deprecation
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the ctype kind
    emitter.instruction("cbz x1, __rt_ctype_object_unknown");                   // a null object pointer has only a generic name
    emitter.instruction("ldr x9, [x1]");                                        // load the concrete runtime class id
    emitter.instruction("cmn x9, #2");                                          // reserved id -2 denotes an incomplete unserialized object
    emitter.instruction("b.eq __rt_ctype_object_incomplete");                   // name the incomplete class exactly as PHP does
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense class-name lookup bound
    emitter.instruction("cmp x9, x10");                                         // validate the object class id before indexing metadata
    emitter.instruction("b.hs __rt_ctype_object_unknown");                      // invalid ids fall back to the generic name
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
    emitter.instruction("lsl x12, x9, #4");                                     // scale the class id by the 16-byte metadata row
    emitter.instruction("add x11, x11, x12");                                   // select the class-name metadata row
    emitter.instruction("ldr x1, [x11]");                                       // load the real class-name pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the real class-name length
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind
    emitter.instruction("bl __rt_ctype_deprecated");                            // report the object's concrete class name
    emitter.instruction("b __rt_ctype_object_done");                            // return the false answer
    emitter.label("__rt_ctype_object_incomplete");
    abi::emit_symbol_address(emitter, "x1", "_incomplete_class_name");
    emitter.instruction("mov x2, #22");                                         // len("__PHP_Incomplete_Class")
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind
    emitter.instruction("bl __rt_ctype_deprecated");                            // report the incomplete class name
    emitter.instruction("b __rt_ctype_object_done");                            // return the false answer
    emitter.label("__rt_ctype_object_unknown");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind
    emit_aarch64_type_deprecation(emitter, TYPE_UNKNOWN);
    emitter.label("__rt_ctype_object_done");
    emitter.instruction("mov x0, #0");                                          // objects are not characters
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame linkage
    emitter.instruction("add sp, sp, #32");                                     // release the object-name frame
    emitter.instruction("ret");                                                 // return the false answer
}

/// Emits the AArch64 generic non-string case.
fn emit_aarch64_check_unknown(emitter: &mut Emitter) {
    emitter.label("__rt_ctype_check_unknown");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind
    emit_aarch64_type_deprecation(emitter, TYPE_UNKNOWN);
    emitter.instruction("mov x0, #0");                                          // unknown tags answer false
    emitter.instruction("b __rt_ctype_check_done");                             // return the false answer
}

/// Emits an AArch64 deprecation whose type name is a fixed runtime label.
fn emit_aarch64_type_deprecation(emitter: &mut Emitter, type_name: &str) {
    abi::emit_symbol_address(emitter, "x1", ctype_type_label(type_name));
    emitter.instruction(&format!("mov x2, #{}", type_name.len()));              // pass the fixed runtime type-name length
    emitter.instruction("bl __rt_ctype_deprecated");                            // report the deprecation with the caller location
}

/// Emits `__rt_ctype_deprecated(kind, type_ptr, type_len)` on AArch64.
fn emit_aarch64_deprecated(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_deprecated ---");
    emitter.label_global("__rt_ctype_deprecated");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the diagnostic frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame linkage across warning dispatch
    emitter.instruction("add x29, sp, #32");                                    // establish a stable diagnostic frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the ctype kind
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the type-name pointer
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the type-name length
    abi::emit_symbol_address(emitter, "x1", "_rt_deprecated_prefix");
    emitter.instruction("mov x2, #12");                                         // len("Deprecated: ")
    emitter.instruction("bl __rt_diag_warning_fragment");                       // start the line with the category prefix
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the ctype kind for the prefix lookup
    abi::emit_symbol_address(emitter, "x9", "_ctype_deprecated_prefix_table");
    emitter.instruction("lsl x10, x0, #4");                                     // scale the kind by the 16-byte prefix row
    emitter.instruction("add x9, x9, x10");                                     // select the function-name prefix row
    emitter.instruction("ldr x1, [x9]");                                        // load the function-name prefix pointer
    emitter.instruction("ldr x2, [x9, #8]");                                    // load the function-name prefix length
    emitter.instruction("bl __rt_diag_warning_fragment");                       // start the deprecation line with the function name
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the type-name pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // restore the type-name length
    emitter.instruction("bl __rt_diag_warning_fragment");                       // append the runtime type name
    abi::emit_symbol_address(emitter, "x1", "_ctype_deprecated_suffix");
    emitter.instruction(&format!("mov x2, #{}", DEPRECATION_SUFFIX.len()));     // pass the fixed deprecation suffix length
    emitter.instruction("bl __rt_diag_warning_fragment");                       // append the future-conversion notice
    abi::emit_load_symbol_to_reg(emitter, "x1", "_php_diagnostic_file", 0);
    abi::emit_load_symbol_to_reg(emitter, "x2", "_php_diagnostic_file_len", 0);
    emitter.instruction("bl __rt_diag_warning_fragment");                       // append the caller source file
    abi::emit_symbol_address(emitter, "x1", "_ctype_deprecated_line_sep");
    emitter.instruction(&format!("mov x2, #{}", LINE_SEPARATOR.len()));         // pass the fixed file/line separator length
    emitter.instruction("bl __rt_diag_warning_fragment");                       // append the line separator
    abi::emit_load_symbol_to_reg(emitter, "x0", "_php_diagnostic_line", 0);
    emitter.instruction("bl __rt_itoa");                                        // format the caller line number
    emitter.instruction("bl __rt_diag_warning_fragment");                       // append the formatted caller line
    abi::emit_symbol_address(emitter, "x1", "_ctype_deprecated_newline");
    emitter.instruction("mov x2, #1");                                          // append the terminal newline
    emitter.instruction("bl __rt_diag_warning");                                // dispatch the complete deprecation
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame linkage
    emitter.instruction("add sp, sp, #48");                                     // release the diagnostic frame
    emitter.instruction("ret");                                                 // return after the deprecation callback
}

/// Returns the runtime data label holding a fixed ctype type-name string.
fn ctype_type_label(type_name: &str) -> &'static str {
    match type_name {
        TYPE_INT => "_ctype_type_int",
        TYPE_FLOAT => "_ctype_type_float",
        TYPE_BOOL => "_ctype_type_bool",
        TYPE_NULL => "_ctype_type_null",
        TYPE_ARRAY => "_ctype_type_array",
        TYPE_RESOURCE => "_ctype_type_resource",
        TYPE_CLOSURE => "_ctype_type_closure",
        _ => "_ctype_type_unknown",
    }
}

/// Emits the complete x86_64 ctype helper family.
fn emit_ctype_x86_64(emitter: &mut Emitter) {
    emit_x86_64_string(emitter);
    emit_x86_64_byte(emitter);
    emit_x86_64_int(emitter);
    emit_x86_64_object(emitter);
    emit_x86_64_check(emitter);
    emit_x86_64_deprecated(emitter);
}

/// Emits `__rt_ctype_string(kind, ptr, len)` on x86_64.
fn emit_x86_64_string(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_string ---");
    emitter.label_global("__rt_ctype_string");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable scan frame
    emitter.instruction("sub rsp, 32");                                         // reserve kind, pointer, length, and index slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // retain the ctype kind
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // retain the PHP string payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // retain the PHP string byte length
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start scanning at byte offset zero
    emitter.instruction("test rdx, rdx");                                       // PHP ctype rejects the empty string
    emitter.instruction("jz __rt_ctype_string_fail");                           // branch to false for an empty input string
    emitter.label("__rt_ctype_string_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current byte offset
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // check whether every byte has been examined
    emitter.instruction("jae __rt_ctype_string_pass");                          // report success once the scan reaches the string length
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the PHP string payload pointer
    emitter.instruction("movzx esi, BYTE PTR [r10 + rcx]");                     // load the current byte into the classifier argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the selected ctype kind
    emitter.instruction("call __rt_ctype_byte");                                // classify the byte through the active libc locale
    emitter.instruction("test eax, eax");                                       // reject the string when any byte fails the class
    emitter.instruction("jz __rt_ctype_string_fail");                           // branch to false for a rejected byte
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next byte after accepting the current byte
    emitter.instruction("jmp __rt_ctype_string_loop");                          // continue scanning the remaining bytes
    emitter.label("__rt_ctype_string_fail");
    emitter.instruction("xor eax, eax");                                        // materialize false for an empty string or rejected byte
    emitter.instruction("leave");                                               // release the scan frame
    emitter.instruction("ret");                                                 // return the failed ctype predicate
    emitter.label("__rt_ctype_string_pass");
    emitter.instruction("mov eax, 1");                                          // materialize true when every byte satisfies the predicate
    emitter.instruction("leave");                                               // release the scan frame
    emitter.instruction("ret");                                                 // return the successful ctype predicate
}

/// Emits `__rt_ctype_byte(kind, byte)` on x86_64, delegating classification to libc.
fn emit_x86_64_byte(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_byte ---");
    emitter.label_global("__rt_ctype_byte");
    for (kind, symbol) in [
        (0, "isalnum"),
        (1, "isalpha"),
        (2, "iscntrl"),
        (3, "isdigit"),
        (4, "isgraph"),
        (5, "islower"),
        (6, "isprint"),
        (7, "ispunct"),
        (8, "isspace"),
        (9, "isupper"),
        (10, "isxdigit"),
    ] {
        emitter.instruction(&format!("cmp rdi, {kind}"));                       // compare the kind index with this libc classifier
        emitter.instruction(&format!("je __rt_ctype_byte_{symbol}"));           // dispatch to the matching libc predicate
    }
    emitter.instruction("xor eax, eax");                                        // unknown kind indices answer false
    emitter.instruction("ret");                                                 // return the unknown-kind answer
    for (kind, symbol) in [
        (0, "isalnum"),
        (1, "isalpha"),
        (2, "iscntrl"),
        (3, "isdigit"),
        (4, "isgraph"),
        (5, "islower"),
        (6, "isprint"),
        (7, "ispunct"),
        (8, "isspace"),
        (9, "isupper"),
        (10, "isxdigit"),
    ] {
        let _ = kind;
        emitter.label(&format!("__rt_ctype_byte_{symbol}"));
        emitter.instruction("mov edi, esi");                                    // move the byte into the libc predicate argument
        emitter.instruction(&format!("jmp {symbol}"));                          // tail-call the locale-aware libc classifier
    }
}

/// Emits `__rt_ctype_int(kind, value)` on x86_64.
fn emit_x86_64_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_int ---");
    emitter.label_global("__rt_ctype_int");
    emitter.instruction("cmp rsi, 0");                                          // distinguish the non-negative byte window from negative codes
    emitter.instruction("jl __rt_ctype_int_negative");                          // handle -1..-128 through PHP's wrap rule
    emitter.instruction("cmp rsi, 255");                                        // check the upper bound of the one-byte window
    emitter.instruction("jg __rt_ctype_int_high");                              // values above 255 use the allow_digits fallback
    emitter.label("__rt_ctype_int_byte");
    emitter.instruction("sub rsp, 24");                                         // reserve an ABI-aligned temporary byte
    emitter.instruction("mov BYTE PTR [rsp], sil");                             // store the requested character code as one byte
    emitter.instruction("mov rsi, rsp");                                        // pass the temporary byte as the scanner's string pointer
    emitter.instruction("mov edx, 1");                                          // scan exactly one byte
    emitter.instruction("call __rt_ctype_string");                              // evaluate the byte predicate shared with string inputs
    emitter.instruction("add rsp, 24");                                         // release the temporary byte after the scan
    emitter.instruction("ret");                                                 // return the predicate answer
    emitter.label("__rt_ctype_int_negative");
    emitter.instruction("cmp rsi, -128");                                       // check whether the value is below the wrapped window
    emitter.instruction("jl __rt_ctype_int_low");                               // values below -128 use the allow_minus fallback
    emitter.instruction("add rsi, 256");                                        // wrap -128..-1 into the 128..255 byte window
    emitter.instruction("jmp __rt_ctype_int_byte");                             // evaluate the wrapped byte
    emitter.label("__rt_ctype_int_high");
    emit_x86_64_int_fallback(emitter);
    emitter.label("__rt_ctype_int_low");
    emit_x86_64_int_minus_fallback(emitter);
}

/// Emits the x86_64 kind-dependent allow_digits fallback for positive out-of-byte values.
fn emit_x86_64_int_fallback(emitter: &mut Emitter) {
    for kind in 0..11 {
        emitter.instruction(&format!("cmp rdi, {kind}"));                       // compare the kind index with this fallback case
        emitter.instruction(&format!("je __rt_ctype_int_high_{kind}"));         // select this kind's allow_digits answer
    }
    emitter.instruction("jmp __rt_ctype_int_high_false");                       // unknown kinds answer false
    for (kind, allowed) in ALLOW_DIGITS.iter().enumerate() {
        emitter.label(&format!("__rt_ctype_int_high_{kind}"));
        emit_x86_64_answer(emitter, *allowed);
    }
    emitter.label("__rt_ctype_int_high_false");
    emit_x86_64_answer(emitter, false);
}

/// Emits the x86_64 kind-dependent allow_minus fallback for negative out-of-byte values.
fn emit_x86_64_int_minus_fallback(emitter: &mut Emitter) {
    for kind in 0..11 {
        emitter.instruction(&format!("cmp rdi, {kind}"));                       // compare the kind index with this allow_minus case
        emitter.instruction(&format!("je __rt_ctype_int_low_{kind}"));          // select this kind's allow_minus answer
    }
    emitter.instruction("jmp __rt_ctype_int_low_false");                        // unknown kinds answer false
    for (kind, allowed) in ALLOW_MINUS.iter().enumerate() {
        emitter.label(&format!("__rt_ctype_int_low_{kind}"));
        emit_x86_64_answer(emitter, *allowed);
    }
    emitter.label("__rt_ctype_int_low_false");
    emit_x86_64_answer(emitter, false);
}

/// Emits `__rt_ctype_check(kind, cell)` on x86_64.
fn emit_x86_64_check(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_check ---");
    emitter.label_global("__rt_ctype_check");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable dispatch frame
    emitter.instruction("sub rsp, 64");                                         // reserve the dispatch frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the ctype kind
    emitter.instruction("mov rax, rsi");                                        // pass the Mixed cell to the unbox helper
    emitter.instruction("call __rt_mixed_unbox");                               // read the concrete runtime tag and payload
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the payload low word
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve the payload high word or string length
    emitter.instruction("cmp rax, 1");                                          // recognize a PHP string payload
    emitter.instruction("je __rt_ctype_check_string");                          // strings scan every byte without a deprecation
    emitter.instruction("cmp rax, 0");                                          // recognize a PHP integer payload
    emitter.instruction("je __rt_ctype_check_int");                             // integers use the deprecated fallback matrix
    emitter.instruction("cmp rax, 2");                                          // recognize a PHP float payload
    emitter.instruction("je __rt_ctype_check_float");                           // floats are named in the deprecation
    emitter.instruction("cmp rax, 3");                                          // recognize a PHP boolean payload
    emitter.instruction("je __rt_ctype_check_bool");                            // booleans are named in the deprecation
    emitter.instruction("cmp rax, 4");                                          // recognize a packed array payload
    emitter.instruction("je __rt_ctype_check_array");                           // arrays are named in the deprecation
    emitter.instruction("cmp rax, 5");                                          // recognize a hash payload
    emitter.instruction("je __rt_ctype_check_array");                           // hashes are arrays for PHP's type name
    emitter.instruction("cmp rax, 6");                                          // recognize an object payload
    emitter.instruction("je __rt_ctype_check_object");                          // objects are named by their class
    emitter.instruction("cmp rax, 8");                                          // recognize the null runtime tag
    emitter.instruction("je __rt_ctype_check_null");                            // null is named in the deprecation
    emitter.instruction("cmp rax, 9");                                          // recognize a resource payload
    emitter.instruction("je __rt_ctype_check_resource");                        // resources are named in the deprecation
    emitter.instruction("cmp rax, 10");                                         // recognize a callable/Closure payload
    emitter.instruction("je __rt_ctype_check_closure");                         // closures are named in the deprecation
    emitter.instruction("jmp __rt_ctype_check_unknown");                        // every other tag is answered as unknown
    emitter.label("__rt_ctype_check_string");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // restore the string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // restore the string length
    emitter.instruction("call __rt_ctype_string");                              // scan the string with the shared predicate
    emitter.instruction("jmp __rt_ctype_check_done");                           // return the predicate answer
    emitter.label("__rt_ctype_check_int");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind
    emit_x86_64_type_deprecation(emitter, TYPE_INT);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind after the diagnostic
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // restore the integer code after the diagnostic
    emitter.instruction("call __rt_ctype_int");                                 // apply PHP's deprecated integer fallback matrix
    emitter.instruction("jmp __rt_ctype_check_done");                           // return the integer answer
    emit_x86_64_named_check_case(emitter, "float", TYPE_FLOAT);
    emit_x86_64_named_check_case(emitter, "bool", TYPE_BOOL);
    emit_x86_64_named_check_case(emitter, "array", TYPE_ARRAY);
    emit_x86_64_named_check_case(emitter, "null", TYPE_NULL);
    emit_x86_64_named_check_case(emitter, "resource", TYPE_RESOURCE);
    emit_x86_64_named_check_case(emitter, "closure", TYPE_CLOSURE);
    emit_x86_64_check_object(emitter);
    emit_x86_64_check_unknown(emitter);
    emitter.label("__rt_ctype_check_done");
    emitter.instruction("leave");                                               // release the dispatch frame
    emitter.instruction("ret");                                                 // return the ctype boolean answer
}

/// Emits one x86_64 deprecation-only Mixed case and rejoins the dispatch footer.
fn emit_x86_64_named_check_case(emitter: &mut Emitter, label: &str, type_name: &str) {
    emitter.label(&format!("__rt_ctype_check_{label}"));
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind
    emit_x86_64_type_deprecation(emitter, type_name);
    emitter.instruction("xor eax, eax");                                        // non-string, non-integer arguments answer false
    emitter.instruction("jmp __rt_ctype_check_done");                           // return the false answer
}

/// Emits the x86_64 object case, delegating to the shared object-name helper.
fn emit_x86_64_check_object(emitter: &mut Emitter) {
    emitter.label("__rt_ctype_check_object");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // restore the unboxed object pointer
    emitter.instruction("call __rt_ctype_object");                              // report the concrete class name
    emitter.instruction("jmp __rt_ctype_check_done");                           // return the false answer
}

/// Emits `__rt_ctype_object(kind, object)` on x86_64.
fn emit_x86_64_object(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_object ---");
    emitter.label_global("__rt_ctype_object");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable object-name frame
    emitter.instruction("sub rsp, 32");                                         // reserve the object-name frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the ctype kind
    emitter.instruction("test rsi, rsi");                                       // a null object pointer has only a generic name
    emitter.instruction("jz __rt_ctype_object_unknown");                        // fall back to the generic type name
    emitter.instruction("mov r8, QWORD PTR [rsi]");                             // load the object's concrete runtime class id
    emitter.instruction("cmp r8, -2");                                          // reserved id -2 denotes an incomplete unserialized object
    emitter.instruction("je __rt_ctype_object_incomplete");                     // name the incomplete class exactly as PHP does
    emitter.instruction("mov r9, QWORD PTR [rip + _class_name_count]");         // load the dense class-name lookup bound
    emitter.instruction("cmp r8, r9");                                          // validate the object class id before indexing metadata
    emitter.instruction("jae __rt_ctype_object_unknown");                       // invalid ids fall back to the generic name
    emitter.instruction("lea r10, [rip + _class_name_entries]");                // materialize the class-name metadata table base
    emitter.instruction("shl r8, 4");                                           // scale the class id by the 16-byte metadata row
    emitter.instruction("mov rsi, QWORD PTR [r10 + r8]");                       // load the real class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + r8 + 8]");                   // load the real class-name length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind
    emitter.instruction("call __rt_ctype_deprecated");                          // report the object's concrete class name
    emitter.instruction("jmp __rt_ctype_object_done");                          // return the false answer
    emitter.label("__rt_ctype_object_incomplete");
    emitter.instruction("lea rsi, [rip + _incomplete_class_name]");             // pass PHP's incomplete-class name
    emitter.instruction("mov edx, 22");                                         // len("__PHP_Incomplete_Class")
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind
    emitter.instruction("call __rt_ctype_deprecated");                          // report the incomplete class name
    emitter.instruction("jmp __rt_ctype_object_done");                          // return the false answer
    emitter.label("__rt_ctype_object_unknown");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind
    emit_x86_64_type_deprecation(emitter, TYPE_UNKNOWN);
    emitter.label("__rt_ctype_object_done");
    emitter.instruction("xor eax, eax");                                        // objects are not characters
    emitter.instruction("leave");                                               // release the object-name frame
    emitter.instruction("ret");                                                 // return the false answer
}

/// Emits the x86_64 generic non-string case.
fn emit_x86_64_check_unknown(emitter: &mut Emitter) {
    emitter.label("__rt_ctype_check_unknown");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the ctype kind
    emit_x86_64_type_deprecation(emitter, TYPE_UNKNOWN);
    emitter.instruction("xor eax, eax");                                        // unknown tags answer false
    emitter.instruction("jmp __rt_ctype_check_done");                           // return the false answer
}

/// Emits an x86_64 deprecation whose type name is a fixed runtime label.
fn emit_x86_64_type_deprecation(emitter: &mut Emitter, type_name: &str) {
    abi::emit_symbol_address(emitter, "rsi", ctype_type_label(type_name));
    emitter.instruction(&format!("mov edx, {}", type_name.len()));              // pass the fixed runtime type-name length
    emitter.instruction("call __rt_ctype_deprecated");                          // report the deprecation with the caller location
}

/// Verifies every supported target emits the complete ctype helper family.
///
/// The execution suites run on the host target only, so this compile-time check keeps the
/// iOS and Linux emitters from silently losing a helper the shared lowering calls.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target defines the four ctype entry points and eleven kind prefixes.
    #[test]
    fn every_target_emits_the_complete_ctype_helper_family() {
        assert_eq!(PREFIXES.len(), 11);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_ctype(&mut emitter);
            let asm = emitter.output();
            for label in [
                "__rt_ctype_string:",
                "__rt_ctype_int:",
                "__rt_ctype_object:",
                "__rt_ctype_check:",
                "__rt_ctype_deprecated:",
            ] {
                assert!(asm.contains(label), "{name} is missing {label}");
            }
        }
    }
}

/// Emits `__rt_ctype_deprecated(kind, type_ptr, type_len)` on x86_64.
fn emit_x86_64_deprecated(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctype_deprecated ---");
    emitter.label_global("__rt_ctype_deprecated");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable diagnostic frame
    emitter.instruction("sub rsp, 48");                                         // reserve the diagnostic frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the ctype kind
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the type-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the type-name length
    abi::emit_symbol_address(emitter, "rdi", "_rt_deprecated_prefix");
    emitter.instruction("mov esi, 12");                                         // len("Deprecated: ")
    emitter.instruction("call __rt_diag_warning_fragment");                     // start the line with the category prefix
    emitter.instruction("lea r10, [rip + _ctype_deprecated_prefix_table]");     // materialize the function-name prefix table
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the ctype kind for the prefix lookup
    emitter.instruction("shl r11, 4");                                          // scale the kind by the 16-byte prefix row
    emitter.instruction("add r10, r11");                                        // select the function-name prefix row
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load the function-name prefix pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load the function-name prefix length
    emitter.instruction("call __rt_diag_warning_fragment");                     // start the deprecation line with the function name
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // restore the type-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // restore the type-name length
    emitter.instruction("call __rt_diag_warning_fragment");                     // append the runtime type name
    abi::emit_symbol_address(emitter, "rdi", "_ctype_deprecated_suffix");
    emitter.instruction(&format!("mov esi, {}", DEPRECATION_SUFFIX.len()));     // pass the fixed deprecation suffix length
    emitter.instruction("call __rt_diag_warning_fragment");                     // append the future-conversion notice
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_php_diagnostic_file", 0);
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_php_diagnostic_file_len", 0);
    emitter.instruction("call __rt_diag_warning_fragment");                     // append the caller source file
    abi::emit_symbol_address(emitter, "rdi", "_ctype_deprecated_line_sep");
    emitter.instruction(&format!("mov esi, {}", LINE_SEPARATOR.len()));         // pass the fixed file/line separator length
    emitter.instruction("call __rt_diag_warning_fragment");                     // append the line separator
    abi::emit_load_symbol_to_reg(emitter, "rax", "_php_diagnostic_line", 0);
    emitter.instruction("call __rt_itoa");                                      // format the caller line number
    emitter.instruction("mov rdi, rax");                                        // pass the formatted line-number pointer
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted line-number length
    emitter.instruction("call __rt_diag_warning_fragment");                     // append the formatted caller line
    abi::emit_symbol_address(emitter, "rdi", "_ctype_deprecated_newline");
    emitter.instruction("mov esi, 1");                                          // append the terminal newline
    emitter.instruction("call __rt_diag_warning");                              // dispatch the complete deprecation
    emitter.instruction("leave");                                               // release the diagnostic frame
    emitter.instruction("ret");                                                 // return after the deprecation callback
}
