//! Purpose:
//! Emits the runtime helpers that throw a PHP Throwable whose message names an object's class:
//! `__rt_throw_object_not_array` (catchable `Error` for indexing an object that is not
//! `ArrayAccess`) and `__rt_throw_serialization_denied` (catchable `Exception` for serializing an
//! object PHP refuses to serialize).
//!
//! Called from:
//! - `crate::codegen_support::runtime::objects::mixed_array_get`'s object paths.
//! - `__rt_serialize_object`, through a denied class's `_class_serialize_ptrs` entry.
//!
//! Key details:
//! - PHP stops the program for `$o["k"]` on any object that does not implement `ArrayAccess`,
//!   `stdClass` included, and it does so in the quiet contexts too — `isset`, `??` and `empty`
//!   all raise, measured against 8.5. So that helper takes no warning flag: reaching it is
//!   already the error.
//! - The class name comes from the dense `_class_name_entries` metadata `get_class()` reads, so
//!   the message carries php-src's wording verbatim.
//! - `__rt_concat` reads its LEFT operand from the string-result pair and its RIGHT one from a
//!   different pair per target; both are spelled out below rather than assumed, because the
//!   two are not the same registers and only one architecture is exercised by CI.
//! - Control never returns. `__rt_throw_current` unwinds to the nearest handler, or reports the
//!   uncaught Throwable and exits like PHP when there is none.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::{
    OBJECT_NOT_ARRAY_PREFIX, OBJECT_NOT_ARRAY_SUFFIX, SERIALIZATION_DENIED_PREFIX,
    SERIALIZATION_DENIED_SUFFIX,
};
use crate::codegen_support::sentinels::{
    emit_throwable_creation_line_unknown, x86_64_heap_kind_word,
};

/// One runtime helper that throws `<prefix><Class><suffix>` as a given Throwable class.
struct ClassNamedThrow {
    /// Global label of the helper.
    label: &'static str,
    /// Prefix for the helper's local labels, unique per helper.
    local: &'static str,
    /// Data symbol and byte length of the message text before the class name.
    prefix_symbol: &'static str,
    prefix_len: usize,
    /// Data symbol and byte length of the message text after the class name.
    suffix_symbol: &'static str,
    suffix_len: usize,
    /// Data symbol holding the per-program class id of the Throwable to raise.
    class_id_symbol: &'static str,
    /// Human description of the Throwable, for the emitted comments.
    what: &'static str,
}

const OBJECT_NOT_ARRAY: ClassNamedThrow = ClassNamedThrow {
    label: "__rt_throw_object_not_array",
    local: "__rt_object_not_array",
    prefix_symbol: "_object_not_array_prefix",
    prefix_len: OBJECT_NOT_ARRAY_PREFIX.len(),
    suffix_symbol: "_object_not_array_suffix",
    suffix_len: OBJECT_NOT_ARRAY_SUFFIX.len(),
    class_id_symbol: "_spl_error_class_id",
    what: "object-not-array Error",
};

const SERIALIZATION_DENIED: ClassNamedThrow = ClassNamedThrow {
    label: "__rt_throw_serialization_denied",
    local: "__rt_serialization_denied",
    prefix_symbol: "_serialization_denied_prefix",
    prefix_len: SERIALIZATION_DENIED_PREFIX.len(),
    suffix_symbol: "_serialization_denied_suffix",
    suffix_len: SERIALIZATION_DENIED_SUFFIX.len(),
    class_id_symbol: "_spl_exception_class_id",
    what: "serialization-denied Exception",
};

/// Emits `__rt_throw_object_not_array`. Input: the unboxed object pointer (`x0` / `rdi`).
pub fn emit_throw_object_not_array(emitter: &mut Emitter) {
    emit_class_named_throw(emitter, &OBJECT_NOT_ARRAY);
}

/// Emits `__rt_throw_serialization_denied`. Input: the object pointer (`x0` / `rdi`), which is
/// how `__rt_serialize_object` calls a class's `__serialize` entry.
pub fn emit_throw_serialization_denied(emitter: &mut Emitter) {
    emit_class_named_throw(emitter, &SERIALIZATION_DENIED);
}

/// Dispatches to the target-specific emitter for one class-named throw helper.
fn emit_class_named_throw(emitter: &mut Emitter, spec: &ClassNamedThrow) {
    if emitter.target.arch == Arch::X86_64 {
        emit_class_named_throw_x86_64(emitter, spec);
        return;
    }
    emit_class_named_throw_aarch64(emitter, spec);
}

/// Emits one class-named throw helper for ARM64. Input: `x0` = the object. Never returns.
fn emit_class_named_throw_aarch64(emitter: &mut Emitter, spec: &ClassNamedThrow) {
    let fallback = format!("{}_name_fallback", spec.local);
    let ready = format!("{}_name_ready", spec.local);
    emitter.blank();
    emitter.comment(&format!("--- runtime: throw {} ---", spec.what));
    emitter.label_global(spec.label);

    // Stack (48 bytes): [sp, #0] holds the message pair across the object allocation.
    emitter.instruction("sub sp, sp, #48");                                     // reserve message state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable Throwable-construction frame

    emitter.instruction("ldr x13, [x0]");                                       // keep the class id outside the symbol helper's x9 scratch
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_name_count", 0);
    emitter.instruction("cmp x13, x10");                                        // is the class id within the dense name table?
    emitter.instruction(&format!("b.hs {fallback}"));                           // malformed ids use the generic spelling
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x13, lsl #4");                           // select the 16-byte class-name row
    emitter.instruction("ldp x11, x12, [x10]");                                 // borrow the class-name pointer and byte length
    emitter.instruction(&format!("cbnz x12, {ready}"));                         // a non-empty name is what PHP prints
    emitter.label(&fallback);
    abi::emit_symbol_address(emitter, "x11", "_unser_type_object");
    emitter.instruction("mov x12, #6");                                         // fallback length for the bare word "object"
    emitter.label(&ready);

    abi::emit_symbol_address(emitter, "x1", spec.prefix_symbol);                // concat left operand pointer
    emitter.instruction(&format!("mov x2, #{}", spec.prefix_len));              // concat left operand length
    emitter.instruction("mov x3, x11");                                         // right operand: the resolved class name
    emitter.instruction("mov x4, x12");                                         // and its byte length
    emitter.instruction("bl __rt_concat");                                      // build `<prefix><Class>`
    abi::emit_symbol_address(emitter, "x3", spec.suffix_symbol);                // right operand pointer
    emitter.instruction(&format!("mov x4, #{}", spec.suffix_len));              // right operand length
    emitter.instruction("bl __rt_concat");                                      // append the message suffix
    emitter.instruction("bl __rt_str_persist");                                 // give the Throwable stable message ownership
    emitter.instruction("stp x1, x2, [sp]");                                    // preserve the message pair across the allocation

    emitter.instruction("mov x0, #56");                                         // canonical Throwable payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the Throwable object payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 identifies a throwable object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a runtime object
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the Throwable to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "x9", spec.class_id_symbol, 0);
    emitter.instruction("str x9, [x0]");                                        // stamp the per-program Throwable class id
    emitter.instruction("ldp x10, x11, [sp]");                                  // recover the persisted message pair
    emitter.instruction("str x10, [x0, #8]");                                   // message pointer
    emitter.instruction("str x11, [x0, #16]");                                  // message byte length
    // __rt_heap_alloc recycles blocks without zeroing, so every remaining slot is written here.
    emitter.instruction("str xzr, [x0, #24]");                                  // code = 0
    emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("str xzr, [x0, #40]");                                  // previous = null
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // publish the Throwable for the unwinder
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("b __rt_throw_current");                                // unwind, or report it uncaught and exit like PHP
}

/// Emits one class-named throw helper for x86_64. Input: `rdi` = the object. Never returns.
fn emit_class_named_throw_x86_64(emitter: &mut Emitter, spec: &ClassNamedThrow) {
    let fallback = format!("{}_name_fallback", spec.local);
    let ready = format!("{}_name_ready", spec.local);
    emitter.blank();
    emitter.comment(&format!("--- runtime: throw {} ---", spec.what));
    emitter.label_global(spec.label);

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a Throwable-construction frame
    emitter.instruction("sub rsp, 32");                                         // reserve the message pair, keeping rsp aligned

    emitter.instruction("mov r13, QWORD PTR [rdi]");                            // runtime class id
    emitter.instruction("cmp r13, QWORD PTR [rip + _class_name_count]");        // is the class id within the dense name table?
    emitter.instruction(&format!("jae {fallback}"));                            // malformed ids use the generic spelling
    emitter.instruction("lea r10, [rip + _class_name_entries]");                // dense class-name metadata table
    emitter.instruction("shl r13, 4");                                          // scale the class id to the 16-byte row
    emitter.instruction("mov r11, QWORD PTR [r10 + r13]");                      // borrow the class-name pointer
    emitter.instruction("mov r12, QWORD PTR [r10 + r13 + 8]");                  // borrow the class-name byte length
    emitter.instruction("test r12, r12");                                       // is the name non-empty?
    emitter.instruction(&format!("jnz {ready}"));                               // a non-empty name is what PHP prints
    emitter.label(&fallback);
    emitter.instruction("lea r11, [rip + _unser_type_object]");                 // fall back to the bare word "object"
    emitter.instruction("mov r12, 6");                                          // fallback name length
    emitter.label(&ready);

    emitter.instruction(&format!("lea rax, [rip + {}]", spec.prefix_symbol));  // concat left operand pointer
    emitter.instruction(&format!("mov rdx, {}", spec.prefix_len));              // concat left operand length
    emitter.instruction("mov rdi, r11");                                        // right operand: the resolved class name
    emitter.instruction("mov rsi, r12");                                        // and its byte length
    abi::emit_call_label(emitter, "__rt_concat");                               // build `<prefix><Class>`
    emitter.instruction(&format!("lea rdi, [rip + {}]", spec.suffix_symbol));  // right operand pointer
    emitter.instruction(&format!("mov rsi, {}", spec.suffix_len));              // right operand length
    abi::emit_call_label(emitter, "__rt_concat");                               // append the message suffix
    abi::emit_call_label(emitter, "__rt_str_persist");                          // give the Throwable stable message ownership
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the message pointer across the allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the message byte length

    emitter.instruction("mov rax, 56");                                         // canonical Throwable payload size
    abi::emit_call_label(emitter, "__rt_heap_alloc");                           // allocate the Throwable object payload (rax = payload)
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(6))); // magic + kind 6 identifies a throwable object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform heap header
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");                // bind the Throwable to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "r10", spec.class_id_symbol, 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // stamp the per-program Throwable class id
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the message pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // recover the message byte length
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // message pointer
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // message byte length
    // __rt_heap_alloc recycles blocks without zeroing, so every remaining slot is written here.
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // code = 0
    emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous = null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the Throwable for the unwinder
    emitter.instruction("mov rsp, rbp");                                        // release the local frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // unwind, or report it uncaught and exit like PHP
}
