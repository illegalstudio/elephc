//! Purpose:
//! Emits the runtime helpers that let the value renderers recognize a PHP enum
//! case behind an ordinary object pointer: `__rt_obj_enum_kind`,
//! `__rt_obj_enum_name_offset`, `__rt_obj_enum_case_name`, and the
//! `__rt_var_dump_emit_enum_line` line emitter that renders `enum(E::C)`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::objects`.
//! - `__rt_vd_val_obj` (tag 6) in `runtime::io::var_dump_walk`, which consults
//!   `__rt_obj_enum_name_offset` before opening an object body.
//! - `__rt_print_r_object` in `super::print_r_object`, for PHP's ` Enum` /
//!   ` Enum:int` / ` Enum:string` header suffix.
//! - The `__elephc_object_is_enum` prelude builtin used by `var_export`.
//!
//! Key details:
//! - Enum-ness is a property of the CLASS, not of the instance: the object header
//!   carries a class id, and `_class_enum_kinds[class_id]` /
//!   `_class_enum_name_offsets[class_id]` (emitted by
//!   `crate::codegen_support::runtime::data::user`) answer both questions with one
//!   indexed load. Kind is `0` for a plain class and `1`/`2`/`3` for a pure /
//!   int-backed / string-backed enum; the name offset is `-1` for a plain class.
//! - Both tables are bounds-checked against `_class_gc_desc_count`, the shared
//!   class-id table extent every other per-class lookup uses, so a synthetic or
//!   stale class id reports "not an enum" instead of reading past the table.
//! - elephc materializes PHP's readonly `name` case property as an ordinary
//!   declared string property, so the case name is just the 16-byte `(ptr, len)`
//!   pair at that offset — no case table lookup and no allocation.
//! - `__rt_var_dump_emit_enum_line` writes through `__rt_vd_pad` / `__rt_vd_write`,
//!   the same indent-aware sink every other var_dump line uses, so a nested enum
//!   inside an array or object lands at the right column for free.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// `__rt_obj_enum_kind`: classify an object's class as plain or enum.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
/// Output: AArch64 x0 / x86_64 rax = 0 for a plain class, 1 for a pure enum,
/// 2 for an int-backed enum, 3 for a string-backed enum.
pub fn emit_obj_enum_kind(emitter: &mut Emitter) {
    emit_class_table_lookup(
        emitter,
        "__rt_obj_enum_kind",
        "_class_enum_kinds",
        "__rt_obj_enum_kind_none",
        0,
    );
}

/// `__rt_obj_enum_name_offset`: locate an enum instance's `name` property slot.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
/// Output: AArch64 x0 / x86_64 rax = byte offset of the `name` slot within the
/// instance, or `-1` when the class is not an enum (which is also what every
/// caller uses as the "this is an ordinary object" test).
pub fn emit_obj_enum_name_offset(emitter: &mut Emitter) {
    emit_class_table_lookup(
        emitter,
        "__rt_obj_enum_name_offset",
        "_class_enum_name_offsets",
        "__rt_obj_enum_name_offset_none",
        -1,
    );
}

/// Emits a bounds-checked `table[object->class_id]` lookup helper.
///
/// `miss_label` names the out-of-range arm and `miss_value` is what an unknown
/// class id reports; both tables this serves are `.quad`-per-class-id and are
/// sized from the same `_class_gc_desc_count` extent as the descriptor tables.
fn emit_class_table_lookup(
    emitter: &mut Emitter,
    symbol: &str,
    table: &str,
    miss_label: &str,
    miss_value: i64,
) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", symbol.trim_start_matches("__rt_")));
    emitter.label_global(symbol);

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("ldr x10, [x10]");                              // load the number of registered class ids
            emitter.instruction("cmp x9, x10");                                 // is the class id within the per-class tables?
            emitter.instruction(&format!("b.hs {}", miss_label));               // an unknown class id reports the miss value
            abi::emit_symbol_address(emitter, "x11", table);                    // resolve the per-class enum table
            emitter.instruction("ldr x0, [x11, x9, lsl #3]");                   // load this class's entry
            emitter.instruction("ret");                                         // return to caller
            emitter.label(miss_label);
            emitter.instruction(&format!("mov x0, #{}", miss_value));           // report the not-an-enum value
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rdi]");                     // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // load the number of registered class ids
            emitter.instruction("cmp r9, r10");                                 // is the class id within the per-class tables?
            emitter.instruction(&format!("jae {}_x86", miss_label));            // an unknown class id reports the miss value
            abi::emit_symbol_address(emitter, "r11", table);                    // resolve the per-class enum table
            emitter.instruction("mov rax, QWORD PTR [r11 + r9 * 8]");           // load this class's entry
            emitter.instruction("ret");                                         // return to caller
            emitter.label(&format!("{}_x86", miss_label));
            emitter.instruction(&format!("mov rax, {}", miss_value));           // report the not-an-enum value
            emitter.instruction("ret");                                         // return to caller
        }
    }
}

/// `__rt_obj_enum_case_name`: read an enum instance's case-name string.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer of a class already known to be
/// an enum. Output: AArch64 x0=ptr x1=len / x86_64 rax=ptr rdx=len. A class with
/// no `name` slot yields a zero-length string rather than a wild pointer.
pub fn emit_obj_enum_case_name(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: obj_enum_case_name ---");
    emitter.label_global("__rt_obj_enum_case_name");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // allocate the case-name frame
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #16");                            // establish the case-name frame pointer
            emitter.instruction("str x0, [sp, #0]");                            // save the object pointer across the lookup
            emitter.instruction("bl __rt_obj_enum_name_offset");                // x0 = `name` slot byte offset, or -1
            emitter.instruction("cmp x0, #0");                                  // does this class carry a `name` slot?
            emitter.instruction("b.lt __rt_obj_enum_case_name_none");           // a plain class reports an empty case name
            emitter.instruction("ldr x9, [sp, #0]");                            // reload the object pointer
            emitter.instruction("add x9, x9, x0");                              // resolve the absolute `name` slot address
            emitter.instruction("ldr x1, [x9, #8]");                            // load the case-name length from the slot high word
            emitter.instruction("ldr x0, [x9]");                                // load the case-name pointer from the slot low word
            emitter.instruction("b __rt_obj_enum_case_name_done");              // return the resolved case name
            emitter.label("__rt_obj_enum_case_name_none");
            emitter.instruction("mov x0, #0");                                  // no `name` slot → null pointer
            emitter.instruction("mov x1, #0");                                  // no `name` slot → zero length
            emitter.label("__rt_obj_enum_case_name_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the case-name frame
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // save caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the case-name frame pointer
            emitter.instruction("sub rsp, 16");                                 // allocate the case-name frame
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // save the object pointer across the lookup
            emitter.instruction("call __rt_obj_enum_name_offset");              // rax = `name` slot byte offset, or -1
            emitter.instruction("cmp rax, 0");                                  // does this class carry a `name` slot?
            emitter.instruction("jl __rt_obj_enum_case_name_none_x86");         // a plain class reports an empty case name
            emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                 // reload the object pointer
            emitter.instruction("add r9, rax");                                 // resolve the absolute `name` slot address
            emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                 // load the case-name length from the slot high word
            emitter.instruction("mov rax, QWORD PTR [r9]");                     // load the case-name pointer from the slot low word
            emitter.instruction("jmp __rt_obj_enum_case_name_done_x86");        // return the resolved case name
            emitter.label("__rt_obj_enum_case_name_none_x86");
            emitter.instruction("xor rax, rax");                                // no `name` slot → null pointer
            emitter.instruction("xor rdx, rdx");                                // no `name` slot → zero length
            emitter.label("__rt_obj_enum_case_name_done_x86");
            emitter.instruction("add rsp, 16");                                 // release the case-name frame
            emitter.instruction("pop rbp");                                     // restore caller frame pointer
            emitter.instruction("ret");                                         // return to caller
        }
    }
}

/// `__rt_var_dump_emit_enum_line`: write `<indent>enum(Class::Case)\n`.
///
/// This is what PHP prints for an enum case instead of an object body, at every
/// nesting depth. Input: AArch64 x0 / x86_64 rdi = enum instance pointer.
pub fn emit_var_dump_emit_enum_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_enum_line_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_enum_line ---");
    emitter.label_global("__rt_var_dump_emit_enum_line");

    // Frame (32 bytes): [0] object ptr, [16] saved x29, [24] saved x30.
    emitter.instruction("sub sp, sp, #32");                                     // allocate the enum-line frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the enum-line frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the enum instance pointer

    emitter.instruction("bl __rt_vd_pad_value");                                // indent, then write php's pending `&` marker
    abi::emit_symbol_address(emitter, "x1", "_vd_enum_prefix");                 // load the `enum(` prefix
    emitter.instruction("mov x2, #5");                                          // len("enum(") = 5
    emitter.instruction("bl __rt_vd_write");                                    // write `enum(`

    // -- class name from the shared class-id → (name ptr, name len) table --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the enum instance pointer
    emitter.instruction("ldr x9, [x9]");                                        // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");              // resolve the class-name table extent
    emitter.instruction("ldr x10, [x10]");                                      // load the number of named class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the name table?
    emitter.instruction("b.hs __rt_vd_enum_anon");                              // an unknown class id writes no name
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");            // resolve the class-name entry table
    emitter.instruction("add x11, x11, x9, lsl #4");                            // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x1, [x11]");                                       // load the class-name pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the class-name length
    emitter.instruction("b __rt_vd_enum_name");                                 // write the resolved name
    emitter.label("__rt_vd_enum_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");             // fall back to the empty class-name slot
    emitter.instruction("mov x2, #0");                                          // a zero-length write emits nothing
    emitter.label("__rt_vd_enum_name");
    emitter.instruction("bl __rt_vd_write");                                    // write the enum class name

    abi::emit_symbol_address(emitter, "x1", "_vd_enum_sep");                    // load the `::` case separator
    emitter.instruction("mov x2, #2");                                          // len("::") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write `::`

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the enum instance pointer
    emitter.instruction("bl __rt_obj_enum_case_name");                          // x0=case-name ptr, x1=case-name len
    emitter.instruction("mov x2, x1");                                          // case-name length → write length argument
    emitter.instruction("mov x1, x0");                                          // case-name pointer → write buffer argument
    emitter.instruction("bl __rt_vd_write");                                    // write the case name

    abi::emit_symbol_address(emitter, "x1", "_vd_enum_close");                  // load the `)\n` terminator
    emitter.instruction("mov x2, #2");                                          // len(")\n") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write `)\n`

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the enum-line frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 `enum(Class::Case)` var_dump line emitter.
fn emit_var_dump_emit_enum_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_enum_line ---");
    emitter.label_global("__rt_var_dump_emit_enum_line");

    // rbp-relative frame: [-8] object ptr.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the enum-line frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the enum-line frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the enum instance pointer

    emitter.instruction("call __rt_vd_pad_value");                              // indent, then write php's pending `&` marker
    abi::emit_symbol_address(emitter, "rsi", "_vd_enum_prefix");                // load the `enum(` prefix
    emitter.instruction("mov edx, 5");                                          // len("enum(") = 5
    emitter.instruction("call __rt_vd_write");                                  // write `enum(`

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the enum instance pointer
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");              // resolve the class-name table extent
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the number of named class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id within the name table?
    emitter.instruction("jae __rt_vd_enum_anon_x86");                           // an unknown class id writes no name
    abi::emit_symbol_address(emitter, "r11", "_class_name_entries");            // resolve the class-name entry table
    emitter.instruction("shl r9, 4");                                           // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r11, r9");                                         // advance to this class's entry
    emitter.instruction("mov rsi, QWORD PTR [r11]");                            // load the class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load the class-name length
    emitter.instruction("jmp __rt_vd_enum_name_x86");                           // write the resolved name
    emitter.label("__rt_vd_enum_anon_x86");
    abi::emit_symbol_address(emitter, "rsi", "_class_name_missing");            // fall back to the empty class-name slot
    emitter.instruction("xor edx, edx");                                        // a zero-length write emits nothing
    emitter.label("__rt_vd_enum_name_x86");
    emitter.instruction("call __rt_vd_write");                                  // write the enum class name

    abi::emit_symbol_address(emitter, "rsi", "_vd_enum_sep");                   // load the `::` case separator
    emitter.instruction("mov edx, 2");                                          // len("::") = 2
    emitter.instruction("call __rt_vd_write");                                  // write `::`

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the enum instance pointer
    emitter.instruction("call __rt_obj_enum_case_name");                        // rax=case-name ptr, rdx=case-name len
    emitter.instruction("mov rsi, rax");                                        // case-name pointer → write buffer argument
    emitter.instruction("call __rt_vd_write");                                  // write the case name

    abi::emit_symbol_address(emitter, "rsi", "_vd_enum_close");                 // load the `)\n` terminator
    emitter.instruction("mov edx, 2");                                          // len(")\n") = 2
    emitter.instruction("call __rt_vd_write");                                  // write `)\n`

    emitter.instruction("add rsp, 16");                                         // release the enum-line frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
