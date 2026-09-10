//! Purpose:
//! Defines owned reference-cell storage and emits its allocation, cloning and release helpers.
//!
//! Called from:
//! - Object property lowering, managed runtime emission and cycle collection.
//!
//! Key details:
//! - The two payload words stay compatible with borrowed reference addresses.
//! - Heap kind 7 identifies an owned cell; bits 8 through 14 describe its payload.
//! - Singleton cells separate on clone, while cells with live aliases remain shared.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::types::PhpType;

/// Returns the descriptor tag for the actual stored property representation.
pub(crate) fn payload_tag(php_type: &PhpType) -> i64 {
    match php_type.codegen_repr() {
        PhpType::Str => 1,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => 7,
        PhpType::Callable => 10,
        _ => 0,
    }
}

/// Emits managed reference cells with the same ownership rules on every supported target.
pub(super) fn emit_reference_cells(emitter: &mut Emitter) {
    emit_new(emitter);
    emit_value_release(emitter);
    emit_release(emitter);
    emit_clone(emitter);
    emit_owner_lookup(emitter);
}

/// Allocates a zeroed two-word cell; C argument zero supplies its payload descriptor tag.
fn emit_new(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_new");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_load_int_immediate(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::load_at_offset(emitter, scratch, 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("lsl {scratch}, {scratch}, #8"));      // encode the stored value type above the cell kind
            emitter.instruction(&format!("orr {scratch}, {scratch}, #7"));      // identify an independently owned reference cell
            emitter.instruction(&format!("str {scratch}, [x0, #-8]"));          // publish the cell shape in the uniform header
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("shl {scratch}, 8"));                  // encode the stored value type above the cell kind
            abi::emit_load_int_immediate(emitter, "r11", crate::codegen_support::sentinels::x86_64_heap_kind_word(7) as i64);
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
fn emit_value_release(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_reference_cell_value_release");
    match emitter.target.arch {
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
fn emit_payload_dispatch(emitter: &mut Emitter, ordinary: &str, callable: &str) {
    for tag in [1, 4, 5, 6, 7, 10] {
        let target = if tag == 10 { callable } else { ordinary };
        match emitter.target.arch {
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
fn emit_release(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_reference_cell_free_deep");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #1");                                  // collapse unreachable graph owners for final sweep retirement
            emitter.instruction("str w9, [x0, #-12]");                          // let the ordinary final-owner path release this cell once
        }
        Arch::X86_64 => {
            emitter.instruction("mov DWORD PTR [rax - 12], 1");                 // let the ordinary final-owner path release this cell once
        }
    }
    emitter.label_global("__rt_reference_cell_release");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), abi::int_result_reg(emitter));
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 2), 0);
    abi::emit_jump(emitter, "__rt_local_ref_cell_release");
}

/// Shares aliased cells but copies singleton cells and retains their contained value.
fn emit_clone(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_reference_cell_clone");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_cell_clone_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr w9, [x0, #-12]");                          // distinguish a physical singleton cell from a live PHP reference
            emitter.instruction("ldr x10, [x0, #-8]");                          // inspect transient ownership held by the cycle collector
            emitter.instruction("ubfx x10, x10, #18, #1");                      // isolate the artificial destructor pin
            emitter.instruction("sub w9, w9, w10");                             // PHP alias sharing excludes collector-only ownership
            emitter.instruction("cmp w9, #1");                                  // singleton references separate when their object is cloned
            emitter.instruction("b.hi __rt_reference_cell_clone_shared");       // preserve sharing while another alias owns the reference
        }
        Arch::X86_64 => {
            emitter.instruction("mov ecx, DWORD PTR [rax - 12]");               // read the cell's physical owner count
            emitter.instruction("mov r9, QWORD PTR [rax - 8]");                 // inspect transient ownership held by the cycle collector
            emitter.instruction("shr r9, 18");                                  // position the artificial destructor pin
            emitter.instruction("and r9d, 1");                                  // isolate collector-only ownership
            emitter.instruction("sub ecx, r9d");                                // PHP alias sharing excludes collector pins
            emitter.instruction("cmp ecx, 1");                                  // singleton references separate when their object is cloned
            emitter.instruction("ja __rt_reference_cell_clone_shared");         // preserve sharing while another alias owns the reference
        }
    }
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, result, 8);
    match emitter.target.arch {
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
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 24);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, scratch, 8);
    let word = abi::tertiary_scratch_reg(emitter);
    for offset in [0, 8] {
        abi::emit_load_from_address(emitter, word, scratch, offset);
        abi::emit_store_to_address(emitter, word, result, offset);
    }
    abi::emit_load_from_address(emitter, result, result, 0);
    let tag = match emitter.target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r9" };
    abi::load_at_offset(emitter, tag, 24);
    emit_payload_dispatch(emitter, "__rt_reference_cell_clone_retain", "__rt_reference_cell_clone_retain");
    abi::emit_jump(emitter, "__rt_reference_cell_clone_finish");
    emitter.label("__rt_reference_cell_clone_retain");
    abi::emit_call_label(emitter, "__rt_incref");
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


/// Finds an exact live cell allocation, rejecting borrowed frame and array-interior addresses.
fn emit_owner_lookup(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_reference_cell_owner");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_heap_buf");
            emitter.instruction("add x10, x9, #16");                            // require room for a complete allocation header
            emitter.instruction("cmp x0, x10");                                 // reject null, scalar and frame addresses below the managed payload range
            emitter.instruction("b.lo __rt_reference_cell_owner_none");         // borrowed addresses have no independent cell owner
            abi::emit_load_symbol_to_reg(emitter, "x10", "_heap_off", 0);
            abi::emit_symbol_address(emitter, "x9", "_heap_buf");
            emitter.instruction("add x10, x9, x10");                            // bound the scan by the current managed heap extent
            emitter.instruction("cmp x0, x10");                                 // reject pointers outside the allocated heap window
            emitter.instruction("b.hs __rt_reference_cell_owner_none");         // never inspect foreign or frame storage as a heap header
            emitter.instruction("ldrb w11, [x0, #-8]");                         // cheaply reject ordinary values and raw fallback cells
            emitter.instruction("cmp w11, #7");                                 // only managed reference cells can transfer this owner
            emitter.instruction("b.ne __rt_reference_cell_owner_none");         // borrowed cells do not participate in managed cell cleanup
            emitter.instruction("ldr w11, [x0, #-12]");                         // reject previously retired cell allocations
            emitter.instruction("cbz w11, __rt_reference_cell_owner_none");     // freed storage cannot transfer an owner
            emitter.label("__rt_reference_cell_owner_scan");
            emitter.instruction("add x11, x9, #16");                            // compute this allocation's exact payload address
            emitter.instruction("cmp x11, x0");                                 // distinguish an allocation from a forged-looking interior word
            emitter.instruction("b.eq __rt_reference_cell_owner_done");         // the input is a live managed reference-cell allocation
            emitter.instruction("b.hi __rt_reference_cell_owner_none");         // the pointer lies inside an earlier allocation
            emitter.instruction("ldr w12, [x9]");                               // load this block's allocator-owned payload extent
            emitter.instruction("add x9, x11, x12");                            // advance to the next uniform allocation header
            emitter.instruction("b __rt_reference_cell_owner_scan");            // validate the candidate against allocation boundaries
            emitter.label("__rt_reference_cell_owner_none");
            emitter.instruction("mov x0, xzr");                                 // report that borrowed storage has no transferable cell owner
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r8", "_heap_buf");
            emitter.instruction("lea r9, [r8 + 16]");                           // require room for a complete allocation header
            emitter.instruction("cmp rax, r9");                                 // reject null, scalar and frame addresses below the managed payload range
            emitter.instruction("jb __rt_reference_cell_owner_none");           // borrowed addresses have no independent cell owner
            abi::emit_load_symbol_to_reg(emitter, "r9", "_heap_off", 0);
            emitter.instruction("add r9, r8");                                  // bound the scan by the current managed heap extent
            emitter.instruction("cmp rax, r9");                                 // reject pointers outside the allocated heap window
            emitter.instruction("jae __rt_reference_cell_owner_none");          // never inspect foreign or frame storage as a heap header
            emitter.instruction("cmp BYTE PTR [rax - 8], 7");                   // cheaply reject ordinary values and raw fallback cells
            emitter.instruction("jne __rt_reference_cell_owner_none");          // borrowed cells do not participate in managed cell cleanup
            emitter.instruction("cmp DWORD PTR [rax - 12], 0");                 // reject previously retired cell allocations
            emitter.instruction("je __rt_reference_cell_owner_none");           // freed storage cannot transfer an owner
            emitter.label("__rt_reference_cell_owner_scan");
            emitter.instruction("lea r10, [r8 + 16]");                          // compute this allocation's exact payload address
            emitter.instruction("cmp r10, rax");                                // distinguish an allocation from a forged-looking interior word
            emitter.instruction("je __rt_reference_cell_owner_done");           // the input is a live managed reference-cell allocation
            emitter.instruction("ja __rt_reference_cell_owner_none");           // the pointer lies inside an earlier allocation
            emitter.instruction("mov r11d, DWORD PTR [r8]");                    // load this block's allocator-owned payload extent
            emitter.instruction("lea r8, [r10 + r11]");                         // advance to the next uniform allocation header
            emitter.instruction("jmp __rt_reference_cell_owner_scan");          // validate the candidate against allocation boundaries
            emitter.label("__rt_reference_cell_owner_none");
            emitter.instruction("xor eax, eax");                                // report that borrowed storage has no transferable cell owner
        }
    }
    emitter.label("__rt_reference_cell_owner_done");
    abi::emit_return(emitter);

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The framed clone returns before the separate frameless null path on every supported ABI.
    #[test]
    fn reference_cell_clone_has_distinct_framed_and_frameless_returns() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_clone(&mut emitter);
            let assembly = emitter.output();
            let (framed, early) = assembly.split_once("__rt_reference_cell_clone_done:").unwrap();
            assert!(framed.trim_end().ends_with("ret"), "{name}: framed path must return before the early-out label");
            assert!(early.trim_start().starts_with("ret"), "{name}: null path must not restore a frame");
        }
    }

    /// Mach-O conditional branches require local labels even when a helper is emitted in the same object.
    #[test]
    fn reference_cell_dispatch_uses_local_stubs_before_global_tail_calls() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_reference_cells(&mut emitter);
            super::super::arrays::emit_decref_any(&mut emitter);
            let assembly = emitter.output();
            for entry in ["__rt_decref_any", "__rt_callable_descriptor_release",
                "__rt_incref", "__rt_reference_cell_release"] {
                for line in assembly.lines() {
                    let words = line.split_whitespace().collect::<Vec<_>>();
                    if words.len() == 2 && words[1] == entry {
                        assert!(!words[0].starts_with("b.") && !matches!(words[0], "je" | "ja"),
                            "{name}: global conditional branch {line}");
                    }
                }
            }
            for stub in ["__rt_reference_cell_value_heap:", "__rt_reference_cell_value_callable:",
                "__rt_reference_cell_clone_shared:", "__rt_decref_any_reference:"] {
                assert!(assembly.contains(stub), "{name}: missing local dispatch stub {stub}");
            }
        }
    }

    /// All supported emitters expose typed cell allocation, release and graph traversal together.
    #[test]
    fn owned_reference_cells_are_complete_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_reference_cells(&mut emitter);
            super::super::exceptions::emit_local_ref_cell_release(&mut emitter);
            super::super::arrays::emit_decref_any(&mut emitter);
            super::super::arrays::emit_gc_collect_cycles(&mut emitter);
            super::super::arrays::emit_gc_mark_reachable(&mut emitter);
            super::super::arrays::emit_object_free_deep(&mut emitter, crate::codegen_support::RuntimeFeatures::all());
            let assembly = emitter.output();
            for entry in ["__rt_reference_cell_new", "__rt_reference_cell_clone",
                "__rt_reference_cell_value_release", "__rt_reference_cell_free_deep",
                "__rt_local_ref_cell_release_managed", "__rt_gc_collect_cycles_count_reference",
                "__rt_gc_collect_cycles_free_reference", "__rt_gc_mark_reachable_reference"] {
                assert!(assembly.contains(&format!("{entry}:")), "{name}: missing {entry}");
            }
            let (alias, retire, singleton) = if name == "linux-x86_64" {
                ("je __rt_decref_any_reference", "cmp r8, 11", "ja __rt_reference_cell_clone_shared")
            } else {
                ("b.eq __rt_decref_any_reference", "cmp x15, #11", "b.hi __rt_reference_cell_clone_shared")
            };
            assert!(assembly.contains(alias) && assembly.contains(retire), "{name}: object owners use cell release");
            assert!(assembly.contains(singleton), "{name}: live aliases survive object cloning");
        }
    }

    /// Payload tags describe backend storage, not the PHP array or nullable spelling alone.
    #[test]
    fn owned_reference_cell_tags_follow_storage_representation() {
        assert_eq!(payload_tag(&PhpType::Int), 0);
        assert_eq!(payload_tag(&PhpType::Str), 1);
        assert_eq!(payload_tag(&PhpType::Array(Box::new(PhpType::Int))), 4);
        assert_eq!(payload_tag(&PhpType::Mixed), 7);
        assert_eq!(payload_tag(&PhpType::Callable), 10);
    }
}
