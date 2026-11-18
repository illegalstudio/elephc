//! Purpose:
//! Emits the runtime guards that enforce a typed property's
//! declared type on writes through a by-reference call temporary.
//!
//! Called from:
//! - `crate::codegen_support::runtime::reference_cells::emit_reference_cells()`.
//!
//! Key details:
//! - The cell keeps the ordinary two-word payload layout, so callees read and write it with
//!   the parameter's native representation; metadata lives after those two words.
//! - A header flag distinguishes typed-property cells from ordinary local and variadic cells.
//! - Guards run before coercion, so a rejected write never reaches the property's copied value.
//! - Every emitted path is target-specific through `abi`: AArch64 and x86_64 share semantics.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::REFERENCE_CELL_HEAP_KIND;
use crate::types::param_binding::{
    PROPERTY_REF_TYPE_ARRAY, PROPERTY_REF_TYPE_BOOL, PROPERTY_REF_TYPE_FALSE,
    PROPERTY_REF_TYPE_FLOAT, PROPERTY_REF_TYPE_INT, PROPERTY_REF_TYPE_STRING,
};
use elephc_memoir::reference_cells::property::{
    self, CELL_FLAG, SUFFIX_PTR_OFFSET, SUFFIX_LEN_OFFSET, TYPE_CODE_OFFSET,
};

/// Prefix php-src uses for every typed-reference assignment diagnostic.
const PROPERTY_REFERENCE_ERROR_PREFIX: &str = "Cannot assign ";

/// Rejects a callee write that the typed property backing a reference cell would refuse.
///
/// Input: the cell address (or any unmanaged stack address) and the boxed source value. Only
/// cells carrying the typed-property flag are inspected; every other write returns immediately.
/// A rejected value never reaches the cell payload, so the caller's property keeps its value.
fn emit_property_store_guard(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_property_reference_store_guard");
    match emitter.target.arch {
        Arch::AArch64 => emit_property_store_guard_aarch64(emitter),
        Arch::X86_64 => emit_property_store_guard_x86_64(emitter),
    }
}

/// Emits the reference-assignment type guard for AArch64.
fn emit_property_store_guard_aarch64(emitter: &mut Emitter) {
    // -- validate that the reference target is a live typed-property cell --
    emitter.instruction("sub sp, sp, #96");                                     // reserve target, source, unbox and message slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the guard frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // keep the cell address and boxed source
    abi::emit_call_label(emitter, "__rt_gc_candidate_live");                   // validate allocator-owned cell identity
    emitter.instruction("cbz x0, __rt_property_reference_store_guard_done");    // stack-backed references need no property guard
    emitter.instruction("ldrb w10, [x0, #-8]");                                 // inspect the live allocation kind
    emitter.instruction(&format!("cmp w10, #{REFERENCE_CELL_HEAP_KIND}"));      // is this an owned reference cell?
    emitter.instruction("b.ne __rt_property_reference_store_guard_done");       // ordinary heap allocations have no typed-property metadata
    emitter.instruction("ldr x10, [x0, #-8]");                                  // reload the full cell header
    abi::emit_load_int_immediate(emitter, "x11", CELL_FLAG); // materialize the property marker
    emitter.instruction("tst x10, x11");                                        // does this cell carry a typed-property guard?
    emitter.instruction("b.eq __rt_property_reference_store_guard_done");       // ordinary local references keep their fast path
    emitter.instruction("str x0, [sp, #16]");                                   // keep the validated property cell
    // -- unbox the source value into a runtime tag and payload words --
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the boxed source value
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                         // expose the source runtime tag and payload
    emitter.instruction("stp x0, x1, [sp, #24]");                               // keep the source tag and low payload word
    emitter.instruction("str x2, [sp, #40]");                                   // keep the source high payload word for string lengths
    // -- dispatch on the property's declared type --
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the property cell
    emitter.instruction(&format!("ldr x10, [x9, #{TYPE_CODE_OFFSET}]"));        // load the declared-type acceptance code
    emitter.instruction(&format!("cmp x10, #{PROPERTY_REF_TYPE_INT}"));         // is the property declared int?
    emitter.instruction("b.eq __rt_property_reference_guard_int");              // apply weak int assignment rules
    emitter.instruction(&format!("cmp x10, #{PROPERTY_REF_TYPE_STRING}"));      // is the property declared string?
    emitter.instruction("b.eq __rt_property_reference_guard_string");           // accept scalar and string sources
    emitter.instruction(&format!("cmp x10, #{PROPERTY_REF_TYPE_FLOAT}"));       // is the property declared float?
    emitter.instruction("b.eq __rt_property_reference_guard_float");            // accept numeric sources
    emitter.instruction(&format!("cmp x10, #{PROPERTY_REF_TYPE_BOOL}"));        // is the property declared bool?
    emitter.instruction("b.eq __rt_property_reference_guard_bool");             // accept scalar sources
    emitter.instruction(&format!("cmp x10, #{PROPERTY_REF_TYPE_FALSE}"));       // is the property declared false?
    emitter.instruction("b.eq __rt_property_reference_guard_false");            // accept only the false literal
    emitter.instruction(&format!("cmp x10, #{PROPERTY_REF_TYPE_ARRAY}"));       // is the property declared array?
    emitter.instruction("b.eq __rt_property_reference_guard_array");            // accept array payloads
    emitter.instruction("b __rt_property_reference_store_guard_done");          // unknown selector keeps prior semantics

    emitter.label("__rt_property_reference_guard_int");
    // -- int properties accept ints, bools, and numeric strings --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source tag
    emitter.instruction("cbz x9, __rt_property_reference_store_guard_done");    // int payloads satisfy int properties
    emitter.instruction("cmp x9, #3");                                          // is the source a bool?
    emitter.instruction("b.eq __rt_property_reference_store_guard_done");       // bool values weakly convert to int
    emitter.instruction("cmp x9, #1");                                          // is the source a string?
    emitter.instruction("b.eq __rt_property_reference_guard_int_string");       // apply numeric-string acceptance
    emitter.instruction("cmp x9, #2");                                          // is the source a float?
    emitter.instruction("b.eq __rt_property_reference_guard_int_float");        // apply finite-range acceptance
    emitter.instruction("b __rt_property_reference_guard_reject");              // every other payload is a property TypeError

    emitter.label("__rt_property_reference_guard_int_string");
    emitter.instruction("ldr x1, [sp, #32]");                                   // load the string pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // load the string length
    abi::emit_call_label(emitter, "__rt_property_reference_numeric");         // classify the numeric spelling
    emitter.instruction("cmp x0, #1");                                          // does the spelling name an i64 integer?
    emitter.instruction("b.eq __rt_property_reference_store_guard_done");       // exact integers are accepted
    emitter.instruction("cbz x0, __rt_property_reference_guard_reject");        // non-numeric strings are rejected
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the boxed string source
    abi::emit_call_label(emitter, "__rt_mixed_cast_float");                   // convert the numeric spelling through the float path
    emitter.instruction("b __rt_property_reference_guard_int_float_check");     // apply the shared finite-range acceptance

    emitter.label("__rt_property_reference_guard_int_float");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load the float bit pattern
    emitter.instruction("fmov d0, x9");                                         // move it into the FPU
    emitter.label("__rt_property_reference_guard_int_float_check");
    emitter.instruction("fcmp d0, d0");                                         // is the float NaN?
    emitter.instruction("b.vs __rt_property_reference_guard_reject");           // NaN never fits an int
    abi::emit_load_int_immediate(emitter, "x9", 0x43e0000000000000_u64 as i64); // materialize 2^63 as a float bit pattern
    emitter.instruction("fmov d1, x9");                                         // compare against the upper bound
    emitter.instruction("fcmp d0, d1");                                         // is the float at or above 2^63?
    emitter.instruction("b.ge __rt_property_reference_guard_reject");           // out-of-range floats are rejected
    abi::emit_load_int_immediate(emitter, "x9", 0xc3e0000000000000_u64 as i64); // materialize -2^63 as a float bit pattern
    emitter.instruction("fmov d1, x9");                                         // compare against the lower bound
    emitter.instruction("fcmp d0, d1");                                         // is the float below -2^63?
    emitter.instruction("b.lt __rt_property_reference_guard_reject");           // out-of-range floats are rejected
    emitter.instruction("b __rt_property_reference_store_guard_done");          // finite in-range floats weakly convert

    emitter.label("__rt_property_reference_guard_string");
    // -- string properties accept scalars and Stringable objects --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source tag
    emitter.instruction("cmp x9, #3");                                          // scalars convert to strings
    emitter.instruction("b.ls __rt_property_reference_store_guard_done");       // accept int, float, string and bool sources
    emitter.instruction("cmp x9, #6");                                          // objects may implement Stringable
    emitter.instruction("b.eq __rt_property_reference_store_guard_done");       // leave object coercion to the existing store path
    emitter.instruction("b __rt_property_reference_guard_reject");              // arrays and null are rejected

    emitter.label("__rt_property_reference_guard_float");
    // -- float properties accept ints, floats, bools, and numeric strings --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source tag
    emitter.instruction("cbz x9, __rt_property_reference_store_guard_done");    // ints widen to float
    emitter.instruction("cmp x9, #2");                                          // is the source already a float?
    emitter.instruction("b.eq __rt_property_reference_store_guard_done");       // floats satisfy float properties
    emitter.instruction("cmp x9, #3");                                          // is the source a bool?
    emitter.instruction("b.eq __rt_property_reference_store_guard_done");       // bools weakly convert to float
    emitter.instruction("cmp x9, #1");                                          // is the source a string?
    emitter.instruction("b.eq __rt_property_reference_guard_float_string");     // numeric strings weakly convert to float
    emitter.instruction("b __rt_property_reference_guard_reject");              // every other payload is a property TypeError

    emitter.label("__rt_property_reference_guard_float_string");
    emitter.instruction("ldr x1, [sp, #32]");                                   // load the string pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // load the string length
    abi::emit_call_label(emitter, "__rt_property_reference_numeric");         // classify the numeric spelling
    emitter.instruction("cbnz x0, __rt_property_reference_store_guard_done");   // every numeric spelling widens to float
    emitter.instruction("b __rt_property_reference_guard_reject");              // non-numeric strings are rejected

    emitter.label("__rt_property_reference_guard_bool");
    // -- bool properties accept scalar sources; false and array keep their exact contracts --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source tag
    emitter.instruction("cmp x9, #3");                                          // scalars convert to bool
    emitter.instruction("b.hi __rt_property_reference_guard_reject");           // null and containers are not scalar bools
    emitter.instruction("b __rt_property_reference_store_guard_done");          // accept int, float, string and bool sources

    emitter.label("__rt_property_reference_guard_false");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source tag
    emitter.instruction("cmp x9, #3");                                          // only a bool value can satisfy false
    emitter.instruction("b.ne __rt_property_reference_guard_reject");           // everything else is rejected
    emitter.instruction("ldr x9, [sp, #32]");                                   // load the bool payload
    emitter.instruction("cbnz x9, __rt_property_reference_guard_reject");       // true is rejected by a false-typed property
    emitter.instruction("b __rt_property_reference_store_guard_done");          // false is accepted

    emitter.label("__rt_property_reference_guard_array");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source tag
    emitter.instruction("cmp x9, #4");                                          // is the source an indexed array?
    emitter.instruction("b.eq __rt_property_reference_store_guard_done");       // indexed arrays satisfy array properties
    emitter.instruction("cmp x9, #5");                                          // is the source an associative array?
    emitter.instruction("b.eq __rt_property_reference_store_guard_done");       // associative arrays satisfy array properties
    emitter.instruction("b __rt_property_reference_guard_reject");              // everything else is rejected

    emitter.label("__rt_property_reference_guard_reject");
    // -- render the rejected source's PHP type name and raise the reference TypeError --
    emit_property_reference_name_row_aarch64(emitter);
    emitter.label("__rt_property_reference_guard_throw");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the property cell
    emitter.instruction(&format!("ldr x3, [x9, #{SUFFIX_PTR_OFFSET}]"));        // load the static diagnostic suffix pointer
    emitter.instruction(&format!("ldr x4, [x9, #{SUFFIX_LEN_OFFSET}]"));        // load the static diagnostic suffix length
    emitter.instruction("add x2, sp, #48");                                     // point at the {name, length} row
    abi::emit_symbol_address(emitter, "x0", "_unser_property_assign_prefix");  // load php's "Cannot assign " prefix
    emitter.instruction(&format!("mov x1, #{}", PROPERTY_REFERENCE_ERROR_PREFIX.len())); // pass its byte length
    abi::emit_call_label(emitter, "__rt_spl_throw_offset_type");               // raise the reference-form TypeError
    emitter.label("__rt_property_reference_store_guard_done");
    // -- restore the guard frame --
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the guard frame
    abi::emit_return(emitter);
}

/// Materializes the rejected source's PHP type name row for the AArch64 guard.
fn emit_property_reference_name_row_aarch64(emitter: &mut Emitter) {
    let int = "__rt_property_reference_guard_name_int";
    let string = "__rt_property_reference_guard_name_string";
    let float = "__rt_property_reference_guard_name_float";
    let array = "__rt_property_reference_guard_name_array";
    let object = "__rt_property_reference_guard_name_object";
    let null = "__rt_property_reference_guard_name_null";
    let resource = "__rt_property_reference_guard_name_resource";
    let closure = "__rt_property_reference_guard_name_closure";
    let unknown = "__rt_property_reference_guard_name_unknown";
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the source runtime tag
    emitter.instruction("cbz x9, __rt_property_reference_guard_name_int");      // int sources are spelled "int"
    emitter.instruction("cmp x9, #1");                                          // string source?
    emitter.instruction(&format!("b.eq {string}"));                             // select "string"
    emitter.instruction("cmp x9, #2");                                          // float source?
    emitter.instruction(&format!("b.eq {float}"));                              // select "float"
    emitter.instruction("cmp x9, #3");                                          // bool source?
    emitter.instruction("b.eq __rt_property_reference_guard_name_bool");        // spell the literal true/false
    emitter.instruction("cmp x9, #4");                                          // indexed array source?
    emitter.instruction(&format!("b.eq {array}"));                              // select "array"
    emitter.instruction("cmp x9, #5");                                          // associative array source?
    emitter.instruction(&format!("b.eq {array}"));                              // select "array"
    emitter.instruction("cmp x9, #6");                                          // object source?
    emitter.instruction(&format!("b.eq {object}"));                             // select the class fallback
    emitter.instruction("cmp x9, #8");                                          // null source?
    emitter.instruction(&format!("b.eq {null}"));                               // select "null"
    emitter.instruction("cmp x9, #9");                                          // resource source?
    emitter.instruction(&format!("b.eq {resource}"));                           // select "resource"
    emitter.instruction("cmp x9, #10");                                         // callable source?
    emitter.instruction(&format!("b.eq {closure}"));                            // PHP spells closures as their class
    emitter.instruction(&format!("b {unknown}"));                               // fall back to the shared unknown name
    emit_name_row_aarch64(emitter, int, "_unser_type_int", 3);
    emit_name_row_aarch64(emitter, string, "_unser_type_string", 6);
    emit_name_row_aarch64(emitter, float, "_unser_type_float", 5);
    emitter.label("__rt_property_reference_guard_name_bool");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load the bool payload
    emitter.instruction("cbnz x9, __rt_property_reference_guard_name_true");    // true gets its literal spelling
    emit_name_row_aarch64(emitter, "__rt_property_reference_guard_name_false", "_unser_type_false", 5);
    emitter.label("__rt_property_reference_guard_name_true");
    emit_name_row_aarch64(emitter, "__rt_property_reference_guard_name_true_row", "_unser_type_true", 4);
    emit_name_row_aarch64(emitter, array, "_unser_type_array", 5);
    emit_name_row_aarch64(emitter, object, "_unser_type_object", 6);
    emit_name_row_aarch64(emitter, null, "_unser_type_null", 4);
    emit_name_row_aarch64(emitter, resource, "_unser_type_resource", 8);
    emit_name_row_aarch64(emitter, closure, "_sprintf_closure_class_name", 7);
    emit_name_row_aarch64(emitter, unknown, "_unser_type_unknown", 7);
}

/// Materializes one AArch64 name row and jumps back to the shared rejection tail.
fn emit_name_row_aarch64(emitter: &mut Emitter, label: &str, symbol: &str, len: usize) {
    emitter.label(label);
    abi::emit_symbol_address(emitter, "x1", symbol);
    emitter.instruction(&format!("mov x2, #{len}"));                            // pass the PHP type-name byte length
    emitter.instruction("stp x1, x2, [sp, #48]");                               // publish the {name, length} row on the guard frame
    emitter.instruction("b __rt_property_reference_guard_throw");               // share the TypeError construction tail
}

/// Emits the reference-assignment type guard for x86_64.
fn emit_property_store_guard_x86_64(emitter: &mut Emitter) {
    // -- validate that the reference target is a live typed-property cell --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the guard frame
    emitter.instruction("sub rsp, 96");                                         // reserve target, source, unbox and message slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the cell address
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the boxed source
    emitter.instruction("mov rax, rdi");                                        // validate the candidate cell address
    abi::emit_call_label(emitter, "__rt_gc_candidate_live");                   // require exact allocator-owned identity
    emitter.instruction("test rax, rax");                                       // did the address name a live allocation?
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // stack-backed references need no property guard
    emitter.instruction(&format!("cmp BYTE PTR [rax - 8], {REFERENCE_CELL_HEAP_KIND}")); // is this an owned reference cell?
    emitter.instruction("jne __rt_property_reference_store_guard_done_x");      // ordinary allocations have no typed-property metadata
    abi::emit_load_int_immediate(emitter, "r10", CELL_FLAG); // materialize the property marker
    emitter.instruction("test QWORD PTR [rax - 8], r10");                       // does this cell carry a typed-property guard?
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // ordinary local references keep their fast path
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the validated property cell
    // -- unbox the source value into a runtime tag and payload words --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the boxed source value
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                         // expose the source runtime tag and payload
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep the source tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // keep the source low payload word
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // keep the source high payload word for string lengths
    // -- dispatch on the property's declared type --
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the property cell
    emitter.instruction(&format!("mov r10, QWORD PTR [r9 + {TYPE_CODE_OFFSET}]")); // load the declared-type acceptance code
    emitter.instruction(&format!("cmp r10, {PROPERTY_REF_TYPE_INT}"));          // is the property declared int?
    emitter.instruction("je __rt_property_reference_guard_int_x");              // apply weak int assignment rules
    emitter.instruction(&format!("cmp r10, {PROPERTY_REF_TYPE_STRING}"));       // is the property declared string?
    emitter.instruction("je __rt_property_reference_guard_string_x");           // accept scalar and string sources
    emitter.instruction(&format!("cmp r10, {PROPERTY_REF_TYPE_FLOAT}"));        // is the property declared float?
    emitter.instruction("je __rt_property_reference_guard_float_x");            // accept numeric sources
    emitter.instruction(&format!("cmp r10, {PROPERTY_REF_TYPE_BOOL}"));         // is the property declared bool?
    emitter.instruction("je __rt_property_reference_guard_bool_x");             // accept scalar sources
    emitter.instruction(&format!("cmp r10, {PROPERTY_REF_TYPE_FALSE}"));        // is the property declared false?
    emitter.instruction("je __rt_property_reference_guard_false_x");            // accept only the false literal
    emitter.instruction(&format!("cmp r10, {PROPERTY_REF_TYPE_ARRAY}"));        // is the property declared array?
    emitter.instruction("je __rt_property_reference_guard_array_x");            // accept array payloads
    emitter.instruction("jmp __rt_property_reference_store_guard_done_x");      // unknown selector keeps prior semantics

    emitter.label("__rt_property_reference_guard_int_x");
    // -- int properties accept ints, bools, and numeric strings --
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the source tag
    emitter.instruction("test r9, r9");                                         // int payloads satisfy int properties
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // accept raw ints
    emitter.instruction("cmp r9, 3");                                           // is the source a bool?
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // bool values weakly convert to int
    emitter.instruction("cmp r9, 1");                                           // is the source a string?
    emitter.instruction("je __rt_property_reference_guard_int_string_x");       // apply numeric-string acceptance
    emitter.instruction("cmp r9, 2");                                           // is the source a float?
    emitter.instruction("je __rt_property_reference_guard_int_float_x");        // apply finite-range acceptance
    emitter.instruction("jmp __rt_property_reference_guard_reject_x");          // every other payload is a property TypeError

    emitter.label("__rt_property_reference_guard_int_string_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // load the string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // load the string length from the unbox high word
    abi::emit_call_label(emitter, "__rt_property_reference_numeric_x");       // classify the numeric spelling
    emitter.instruction("cmp rax, 1");                                          // does the spelling name an i64 integer?
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // exact integers are accepted
    emitter.instruction("test rax, rax");                                       // is the spelling non-numeric?
    emitter.instruction("je __rt_property_reference_guard_reject_x");           // non-numeric strings are rejected
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the boxed string source
    abi::emit_call_label(emitter, "__rt_mixed_cast_float");                   // convert the numeric spelling through the float path
    emitter.instruction("jmp __rt_property_reference_guard_int_float_check_x"); // apply the shared finite-range acceptance

    emitter.label("__rt_property_reference_guard_int_float_x");
    emitter.instruction("movq xmm0, QWORD PTR [rbp - 40]");                     // load the float bit pattern
    emitter.label("__rt_property_reference_guard_int_float_check_x");
    emitter.instruction("ucomisd xmm0, xmm0");                                  // is the float NaN?
    emitter.instruction("jp __rt_property_reference_guard_reject_x");           // NaN never fits an int
    abi::emit_load_int_immediate(emitter, "r10", 0x43e0000000000000_u64 as i64); // materialize 2^63 as a float bit pattern
    emitter.instruction("movq xmm1, r10");                                      // compare against the upper bound
    emitter.instruction("ucomisd xmm0, xmm1");                                  // is the float at or above 2^63?
    emitter.instruction("jae __rt_property_reference_guard_reject_x");          // out-of-range floats are rejected
    abi::emit_load_int_immediate(emitter, "r10", 0xc3e0000000000000_u64 as i64); // materialize -2^63 as a float bit pattern
    emitter.instruction("movq xmm1, r10");                                      // compare against the lower bound
    emitter.instruction("ucomisd xmm0, xmm1");                                  // is the float below -2^63?
    emitter.instruction("jb __rt_property_reference_guard_reject_x");           // out-of-range floats are rejected
    emitter.instruction("jmp __rt_property_reference_store_guard_done_x");      // finite in-range floats weakly convert

    emitter.label("__rt_property_reference_guard_string_x");
    // -- string properties accept scalars and Stringable objects --
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the source tag
    emitter.instruction("cmp r9, 3");                                           // scalars convert to strings
    emitter.instruction("jbe __rt_property_reference_store_guard_done_x");      // accept int, float, string and bool sources
    emitter.instruction("cmp r9, 6");                                           // objects may implement Stringable
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // leave object coercion to the existing store path
    emitter.instruction("jmp __rt_property_reference_guard_reject_x");          // arrays and null are rejected

    emitter.label("__rt_property_reference_guard_float_x");
    // -- float properties accept ints, floats, bools, and numeric strings --
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the source tag
    emitter.instruction("test r9, r9");                                         // ints widen to float
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // accept raw ints
    emitter.instruction("cmp r9, 2");                                           // is the source already a float?
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // floats satisfy float properties
    emitter.instruction("cmp r9, 3");                                           // is the source a bool?
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // bools weakly convert to float
    emitter.instruction("cmp r9, 1");                                           // is the source a string?
    emitter.instruction("je __rt_property_reference_guard_float_string_x");     // numeric strings weakly convert to float
    emitter.instruction("jmp __rt_property_reference_guard_reject_x");          // every other payload is a property TypeError

    emitter.label("__rt_property_reference_guard_float_string_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // load the string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // load the string length from the unbox high word
    abi::emit_call_label(emitter, "__rt_property_reference_numeric_x");       // classify the numeric spelling
    emitter.instruction("test rax, rax");                                       // is the whole spelling numeric?
    emitter.instruction("jnz __rt_property_reference_store_guard_done_x");      // every numeric spelling widens to float
    emitter.instruction("jmp __rt_property_reference_guard_reject_x");          // non-numeric strings are rejected

    emitter.label("__rt_property_reference_guard_bool_x");
    // -- bool properties accept scalar sources; false and array keep their exact contracts --
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the source tag
    emitter.instruction("cmp r9, 3");                                           // scalars convert to bool
    emitter.instruction("ja __rt_property_reference_guard_reject_x");           // null and containers are not scalar bools
    emitter.instruction("jmp __rt_property_reference_store_guard_done_x");      // accept int, float, string and bool sources

    emitter.label("__rt_property_reference_guard_false_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the source tag
    emitter.instruction("cmp r9, 3");                                           // only a bool value can satisfy false
    emitter.instruction("jne __rt_property_reference_guard_reject_x");          // everything else is rejected
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // load the bool payload
    emitter.instruction("jne __rt_property_reference_guard_reject_x");          // true is rejected by a false-typed property
    emitter.instruction("jmp __rt_property_reference_store_guard_done_x");      // false is accepted

    emitter.label("__rt_property_reference_guard_array_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the source tag
    emitter.instruction("cmp r9, 4");                                           // is the source an indexed array?
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // indexed arrays satisfy array properties
    emitter.instruction("cmp r9, 5");                                           // is the source an associative array?
    emitter.instruction("je __rt_property_reference_store_guard_done_x");       // associative arrays satisfy array properties
    emitter.instruction("jmp __rt_property_reference_guard_reject_x");          // everything else is rejected

    emitter.label("__rt_property_reference_guard_reject_x");
    // -- render the rejected source's PHP type name and raise the reference TypeError --
    emit_property_reference_name_row_x86_64(emitter);
    emitter.label("__rt_property_reference_guard_throw_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the property cell
    emitter.instruction(&format!("mov rcx, QWORD PTR [r9 + {SUFFIX_PTR_OFFSET}]")); // load the static diagnostic suffix pointer
    emitter.instruction(&format!("mov r8, QWORD PTR [r9 + {SUFFIX_LEN_OFFSET}]")); // load the static diagnostic suffix length
    emitter.instruction("lea rdx, [rbp - 80]");                                 // point at the {name, length} row
    abi::emit_symbol_address(emitter, "rdi", "_unser_property_assign_prefix"); // load php's "Cannot assign " prefix
    emitter.instruction(&format!("mov rsi, {}", PROPERTY_REFERENCE_ERROR_PREFIX.len())); // pass its byte length
    abi::emit_call_label(emitter, "__rt_spl_throw_offset_type");               // raise the reference-form TypeError
    emitter.label("__rt_property_reference_store_guard_done_x");
    // -- restore the guard frame --
    emitter.instruction("mov rsp, rbp");                                        // release the guard frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    abi::emit_return(emitter);
}

/// Materializes the rejected source's PHP type name row for the x86_64 guard.
fn emit_property_reference_name_row_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the source runtime tag
    emitter.instruction("test r9, r9");                                         // int sources are spelled "int"
    emitter.instruction("je __rt_property_reference_guard_name_int_x");         // select "int"
    emitter.instruction("cmp r9, 1");                                           // string source?
    emitter.instruction("je __rt_property_reference_guard_name_string_x");      // select "string"
    emitter.instruction("cmp r9, 2");                                           // float source?
    emitter.instruction("je __rt_property_reference_guard_name_float_x");       // select "float"
    emitter.instruction("cmp r9, 3");                                           // bool source?
    emitter.instruction("je __rt_property_reference_guard_name_bool_x");        // spell the literal true/false
    emitter.instruction("cmp r9, 4");                                           // indexed array source?
    emitter.instruction("je __rt_property_reference_guard_name_array_x");       // select "array"
    emitter.instruction("cmp r9, 5");                                           // associative array source?
    emitter.instruction("je __rt_property_reference_guard_name_array_x");       // select "array"
    emitter.instruction("cmp r9, 6");                                           // object source?
    emitter.instruction("je __rt_property_reference_guard_name_object_x");      // select the class fallback
    emitter.instruction("cmp r9, 8");                                           // null source?
    emitter.instruction("je __rt_property_reference_guard_name_null_x");        // select "null"
    emitter.instruction("cmp r9, 9");                                           // resource source?
    emitter.instruction("je __rt_property_reference_guard_name_resource_x");    // select "resource"
    emitter.instruction("cmp r9, 10");                                          // callable source?
    emitter.instruction("je __rt_property_reference_guard_name_closure_x");     // PHP spells closures as their class
    emitter.instruction("jmp __rt_property_reference_guard_name_unknown_x");    // fall back to the shared unknown name
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_int_x", "_unser_type_int", 3);
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_string_x", "_unser_type_string", 6);
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_float_x", "_unser_type_float", 5);
    emitter.label("__rt_property_reference_guard_name_bool_x");
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // load the bool payload
    emitter.instruction("jne __rt_property_reference_guard_name_true_x");       // true gets its literal spelling
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_false_x", "_unser_type_false", 5);
    emitter.label("__rt_property_reference_guard_name_true_x");
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_true_row_x", "_unser_type_true", 4);
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_array_x", "_unser_type_array", 5);
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_object_x", "_unser_type_object", 6);
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_null_x", "_unser_type_null", 4);
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_resource_x", "_unser_type_resource", 8);
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_closure_x", "_sprintf_closure_class_name", 7);
    emit_name_row_x86_64(emitter, "__rt_property_reference_guard_name_unknown_x", "_unser_type_unknown", 7);
}

/// Materializes one x86_64 name row and jumps back to the shared rejection tail.
fn emit_name_row_x86_64(emitter: &mut Emitter, label: &str, symbol: &str, len: usize) {
    emitter.label(label);
    abi::emit_symbol_address(emitter, "r10", symbol);
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // publish the PHP type-name pointer in the row
    emitter.instruction(&format!("mov QWORD PTR [rbp - 72], {len}"));           // publish the PHP type-name byte length
    emitter.instruction("jmp __rt_property_reference_guard_throw_x");           // share the TypeError construction tail
}

/// Emits the numeric-string predicate used by typed-property reference guards.
///
/// The JSON encoder's scanner is feature-gated, and a reference write must work in every
/// program shape. This leaf follows PHP's `is_numeric()` spelling: optional surrounding
/// whitespace, an optional sign, an integer and/or fractional part, and an optional exponent
/// with at least one digit.
///
/// Returns in `x0` / `rax`: 0 for a non-numeric spelling, 1 for an integer-form spelling whose
/// value fits `i64`, and 2 for every other numeric spelling (fraction, exponent, or an integer
/// magnitude outside `i64`). An exact-integer caller range-checks classification 2 through its
/// float path; a float target accepts every nonzero classification.
fn emit_property_numeric(emitter: &mut Emitter) {
    emitter.blank();
    match emitter.target.arch {
        Arch::AArch64 => emit_property_numeric_aarch64(emitter),
        Arch::X86_64 => emit_property_numeric_x86_64(emitter),
    }
}

/// Emits the AArch64 numeric-string scanner.
fn emit_property_numeric_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_property_reference_numeric");
    // -- scanner state: byte index, magnitude, float-form flag, and sign --
    emitter.instruction("mov x0, #0");                                          // 0 = non-numeric
    emitter.instruction("cbz x2, __rt_property_reference_numeric_done");        // an empty string is not numeric
    emitter.instruction("mov x9, #0");                                          // start at the first byte
    emitter.instruction("mov x13, #0");                                         // accumulated integer magnitude
    emitter.instruction("mov x14, #0");                                         // 0 = exact integer form, 1 = float form, 2 = integer overflow
    emitter.instruction("mov x15, #0");                                         // negative sign flag
    emitter.label("__rt_property_reference_numeric_leading");
    // -- skip PHP's leading whitespace bytes --
    emitter.instruction("ldrb w10, [x1, x9]");                                  // inspect the current byte
    emitter.instruction("cmp w10, #0x20");                                      // space?
    emitter.instruction("b.eq __rt_property_reference_numeric_leading_space");  // consume PHP whitespace
    emitter.instruction("cmp w10, #0x09");                                      // tab?
    emitter.instruction("b.eq __rt_property_reference_numeric_leading_space");  // consume PHP whitespace
    emitter.instruction("cmp w10, #0x0a");                                      // newline?
    emitter.instruction("b.eq __rt_property_reference_numeric_leading_space");  // consume PHP whitespace
    emitter.instruction("cmp w10, #0x0d");                                      // carriage return?
    emitter.instruction("b.eq __rt_property_reference_numeric_leading_space");  // consume PHP whitespace
    emitter.instruction("cmp w10, #0x0b");                                      // vertical tab?
    emitter.instruction("b.eq __rt_property_reference_numeric_leading_space");  // consume PHP whitespace
    emitter.instruction("cmp w10, #0x0c");                                      // form feed?
    emitter.instruction("b.ne __rt_property_reference_numeric_sign");           // start the sign scan
    emitter.label("__rt_property_reference_numeric_leading_space");
    emitter.instruction("add x9, x9, #1");                                      // consume the whitespace byte
    emitter.instruction("cmp x9, x2");                                          // reached the end?
    emitter.instruction("b.lo __rt_property_reference_numeric_leading");        // keep skipping
    emitter.instruction("b __rt_property_reference_numeric_done");              // whitespace-only is not numeric
    emitter.label("__rt_property_reference_numeric_sign");
    // -- consume an optional sign --
    emitter.instruction("cmp w10, #0x2b");                                      // leading plus?
    emitter.instruction("b.eq __rt_property_reference_numeric_sign_done");      // consume it
    emitter.instruction("cmp w10, #0x2d");                                      // leading minus?
    emitter.instruction("b.ne __rt_property_reference_numeric_int");            // start with the integer digits
    emitter.instruction("mov x15, #1");                                         // remember the negative sign
    emitter.label("__rt_property_reference_numeric_sign_done");
    emitter.instruction("add x9, x9, #1");                                      // consume the sign
    emitter.instruction("cmp x9, x2");                                          // is anything left?
    emitter.instruction("b.hs __rt_property_reference_numeric_done");           // a bare sign is not numeric
    emitter.label("__rt_property_reference_numeric_int");
    // -- accumulate the integer digits with overflow detection --
    emitter.instruction("mov x11, #0");                                         // count integer and fractional digits
    emitter.label("__rt_property_reference_numeric_int_loop");
    emitter.instruction("ldrb w10, [x1, x9]");                                  // inspect the current byte
    emitter.instruction("cmp w10, #0x30");                                      // below ASCII '0'?
    emitter.instruction("b.lo __rt_property_reference_numeric_int_done");       // leave the integer part
    emitter.instruction("cmp w10, #0x39");                                      // above ASCII '9'?
    emitter.instruction("b.hi __rt_property_reference_numeric_int_done");       // leave the integer part
    emitter.instruction("add x11, x11, #1");                                    // count one digit
    emitter.instruction("sub w16, w10, #0x30");                                 // convert the byte to its digit value
    emitter.instruction("mov x17, #10");                                        // multiply the accumulator by ten
    emitter.instruction("umulh x10, x13, x17");                                 // detect multiplication overflow
    emitter.instruction("cbnz x10, __rt_property_reference_numeric_int_overflow"); // the magnitude no longer fits unsigned 64 bits
    emitter.instruction("mul x13, x13, x17");                                   // apply the decimal shift
    emitter.instruction("add x16, x13, x16");                                   // add the current digit
    emitter.instruction("cmp x16, x13");                                        // detect addition overflow
    emitter.instruction("b.lo __rt_property_reference_numeric_int_overflow");   // the magnitude no longer fits unsigned 64 bits
    emitter.instruction("mov x13, x16");                                        // publish the accumulated magnitude
    emitter.instruction("add x9, x9, #1");                                      // advance
    emitter.instruction("cmp x9, x2");                                          // reached the end?
    emitter.instruction("b.lo __rt_property_reference_numeric_int_loop");       // keep reading digits
    emitter.instruction("b __rt_property_reference_numeric_after");             // no fraction or exponent remains
    emitter.label("__rt_property_reference_numeric_int_overflow");
    // -- stop accumulating once the magnitude leaves unsigned 64 bits --
    emitter.instruction("mov x14, #2");                                         // remember that the integer exceeded i64
    emitter.instruction("add x9, x9, #1");                                      // consume the overflowing digit
    emitter.label("__rt_property_reference_numeric_int_skip");
    emitter.instruction("cmp x9, x2");                                          // reached the end?
    emitter.instruction("b.hs __rt_property_reference_numeric_after");          // the spelling is still numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // inspect the current byte
    emitter.instruction("cmp w10, #0x30");                                      // below ASCII '0'?
    emitter.instruction("b.lo __rt_property_reference_numeric_int_done");       // leave the integer part
    emitter.instruction("cmp w10, #0x39");                                      // above ASCII '9'?
    emitter.instruction("b.hi __rt_property_reference_numeric_int_done");       // leave the integer part
    emitter.instruction("add x9, x9, #1");                                      // consume the digit without accumulating
    emitter.instruction("b __rt_property_reference_numeric_int_skip");          // keep consuming digits
    emitter.label("__rt_property_reference_numeric_int_done");
    // -- parse the fractional part when a decimal point follows --
    emitter.instruction("cmp w10, #0x2e");                                      // decimal point?
    emitter.instruction("b.ne __rt_property_reference_numeric_exponent");       // no fractional part
    emitter.instruction("mov x14, #1");                                         // mark a float-form spelling
    emitter.instruction("add x9, x9, #1");                                      // consume the decimal point
    emitter.label("__rt_property_reference_numeric_frac");
    emitter.instruction("cmp x9, x2");                                          // reached the end?
    emitter.instruction("b.hs __rt_property_reference_numeric_after");          // a trailing point is numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // inspect the current byte
    emitter.instruction("cmp w10, #0x30");                                      // below ASCII '0'?
    emitter.instruction("b.lo __rt_property_reference_numeric_exponent");       // leave the fractional part
    emitter.instruction("cmp w10, #0x39");                                      // above ASCII '9'?
    emitter.instruction("b.hi __rt_property_reference_numeric_exponent");       // leave the fractional part
    emitter.instruction("add x11, x11, #1");                                    // count one fractional digit
    emitter.instruction("add x9, x9, #1");                                      // advance
    emitter.instruction("b __rt_property_reference_numeric_frac");              // keep reading fractional digits
    emitter.label("__rt_property_reference_numeric_exponent");
    // -- parse an optional exponent and its sign --
    emitter.instruction("cbz x11, __rt_property_reference_numeric_done");       // a point alone is not numeric
    emitter.instruction("cmp w10, #0x65");                                      // lowercase exponent marker?
    emitter.instruction("b.eq __rt_property_reference_numeric_exponent_digits"); // parse its digits
    emitter.instruction("cmp w10, #0x45");                                      // uppercase exponent marker?
    emitter.instruction("b.ne __rt_property_reference_numeric_trailing");       // no exponent
    emitter.label("__rt_property_reference_numeric_exponent_digits");
    emitter.instruction("mov x14, #1");                                         // mark a float-form spelling
    emitter.instruction("add x9, x9, #1");                                      // consume the exponent marker
    emitter.instruction("cmp x9, x2");                                          // is anything left?
    emitter.instruction("b.hs __rt_property_reference_numeric_done");           // a bare exponent is not numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // inspect the exponent sign
    emitter.instruction("cmp w10, #0x2b");                                      // positive exponent?
    emitter.instruction("b.eq __rt_property_reference_numeric_exponent_sign");  // consume it
    emitter.instruction("cmp w10, #0x2d");                                      // negative exponent?
    emitter.instruction("b.ne __rt_property_reference_numeric_exponent_first"); // parse digits directly
    emitter.label("__rt_property_reference_numeric_exponent_sign");
    emitter.instruction("add x9, x9, #1");                                      // consume the sign
    emitter.instruction("cmp x9, x2");                                          // is anything left?
    emitter.instruction("b.hs __rt_property_reference_numeric_done");           // a bare exponent sign is not numeric
    emitter.label("__rt_property_reference_numeric_exponent_first");
    emitter.instruction("mov x12, #0");                                         // count exponent digits
    emitter.label("__rt_property_reference_numeric_exponent_loop");
    emitter.instruction("cmp x9, x2");                                          // reached the end?
    emitter.instruction("b.hs __rt_property_reference_numeric_exponent_done");  // finish the exponent
    emitter.instruction("ldrb w10, [x1, x9]");                                  // inspect the current byte
    emitter.instruction("cmp w10, #0x30");                                      // below ASCII '0'?
    emitter.instruction("b.lo __rt_property_reference_numeric_exponent_done");  // leave the exponent
    emitter.instruction("cmp w10, #0x39");                                      // above ASCII '9'?
    emitter.instruction("b.hi __rt_property_reference_numeric_exponent_done");  // leave the exponent
    emitter.instruction("add x12, x12, #1");                                    // count one exponent digit
    emitter.instruction("add x9, x9, #1");                                      // advance
    emitter.instruction("b __rt_property_reference_numeric_exponent_loop");     // keep reading exponent digits
    emitter.label("__rt_property_reference_numeric_exponent_done");
    emitter.instruction("cbz x12, __rt_property_reference_numeric_done");       // an exponent needs at least one digit
    emitter.instruction("b __rt_property_reference_numeric_trailing");          // validate the trailing bytes
    emitter.label("__rt_property_reference_numeric_after");
    emitter.instruction("cbz x11, __rt_property_reference_numeric_done");       // a sign with no digits is not numeric
    emitter.label("__rt_property_reference_numeric_trailing");
    // -- consume trailing PHP whitespace --
    emitter.instruction("cmp x9, x2");                                          // reached the end?
    emitter.instruction("b.hs __rt_property_reference_numeric_accept");         // the whole spelling is numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // inspect the current byte
    emitter.instruction("cmp w10, #0x20");                                      // space?
    emitter.instruction("b.eq __rt_property_reference_numeric_trailing_space"); // consume trailing whitespace
    emitter.instruction("cmp w10, #0x09");                                      // tab?
    emitter.instruction("b.eq __rt_property_reference_numeric_trailing_space"); // consume trailing whitespace
    emitter.instruction("cmp w10, #0x0a");                                      // newline?
    emitter.instruction("b.eq __rt_property_reference_numeric_trailing_space"); // consume trailing whitespace
    emitter.instruction("cmp w10, #0x0d");                                      // carriage return?
    emitter.instruction("b.eq __rt_property_reference_numeric_trailing_space"); // consume trailing whitespace
    emitter.instruction("cmp w10, #0x0b");                                      // vertical tab?
    emitter.instruction("b.eq __rt_property_reference_numeric_trailing_space"); // consume trailing whitespace
    emitter.instruction("cmp w10, #0x0c");                                      // form feed?
    emitter.instruction("b.ne __rt_property_reference_numeric_done");           // trailing garbage makes it non-numeric
    emitter.label("__rt_property_reference_numeric_trailing_space");
    emitter.instruction("add x9, x9, #1");                                      // consume the whitespace byte
    emitter.instruction("b __rt_property_reference_numeric_trailing");          // continue scanning
    emitter.label("__rt_property_reference_numeric_accept");
    // -- classify the numeric spelling by value --
    emitter.instruction("cbnz x14, __rt_property_reference_numeric_float");     // fraction and exponent forms need a float check
    emitter.instruction("mov x16, #1");                                         // materialize 2^63
    emitter.instruction("lsl x16, x16, #63");                                   // the exclusive upper bound for both signs
    emitter.instruction("cbnz x15, __rt_property_reference_numeric_negative");  // select the negative limit
    emitter.instruction("sub x17, x16, #1");                                    // positive integers stop at 2^63-1
    emitter.instruction("cmp x13, x17");                                        // does the magnitude fit?
    emitter.instruction("b.hi __rt_property_reference_numeric_float");          // report a numeric spelling that is not an i64 integer
    emitter.instruction("b __rt_property_reference_numeric_integer");           // accept the integer form
    emitter.label("__rt_property_reference_numeric_negative");
    emitter.instruction("cmp x13, x16");                                        // negative integers may reach 2^63
    emitter.instruction("b.hi __rt_property_reference_numeric_float");          // report a numeric spelling that is not an i64 integer
    emitter.label("__rt_property_reference_numeric_integer");
    emitter.instruction("mov x0, #1");                                          // report an integer that fits
    emitter.instruction("b __rt_property_reference_numeric_done");              // return the classification
    emitter.label("__rt_property_reference_numeric_float");
    emitter.instruction("mov x0, #2");                                          // ask the caller to check the numeric value
    emitter.label("__rt_property_reference_numeric_done");
    // -- return the classification --
    abi::emit_return(emitter);
}

/// Emits the x86_64 numeric-string scanner.
fn emit_property_numeric_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_property_reference_numeric_x");
    // -- scanner state: byte index, magnitude, float-form flag, and sign --
    emitter.instruction("sub rsp, 32");                                         // reserve length, sign and float-form slots
    emitter.instruction("mov r11, rax");                                        // preserve the string pointer before returning the classification in rax
    emitter.instruction("mov QWORD PTR [rsp], rdx");                            // keep the string length across the `mul` instruction
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // clear the negative sign flag
    emitter.instruction("mov QWORD PTR [rsp + 16], 0");                         // 0 = exact integer form
    emitter.instruction("xor rsi, rsi");                                        // accumulated integer magnitude
    emitter.instruction("test rdx, rdx");                                       // an empty string is not numeric
    emitter.instruction("je __rt_property_reference_numeric_fail_x");           // return 0
    emitter.instruction("xor r8, r8");                                          // start at the first byte
    emitter.label("__rt_property_reference_numeric_leading_x");
    // -- skip PHP's leading whitespace bytes --
    emitter.instruction("movzx r9d, BYTE PTR [r11 + r8]");                      // inspect the current byte
    emitter.instruction("cmp r9d, 0x20");                                       // space?
    emitter.instruction("je __rt_property_reference_numeric_leading_space_x");  // consume PHP whitespace
    emitter.instruction("cmp r9d, 0x09");                                       // tab?
    emitter.instruction("je __rt_property_reference_numeric_leading_space_x");  // consume PHP whitespace
    emitter.instruction("cmp r9d, 0x0a");                                       // newline?
    emitter.instruction("je __rt_property_reference_numeric_leading_space_x");  // consume PHP whitespace
    emitter.instruction("cmp r9d, 0x0d");                                       // carriage return?
    emitter.instruction("je __rt_property_reference_numeric_leading_space_x");  // consume PHP whitespace
    emitter.instruction("cmp r9d, 0x0b");                                       // vertical tab?
    emitter.instruction("je __rt_property_reference_numeric_leading_space_x");  // consume PHP whitespace
    emitter.instruction("cmp r9d, 0x0c");                                       // form feed?
    emitter.instruction("jne __rt_property_reference_numeric_sign_x");          // start the sign scan
    emitter.label("__rt_property_reference_numeric_leading_space_x");
    emitter.instruction("add r8, 1");                                           // consume the whitespace byte
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // reached the end?
    emitter.instruction("jb __rt_property_reference_numeric_leading_x");        // keep skipping
    emitter.instruction("jmp __rt_property_reference_numeric_fail_x");          // whitespace-only is not numeric
    emitter.label("__rt_property_reference_numeric_sign_x");
    // -- consume an optional sign --
    emitter.instruction("cmp r9d, 0x2b");                                       // leading plus?
    emitter.instruction("je __rt_property_reference_numeric_sign_done_x");      // consume it
    emitter.instruction("cmp r9d, 0x2d");                                       // leading minus?
    emitter.instruction("jne __rt_property_reference_numeric_int_x");           // start with the integer digits
    emitter.instruction("mov QWORD PTR [rsp + 8], 1");                          // remember the negative sign
    emitter.label("__rt_property_reference_numeric_sign_done_x");
    emitter.instruction("add r8, 1");                                           // consume the sign
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // is anything left?
    emitter.instruction("jae __rt_property_reference_numeric_fail_x");          // a bare sign is not numeric
    emitter.label("__rt_property_reference_numeric_int_x");
    // -- accumulate the integer digits with overflow detection --
    emitter.instruction("xor ecx, ecx");                                        // count integer and fractional digits
    emitter.label("__rt_property_reference_numeric_int_loop_x");
    emitter.instruction("movzx r9d, BYTE PTR [r11 + r8]");                      // inspect the current byte
    emitter.instruction("cmp r9d, 0x30");                                       // below ASCII '0'?
    emitter.instruction("jb __rt_property_reference_numeric_int_done_x");       // leave the integer part
    emitter.instruction("cmp r9d, 0x39");                                       // above ASCII '9'?
    emitter.instruction("ja __rt_property_reference_numeric_int_done_x");       // leave the integer part
    emitter.instruction("add rcx, 1");                                          // count one digit
    emitter.instruction("sub r9d, 0x30");                                       // convert the byte to its digit value
    emitter.instruction("mov rax, rsi");                                        // seed the 128-bit multiply with the accumulator
    emitter.instruction("mov rdx, 10");                                         // multiply by ten
    emitter.instruction("mul rdx");                                             // rdx:rax = accumulator * 10
    emitter.instruction("test rdx, rdx");                                       // did the multiplication overflow?
    emitter.instruction("jnz __rt_property_reference_numeric_int_overflow_x");  // the magnitude no longer fits unsigned 64 bits
    emitter.instruction("add rax, r9");                                         // add the current digit
    emitter.instruction("jc __rt_property_reference_numeric_int_overflow_x");   // the magnitude no longer fits unsigned 64 bits
    emitter.instruction("mov rsi, rax");                                        // publish the accumulated magnitude
    emitter.instruction("add r8, 1");                                           // advance
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // reached the end?
    emitter.instruction("jb __rt_property_reference_numeric_int_loop_x");       // keep reading digits
    emitter.instruction("jmp __rt_property_reference_numeric_after_x");         // no fraction or exponent remains
    emitter.label("__rt_property_reference_numeric_int_overflow_x");
    // -- stop accumulating once the magnitude leaves unsigned 64 bits --
    emitter.instruction("mov QWORD PTR [rsp + 16], 2");                         // remember that the integer exceeded i64
    emitter.instruction("add r8, 1");                                           // consume the overflowing digit
    emitter.label("__rt_property_reference_numeric_int_skip_x");
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // reached the end?
    emitter.instruction("jae __rt_property_reference_numeric_after_x");         // the spelling is still numeric
    emitter.instruction("movzx r9d, BYTE PTR [r11 + r8]");                      // inspect the current byte
    emitter.instruction("cmp r9d, 0x30");                                       // below ASCII '0'?
    emitter.instruction("jb __rt_property_reference_numeric_int_done_x");       // leave the integer part
    emitter.instruction("cmp r9d, 0x39");                                       // above ASCII '9'?
    emitter.instruction("ja __rt_property_reference_numeric_int_done_x");       // leave the integer part
    emitter.instruction("add r8, 1");                                           // consume the digit without accumulating
    emitter.instruction("jmp __rt_property_reference_numeric_int_skip_x");      // keep consuming digits
    emitter.label("__rt_property_reference_numeric_int_done_x");
    // -- parse the fractional part when a decimal point follows --
    emitter.instruction("cmp r9d, 0x2e");                                       // decimal point?
    emitter.instruction("jne __rt_property_reference_numeric_exponent_x");      // no fractional part
    emitter.instruction("mov QWORD PTR [rsp + 16], 1");                         // mark a float-form spelling
    emitter.instruction("add r8, 1");                                           // consume the decimal point
    emitter.label("__rt_property_reference_numeric_frac_x");
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // reached the end?
    emitter.instruction("jae __rt_property_reference_numeric_after_x");         // a trailing point is numeric
    emitter.instruction("movzx r9d, BYTE PTR [r11 + r8]");                      // inspect the current byte
    emitter.instruction("cmp r9d, 0x30");                                       // below ASCII '0'?
    emitter.instruction("jb __rt_property_reference_numeric_exponent_x");       // leave the fractional part
    emitter.instruction("cmp r9d, 0x39");                                       // above ASCII '9'?
    emitter.instruction("ja __rt_property_reference_numeric_exponent_x");       // leave the fractional part
    emitter.instruction("add rcx, 1");                                          // count one fractional digit
    emitter.instruction("add r8, 1");                                           // advance
    emitter.instruction("jmp __rt_property_reference_numeric_frac_x");          // keep reading fractional digits
    emitter.label("__rt_property_reference_numeric_exponent_x");
    // -- parse an optional exponent and its sign --
    emitter.instruction("test rcx, rcx");                                       // a point alone is not numeric
    emitter.instruction("je __rt_property_reference_numeric_fail_x");           // return 0
    emitter.instruction("cmp r9d, 0x65");                                       // lowercase exponent marker?
    emitter.instruction("je __rt_property_reference_numeric_exponent_digits_x"); // parse its digits
    emitter.instruction("cmp r9d, 0x45");                                       // uppercase exponent marker?
    emitter.instruction("jne __rt_property_reference_numeric_trailing_x");      // no exponent
    emitter.label("__rt_property_reference_numeric_exponent_digits_x");
    emitter.instruction("mov QWORD PTR [rsp + 16], 1");                         // mark a float-form spelling
    emitter.instruction("add r8, 1");                                           // consume the exponent marker
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // is anything left?
    emitter.instruction("jae __rt_property_reference_numeric_fail_x");          // a bare exponent is not numeric
    emitter.instruction("movzx r9d, BYTE PTR [r11 + r8]");                      // inspect the exponent sign
    emitter.instruction("cmp r9d, 0x2b");                                       // positive exponent?
    emitter.instruction("je __rt_property_reference_numeric_exponent_sign_x");  // consume it
    emitter.instruction("cmp r9d, 0x2d");                                       // negative exponent?
    emitter.instruction("jne __rt_property_reference_numeric_exponent_first_x"); // parse digits directly
    emitter.label("__rt_property_reference_numeric_exponent_sign_x");
    emitter.instruction("add r8, 1");                                           // consume the sign
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // is anything left?
    emitter.instruction("jae __rt_property_reference_numeric_fail_x");          // a bare exponent sign is not numeric
    emitter.label("__rt_property_reference_numeric_exponent_first_x");
    emitter.instruction("xor r10d, r10d");                                      // count exponent digits
    emitter.label("__rt_property_reference_numeric_exponent_loop_x");
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // reached the end?
    emitter.instruction("jae __rt_property_reference_numeric_exponent_done_x"); // finish the exponent
    emitter.instruction("movzx r9d, BYTE PTR [r11 + r8]");                      // inspect the current byte
    emitter.instruction("cmp r9d, 0x30");                                       // below ASCII '0'?
    emitter.instruction("jb __rt_property_reference_numeric_exponent_done_x");  // leave the exponent
    emitter.instruction("cmp r9d, 0x39");                                       // above ASCII '9'?
    emitter.instruction("ja __rt_property_reference_numeric_exponent_done_x");  // leave the exponent
    emitter.instruction("add r10, 1");                                          // count one exponent digit
    emitter.instruction("add r8, 1");                                           // advance
    emitter.instruction("jmp __rt_property_reference_numeric_exponent_loop_x"); // keep reading exponent digits
    emitter.label("__rt_property_reference_numeric_exponent_done_x");
    emitter.instruction("test r10, r10");                                       // an exponent needs at least one digit
    emitter.instruction("je __rt_property_reference_numeric_fail_x");           // return 0
    emitter.instruction("jmp __rt_property_reference_numeric_trailing_x");      // validate the trailing bytes
    emitter.label("__rt_property_reference_numeric_after_x");
    emitter.instruction("test rcx, rcx");                                       // a sign with no digits is not numeric
    emitter.instruction("je __rt_property_reference_numeric_fail_x");           // return 0
    emitter.label("__rt_property_reference_numeric_trailing_x");
    // -- consume trailing PHP whitespace --
    emitter.instruction("cmp r8, QWORD PTR [rsp]");                             // reached the end?
    emitter.instruction("jae __rt_property_reference_numeric_accept_x");        // the whole spelling is numeric
    emitter.instruction("movzx r9d, BYTE PTR [r11 + r8]");                      // inspect the current byte
    emitter.instruction("cmp r9d, 0x20");                                       // space?
    emitter.instruction("je __rt_property_reference_numeric_trailing_space_x"); // consume trailing whitespace
    emitter.instruction("cmp r9d, 0x09");                                       // tab?
    emitter.instruction("je __rt_property_reference_numeric_trailing_space_x"); // consume trailing whitespace
    emitter.instruction("cmp r9d, 0x0a");                                       // newline?
    emitter.instruction("je __rt_property_reference_numeric_trailing_space_x"); // consume trailing whitespace
    emitter.instruction("cmp r9d, 0x0d");                                       // carriage return?
    emitter.instruction("je __rt_property_reference_numeric_trailing_space_x"); // consume trailing whitespace
    emitter.instruction("cmp r9d, 0x0b");                                       // vertical tab?
    emitter.instruction("je __rt_property_reference_numeric_trailing_space_x"); // consume trailing whitespace
    emitter.instruction("cmp r9d, 0x0c");                                       // form feed?
    emitter.instruction("jne __rt_property_reference_numeric_fail_x");          // trailing garbage makes it non-numeric
    emitter.label("__rt_property_reference_numeric_trailing_space_x");
    emitter.instruction("add r8, 1");                                           // consume the whitespace byte
    emitter.instruction("jmp __rt_property_reference_numeric_trailing_x");      // continue scanning
    emitter.label("__rt_property_reference_numeric_accept_x");
    // -- classify the numeric spelling by value --
    emitter.instruction("cmp QWORD PTR [rsp + 16], 0");                         // fraction and exponent forms need a float check
    emitter.instruction("jne __rt_property_reference_numeric_float_x");         // report a numeric spelling that is not an i64 integer
    abi::emit_load_int_immediate(emitter, "rdx", i64::MIN);                     // materialize 2^63 as the negative limit
    emitter.instruction("cmp QWORD PTR [rsp + 8], 0");                          // is the integer negative?
    emitter.instruction("jne __rt_property_reference_numeric_limit_x");         // keep the negative limit
    abi::emit_load_int_immediate(emitter, "rdx", i64::MAX);                     // positive integers stop at 2^63-1
    emitter.label("__rt_property_reference_numeric_limit_x");
    emitter.instruction("cmp rsi, rdx");                                        // does the magnitude fit?
    emitter.instruction("ja __rt_property_reference_numeric_float_x");          // report a numeric spelling that is not an i64 integer
    emitter.instruction("mov rax, 1");                                          // report an integer that fits
    emitter.instruction("jmp __rt_property_reference_numeric_done_x");          // return the classification
    emitter.label("__rt_property_reference_numeric_float_x");
    emitter.instruction("mov rax, 2");                                          // ask the caller to check the numeric value
    emitter.instruction("jmp __rt_property_reference_numeric_done_x");          // return the classification
    emitter.label("__rt_property_reference_numeric_fail_x");
    emitter.instruction("xor eax, eax");                                        // 0 = non-numeric
    emitter.label("__rt_property_reference_numeric_done_x");
    // -- return the classification --
    emitter.instruction("add rsp, 32");                                         // release the scanner frame
    abi::emit_return(emitter);
}

/// Emits every typed-property reference allocator, guard and numeric predicate.
pub(super) fn emit(emitter: &mut Emitter) {
    property::emit_new(emitter);
    emit_property_store_guard(emitter);
    emit_property_numeric(emitter);
}

