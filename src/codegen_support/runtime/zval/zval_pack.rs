//! Purpose:
//! Emits the `__rt_zval_pack` and `__rt_zval_pack_element` runtime helpers that
//! convert elephc runtime values into freshly allocated 16-byte PHP `zval`
//! structures.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and recursively from `__rt_zval_pack_array_*` for nested elements.
//!
//! Key details:
//! - `__rt_zval_pack` takes a boxed `Mixed` cell pointer and tail-calls
//!   `__rt_zval_pack_element` with the cell's `(tag, lo, hi)` triple.
//! - `__rt_zval_pack_element` is the shared dispatch core: given a runtime value
//!   tag plus the low/high payload words it produces a `zval`. The array packers
//!   reuse it per element so scalar/string/array/nested handling stays in one place.
//! - Output `zval`: `value` at `+0`, `u1.type_info` at `+8`, `u2` at `+12`.
//! - Refcounted children (strings, arrays) are freshly allocated so the returned
//!   `zval` owns independent PHP-shaped storage.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// zval_pack: convert a boxed elephc Mixed cell into a PHP zval.
/// Input:  x0 / rax = boxed Mixed cell pointer
/// Output: x0 / rax = zval pointer (16-byte heap block)
pub fn emit_zval_pack(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_pack_linux_x86_64(emitter);
        emit_zval_pack_element_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_pack ---");
    emitter.label_global("__rt_zval_pack");
    emitter.instruction("ldr x2, [x0, #16]");                                   // load the high payload word from the Mixed cell
    emitter.instruction("ldr x1, [x0, #8]");                                    // load the low payload word from the Mixed cell
    emitter.instruction("ldr x0, [x0]");                                        // load the runtime value tag from the Mixed cell
    emitter.instruction("b __rt_zval_pack_element");                            // tail-call the shared dispatch core

    emit_zval_pack_element_aarch64(emitter);
}

/// zval_pack_element: convert a runtime (tag, lo, hi) triple into a PHP zval.
/// Input:  x0 = runtime value tag, x1 = low payload word, x2 = high payload word
/// Output: x0 = zval pointer (16-byte heap block)
fn emit_zval_pack_element_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_pack_element ---");
    emitter.label_global("__rt_zval_pack_element");

    emitter.instruction("cmp x0, #4");                                          // only container-shaped tags can carry the null sentinel
    emitter.instruction("b.lt __rt_zval_pack_element_input_ready");             // scalar payloads preserve their original bits
    emitter.instruction("cmp x0, #6");                                          // indexed arrays, hashes, and objects occupy tags 4 through 6
    emitter.instruction("b.gt __rt_zval_pack_element_input_ready");             // nested Mixed and other tags use ordinary dispatch
    emit_branch_if_null_container(
        emitter,
        "x1",
        "x9",
        "__rt_zval_pack_element_null_input",
    );
    emitter.instruction("b __rt_zval_pack_element_input_ready");                // pack the valid container payload
    emitter.label("__rt_zval_pack_element_null_input");
    emitter.instruction("mov x0, #8");                                          // convert a legacy container-shaped null to IS_NULL
    emitter.instruction("mov x1, #0");                                          // canonical null has no low payload word
    emitter.instruction("mov x2, #0");                                          // canonical null has no high payload word
    emitter.label("__rt_zval_pack_element_input_ready");

    // -- set up stack frame and spill the incoming payload triple --
    emitter.instruction("sub sp, sp, #64");                                     // reserve tag/lo/hi/value/type_info slots plus frame records
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #8]");                                    // save the tag
    emitter.instruction("str x1, [sp, #16]");                                   // save the low payload word
    emitter.instruction("str x2, [sp, #24]");                                   // save the high payload word

    // -- dispatch on the runtime value tag --
    emitter.instruction("cmp x0, #0");                                          // tag 0 = integer
    emitter.instruction("b.eq __rt_zval_pack_int");                             // pack the integer payload as a PHP long
    emitter.instruction("cmp x0, #1");                                          // tag 1 = string
    emitter.instruction("b.eq __rt_zval_pack_str");                             // pack the string payload as a zend_string
    emitter.instruction("cmp x0, #2");                                          // tag 2 = float
    emitter.instruction("b.eq __rt_zval_pack_float");                           // pack the float payload as a PHP double
    emitter.instruction("cmp x0, #3");                                          // tag 3 = bool
    emitter.instruction("b.eq __rt_zval_pack_bool");                            // pack the bool payload as IS_TRUE/IS_FALSE
    emitter.instruction("cmp x0, #8");                                          // tag 8 = null
    emitter.instruction("b.eq __rt_zval_pack_null");                            // pack as a PHP null
    emitter.instruction("cmp x0, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __rt_zval_pack_idx_arr");                         // pack the indexed array into a packed zend_array
    emitter.instruction("cmp x0, #5");                                          // tag 5 = associative array
    emitter.instruction("b.eq __rt_zval_pack_hash_arr");                        // pack the associative array into a hash zend_array
    emitter.instruction("cmp x0, #7");                                          // tag 7 = nested Mixed cell
    emitter.instruction("b.eq __rt_zval_pack_nested");                          // unwrap and pack the nested Mixed cell
    emitter.instruction("cmp x0, #6");                                          // tag 6 = object
    emitter.instruction("b.eq __rt_zval_pack_object");                          // pack the object pointer as IS_OBJECT_EX
    emitter.instruction("cmp x0, #9");                                          // tag 9 = resource
    emitter.instruction("b.eq __rt_zval_pack_resource");                        // pack the resource pointer as IS_RESOURCE_EX
    emitter.instruction("b __rt_zval_pack_null");                               // unknown kinds (callable, etc.) pack as null

    // -- integer: value = lo, type_info = IS_LONG (4) --
    emitter.label("__rt_zval_pack_int");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the integer payload
    emitter.instruction("str x10, [sp, #32]");                                  // stage the zval value
    emitter.instruction("mov x9, #4");                                          // IS_LONG = 4
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- float: value = lo (f64 bits), type_info = IS_DOUBLE (5) --
    emitter.label("__rt_zval_pack_float");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the float bit payload
    emitter.instruction("str x10, [sp, #32]");                                  // stage the zval value
    emitter.instruction("mov x9, #5");                                          // IS_DOUBLE = 5
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- bool: value = 0, type_info = IS_TRUE (3) or IS_FALSE (2) --
    emitter.label("__rt_zval_pack_bool");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the bool payload (0 or 1)
    emitter.instruction("cmp x10, #0");                                         // test the bool truthiness
    emitter.instruction("cset x9, ne");                                         // 1 when true, 0 when false
    emitter.instruction("add x9, x9, #2");                                      // IS_FALSE = 2, IS_TRUE = 3
    emitter.instruction("str xzr, [sp, #32]");                                  // bool zval value is unused
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- null: value = 0, type_info = IS_NULL (1) --
    emitter.label("__rt_zval_pack_null");
    emitter.instruction("str xzr, [sp, #32]");                                  // null zval value is zero
    emitter.instruction("mov x9, #1");                                          // IS_NULL = 1
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- object: value = elephc object pointer, type_info = IS_OBJECT_EX (0x108) --
    emitter.label("__rt_zval_pack_object");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the object pointer payload
    emitter.instruction("str x10, [sp, #32]");                                  // stage the zval value
    emitter.instruction("mov x9, #0x108");                                      // IS_OBJECT_EX = type 8 | IS_TYPE_REFCOUNTED
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- resource: value = elephc resource pointer, type_info = IS_RESOURCE_EX (0x109) --
    emitter.label("__rt_zval_pack_resource");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the resource pointer payload
    emitter.instruction("str x10, [sp, #32]");                                  // stage the zval value
    emitter.instruction("mov x9, #0x109");                                      // IS_RESOURCE_EX = type 9 | IS_TYPE_REFCOUNTED
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- string: build a zend_string copy, type_info = IS_STRING_EX (0x106) --
    emitter.label("__rt_zval_pack_str");
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the source string pointer to zend_string_new
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the source string length to zend_string_new
    emitter.instruction("bl __rt_zval_string_new");                             // x0 = freshly allocated zend_string pointer
    emitter.instruction("str x0, [sp, #32]");                                   // stage the zval value (zend_string pointer)
    emitter.instruction("mov x9, #0x106");                                      // IS_STRING_EX = type 6 | IS_TYPE_REFCOUNTED
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- indexed array: build a packed zend_array, type_info = IS_ARRAY_EX (0x307) --
    emitter.label("__rt_zval_pack_idx_arr");
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the indexed array pointer to the packer
    emitter.instruction("bl __rt_zval_pack_array_packed");                      // x0 = freshly allocated zend_array pointer
    emitter.instruction("str x0, [sp, #32]");                                   // stage the zval value (zend_array pointer)
    emitter.instruction("mov x9, #0x307");                                      // IS_ARRAY_EX = type 7 | IS_TYPE_REFCOUNTED
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- associative array: build a hash zend_array, type_info = IS_ARRAY_EX (0x307) --
    emitter.label("__rt_zval_pack_hash_arr");
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the hash array pointer to the packer
    emitter.instruction("bl __rt_zval_pack_array_hash");                        // x0 = freshly allocated zend_array pointer
    emitter.instruction("str x0, [sp, #32]");                                   // stage the zval value (zend_array pointer)
    emitter.instruction("mov x9, #0x307");                                      // IS_ARRAY_EX = type 7 | IS_TYPE_REFCOUNTED
    emitter.instruction("str x9, [sp, #40]");                                   // stage the type_info
    emitter.instruction("b __rt_zval_pack_build");                              // build the zval from the staged value and type_info

    // -- nested Mixed cell: flatten by recursing and returning the inner zval directly --
    emitter.label("__rt_zval_pack_nested");
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the child Mixed cell pointer (lo payload)
    emitter.instruction("bl __rt_zval_pack");                                   // x0 = inner zval representing the unwrapped value
    emitter.instruction("b __rt_zval_pack_done");                               // skip allocation, return the inner zval

    // -- build the 16-byte zval from the staged value and type_info --
    emitter.label("__rt_zval_pack_build");
    emitter.instruction("mov x0, #16");                                         // zval structures are exactly 16 bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = zval storage pointer
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the staged zval value
    emitter.instruction("str x10, [x0]");                                       // store the zval value at offset 0
    emitter.instruction("ldr w9, [sp, #40]");                                   // reload the staged type_info (32-bit)
    emitter.instruction("str w9, [x0, #8]");                                    // store the type_info at offset 8
    emitter.instruction("str wzr, [x0, #12]");                                  // zero the u2 slot at offset 12

    emitter.label("__rt_zval_pack_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the stack frame
    emitter.instruction("ret");                                                 // return the zval pointer in x0
}

/// x86_64 Linux implementation of `__rt_zval_pack` (thin wrapper) and
/// `__rt_zval_pack_element` (dispatch core). Same semantics as the ARM64 pair.
fn emit_zval_pack_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_pack ---");
    emitter.label_global("__rt_zval_pack");
    emitter.instruction("mov rcx, QWORD PTR [rax + 16]");                       // load the high payload word from the Mixed cell
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // load the low payload word from the Mixed cell
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // load the runtime value tag from the Mixed cell
    emitter.instruction("mov rsi, rcx");                                        // place the high payload word in the element hi register
    emitter.instruction("mov rdi, rdx");                                        // place the low payload word in the element lo register
    emitter.instruction("jmp __rt_zval_pack_element");                          // tail-call the shared dispatch core
}

/// x86_64 Linux implementation of `__rt_zval_pack_element`.
/// Input:  rax = runtime value tag, rdi = low payload word, rsi = high payload word
/// Output: rax = zval pointer (16-byte heap block)
fn emit_zval_pack_element_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_pack_element ---");
    emitter.label_global("__rt_zval_pack_element");

    emitter.instruction("cmp rax, 4");                                          // only container-shaped tags can carry the null sentinel
    emitter.instruction("jl __rt_zval_pack_element_input_ready");               // scalar payloads preserve their original bits
    emitter.instruction("cmp rax, 6");                                          // indexed arrays, hashes, and objects occupy tags 4 through 6
    emitter.instruction("jg __rt_zval_pack_element_input_ready");               // nested Mixed and other tags use ordinary dispatch
    emit_branch_if_null_container(
        emitter,
        "rdi",
        "r10",
        "__rt_zval_pack_element_null_input",
    );
    emitter.instruction("jmp __rt_zval_pack_element_input_ready");              // pack the valid container payload
    emitter.label("__rt_zval_pack_element_null_input");
    emitter.instruction("mov rax, 8");                                          // convert a legacy container-shaped null to IS_NULL
    emitter.instruction("xor edi, edi");                                        // canonical null has no low payload word
    emitter.instruction("xor esi, esi");                                        // canonical null has no high payload word
    emitter.label("__rt_zval_pack_element_input_ready");

    // -- set up stack frame and spill the incoming payload triple --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 56");                                         // reserve tag/lo/hi/value/type_info slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the high payload word

    // -- dispatch on the runtime value tag --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the tag
    emitter.instruction("cmp rax, 0");                                          // tag 0 = integer
    emitter.instruction("je __rt_zval_pack_int");                               // pack the integer payload as a PHP long
    emitter.instruction("cmp rax, 1");                                          // tag 1 = string
    emitter.instruction("je __rt_zval_pack_str");                               // pack the string payload as a zend_string
    emitter.instruction("cmp rax, 2");                                          // tag 2 = float
    emitter.instruction("je __rt_zval_pack_float");                             // pack the float payload as a PHP double
    emitter.instruction("cmp rax, 3");                                          // tag 3 = bool
    emitter.instruction("je __rt_zval_pack_bool");                              // pack the bool payload as IS_TRUE/IS_FALSE
    emitter.instruction("cmp rax, 8");                                          // tag 8 = null
    emitter.instruction("je __rt_zval_pack_null");                              // pack as a PHP null
    emitter.instruction("cmp rax, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __rt_zval_pack_idx_arr");                           // pack the indexed array into a packed zend_array
    emitter.instruction("cmp rax, 5");                                          // tag 5 = associative array
    emitter.instruction("je __rt_zval_pack_hash_arr");                          // pack the associative array into a hash zend_array
    emitter.instruction("cmp rax, 7");                                          // tag 7 = nested Mixed cell
    emitter.instruction("je __rt_zval_pack_nested");                            // unwrap and pack the nested Mixed cell
    emitter.instruction("cmp rax, 6");                                          // tag 6 = object
    emitter.instruction("je __rt_zval_pack_object");                            // pack the object pointer as IS_OBJECT_EX
    emitter.instruction("cmp rax, 9");                                          // tag 9 = resource
    emitter.instruction("je __rt_zval_pack_resource");                          // pack the resource pointer as IS_RESOURCE_EX
    emitter.instruction("jmp __rt_zval_pack_null");                             // unknown kinds pack as null

    // -- integer: value = lo, type_info = IS_LONG (4) --
    emitter.label("__rt_zval_pack_int");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the integer payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stage the zval value
    emitter.instruction("mov DWORD PTR [rbp - 48], 4");                         // IS_LONG = 4
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- float: value = lo (f64 bits), type_info = IS_DOUBLE (5) --
    emitter.label("__rt_zval_pack_float");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the float bit payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stage the zval value
    emitter.instruction("mov DWORD PTR [rbp - 48], 5");                         // IS_DOUBLE = 5
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- bool: value = 0, type_info = IS_TRUE (3) or IS_FALSE (2) --
    emitter.label("__rt_zval_pack_bool");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the bool payload
    emitter.instruction("test rax, rax");                                       // test the bool truthiness
    emitter.instruction("setne al");                                            // 1 when true, 0 when false
    emitter.instruction("movzx rax, al");                                       // widen the truth flag
    emitter.instruction("add rax, 2");                                          // IS_FALSE = 2, IS_TRUE = 3
    emitter.instruction("mov DWORD PTR [rbp - 48], eax");                       // stage the type_info
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // bool zval value is unused
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- null: value = 0, type_info = IS_NULL (1) --
    emitter.label("__rt_zval_pack_null");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // null zval value is zero
    emitter.instruction("mov DWORD PTR [rbp - 48], 1");                         // IS_NULL = 1
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- object: value = elephc object pointer, type_info = IS_OBJECT_EX (0x108) --
    emitter.label("__rt_zval_pack_object");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the object pointer payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stage the zval value
    emitter.instruction("mov DWORD PTR [rbp - 48], 264");                       // IS_OBJECT_EX = 0x108
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- resource: value = elephc resource pointer, type_info = IS_RESOURCE_EX (0x109) --
    emitter.label("__rt_zval_pack_resource");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the resource pointer payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stage the zval value
    emitter.instruction("mov DWORD PTR [rbp - 48], 265");                       // IS_RESOURCE_EX = 0x109
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- string: build a zend_string copy, type_info = IS_STRING_EX (0x106) --
    emitter.label("__rt_zval_pack_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // pass the source string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // pass the source string length
    emitter.instruction("call __rt_zval_string_new");                           // rax = freshly allocated zend_string pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stage the zval value (zend_string pointer)
    emitter.instruction("mov DWORD PTR [rbp - 48], 262");                       // IS_STRING_EX = 0x106
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- indexed array: build a packed zend_array, type_info = IS_ARRAY_EX (0x307) --
    emitter.label("__rt_zval_pack_idx_arr");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // pass the indexed array pointer
    emitter.instruction("call __rt_zval_pack_array_packed");                    // rax = freshly allocated zend_array pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stage the zval value (zend_array pointer)
    emitter.instruction("mov DWORD PTR [rbp - 48], 775");                       // IS_ARRAY_EX = 0x307
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- associative array: build a hash zend_array, type_info = IS_ARRAY_EX (0x307) --
    emitter.label("__rt_zval_pack_hash_arr");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // pass the hash array pointer
    emitter.instruction("call __rt_zval_pack_array_hash");                      // rax = freshly allocated zend_array pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stage the zval value (zend_array pointer)
    emitter.instruction("mov DWORD PTR [rbp - 48], 775");                       // IS_ARRAY_EX = 0x307
    emitter.instruction("jmp __rt_zval_pack_build");                            // build the zval from the staged value and type_info

    // -- nested Mixed cell: flatten by recursing and returning the inner zval directly --
    emitter.label("__rt_zval_pack_nested");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the child Mixed cell pointer (lo payload)
    emitter.instruction("call __rt_zval_pack");                                 // rax = inner zval representing the unwrapped value
    emitter.instruction("jmp __rt_zval_pack_done");                             // skip allocation, return the inner zval

    // -- build the 16-byte zval from the staged value and type_info --
    emitter.label("__rt_zval_pack_build");
    emitter.instruction("mov rax, 16");                                         // zval structures are exactly 16 bytes
    emitter.instruction("call __rt_heap_alloc");                                // rax = zval storage pointer
    emitter.instruction("mov rcx, rax");                                        // preserve the zval pointer across stores
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the staged zval value
    emitter.instruction("mov QWORD PTR [rcx], rax");                            // store the zval value at offset 0
    emitter.instruction("mov eax, DWORD PTR [rbp - 48]");                       // reload the staged type_info (32-bit)
    emitter.instruction("mov DWORD PTR [rcx + 8], eax");                        // store the type_info at offset 8
    emitter.instruction("mov DWORD PTR [rcx + 12], 0");                         // zero the u2 slot at offset 12
    emitter.instruction("mov rax, rcx");                                        // return the zval pointer in rax

    emitter.label("__rt_zval_pack_done");
    emitter.instruction("add rsp, 56");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the zval pointer in rax
}
