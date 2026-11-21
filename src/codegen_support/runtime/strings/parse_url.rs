//! Purpose:
//! Emits PHP-compatible `parse_url()` runtime helpers and target-specific byte scanners.
//! Owns component materialization, Mixed boxing, associative-array construction, and invalid-selector errors.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the string runtime registry.
//!
//! Key details:
//! - AArch64 and x86_64 mirror the Magician scanner and preserve missing versus present-empty components.
//! - Component strings are copied before ASCII control bytes are replaced, so the input string is never mutated.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

const COMPONENT_ERROR_PREFIX: &str =
    "parse_url(): Argument #2 ($component) must be a valid URL component identifier, ";
const COMPONENT_ERROR_SUFFIX: &str = " given";

const COMPONENT_KEYS: [(&str, &str); 8] = [
    ("_parse_url_key_scheme", "scheme"),
    ("_parse_url_key_host", "host"),
    ("_parse_url_key_port", "port"),
    ("_parse_url_key_user", "user"),
    ("_parse_url_key_pass", "pass"),
    ("_parse_url_key_path", "path"),
    ("_parse_url_key_query", "query"),
    ("_parse_url_key_fragment", "fragment"),
];

/// Emits the primary Mixed-returning `parse_url()` scanner and its private helpers.
pub fn emit_parse_url(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: parse_url ---");
    emitter.label_global("__rt_parse_url");
    match emitter.target.arch {
        Arch::AArch64 => emitter.raw(include_str!("parse_url/aarch64.s")),
        Arch::X86_64 => emitter.raw(&include_str!("parse_url/x86_64.s").replace(
            "{{MIXED_HEAP_KIND}}",
            &format!(
                "0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(5)
            ),
        )),
    }
    emit_parse_url_key_address(emitter);
    emit_parse_url_throw_component(emitter);
}

/// Emits the private component-index to key-pointer mapper used during hash construction.
fn emit_parse_url_key_address(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_parse_url_key_address");
    match emitter.target.arch {
        Arch::AArch64 => {
            for component in 0..COMPONENT_KEYS.len() - 1 {
                emitter.instruction(&format!("cmp x0, #{}", component));        // compare the requested component index with this key slot
                emitter.instruction(&format!("b.eq Lparse_url_key_{}", component)); // select the matching static component key
            }
            emitter.instruction("b Lparse_url_key_7");                          // component seven maps to the fragment key
            for (component, (symbol, key)) in COMPONENT_KEYS.iter().enumerate() {
                emitter.label(&format!("Lparse_url_key_{}", component));
                abi::emit_symbol_address(emitter, "x1", symbol);
                emitter.instruction(&format!("mov x2, #{}", key.len()));        // return the static component key length
                emitter.instruction("ret");                                     // return key pointer and length in the string result registers
            }
        }
        Arch::X86_64 => {
            for component in 0..COMPONENT_KEYS.len() - 1 {
                emitter.instruction(&format!("cmp rax, {}", component));        // compare the requested component index with this key slot
                emitter.instruction(&format!("je Lparse_url_key_{}", component)); // select the matching static component key
            }
            emitter.instruction("jmp Lparse_url_key_7");                        // component seven maps to the fragment key
            for (component, (symbol, key)) in COMPONENT_KEYS.iter().enumerate() {
                emitter.label(&format!("Lparse_url_key_{}", component));
                abi::emit_symbol_address(emitter, "rsi", symbol);
                emitter.instruction(&format!("mov rdx, {}", key.len()));        // return the static component key length
                emitter.instruction("ret");                                     // return key pointer and length in the hash-key registers
            }
        }
    }
}

/// Emits a dynamic `ValueError` thrower for component identifiers greater than seven.
fn emit_parse_url_throw_component(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_parse_url_throw_component");
    match emitter.target.arch {
        Arch::AArch64 => emit_parse_url_throw_component_aarch64(emitter),
        Arch::X86_64 => emit_parse_url_throw_component_x86_64(emitter),
    }
}

/// Emits the AArch64 dynamic component-selector `ValueError` construction path.
fn emit_parse_url_throw_component_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #64");                                     // reserve selector, message, and saved-register spill slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the throw-helper frame
    emitter.instruction("bl __rt_itoa");                                        // format the invalid component identifier as decimal bytes
    emitter.instruction("stp x1, x2, [sp, #0]");                                // preserve the formatted identifier across concatenation
    abi::emit_symbol_address(emitter, "x1", "_parse_url_component_error_prefix");
    emitter.instruction(&format!("mov x2, #{}", COMPONENT_ERROR_PREFIX.len())); // load the fixed diagnostic prefix length
    emitter.instruction("ldp x3, x4, [sp, #0]");                                // append the formatted component identifier
    emitter.instruction("bl __rt_concat");                                      // form prefix plus numeric component text
    abi::emit_symbol_address(emitter, "x3", "_parse_url_component_error_suffix");
    emitter.instruction(&format!("mov x4, #{}", COMPONENT_ERROR_SUFFIX.len())); // load the fixed diagnostic suffix length
    emitter.instruction("bl __rt_concat");                                      // append the trailing ` given` text
    emitter.instruction("bl __rt_str_persist");                                 // give the exception object owned message storage
    emitter.instruction("stp x1, x2, [sp, #16]");                               // preserve message pointer and length during object allocation
    emitter.instruction("mov x0, #56");                                         // request the standard Throwable payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the ValueError object
    emitter.instruction("mov x9, #6");                                          // heap kind 6 identifies object payloads
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the ValueError allocation as an object
    emitter.instruction("bl __rt_object_handle_acquire");                       // assign the object its PHP-visible handle
    abi::emit_symbol_address(emitter, "x9", "_spl_value_error_class_id");
    emitter.instruction("ldr x9, [x9]");                                        // load the program-local ValueError class id
    emitter.instruction("str x9, [x0]");                                        // install the ValueError class id
    emitter.instruction("ldp x10, x11, [sp, #16]");                             // reload the owned diagnostic message
    emitter.instruction("stp x10, x11, [x0, #8]");                              // install message pointer and byte length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    emitter.instruction("str xzr, [x0, #40]");                                  // previous exception defaults to null
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // publish the active ValueError for the unwinder
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame before unwinding
    emitter.instruction("add sp, sp, #64");                                     // discard the throw-helper frame
    emitter.instruction("b __rt_throw_current");                                // enter the catchable exception path without returning
}

/// Emits the x86_64 dynamic component-selector `ValueError` construction path.
fn emit_parse_url_throw_component_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned throw-helper frame
    emitter.instruction("sub rsp, 48");                                         // reserve numeric and message payload spill slots
    emitter.instruction("call __rt_itoa");                                      // format the invalid component identifier as decimal bytes
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve formatted identifier pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve formatted identifier length
    abi::emit_symbol_address(emitter, "rax", "_parse_url_component_error_prefix");
    emitter.instruction(&format!("mov rdx, {}", COMPONENT_ERROR_PREFIX.len())); // load the fixed diagnostic prefix length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // append the formatted component bytes
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the formatted component byte length
    emitter.instruction("call __rt_concat");                                    // form prefix plus numeric component text
    abi::emit_symbol_address(emitter, "rdi", "_parse_url_component_error_suffix");
    emitter.instruction(&format!("mov rsi, {}", COMPONENT_ERROR_SUFFIX.len())); // load the fixed diagnostic suffix length
    emitter.instruction("call __rt_concat");                                    // append the trailing ` given` text
    emitter.instruction("call __rt_str_persist");                               // give the exception object owned message storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve owned message pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve owned message length
    emitter.instruction("mov rax, 56");                                         // request the standard Throwable payload size
    emitter.instruction("call __rt_heap_alloc");                                // allocate the ValueError object
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // materialize the canonical object heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the ValueError allocation as an object
    emitter.instruction("call __rt_object_handle_acquire");                     // assign the object its PHP-visible handle
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_value_error_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // install the ValueError class id
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload owned diagnostic message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // install diagnostic message pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload diagnostic message byte length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // install diagnostic message byte length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous exception defaults to null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before unwinding
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // enter the catchable exception path without returning
}
