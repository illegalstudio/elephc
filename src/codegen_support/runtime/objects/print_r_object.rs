//! Purpose:
//! Emits `__rt_print_r_object` (and its `__rt_pr_obj_desc` descriptor lookup): the
//! runtime walker that renders PHP `print_r` output for an OBJECT —
//! `C Object\n<base>(\n<base+4>[prop] => value\n<base>)\n` — including the
//! ` Enum` / ` Enum:int` / ` Enum:string` header PHP gives an enum case and the
//! ` *RECURSION*` marker a revisited instance renders instead of a body.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::objects`.
//! - `__rt_pr_val_obj` (tag 6) in `runtime::io::print_r_walk`, for an object
//!   nested inside an array, a hash, another object, or a boxed Mixed cell.
//! - `crate::codegen::lower_inst::builtins::debug::lower_print_r` for a
//!   statically typed top-level object, with base indent 0.
//!
//! Key details:
//! - INDENT CONTRACT. `print_r` passes indents as call arguments (unlike
//!   var_dump's `_vd_indent` global). `base` is the column of the `(` and `)`
//!   lines, entries sit at `base + 4`, and a container VALUE inside an entry opens
//!   at `base + 8` — exactly php-src's `print_hash(indent)` / `indent + 4` /
//!   `php_print_zval_r_to_buf(indent + 8)`. The per-entry `\n` is written here,
//!   which is what produces PHP's blank line after a nested `)`.
//! - Property enumeration is driven by `_class_prop_desc_ptrs[class_id]` emitted
//!   by `crate::codegen_support::runtime::data::user`: a property count at offset
//!   0, then one 48-byte row per rendered property —
//!   `(key_ptr, key_len, byte_offset, value_tag, plain_key_ptr, plain_key_len)`.
//!   `key` already carries print_r's visibility annotation (`x`, `y:protected`,
//!   `z:C:private`), so no visibility reasoning happens at runtime. The rows are
//!   the SAME rows `_class_vd_desc_*` uses, so print_r and var_dump can never
//!   disagree about an object's shape.
//! - Uninitialized typed properties are SKIPPED entirely (PHP omits them from
//!   `print_r`, unlike var_dump which prints `uninitialized(T)`); the marker is
//!   the `UNINITIALIZED_TYPED_PROPERTY_SENTINEL` in the slot's high word.
//! - RECURSION GUARD: this reuses var_dump's `_vd_seen` pointer stack through
//!   `__rt_vd_seen_find` / `__rt_vd_seen_push` / `__rt_vd_seen_pop`. The two
//!   renderers can never be walking at the same time (no PHP callback runs inside
//!   either), and sharing the stack keeps one bound and one `*RECURSION*` policy.
//!   PHP marks the object only around its BODY, so two sibling references to one
//!   instance both render in full.
//! - DYNAMIC PROPERTIES are not in the descriptor: after the declared rows the walker
//!   appends the instance's dynamic-property hash (`__rt_obj_dump_dyn_props`) through
//!   `__rt_print_r_hash_entries`, in insertion order, at the same entry indent.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Byte width of one `_class_prop_desc_*` property row.
const PROP_DESC_ROW_BYTES: u64 = 48;

/// `__rt_pr_obj_desc`: resolve an object's print_r/var_export property descriptor.
///
/// Bounds-checks the header class id against `_class_gc_desc_count` so a stale or
/// synthetic id lands on the empty `_class_prop_desc_missing` descriptor instead
/// of reading past the table.
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
/// Output: AArch64 x0 / x86_64 rax = descriptor pointer.
pub fn emit_pr_obj_desc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_obj_desc ---");
    emitter.label_global("__rt_pr_obj_desc");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("ldr x10, [x10]");                              // load the number of registered class ids
            emitter.instruction("cmp x9, x10");                                 // is the class id within the descriptor table?
            emitter.instruction("b.hs __rt_pr_obj_desc_missing");               // out-of-range ids fall back to the empty descriptor
            abi::emit_symbol_address(emitter, "x11", "_class_prop_desc_ptrs");  // resolve the per-class descriptor pointer table
            emitter.instruction("ldr x0, [x11, x9, lsl #3]");                   // load this class's print_r descriptor
            emitter.instruction("ret");                                         // return to caller
            emitter.label("__rt_pr_obj_desc_missing");
            abi::emit_symbol_address(emitter, "x0", "_class_prop_desc_missing"); // fall back to the zero-property descriptor
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rdi]");                     // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // load the number of registered class ids
            emitter.instruction("cmp r9, r10");                                 // is the class id within the descriptor table?
            emitter.instruction("jae __rt_pr_obj_desc_missing_x86");            // out-of-range ids fall back to the empty descriptor
            abi::emit_symbol_address(emitter, "r11", "_class_prop_desc_ptrs");  // resolve the per-class descriptor pointer table
            emitter.instruction("mov rax, QWORD PTR [r11 + r9 * 8]");           // load this class's print_r descriptor
            emitter.instruction("ret");                                         // return to caller
            emitter.label("__rt_pr_obj_desc_missing_x86");
            abi::emit_symbol_address(emitter, "rax", "_class_prop_desc_missing");// fall back to the zero-property descriptor
            emitter.instruction("ret");                                         // return to caller
        }
    }
}

/// `__rt_print_r_object`: render one object exactly as PHP's `print_r` does.
///
/// Writes `C Object\n` (or the enum header), then the `<base>(\n` … `<base>)\n`
/// body with one `<base+4>[key] => value\n` line per initialized declared
/// property followed by one per dynamic property, or ` *RECURSION*` when the
/// instance is already being walked.
/// Input: AArch64 x0=object x1=base indent / x86_64 rdi=object rsi=base indent.
pub fn emit_print_r_object(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_object_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: print_r_object ---");
    emitter.label_global("__rt_print_r_object");

    // Frame (96 bytes): [0] object ptr, [8] base indent, [16] entry indent,
    //   [24] descriptor ptr, [32] property count, [40] property index,
    //   [48] descriptor row ptr, [56] property slot ptr, [80] x29, [88] x30.
    emitter.instruction("sub sp, sp, #96");                                     // allocate the object-walk frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the object-walk frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the paren base indent
    emitter.instruction("add x9, x1, #4");                                      // entry indent = base + 4
    emitter.instruction("str x9, [sp, #16]");                                   // save the entry indent

    // -- header: the class name from the shared class-id → (ptr, len) table --
    emitter.instruction("ldr x9, [x0]");                                        // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");              // resolve the class-name table extent
    emitter.instruction("ldr x10, [x10]");                                      // load the number of named class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the name table?
    emitter.instruction("b.hs __rt_pr_obj_anon");                               // an unknown class id writes no name
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");            // resolve the class-name entry table
    emitter.instruction("add x11, x11, x9, lsl #4");                            // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x1, [x11]");                                       // load the class-name pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the class-name length
    emitter.instruction("b __rt_pr_obj_name");                                  // write the resolved name
    emitter.label("__rt_pr_obj_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");             // fall back to the empty class-name slot
    emitter.instruction("mov x2, #0");                                          // a zero-length write emits nothing
    emitter.label("__rt_pr_obj_name");
    emitter.instruction("bl __rt_pr_write");                                    // write the class name

    // -- header suffix: PHP writes ` Object` for a class and ` Enum[:type]` for an enum --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_obj_enum_kind");                               // x0 = 0 plain / 1 pure / 2 int-backed / 3 string-backed
    emitter.instruction("cmp x0, #1");                                          // a pure enum prints ` Enum`
    emitter.instruction("b.eq __rt_pr_obj_sfx_enum");                           // select the bare enum suffix
    emitter.instruction("cmp x0, #2");                                          // an int-backed enum prints ` Enum:int`
    emitter.instruction("b.eq __rt_pr_obj_sfx_enum_int");                       // select the int-backed enum suffix
    emitter.instruction("cmp x0, #3");                                          // a string-backed enum prints ` Enum:string`
    emitter.instruction("b.eq __rt_pr_obj_sfx_enum_str");                       // select the string-backed enum suffix
    abi::emit_symbol_address(emitter, "x1", "_pr_object_suffix");               // load the ` Object\n` header suffix
    emitter.instruction("mov x2, #8");                                          // len(" Object\n") = 8
    emitter.instruction("b __rt_pr_obj_sfx_write");                             // write the selected suffix
    emitter.label("__rt_pr_obj_sfx_enum");
    abi::emit_symbol_address(emitter, "x1", "_pr_enum_suffix");                 // load the ` Enum\n` header suffix
    emitter.instruction("mov x2, #6");                                          // len(" Enum\n") = 6
    emitter.instruction("b __rt_pr_obj_sfx_write");                             // write the selected suffix
    emitter.label("__rt_pr_obj_sfx_enum_int");
    abi::emit_symbol_address(emitter, "x1", "_pr_enum_int_suffix");             // load the ` Enum:int\n` header suffix
    emitter.instruction("mov x2, #10");                                         // len(" Enum:int\n") = 10
    emitter.instruction("b __rt_pr_obj_sfx_write");                             // write the selected suffix
    emitter.label("__rt_pr_obj_sfx_enum_str");
    abi::emit_symbol_address(emitter, "x1", "_pr_enum_str_suffix");             // load the ` Enum:string\n` header suffix
    emitter.instruction("mov x2, #13");                                         // len(" Enum:string\n") = 13
    emitter.label("__rt_pr_obj_sfx_write");
    emitter.instruction("bl __rt_pr_write");                                    // write the header suffix

    // -- a revisited instance renders ` *RECURSION*` instead of a body --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_vd_seen_find");                                // is this object already on the walk stack?
    emitter.instruction("cbnz x0, __rt_pr_obj_recursion");                      // PHP renders a revisited object as *RECURSION*
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_vd_seen_push");                                // mark the object as being walked

    emitter.instruction("ldr x0, [sp, #8]");                                    // base → open helper argument
    emitter.instruction("bl __rt_print_r_open");                                // write `<base>(\n`

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_pr_obj_desc");                                 // x0 = this class's print_r descriptor
    emitter.instruction("str x0, [sp, #24]");                                   // save the descriptor pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the rendered property count
    emitter.instruction("str x9, [sp, #32]");                                   // save the property count for the loop guard
    emitter.instruction("str xzr, [sp, #40]");                                  // property index = 0

    emitter.label("__rt_pr_obj_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the property index
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the property count
    emitter.instruction("cmp x9, x10");                                         // rendered every property?
    emitter.instruction("b.ge __rt_pr_obj_done");                               // walk complete

    // -- resolve this property's 48-byte descriptor row --
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the descriptor pointer
    emitter.instruction(&format!("mov x12, #{}", PROP_DESC_ROW_BYTES));         // each descriptor row occupies 48 bytes
    emitter.instruction("mul x12, x9, x12");                                    // byte offset of this property's row
    emitter.instruction("add x11, x11, x12");                                   // advance into the descriptor
    emitter.instruction("add x11, x11, #8");                                    // skip the leading property-count word
    emitter.instruction("str x11, [sp, #48]");                                  // save the row pointer across the calls

    // -- resolve this property's 16-byte slot inside the instance --
    emitter.instruction("ldr x13, [x11, #16]");                                 // load the property's byte offset within the object
    emitter.instruction("ldr x14, [sp, #0]");                                   // reload the object pointer
    emitter.instruction("add x13, x14, x13");                                   // resolve the absolute property slot address
    emitter.instruction("str x13, [sp, #56]");                                  // save the slot pointer across the calls

    // -- PHP omits an uninitialized typed property from print_r entirely --
    emitter.instruction("ldr x14, [x13, #8]");                                  // load the slot's high word (the init marker)
    emit_uninit_sentinel_aarch64(emitter, "x15");                               // materialize the uninitialized-property marker
    emitter.instruction("cmp x14, x15");                                        // is this property still uninitialized?
    emitter.instruction("b.eq __rt_pr_obj_next");                               // skip it without emitting a line

    // -- emit `<entry indent>[KEY] => ` --
    emitter.instruction("ldr x0, [x11]");                                       // load the pre-rendered key pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // load the pre-rendered key length
    emitter.instruction("ldr x2, [sp, #16]");                                   // entry indent → key helper argument
    emitter.instruction("bl __rt_print_r_str_key");                             // write `<indent>[KEY] => `

    // -- render the value; __rt_print_r_value unboxes Mixed cells and recurses --
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the descriptor row pointer
    emitter.instruction("ldr x13, [sp, #56]");                                  // reload the property slot pointer
    emitter.instruction("ldr x0, [x11, #24]");                                  // property value tag → value renderer
    emitter.instruction("ldr x1, [x13]");                                       // slot low word → value renderer
    emitter.instruction("ldr x2, [x13, #8]");                                   // slot high word → value renderer
    emitter.instruction("cmp x0, #4");                                          // only pointer-shaped tags (4-7) can carry a null payload
    emitter.instruction("b.lt __rt_pr_obj_value");                              // scalar payloads keep their exact bit pattern
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter,
        "x1",
        "x9",
        "__rt_pr_obj_value_null",
    );
    emitter.instruction("b __rt_pr_obj_value");                                 // a real pointer renders through its own tag
    emitter.label("__rt_pr_obj_value_null");
    emitter.instruction("mov x0, #8");                                          // canonical PHP null: print_r renders the empty string
    emitter.label("__rt_pr_obj_value");
    emitter.instruction("ldr x3, [sp, #16]");                                   // entry indent
    emitter.instruction("add x3, x3, #4");                                      // nested container base = entry indent + 4
    emitter.instruction("bl __rt_print_r_value");                               // render the property value

    abi::emit_symbol_address(emitter, "x1", "_pr_nl");                          // load the line terminator
    emitter.instruction("mov x2, #1");                                          // len("\n") = 1
    emitter.instruction("bl __rt_pr_write");                                    // terminate the entry line

    emitter.label("__rt_pr_obj_next");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the property index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next property
    emitter.instruction("str x9, [sp, #40]");                                   // save the updated index
    emitter.instruction("b __rt_pr_obj_loop");                                  // continue the walk

    emitter.label("__rt_pr_obj_done");
    // -- dynamic properties follow the declared ones, in insertion order --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_obj_dump_dyn_props");                          // x0 = dynamic-property hash, or 0
    emitter.instruction("cbz x0, __rt_pr_obj_close");                           // no dynamic properties to append
    emitter.instruction("ldr x1, [sp, #16]");                                   // entry indent → entry walker
    emitter.instruction("bl __rt_print_r_hash_entries");                        // write one `[name] => value` line per dynamic property
    emitter.label("__rt_pr_obj_close");
    emitter.instruction("ldr x0, [sp, #8]");                                    // base → close helper argument
    emitter.instruction("bl __rt_print_r_close");                               // write `<base>)\n`
    emitter.instruction("bl __rt_vd_seen_pop");                                 // the object is no longer on the walk stack
    emitter.instruction("b __rt_pr_obj_exit");                                  // object rendered

    emitter.label("__rt_pr_obj_recursion");
    abi::emit_symbol_address(emitter, "x1", "_pr_recursion");                   // load the ` *RECURSION*` marker
    emitter.instruction("mov x2, #12");                                         // len(" *RECURSION*") = 12
    emitter.instruction("bl __rt_pr_write");                                    // write ` *RECURSION*` in place of the body

    emitter.label("__rt_pr_obj_exit");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the object-walk frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Materializes the uninitialized-typed-property sentinel into an AArch64 register.
///
/// The value (`0x7fff_ffff_ffff_fffd`) needs the full four-halfword `movz`/`movk`
/// sequence and must match `codegen_support::sentinels::
/// UNINITIALIZED_TYPED_PROPERTY_SENTINEL`, or an initialized property would be
/// silently dropped from the rendered body.
fn emit_uninit_sentinel_aarch64(emitter: &mut Emitter, reg: &str) {
    emitter.instruction(&format!("movz {}, #0xfffd", reg));                     // low halfword of the uninitialized-typed-property sentinel
    emitter.instruction(&format!("movk {}, #0xffff, lsl #16", reg));            // second halfword of the uninitialized sentinel
    emitter.instruction(&format!("movk {}, #0xffff, lsl #32", reg));            // third halfword of the uninitialized sentinel
    emitter.instruction(&format!("movk {}, #0x7fff, lsl #48", reg));            // top halfword of the uninitialized sentinel
}

/// Emits the Linux x86_64 print_r object walker.
fn emit_print_r_object_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_object ---");
    emitter.label_global("__rt_print_r_object");

    // rbp-relative frame: [-8] object ptr, [-16] base indent, [-24] entry indent,
    //   [-32] descriptor ptr, [-40] property count, [-48] property index,
    //   [-56] descriptor row ptr, [-64] property slot ptr.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the object-walk frame pointer
    emitter.instruction("sub rsp, 80");                                         // allocate the object-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the paren base indent
    emitter.instruction("lea rax, [rsi + 4]");                                  // entry indent = base + 4
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the entry indent

    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");              // resolve the class-name table extent
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the number of named class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id within the name table?
    emitter.instruction("jae __rt_pr_obj_anon_x86");                            // an unknown class id writes no name
    abi::emit_symbol_address(emitter, "r11", "_class_name_entries");            // resolve the class-name entry table
    emitter.instruction("shl r9, 4");                                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r11, r9");                                         // advance to this class's entry
    emitter.instruction("mov rsi, QWORD PTR [r11]");                            // load the class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load the class-name length
    emitter.instruction("jmp __rt_pr_obj_name_x86");                            // write the resolved name
    emitter.label("__rt_pr_obj_anon_x86");
    abi::emit_symbol_address(emitter, "rsi", "_class_name_missing");            // fall back to the empty class-name slot
    emitter.instruction("xor edx, edx");                                        // a zero-length write emits nothing
    emitter.label("__rt_pr_obj_name_x86");
    emitter.instruction("call __rt_pr_write");                                  // write the class name

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_obj_enum_kind");                             // rax = 0 plain / 1 pure / 2 int-backed / 3 string-backed
    emitter.instruction("cmp rax, 1");                                          // a pure enum prints ` Enum`
    emitter.instruction("je __rt_pr_obj_sfx_enum_x86");                         // select the bare enum suffix
    emitter.instruction("cmp rax, 2");                                          // an int-backed enum prints ` Enum:int`
    emitter.instruction("je __rt_pr_obj_sfx_enum_int_x86");                     // select the int-backed enum suffix
    emitter.instruction("cmp rax, 3");                                          // a string-backed enum prints ` Enum:string`
    emitter.instruction("je __rt_pr_obj_sfx_enum_str_x86");                     // select the string-backed enum suffix
    abi::emit_symbol_address(emitter, "rsi", "_pr_object_suffix");              // load the ` Object\n` header suffix
    emitter.instruction("mov edx, 8");                                          // len(" Object\n") = 8
    emitter.instruction("jmp __rt_pr_obj_sfx_write_x86");                       // write the selected suffix
    emitter.label("__rt_pr_obj_sfx_enum_x86");
    abi::emit_symbol_address(emitter, "rsi", "_pr_enum_suffix");                // load the ` Enum\n` header suffix
    emitter.instruction("mov edx, 6");                                          // len(" Enum\n") = 6
    emitter.instruction("jmp __rt_pr_obj_sfx_write_x86");                       // write the selected suffix
    emitter.label("__rt_pr_obj_sfx_enum_int_x86");
    abi::emit_symbol_address(emitter, "rsi", "_pr_enum_int_suffix");            // load the ` Enum:int\n` header suffix
    emitter.instruction("mov edx, 10");                                         // len(" Enum:int\n") = 10
    emitter.instruction("jmp __rt_pr_obj_sfx_write_x86");                       // write the selected suffix
    emitter.label("__rt_pr_obj_sfx_enum_str_x86");
    abi::emit_symbol_address(emitter, "rsi", "_pr_enum_str_suffix");            // load the ` Enum:string\n` header suffix
    emitter.instruction("mov edx, 13");                                         // len(" Enum:string\n") = 13
    emitter.label("__rt_pr_obj_sfx_write_x86");
    emitter.instruction("call __rt_pr_write");                                  // write the header suffix

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_vd_seen_find");                              // is this object already on the walk stack?
    emitter.instruction("cmp rax, 0");                                          // a hit means we are inside this same instance
    emitter.instruction("jne __rt_pr_obj_recursion_x86");                       // PHP renders a revisited object as *RECURSION*
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_vd_seen_push");                              // mark the object as being walked

    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // base → open helper argument
    emitter.instruction("call __rt_print_r_open");                              // write `<base>(\n`

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_pr_obj_desc");                               // rax = this class's print_r descriptor
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the descriptor pointer
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // load the rendered property count
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the property count for the loop guard
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // property index = 0

    emitter.label("__rt_pr_obj_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the property index
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the property count
    emitter.instruction("cmp r9, r10");                                         // rendered every property?
    emitter.instruction("jge __rt_pr_obj_done_x86");                            // walk complete

    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the descriptor pointer
    emitter.instruction(&format!("imul r9, r9, {}", PROP_DESC_ROW_BYTES));      // each descriptor row occupies 48 bytes
    emitter.instruction("add r11, r9");                                         // advance into the descriptor
    emitter.instruction("add r11, 8");                                          // skip the leading property-count word
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row pointer across the calls

    emitter.instruction("mov r10, QWORD PTR [r11 + 16]");                       // load the property's byte offset within the object
    emitter.instruction("add r10, QWORD PTR [rbp - 8]");                        // resolve the absolute property slot address
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save the slot pointer across the calls

    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // load the slot's high word (the init marker)
    emitter.instruction("movabs r8, 0x7ffffffffffffffd");                       // materialize the uninitialized-property marker
    emitter.instruction("cmp rax, r8");                                         // is this property still uninitialized?
    emitter.instruction("je __rt_pr_obj_next_x86");                             // skip it without emitting a line

    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the pre-rendered key pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the pre-rendered key length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // entry indent → key helper argument
    emitter.instruction("call __rt_print_r_str_key");                           // write `<indent>[KEY] => `

    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the descriptor row pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload the property slot pointer
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");                       // property value tag → value renderer
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // slot low word → value renderer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // slot high word → value renderer
    emitter.instruction("cmp rdi, 4");                                          // only pointer-shaped tags (4-7) can carry a null payload
    emitter.instruction("jl __rt_pr_obj_value_x86");                            // scalar payloads keep their exact bit pattern
    crate::codegen_support::sentinels::emit_branch_if_null_container(
        emitter,
        "rsi",
        "r8",
        "__rt_pr_obj_value_null_x86",
    );
    emitter.instruction("jmp __rt_pr_obj_value_x86");                           // a real pointer renders through its own tag
    emitter.label("__rt_pr_obj_value_null_x86");
    emitter.instruction("mov rdi, 8");                                          // canonical PHP null: print_r renders the empty string
    emitter.label("__rt_pr_obj_value_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // entry indent
    emitter.instruction("add rcx, 4");                                          // nested container base = entry indent + 4
    emitter.instruction("call __rt_print_r_value");                             // render the property value

    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");                         // load the line terminator
    emitter.instruction("mov edx, 1");                                          // len("\n") = 1
    emitter.instruction("call __rt_pr_write");                                  // terminate the entry line

    emitter.label("__rt_pr_obj_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the property index
    emitter.instruction("add r9, 1");                                           // advance to the next property
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the updated index
    emitter.instruction("jmp __rt_pr_obj_loop_x86");                            // continue the walk

    emitter.label("__rt_pr_obj_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_obj_dump_dyn_props");                        // rax = dynamic-property hash, or 0
    emitter.instruction("test rax, rax");                                       // does the object carry dynamic properties?
    emitter.instruction("jz __rt_pr_obj_close_x86");                            // no dynamic properties to append
    emitter.instruction("mov rdi, rax");                                        // dynamic-property hash → entry walker
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // entry indent → entry walker
    emitter.instruction("call __rt_print_r_hash_entries");                      // write one `[name] => value` line per dynamic property
    emitter.label("__rt_pr_obj_close_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // base → close helper argument
    emitter.instruction("call __rt_print_r_close");                             // write `<base>)\n`
    emitter.instruction("call __rt_vd_seen_pop");                               // the object is no longer on the walk stack
    emitter.instruction("jmp __rt_pr_obj_exit_x86");                            // object rendered

    emitter.label("__rt_pr_obj_recursion_x86");
    abi::emit_symbol_address(emitter, "rsi", "_pr_recursion");                  // load the ` *RECURSION*` marker
    emitter.instruction("mov edx, 12");                                         // len(" *RECURSION*") = 12
    emitter.instruction("call __rt_pr_write");                                  // write ` *RECURSION*` in place of the body

    emitter.label("__rt_pr_obj_exit_x86");
    emitter.instruction("add rsp, 80");                                         // release the object-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
