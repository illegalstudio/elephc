//! Purpose:
//! Emits the `__rt_obj_prop_count` / `__rt_obj_prop_name` / `__rt_obj_prop_value`
//! runtime helpers: indexed, PHP-callable access to an object's renderable
//! properties, which is what lets the injected `var_export` prelude walk an object
//! in ordinary PHP instead of in assembly.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::objects`.
//! - The `__elephc_object_prop_*` builtins lowered by
//!   `crate::codegen::lower_inst::builtins::object_props`.
//!
//! Key details:
//! - All three read `_class_prop_desc_ptrs[class_id]`, the SAME descriptor
//!   `__rt_print_r_object` walks and the same rows `_class_vd_desc_*` carries, so
//!   `var_export`, `print_r` and `var_dump` cannot disagree about an object's
//!   shape. Row layout is 48 bytes:
//!   `(print_r_key_ptr, print_r_key_len, byte_offset, value_tag, name_ptr, name_len)`.
//! - A null instance, an out-of-range index, and an uninitialized typed property
//!   are all "no property here": the count is 0, the name is the empty string, and
//!   the value is boxed PHP null. PHP omits uninitialized typed properties from
//!   `var_export` output, and no real property name is empty, so the prelude skips
//!   on an empty name without needing a fourth accessor.
//! - `__rt_obj_prop_value` ALWAYS returns a freshly boxed cell: a slot that already
//!   holds a `Mixed` cell is unboxed and re-boxed rather than handed back, because
//!   the caller owns and releases what it receives and must not be able to free
//!   storage that belongs to the object. `__rt_mixed_from_value` persists strings
//!   and increfs containers/objects, so the copy is independently owned.
//! - The two leaf helpers make no calls and touch caller-saved scratch only;
//!   `__rt_obj_prop_value` sets up a frame because it calls into the boxing helpers.

use crate::codegen_support::abi;
use crate::codegen_support::sentinels::{emit_branch_if_null_container, NULL_SENTINEL};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Byte width of one `_class_prop_desc_*` property row.
const PROP_DESC_ROW_BYTES: u64 = 48;

/// `__rt_obj_prop_count`: number of properties an object renders.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer (0 for a non-object).
/// Output: AArch64 x0 / x86_64 rax = row count, 0 when there is no descriptor.
pub fn emit_obj_prop_count(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: obj_prop_count ---");
    emitter.label_global("__rt_obj_prop_count");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x0, __rt_obj_prop_count_none");            // a non-object value has no properties
            emitter.instruction("ldr x9, [x0]");                                // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("ldr x10, [x10]");                              // load the number of registered class ids
            emitter.instruction("cmp x9, x10");                                 // is the class id within the descriptor table?
            emitter.instruction("b.hs __rt_obj_prop_count_none");               // an unknown class renders no properties
            abi::emit_symbol_address(emitter, "x11", "_class_prop_desc_ptrs");  // resolve the per-class descriptor pointer table
            emitter.instruction("ldr x11, [x11, x9, lsl #3]");                  // load this class's property descriptor
            emitter.instruction("ldr x0, [x11]");                               // the row count sits at descriptor offset 0
            emitter.instruction("ret");                                         // return to caller
            emitter.label("__rt_obj_prop_count_none");
            emitter.instruction("mov x0, #0");                                  // report zero renderable properties
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            emitter.instruction("test rdi, rdi");                               // a non-object value has no properties
            emitter.instruction("jz __rt_obj_prop_count_none_x86");             // report zero renderable properties
            emitter.instruction("mov r9, QWORD PTR [rdi]");                     // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // load the number of registered class ids
            emitter.instruction("cmp r9, r10");                                 // is the class id within the descriptor table?
            emitter.instruction("jae __rt_obj_prop_count_none_x86");            // an unknown class renders no properties
            abi::emit_symbol_address(emitter, "r11", "_class_prop_desc_ptrs");  // resolve the per-class descriptor pointer table
            emitter.instruction("mov r11, QWORD PTR [r11 + r9 * 8]");           // load this class's property descriptor
            emitter.instruction("mov rax, QWORD PTR [r11]");                    // the row count sits at descriptor offset 0
            emitter.instruction("ret");                                         // return to caller
            emitter.label("__rt_obj_prop_count_none_x86");
            emitter.instruction("xor rax, rax");                                // report zero renderable properties
            emitter.instruction("ret");                                         // return to caller
        }
    }
}

/// `__rt_obj_prop_name`: bare name of an object's Nth renderable property.
///
/// Input: AArch64 x0=object x1=index / x86_64 rdi=object rsi=index.
/// Output: the platform string result pair (AArch64 x1=ptr x2=len, x86_64
/// rax=ptr rdx=len). An absent, out-of-range or still-uninitialized property
/// yields a zero-length string, which is what the prelude skips on.
pub fn emit_obj_prop_name(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: obj_prop_name ---");
    emitter.label_global("__rt_obj_prop_name");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x0, __rt_obj_prop_name_none");             // a non-object value has no property names
            emitter.instruction("cmp x1, #0");                                  // reject a negative index before scaling it
            emitter.instruction("b.lt __rt_obj_prop_name_none");                // out-of-range indices yield the empty string
            emitter.instruction("ldr x9, [x0]");                                // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("ldr x10, [x10]");                              // load the number of registered class ids
            emitter.instruction("cmp x9, x10");                                 // is the class id within the descriptor table?
            emitter.instruction("b.hs __rt_obj_prop_name_none");                // an unknown class has no property names
            abi::emit_symbol_address(emitter, "x11", "_class_prop_desc_ptrs");  // resolve the per-class descriptor pointer table
            emitter.instruction("ldr x11, [x11, x9, lsl #3]");                  // load this class's property descriptor
            emitter.instruction("ldr x12, [x11]");                              // load the descriptor row count
            emitter.instruction("cmp x1, x12");                                 // is the requested index within the row count?
            emitter.instruction("b.hs __rt_obj_prop_name_none");                // past the last property → empty string
            emitter.instruction(&format!("mov x13, #{}", PROP_DESC_ROW_BYTES)); // each descriptor row occupies 48 bytes
            emitter.instruction("mul x13, x1, x13");                            // byte offset of this property's row
            emitter.instruction("add x13, x11, x13");                           // advance into the descriptor
            emitter.instruction("add x13, x13, #8");                            // skip the leading row-count word
            emitter.instruction("ldr x14, [x13, #16]");                         // load the property's byte offset within the object
            emitter.instruction("add x14, x0, x14");                            // resolve the absolute property slot address
            emitter.instruction("ldr x15, [x14, #8]");                          // load the slot's high word (the init marker)
            emit_uninit_sentinel_aarch64(emitter, "x16");                       // materialize the uninitialized-property marker
            emitter.instruction("cmp x15, x16");                                // is this property still uninitialized?
            emitter.instruction("b.eq __rt_obj_prop_name_none");                // PHP omits it from var_export output
            emitter.instruction("ldr x2, [x13, #40]");                          // load the bare property-name length
            emitter.instruction("ldr x1, [x13, #32]");                          // load the bare property-name pointer
            emitter.instruction("ret");                                         // return to caller
            emitter.label("__rt_obj_prop_name_none");
            abi::emit_symbol_address(emitter, "x1", "_class_name_missing");     // reuse the shared empty-name slot as the buffer
            emitter.instruction("mov x2, #0");                                  // a zero-length string means "no property here"
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            emitter.instruction("test rdi, rdi");                               // a non-object value has no property names
            emitter.instruction("jz __rt_obj_prop_name_none_x86");              // yield the empty string
            emitter.instruction("cmp rsi, 0");                                  // reject a negative index before scaling it
            emitter.instruction("jl __rt_obj_prop_name_none_x86");              // out-of-range indices yield the empty string
            emitter.instruction("mov r9, QWORD PTR [rdi]");                     // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // load the number of registered class ids
            emitter.instruction("cmp r9, r10");                                 // is the class id within the descriptor table?
            emitter.instruction("jae __rt_obj_prop_name_none_x86");             // an unknown class has no property names
            abi::emit_symbol_address(emitter, "r11", "_class_prop_desc_ptrs");  // resolve the per-class descriptor pointer table
            emitter.instruction("mov r11, QWORD PTR [r11 + r9 * 8]");           // load this class's property descriptor
            emitter.instruction("mov r10, QWORD PTR [r11]");                    // load the descriptor row count
            emitter.instruction("cmp rsi, r10");                                // is the requested index within the row count?
            emitter.instruction("jae __rt_obj_prop_name_none_x86");             // past the last property → empty string
            emitter.instruction("mov rax, rsi");                                // copy the index for row-offset scaling
            let scale = format!("imul rax, rax, {}", PROP_DESC_ROW_BYTES);
            emitter.instruction(&scale);                                        // each descriptor row occupies 48 bytes
            emitter.instruction("add rax, r11");                                // advance into the descriptor
            emitter.instruction("add rax, 8");                                  // skip the leading row-count word
            emitter.instruction("mov r10, QWORD PTR [rax + 16]");               // load the property's byte offset within the object
            emitter.instruction("add r10, rdi");                                // resolve the absolute property slot address
            emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                // load the slot's high word (the init marker)
            emitter.instruction("movabs r8, 0x7ffffffffffffffd");               // materialize the uninitialized-property marker
            emitter.instruction("cmp r10, r8");                                 // is this property still uninitialized?
            emitter.instruction("je __rt_obj_prop_name_none_x86");              // PHP omits it from var_export output
            emitter.instruction("mov rdx, QWORD PTR [rax + 40]");               // load the bare property-name length
            emitter.instruction("mov rax, QWORD PTR [rax + 32]");               // load the bare property-name pointer
            emitter.instruction("ret");                                         // return to caller
            emitter.label("__rt_obj_prop_name_none_x86");
            abi::emit_symbol_address(emitter, "rax", "_class_name_missing");    // reuse the shared empty-name slot as the buffer
            emitter.instruction("xor edx, edx");                                // a zero-length string means "no property here"
            emitter.instruction("ret");                                         // return to caller
        }
    }
}

/// Materializes the uninitialized-typed-property sentinel into an AArch64 register.
///
/// Must match `codegen_support::sentinels::UNINITIALIZED_TYPED_PROPERTY_SENTINEL`
/// (`0x7fff_ffff_ffff_fffd`) exactly, or a property that never got a value would be
/// exported as garbage instead of being skipped.
fn emit_uninit_sentinel_aarch64(emitter: &mut Emitter, reg: &str) {
    emitter.instruction(&format!("movz {}, #0xfffd", reg));                     // low halfword of the uninitialized-typed-property sentinel
    emitter.instruction(&format!("movk {}, #0xffff, lsl #16", reg));            // second halfword of the uninitialized sentinel
    emitter.instruction(&format!("movk {}, #0xffff, lsl #32", reg));            // third halfword of the uninitialized sentinel
    emitter.instruction(&format!("movk {}, #0x7fff, lsl #48", reg));            // top halfword of the uninitialized sentinel
}

/// `__rt_obj_prop_value`: value of an object's Nth renderable property, boxed.
///
/// Input: AArch64 x0=object x1=index / x86_64 rdi=object rsi=index.
/// Output: AArch64 x0 / x86_64 rax = a FRESHLY allocated Mixed cell the caller
/// owns. Anything that is not a readable property boxes canonical PHP null.
pub fn emit_obj_prop_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_obj_prop_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: obj_prop_value ---");
    emitter.label_global("__rt_obj_prop_value");

    // Frame (32 bytes): [16] saved x29, [24] saved x30.
    emitter.instruction("sub sp, sp, #32");                                     // allocate the property-read frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the property-read frame pointer

    emitter.instruction("cbz x0, __rt_obj_prop_value_null");                    // a non-object value has no properties
    emitter.instruction("cmp x1, #0");                                          // reject a negative index before scaling it
    emitter.instruction("b.lt __rt_obj_prop_value_null");                       // out-of-range indices box PHP null
    emitter.instruction("ldr x9, [x0]");                                        // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");           // resolve the class-id table extent
    emitter.instruction("ldr x10, [x10]");                                      // load the number of registered class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the descriptor table?
    emitter.instruction("b.hs __rt_obj_prop_value_null");                       // an unknown class has no readable properties
    abi::emit_symbol_address(emitter, "x11", "_class_prop_desc_ptrs");          // resolve the per-class descriptor pointer table
    emitter.instruction("ldr x11, [x11, x9, lsl #3]");                          // load this class's property descriptor
    emitter.instruction("ldr x12, [x11]");                                      // load the descriptor row count
    emitter.instruction("cmp x1, x12");                                         // is the requested index within the row count?
    emitter.instruction("b.hs __rt_obj_prop_value_null");                       // past the last property → PHP null
    emitter.instruction(&format!("mov x13, #{}", PROP_DESC_ROW_BYTES));         // each descriptor row occupies 48 bytes
    emitter.instruction("mul x13, x1, x13");                                    // byte offset of this property's row
    emitter.instruction("add x13, x11, x13");                                   // advance into the descriptor
    emitter.instruction("add x13, x13, #8");                                    // skip the leading row-count word
    emitter.instruction("ldr x14, [x13, #16]");                                 // load the property's byte offset within the object
    emitter.instruction("add x14, x0, x14");                                    // resolve the absolute property slot address
    emitter.instruction("ldr x2, [x14, #8]");                                   // load the slot high word (payload high / init marker)
    emit_uninit_sentinel_aarch64(emitter, "x16");                               // materialize the uninitialized-property marker
    emitter.instruction("cmp x2, x16");                                         // is this property still uninitialized?
    emitter.instruction("b.eq __rt_obj_prop_value_null");                       // PHP omits it, so report PHP null
    emitter.instruction("ldr x1, [x14]");                                       // load the slot low word (the payload)
    emitter.instruction("ldr x0, [x13, #24]");                                  // load the property's runtime value tag

    emitter.instruction("cmp x0, #11");                                         // does the descriptor describe an inline nullable scalar?
    emitter.instruction("csel x0, x2, x0, eq");                                 // box tagged scalars using their stored int-or-null tag
    emitter.instruction("csel x2, xzr, x2, eq");                                // tagged scalar payloads have no third value word

    emitter.instruction("cmp x0, #4");                                          // only pointer-shaped tags can carry a null payload
    emitter.instruction("b.lt __rt_obj_prop_value_box");                        // scalar payloads box exactly as stored
    emit_branch_if_null_container(emitter, "x1", "x9", "__rt_obj_prop_value_null"); // a zero/sentinel pointer is PHP null
    emitter.instruction("cmp x0, #7");                                          // does the slot already hold a boxed Mixed cell?
    emitter.instruction("b.ne __rt_obj_prop_value_box");                        // a direct payload boxes as-is
    emitter.instruction("mov x0, x1");                                          // pass the existing cell to the unboxer
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=inner tag, x1=lo, x2=hi — re-box for a fresh copy

    emitter.label("__rt_obj_prop_value_box");
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = freshly owned Mixed cell
    emitter.instruction("b __rt_obj_prop_value_done");                          // return the boxed property value

    emitter.label("__rt_obj_prop_value_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = canonical PHP null
    abi::emit_load_int_immediate(emitter, "x1", NULL_SENTINEL);                 // null payload uses the shared in-band sentinel
    emitter.instruction("mov x2, #0");                                          // null carries no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box PHP null for the caller

    emitter.label("__rt_obj_prop_value_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the property-read frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 boxed property reader.
fn emit_obj_prop_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: obj_prop_value ---");
    emitter.label_global("__rt_obj_prop_value");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the property-read frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the property-read frame

    emitter.instruction("test rdi, rdi");                                       // a non-object value has no properties
    emitter.instruction("jz __rt_obj_prop_value_null_x86");                     // box PHP null
    emitter.instruction("cmp rsi, 0");                                          // reject a negative index before scaling it
    emitter.instruction("jl __rt_obj_prop_value_null_x86");                     // out-of-range indices box PHP null
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");           // resolve the class-id table extent
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the number of registered class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id within the descriptor table?
    emitter.instruction("jae __rt_obj_prop_value_null_x86");                    // an unknown class has no readable properties
    abi::emit_symbol_address(emitter, "r11", "_class_prop_desc_ptrs");          // resolve the per-class descriptor pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r9 * 8]");                   // load this class's property descriptor
    emitter.instruction("mov r10, QWORD PTR [r11]");                            // load the descriptor row count
    emitter.instruction("cmp rsi, r10");                                        // is the requested index within the row count?
    emitter.instruction("jae __rt_obj_prop_value_null_x86");                    // past the last property → PHP null
    emitter.instruction("mov rax, rsi");                                        // copy the index for row-offset scaling
    emitter.instruction(&format!("imul rax, rax, {}", PROP_DESC_ROW_BYTES));    // each descriptor row occupies 48 bytes
    emitter.instruction("add rax, r11");                                        // advance into the descriptor
    emitter.instruction("add rax, 8");                                          // skip the leading row-count word
    emitter.instruction("mov r10, QWORD PTR [rax + 16]");                       // load the property's byte offset within the object
    emitter.instruction("add r10, rdi");                                        // resolve the absolute property slot address
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load the slot high word (payload high / init marker)
    emitter.instruction("movabs r8, 0x7ffffffffffffffd");                       // materialize the uninitialized-property marker
    emitter.instruction("cmp rsi, r8");                                         // is this property still uninitialized?
    emitter.instruction("je __rt_obj_prop_value_null_x86");                     // PHP omits it, so report PHP null
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load the slot low word (the payload)
    emitter.instruction("mov rax, QWORD PTR [rax + 24]");                       // load the property's runtime value tag

    emitter.instruction("cmp rax, 11");                                         // does the descriptor describe an inline nullable scalar?
    emitter.instruction("jne __rt_obj_prop_value_tag_ready_x86");               // ordinary property tags already use canonical value words
    emitter.instruction("mov rax, rsi");                                        // dispatch tagged scalars using their stored int-or-null tag
    emitter.instruction("xor esi, esi");                                        // tagged scalar payloads have no third value word
    emitter.label("__rt_obj_prop_value_tag_ready_x86");

    emitter.instruction("cmp rax, 4");                                          // only pointer-shaped tags can carry a null payload
    emitter.instruction("jl __rt_obj_prop_value_box_x86");                      // scalar payloads box exactly as stored
    emit_branch_if_null_container(emitter, "rdi", "r9", "__rt_obj_prop_value_null_x86"); // a zero/sentinel pointer is PHP null
    emitter.instruction("cmp rax, 7");                                          // does the slot already hold a boxed Mixed cell?
    emitter.instruction("jne __rt_obj_prop_value_box_x86");                     // a direct payload boxes as-is
    emitter.instruction("mov rax, rdi");                                        // pass the existing cell to the unboxer
    emitter.instruction("call __rt_mixed_unbox");                               // rax=inner tag, rdi=lo, rdx=hi
    emitter.instruction("mov rsi, rdx");                                        // unbox reports the high word in rdx; boxing reads it from rsi

    emitter.label("__rt_obj_prop_value_box_x86");
    emitter.instruction("call __rt_mixed_from_value");                          // rax = freshly owned Mixed cell
    emitter.instruction("jmp __rt_obj_prop_value_done_x86");                    // return the boxed property value

    emitter.label("__rt_obj_prop_value_null_x86");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = canonical PHP null
    abi::emit_load_int_immediate(emitter, "rdi", NULL_SENTINEL);                // null payload uses the shared in-band sentinel
    emitter.instruction("xor esi, esi");                                        // null carries no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box PHP null for the caller

    emitter.label("__rt_obj_prop_value_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the property-read frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
