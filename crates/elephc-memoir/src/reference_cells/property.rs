//! Purpose:
//! Emits storage for managed reference cells that carry a property write guard.
//!
//! Called from:
//! - The compiler's typed-property reference emitter.
//!
//! Key details:
//! - The ordinary two-word payload precedes the guard's metadata.
//! - The host supplies the acceptance code and the static diagnostic suffix.

use crate::{abi, Arch, Emitter, TargetAbi};
use crate::layout::{x86_64_heap_kind_word, REFERENCE_CELL_HEAP_KIND, REFERENCE_CELL_SIZE};

/// Header flag distinguishing a typed-property reference cell from an ordinary owned cell.
///
/// The bit sits above the payload tag and the collector pin, so every existing cell kind and
/// ownership dispatcher keeps reading the same low bytes.
pub const CELL_FLAG: i64 = 1 << 19;

/// Offset of the static diagnostic suffix pointer in a typed-property reference cell.
pub const SUFFIX_PTR_OFFSET: usize = REFERENCE_CELL_SIZE;
/// Offset of the static diagnostic suffix length.
pub const SUFFIX_LEN_OFFSET: usize = SUFFIX_PTR_OFFSET + 8;
/// Offset of the runtime acceptance code selected from the property's declared type.
pub const TYPE_CODE_OFFSET: usize = SUFFIX_LEN_OFFSET + 8;
/// Offset of the zeroed final word in the metadata.
const RESERVED_OFFSET: usize = TYPE_CODE_OFFSET + 8;
/// Byte size of a typed-property reference cell: two payload words plus four metadata words.
const CELL_BYTES: i64 = (RESERVED_OFFSET + 8) as i64;

/// Allocates a managed cell carrying a typed-property reference-assignment guard.
///
/// Input: payload descriptor tag plus the static diagnostic suffix pointer/length and the
/// acceptance code selected by the property's declared type. The cell has the ordinary two
/// payload words at offset zero, so callees keep reading and writing it with the parameter's
/// native representation. The extra words are read only by the store guard.
pub fn emit_new(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.label_global("__rt_property_reference_cell_new");
    match emitter.arch() {
        Arch::AArch64 => {
            // -- preserve the descriptor and diagnostics, then allocate the owned cell --
            emitter.instruction("sub sp, sp, #48");                             // reserve metadata and linkage slots
            emitter.instruction("stp x29, x30, [sp, #32]");                     // preserve the caller frame and return address
            emitter.instruction("add x29, sp, #32");                            // establish the allocator frame
            emitter.instruction("stp x1, x2, [sp, #0]");                        // keep the static suffix pointer and length
            emitter.instruction("str x3, [sp, #16]");                           // keep the acceptance code
            emitter.instruction("str x0, [sp, #24]");                           // keep the payload descriptor tag
            abi::emit_load_int_immediate(emitter, "x0", CELL_BYTES); // request payload plus metadata words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the owned cell
            // -- publish the cell header, diagnostic metadata, and zeroed payload --
            emitter.instruction("ldr x9, [sp, #24]");                           // reload the payload descriptor tag
            emitter.instruction("lsl x9, x9, #8");                              // position the tag above the cell kind
            abi::emit_load_int_immediate(emitter, "x10", CELL_FLAG); // materialize the typed-property marker
            emitter.instruction("orr x9, x9, x10");                             // identify a typed-property reference cell
            emitter.instruction(&format!("mov x10, #{}", REFERENCE_CELL_HEAP_KIND)); // load the managed reference-cell heap kind
            emitter.instruction("orr x9, x9, x10");                             // preserve ordinary cell ownership dispatch
            emitter.instruction("str x9, [x0, #-8]");                           // publish the cell header
            emitter.instruction("ldr x10, [sp, #0]");                           // reload the static suffix pointer
            emitter.instruction("ldr x11, [sp, #8]");                           // reload the static suffix length
            emitter.instruction(&format!("str x10, [x0, #{SUFFIX_PTR_OFFSET}]")); // publish the diagnostic suffix pointer
            emitter.instruction(&format!("str x11, [x0, #{SUFFIX_LEN_OFFSET}]")); // publish the diagnostic suffix length
            emitter.instruction("ldr x10, [sp, #16]");                          // reload the property acceptance code
            emitter.instruction(&format!("str x10, [x0, #{TYPE_CODE_OFFSET}]")); // publish the guard selector
            emitter.instruction(&format!("str xzr, [x0, #{RESERVED_OFFSET}]")); // leave the metadata tail zeroed
            emitter.instruction("str xzr, [x0, #0]");                           // initialize the low payload word
            emitter.instruction("str xzr, [x0, #8]");                           // initialize the high payload word
            // -- restore the allocator frame --
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore the caller frame and return address
            emitter.instruction("add sp, sp, #48");                             // release the allocator frame
            abi::emit_return(emitter);
        }
        Arch::X86_64 => {
            // -- preserve the descriptor and diagnostics, then allocate the owned cell --
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the allocator frame
            emitter.instruction("sub rsp, 48");                                 // reserve metadata and linkage slots
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // keep the payload descriptor tag
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // keep the static suffix pointer
            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");               // keep the static suffix length
            emitter.instruction("mov QWORD PTR [rbp - 32], rcx");               // keep the property acceptance code
            abi::emit_load_int_immediate(emitter, "rax", CELL_BYTES); // request payload plus metadata words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the owned cell
            // -- publish the cell header, diagnostic metadata, and zeroed payload --
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload the payload descriptor tag
            emitter.instruction("shl r10, 8");                                  // position the tag above the cell kind
            abi::emit_load_int_immediate(emitter, "r11", CELL_FLAG); // materialize the typed-property marker
            emitter.instruction("or r10, r11");                                 // identify a typed-property reference cell
            emitter.instruction(&format!("mov r11, 0x{:x}", x86_64_heap_kind_word(REFERENCE_CELL_HEAP_KIND))); // load the managed reference-cell heap kind
            emitter.instruction("or r10, r11");                                 // preserve ordinary cell ownership dispatch
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // publish the cell header
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload the static suffix pointer
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the static suffix length
            emitter.instruction(&format!("mov QWORD PTR [rax + {SUFFIX_PTR_OFFSET}], r10")); // publish the diagnostic suffix pointer
            emitter.instruction(&format!("mov QWORD PTR [rax + {SUFFIX_LEN_OFFSET}], r11")); // publish the diagnostic suffix length
            emitter.instruction("mov r10, QWORD PTR [rbp - 32]");               // reload the property acceptance code
            emitter.instruction(&format!("mov QWORD PTR [rax + {TYPE_CODE_OFFSET}], r10")); // publish the guard selector
            emitter.instruction(&format!("mov QWORD PTR [rax + {RESERVED_OFFSET}], 0")); // leave the metadata tail zeroed
            emitter.instruction("mov QWORD PTR [rax], 0");                      // initialize the low payload word
            emitter.instruction("mov QWORD PTR [rax + 8], 0");                  // initialize the high payload word
            // -- restore the allocator frame --
            emitter.instruction("mov rsp, rbp");                                // release the allocator frame
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            abi::emit_return(emitter);
        }
    }
}

/// Stores the persisted string pair into one object slot, through a fresh reference cell when
/// the slot owns one.
///
/// `reload_object` must load the owner object pointer into `object_reg`; it runs after any
/// cell allocation, so it may read the helper's stack frame at its usual offsets.
pub fn emit_store_owned_string_property<E: TargetAbi>(
    emitter: &mut E,
    object_reg: &str,
    offset: usize,
    owns_reference: bool,
    reload_object: impl Fn(&mut E),
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    if !owns_reference {
        reload_object(emitter);
        abi::emit_store_to_address(emitter, ptr_reg, object_reg, offset);
        abi::emit_store_to_address(emitter, len_reg, object_reg, offset + 8);
        return;
    }
    // -- wrap the string in the object-owned reference cell its slot expects --
    abi::emit_push_reg(emitter, ptr_reg);
    abi::emit_push_reg(emitter, len_reg);
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter, 0),
        crate::layout::STRING_REFERENCE_PAYLOAD_TAG,
    );
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    let cell_reg = abi::int_result_reg(emitter);
    let value_ptr_reg = abi::secondary_scratch_reg(emitter);
    let value_len_reg = abi::tertiary_scratch_reg(emitter);
    abi::emit_pop_reg(emitter, value_len_reg);
    abi::emit_pop_reg(emitter, value_ptr_reg);
    abi::emit_store_to_address(emitter, value_ptr_reg, cell_reg, 0);
    abi::emit_store_to_address(emitter, value_len_reg, cell_reg, 8);
    reload_object(emitter);
    abi::emit_store_to_address(emitter, cell_reg, object_reg, offset);
    abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
}

/// Replaces the property cell and leaves its previous owner in the caller's retirement slot.
///
/// The caller owns the frame, with the property slot address at offset 8 and the displaced
/// owner at offset 16. The new cell is returned for type-source rebinding before retirement.
pub fn emit_replace_with_uninitialized(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, scratch, result, 0);
    abi::store_at_offset(emitter, scratch, 16);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [{scratch}, #-8]"));          // read the old managed cell's payload descriptor
            emitter.instruction("ubfx x0, x0, #8, #7");                         // preserve the declared native payload shape
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [{scratch} - 8]")); // read the old managed cell's payload descriptor
            emitter.instruction("shr rdi, 8");                                  // position the payload tag for cell allocation
            emitter.instruction("and edi, 127");                                // exclude heap-kind and collector flag bits
        }
    }
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::emit_load_int_immediate(emitter, scratch, emitter.uninitialized_property_sentinel());
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::emit_store_zero_to_address(emitter, result, 8);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 8);
}
