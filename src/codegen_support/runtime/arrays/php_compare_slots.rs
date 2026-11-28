//! Purpose:
//! Emits `__rt_php_compare_slots` / `__rt_php_compare_slots_desc`: comparator
//! callbacks that order two boxed `Mixed` array slots by PHP's own rules.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Uses `(a, b)` in the first two argument registers and ordering in the result
//!   register. The stable scalar-slot sorter consumes this adapter without
//!   duplicating the shared PHP ordering table.
//! - A validation pass rejects non-scalar runtime tags before sorting. Internal
//!   contract classes that declare a comparison adapter (GMP) are the exception:
//!   their boxed objects reach the program-specific comparator instead of being
//!   silently reported equal, while containers and unhandled objects still fail.
//! - The ordering itself is `__rt_php_compare`, which is what `<` and `<=>`
//!   already use for runtime-tagged operands. Reimplementing PHP's cross-type
//!   comparison here would be a second answer to a question the runtime already
//!   answers, and the two would drift.
//! - The descending variant negates the result rather than swapping the operands.
//!   NAN comparisons are not antisymmetric, so swapping operands would give a
//!   different result from reversing the PHP comparison result.

use elephc_memoir::{
    hash_layout::{self, EntryField, HeaderField},
    values::packed_slots,
};

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::data::MIXED_SORT_NON_SCALAR_MSG;

/// Emits the Mixed-sort scalar guard and both comparator callbacks for the current target.
pub fn emit_php_compare_slots(emitter: &mut Emitter) {
    emit_mixed_sort_scalar_guard(emitter);
    emit_one(emitter, "__rt_php_compare_slots", false);
    emit_one(emitter, "__rt_php_compare_slots_desc", true);
}

/// Emits a target-specific guard that rejects unsupported Mixed array elements.
///
/// The helper receives an indexed-array pointer in the first integer argument
/// register and returns the same pointer in the integer result register. Tags
/// 0 through 3 and tag 8 are the supported scalar and null values; tag 6 is
/// accepted only when the object's class declares a comparison adapter. Any
/// other value reports the unsupported runtime value and terminates before
/// sorting.
fn emit_mixed_sort_scalar_guard(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_sort_require_scalars ---");
    emitter.label_global("__rt_mixed_sort_require_scalars");

    match emitter.target.arch {
        Arch::AArch64 => {
            // -- scan every boxed Mixed slot before the sorter can mutate the array --
            emitter.instruction("ldr x9, [x0]");                                // load the indexed-array element count
            emitter.instruction("mov x10, #0");                                 // start at the first Mixed slot
            packed_slots::emit_packed_payload_aarch64(emitter, "x11", "x0");
            emitter.label("__rt_mixed_sort_scalar_scan");
            emitter.instruction("cmp x10, x9");                                 // have all live Mixed slots been validated?
            emitter.instruction("b.hs __rt_mixed_sort_scalar_done");            // return once every runtime tag is supported
            emitter.instruction("ldr x12, [x11, x10, lsl #3]");                 // load the boxed Mixed cell pointer for this slot
            emitter.instruction("ldr x12, [x12]");                              // read the cell's runtime value tag
            emitter.instruction("cmp x12, #3");                                 // are int, string, float, or bool semantics sufficient?
            emitter.instruction("b.ls __rt_mixed_sort_scalar_next");            // accept runtime scalar tags 0 through 3
            emitter.instruction("cmp x12, #8");                                 // is this value PHP null?
            emitter.instruction("b.eq __rt_mixed_sort_scalar_next");            // null is a supported scalar comparison operand
            emitter.instruction("cmp x12, #6");                                 // is this value an object?
            emitter.instruction("b.ne __rt_mixed_sort_non_scalar");             // reject containers, resources, and callables
            // -- objects sort only through a reviewed internal comparison adapter --
            emitter.instruction("ldr x12, [x11, x10, lsl #3]");                 // reload this slot's boxed Mixed cell
            emitter.instruction("ldr x12, [x12, #8]");                          // load the object pointer from the cell payload
            elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x12", "x12");
            abi::emit_symbol_address(emitter, "x13", "_class_compare_ptrs");    // resolve the per-class comparison adapter table
            emitter.instruction("ldr x13, [x13, x12, lsl #3]");                 // read this class's comparison adapter
            emitter.instruction("cbz x13, __rt_mixed_sort_non_scalar");         // unhandled object classes still fail closed
            emitter.label("__rt_mixed_sort_scalar_next");
            emitter.instruction("add x10, x10, #1");                            // advance to the next Mixed slot
            emitter.instruction("b __rt_mixed_sort_scalar_scan");               // continue validating runtime tags
            emitter.label("__rt_mixed_sort_scalar_done");
            emitter.instruction("ret");                                         // return the unchanged array pointer in x0

            // -- local conditional target that reaches the shared fatal helper --
            emitter.label("__rt_mixed_sort_non_scalar");
            abi::emit_jump(emitter, "__rt_mixed_sort_reject_non_scalar");
            // -- report an unsupported runtime value before terminating --
            emitter.label_global("__rt_mixed_sort_reject_non_scalar");
            if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
            emitter.instruction("mov x0, #2");                                  // select stderr for the explicit Mixed-sort diagnostic
            abi::emit_symbol_address(emitter, "x1", "_mixed_sort_non_scalar_msg");
            emitter.instruction(&format!("mov x2, #{}", MIXED_SORT_NON_SCALAR_MSG.len())); // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            // -- scan every boxed Mixed slot before the sorter can mutate the array --
            emitter.instruction("sub rsp, 8");                                  // align the stack for the fatal path's output-buffer flush
            emitter.instruction("mov rax, rdi");                                // preserve the array pointer as the helper result
            emitter.instruction("mov rcx, QWORD PTR [rax]");                    // load the indexed-array element count
            emitter.instruction("xor edx, edx");                                // start at the first Mixed slot
            packed_slots::emit_packed_payload_x86_64(emitter, "r8", "rax");
            emitter.label("__rt_mixed_sort_scalar_scan_x86");
            emitter.instruction("cmp rdx, rcx");                                // have all live Mixed slots been validated?
            emitter.instruction("jae __rt_mixed_sort_scalar_done_x86");         // return once every runtime tag is supported
            emitter.instruction("mov r9, QWORD PTR [r8 + rdx * 8]");            // load the boxed Mixed cell pointer for this slot
            emitter.instruction("mov r9, QWORD PTR [r9]");                      // read the cell's runtime value tag
            emitter.instruction("cmp r9, 3");                                   // are int, string, float, or bool semantics sufficient?
            emitter.instruction("jbe __rt_mixed_sort_scalar_next_x86");         // accept runtime scalar tags 0 through 3
            emitter.instruction("cmp r9, 8");                                   // is this value PHP null?
            emitter.instruction("je __rt_mixed_sort_scalar_next_x86");          // null is a supported scalar comparison operand
            emitter.instruction("cmp r9, 6");                                   // is this value an object?
            emitter.instruction("jne __rt_mixed_sort_non_scalar_x86");          // reject containers, resources, and callables
            // -- objects sort only through a reviewed internal comparison adapter --
            emitter.instruction("mov r9, QWORD PTR [r8 + rdx * 8]");            // reload this slot's boxed Mixed cell
            emitter.instruction("mov r9, QWORD PTR [r9 + 8]");                  // load the object pointer from the cell payload
            elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r9", "r9");
            abi::emit_symbol_address(emitter, "r10", "_class_compare_ptrs");    // resolve the per-class comparison adapter table
            emitter.instruction("cmp QWORD PTR [r10 + r9 * 8], 0");             // does this class declare a comparison adapter?
            emitter.instruction("je __rt_mixed_sort_non_scalar_x86");           // unhandled object classes still fail closed
            emitter.label("__rt_mixed_sort_scalar_next_x86");
            emitter.instruction("add rdx, 1");                                  // advance to the next Mixed slot
            emitter.instruction("jmp __rt_mixed_sort_scalar_scan_x86");         // continue validating runtime tags
            emitter.label("__rt_mixed_sort_scalar_done_x86");
            emitter.instruction("add rsp, 8");                                  // release the alignment slot before returning
            emitter.instruction("ret");                                         // return the unchanged array pointer in rax

            // -- local conditional target that reaches the shared fatal helper --
            emitter.label("__rt_mixed_sort_non_scalar_x86");
            abi::emit_jump(emitter, "__rt_mixed_sort_reject_non_scalar_x86");
            // -- report an unsupported runtime value before terminating --
            emitter.label_global("__rt_mixed_sort_reject_non_scalar_x86");
            emitter.instruction("and rsp, -16");                                // realign any caller path before the output flush call
            if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
            emitter.instruction("mov edi, 2");                                  // select stderr for the explicit Mixed-sort diagnostic
            abi::emit_symbol_address(emitter, "rsi", "_mixed_sort_non_scalar_msg");
            emitter.instruction(&format!("mov edx, {}", MIXED_SORT_NON_SCALAR_MSG.len())); // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // select the Linux x86_64 write syscall
            emitter.instruction("syscall");                                     // write the explicit Mixed-sort diagnostic
            abi::emit_cdylib_exit_escape(emitter);
            emitter.instruction("call __rt_ob_flush_all");                      // flush buffered output from the aligned helper frame
            emitter.instruction("mov edi, 1");                                  // select process failure status
            emitter.instruction("mov eax, 231");                                // Linux x86_64 syscall 231 is exit_group
            emitter.instruction("syscall");                                     // terminate after reporting the unsupported value
        }
    }
}

/// Emits one target-specific Mixed-slot comparator in the requested direction.
fn emit_one(emitter: &mut Emitter, label: &str, descending: bool) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {label} ---"));
    emitter.label_global(label);

    match emitter.target.arch {
        Arch::AArch64 => {
            // Frame: [sp,#0] b's cell, [sp,#8] a.tag, [sp,#16] a.lo, [sp,#24] a.hi.
            emitter.instruction("sub sp, sp, #48");                             // reserve comparator locals and the saved frame record
            emitter.instruction("stp x29, x30, [sp, #32]");                     // save the frame record across two nested calls
            emitter.instruction("add x29, sp, #32");                            // establish a stable comparator frame pointer
            emitter.instruction("str x1, [sp, #0]");                            // keep b's cell: unboxing a clobbers the argument registers
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // a becomes x0=tag, x1=lo, x2=hi
            emitter.instruction("stp x0, x1, [sp, #8]");                        // stash a's tag and low word
            emitter.instruction("str x2, [sp, #24]");                           // stash a's high word
            emitter.instruction("ldr x0, [sp, #0]");                            // reload b's boxed Mixed cell for unboxing
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // b becomes x0=tag, x1=lo, x2=hi
            emitter.instruction("mov x3, x0");                                  // right tag
            emitter.instruction("mov x4, x1");                                  // right low word
            emitter.instruction("mov x5, x2");                                  // right high word
            emitter.instruction("ldp x0, x1, [sp, #8]");                        // left tag and low word
            emitter.instruction("ldr x2, [sp, #24]");                           // left high word
            abi::emit_call_label(emitter, "__rt_php_compare");                  // return ordering as x0 = -1, 0, or 1
            if descending {
                emitter.instruction("neg x0, x0");                              // reverse the order without reversing equal elements
            }
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore the caller frame pointer and return address
            emitter.instruction("add sp, sp, #48");                             // release comparator locals
            emitter.instruction("ret");                                         // return the PHP ordering result
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish a stable comparator frame
            emitter.instruction("sub rsp, 48");                                 // locals, and rsp stays 16-byte aligned for the nested calls
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // keep b's cell before unboxing a clobbers it
            emitter.instruction("mov rax, rdi");                                // __rt_mixed_unbox reads its cell from rax
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // a becomes rax=tag, rdi=lo, rdx=hi
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // save a's runtime value tag
            emitter.instruction("mov QWORD PTR [rbp - 24], rdi");               // save a's low payload word
            emitter.instruction("mov QWORD PTR [rbp - 32], rdx");               // save a's high payload word
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // reload b's boxed Mixed cell for unboxing
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // b becomes rax=tag, rdi=lo, rdx=hi
            emitter.instruction("mov rcx, rax");                                // right tag
            emitter.instruction("mov r8, rdi");                                 // right low word
            emitter.instruction("mov r9, rdx");                                 // right high word
            emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");               // left tag
            emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");               // left low word
            emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");               // left high word
            abi::emit_call_label(emitter, "__rt_php_compare");                  // return ordering as rax = -1, 0, or 1
            if descending {
                emitter.instruction("neg rax");                                 // reverse the order without reversing equal elements
            }
            emitter.instruction("add rsp, 48");                                 // release comparator locals
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the PHP ordering result
        }
    }
}

/// Emits `__rt_hash_sort_require_scalars`, the hash-backed sibling of the packed guard.
///
/// Hash entries store one runtime tag per entry: a boxed `Mixed` cell (tag 7) or a managed
/// reference cell (tag 11) hides the comparable tag one level down. The guard walks every
/// occupied slot, normalizes those two shapes to the value they own, and joins the shared
/// `__rt_mixed_sort_non_scalar` diagnostic for arrays, resources, callables, and objects
/// without a comparison adapter. Input/output: the hash table in `x0` / `rdi`; the helper
/// returns it unchanged so the caller can tail into the value sorter.
pub fn emit_hash_sort_require_scalars(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_sort_require_scalars ---");
    emitter.label_global("__rt_hash_sort_require_scalars");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x0, __rt_hash_sort_scalar_done");          // a null table has no entries to validate
            hash_layout::emit_load_header_field(emitter, "x9", "x0", HeaderField::Count);
            emitter.instruction("cbz x9, __rt_hash_sort_scalar_done");          // an empty hash is trivially sortable
            hash_layout::emit_load_header_field(emitter, "x10", "x0", HeaderField::Capacity);
            hash_layout::emit_entries(emitter, "x11", "x0");
            emitter.instruction("mov x12, #0");                                 // start at the first occupied slot
            emitter.label("__rt_hash_sort_scalar_scan");
            emitter.instruction("cmp x12, x10");                                // have all entry slots been inspected?
            emitter.instruction("b.hs __rt_hash_sort_scalar_done");             // return once every live value is supported
            hash_layout::emit_entry_address_from_storage(emitter, "x13", "x11", "x12");
            hash_layout::emit_load_entry_field(emitter, "x14", "x13", EntryField::Occupied);
            emitter.instruction("cmp x14, #1");                                 // only live entries carry a value to compare
            emitter.instruction("b.ne __rt_hash_sort_scalar_next");             // skip empty and tombstone slots
            emitter.instruction("mov x16, #0");                                 // inline payloads have no boxed Mixed pointer
            hash_layout::emit_load_entry_field(emitter, "x15", "x13", EntryField::ValueTag);
            emitter.instruction("cmp x15, #11");                                // does this entry belong to a PHP reference set?
            emitter.instruction("b.eq __rt_hash_sort_scalar_reference");        // the managed cell owns the comparable value
            emitter.instruction("cmp x15, #7");                                 // does this entry hold a boxed Mixed cell?
            emitter.instruction("b.eq __rt_hash_sort_scalar_boxed");            // the box carries the comparable tag
            emitter.instruction("b __rt_hash_sort_scalar_classify");            // inline scalar and object payloads are ready
            emitter.label("__rt_hash_sort_scalar_reference");
            hash_layout::emit_load_entry_field(emitter, "x16", "x13", EntryField::ValueLow);
            emitter.instruction("ldr x16, [x16]");                              // read the boxed Mixed value the cell owns
            emitter.instruction("ldr x15, [x16]");                              // read the comparable runtime tag
            emitter.instruction("b __rt_hash_sort_scalar_classify");            // classify the dereferenced value
            emitter.label("__rt_hash_sort_scalar_boxed");
            hash_layout::emit_load_entry_field(emitter, "x16", "x13", EntryField::ValueLow);
            emitter.instruction("ldr x15, [x16]");                              // read the comparison tag from the cell
            emitter.label("__rt_hash_sort_scalar_classify");
            emitter.instruction("cmp x15, #3");                                 // are int, string, float, or bool semantics sufficient?
            emitter.instruction("b.ls __rt_hash_sort_scalar_next");             // accept runtime scalar tags 0 through 3
            emitter.instruction("cmp x15, #8");                                 // is this value PHP null?
            emitter.instruction("b.eq __rt_hash_sort_scalar_next");             // null is a supported scalar comparison operand
            emitter.instruction("cmp x15, #6");                                 // is this value an object?
            emitter.instruction("b.ne __rt_hash_sort_scalar_non_scalar");       // reject containers, resources, and callables
            emitter.instruction("cbz x16, __rt_hash_sort_scalar_inline_object"); // an inline object pointer lives in the entry
            emitter.instruction("ldr x14, [x16, #8]");                          // load the object pointer from the box payload
            emitter.instruction("b __rt_hash_sort_scalar_object_class");        // resolve the object's comparison adapter
            emitter.label("__rt_hash_sort_scalar_inline_object");
            hash_layout::emit_load_entry_field(emitter, "x14", "x13", EntryField::ValueLow);
            emitter.label("__rt_hash_sort_scalar_object_class");
            elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x14", "x14");
            abi::emit_symbol_address(emitter, "x17", "_class_compare_ptrs");    // resolve the per-class comparison adapter table
            emitter.instruction("ldr x14, [x17, x14, lsl #3]");                 // read this class's comparison adapter
            emitter.instruction("cbz x14, __rt_hash_sort_scalar_non_scalar");   // unhandled object classes still fail closed
            emitter.label("__rt_hash_sort_scalar_next");
            emitter.instruction("add x12, x12, #1");                            // advance to the next entry slot
            emitter.instruction("b __rt_hash_sort_scalar_scan");                // continue validating runtime tags
            emitter.label("__rt_hash_sort_scalar_done");
            emitter.instruction("ret");                                         // return the unchanged table pointer in x0
            emitter.label("__rt_hash_sort_scalar_non_scalar");
            abi::emit_jump(emitter, "__rt_mixed_sort_reject_non_scalar");       // share the explicit Mixed-sort fatal helper
        }
        Arch::X86_64 => {
            emitter.instruction("test rdi, rdi");                               // a null table has no entries to validate
            emitter.instruction("jz __rt_hash_sort_scalar_done_x86");           // return without touching a null table
            hash_layout::emit_load_header_field(emitter, "r11", "rdi", HeaderField::Count);
            emitter.instruction("cmp r11, 0");                                  // does the hash hold any live entries?
            emitter.instruction("je __rt_hash_sort_scalar_done_x86");           // an empty hash is trivially sortable
            hash_layout::emit_load_header_field(emitter, "rcx", "rdi", HeaderField::Capacity);
            hash_layout::emit_entries(emitter, "r8", "rdi");
            emitter.instruction("xor r9d, r9d");                                // start at the first occupied slot
            emitter.label("__rt_hash_sort_scalar_scan_x86");
            emitter.instruction("cmp r9, rcx");                                 // have all entry slots been inspected?
            emitter.instruction("jae __rt_hash_sort_scalar_done_x86");          // return once every live value is supported
            hash_layout::emit_entry_address_from_storage(emitter, "r10", "r8", "r9");
            hash_layout::emit_load_entry_field(emitter, "rax", "r10", EntryField::Occupied);
            emitter.instruction("cmp rax, 1");                                  // only live entries carry a value to compare
            emitter.instruction("jne __rt_hash_sort_scalar_next_x86");          // skip empty and tombstone slots
            emitter.instruction("xor edx, edx");                                // inline payloads have no boxed Mixed pointer
            hash_layout::emit_load_entry_field(emitter, "r11", "r10", EntryField::ValueTag);
            emitter.instruction("cmp r11, 11");                                 // does this entry belong to a PHP reference set?
            emitter.instruction("je __rt_hash_sort_scalar_reference_x86");      // the managed cell owns the comparable value
            emitter.instruction("cmp r11, 7");                                  // does this entry hold a boxed Mixed cell?
            emitter.instruction("je __rt_hash_sort_scalar_boxed_x86");          // the box carries the comparable tag
            emitter.instruction("jmp __rt_hash_sort_scalar_classify_x86");      // inline scalar and object payloads are ready
            emitter.label("__rt_hash_sort_scalar_reference_x86");
            hash_layout::emit_load_entry_field(emitter, "rdx", "r10", EntryField::ValueLow);
            emitter.instruction("mov rdx, QWORD PTR [rdx]");                    // read the boxed Mixed value the cell owns
            emitter.instruction("mov r11, QWORD PTR [rdx]");                    // read the comparable runtime tag
            emitter.instruction("jmp __rt_hash_sort_scalar_classify_x86");      // classify the dereferenced value
            emitter.label("__rt_hash_sort_scalar_boxed_x86");
            hash_layout::emit_load_entry_field(emitter, "rdx", "r10", EntryField::ValueLow);
            emitter.instruction("mov r11, QWORD PTR [rdx]");                    // read the comparison tag from the cell
            emitter.label("__rt_hash_sort_scalar_classify_x86");
            emitter.instruction("cmp r11, 3");                                  // are int, string, float, or bool semantics sufficient?
            emitter.instruction("jbe __rt_hash_sort_scalar_next_x86");          // accept runtime scalar tags 0 through 3
            emitter.instruction("cmp r11, 8");                                  // is this value PHP null?
            emitter.instruction("je __rt_hash_sort_scalar_next_x86");           // null is a supported scalar comparison operand
            emitter.instruction("cmp r11, 6");                                  // is this value an object?
            emitter.instruction("jne __rt_hash_sort_scalar_non_scalar_x86");    // reject containers, resources, and callables
            emitter.instruction("test rdx, rdx");                               // does a box carry this object value?
            emitter.instruction("jz __rt_hash_sort_scalar_inline_object_x86");  // an inline object pointer lives in the entry
            emitter.instruction("mov rax, QWORD PTR [rdx + 8]");                // load the object pointer from the box payload
            emitter.instruction("jmp __rt_hash_sort_scalar_object_class_x86");  // resolve the object's comparison adapter
            emitter.label("__rt_hash_sort_scalar_inline_object_x86");
            hash_layout::emit_load_entry_field(emitter, "rax", "r10", EntryField::ValueLow);
            emitter.label("__rt_hash_sort_scalar_object_class_x86");
            elephc_memoir::values::object_storage::emit_load_class_id(emitter, "rax", "rax");
            abi::emit_symbol_address(emitter, "rsi", "_class_compare_ptrs");    // resolve the per-class comparison adapter table
            emitter.instruction("cmp QWORD PTR [rsi + rax * 8], 0");            // does this class declare a comparison adapter?
            emitter.instruction("je __rt_hash_sort_scalar_non_scalar_x86");     // unhandled object classes still fail closed
            emitter.label("__rt_hash_sort_scalar_next_x86");
            emitter.instruction("add r9, 1");                                   // advance to the next entry slot
            emitter.instruction("jmp __rt_hash_sort_scalar_scan_x86");          // continue validating runtime tags
            emitter.label("__rt_hash_sort_scalar_done_x86");
            emitter.instruction("ret");                                         // return the unchanged table pointer in rax
            emitter.label("__rt_hash_sort_scalar_non_scalar_x86");
            abi::emit_jump(emitter, "__rt_mixed_sort_reject_non_scalar_x86");   // share the explicit Mixed-sort fatal helper
        }
    }
}
