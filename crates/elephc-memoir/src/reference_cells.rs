//! Purpose:
//! Emits managed reference-cell allocation, ownership lookup, cloning and release.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

use crate::{abi, Arch, Emitter};
use crate::layout::{x86_64_heap_kind_word, REFERENCE_CELL_HEAP_KIND};
pub mod argument_pins;
pub mod property;
pub mod validation_pins;
use validation_pins::subtract as subtract_validation_pins;

/// Allocates a zeroed two-word cell; C argument zero supplies its payload descriptor tag.
pub fn emit_new(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_new");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::emit_load_int_immediate(emitter, result, crate::layout::REFERENCE_CELL_SIZE as i64);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::load_at_offset(emitter, scratch, 8);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("lsl {scratch}, {scratch}, #8"));      // encode the stored value type above the cell kind
            emitter.instruction(&format!("orr {scratch}, {scratch}, #{REFERENCE_CELL_HEAP_KIND}")); // identify an independently owned reference cell
            emitter.instruction(&format!("str {scratch}, [x0, #-8]"));          // publish the cell shape in the uniform header
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("shl {scratch}, 8"));                  // encode the stored value type above the cell kind
            abi::emit_load_int_immediate(
                emitter,
                "r11",
                x86_64_heap_kind_word(REFERENCE_CELL_HEAP_KIND) as i64,
            );
            emitter.instruction(&format!("or {scratch}, r11"));                 // preserve the managed-heap marker with the typed cell kind
            emitter.instruction(&format!("mov QWORD PTR [rax - 8], {scratch}")); // publish the cell shape in the uniform header
        }
    }
    abi::emit_store_zero_to_address(emitter, result, 0);
    abi::emit_store_zero_to_address(emitter, result, 8);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Releases the typed payload of a final-owner cell inside the existing cleanup boundary.
pub fn emit_value_release(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.label_global("__rt_reference_cell_value_release");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #-8]");                           // read the stored value type before replacing the cell pointer
            emitter.instruction("ubfx x9, x9, #8, #7");                         // discard heap kind and collector flags
            emitter.instruction("ldr x0, [x0]");                                // load the cell's low-word payload for unary release
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rax - 8]");                 // read the stored value type before replacing the cell pointer
            emitter.instruction("shr r9, 8");                                   // move the payload descriptor to the low bits
            emitter.instruction("and r9d, 0x7f");                               // discard heap kind and collector flags
            emitter.instruction("mov rax, QWORD PTR [rax]");                    // load the cell's low-word payload for unary release
        }
    }
    emit_payload_dispatch(emitter, "__rt_reference_cell_value_heap", "__rt_reference_cell_value_callable");
    abi::emit_return(emitter);
    emitter.label("__rt_reference_cell_value_heap");
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_reference_cell_value_callable");
    abi::emit_jump(emitter, "__rt_callable_descriptor_release");
}

/// Branches to assembler-local stubs by payload tag, preserving the low-word result register.
fn emit_payload_dispatch(emitter: &mut impl Emitter, ordinary: &str, callable: &str) {
    for tag in [1, 4, 5, 6, 7, 10] {
        let target = if tag == 10 { callable } else { ordinary };
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp x9, #{tag}"));                // select only payload shapes that own heap storage
                emitter.instruction(&format!("b.eq {target}"));                 // delegate ownership to the concrete payload helper
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp r9, {tag}"));                 // select only payload shapes that own heap storage
                emitter.instruction(&format!("je {target}"));                   // delegate ownership to the concrete payload helper
            }
        }
    }
}

/// Adapts legacy unary cell release and collector-forced retirement to exception-safe cleanup.
pub fn emit_release(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.label_global("__rt_reference_cell_free_deep");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #1");                                  // collapse unreachable graph owners for final sweep retirement
            emitter.instruction("str w9, [x0, #-12]");                          // let the ordinary final-owner path release this cell once
        }
        Arch::X86_64 => {
            emitter.instruction("mov DWORD PTR [rax - 12], 1");                 // let the ordinary final-owner path release this cell once
        }
    }
    emitter.label_global("__rt_reference_cell_release");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter, 1), abi::int_result_reg(emitter));
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter, 0), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter, 2), 0);
    abi::emit_jump(emitter, "__rt_local_ref_cell_release");
}

/// Shares aliased cells and copies singleton cells with independent boxed value storage.
pub fn emit_clone(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_clone");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_cell_clone_done");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr w9, [x0, #-12]");                          // distinguish a physical singleton cell from a live PHP reference
            emitter.instruction("ldr x10, [x0, #-8]");                          // inspect transient ownership held by the cycle collector
            emitter.instruction("ubfx x10, x10, #18, #1");                      // isolate the artificial destructor pin
            emitter.instruction("sub w9, w9, w10");                             // PHP alias sharing excludes collector-only ownership
            subtract_validation_pins(emitter, "x0", "x9", "x10", "x11", "__rt_reference_cell_clone_cell");
            emitter.instruction("cmp w9, #1");                                  // singleton references separate when their object is cloned
            emitter.instruction("b.hi __rt_reference_cell_clone_shared");       // preserve sharing while another alias owns the reference
        }
        Arch::X86_64 => {
            emitter.instruction("mov ecx, DWORD PTR [rax - 12]");               // read the cell's physical owner count
            emitter.instruction("mov r9, QWORD PTR [rax - 8]");                 // inspect transient ownership held by the cycle collector
            emitter.instruction("shr r9, 18");                                  // position the artificial destructor pin
            emitter.instruction("and r9d, 1");                                  // isolate collector-only ownership
            emitter.instruction("sub ecx, r9d");                                // PHP alias sharing excludes collector pins
            subtract_validation_pins(emitter, "rax", "rcx", "r10", "r11", "__rt_reference_cell_clone_cell");
            emitter.instruction("cmp ecx, 1");                                  // singleton references separate when their object is cloned
            emitter.instruction("ja __rt_reference_cell_clone_shared");         // preserve sharing while another alias owns the reference
        }
    }
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, result, 8);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #-8]");                           // read the source cell's stored value shape
            emitter.instruction("ubfx x0, x9, #8, #7");                         // pass the clean payload tag to cell allocation
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rax - 8]");                // read the source cell's stored value shape
            emitter.instruction("shr rdi, 8");                                  // position the payload type for the allocator
            emitter.instruction("and edi, 0x7f");                               // exclude collector flags and the heap marker
        }
    }
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 24);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, scratch, 8);
    let word = abi::tertiary_scratch_reg(emitter);
    for offset in [0, 8] {
        abi::emit_load_from_address(emitter, word, scratch, offset);
        abi::emit_store_to_address(emitter, word, result, offset);
    }
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_cell_clone_finish");
    abi::emit_load_int_immediate(emitter, scratch, emitter.uninitialized_property_sentinel());
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", result, scratch));       // keep an uninitialized property sentinel out of payload retention
            emitter.instruction("b.eq __rt_reference_cell_clone_finish");       // cloned uninitialized reference cells have no live payload owner
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", result, scratch));       // keep an uninitialized property sentinel out of payload retention
            emitter.instruction("je __rt_reference_cell_clone_finish");         // cloned uninitialized reference cells have no live payload owner
        }
    }
    let tag = match emitter.arch() { Arch::AArch64 => "x9", Arch::X86_64 => "r9" };
    abi::load_at_offset(emitter, tag, 24);
    emit_singleton_mixed_clone_branch(emitter);
    emit_payload_dispatch(emitter, "__rt_reference_cell_clone_retain", "__rt_reference_cell_clone_retain");
    abi::emit_jump(emitter, "__rt_reference_cell_clone_finish");
    emitter.label("__rt_reference_cell_clone_retain");
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, "__rt_reference_cell_clone_finish");
    emitter.label("__rt_reference_cell_clone_mixed");
    abi::emit_call_label(emitter, "__rt_mixed_clone");
    abi::emit_jump(emitter, "__rt_reference_cell_clone_store");
    emitter.label("__rt_reference_cell_clone_reference");
    abi::emit_call_label(emitter, "__rt_reference_new");
    emitter.label("__rt_reference_cell_clone_store");
    abi::load_at_offset(emitter, scratch, 16);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    emitter.label("__rt_reference_cell_clone_finish");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
    // Keep the frameless null return separate from the restored framed path
    // so instruction-level ABI audits never merge distinct stack depths.
    emitter.label("__rt_reference_cell_clone_done");
    abi::emit_return(emitter);
    emitter.label("__rt_reference_cell_clone_shared");
    abi::emit_jump(emitter, "__rt_incref");
}

/// Separates ordinary Mixed zvals while preserving an independently shared PHP reference wrapper.
fn emit_singleton_mixed_clone_branch(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("cmp x9, #7");                                  // only boxed or iterable payloads can need zval separation
            emitter.instruction("b.ne __rt_reference_cell_clone_dispatch");     // preserve the ordinary payload-specific ownership path
            emitter.instruction("ldrb w10, [x0, #-8]");                         // distinguish boxed Mixed storage from a raw iterable container
            emitter.instruction("cmp w10, #5");                                 // heap kind five owns a boxed PHP value
            emitter.instruction("b.ne __rt_reference_cell_clone_dispatch");     // raw iterable payloads retain their container identity
            emitter.instruction("ldr x10, [x0]");                               // inspect whether the boxed value itself carries reference identity
            emitter.instruction("cmp x10, #7");                                 // persistent PHP reference wrappers use the nested Mixed tag
            emitter.instruction("b.ne __rt_reference_cell_clone_mixed");        // ordinary boxed values need an independent zval
            emitter.instruction("ldr x10, [x0, #16]");                          // inspect the persistent-reference marker
            emitter.instruction("cmp x10, #1");                                 // ordinary nested Mixed boxes have no reference identity
            emitter.instruction("b.ne __rt_reference_cell_clone_mixed");        // clone an ordinary nested boxed value
            emitter.instruction("ldr w10, [x0, #-12]");                         // count owners of the inner persistent reference
            emitter.instruction("ldr x11, [x0, #-8]");                          // inspect the wrapper's collector-only ownership
            emitter.instruction("ubfx x11, x11, #18, #1");                      // isolate the artificial destructor pin
            emitter.instruction("sub w10, w10, w11");                           // only PHP-visible wrapper aliases preserve sharing
            subtract_validation_pins(emitter, "x0", "x10", "x11", "x12", "__rt_reference_cell_clone_wrapper");
            emitter.instruction("cmp w10, #1");                                 // a singleton wrapper can separate like an ordinary value
            emitter.instruction("b.hi __rt_reference_cell_clone_retain");       // preserve references shared outside the native cell
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r9, 7");                                   // only boxed or iterable payloads can need zval separation
            emitter.instruction("jne __rt_reference_cell_clone_dispatch");      // preserve payload-specific ownership for other tags
            emitter.instruction("cmp BYTE PTR [rax - 8], 5");                   // distinguish boxed Mixed storage from a raw iterable container
            emitter.instruction("jne __rt_reference_cell_clone_dispatch");      // raw iterable payloads retain their container identity
            emitter.instruction("cmp QWORD PTR [rax], 7");                      // recognize a potentially persistent reference wrapper
            emitter.instruction("jne __rt_reference_cell_clone_mixed");         // ordinary boxed values need an independent zval
            emitter.instruction("cmp QWORD PTR [rax + 16], 1");                 // recognize the persistent-reference ABI marker
            emitter.instruction("jne __rt_reference_cell_clone_mixed");         // clone an ordinary nested boxed value
            emitter.instruction("mov ecx, DWORD PTR [rax - 12]");               // count owners of the inner persistent reference
            emitter.instruction("mov r10, QWORD PTR [rax - 8]");                // inspect the wrapper's collector-only ownership
            emitter.instruction("shr r10, 18");                                 // position the artificial destructor pin
            emitter.instruction("and r10d, 1");                                 // isolate collector-only ownership
            emitter.instruction("sub ecx, r10d");                               // only PHP-visible wrapper aliases preserve sharing
            subtract_validation_pins(emitter, "rax", "rcx", "r10", "r11", "__rt_reference_cell_clone_wrapper");
            emitter.instruction("cmp ecx, 1");                                  // singleton references separate despite a collector snapshot
            emitter.instruction("ja __rt_reference_cell_clone_retain");         // preserve references shared outside the native cell
        }
    }
    abi::emit_jump(emitter, "__rt_reference_cell_clone_reference");
    emitter.label("__rt_reference_cell_clone_dispatch");
}

/// Finds an exact live cell in constant time, rejecting borrowed and interior addresses.
pub fn emit_owner_lookup(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.label_global("__rt_reference_cell_owner");
    abi::emit_frame_prologue(emitter, 16);
    // -- reuse allocator-owned identity validation before inspecting any cell header --
    abi::emit_call_label(emitter, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_cell_owner_done");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldrb w9, [x0, #-8]");                          // inspect only an exact live allocation's physical representation
            emitter.instruction(&format!("cmp w9, #{REFERENCE_CELL_HEAP_KIND}")); // only managed reference cells can transfer this owner
            emitter.instruction("b.eq __rt_reference_cell_owner_done");         // preserve the exact cell address without acquiring a reference
            emitter.instruction("mov x0, xzr");                                 // ordinary allocations and borrowed storage have no cell owner
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp BYTE PTR [rax - 8], {REFERENCE_CELL_HEAP_KIND}")); // only managed reference cells can transfer this owner
            emitter.instruction("je __rt_reference_cell_owner_done");           // preserve the validated cell identity without changing ownership
            emitter.instruction("xor eax, eax");                                // reject other live allocations as reference-cell owners
        }
    }
    emitter.label("__rt_reference_cell_owner_done");
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

pub mod unmanaged_borrows;
pub use unmanaged_borrows::emit_unmanaged_borrow_lookup;

pub mod local_release;

pub mod parameter_bindings;

pub mod mixed_payload;

pub mod static_cells;

pub mod mixed_symbol;

pub mod validated_commit;

pub mod eval_writeback;

pub mod staged_pin;

/// Allocates temporary two-word reference-argument storage for an already staged value.
pub fn emit_allocate_argument_pair(emitter: &mut impl crate::Emitter, destination: &str) {
    let result = emitter.int_result_reg();
    emitter.emit_load_int_immediate(result, crate::layout::REFERENCE_CELL_SIZE as i64);
    emitter.emit_call_label("__rt_heap_alloc");
    emitter.emit_push_reg(result);
    emitter.emit_pop_reg(destination);
}

pub mod canonical;

pub mod magic_set_guard;
