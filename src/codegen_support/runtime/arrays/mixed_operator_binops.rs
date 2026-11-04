//! Purpose:
//! Emits `__rt_mixed_operator_add/sub/mul`, the boxed-Mixed helpers behind PHP's `+`, `-` and `*`
//! operators, which unlike plain numeric arithmetic must also see the arrays a Mixed value holds.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed` via `crate::codegen_support::runtime::arrays`.
//! - `crate::codegen::lower_inst::arithmetic::lower_mixed_numeric_binop()`, and its integer-RHS fast
//!   path through `crate::codegen_support::abi::emit_mixed_operator_integer_rhs_call()`.
//!
//! Key details:
//! - With no array operand each entry restores its operands and tail-jumps to
//!   `__rt_mixed_numeric_add/sub/mul`, so numeric results, overflow promotion and the numeric-string
//!   rules are those helpers'. `array_sum()`, `array_product()`, string increment and the eval bridge
//!   keep calling the numeric helpers directly: their array operands follow other PHP rules.
//! - `array + array` is PHP's union. Two arrays held in Mixed values may store different element
//!   types, so each operand is cloned and widened to Mixed elements before the union helper that
//!   matches the two storage kinds runs; the clones are released and the fresh result is boxed. The
//!   operands are only read.
//! - Any other array operand, and two arrays under `-` or `*`, throw the catchable
//!   `TypeError: Unsupported operand types: <left> <op> <right>`, each operand named as
//!   `get_debug_type()` names it (an object by its class). Control does not return.
//! - Input cells are read-only and do not escape; the right operand may be the headerless caller-stack
//!   integer cell `abi::emit_mixed_operator_integer_rhs_call()` lends, which is only unboxed and named.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::UNSUPPORTED_OPERANDS_PREFIX;
use crate::codegen_support::sentinels::{
    emit_throwable_creation_line_unknown, x86_64_heap_kind_word,
};

/// The three operator entries and the opcode each one hands the shared body.
const ENTRIES: [(&str, i64); 3] = [
    ("__rt_mixed_operator_add", 0),
    ("__rt_mixed_operator_sub", 1),
    ("__rt_mixed_operator_mul", 2),
];

/// Runtime tags and the label answering each one's name.
const TYPE_NAME_LABELS: [(i64, &str); 10] = [
    (0, "__rt_mixed_operator_name_int"),
    (1, "__rt_mixed_operator_name_string"),
    (2, "__rt_mixed_operator_name_float"),
    (3, "__rt_mixed_operator_name_bool"),
    (4, "__rt_mixed_operator_name_array"),
    (5, "__rt_mixed_operator_name_array"),
    (6, "__rt_mixed_operator_name_object"),
    (8, "__rt_mixed_operator_name_null"),
    (9, "__rt_mixed_operator_name_resource"),
    (10, "__rt_mixed_operator_name_closure"),
];

/// Each fixed name: its label, its data symbol and its byte length.
const TYPE_NAMES: [(&str, &str, usize); 9] = [
    ("__rt_mixed_operator_name_int", "_unser_type_int", 3),
    ("__rt_mixed_operator_name_string", "_unser_type_string", 6),
    ("__rt_mixed_operator_name_float", "_unser_type_float", 5),
    ("__rt_mixed_operator_name_bool", "_unser_type_bool", 4),
    ("__rt_mixed_operator_name_array", "_unser_type_array", 5),
    ("__rt_mixed_operator_name_null", "_unser_type_null", 4),
    ("__rt_mixed_operator_name_resource", "_unser_type_resource", 8),
    ("__rt_mixed_operator_name_closure", "_closure_class_name", 7),
    ("__rt_mixed_operator_name_unknown", "_unser_type_unknown", 7),
];

/// Emits `__rt_mixed_operator_add/sub/mul` and their helpers for the target.
///
/// Input:  AArch64 `x0` = left Mixed*, `x1` = right Mixed*; x86_64 `rax` = left, `rdi` = right.
/// Output: a fresh owned boxed Mixed result in `x0` / `rax`, or a thrown `TypeError`.
pub fn emit_mixed_operator_binops(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_operator_binops_linux_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: mixed_operator_binops ---");
    for (label, opcode) in ENTRIES {
        emitter.label_global(label);
        emitter.instruction("sub sp, sp, #96");                                 // reserve operand, tag, payload and union slots plus frame linkage
        emitter.instruction("stp x29, x30, [sp, #80]");                         // save frame pointer and return address
        emitter.instruction("add x29, sp, #80");                                // establish a stable helper frame pointer
        abi::emit_load_int_immediate(emitter, "x9", opcode);
        emitter.instruction("b __rt_mixed_operator_common");                    // enter the shared operator implementation
    }

    // Stack: [sp] left box, [8] right box, [16] opcode, [24] left tag, [32] left payload,
    // [40] right tag, [48] right payload, [56..72] union temporaries and operand names.
    emitter.label_shared("__rt_mixed_operator_common");
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed left operand
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed right operand
    emitter.instruction("str x9, [sp, #16]");                                   // save the operator opcode across helper calls
    emitter.instruction("bl __rt_mixed_unbox");                                 // read the left runtime tag and payload
    emitter.instruction("str x0, [sp, #24]");                                   // save the left runtime tag
    emitter.instruction("str x1, [sp, #32]");                                   // save the left payload word
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right operand
    emitter.instruction("bl __rt_mixed_unbox");                                 // read the right runtime tag and payload
    emitter.instruction("str x0, [sp, #40]");                                   // save the right runtime tag
    emitter.instruction("str x1, [sp, #48]");                                   // save the right payload word

    // -- tags 4 and 5 are the indexed and associative arrays --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime tag
    emitter.instruction("sub x9, x9, #4");                                      // map the two array tags onto 0 and 1
    emitter.instruction("cmp x9, #1");                                          // is the left operand an array?
    emitter.instruction("b.ls __rt_mixed_operator_left_array");                 // an array on the left decides the operation
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the right runtime tag
    emitter.instruction("sub x9, x9, #4");                                      // map the two array tags onto 0 and 1
    emitter.instruction("cmp x9, #1");                                          // is only the right operand an array?
    emitter.instruction("b.ls __rt_mixed_operator_unsupported");                // a scalar beside an array is PHP's TypeError

    // -- no array operand: the numeric helper computes the result with the original operands --
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the boxed left operand
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the boxed right operand
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the operator opcode
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame before the tail call
    emitter.instruction("cmp x9, #1");                                          // is this the subtraction entry?
    emitter.instruction("b.eq __rt_mixed_numeric_sub");                         // numeric subtraction returns to our caller
    emitter.instruction("cmp x9, #2");                                          // is this the multiplication entry?
    emitter.instruction("b.eq __rt_mixed_numeric_mul");                         // numeric multiplication returns to our caller
    emitter.instruction("b __rt_mixed_numeric_add");                            // numeric addition returns to our caller

    emitter.label("__rt_mixed_operator_left_array");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the right runtime tag
    emitter.instruction("sub x9, x9, #4");                                      // map the two array tags onto 0 and 1
    emitter.instruction("cmp x9, #1");                                          // is the right operand an array too?
    emitter.instruction("b.hi __rt_mixed_operator_unsupported");                // an array beside a scalar is PHP's TypeError
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the operator opcode
    emitter.instruction("cbnz x9, __rt_mixed_operator_unsupported");            // arrays only combine under addition

    // -- array + array: widen copies of both, unite them by storage kind, box the result --
    emitter.instruction("ldr x0, [sp, #32]");                                   // the left array or hash
    emitter.instruction("bl __rt_mixed_operator_widen");                        // an owned Mixed-element copy of the left operand
    emitter.instruction("str x0, [sp, #56]");                                   // keep the widened left copy
    emitter.instruction("ldr x0, [sp, #48]");                                   // the right array or hash
    emitter.instruction("bl __rt_mixed_operator_widen");                        // an owned Mixed-element copy of the right operand
    emitter.instruction("str x0, [sp, #64]");                                   // keep the widened right copy
    emitter.instruction("ldr x0, [sp, #56]");                                   // classify the left copy's storage
    emitter.instruction("bl __rt_heap_kind");                                   // heap kind 3 is a hash
    emitter.instruction("str x0, [sp, #72]");                                   // keep the left storage kind
    emitter.instruction("ldr x0, [sp, #64]");                                   // classify the right copy's storage
    emitter.instruction("bl __rt_heap_kind");                                   // heap kind 3 is a hash
    emitter.instruction("mov x9, x0");                                          // keep the right storage kind
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the left storage kind
    emitter.instruction("ldr x0, [sp, #56]");                                   // union left operand
    emitter.instruction("ldr x1, [sp, #64]");                                   // union right operand
    emitter.instruction("cmp x10, #3");                                         // is the left copy a hash?
    emitter.instruction("b.eq __rt_mixed_operator_left_hash");                  // hash-left unions have their own helpers
    emitter.instruction("cmp x9, #3");                                          // is the right copy a hash?
    emitter.instruction("b.eq __rt_mixed_operator_array_hash");                 // indexed plus hash uses the cross-layout helper
    emitter.instruction("bl __rt_array_union");                                 // unite two indexed arrays
    emitter.instruction("b __rt_mixed_operator_united");                        // box the fresh union
    emitter.label("__rt_mixed_operator_left_hash");
    emitter.instruction("cmp x9, #3");                                          // is the right copy a hash too?
    emitter.instruction("b.eq __rt_mixed_operator_hash_hash");                  // two hashes use the associative helper
    emitter.instruction("bl __rt_hash_array_union");                            // unite a hash with an indexed array
    emitter.instruction("b __rt_mixed_operator_united");                        // box the fresh union
    emitter.label("__rt_mixed_operator_array_hash");
    emitter.instruction("bl __rt_array_hash_union");                            // unite an indexed array with a hash
    emitter.instruction("b __rt_mixed_operator_united");                        // box the fresh union
    emitter.label("__rt_mixed_operator_hash_hash");
    emitter.instruction("bl __rt_hash_union");                                  // unite two hashes
    emitter.label("__rt_mixed_operator_united");
    emitter.instruction("str x0, [sp, #72]");                                   // keep the fresh union result
    emitter.instruction("ldr x0, [sp, #56]");                                   // the widened left copy
    emitter.instruction("bl __rt_decref_any");                                  // release it; the union retained what it shares
    emitter.instruction("ldr x0, [sp, #64]");                                   // the widened right copy
    emitter.instruction("bl __rt_decref_any");                                  // release it; the union retained what it shares
    emitter.instruction("ldr x0, [sp, #72]");                                   // classify the union's storage
    emitter.instruction("bl __rt_heap_kind");                                   // heap kind 3 is a hash
    emitter.instruction("cmp x0, #3");                                          // did the union produce a hash?
    emitter.instruction("mov x9, #4");                                          // Mixed tag 4 boxes an indexed array
    emitter.instruction("mov x10, #5");                                         // Mixed tag 5 boxes a hash
    emitter.instruction("csel x0, x10, x9, eq");                                // choose the tag for the union's storage
    emitter.instruction("ldr x1, [sp, #72]");                                   // the union is the payload
    emitter.instruction("mov x2, xzr");                                         // containers use no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the union; the box retains it
    emitter.instruction("str x0, [sp, #56]");                                   // keep the boxed result
    emitter.instruction("ldr x0, [sp, #72]");                                   // the union's own reference
    emitter.instruction("bl __rt_decref_any");                                  // hand the union to the box alone
    emitter.instruction("ldr x0, [sp, #56]");                                   // return the boxed union
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the fresh boxed Mixed result

    // -- any other array operand: TypeError "Unsupported operand types: <left> <op> <right>" --
    emitter.label("__rt_mixed_operator_unsupported");
    emitter.instruction("ldr x0, [sp, #24]");                                   // the left runtime tag
    emitter.instruction("ldr x1, [sp, #32]");                                   // the left payload word
    emitter.instruction("bl __rt_mixed_operator_type_name");                    // name the left operand
    emitter.instruction("stp x1, x2, [sp, #56]");                               // keep the left name
    emitter.instruction("ldr x0, [sp, #40]");                                   // the right runtime tag
    emitter.instruction("ldr x1, [sp, #48]");                                   // the right payload word
    emitter.instruction("bl __rt_mixed_operator_type_name");                    // name the right operand
    emitter.instruction("str x1, [sp, #72]");                                   // keep the right name pointer
    emitter.instruction("str x2, [sp, #0]");                                    // keep the right name length; the left box is no longer read
    abi::emit_symbol_address(emitter, "x1", "_unsupported_operands_prefix");
    emitter.instruction(&format!("mov x2, #{}", UNSUPPORTED_OPERANDS_PREFIX.len())); // the prefix length
    emitter.instruction("ldp x3, x4, [sp, #56]");                               // append the left name
    emitter.instruction("bl __rt_concat");                                      // build `<prefix><left>`
    abi::emit_symbol_address(emitter, "x3", "_unsupported_operands_ops");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the operator opcode
    emitter.instruction("add x9, x9, x9, lsl #1");                              // three bytes per operator spelling
    emitter.instruction("add x3, x3, x9");                                      // select the operator's spelling
    emitter.instruction("mov x4, #3");                                          // the operator spelling's length
    emitter.instruction("bl __rt_concat");                                      // append the operator
    emitter.instruction("ldr x3, [sp, #72]");                                   // the right name pointer
    emitter.instruction("ldr x4, [sp, #0]");                                    // the right name length
    emitter.instruction("bl __rt_concat");                                      // append the right name
    emitter.instruction("bl __rt_str_persist");                                 // give the Throwable stable message ownership
    emitter.instruction("stp x1, x2, [sp]");                                    // preserve the message pair across the allocation
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
    // __rt_heap_alloc recycles blocks without zeroing, so every remaining slot is written here.
    emitter.instruction("str xzr, [x0, #24]");                                  // code = 0
    emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("str xzr, [x0, #40]");                                  // previous = null
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("b __rt_throw_current");                                // unwind, or report it uncaught and exit like PHP

    emit_widen_aarch64(emitter);
    emit_type_name_aarch64(emitter);
}

/// Emits `__rt_mixed_operator_widen` for ARM64: `x0` = an array or hash the caller only borrows;
/// returns in `x0` an owned shallow copy whose elements are boxed Mixed cells.
fn emit_widen_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mixed_operator_widen");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the source slot and frame linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp]");                                        // keep the borrowed source
    emitter.instruction("bl __rt_heap_kind");                                   // heap kind 3 is a hash
    emitter.instruction("cmp x0, #3");                                          // is the source a hash?
    emitter.instruction("b.eq __rt_mixed_operator_widen_hash");                 // hashes widen through their own helpers
    emitter.instruction("ldr x0, [sp]");                                        // the borrowed indexed array
    emitter.instruction("bl __rt_array_clone_shallow");                         // an owned copy, so widening never writes the operand
    emitter.instruction("ldr x9, [x0, #-8]");                                   // the copy's packed kind word
    emitter.instruction("lsr x9, x9, #8");                                      // move the value_type tag into the low bits
    emitter.instruction("and x1, x9, #0x7f");                                   // the copy's current value_type tag
    emitter.instruction("bl __rt_array_to_mixed");                              // box every element of the copy in place
    emitter.instruction("b __rt_mixed_operator_widen_done");                    // return the widened copy
    emitter.label("__rt_mixed_operator_widen_hash");
    emitter.instruction("ldr x0, [sp]");                                        // the borrowed hash
    emitter.instruction("bl __rt_hash_clone_shallow");                          // an owned copy, so widening never writes the operand
    emitter.instruction("bl __rt_hash_to_mixed");                               // box every entry of the copy
    emitter.label("__rt_mixed_operator_widen_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned widened copy
}

/// Emits `__rt_mixed_operator_type_name` for ARM64: `x0` = a runtime tag, `x1` = its payload word;
/// returns the operand's `get_debug_type()` name in `x1`/`x2`. A leaf: it calls nothing.
fn emit_type_name_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mixed_operator_type_name");
    for (tag, label) in TYPE_NAME_LABELS {
        emitter.instruction(&format!("cmp x0, #{tag}"));                        // does the tag name this type?
        emitter.instruction(&format!("b.eq {label}"));                          // answer its name
    }
    emitter.instruction("b __rt_mixed_operator_name_unknown");                  // any other tag has no PHP spelling
    for (label, symbol, len) in TYPE_NAMES {
        emitter.label(label);
        abi::emit_symbol_address(emitter, "x1", symbol);
        emitter.instruction(&format!("mov x2, #{len}"));                        // the name's byte length
        emitter.instruction("ret");                                             // return the name pair
    }
    emitter.label("__rt_mixed_operator_name_object");
    emitter.instruction("ldr x13, [x1]");                                       // the object's runtime class id
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_name_count", 0);
    emitter.instruction("cmp x13, x10");                                        // is the class id within the dense name table?
    emitter.instruction("b.hs __rt_mixed_operator_name_object_fallback");       // malformed ids use the generic spelling
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x13, lsl #4");                           // select the 16-byte class-name row
    emitter.instruction("ldp x1, x2, [x10]");                                   // borrow the class-name pointer and byte length
    emitter.instruction("cbz x2, __rt_mixed_operator_name_object_fallback");    // an empty name has no spelling
    emitter.instruction("ret");                                                 // PHP names an object operand by its class
    emitter.label("__rt_mixed_operator_name_object_fallback");
    abi::emit_symbol_address(emitter, "x1", "_unser_type_object");
    emitter.instruction("mov x2, #6");                                          // the bare word "object"
    emitter.instruction("ret");                                                 // return the fallback name
}

/// Emits the Linux x86_64 `__rt_mixed_operator_add/sub/mul` (see the ARM64 twin above).
fn emit_mixed_operator_binops_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_operator_binops ---");
    for (label, opcode) in ENTRIES {
        emitter.label_global(label);
        emitter.instruction("push rbp");                                        // save the caller frame pointer before nested runtime calls
        emitter.instruction("mov rbp, rsp");                                    // establish a stable helper frame pointer
        emitter.instruction("sub rsp, 80");                                     // reserve operand, tag, payload and union slots, keeping rsp aligned
        abi::emit_load_int_immediate(emitter, "r10", opcode);
        emitter.instruction("jmp __rt_mixed_operator_common_linux_x86_64");     // enter the shared operator implementation
    }

    // Frame: [rbp-8] left box, [-16] right box, [-24] opcode, [-32] left tag, [-40] left payload,
    // [-48] right tag, [-56] right payload, [-64..-80] union temporaries and operand names.
    emitter.label("__rt_mixed_operator_common_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the boxed left operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the boxed right operand
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the operator opcode across helper calls
    emitter.instruction("call __rt_mixed_unbox");                               // read the left runtime tag and payload
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the left payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right operand
    emitter.instruction("call __rt_mixed_unbox");                               // read the right runtime tag and payload
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the right runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the right payload word

    // -- tags 4 and 5 are the indexed and associative arrays --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the left runtime tag
    emitter.instruction("sub rcx, 4");                                          // map the two array tags onto 0 and 1
    emitter.instruction("cmp rcx, 1");                                          // is the left operand an array?
    emitter.instruction("jbe __rt_mixed_operator_left_array_linux_x86_64");     // an array on the left decides the operation
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the right runtime tag
    emitter.instruction("sub rcx, 4");                                          // map the two array tags onto 0 and 1
    emitter.instruction("cmp rcx, 1");                                          // is only the right operand an array?
    emitter.instruction("jbe __rt_mixed_operator_unsupported_linux_x86_64");    // a scalar beside an array is PHP's TypeError

    // -- no array operand: the numeric helper computes the result with the original operands --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the boxed left operand
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // restore the boxed right operand
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the operator opcode
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before the tail call
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("cmp r10, 1");                                          // is this the subtraction entry?
    emitter.instruction("je __rt_mixed_numeric_sub");                           // numeric subtraction returns to our caller
    emitter.instruction("cmp r10, 2");                                          // is this the multiplication entry?
    emitter.instruction("je __rt_mixed_numeric_mul");                           // numeric multiplication returns to our caller
    emitter.instruction("jmp __rt_mixed_numeric_add");                          // numeric addition returns to our caller

    emitter.label("__rt_mixed_operator_left_array_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the right runtime tag
    emitter.instruction("sub rcx, 4");                                          // map the two array tags onto 0 and 1
    emitter.instruction("cmp rcx, 1");                                          // is the right operand an array too?
    emitter.instruction("ja __rt_mixed_operator_unsupported_linux_x86_64");     // an array beside a scalar is PHP's TypeError
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // is this the addition entry?
    emitter.instruction("jne __rt_mixed_operator_unsupported_linux_x86_64");    // arrays only combine under addition

    // -- array + array: widen copies of both, unite them by storage kind, box the result --
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the left array or hash
    emitter.instruction("call __rt_mixed_operator_widen");                      // an owned Mixed-element copy of the left operand
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // keep the widened left copy
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // the right array or hash
    emitter.instruction("call __rt_mixed_operator_widen");                      // an owned Mixed-element copy of the right operand
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // keep the widened right copy
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // classify the left copy's storage
    emitter.instruction("call __rt_heap_kind");                                 // heap kind 3 is a hash
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // keep the left storage kind
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // classify the right copy's storage
    emitter.instruction("call __rt_heap_kind");                                 // heap kind 3 is a hash
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // union left operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // union right operand
    emitter.instruction("cmp QWORD PTR [rbp - 80], 3");                         // is the left copy a hash?
    emitter.instruction("je __rt_mixed_operator_left_hash_linux_x86_64");       // hash-left unions have their own helpers
    emitter.instruction("cmp rax, 3");                                          // is the right copy a hash?
    emitter.instruction("je __rt_mixed_operator_array_hash_linux_x86_64");      // indexed plus hash uses the cross-layout helper
    emitter.instruction("call __rt_array_union");                               // unite two indexed arrays
    emitter.instruction("jmp __rt_mixed_operator_united_linux_x86_64");         // box the fresh union
    emitter.label("__rt_mixed_operator_left_hash_linux_x86_64");
    emitter.instruction("cmp rax, 3");                                          // is the right copy a hash too?
    emitter.instruction("je __rt_mixed_operator_hash_hash_linux_x86_64");       // two hashes use the associative helper
    emitter.instruction("call __rt_hash_array_union");                          // unite a hash with an indexed array
    emitter.instruction("jmp __rt_mixed_operator_united_linux_x86_64");         // box the fresh union
    emitter.label("__rt_mixed_operator_array_hash_linux_x86_64");
    emitter.instruction("call __rt_array_hash_union");                          // unite an indexed array with a hash
    emitter.instruction("jmp __rt_mixed_operator_united_linux_x86_64");         // box the fresh union
    emitter.label("__rt_mixed_operator_hash_hash_linux_x86_64");
    emitter.instruction("call __rt_hash_union");                                // unite two hashes
    emitter.label("__rt_mixed_operator_united_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // keep the fresh union result
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // the widened left copy
    emitter.instruction("call __rt_decref_any");                                // release it; the union retained what it shares
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // the widened right copy
    emitter.instruction("call __rt_decref_any");                                // release it; the union retained what it shares
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // classify the union's storage
    emitter.instruction("call __rt_heap_kind");                                 // heap kind 3 is a hash
    emitter.instruction("mov r10, 4");                                          // Mixed tag 4 boxes an indexed array
    emitter.instruction("mov r11, 5");                                          // Mixed tag 5 boxes a hash
    emitter.instruction("cmp rax, 3");                                          // did the union produce a hash?
    emitter.instruction("cmove r10, r11");                                      // choose the tag for the union's storage
    emitter.instruction("mov rax, r10");                                        // the chosen Mixed tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // the union is the payload
    emitter.instruction("xor rsi, rsi");                                        // containers use no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box the union; the box retains it
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // keep the boxed result
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // the union's own reference
    emitter.instruction("call __rt_decref_any");                                // hand the union to the box alone
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // return the boxed union
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the fresh boxed Mixed result

    // -- any other array operand: TypeError "Unsupported operand types: <left> <op> <right>" --
    emitter.label("__rt_mixed_operator_unsupported_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // the left runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // the left payload word
    emitter.instruction("call __rt_mixed_operator_type_name");                  // name the left operand
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // keep the left name pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // keep the left name length
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // the right runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // the right payload word
    emitter.instruction("call __rt_mixed_operator_type_name");                  // name the right operand
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // keep the right name pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // keep the right name length; the left box is no longer read
    emitter.instruction("lea rax, [rip + _unsupported_operands_prefix]");       // concat left operand pointer
    emitter.instruction(&format!("mov rdx, {}", UNSUPPORTED_OPERANDS_PREFIX.len())); // the prefix length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // the left name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // the left name length
    emitter.instruction("call __rt_concat");                                    // build `<prefix><left>`
    emitter.instruction("lea rdi, [rip + _unsupported_operands_ops]");          // the three operator spellings
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the operator opcode
    emitter.instruction("lea r10, [r10 + r10 * 2]");                            // three bytes per operator spelling
    emitter.instruction("add rdi, r10");                                        // select the operator's spelling
    emitter.instruction("mov rsi, 3");                                          // the operator spelling's length
    emitter.instruction("call __rt_concat");                                    // append the operator
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // the right name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // the right name length
    emitter.instruction("call __rt_concat");                                    // append the right name
    emitter.instruction("call __rt_str_persist");                               // give the Throwable stable message ownership
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the message pointer across the allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the message byte length
    emitter.instruction("mov rax, 56");                                         // canonical Throwable payload size
    emitter.instruction("call __rt_heap_alloc");                                // allocate the Throwable object payload
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(6))); // magic plus kind 6 identifies a throwable object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform heap header
    emitter.instruction("call __rt_object_handle_acquire");                     // bind the Throwable to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_type_error_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // stamp the TypeError class id
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the message pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // recover the message byte length
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // message pointer
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // message byte length
    // __rt_heap_alloc recycles blocks without zeroing, so every remaining slot is written here.
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // code = 0
    emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous = null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // unwind, or report it uncaught and exit like PHP

    emit_widen_linux_x86_64(emitter);
    emit_type_name_linux_x86_64(emitter);
}

/// Emits `__rt_mixed_operator_widen` for x86_64: `rax` = an array or hash the caller only borrows;
/// returns in `rax` an owned shallow copy whose elements are boxed Mixed cells.
fn emit_widen_linux_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mixed_operator_widen");
    emitter.instruction("push rbp");                                            // save the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the source slot, keeping rsp aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the borrowed source
    emitter.instruction("call __rt_heap_kind");                                 // heap kind 3 is a hash
    emitter.instruction("cmp rax, 3");                                          // is the source a hash?
    emitter.instruction("je __rt_mixed_operator_widen_hash_linux_x86_64");      // hashes widen through their own helpers
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the borrowed indexed array
    emitter.instruction("call __rt_array_clone_shallow");                       // an owned copy, so widening never writes the operand
    emitter.instruction("mov rsi, QWORD PTR [rax - 8]");                        // the copy's packed kind word
    emitter.instruction("shr rsi, 8");                                          // move the value_type tag into the low bits
    emitter.instruction("and rsi, 0x7f");                                       // the copy's current value_type tag
    emitter.instruction("mov rdi, rax");                                        // the copy to widen
    emitter.instruction("call __rt_array_to_mixed");                            // box every element of the copy in place
    emitter.instruction("jmp __rt_mixed_operator_widen_done_linux_x86_64");     // return the widened copy
    emitter.label("__rt_mixed_operator_widen_hash_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the borrowed hash
    emitter.instruction("call __rt_hash_clone_shallow");                        // an owned copy, so widening never writes the operand
    emitter.instruction("mov rdi, rax");                                        // the copy to widen
    emitter.instruction("call __rt_hash_to_mixed");                             // box every entry of the copy
    emitter.label("__rt_mixed_operator_widen_done_linux_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned widened copy
}

/// Emits `__rt_mixed_operator_type_name` for x86_64: `rax` = a runtime tag, `rdi` = its payload
/// word; returns the operand's `get_debug_type()` name in `rax`/`rdx`. A leaf: it calls nothing.
fn emit_type_name_linux_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mixed_operator_type_name");
    for (tag, label) in TYPE_NAME_LABELS {
        emitter.instruction(&format!("cmp rax, {tag}"));                        // does the tag name this type?
        emitter.instruction(&format!("je {label}_linux_x86_64"));               // answer its name
    }
    emitter.instruction("jmp __rt_mixed_operator_name_unknown_linux_x86_64");   // any other tag has no PHP spelling
    for (label, symbol, len) in TYPE_NAMES {
        emitter.label(&format!("{label}_linux_x86_64"));
        emitter.instruction(&format!("lea rax, [rip + {symbol}]"));             // the name's bytes
        emitter.instruction(&format!("mov rdx, {len}"));                        // the name's byte length
        emitter.instruction("ret");                                             // return the name pair
    }
    emitter.label("__rt_mixed_operator_name_object_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // the object's runtime class id
    emitter.instruction("cmp r10, QWORD PTR [rip + _class_name_count]");        // is the class id within the dense name table?
    emitter.instruction("jae __rt_mixed_operator_name_object_fallback_linux_x86_64"); // malformed ids use the generic spelling
    emitter.instruction("lea r11, [rip + _class_name_entries]");                // dense class-name metadata table
    emitter.instruction("shl r10, 4");                                          // scale the class id to the 16-byte row
    emitter.instruction("mov rax, QWORD PTR [r11 + r10]");                      // borrow the class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10 + 8]");                  // borrow the class-name byte length
    emitter.instruction("test rdx, rdx");                                       // is the name non-empty?
    emitter.instruction("jz __rt_mixed_operator_name_object_fallback_linux_x86_64"); // an empty name has no spelling
    emitter.instruction("ret");                                                 // PHP names an object operand by its class
    emitter.label("__rt_mixed_operator_name_object_fallback_linux_x86_64");
    emitter.instruction("lea rax, [rip + _unser_type_object]");                 // fall back to the bare word "object"
    emitter.instruction("mov rdx, 6");                                          // the fallback name's length
    emitter.instruction("ret");                                                 // return the fallback name
}
