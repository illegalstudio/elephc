//! Purpose:
//! Builds PHP argument containers for re-entrant DOM external-entity loader callbacks.
//! Converts the bridge's flat nullable-string request into the resolver's three arguments.
//!
//! Called from:
//! - `super::emit_dom_runtime()` as private runtime helpers for `__rt_dom_host_call`.
//!
//! Key details:
//! - Request strings are persisted before the host callback can outlive the native request.
//! - The associative context owns four boxed Mixed values with php-src's exact key names.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const EXTERNAL_LOADER_FIXED_SIZE: usize = 216;
const PUBLIC_ID_VALUE_OFFSET: usize = 72;
const SYSTEM_ID_VALUE_OFFSET: usize = 96;
const CONTEXT_VALUE_OFFSETS: [usize; 4] = [120, 144, 168, 192];
const CONTEXT_KEYS: [(&str, usize); 4] = [
    ("_elephc_dom_loader_context_directory", 9),
    ("_elephc_dom_loader_context_int_sub_name", 10),
    ("_elephc_dom_loader_context_ext_sub_uri", 9),
    ("_elephc_dom_loader_context_ext_sub_system", 12),
];

/// Emits the target-specific loader request boxing, hash insertion, and argument builders.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_box_request_value_x86_64(emitter);
        emit_context_insert_x86_64(emitter);
        emit_build_args_x86_64(emitter);
        return;
    }
    emit_box_request_value_aarch64(emitter);
    emit_context_insert_aarch64(emitter);
    emit_build_args_aarch64(emitter);
}

/// Emits the AArch64 helper that boxes one validated nullable request string.
fn emit_box_request_value_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host loader box request value ---");
    emitter.label_global("__rt_dom_host_loader_box_request_value");
    emitter.instruction("sub sp, sp, #32");                                     // reserve request/value spills and the saved native frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame across Mixed allocation
    emitter.instruction("add x29, sp, #16");                                    // establish the request-value helper frame
    emitter.instruction("str x0, [sp]");                                        // preserve the flat loader request pointer
    emitter.instruction("str x1, [sp, #8]");                                    // preserve this value-record offset
    emitter.instruction("add x9, x0, x1");                                      // address the selected validated value record
    emitter.instruction("ldr w10, [x9]");                                       // load its null-or-bytes ABI tag
    emitter.instruction("cbz w10, __rt_dom_host_loader_box_null");              // canonical null needs no request byte range
    emitter.instruction("ldr x11, [x9, #8]");                                   // load the byte-section offset
    emitter.instruction("ldr x2, [x9, #16]");                                   // load the byte-string length
    emitter.instruction("add x1, x0, x11");                                     // add the request base to the byte-section offset
    emitter.instruction(&format!("add x1, x1, #{}", EXTERNAL_LOADER_FIXED_SIZE)); // address the exact request string bytes
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 boxes a PHP string
    emitter.instruction("b __rt_dom_host_loader_box_value");                    // persist and box the request string
    emitter.label("__rt_dom_host_loader_box_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 boxes canonical PHP null
    emitter.instruction("mov x1, xzr");                                         // null has no low payload
    emitter.instruction("mov x2, xzr");                                         // null has no high payload
    emitter.label("__rt_dom_host_loader_box_value");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // return one owned boxed resolver argument
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame after boxing
    emitter.instruction("add sp, sp, #32");                                     // release the request-value helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in x0
}

/// Emits the x86_64 helper that boxes one validated nullable request string.
fn emit_box_request_value_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host loader box request value ---");
    emitter.label_global("__rt_dom_host_loader_box_request_value");
    emitter.instruction("push rbp");                                            // preserve the caller frame before Mixed allocation
    emitter.instruction("mov rbp, rsp");                                        // establish the request-value helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve request and value-offset spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the flat loader request pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve this value-record offset
    emitter.instruction("lea r10, [rdi + rsi]");                                // address the selected validated value record
    emitter.instruction("cmp DWORD PTR [r10], 0");                              // is this parser metadata value PHP null?
    emitter.instruction("je __rt_dom_host_loader_box_null");                    // canonical null needs no request byte range
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // load the byte-section offset
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // load the byte-string length
    emitter.instruction(&format!("lea rdi, [rdi + rax + {}]", EXTERNAL_LOADER_FIXED_SIZE)); // address the exact request string bytes
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 boxes a PHP string
    emitter.instruction("jmp __rt_dom_host_loader_box_value");                  // persist and box the request string
    emitter.label("__rt_dom_host_loader_box_null");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 boxes canonical PHP null
    emitter.instruction("xor edi, edi");                                        // null has no low payload
    emitter.instruction("xor esi, esi");                                        // null has no high payload
    emitter.label("__rt_dom_host_loader_box_value");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // return one owned boxed resolver argument
    emitter.instruction("add rsp, 16");                                         // release request and offset spills
    emitter.instruction("pop rbp");                                             // restore the caller frame after boxing
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in rax
}

/// Emits the AArch64 helper that inserts one boxed parser field into the context hash.
fn emit_context_insert_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host loader context insert ---");
    emitter.label_global("__rt_dom_host_loader_context_insert");
    emitter.instruction("sub sp, sp, #80");                                     // reserve hash/key/request/value and ownership spills
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame across hash helpers
    emitter.instruction("add x29, sp, #64");                                    // establish the context-insertion frame
    emitter.instruction("str x0, [sp]");                                        // preserve the current context hash pointer
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the persistent context key pointer
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the context key length
    emitter.instruction("str x3, [sp, #24]");                                   // preserve the flat loader request pointer
    emitter.instruction("str x4, [sp, #32]");                                   // preserve the selected request value offset
    emitter.instruction("mov x0, x3");                                          // box helper arg0 is the flat request
    emitter.instruction("mov x1, x4");                                          // box helper arg1 is the value-record offset
    abi::emit_call_label(emitter, "__rt_dom_host_loader_box_request_value");     // create an owned boxed null-or-string value
    emitter.instruction("mov x3, x0");                                          // transfer the boxed Mixed owner into the hash value slot
    emitter.instruction("mov x4, xzr");                                         // boxed Mixed hash values use no high word
    emitter.instruction("mov x5, #7");                                          // hash value tag 7 retains a boxed Mixed child
    emitter.instruction("ldr x0, [sp]");                                        // reload the current context hash
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the persistent context key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the context key length
    abi::emit_call_label(emitter, "__rt_hash_set");                             // insert and return the possibly grown hash
    emitter.instruction("str x0, [sp, #48]");                                   // preserve the updated context hash
    emitter.instruction("ldr x0, [sp, #48]");                                   // return the possibly grown context hash
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame after insertion
    emitter.instruction("add sp, sp, #80");                                     // release the context-insertion frame
    emitter.instruction("ret");                                                 // return the updated hash in x0
}

/// Emits the x86_64 helper that inserts one boxed parser field into the context hash.
fn emit_context_insert_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host loader context insert ---");
    emitter.label_global("__rt_dom_host_loader_context_insert");
    emitter.instruction("push rbp");                                            // preserve the caller frame across hash helpers
    emitter.instruction("mov rbp, rsp");                                        // establish the context-insertion frame
    emitter.instruction("sub rsp, 48");                                         // reserve hash/key/request/value ownership spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the current context hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the persistent context key pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the context key length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the flat loader request pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // preserve the selected request value offset
    emitter.instruction("mov rdi, rcx");                                        // box helper arg0 is the flat request
    emitter.instruction("mov rsi, r8");                                         // box helper arg1 is the value-record offset
    abi::emit_call_label(emitter, "__rt_dom_host_loader_box_request_value");     // create an owned boxed null-or-string value
    emitter.instruction("mov rcx, rax");                                        // transfer the boxed Mixed owner into the hash value slot
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed hash values use no high word
    emitter.instruction("mov r9d, 7");                                          // hash value tag 7 retains a boxed Mixed child
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the current context hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the persistent context key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the context key length
    abi::emit_call_label(emitter, "__rt_hash_set");                             // insert and return the possibly grown hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the updated context hash
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the possibly grown context hash
    emitter.instruction("add rsp, 48");                                         // release the context-insertion spills
    emitter.instruction("pop rbp");                                             // restore the caller frame after insertion
    emitter.instruction("ret");                                                 // return the updated hash in rax
}

/// Emits the AArch64 helper that builds and boxes the resolver's three-argument array.
fn emit_build_args_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host loader build args ---");
    emitter.label_global("__rt_dom_host_loader_build_args");
    emitter.instruction("sub sp, sp, #96");                                     // reserve request, hash, array, values, and saved frame slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame across container allocation
    emitter.instruction("add x29, sp, #80");                                    // establish the loader-argument builder frame
    emitter.instruction("str x0, [sp]");                                        // preserve the validated flat loader request
    emitter.instruction("mov x0, #8");                                          // allocate spare capacity for four fixed context keys
    emitter.instruction("mov x1, #7");                                          // context hash values are boxed Mixed pointers
    abi::emit_call_label(emitter, "__rt_hash_new");                             // create the resolver context hash
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the current possibly grown context hash
    for ((key, key_len), value_offset) in CONTEXT_KEYS.iter().zip(CONTEXT_VALUE_OFFSETS) {
        emitter.instruction("ldr x0, [sp, #8]");                                // insert into the current context hash
        abi::emit_symbol_address(emitter, "x1", key);
        emitter.instruction(&format!("mov x2, #{}", key_len));                  // pass this php-src context key length
        emitter.instruction("ldr x3, [sp]");                                    // pass the validated loader request
        emitter.instruction(&format!("mov x4, #{}", value_offset));             // select this nullable parser metadata value
        abi::emit_call_label(emitter, "__rt_dom_host_loader_context_insert");   // insert and return the possibly grown hash
        emitter.instruction("str x0, [sp, #8]");                                // retain the updated context hash for the next key
    }
    emitter.instruction("mov x0, #3");                                          // allocate exactly three resolver arguments
    emitter.instruction("mov x1, #8");                                          // each indexed slot stores one boxed Mixed pointer
    abi::emit_call_label(emitter, "__rt_array_new");                            // create the raw resolver argument array
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("mov x12, #0x80ff");                                    // preserve indexed kind and persistent COW metadata
    emitter.instruction("and x10, x10, x12");                                   // keep only persistent array metadata
    emitter.instruction("mov x11, #7");                                         // value type 7 denotes boxed Mixed slots
    emitter.instruction("lsl x11, x11, #8");                                    // move the value tag into its kind-word lane
    emitter.instruction("orr x10, x10, x11");                                   // combine the indexed kind with its Mixed slot type
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the Mixed-slot stamp
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the raw argument array
    emitter.instruction("ldr x0, [sp]");                                        // box the public identifier from the request
    emitter.instruction(&format!("mov x1, #{}", PUBLIC_ID_VALUE_OFFSET));       // select the public identifier value record
    abi::emit_call_label(emitter, "__rt_dom_host_loader_box_request_value");     // return boxed string or null
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the raw argument array
    emitter.instruction("str x0, [x9, #24]");                                   // store public identifier at argument index zero
    emitter.instruction("mov x10, #1");                                         // publish one initialized argument
    emitter.instruction("str x10, [x9]");                                       // update the indexed-array length
    emitter.instruction("ldr x0, [sp]");                                        // box the system identifier from the request
    emitter.instruction(&format!("mov x1, #{}", SYSTEM_ID_VALUE_OFFSET));       // select the system identifier value record
    abi::emit_call_label(emitter, "__rt_dom_host_loader_box_request_value");     // return boxed string or null
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the raw argument array
    emitter.instruction("str x0, [x9, #32]");                                   // store system identifier at argument index one
    emitter.instruction("mov x10, #2");                                         // publish two initialized arguments
    emitter.instruction("str x10, [x9]");                                       // update the indexed-array length
    emitter.instruction("mov x0, #5");                                          // runtime tag 5 boxes an associative array
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload is the completed context hash
    emitter.instruction("mov x2, xzr");                                         // associative arrays use no high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // create the boxed context argument
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the boxed context while dropping the raw owner
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the raw context-hash owner
    abi::emit_call_label(emitter, "__rt_decref_any");                           // leave the boxed context as the sole hash owner
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed context argument
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the raw argument array
    emitter.instruction("str x0, [x9, #40]");                                   // store context at argument index two
    emitter.instruction("mov x10, #3");                                         // publish all three initialized arguments
    emitter.instruction("str x10, [x9]");                                       // update the final indexed-array length
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 boxes an indexed array
    emitter.instruction("mov x1, x9");                                          // payload is the raw resolver argument array
    emitter.instruction("mov x2, xzr");                                         // indexed arrays use no high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // box the complete resolver argument array
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the independent raw array owner beside the box
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame after construction
    emitter.instruction("add sp, sp, #96");                                     // release the loader-argument builder frame
    emitter.instruction("ret");                                                 // return boxed args in x0 and raw args in x1
}

/// Emits the x86_64 helper that builds and boxes the resolver's three-argument array.
fn emit_build_args_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM host loader build args ---");
    emitter.label_global("__rt_dom_host_loader_build_args");
    emitter.instruction("push rbp");                                            // preserve the caller frame across container allocation
    emitter.instruction("mov rbp, rsp");                                        // establish the loader-argument builder frame
    emitter.instruction("sub rsp, 48");                                         // reserve request, hash, array, and context-value spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the validated flat loader request
    emitter.instruction("mov edi, 8");                                          // allocate spare capacity for four fixed context keys
    emitter.instruction("mov esi, 7");                                          // context hash values are boxed Mixed pointers
    abi::emit_call_label(emitter, "__rt_hash_new");                             // create the resolver context hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the current possibly grown context hash
    for ((key, key_len), value_offset) in CONTEXT_KEYS.iter().zip(CONTEXT_VALUE_OFFSETS) {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // insert into the current context hash
        abi::emit_symbol_address(emitter, "rsi", key);
        emitter.instruction(&format!("mov edx, {}", key_len));                  // pass this php-src context key length
        emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                    // pass the validated loader request
        emitter.instruction(&format!("mov r8d, {}", value_offset));             // select this nullable parser metadata value
        abi::emit_call_label(emitter, "__rt_dom_host_loader_context_insert");   // insert and return the possibly grown hash
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                   // retain the updated context hash for the next key
    }
    emitter.instruction("mov edi, 3");                                          // allocate exactly three resolver arguments
    emitter.instruction("mov esi, 8");                                          // each indexed slot stores one boxed Mixed pointer
    abi::emit_call_label(emitter, "__rt_array_new");                            // create the raw resolver argument array
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker, indexed kind, and COW metadata
    emitter.instruction("and r10, r11");                                        // keep only persistent array metadata
    emitter.instruction("mov r11, 7");                                          // value type 7 denotes boxed Mixed slots
    emitter.instruction("shl r11, 8");                                          // move the value tag into its kind-word lane
    emitter.instruction("or r10, r11");                                         // combine the indexed kind with its Mixed slot type
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist the Mixed-slot stamp
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the raw argument array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // box the public identifier from the request
    emitter.instruction(&format!("mov esi, {}", PUBLIC_ID_VALUE_OFFSET));       // select the public identifier value record
    abi::emit_call_label(emitter, "__rt_dom_host_loader_box_request_value");     // return boxed string or null
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the raw argument array
    emitter.instruction("mov QWORD PTR [r10 + 24], rax");                       // store public identifier at argument index zero
    emitter.instruction("mov QWORD PTR [r10], 1");                              // publish one initialized argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // box the system identifier from the request
    emitter.instruction(&format!("mov esi, {}", SYSTEM_ID_VALUE_OFFSET));       // select the system identifier value record
    abi::emit_call_label(emitter, "__rt_dom_host_loader_box_request_value");     // return boxed string or null
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the raw argument array
    emitter.instruction("mov QWORD PTR [r10 + 32], rax");                       // store system identifier at argument index one
    emitter.instruction("mov QWORD PTR [r10], 2");                              // publish two initialized arguments
    emitter.instruction("mov eax, 5");                                          // runtime tag 5 boxes an associative array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // payload is the completed context hash
    emitter.instruction("xor esi, esi");                                        // associative arrays use no high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // create the boxed context argument
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the boxed context while dropping the raw owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the raw context-hash owner
    abi::emit_call_label(emitter, "__rt_decref_any");                           // leave the boxed context as the sole hash owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed context argument
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the raw argument array
    emitter.instruction("mov QWORD PTR [r10 + 40], rax");                       // store context at argument index two
    emitter.instruction("mov QWORD PTR [r10], 3");                              // publish all three initialized arguments
    emitter.instruction("mov eax, 4");                                          // runtime tag 4 boxes an indexed array
    emitter.instruction("mov rdi, r10");                                        // payload is the raw resolver argument array
    emitter.instruction("xor esi, esi");                                        // indexed arrays use no high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // box the complete resolver argument array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // return the independent raw array owner beside the box
    emitter.instruction("add rsp, 48");                                         // release loader-argument builder spills
    emitter.instruction("pop rbp");                                             // restore the caller frame after construction
    emitter.instruction("ret");                                                 // return boxed args in rax and raw args in rdi
}
