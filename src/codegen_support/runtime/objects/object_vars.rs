//! Purpose:
//! Emits object-to-associative-array projection for `get_object_vars()` and
//! explicit PHP object casts.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Both projections use serialize descriptors: lexical visibility filtering
//!   demangles accessible keys, while casts retain PHP's mangled keys.
//! - Protected filtering uses each descriptor row's declaring class, and copied
//!   dynamic-property names are normalized with PHP array-key semantics.
//! - Incomplete objects use class id `-2` and own their original class name at
//!   offsets 8/16 plus an opaque boxed-Mixed property hash at offset 24.

use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_object_to_hash(object, cast_mode, scope_class_id)` for the active target.
///
/// `cast_mode == 0` exposes properties visible from `scope_class_id`, with `-1`
/// representing global scope. A non-zero mode exposes every declared property
/// under its PHP visibility-mangled cast key. Both modes append dynamic
/// properties and expose the synthetic incomplete-class marker plus retained
/// opaque values.
pub(crate) fn emit_object_to_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_object_to_hash_x86_64(emitter);
    } else {
        emit_object_to_hash_aarch64(emitter);
    }
}

/// Emits the AArch64 object projection helper.
fn emit_object_to_hash_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_to_hash ---");
    emitter.label_global("__rt_object_to_hash");

    // Frame slots: object, mode, result, descriptor, index, count, scope,
    // key pointer/length, slot offset, tag, low/high payload, fp/lr.
    emitter.instruction("sub sp, sp, #128");                                    // reserve the object-projection helper frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // preserve the caller frame before nested runtime calls
    emitter.instruction("add x29, sp, #112");                                   // establish a stable frame for projection spill slots
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer slot across nested runtime calls
    emitter.instruction("str x1, [sp, #8]");                                    // save the cast-mode slot across nested runtime calls
    emitter.instruction("str x2, [sp, #48]");                                   // save the lexical class id or global -1 sentinel
    emitter.instruction("cbz x0, __rt_object_to_hash_empty");                   // use `__rt_object_to_hash_empty` when the object or metadata is absent
    emitter.instruction("ldr x9, [x0]");                                        // load the current object/descriptor operand for `ldr x9, [x0]`
    emitter.instruction("cmn x9, #2");                                          // evaluate `cmn x9, #2` before selecting the projection branch
    emitter.instruction("b.eq __rt_object_to_hash_incomplete");                 // route class id -2 through `__rt_object_to_hash_incomplete`

    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the current object/descriptor operand for `ldr x10, [x10]`
    emitter.instruction("cmp x9, x10");                                         // evaluate `cmp x9, x10` before selecting the projection branch
    emitter.instruction("b.hs __rt_object_to_hash_empty");                      // use `__rt_object_to_hash_empty` when the object or metadata is absent
    abi::emit_symbol_address(emitter, "x11", "_class_serprop_ptrs");
    emitter.instruction("ldr x11, [x11, x9, lsl #3]");                          // load the current object/descriptor operand for `ldr x11, [x11, x9, lsl #3]`
    emitter.instruction("ldr x12, [x11]");                                      // load the current object/descriptor operand for `ldr x12, [x11]`
    emitter.instruction("str x11, [sp, #24]");                                  // save the descriptor-pointer slot across nested runtime calls
    emitter.instruction("str x12, [sp, #40]");                                  // save the declared-property count slot across nested runtime calls
    emitter.instruction("lsl x0, x12, #1");                                     // derive the current descriptor/property offset with `lsl x0, x12, #1`
    emitter.instruction("add x0, x0, #16");                                     // derive the current descriptor/property offset with `add x0, x0, #16`
    emitter.instruction("mov x1, #7");                                          // prepare the projection argument or result with `mov x1, #7`
    emitter.instruction("bl __rt_hash_new");                                    // call `__rt_hash_new` with the prepared projection arguments
    emitter.instruction("str x0, [sp, #16]");                                   // save the result-hash slot across nested runtime calls
    emitter.instruction("str xzr, [sp, #32]");                                  // save the descriptor-row index slot across nested runtime calls

    emitter.label("__rt_object_to_hash_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load the descriptor-row index slot for `ldr x9, [sp, #32]`
    emitter.instruction("ldr x10, [sp, #40]");                                  // load the declared-property count slot for `ldr x10, [sp, #40]`
    emitter.instruction("cmp x9, x10");                                         // evaluate `cmp x9, x10` before selecting the projection branch
    emitter.instruction("b.ge __rt_object_to_hash_dynamic");                    // continue at `__rt_object_to_hash_dynamic` after declared rows are exhausted
    emitter.instruction("ldr x11, [sp, #24]");                                  // load the descriptor-pointer slot for `ldr x11, [sp, #24]`
    emitter.instruction("lsl x12, x9, #5");                                     // derive the current descriptor/property offset with `lsl x12, x9, #5`
    emitter.instruction("add x11, x11, #8");                                    // derive the current descriptor/property offset with `add x11, x11, #8`
    emitter.instruction("add x11, x11, x12");                                   // derive the current descriptor/property offset with `add x11, x11, x12`
    emitter.instruction("ldr x13, [x11]");                                      // load the current object/descriptor operand for `ldr x13, [x11]`
    emitter.instruction("ldr x14, [x11, #8]");                                  // load the current object/descriptor operand for `ldr x14, [x11, #8]`
    emitter.instruction("ldr x15, [x11, #16]");                                 // load the current object/descriptor operand for `ldr x15, [x11, #16]`
    emitter.instruction("ldr x16, [x11, #24]");                                 // load the current object/descriptor operand for `ldr x16, [x11, #24]`
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload cast mode before applying get_object_vars visibility
    emitter.instruction("cbnz x9, __rt_object_to_hash_row_ready");              // casts retain every serialize-mangled declared property key
    emitter.instruction("ldrb w9, [x13]");                                      // inspect the first key byte to distinguish public from mangled visibility
    emitter.instruction("cbnz x9, __rt_object_to_hash_row_ready");              // ordinary public names are globally visible without demangling
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the lexical class id for protected/private visibility
    emitter.instruction("cmp x9, #0");                                          // global scope is encoded as a negative class id
    emitter.instruction("b.lt __rt_object_to_hash_next");                       // global calls skip every protected or private property
    emitter.instruction("ldrb w10, [x13, #1]");                                 // inspect the mangled owner marker after the leading NUL
    emitter.instruction("cmp w10, #42");                                        // ASCII '*' denotes a protected property key
    emitter.instruction("b.ne __rt_object_to_hash_private");                    // other mangled keys carry a private declaring-class name
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the runtime object before resolving property declaration metadata
    emitter.instruction("ldr x10, [x10]");                                      // load the runtime object's dense class id
    abi::emit_symbol_address(emitter, "x11", "_class_serprop_declaring_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // select the runtime class's declaring-class id table
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the current serialize-property row index
    emitter.instruction("ldr x17, [x11, x10, lsl #3]");                         // load this property's declaring class id
    emitter.instruction("mov x10, x17");                                        // seed the first ancestry walk with the declaring class id
    emitter.label("__rt_object_to_hash_protected_declaring_walk");
    emitter.instruction("cmp x10, x9");                                         // is the lexical scope an ancestor of the declaring class?
    emitter.instruction("b.eq __rt_object_to_hash_protected_visible");          // related declaration ancestry grants protected visibility
    emitter.instruction("cmp x10, #0");                                         // stop before indexing the parent table with its -1 sentinel
    emitter.instruction("b.lt __rt_object_to_hash_protected_scope_start");      // try the reverse ancestry direction when needed
    abi::emit_symbol_address(emitter, "x11", "_class_parent_ids");
    emitter.instruction("ldr x10, [x11, x10, lsl #3]");                         // advance to the declaring class's parent class id
    emitter.instruction("b __rt_object_to_hash_protected_declaring_walk");      // continue walking declaration ancestors
    emitter.label("__rt_object_to_hash_protected_scope_start");
    emitter.instruction("mov x10, x9");                                         // seed the reverse walk with the lexical scope class id
    emitter.label("__rt_object_to_hash_protected_scope_walk");
    emitter.instruction("cmp x10, x17");                                        // is the declaring class an ancestor of lexical scope?
    emitter.instruction("b.eq __rt_object_to_hash_protected_visible");          // reverse ancestry also grants protected visibility
    emitter.instruction("cmp x10, #0");                                         // stop before indexing the parent table with -1
    emitter.instruction("b.lt __rt_object_to_hash_next");                       // unrelated scopes cannot observe the protected property
    abi::emit_symbol_address(emitter, "x12", "_class_parent_ids");
    emitter.instruction("ldr x10, [x12, x10, lsl #3]");                         // advance to the lexical scope's parent class id
    emitter.instruction("b __rt_object_to_hash_protected_scope_walk");          // continue walking lexical-scope ancestors
    emitter.label("__rt_object_to_hash_protected_visible");
    emitter.instruction("add x13, x13, #3");                                    // strip the protected NUL-star-NUL key prefix
    emitter.instruction("sub x14, x14, #3");                                    // expose only the plain protected property-name length
    emitter.instruction("b __rt_object_to_hash_row_ready");                     // insert the now-visible protected property
    emitter.label("__rt_object_to_hash_private");
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // resolve lexical class-name metadata by dense class id
    emitter.instruction("ldr x11, [x10]");                                      // load the lexical class-name pointer
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the lexical class-name byte length
    emitter.instruction("sub x10, x14, #2");                                    // private owner plus property text excludes two NUL separators
    emitter.instruction("cmp x10, x12");                                        // owner length must leave at least one property-name byte
    emitter.instruction("b.lt __rt_object_to_hash_next");                       // malformed or shorter keys cannot match lexical private scope
    emitter.instruction("add x13, x13, #1");                                    // advance from the leading NUL to the private owner text
    emitter.instruction("mov x10, #0");                                         // initialize the private owner byte comparator
    emitter.label("__rt_object_to_hash_private_compare");
    emitter.instruction("cmp x10, x12");                                        // have all lexical class-name bytes matched?
    emitter.instruction("b.ge __rt_object_to_hash_private_separator");          // validate the owner terminator after a full match
    emitter.instruction("ldrb w17, [x13, x10]");                                // load the next private declaring-class byte
    emitter.instruction("ldrb w9, [x11, x10]");                                 // load the matching lexical class-name byte
    emitter.instruction("cmp w17, w9");                                         // private visibility requires byte-identical declaring class names
    emitter.instruction("b.ne __rt_object_to_hash_next");                       // mismatched owners remain invisible in this lexical scope
    emitter.instruction("add x10, x10, #1");                                    // advance the private owner comparator
    emitter.instruction("b __rt_object_to_hash_private_compare");               // continue comparing owner bytes
    emitter.label("__rt_object_to_hash_private_separator");
    emitter.instruction("ldrb w9, [x13, x12]");                                 // inspect the NUL separator after the private owner
    emitter.instruction("cbnz w9, __rt_object_to_hash_next");                   // longer or different owner names do not grant private visibility
    emitter.instruction("add x13, x13, x12");                                   // advance from owner start to its terminating NUL
    emitter.instruction("add x13, x13, #1");                                    // advance to the plain private property name
    emitter.instruction("sub x14, x14, x12");                                   // remove the declaring-class bytes from key length
    emitter.instruction("sub x14, x14, #2");                                    // remove both NUL separators from key length
    emitter.label("__rt_object_to_hash_row_ready");
    emitter.instruction("str x13, [sp, #56]");                                  // save the property-key pointer slot across nested runtime calls
    emitter.instruction("str x14, [sp, #64]");                                  // save the property-key length slot across nested runtime calls
    emitter.instruction("str x15, [sp, #72]");                                  // save the property-storage offset slot across nested runtime calls
    emitter.instruction("str x16, [sp, #80]");                                  // save the property runtime-tag slot across nested runtime calls
    emitter.instruction("ldr x17, [sp, #0]");                                   // load the object pointer slot for `ldr x17, [sp, #0]`
    emitter.instruction("add x17, x17, x15");                                   // derive the current descriptor/property offset with `add x17, x17, x15`
    emitter.instruction("ldr x1, [x17]");                                       // load the current object/descriptor operand for `ldr x1, [x17]`
    emitter.instruction("ldr x2, [x17, #8]");                                   // load the current object/descriptor operand for `ldr x2, [x17, #8]`
    emitter.instruction("str x1, [sp, #88]");                                   // save the property low-word slot across nested runtime calls
    emitter.instruction("str x2, [sp, #96]");                                   // save the property high-word slot across nested runtime calls
    abi::emit_load_int_immediate(emitter, "x10", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x2, x10");                                         // evaluate `cmp x2, x10` before selecting the projection branch
    emitter.instruction("b.eq __rt_object_to_hash_next");                       // skip or advance the current property via `__rt_object_to_hash_next`
    emitter.instruction("cmp x16, #11");                                        // does the descriptor describe an inline nullable scalar?
    emitter.instruction("csel x0, x2, x16, eq");                                // box tagged scalars using their stored int-or-null tag
    emitter.instruction("csel x2, xzr, x2, eq");                                // tagged scalar payloads have no third value word
    emitter.instruction("bl __rt_mixed_from_value");                            // call `__rt_mixed_from_value` with the prepared projection arguments
    emitter.instruction("mov x3, x0");                                          // prepare the projection argument or result with `mov x3, x0`
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the result-hash slot for `ldr x0, [sp, #16]`
    emitter.instruction("ldr x1, [sp, #56]");                                   // load the property-key pointer slot for `ldr x1, [sp, #56]`
    emitter.instruction("ldr x2, [sp, #64]");                                   // load the property-key length slot for `ldr x2, [sp, #64]`
    emitter.instruction("mov x4, xzr");                                         // prepare the projection argument or result with `mov x4, xzr`
    emitter.instruction("mov x5, #7");                                          // prepare the projection argument or result with `mov x5, #7`
    emitter.instruction("bl __rt_hash_set");                                    // call `__rt_hash_set` with the prepared projection arguments
    emitter.instruction("str x0, [sp, #16]");                                   // save the result-hash slot across nested runtime calls
    emitter.label("__rt_object_to_hash_next");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load the descriptor-row index slot for `ldr x9, [sp, #32]`
    emitter.instruction("add x9, x9, #1");                                      // derive the current descriptor/property offset with `add x9, x9, #1`
    emitter.instruction("str x9, [sp, #32]");                                   // save the descriptor-row index slot across nested runtime calls
    emitter.instruction("b __rt_object_to_hash_loop");                          // continue the descriptor scan at `__rt_object_to_hash_loop`

    emitter.label("__rt_object_to_hash_dynamic");
    emitter.instruction("ldr x9, [sp, #0]");                                    // load the object pointer slot for `ldr x9, [sp, #0]`
    emitter.instruction("ldr x10, [x9]");                                       // load the current object/descriptor operand for `ldr x10, [x9]`
    abi::emit_symbol_address(emitter, "x11", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // load the current object/descriptor operand for `ldr x11, [x11, x10, lsl #3]`
    emitter.instruction("cbz x11, __rt_object_to_hash_done");                   // finish the projection through `__rt_object_to_hash_done`
    abi::emit_symbol_address(emitter, "x11", "_class_object_payload_sizes");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // load the current object/descriptor operand for `ldr x11, [x11, x10, lsl #3]`
    emitter.instruction("sub x11, x11, #8");                                    // derive the current descriptor/property offset with `sub x11, x11, #8`
    emitter.instruction("ldr x1, [x9, x11]");                                   // load the current object/descriptor operand for `ldr x1, [x9, x11]`
    emitter.instruction("cbz x1, __rt_object_to_hash_done");                    // finish the projection through `__rt_object_to_hash_done`
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the result-hash slot for `ldr x0, [sp, #16]`
    emitter.instruction("bl __rt_hash_project_spread");                         // copy dynamic properties while normalizing PHP array keys
    emitter.instruction("str x0, [sp, #16]");                                   // save the result-hash slot across nested runtime calls
    emitter.instruction("b __rt_object_to_hash_done");                          // finish the projection through `__rt_object_to_hash_done`

    emitter.label("__rt_object_to_hash_incomplete");
    emitter.instruction("ldr x12, [x0, #24]");                                  // load the current object/descriptor operand for `ldr x12, [x0, #24]`
    emitter.instruction("cbz x12, __rt_object_to_hash_empty");                  // use `__rt_object_to_hash_empty` when the object or metadata is absent
    emitter.instruction("ldr x9, [x12]");                                       // load the current object/descriptor operand for `ldr x9, [x12]`
    emitter.instruction("lsl x0, x9, #1");                                      // derive the current descriptor/property offset with `lsl x0, x9, #1`
    emitter.instruction("add x0, x0, #16");                                     // derive the current descriptor/property offset with `add x0, x0, #16`
    emitter.instruction("mov x1, #7");                                          // prepare the projection argument or result with `mov x1, #7`
    emitter.instruction("bl __rt_hash_new");                                    // call `__rt_hash_new` with the prepared projection arguments
    emitter.instruction("str x0, [sp, #16]");                                   // save the result-hash slot across nested runtime calls
    emitter.instruction("ldr x9, [sp, #0]");                                    // load the object pointer slot for `ldr x9, [sp, #0]`
    emitter.instruction("mov x0, #1");                                          // prepare the projection argument or result with `mov x0, #1`
    emitter.instruction("ldr x1, [x9, #8]");                                    // load the current object/descriptor operand for `ldr x1, [x9, #8]`
    emitter.instruction("ldr x2, [x9, #16]");                                   // load the current object/descriptor operand for `ldr x2, [x9, #16]`
    emitter.instruction("bl __rt_mixed_from_value");                            // call `__rt_mixed_from_value` with the prepared projection arguments
    emitter.instruction("mov x3, x0");                                          // prepare the projection argument or result with `mov x3, x0`
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the result-hash slot for `ldr x0, [sp, #16]`
    abi::emit_symbol_address(emitter, "x1", "_incomplete_class_property_name");
    emitter.instruction("mov x2, #27");                                         // prepare the projection argument or result with `mov x2, #27`
    emitter.instruction("mov x4, xzr");                                         // prepare the projection argument or result with `mov x4, xzr`
    emitter.instruction("mov x5, #7");                                          // prepare the projection argument or result with `mov x5, #7`
    emitter.instruction("bl __rt_hash_set");                                    // call `__rt_hash_set` with the prepared projection arguments
    emitter.instruction("str x0, [sp, #16]");                                   // save the result-hash slot across nested runtime calls
    emitter.instruction("ldr x1, [sp, #0]");                                    // load the object pointer slot for `ldr x1, [sp, #0]`
    emitter.instruction("ldr x1, [x1, #24]");                                   // load the current object/descriptor operand for `ldr x1, [x1, #24]`
    emitter.instruction("bl __rt_hash_project_spread");                         // copy retained incomplete-object properties with PHP array keys
    emitter.instruction("str x0, [sp, #16]");                                   // save the result-hash slot across nested runtime calls
    emitter.instruction("b __rt_object_to_hash_done");                          // finish the projection through `__rt_object_to_hash_done`

    emitter.label("__rt_object_to_hash_empty");
    emitter.instruction("mov x0, #16");                                         // prepare the projection argument or result with `mov x0, #16`
    emitter.instruction("mov x1, #7");                                          // prepare the projection argument or result with `mov x1, #7`
    emitter.instruction("bl __rt_hash_new");                                    // call `__rt_hash_new` with the prepared projection arguments
    emitter.instruction("str x0, [sp, #16]");                                   // save the result-hash slot across nested runtime calls
    emitter.label("__rt_object_to_hash_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the result-hash slot for `ldr x0, [sp, #16]`
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore the caller frame before returning
    emitter.instruction("add sp, sp, #128");                                    // release the object-projection helper frame
    emitter.instruction("ret");                                                 // return the fresh associative property table
}

/// Emits the x86_64 System V object projection helper.
fn emit_object_to_hash_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_to_hash ---");
    emitter.label_global("__rt_object_to_hash");
    emitter.instruction("push rbp");                                            // preserve the caller frame before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for projection spill slots
    emitter.instruction("sub rsp, 128");                                        // reserve the object-projection helper frame
    emitter.instruction("mov QWORD PTR [rbp - 96], r12");                       // save the saved r12 slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 104], r13");                      // save the saved r13 slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 112], r14");                      // save the saved r14 slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 120], r15");                      // save the saved r15 slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the object pointer slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the cast-mode slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the lexical class id or global -1 sentinel
    emitter.instruction("test rax, rax");                                       // evaluate `test rax, rax` before selecting the projection branch
    emitter.instruction("jz __rt_object_to_hash_empty_x");                      // use `__rt_object_to_hash_empty_x` when the object or metadata is absent
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // load the current object/descriptor operand for `mov r9, QWORD PTR [rax]`
    emitter.instruction("cmp r9, -2");                                          // evaluate `cmp r9, -2` before selecting the projection branch
    emitter.instruction("je __rt_object_to_hash_incomplete_x");                 // route class id -2 through `__rt_object_to_hash_incomplete_x`
    emitter.instruction("mov r10, QWORD PTR [rip + _class_gc_desc_count]");     // load the current object/descriptor operand for `mov r10, QWORD PTR [rip + _class_gc_desc_count]`
    emitter.instruction("cmp r9, r10");                                         // evaluate `cmp r9, r10` before selecting the projection branch
    emitter.instruction("jae __rt_object_to_hash_empty_x");                     // use `__rt_object_to_hash_empty_x` when the object or metadata is absent
    emitter.instruction("lea r10, [rip + _class_serprop_ptrs]");                // materialize the metadata symbol used by `lea r10, [rip + _class_serprop_ptrs]`
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");                   // load the current object/descriptor operand for `mov r10, QWORD PTR [r10 + r9 * 8]`
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current object/descriptor operand for `mov r11, QWORD PTR [r10]`
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the descriptor-pointer slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the declared-property count slot across nested runtime calls
    emitter.instruction("lea rdi, [r11 * 2 + 16]");                             // derive the fresh hash capacity with `lea rdi, [r11 * 2 + 16]`
    emitter.instruction("mov rsi, 7");                                          // prepare the projection argument or result with `mov rsi, 7`
    emitter.instruction("call __rt_hash_new");                                  // call `__rt_hash_new` with the prepared projection arguments
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result-hash slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // save the descriptor-row index slot across nested runtime calls

    emitter.label("__rt_object_to_hash_loop_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // load the descriptor-row index slot for `mov r9, QWORD PTR [rbp - 40]`
    emitter.instruction("cmp r9, QWORD PTR [rbp - 48]");                        // evaluate `cmp r9, QWORD PTR [rbp - 48]` before selecting the projection branch
    emitter.instruction("jge __rt_object_to_hash_dynamic_x");                   // continue at `__rt_object_to_hash_dynamic_x` after declared rows are exhausted
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // load the descriptor-pointer slot for `mov r10, QWORD PTR [rbp - 32]`
    emitter.instruction("mov r11, r9");                                         // prepare the projection argument or result with `mov r11, r9`
    emitter.instruction("shl r11, 5");                                          // derive the current descriptor/property offset with `shl r11, 5`
    emitter.instruction("add r10, 8");                                          // derive the current descriptor/property offset with `add r10, 8`
    emitter.instruction("add r10, r11");                                        // derive the current descriptor/property offset with `add r10, r11`
    emitter.instruction("mov r12, QWORD PTR [r10]");                            // load the current object/descriptor operand for `mov r12, QWORD PTR [r10]`
    emitter.instruction("mov r13, QWORD PTR [r10 + 8]");                        // load the current object/descriptor operand for `mov r13, QWORD PTR [r10 + 8]`
    emitter.instruction("mov r14, QWORD PTR [r10 + 16]");                       // load the current object/descriptor operand for `mov r14, QWORD PTR [r10 + 16]`
    emitter.instruction("mov r15, QWORD PTR [r10 + 24]");                       // load the current object/descriptor operand for `mov r15, QWORD PTR [r10 + 24]`
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // decide whether visibility filtering applies to this serialize row
    emitter.instruction("jne __rt_object_to_hash_row_ready_x");                 // casts retain every serialize-mangled declared property key
    emitter.instruction("cmp BYTE PTR [r12], 0");                               // distinguish public names from visibility-mangled keys
    emitter.instruction("jne __rt_object_to_hash_row_ready_x");                 // ordinary public names are globally visible
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the lexical class id for protected/private visibility
    emitter.instruction("cmp r8, 0");                                           // global scope is encoded as a negative class id
    emitter.instruction("jl __rt_object_to_hash_next_x");                       // global calls skip every protected or private property
    emitter.instruction("cmp BYTE PTR [r12 + 1], 42");                          // ASCII '*' denotes a protected property key
    emitter.instruction("jne __rt_object_to_hash_private_x");                   // other mangled keys carry a private declaring-class name
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the runtime object before resolving declaration metadata
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the runtime object's dense class id
    emitter.instruction("lea r10, [rip + _class_serprop_declaring_ptrs]");      // materialize per-class property declaration metadata
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");                   // select the runtime class's declaring-class id table
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the current serialize-property row index
    emitter.instruction("mov r11, QWORD PTR [r10 + r9 * 8]");                   // load this property's declaring class id
    emitter.instruction("mov QWORD PTR [rbp - 80], r11");                       // preserve the declaring class id across both ancestry walks
    emitter.instruction("mov r9, r11");                                         // seed the first ancestry walk with the declaring class id
    emitter.label("__rt_object_to_hash_protected_declaring_walk_x");
    emitter.instruction("cmp r9, r8");                                          // is lexical scope an ancestor of the declaring class?
    emitter.instruction("je __rt_object_to_hash_protected_visible_x");          // related declaration ancestry grants protected visibility
    emitter.instruction("cmp r9, 0");                                           // stop before indexing the parent table with -1
    emitter.instruction("jl __rt_object_to_hash_protected_scope_start_x");      // try the reverse ancestry direction when needed
    emitter.instruction("lea r10, [rip + _class_parent_ids]");                  // materialize the dense parent-id table
    emitter.instruction("mov r9, QWORD PTR [r10 + r9 * 8]");                    // advance to the declaring class's parent class id
    emitter.instruction("jmp __rt_object_to_hash_protected_declaring_walk_x");  // continue walking declaration ancestors
    emitter.label("__rt_object_to_hash_protected_scope_start_x");
    emitter.instruction("mov r9, r8");                                          // seed the reverse walk with the lexical scope class id
    emitter.label("__rt_object_to_hash_protected_scope_walk_x");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 80]");                        // is the declaring class an ancestor of lexical scope?
    emitter.instruction("je __rt_object_to_hash_protected_visible_x");          // reverse ancestry also grants protected visibility
    emitter.instruction("cmp r9, 0");                                           // stop before indexing the parent table with -1
    emitter.instruction("jl __rt_object_to_hash_next_x");                       // unrelated scopes cannot observe the protected property
    emitter.instruction("lea r11, [rip + _class_parent_ids]");                  // materialize the dense parent-id table for reverse walking
    emitter.instruction("mov r9, QWORD PTR [r11 + r9 * 8]");                    // advance to the lexical scope's parent class id
    emitter.instruction("jmp __rt_object_to_hash_protected_scope_walk_x");      // continue walking lexical-scope ancestors
    emitter.label("__rt_object_to_hash_protected_visible_x");
    emitter.instruction("add r12, 3");                                          // strip the protected NUL-star-NUL key prefix
    emitter.instruction("sub r13, 3");                                          // expose only the plain protected property-name length
    emitter.instruction("jmp __rt_object_to_hash_row_ready_x");                 // insert the now-visible protected property
    emitter.label("__rt_object_to_hash_private_x");
    emitter.instruction("lea r9, [rip + _class_name_entries]");                 // materialize lexical class-name metadata
    emitter.instruction("shl r8, 4");                                           // scale lexical class id by the 16-byte name row
    emitter.instruction("add r9, r8");                                          // resolve the lexical class-name metadata row
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the lexical class-name pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // load the lexical class-name byte length
    emitter.instruction("lea r9, [r13 - 2]");                                   // exclude both NUL separators from the mangled key length
    emitter.instruction("cmp r9, r11");                                         // owner length must leave room for the lexical class name
    emitter.instruction("jl __rt_object_to_hash_next_x");                       // malformed or shorter keys cannot match lexical private scope
    emitter.instruction("add r12, 1");                                          // advance from the leading NUL to private owner text
    emitter.instruction("xor r8d, r8d");                                        // initialize the private owner byte comparator
    emitter.label("__rt_object_to_hash_private_compare_x");
    emitter.instruction("cmp r8, r11");                                         // have all lexical class-name bytes matched?
    emitter.instruction("jge __rt_object_to_hash_private_separator_x");         // validate the owner terminator after a full match
    emitter.instruction("mov r9b, BYTE PTR [r12 + r8]");                        // load the next private declaring-class byte
    emitter.instruction("cmp r9b, BYTE PTR [r10 + r8]");                        // compare it with the lexical class-name byte
    emitter.instruction("jne __rt_object_to_hash_next_x");                      // mismatched owners remain invisible in this lexical scope
    emitter.instruction("add r8, 1");                                           // advance the private owner comparator
    emitter.instruction("jmp __rt_object_to_hash_private_compare_x");           // continue comparing owner bytes
    emitter.label("__rt_object_to_hash_private_separator_x");
    emitter.instruction("cmp BYTE PTR [r12 + r11], 0");                         // validate the NUL separator after the private owner
    emitter.instruction("jne __rt_object_to_hash_next_x");                      // longer or different owner names remain invisible
    emitter.instruction("add r12, r11");                                        // advance from owner start to its terminating NUL
    emitter.instruction("add r12, 1");                                          // advance to the plain private property name
    emitter.instruction("sub r13, r11");                                        // remove declaring-class bytes from key length
    emitter.instruction("sub r13, 2");                                          // remove both NUL separators from key length
    emitter.label("__rt_object_to_hash_row_ready_x");
    emitter.instruction("mov QWORD PTR [rbp - 64], r12");                       // save the property-key pointer slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 72], r13");                       // save the property-key length slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 80], r14");                       // save the property-storage offset slot across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 88], r15");                       // save the property runtime-tag slot across nested runtime calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the object pointer slot for `mov r10, QWORD PTR [rbp - 8]`
    emitter.instruction("add r10, r14");                                        // derive the current descriptor/property offset with `add r10, r14`
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load the current object/descriptor operand for `mov rdi, QWORD PTR [r10]`
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load the current object/descriptor operand for `mov rsi, QWORD PTR [r10 + 8]`
    abi::emit_load_int_immediate(emitter, "r11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp rsi, r11");                                        // evaluate `cmp rsi, r11` before selecting the projection branch
    emitter.instruction("je __rt_object_to_hash_next_x");                       // skip or advance the current property via `__rt_object_to_hash_next_x`
    emitter.instruction("cmp r15, 11");                                         // does the descriptor describe an inline nullable scalar?
    emitter.instruction("jne __rt_object_to_hash_value_ready_x");               // ordinary property tags already use canonical value words
    emitter.instruction("mov rax, rsi");                                        // dispatch tagged scalars using their stored int-or-null tag
    emitter.instruction("xor esi, esi");                                        // tagged scalar payloads have no third value word
    emitter.instruction("jmp __rt_object_to_hash_value_box_x");                 // skip ordinary descriptor-tag materialization
    emitter.label("__rt_object_to_hash_value_ready_x");
    emitter.instruction("mov rax, r15");                                        // use the ordinary property descriptor tag
    emitter.label("__rt_object_to_hash_value_box_x");
    emitter.instruction("call __rt_mixed_from_value");                          // call `__rt_mixed_from_value` with the prepared projection arguments
    emitter.instruction("mov rcx, rax");                                        // prepare the projection argument or result with `mov rcx, rax`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // load the result-hash slot for `mov rdi, QWORD PTR [rbp - 24]`
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // load the property-key pointer slot for `mov rsi, QWORD PTR [rbp - 64]`
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // load the property-key length slot for `mov rdx, QWORD PTR [rbp - 72]`
    emitter.instruction("xor r8d, r8d");                                        // zero `r8d` before the runtime call
    emitter.instruction("mov r9, 7");                                           // prepare the projection argument or result with `mov r9, 7`
    emitter.instruction("call __rt_hash_set");                                  // call `__rt_hash_set` with the prepared projection arguments
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result-hash slot across nested runtime calls
    emitter.label("__rt_object_to_hash_next_x");
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // derive the current descriptor/property offset with `add QWORD PTR [rbp - 40], 1`
    emitter.instruction("jmp __rt_object_to_hash_loop_x");                      // continue the descriptor scan at `__rt_object_to_hash_loop_x`

    emitter.label("__rt_object_to_hash_dynamic_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the object pointer slot for `mov r10, QWORD PTR [rbp - 8]`
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current object/descriptor operand for `mov r11, QWORD PTR [r10]`
    emitter.instruction("lea r12, [rip + _class_object_dynamic_prop_flags]");   // materialize the metadata symbol used by `lea r12, [rip + _class_object_dynamic_prop_flags]`
    emitter.instruction("cmp QWORD PTR [r12 + r11 * 8], 0");                    // evaluate `cmp QWORD PTR [r12 + r11 * 8], 0` before selecting the projection branch
    emitter.instruction("je __rt_object_to_hash_done_x");                       // finish the projection through `__rt_object_to_hash_done_x`
    emitter.instruction("lea r12, [rip + _class_object_payload_sizes]");        // materialize the metadata symbol used by `lea r12, [rip + _class_object_payload_sizes]`
    emitter.instruction("mov r12, QWORD PTR [r12 + r11 * 8]");                  // load the current object/descriptor operand for `mov r12, QWORD PTR [r12 + r11 * 8]`
    emitter.instruction("sub r12, 8");                                          // derive the current descriptor/property offset with `sub r12, 8`
    emitter.instruction("mov rsi, QWORD PTR [r10 + r12]");                      // load the current object/descriptor operand for `mov rsi, QWORD PTR [r10 + r12]`
    emitter.instruction("test rsi, rsi");                                       // evaluate `test rsi, rsi` before selecting the projection branch
    emitter.instruction("jz __rt_object_to_hash_done_x");                       // finish the projection through `__rt_object_to_hash_done_x`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // load the result-hash slot for `mov rdi, QWORD PTR [rbp - 24]`
    emitter.instruction("call __rt_hash_project_spread");                       // copy dynamic properties while normalizing PHP array keys
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result-hash slot across nested runtime calls
    emitter.instruction("jmp __rt_object_to_hash_done_x");                      // finish the projection through `__rt_object_to_hash_done_x`

    emitter.label("__rt_object_to_hash_incomplete_x");
    emitter.instruction("mov r10, QWORD PTR [rax + 24]");                       // load the current object/descriptor operand for `mov r10, QWORD PTR [rax + 24]`
    emitter.instruction("test r10, r10");                                       // evaluate `test r10, r10` before selecting the projection branch
    emitter.instruction("jz __rt_object_to_hash_empty_x");                      // use `__rt_object_to_hash_empty_x` when the object or metadata is absent
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current object/descriptor operand for `mov r11, QWORD PTR [r10]`
    emitter.instruction("lea rdi, [r11 * 2 + 16]");                             // derive the fresh hash capacity with `lea rdi, [r11 * 2 + 16]`
    emitter.instruction("mov rsi, 7");                                          // prepare the projection argument or result with `mov rsi, 7`
    emitter.instruction("call __rt_hash_new");                                  // call `__rt_hash_new` with the prepared projection arguments
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result-hash slot across nested runtime calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the object pointer slot for `mov r10, QWORD PTR [rbp - 8]`
    emitter.instruction("mov rax, 1");                                          // prepare the projection argument or result with `mov rax, 1`
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // load the current object/descriptor operand for `mov rdi, QWORD PTR [r10 + 8]`
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // load the current object/descriptor operand for `mov rsi, QWORD PTR [r10 + 16]`
    emitter.instruction("call __rt_mixed_from_value");                          // call `__rt_mixed_from_value` with the prepared projection arguments
    emitter.instruction("mov rcx, rax");                                        // prepare the projection argument or result with `mov rcx, rax`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // load the result-hash slot for `mov rdi, QWORD PTR [rbp - 24]`
    emitter.instruction("lea rsi, [rip + _incomplete_class_property_name]");    // materialize the metadata symbol used by `lea rsi, [rip + _incomplete_class_property_name]`
    emitter.instruction("mov rdx, 27");                                         // prepare the projection argument or result with `mov rdx, 27`
    emitter.instruction("xor r8d, r8d");                                        // zero `r8d` before the runtime call
    emitter.instruction("mov r9, 7");                                           // prepare the projection argument or result with `mov r9, 7`
    emitter.instruction("call __rt_hash_set");                                  // call `__rt_hash_set` with the prepared projection arguments
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result-hash slot across nested runtime calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the object pointer slot for `mov r10, QWORD PTR [rbp - 8]`
    emitter.instruction("mov rsi, QWORD PTR [r10 + 24]");                       // load the current object/descriptor operand for `mov rsi, QWORD PTR [r10 + 24]`
    emitter.instruction("mov rdi, rax");                                        // prepare the projection argument or result with `mov rdi, rax`
    emitter.instruction("call __rt_hash_project_spread");                       // copy retained incomplete-object properties with PHP array keys
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result-hash slot across nested runtime calls
    emitter.instruction("jmp __rt_object_to_hash_done_x");                      // finish the projection through `__rt_object_to_hash_done_x`

    emitter.label("__rt_object_to_hash_empty_x");
    emitter.instruction("mov rdi, 16");                                         // prepare the projection argument or result with `mov rdi, 16`
    emitter.instruction("mov rsi, 7");                                          // prepare the projection argument or result with `mov rsi, 7`
    emitter.instruction("call __rt_hash_new");                                  // call `__rt_hash_new` with the prepared projection arguments
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result-hash slot across nested runtime calls
    emitter.label("__rt_object_to_hash_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the result-hash slot for `mov rax, QWORD PTR [rbp - 24]`
    emitter.instruction("mov r12, QWORD PTR [rbp - 96]");                       // load the saved r12 slot for `mov r12, QWORD PTR [rbp - 96]`
    emitter.instruction("mov r13, QWORD PTR [rbp - 104]");                      // load the saved r13 slot for `mov r13, QWORD PTR [rbp - 104]`
    emitter.instruction("mov r14, QWORD PTR [rbp - 112]");                      // load the saved r14 slot for `mov r14, QWORD PTR [rbp - 112]`
    emitter.instruction("mov r15, QWORD PTR [rbp - 120]");                      // load the saved r15 slot for `mov r15, QWORD PTR [rbp - 120]`
    emitter.instruction("mov rsp, rbp");                                        // discard the x86_64 object-projection spill area
    emitter.instruction("pop rbp");                                             // restore the caller frame before returning
    emitter.instruction("ret");                                                 // return the fresh associative property table
}
