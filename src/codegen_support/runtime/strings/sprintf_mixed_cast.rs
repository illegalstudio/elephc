//! Purpose:
//! Emits the printf-family coercion helpers for deferred non-scalar operands.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//! - `__rt_sprintf` after a packed argument record preserves either a boxed `Mixed`
//!   value or a statically typed non-scalar payload.
//!
//! Key details:
//! - Numeric formatting follows PHP's non-scalar cast rules: empty arrays become 0,
//!   non-empty arrays, objects, and callable descriptors become 1, and resources use
//!   their public resource id rather than their native handle.
//! - String formatting returns `"Array"`, resource display text, or a dynamic
//!   `__toString()` result, including eval-declared classes when an eval context is active.
//!   Non-stringable objects and callables throw a catchable PHP `Error`; raw heap pointers
//!   never become numeric output, and owned results are returned separately so the formatter
//!   can release them after copying.
//! - Array-to-string and object-to-number warnings flow through the standard warning helper,
//!   preserving suppression and dynamic eval class names on every supported target.

use crate::codegen_support::runtime::data::{
    SPRINTF_ARRAY_TO_STRING_WARNING, SPRINTF_OBJECT_NUMERIC_WARNING_PREFIX,
    SPRINTF_OBJECT_TO_FLOAT_WARNING_SUFFIX, SPRINTF_OBJECT_TO_INT_WARNING_SUFFIX,
    UNSER_OBJECT_STRING_ERROR_PREFIX, UNSER_OBJECT_STRING_ERROR_SUFFIX,
};
use crate::codegen_support::sentinels::{
    emit_throwable_creation_line_unknown, x86_64_heap_kind_word,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the deferred non-scalar coercion helpers used by `__rt_sprintf`.
pub fn emit_sprintf_mixed_casts(emitter: &mut Emitter, eval_bridge: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sprintf_mixed_casts_linux_x86_64(emitter, eval_bridge);
        emit_sprintf_warnings_linux_x86_64(emitter, eval_bridge);
        emit_sprintf_string_error_linux_x86_64(emitter);
        return;
    }

    emit_sprintf_mixed_to_int_aarch64(emitter);
    emit_sprintf_mixed_to_string_aarch64(emitter, eval_bridge);
    emit_sprintf_warnings_aarch64(emitter, eval_bridge);
    emit_sprintf_string_error_aarch64(emitter);
}

/// Emits AArch64 `__rt_sprintf_mixed_to_int(tag, payload, numeric_kind, eval_ctx) -> int`.
fn emit_sprintf_mixed_to_int_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf mixed non-scalar to int ---");
    emitter.label_global("__rt_sprintf_mixed_to_int");

    emitter.instruction("sub sp, sp, #48");                                     // reserve payload, conversion kind, context, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish this helper's frame pointer
    emitter.instruction("str x2, [sp, #8]");                                    // preserve integer-versus-float warning kind
    emitter.instruction("str x3, [sp, #16]");                                   // preserve the optional eval context
    emitter.instruction("cmp x0, #7");                                          // is the record payload a boxed Mixed cell?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_unbox");                   // inspect the concrete boxed tag
    emitter.instruction("cmp x0, #11");                                         // type-erased Iterable record?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_iterable");                // classify it by heap kind
    emitter.instruction("b __rt_sprintf_mixed_int_dispatch");                   // other raw tags are already concrete
    emitter.label("__rt_sprintf_mixed_int_unbox");
    emitter.instruction("mov x0, x1");                                          // pass the boxed payload to mixed_unbox
    emitter.instruction("bl __rt_mixed_unbox");                                 // unwrap nested Mixed cells before inspecting the concrete tag
    emitter.instruction("b __rt_sprintf_mixed_int_dispatch");                   // dispatch the unboxed concrete tag
    emitter.label("__rt_sprintf_mixed_int_iterable");
    emitter.instruction("str x1, [sp, #0]");                                    // preserve erased payload across heap classification
    emitter.instruction("mov x0, x1");                                          // pass the erased payload to heap classification
    emitter.instruction("bl __rt_heap_kind");                                   // recover its concrete runtime heap kind
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore the erased payload for conversion
    emitter.instruction("cmp x0, #2");                                          // indexed-array heap kind?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_array");                   // apply PHP array numeric conversion
    emitter.instruction("cmp x0, #3");                                          // associative-array heap kind?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_array");                   // apply PHP array numeric conversion
    emitter.instruction("cmp x0, #4");                                          // native-object heap kind?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_object");                  // warn and normalize the object to one
    emitter.instruction("cmp x0, #6");                                          // eval-object heap kind?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_object");                  // warn and normalize the object to one
    emitter.instruction("b __rt_sprintf_mixed_int_zero");                       // unknown erased values normalize to zero
    emitter.label("__rt_sprintf_mixed_int_dispatch");
    emitter.instruction("cmp x0, #4");                                          // indexed array?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_array");                   // arrays cast to zero or one by emptiness
    emitter.instruction("cmp x0, #5");                                          // associative array?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_array");                   // hashes share PHP's array numeric cast
    emitter.instruction("cmp x0, #6");                                          // object?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_object");                  // every object warns and casts to integer one
    emitter.instruction("cmp x0, #9");                                          // resource?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_resource");                // resources format through their public resource id
    emitter.instruction("cmp x0, #10");                                         // callable descriptor (Closure shape)?
    emitter.instruction("b.eq __rt_sprintf_mixed_int_callable");                // callable objects warn as Closure and cast to one
    emitter.instruction("mov x0, #0");                                          // unknown non-scalars normalize to zero, never their raw payload
    emitter.instruction("b __rt_sprintf_mixed_int_done");                       // restore the helper frame and return

    emitter.label("__rt_sprintf_mixed_int_array");
    emitter.instruction("cbz x1, __rt_sprintf_mixed_int_zero");                 // null container pointers behave like empty arrays
    emitter.instruction("ldr x9, [x1]");                                        // read the live element count from the container header
    emitter.instruction("cmp x9, #0");                                          // is the array empty?
    emitter.instruction("cset x0, ne");                                         // PHP casts empty arrays to 0 and non-empty arrays to 1
    emitter.instruction("b __rt_sprintf_mixed_int_done");                       // return the normalized boolean-sized integer

    emitter.label("__rt_sprintf_mixed_int_object");
    emitter.instruction("mov x0, x1");                                          // native object payload for class-name lookup
    emitter.instruction("ldr x1, [sp, #8]");                                    // integer-versus-float warning kind
    emitter.instruction("ldr x2, [sp, #16]");                                   // optional eval context for dynamic metadata
    emitter.instruction("mov x3, #0");                                          // resolve an ordinary object class name
    emitter.instruction("bl __rt_sprintf_warn_object_numeric");                 // emit or suppress PHP's object conversion warning
    emitter.instruction("b __rt_sprintf_mixed_int_one");                        // all objects normalize to one

    emitter.label("__rt_sprintf_mixed_int_callable");
    emitter.instruction("mov x0, #0");                                          // callable descriptors expose no object payload
    emitter.instruction("ldr x1, [sp, #8]");                                    // integer-versus-float warning kind
    emitter.instruction("ldr x2, [sp, #16]");                                   // optional eval context, unused for Closure
    emitter.instruction("mov x3, #1");                                          // select the fixed Closure class name
    emitter.instruction("bl __rt_sprintf_warn_object_numeric");                 // emit or suppress PHP's Closure conversion warning
    emitter.instruction("b __rt_sprintf_mixed_int_one");                        // callable descriptors normalize to one

    emitter.label("__rt_sprintf_mixed_int_one");
    emitter.instruction("mov x0, #1");                                          // object and callable numeric conversion result
    emitter.instruction("b __rt_sprintf_mixed_int_done");                       // share the frame teardown

    emitter.label("__rt_sprintf_mixed_int_resource");
    emitter.instruction("mov x0, x1");                                          // pass the native resource payload to the id registry
    emitter.instruction("bl __rt_resource_id_of");                              // return PHP's stable resource id instead of a handle/pointer
    emitter.instruction("b __rt_sprintf_mixed_int_done");                       // share the frame teardown

    emitter.label("__rt_sprintf_mixed_int_zero");
    emitter.instruction("mov x0, #0");                                          // empty/null container numeric conversion result
    emitter.label("__rt_sprintf_mixed_int_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release this helper's aligned frame
    emitter.instruction("ret");                                                 // return the normalized integer in x0
}

/// Emits AArch64 `__rt_sprintf_mixed_to_string(tag, payload, eval_ctx) -> (owner, ptr, len)`.
fn emit_sprintf_mixed_to_string_aarch64(emitter: &mut Emitter, eval_bridge: bool) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf mixed non-scalar to string ---");
    emitter.label_global("__rt_sprintf_mixed_to_string");

    // Locals: tag/payload/eval-context at 0/8/16, output ptr/len/owner at 24/32/40,
    // boxed eval input at 48, ownership flag at 56, eval result struct at 64..87,
    // original method pointer/status at 88, and concrete runtime tag at 104.
    emitter.instruction("sub sp, sp, #128");                                    // reserve spills and an eval result structure
    emitter.instruction("stp x29, x30, [sp, #112]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #112");                                   // establish this helper's frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save record tag and payload
    emitter.instruction("str x0, [sp, #104]");                                  // preserve the concrete tag for conversion errors
    emitter.instruction("str x2, [sp, #16]");                                   // save the optional eval context
    emitter.instruction("stp xzr, xzr, [sp, #48]");                             // no eval input box and no ownership yet
    emitter.instruction("cmp x0, #7");                                          // boxed Mixed record?
    emitter.instruction("b.ne __rt_sprintf_mixed_string_raw");                  // raw tags are already concrete
    emitter.instruction("str x1, [sp, #48]");                                   // preserve the borrowed box for eval fallback
    emitter.instruction("mov x0, x1");                                          // mixed_unbox consumes the box in x0
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose concrete tag and payload
    emitter.instruction("str x0, [sp, #104]");                                  // retain the unboxed tag for diagnostics
    emitter.instruction("str x1, [sp, #8]");                                    // retain the unboxed payload for diagnostics
    emitter.instruction("b __rt_sprintf_mixed_string_dispatch");                // apply concrete semantics
    emitter.label("__rt_sprintf_mixed_string_raw");
    emitter.instruction("cmp x0, #11");                                         // type-erased Iterable record?
    emitter.instruction("b.ne __rt_sprintf_mixed_string_dispatch");             // other raw tags are concrete
    emitter.instruction("mov x0, x1");                                          // classify the erased payload by heap kind
    emitter.instruction("bl __rt_heap_kind");                                   // 2/3 arrays, 4/6 objects
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the erased payload
    emitter.instruction("cmp x0, #2");                                          // indexed-array heap kind?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_array");                // render the PHP Array placeholder
    emitter.instruction("cmp x0, #3");                                          // associative-array heap kind?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_array");                // render the PHP Array placeholder
    emitter.instruction("cmp x0, #4");                                          // native-object heap kind?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_object");               // dispatch object string conversion
    emitter.instruction("cmp x0, #6");                                          // eval-object heap kind?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_object");               // dispatch object string conversion
    emitter.instruction("b __rt_sprintf_mixed_string_missing");                 // reject unknown erased values
    emitter.label("__rt_sprintf_mixed_string_dispatch");
    emitter.instruction("cmp x0, #4");                                          // indexed array?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_array");                // arrays stringify to the literal "Array"
    emitter.instruction("cmp x0, #5");                                          // associative array?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_array");                // hashes use the same PHP placeholder
    emitter.instruction("cmp x0, #6");                                          // object?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_object");               // dispatch dynamically through the class __toString table
    emitter.instruction("cmp x0, #9");                                          // resource?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_resource");             // resources use the public Resource id #N rendering
    emitter.instruction("b __rt_sprintf_mixed_string_missing");                 // callable/unknown values cannot be converted to string

    emitter.label("__rt_sprintf_mixed_string_array");
    emitter.instruction("mov x9, #4");                                          // normalize erased arrays to the indexed-array tag
    emitter.instruction("str x9, [sp, #104]");                                  // retain the normalized tag for diagnostics
    emitter.instruction("bl __rt_sprintf_warn_array_to_string");                // emit or suppress PHP's array stringification warning
    abi::emit_symbol_address(emitter, "x1", "_iterable_array_str");
    emitter.instruction("mov x2, #5");                                          // byte length of the literal "Array"
    emitter.instruction("mov x0, #0");                                          // fixed data is borrowed
    emitter.instruction("b __rt_sprintf_mixed_string_done");                    // return the borrowed fixed-data string pair

    emitter.label("__rt_sprintf_mixed_string_resource");
    emitter.instruction("mov x9, #9");                                          // normalize the concrete resource tag
    emitter.instruction("str x9, [sp, #104]");                                  // retain the normalized tag for diagnostics
    emitter.instruction("mov x0, x1");                                          // pass the native resource payload to the display helper
    emitter.instruction("bl __rt_resource_to_string");                          // return borrowed Resource id #N text in x1/x2
    emitter.instruction("mov x0, #0");                                          // resource display storage is borrowed
    emitter.instruction("b __rt_sprintf_mixed_string_done");                    // share the frame teardown

    emitter.label("__rt_sprintf_mixed_string_object");
    emitter.instruction("mov x9, #6");                                          // normalize erased objects to the object tag
    emitter.instruction("str x9, [sp, #104]");                                  // retain the normalized tag for diagnostics
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the receiver for eval fallback
    emitter.instruction("mov x0, x1");                                          // keep the borrowed object as the method receiver
    emitter.instruction("ldr x11, [x0]");                                       // load the object's dense runtime class id
    emitter.instruction("tbnz x11, #63, __rt_sprintf_mixed_string_eval");       // synthetic negative ids belong to eval
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_tostring_count", 0);
    emitter.instruction("cmp x11, x10");                                        // is the class id inside the generated table?
    emitter.instruction("b.hs __rt_sprintf_mixed_string_eval");                 // out-of-range classes may belong to eval
    abi::emit_symbol_address(emitter, "x10", "_class_tostring_ptrs");
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // resolve the concrete or inherited __toString symbol
    emitter.instruction("cbz x10, __rt_sprintf_mixed_string_eval");             // a missing native hook may still be eval-backed
    emitter.instruction("blr x10");                                             // call __toString with the borrowed receiver in x0
    emitter.instruction("b __rt_sprintf_mixed_string_own");                     // stabilize and own the method result

    emitter.label("__rt_sprintf_mixed_string_eval");
    if eval_bridge {
        emitter.instruction("mov x0, #6");                                      // runtime object tag
        emitter.instruction("ldr x1, [sp, #8]");                                // borrowed raw receiver
        emitter.instruction("mov x2, #0");                                      // objects have no high payload
        emitter.instruction("bl __rt_mixed_from_value");                        // retain receiver in a temporary box
        emitter.instruction("str x0, [sp, #48]");                               // save the owned box
        emitter.instruction("mov x9, x0");                                      // bridge object argument
        emitter.instruction("mov x10, #1");                                     // mark the temporary input box as owned
        emitter.instruction("str x10, [sp, #56]");                              // remember that the helper owns it
        emitter.label("__rt_sprintf_mixed_string_eval_call");
        emitter.instruction("stp xzr, xzr, [sp, #64]");                         // clear kind/padding/value
        emitter.instruction("str xzr, [sp, #80]");                              // clear throwable pointer
        emitter.instruction("ldr x0, [sp, #16]");                               // context ABI arg 0
        emitter.instruction("mov x1, x9");                                      // boxed value ABI arg 1
        emitter.instruction("add x2, sp, #64");                                 // result ABI arg 2
        let symbol = emitter.target.extern_symbol("__elephc_eval_string_context");
        abi::emit_call_label(emitter, &symbol);
        emitter.instruction("str x0, [sp, #88]");                               // preserve bridge status during input cleanup
        emitter.instruction("ldr x9, [sp, #56]");                               // load temporary-input ownership
        emitter.instruction("cbz x9, __rt_sprintf_mixed_string_eval_status");   // skip cleanup for borrowed input
        emitter.instruction("ldr x0, [sp, #48]");                               // owned temporary input box
        emitter.instruction("bl __rt_decref_any");                              // balance its retained receiver
        emitter.label("__rt_sprintf_mixed_string_eval_status");
        emitter.instruction("ldr x0, [sp, #88]");                               // bridge status
        emitter.instruction("cbz x0, __rt_sprintf_mixed_string_eval_ok");       // zero status carries a converted value
        emitter.instruction("cmp x0, #3");                                      // uncaught Throwable?
        emitter.instruction("b.eq __rt_sprintf_mixed_string_eval_throw");       // propagate an uncaught eval Throwable
        emitter.instruction("b __rt_sprintf_mixed_string_missing");             // reject other conversion failures
        emitter.label("__rt_sprintf_mixed_string_eval_ok");
        emitter.instruction("ldr x0, [sp, #72]");                               // boxed string result
        emitter.instruction("str x0, [sp, #48]");                               // preserve it through persistence
        emitter.instruction("bl __rt_mixed_unbox");                             // expose the converted string payload
        emitter.instruction("cmp x0, #1");                                      // bridge must return a string cell
        emitter.instruction("b.ne __rt_sprintf_mixed_string_eval_bad");         // reject a non-string bridge result
        emitter.instruction("stp x1, x2, [sp, #24]");                           // source pair for persistence
        emitter.instruction("bl __rt_str_persist");                             // formatter-owned stable copy
        emitter.instruction("stp x1, x2, [sp, #24]");                           // save the stabilized result pair
        emitter.instruction("str x1, [sp, #40]");                               // owner released after copy
        emitter.instruction("ldr x0, [sp, #48]");                               // reload the bridge-owned result box
        emitter.instruction("bl __rt_decref_any");                              // release the bridge-owned result cell
        emitter.instruction("b __rt_sprintf_mixed_string_return_owned");        // return the formatter-owned copy
        emitter.label("__rt_sprintf_mixed_string_eval_bad");
        emitter.instruction("ldr x0, [sp, #48]");                               // reload the invalid bridge result box
        emitter.instruction("bl __rt_decref_any");                              // release the invalid bridge result
        emitter.instruction("b __rt_sprintf_mixed_string_missing");             // report the failed string conversion
        emitter.label("__rt_sprintf_mixed_string_eval_throw");
        emitter.instruction("ldr x0, [sp, #80]");                               // boxed Throwable
        emitter.instruction("bl __rt_mixed_unbox");                             // raw object pointer in x1
        abi::emit_store_reg_to_symbol(emitter, "x1", "_exc_value", 0);
        emitter.instruction("b __rt_throw_current");                            // unwind to the nearest active PHP catch handler
    } else {
        emitter.instruction("b __rt_sprintf_mixed_string_missing");             // no eval bridge in this runtime
    }

    emitter.label("__rt_sprintf_mixed_string_own");
    emitter.instruction("str x1, [sp, #88]");                                   // preserve the original method pointer
    emitter.instruction("bl __rt_str_persist");                                 // make it independent of method scratch/ownership
    emitter.instruction("stp x1, x2, [sp, #24]");                               // save the stabilized method result pair
    emitter.instruction("str x1, [sp, #40]");                                   // formatter owns the stabilized result
    emitter.instruction("ldr x9, [sp, #88]");                                   // original method result pointer
    emitter.instruction("cmp x1, x9");                                          // did persist take over the same concat block?
    emitter.instruction("b.eq __rt_sprintf_mixed_string_return_owned");         // yes, it is already the formatter owner
    emitter.instruction("mov x0, x9");                                          // release an independently owned method result
    emitter.instruction("bl __rt_heap_free_safe");                              // borrowed/static pointers are ignored safely
    emitter.instruction("b __rt_sprintf_mixed_string_return_owned");            // return the surviving owned string

    emitter.label("__rt_sprintf_mixed_string_return_owned");
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // stabilized result pair
    emitter.instruction("ldr x0, [sp, #40]");                                   // owned pointer for post-copy release
    emitter.instruction("b __rt_sprintf_mixed_string_done");                    // share the helper frame teardown

    emitter.label("__rt_sprintf_mixed_string_missing");
    emitter.instruction("ldr x9, [sp, #104]");                                  // concrete runtime tag for the failing value
    emitter.instruction("ldr x10, [sp, #8]");                                   // raw object payload when the tag is object
    emitter.instruction("cmp x9, #6");                                          // is this a native object payload?
    emitter.instruction("csel x0, x10, xzr, eq");                               // pass it only when class metadata can be inspected
    emitter.instruction("cmp x9, #10");                                         // callable descriptors report class Closure
    emitter.instruction("cset x1, eq");                                         // second argument selects the fixed Closure class name
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore the caller frame before throwing
    emitter.instruction("add sp, sp, #128");                                    // release the coercion helper frame
    emitter.instruction("b __rt_sprintf_throw_string_error");                   // construct and throw PHP's catchable Error
    emitter.label("__rt_sprintf_mixed_string_done");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #128");                                    // release this helper's aligned frame
    emitter.instruction("ret");                                                 // return owner/string pair in x0/x1/x2
}

/// Emits both Linux x86_64 deferred non-scalar coercion helpers.
fn emit_sprintf_mixed_casts_linux_x86_64(emitter: &mut Emitter, eval_bridge: bool) {
    emit_sprintf_mixed_to_int_linux_x86_64(emitter);
    emit_sprintf_mixed_to_string_linux_x86_64(emitter, eval_bridge);
}

/// Emits x86_64 `__rt_sprintf_mixed_to_int(tag, payload, numeric_kind, eval_ctx) -> int`.
fn emit_sprintf_mixed_to_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf mixed non-scalar to int ---");
    emitter.label_global("__rt_sprintf_mixed_to_int");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 32");                                         // preserve payload, conversion kind, and eval context
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve raw payload across heap classification
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve integer-versus-float warning kind
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // preserve the optional eval context
    emitter.instruction("cmp rdi, 7");                                          // is the record payload a boxed Mixed cell?
    emitter.instruction("je __rt_sprintf_mixed_int_unbox_x64");                 // inspect the concrete boxed tag
    emitter.instruction("cmp rdi, 11");                                         // type-erased Iterable record?
    emitter.instruction("je __rt_sprintf_mixed_int_iterable_x64");              // classify it by heap kind
    emitter.instruction("jmp __rt_sprintf_mixed_int_dispatch_x64");             // other raw tags are concrete
    emitter.label("__rt_sprintf_mixed_int_unbox_x64");
    emitter.instruction("mov rax, rsi");                                        // __rt_mixed_unbox reads its boxed pointer from rax
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested Mixed cells before inspecting the concrete tag
    emitter.instruction("jmp __rt_sprintf_mixed_int_dispatch_ready_x64");       // rax/rdi now carry tag/payload
    emitter.label("__rt_sprintf_mixed_int_iterable_x64");
    emitter.instruction("mov rax, rsi");                                        // pass the erased payload to heap classification
    emitter.instruction("call __rt_heap_kind");                                 // recover its concrete runtime heap kind
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the erased payload for conversion
    emitter.instruction("cmp rax, 2");                                          // indexed-array heap kind?
    emitter.instruction("je __rt_sprintf_mixed_int_array_x64");                 // apply PHP array numeric conversion
    emitter.instruction("cmp rax, 3");                                          // associative-array heap kind?
    emitter.instruction("je __rt_sprintf_mixed_int_array_x64");                 // apply PHP array numeric conversion
    emitter.instruction("cmp rax, 4");                                          // native-object heap kind?
    emitter.instruction("je __rt_sprintf_mixed_int_object_x64");                // warn and normalize the object to one
    emitter.instruction("cmp rax, 6");                                          // eval-object heap kind?
    emitter.instruction("je __rt_sprintf_mixed_int_object_x64");                // warn and normalize the object to one
    emitter.instruction("jmp __rt_sprintf_mixed_int_zero_x64");                 // unknown erased values normalize to zero
    emitter.label("__rt_sprintf_mixed_int_dispatch_x64");
    emitter.instruction("mov rax, rdi");                                        // raw record tag
    emitter.instruction("mov rdi, rsi");                                        // raw record payload
    emitter.label("__rt_sprintf_mixed_int_dispatch_ready_x64");
    emitter.instruction("cmp rax, 4");                                          // indexed array?
    emitter.instruction("je __rt_sprintf_mixed_int_array_x64");                 // arrays cast to zero or one by emptiness
    emitter.instruction("cmp rax, 5");                                          // associative array?
    emitter.instruction("je __rt_sprintf_mixed_int_array_x64");                 // hashes share PHP's array numeric cast
    emitter.instruction("cmp rax, 6");                                          // object?
    emitter.instruction("je __rt_sprintf_mixed_int_object_x64");                // every object warns and casts to integer one
    emitter.instruction("cmp rax, 9");                                          // resource?
    emitter.instruction("je __rt_sprintf_mixed_int_resource_x64");              // resources format through their public resource id
    emitter.instruction("cmp rax, 10");                                         // callable descriptor (Closure shape)?
    emitter.instruction("je __rt_sprintf_mixed_int_callable_x64");              // callable objects warn as Closure and cast to one
    emitter.instruction("xor eax, eax");                                        // unknown non-scalars normalize to zero, never their raw payload
    emitter.instruction("jmp __rt_sprintf_mixed_int_done_x64");                 // restore the helper frame and return

    emitter.label("__rt_sprintf_mixed_int_array_x64");
    emitter.instruction("test rdi, rdi");                                       // null container pointers behave like empty arrays
    emitter.instruction("jz __rt_sprintf_mixed_int_zero_x64");                  // skip the header load for a null container
    emitter.instruction("cmp QWORD PTR [rdi], 0");                              // is the live container element count zero?
    emitter.instruction("setne al");                                            // non-empty arrays cast to one
    emitter.instruction("movzx rax, al");                                       // normalize the boolean byte to a full integer result
    emitter.instruction("jmp __rt_sprintf_mixed_int_done_x64");                 // share the frame teardown

    emitter.label("__rt_sprintf_mixed_int_object_x64");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // integer-versus-float warning kind
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // optional eval context for dynamic metadata
    emitter.instruction("xor ecx, ecx");                                        // resolve an ordinary object class name
    emitter.instruction("call __rt_sprintf_warn_object_numeric_x64");           // emit or suppress PHP's object conversion warning
    emitter.instruction("jmp __rt_sprintf_mixed_int_one_x64");                  // all objects normalize to one

    emitter.label("__rt_sprintf_mixed_int_callable_x64");
    emitter.instruction("xor edi, edi");                                        // callable descriptors expose no object payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // integer-versus-float warning kind
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // optional eval context, unused for Closure
    emitter.instruction("mov ecx, 1");                                          // select the fixed Closure class name
    emitter.instruction("call __rt_sprintf_warn_object_numeric_x64");           // emit or suppress PHP's Closure conversion warning
    emitter.instruction("jmp __rt_sprintf_mixed_int_one_x64");                  // callable descriptors normalize to one

    emitter.label("__rt_sprintf_mixed_int_one_x64");
    emitter.instruction("mov eax, 1");                                          // object and callable numeric conversion result
    emitter.instruction("jmp __rt_sprintf_mixed_int_done_x64");                 // share the frame teardown

    emitter.label("__rt_sprintf_mixed_int_resource_x64");
    emitter.instruction("mov rax, rdi");                                        // pass the native resource payload to the id registry
    emitter.instruction("call __rt_resource_id_of");                            // return PHP's stable resource id instead of a handle/pointer
    emitter.instruction("jmp __rt_sprintf_mixed_int_done_x64");                 // share the frame teardown

    emitter.label("__rt_sprintf_mixed_int_zero_x64");
    emitter.instruction("xor eax, eax");                                        // empty/null container numeric conversion result
    emitter.label("__rt_sprintf_mixed_int_done_x64");
    emitter.instruction("leave");                                               // release the helper frame and restore rbp
    emitter.instruction("ret");                                                 // return the normalized integer in rax
}

/// Emits x86_64 `__rt_sprintf_mixed_to_string(tag, payload, eval_ctx) -> (ptr, len, owner)`.
fn emit_sprintf_mixed_to_string_linux_x86_64(emitter: &mut Emitter, eval_bridge: bool) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf mixed non-scalar to string ---");
    emitter.label_global("__rt_sprintf_mixed_to_string");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 112");                                        // aligned spills, concrete tag, and eval result structure
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save record tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save record payload
    emitter.instruction("mov QWORD PTR [rbp - 104], rdi");                      // preserve the concrete tag for conversion errors
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save optional eval context
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // no eval input box yet
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // eval input is borrowed by default
    emitter.instruction("cmp rdi, 7");                                          // boxed Mixed record?
    emitter.instruction("jne __rt_sprintf_mixed_string_raw_x64");               // raw tags are already concrete
    emitter.instruction("mov QWORD PTR [rbp - 56], rsi");                       // preserve borrowed box for eval fallback
    emitter.instruction("mov rax, rsi");                                        // mixed_unbox consumes the box in rax
    emitter.instruction("call __rt_mixed_unbox");                               // concrete tag rax, payload rdi
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // retain the unboxed tag for diagnostics
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // retain the unboxed payload for diagnostics
    emitter.instruction("jmp __rt_sprintf_mixed_string_dispatch_x64");          // apply concrete semantics
    emitter.label("__rt_sprintf_mixed_string_raw_x64");
    emitter.instruction("cmp rdi, 11");                                         // type-erased Iterable record?
    emitter.instruction("jne __rt_sprintf_mixed_string_raw_ready_x64");         // other raw tags are already concrete
    emitter.instruction("mov rax, rsi");                                        // erased payload for heap classification
    emitter.instruction("call __rt_heap_kind");                                 // recover the erased payload's heap kind
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // restore erased payload
    emitter.instruction("cmp rax, 2");                                          // indexed-array heap kind?
    emitter.instruction("je __rt_sprintf_mixed_string_array_x64");              // render the PHP Array placeholder
    emitter.instruction("cmp rax, 3");                                          // associative-array heap kind?
    emitter.instruction("je __rt_sprintf_mixed_string_array_x64");              // render the PHP Array placeholder
    emitter.instruction("cmp rax, 4");                                          // native-object heap kind?
    emitter.instruction("je __rt_sprintf_mixed_string_object_x64");             // dispatch object string conversion
    emitter.instruction("cmp rax, 6");                                          // eval-object heap kind?
    emitter.instruction("je __rt_sprintf_mixed_string_object_x64");             // dispatch object string conversion
    emitter.instruction("jmp __rt_sprintf_mixed_string_missing_x64");           // reject unknown erased values
    emitter.label("__rt_sprintf_mixed_string_raw_ready_x64");
    emitter.instruction("mov rax, rdi");                                        // raw record tag
    emitter.instruction("mov rdi, rsi");                                        // raw record payload
    emitter.label("__rt_sprintf_mixed_string_dispatch_x64");
    emitter.instruction("cmp rax, 4");                                          // indexed array?
    emitter.instruction("je __rt_sprintf_mixed_string_array_x64");              // arrays stringify to the literal "Array"
    emitter.instruction("cmp rax, 5");                                          // associative array?
    emitter.instruction("je __rt_sprintf_mixed_string_array_x64");              // hashes use the same PHP placeholder
    emitter.instruction("cmp rax, 6");                                          // object?
    emitter.instruction("je __rt_sprintf_mixed_string_object_x64");             // dispatch dynamically through the class __toString table
    emitter.instruction("cmp rax, 9");                                          // resource?
    emitter.instruction("je __rt_sprintf_mixed_string_resource_x64");           // resources use the public Resource id #N rendering
    emitter.instruction("jmp __rt_sprintf_mixed_string_missing_x64");           // callable/unknown values cannot be converted to string

    emitter.label("__rt_sprintf_mixed_string_array_x64");
    emitter.instruction("mov QWORD PTR [rbp - 104], 4");                        // normalize erased arrays to the indexed-array tag
    emitter.instruction("call __rt_sprintf_warn_array_to_string_x64");          // emit or suppress PHP's array stringification warning
    abi::emit_symbol_address(emitter, "rax", "_iterable_array_str");
    emitter.instruction("mov edx, 5");                                          // byte length of the literal "Array"
    emitter.instruction("xor ecx, ecx");                                        // fixed data is borrowed
    emitter.instruction("jmp __rt_sprintf_mixed_string_done_x64");              // return the borrowed fixed-data string pair

    emitter.label("__rt_sprintf_mixed_string_resource_x64");
    emitter.instruction("mov QWORD PTR [rbp - 104], 9");                        // normalize the concrete resource tag
    emitter.instruction("mov rax, rdi");                                        // pass the native resource payload to the display helper
    emitter.instruction("call __rt_resource_to_string");                        // return borrowed Resource id #N text in rax/rdx
    emitter.instruction("xor ecx, ecx");                                        // resource display storage is borrowed
    emitter.instruction("jmp __rt_sprintf_mixed_string_done_x64");              // share the frame teardown

    emitter.label("__rt_sprintf_mixed_string_object_x64");
    emitter.instruction("mov QWORD PTR [rbp - 104], 6");                        // normalize erased objects to the object tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // preserve receiver for eval fallback
    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // load the object's dense runtime class id
    emitter.instruction("test r8, r8");                                         // reject synthetic negative class ids
    emitter.instruction("js __rt_sprintf_mixed_string_eval_x64");               // synthetic ids belong to eval
    emitter.instruction("cmp r8, QWORD PTR [rip + _class_tostring_count]");     // is the class id inside the generated table?
    emitter.instruction("jae __rt_sprintf_mixed_string_eval_x64");              // out-of-range classes may belong to eval
    emitter.instruction("lea r10, [rip + _class_tostring_ptrs]");               // address the dense __toString function-pointer table
    emitter.instruction("mov r10, QWORD PTR [r10 + r8 * 8]");                   // resolve the concrete or inherited __toString symbol
    emitter.instruction("test r10, r10");                                       // did the class publish a conversion method?
    emitter.instruction("jz __rt_sprintf_mixed_string_eval_x64");               // a missing native hook may still be eval-backed
    emitter.instruction("call r10");                                            // call __toString with the borrowed receiver in rdi
    emitter.instruction("jmp __rt_sprintf_mixed_string_own_x64");               // stabilize and own the method result

    emitter.label("__rt_sprintf_mixed_string_eval_x64");
    if eval_bridge {
        emitter.instruction("mov eax, 6");                                      // runtime object tag
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // borrowed raw receiver
        emitter.instruction("xor esi, esi");                                    // objects have no high payload
        emitter.instruction("call __rt_mixed_from_value");                      // retain receiver in a temporary box
        emitter.instruction("mov QWORD PTR [rbp - 56], rax");                   // save owned input box
        emitter.instruction("mov r9, rax");                                     // pass the temporary box to the eval bridge
        emitter.instruction("mov QWORD PTR [rbp - 64], 1");                     // helper owns this box
        emitter.label("__rt_sprintf_mixed_string_eval_call_x64");
        emitter.instruction("mov QWORD PTR [rbp - 96], 0");                     // clear result kind/padding
        emitter.instruction("mov QWORD PTR [rbp - 88], 0");                     // clear value result pointer
        emitter.instruction("mov QWORD PTR [rbp - 80], 0");                     // clear throwable result pointer
        emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                   // context ABI arg 0
        emitter.instruction("mov rsi, r9");                                     // boxed value ABI arg 1
        emitter.instruction("lea rdx, [rbp - 96]");                             // result ABI arg 2
        let symbol = emitter.target.extern_symbol("__elephc_eval_string_context");
        abi::emit_call_label(emitter, &symbol);
        emitter.instruction("mov QWORD PTR [rbp - 8], rax");                    // preserve bridge status during input cleanup
        emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                     // does this helper own the input box?
        emitter.instruction("je __rt_sprintf_mixed_string_eval_status_x64");    // borrowed input needs no cleanup
        emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                   // owned temporary input box
        emitter.instruction("call __rt_decref_any");                            // balance retained receiver
        emitter.label("__rt_sprintf_mixed_string_eval_status_x64");
        emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                    // bridge status
        emitter.instruction("test rax, rax");                                   // did the bridge report success?
        emitter.instruction("jz __rt_sprintf_mixed_string_eval_ok_x64");        // zero status carries a converted value
        emitter.instruction("cmp rax, 3");                                      // uncaught Throwable?
        emitter.instruction("je __rt_sprintf_mixed_string_eval_throw_x64");     // propagate an uncaught eval Throwable
        emitter.instruction("jmp __rt_sprintf_mixed_string_missing_x64");       // reject other conversion failures
        emitter.label("__rt_sprintf_mixed_string_eval_ok_x64");
        emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                   // boxed string result
        emitter.instruction("mov QWORD PTR [rbp - 56], rax");                   // preserve through persistence
        emitter.instruction("call __rt_mixed_unbox");                           // expose the converted string payload
        emitter.instruction("cmp rax, 1");                                      // bridge must return a string cell
        emitter.instruction("jne __rt_sprintf_mixed_string_eval_bad_x64");      // reject a non-string bridge result
        emitter.instruction("mov rax, rdi");                                    // standard string pointer register
        emitter.instruction("call __rt_str_persist");                           // formatter-owned stable copy
        emitter.instruction("mov QWORD PTR [rbp - 32], rax");                   // save the stabilized result pointer
        emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                   // save the stabilized result length
        emitter.instruction("mov QWORD PTR [rbp - 48], rax");                   // owner released after copy
        emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                   // reload the bridge-owned result box
        emitter.instruction("call __rt_decref_any");                            // release bridge-owned result cell
        emitter.instruction("jmp __rt_sprintf_mixed_string_return_owned_x64");  // return the formatter-owned copy
        emitter.label("__rt_sprintf_mixed_string_eval_bad_x64");
        emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                   // reload the invalid bridge result box
        emitter.instruction("call __rt_decref_any");                            // release the invalid bridge result
        emitter.instruction("jmp __rt_sprintf_mixed_string_missing_x64");       // report the failed string conversion
        emitter.label("__rt_sprintf_mixed_string_eval_throw_x64");
        emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                   // boxed Throwable
        emitter.instruction("call __rt_mixed_unbox");                           // raw object pointer in rdi
        abi::emit_store_reg_to_symbol(emitter, "rdi", "_exc_value", 0);
        emitter.instruction("jmp __rt_throw_current");                          // unwind to the nearest active PHP catch handler
    } else {
        emitter.instruction("jmp __rt_sprintf_mixed_string_missing_x64");       // no eval bridge in this runtime
    }

    emitter.label("__rt_sprintf_mixed_string_own_x64");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve original method result pointer
    emitter.instruction("call __rt_str_persist");                               // stabilize concat/heap/static results
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the stabilized method result pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the stabilized method result length
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // formatter owns stabilized result
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // did persist take over the same block?
    emitter.instruction("je __rt_sprintf_mixed_string_return_owned_x64");       // persist already owns the original block
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // independently owned original result
    emitter.instruction("call __rt_heap_free_safe");                            // borrowed/static pointers are ignored safely
    emitter.label("__rt_sprintf_mixed_string_return_owned_x64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // stabilized result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // stabilized result length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // owner for post-copy release
    emitter.instruction("jmp __rt_sprintf_mixed_string_done_x64");              // share the helper frame teardown

    emitter.label("__rt_sprintf_mixed_string_missing_x64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 104]");                       // concrete runtime tag for the failing value
    emitter.instruction("xor edi, edi");                                        // default to the generic object class spelling
    emitter.instruction("cmp r8, 6");                                           // is this a native object payload?
    emitter.instruction("jne __rt_sprintf_mixed_string_error_kind_x64");        // no object metadata is safe to inspect
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the native object payload for class lookup
    emitter.label("__rt_sprintf_mixed_string_error_kind_x64");
    emitter.instruction("xor esi, esi");                                        // ordinary objects use resolved/fallback metadata
    emitter.instruction("cmp r8, 10");                                          // callable descriptors report class Closure
    emitter.instruction("sete sil");                                            // second argument selects the fixed Closure class name
    emitter.instruction("leave");                                               // restore the caller frame before throwing
    emitter.instruction("jmp __rt_sprintf_throw_string_error_x64");             // construct and throw PHP's catchable Error
    emitter.label("__rt_sprintf_mixed_string_done_x64");
    emitter.instruction("leave");                                               // release the helper frame and restore rbp
    emitter.instruction("ret");                                                 // return the string pair in rax/rdx
}

/// Emits AArch64 warning helpers for array-to-string and object-to-number coercions.
fn emit_sprintf_warnings_aarch64(emitter: &mut Emitter, eval_bridge: bool) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf coercion warnings ---");
    emitter.label_global("__rt_sprintf_warn_array_to_string");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve frame linkage across the diagnostic call
    emitter.instruction("mov x29, sp");                                         // establish an aligned warning frame
    abi::emit_symbol_address(emitter, "x1", "_diag_sprintf_array_to_string");
    emitter.instruction(&format!("mov x2, #{}", SPRINTF_ARRAY_TO_STRING_WARNING.len())); // exact PHP warning byte length
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the array conversion warning
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame linkage
    emitter.instruction("ret");                                                 // resume string formatting with the literal Array

    emitter.label_global("__rt_sprintf_warn_object_numeric");
    emitter.instruction("sub sp, sp, #64");                                     // reserve conversion kind, class metadata, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish an aligned warning frame
    emitter.instruction("str x1, [sp]");                                        // preserve integer-versus-float warning kind
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the object payload for eval metadata lookup
    emitter.instruction("str x2, [sp, #32]");                                   // preserve the optional eval context
    emitter.instruction("cbnz x3, __rt_sprintf_numeric_warning_closure");       // callable descriptors report the Closure class
    emitter.instruction("cbz x0, __rt_sprintf_numeric_warning_fallback");       // missing metadata uses the generic object spelling
    if eval_bridge {
        emitter.instruction("ldr x0, [sp, #32]");                               // optional caller context for the bridge
        emitter.instruction("ldr x1, [sp, #24]");                               // raw object identity for owner-context lookup
        emitter.instruction("ldr x2, [sp]");                                    // integer-versus-float warning kind
        let symbol = emitter
            .target
            .extern_symbol("__elephc_eval_sprintf_numeric_warning");
        abi::emit_call_label(emitter, &symbol);
        emitter.instruction("cbz x0, __rt_sprintf_numeric_warning_done");       // the bridge emitted the exact dynamic class warning
        emitter.instruction("ldr x0, [sp, #24]");                               // restore the native object candidate after bridge failure
    }
    emitter.instruction("ldr x13, [x0]");                                       // load the native object's dense runtime class id
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_name_count", 0);
    emitter.instruction("cmp x13, x10");                                        // is the class id within the dense name table?
    emitter.instruction("b.hs __rt_sprintf_numeric_warning_fallback");          // malformed ids use the generic spelling
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x13, lsl #4");                           // select the static class-name row
    emitter.instruction("ldp x11, x12, [x10]");                                 // borrow the class-name pointer and byte length
    emitter.instruction("cbnz x12, __rt_sprintf_numeric_warning_name_ready");   // use non-empty generated metadata
    emitter.instruction("b __rt_sprintf_numeric_warning_fallback");             // empty metadata uses the generic spelling
    emitter.label("__rt_sprintf_numeric_warning_closure");
    abi::emit_symbol_address(emitter, "x11", "_sprintf_closure_class_name");
    emitter.instruction("mov x12, #7");                                         // byte length of Closure
    emitter.instruction("b __rt_sprintf_numeric_warning_name_ready");           // join warning emission
    emitter.label("__rt_sprintf_numeric_warning_fallback");
    abi::emit_symbol_address(emitter, "x11", "_unser_type_object");
    emitter.instruction("mov x12, #6");                                         // byte length of the generic object spelling
    emitter.label("__rt_sprintf_numeric_warning_name_ready");
    emitter.instruction("stp x11, x12, [sp, #8]");                              // preserve class-name metadata across warning fragments
    abi::emit_symbol_address(emitter, "x1", "_diag_sprintf_object_numeric_prefix");
    emitter.instruction(&format!("mov x2, #{}", SPRINTF_OBJECT_NUMERIC_WARNING_PREFIX.len())); // warning prefix byte length
    emitter.instruction("bl __rt_diag_warning_fragment");                       // emit or suppress the warning prefix
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload the class-name fragment
    emitter.instruction("bl __rt_diag_warning_fragment");                       // emit or suppress the resolved class name
    emitter.instruction("ldr x9, [sp]");                                        // reload integer-versus-float warning kind
    emitter.instruction("cbnz x9, __rt_sprintf_numeric_warning_float");         // non-zero selects the float suffix
    abi::emit_symbol_address(emitter, "x1", "_diag_sprintf_object_to_int_suffix");
    emitter.instruction(&format!("mov x2, #{}", SPRINTF_OBJECT_TO_INT_WARNING_SUFFIX.len())); // integer warning suffix byte length
    emitter.instruction("b __rt_sprintf_numeric_warning_suffix_ready");         // join the final diagnostic call
    emitter.label("__rt_sprintf_numeric_warning_float");
    abi::emit_symbol_address(emitter, "x1", "_diag_sprintf_object_to_float_suffix");
    emitter.instruction(&format!("mov x2, #{}", SPRINTF_OBJECT_TO_FLOAT_WARNING_SUFFIX.len())); // float warning suffix byte length
    emitter.label("__rt_sprintf_numeric_warning_suffix_ready");
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the warning suffix and newline
    emitter.label("__rt_sprintf_numeric_warning_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame linkage
    emitter.instruction("add sp, sp, #64");                                     // release warning state
    emitter.instruction("ret");                                                 // return to numeric normalization
}

/// Emits Linux x86_64 warning helpers for array-to-string and object-to-number coercions.
fn emit_sprintf_warnings_linux_x86_64(emitter: &mut Emitter, eval_bridge: bool) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf coercion warnings ---");
    emitter.label_global("__rt_sprintf_warn_array_to_string_x64");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // align the stack for the diagnostic call
    emitter.instruction("lea rdi, [rip + _diag_sprintf_array_to_string]");      // fixed array conversion warning pointer
    emitter.instruction(&format!("mov esi, {}", SPRINTF_ARRAY_TO_STRING_WARNING.len())); // exact PHP warning byte length
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the array conversion warning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // resume string formatting with the literal Array

    emitter.label_global("__rt_sprintf_warn_object_numeric_x64");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned warning frame
    emitter.instruction("sub rsp, 48");                                         // reserve conversion kind, class metadata, object, and context
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve integer-versus-float warning kind
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // preserve the object payload for eval metadata lookup
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the optional eval context
    emitter.instruction("test rcx, rcx");                                       // callable descriptors use a fixed class spelling
    emitter.instruction("jnz __rt_sprintf_numeric_warning_closure_x64");        // select Closure without inspecting a descriptor
    emitter.instruction("test rdi, rdi");                                       // is native object metadata available?
    emitter.instruction("jz __rt_sprintf_numeric_warning_fallback_x64");        // no, use the generic object spelling
    if eval_bridge {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                   // optional caller context for the bridge
        emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                   // raw object identity for owner-context lookup
        emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                    // integer-versus-float warning kind
        let symbol = emitter
            .target
            .extern_symbol("__elephc_eval_sprintf_numeric_warning");
        abi::emit_call_label(emitter, &symbol);
        emitter.instruction("test eax, eax");                                   // did the bridge emit the dynamic class warning?
        emitter.instruction("jz __rt_sprintf_numeric_warning_done_x64");        // yes, avoid emitting a native duplicate
        emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                   // restore the native object candidate after bridge failure
    }
    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // load the object's dense runtime class id
    emitter.instruction("cmp r8, QWORD PTR [rip + _class_name_count]");         // is the class id within the dense name table?
    emitter.instruction("jae __rt_sprintf_numeric_warning_fallback_x64");       // malformed ids use the generic spelling
    emitter.instruction("lea r9, [rip + _class_name_entries]");                 // class-name metadata base
    emitter.instruction("shl r8, 4");                                           // scale the id by the pointer/length row width
    emitter.instruction("add r9, r8");                                          // select the static class-name row
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // borrow the class-name pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // borrow the class-name byte length
    emitter.instruction("test r11, r11");                                       // empty metadata falls back to object
    emitter.instruction("jnz __rt_sprintf_numeric_warning_name_ready_x64");     // use non-empty generated metadata
    emitter.instruction("jmp __rt_sprintf_numeric_warning_fallback_x64");       // select the generic spelling
    emitter.label("__rt_sprintf_numeric_warning_closure_x64");
    emitter.instruction("lea r10, [rip + _sprintf_closure_class_name]");        // fixed PHP class name for callable descriptors
    emitter.instruction("mov r11, 7");                                          // byte length of Closure
    emitter.instruction("jmp __rt_sprintf_numeric_warning_name_ready_x64");     // join warning emission
    emitter.label("__rt_sprintf_numeric_warning_fallback_x64");
    emitter.instruction("lea r10, [rip + _unser_type_object]");                 // generic object class spelling
    emitter.instruction("mov r11, 6");                                          // byte length of object
    emitter.label("__rt_sprintf_numeric_warning_name_ready_x64");
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve class-name pointer across warning fragments
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // preserve class-name byte length
    emitter.instruction("lea rdi, [rip + _diag_sprintf_object_numeric_prefix]"); // warning prefix pointer
    emitter.instruction(&format!("mov esi, {}", SPRINTF_OBJECT_NUMERIC_WARNING_PREFIX.len())); // warning prefix byte length
    emitter.instruction("call __rt_diag_warning_fragment");                     // emit or suppress the warning prefix
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the class-name byte length
    emitter.instruction("call __rt_diag_warning_fragment");                     // emit or suppress the resolved class name
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // integer or float warning wording?
    emitter.instruction("jne __rt_sprintf_numeric_warning_float_x64");          // non-zero selects the float suffix
    emitter.instruction("lea rdi, [rip + _diag_sprintf_object_to_int_suffix]"); // integer warning suffix pointer
    emitter.instruction(&format!("mov esi, {}", SPRINTF_OBJECT_TO_INT_WARNING_SUFFIX.len())); // integer warning suffix byte length
    emitter.instruction("jmp __rt_sprintf_numeric_warning_suffix_ready_x64");   // join the final diagnostic call
    emitter.label("__rt_sprintf_numeric_warning_float_x64");
    emitter.instruction("lea rdi, [rip + _diag_sprintf_object_to_float_suffix]"); // float warning suffix pointer
    emitter.instruction(&format!("mov esi, {}", SPRINTF_OBJECT_TO_FLOAT_WARNING_SUFFIX.len())); // float warning suffix byte length
    emitter.label("__rt_sprintf_numeric_warning_suffix_ready_x64");
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the warning suffix and newline
    emitter.label("__rt_sprintf_numeric_warning_done_x64");
    emitter.instruction("leave");                                               // release warning state and restore rbp
    emitter.instruction("ret");                                                 // return to numeric normalization
}

/// Emits the AArch64 helper that throws PHP's catchable object-to-string `Error`.
///
/// Input: `x0` is a native object payload or zero, and `x1 != 0` selects the
/// fixed `Closure` class name for callable descriptors. The helper never returns.
fn emit_sprintf_string_error_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf object string-conversion Error ---");
    emitter.label_global("__rt_sprintf_throw_string_error");
    emitter.instruction("sub sp, sp, #64");                                     // reserve class-name, message, and frame state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish an aligned Error-construction frame
    emitter.instruction("cbnz x1, __rt_sprintf_string_error_closure");          // callable descriptors report the Closure class
    emitter.instruction("cbz x0, __rt_sprintf_string_error_fallback");          // missing object metadata uses a generic class spelling
    emitter.instruction("ldr x13, [x0]");                                       // load the native object's dense runtime class id
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_name_count", 0);
    emitter.instruction("cmp x13, x10");                                        // is the class id within the dense name table?
    emitter.instruction("b.hs __rt_sprintf_string_error_fallback");             // malformed or eval ids use the generic spelling
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x13, lsl #4");                           // select the static class-name row
    emitter.instruction("ldp x11, x12, [x10]");                                 // borrow the class-name pointer and byte length
    emitter.instruction("cbnz x12, __rt_sprintf_string_error_name_ready");      // use non-empty generated metadata
    emitter.instruction("b __rt_sprintf_string_error_fallback");                // empty metadata uses the generic spelling
    emitter.label("__rt_sprintf_string_error_closure");
    abi::emit_symbol_address(emitter, "x11", "_sprintf_closure_class_name");
    emitter.instruction("mov x12, #7");                                         // byte length of Closure
    emitter.instruction("b __rt_sprintf_string_error_name_ready");              // join message construction
    emitter.label("__rt_sprintf_string_error_fallback");
    abi::emit_symbol_address(emitter, "x11", "_unser_type_object");
    emitter.instruction("mov x12, #6");                                         // byte length of the generic object spelling
    emitter.label("__rt_sprintf_string_error_name_ready");
    emitter.instruction("stp x11, x12, [sp]");                                  // preserve class-name metadata across concat
    abi::emit_symbol_address(emitter, "x1", "_unser_object_string_error_prefix");
    emitter.instruction(&format!("mov x2, #{}", UNSER_OBJECT_STRING_ERROR_PREFIX.len())); // prefix byte length
    emitter.instruction("ldp x3, x4, [sp]");                                    // append the resolved class name
    emitter.instruction("bl __rt_concat");                                      // build `Object of class <Class>`
    abi::emit_symbol_address(emitter, "x3", "_unser_object_string_error_suffix");
    emitter.instruction(&format!("mov x4, #{}", UNSER_OBJECT_STRING_ERROR_SUFFIX.len())); // suffix byte length
    emitter.instruction("bl __rt_concat");                                      // append PHP's conversion failure suffix
    emitter.instruction("bl __rt_str_persist");                                 // give the Error stable message ownership
    emitter.instruction("stp x1, x2, [sp, #16]");                               // preserve the message pair across allocation
    emitter.instruction("mov x0, #56");                                         // canonical Throwable payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the Error object payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 identifies a throwable object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the runtime object heap header
    emitter.instruction("bl __rt_object_handle_acquire");                       // assign the Error's PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "x9", "_spl_error_class_id", 0);
    emitter.instruction("str x9, [x0]");                                        // stamp the per-program Error class id
    emitter.instruction("ldp x10, x11, [sp, #16]");                             // recover the persisted message pair
    emitter.instruction("stp x10, x11, [x0, #8]");                              // store Error message pointer and length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("str xzr, [x0, #40]");                                  // previous Throwable defaults to null
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame linkage before unwinding
    emitter.instruction("add sp, sp, #64");                                     // release Error-construction state
    emitter.instruction("b __rt_throw_current");                                // enter the standard catchable exception path
}

/// Emits the Linux x86_64 helper that throws PHP's catchable object-to-string `Error`.
///
/// Input: `rdi` is a native object payload or zero, and `rsi != 0` selects the
/// fixed `Closure` class name for callable descriptors. The helper never returns.
fn emit_sprintf_string_error_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf object string-conversion Error ---");
    emitter.label_global("__rt_sprintf_throw_string_error_x64");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned Error-construction frame
    emitter.instruction("sub rsp, 48");                                         // reserve class-name and message state
    emitter.instruction("test rsi, rsi");                                       // callable descriptors use a fixed class spelling
    emitter.instruction("jnz __rt_sprintf_string_error_closure_x64");           // select Closure without inspecting a descriptor
    emitter.instruction("test rdi, rdi");                                       // is native object metadata available?
    emitter.instruction("jz __rt_sprintf_string_error_fallback_x64");           // no, use the generic object spelling
    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // load the object's dense runtime class id
    emitter.instruction("cmp r8, QWORD PTR [rip + _class_name_count]");         // is the class id within the dense name table?
    emitter.instruction("jae __rt_sprintf_string_error_fallback_x64");          // malformed or eval ids use the generic spelling
    emitter.instruction("lea r9, [rip + _class_name_entries]");                 // class-name metadata base
    emitter.instruction("shl r8, 4");                                           // scale the id by the pointer/length row width
    emitter.instruction("add r9, r8");                                          // select the static class-name row
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // borrow the class-name pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // borrow the class-name byte length
    emitter.instruction("test r11, r11");                                       // empty metadata falls back to object
    emitter.instruction("jnz __rt_sprintf_string_error_name_ready_x64");        // use non-empty generated metadata
    emitter.instruction("jmp __rt_sprintf_string_error_fallback_x64");          // select the generic spelling
    emitter.label("__rt_sprintf_string_error_closure_x64");
    emitter.instruction("lea r10, [rip + _sprintf_closure_class_name]");        // fixed PHP class name for callable descriptors
    emitter.instruction("mov r11, 7");                                          // byte length of Closure
    emitter.instruction("jmp __rt_sprintf_string_error_name_ready_x64");        // join message construction
    emitter.label("__rt_sprintf_string_error_fallback_x64");
    emitter.instruction("lea r10, [rip + _unser_type_object]");                 // generic object class spelling
    emitter.instruction("mov r11, 6");                                          // byte length of object
    emitter.label("__rt_sprintf_string_error_name_ready_x64");
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // preserve class-name pointer across concat
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve class-name byte length
    emitter.instruction("lea rax, [rip + _unser_object_string_error_prefix]");  // conversion Error prefix
    emitter.instruction(&format!("mov rdx, {}", UNSER_OBJECT_STRING_ERROR_PREFIX.len())); // prefix byte length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // resolved class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // resolved class-name byte length
    emitter.instruction("call __rt_concat");                                    // build `Object of class <Class>`
    emitter.instruction("lea rdi, [rip + _unser_object_string_error_suffix]");  // conversion Error suffix
    emitter.instruction(&format!("mov rsi, {}", UNSER_OBJECT_STRING_ERROR_SUFFIX.len())); // suffix byte length
    emitter.instruction("call __rt_concat");                                    // append PHP's conversion failure suffix
    emitter.instruction("call __rt_str_persist");                               // give the Error stable message ownership
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve persisted message pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve persisted message length
    emitter.instruction("mov rax, 56");                                         // canonical Throwable payload size
    emitter.instruction("call __rt_heap_alloc");                                // allocate the Error object payload
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(6))); // throwable heap-kind marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform heap header
    emitter.instruction("call __rt_object_handle_acquire");                     // assign the Error's PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_error_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // stamp the per-program Error class id
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // recover persisted message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store Error message pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // recover persisted message byte length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store Error message byte length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous Throwable defaults to null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
    emitter.instruction("leave");                                               // restore the caller frame before unwinding
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard catchable exception path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits both helpers for one target and returns their assembly text.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_sprintf_mixed_casts(&mut emitter, false);
        emitter.output()
    }

    /// Pins non-scalar casts on every supported architecture so arrays, resources,
    /// objects, and callable descriptors never fall back to raw payload bits.
    #[test]
    fn test_sprintf_mixed_casts_preserve_php_non_scalar_semantics_on_both_architectures() {
        let arm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("cset x0, ne"), "{arm}");
        assert!(arm.contains("bl __rt_resource_id_of"), "{arm}");
        assert!(arm.contains("_class_tostring_ptrs"), "{arm}");
        assert!(arm.contains("_iterable_array_str"), "{arm}");
        assert!(arm.contains("__rt_sprintf_warn_array_to_string"), "{arm}");
        assert!(arm.contains("__rt_sprintf_warn_object_numeric"), "{arm}");
        assert!(arm.contains("__rt_sprintf_throw_string_error"), "{arm}");

        let x64 = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("setne al"), "{x64}");
        assert!(x64.contains("call __rt_resource_id_of"), "{x64}");
        assert!(x64.contains("_class_tostring_ptrs"), "{x64}");
        assert!(x64.contains("_iterable_array_str"), "{x64}");
        assert!(x64.contains("__rt_sprintf_warn_array_to_string_x64"), "{x64}");
        assert!(x64.contains("__rt_sprintf_warn_object_numeric_x64"), "{x64}");
        assert!(x64.contains("__rt_sprintf_throw_string_error_x64"), "{x64}");
    }

    /// Both target emitters route eval-declared stringification through the bridge and unwinder.
    #[test]
    fn test_sprintf_eval_string_conversion_uses_owned_results_and_standard_unwinding() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_sprintf_mixed_casts(&mut emitter, true);
            let asm = emitter.output();
            assert!(asm.contains("__elephc_eval_string_context"), "{target:?}\n{asm}");
            assert!(
                asm.contains("__elephc_eval_sprintf_numeric_warning"),
                "{target:?}\n{asm}"
            );
            assert!(asm.contains("__rt_str_persist"), "{target:?}\n{asm}");
            assert!(asm.contains("__rt_decref_any"), "{target:?}\n{asm}");
            assert!(asm.contains("__rt_throw_current"), "{target:?}\n{asm}");
        }
    }
}
