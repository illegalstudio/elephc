//! Purpose:
//! Counts PHP-visible owners of canonical references and their native storage cells.
//!
//! Called from:
//! - Eval reference queries and static setter destination capture.
//!
//! Key details:
//! - Collector and validation leases do not create PHP aliases.
//! - Several native aliases can share a cell that owns only one canonical wrapper.

use super::*;

/// Emits the shared wrapper predicate using the C argument register on every target.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_reference_is_shared");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("cbz x0, __rt_reference_unshared");                 // null cannot own a PHP reference
        emitter.instruction("ldr x9, [x0]");                                    // inspect the boxed runtime tag
        emitter.instruction("cmp x9, #7");                                      // persistent references use nested Mixed storage
        emitter.instruction("b.ne __rt_reference_unshared");                    // ordinary boxes have no writable reference
        emitter.instruction("ldr x9, [x0, #16]");                               // inspect the persistent reference flag
        emitter.instruction("cmp x9, #1");                                      // flag one identifies a PHP reference
        emitter.instruction("b.ne __rt_reference_unshared");                    // unrelated nested boxes remain ordinary
        emitter.instruction("ldr w9, [x0, #-12]");                              // read the wrapper's physical owner count
        emitter.instruction("ldr x10, [x0, #-8]");                              // inspect an artificial collector pin
        emitter.instruction("ubfx x10, x10, #18, #1");                          // isolate the collector-only owner bit
        emitter.instruction("sub w9, w9, w10");                                 // PHP sharing excludes collector pins
        super::subtract_validation_pins(
            emitter, "x0", "x9", "x10", "x11", "__rt_reference_shared",
        );
        emitter.instruction("cmp w9, #1");                                      // one owner separates during array COW
        emitter.instruction("cset x0, hi");                                     // another owner keeps the reference shared
        emitter.instruction("ret");                                             // return the sharing predicate
        emitter.label("__rt_reference_unshared");
        emitter.instruction("mov x0, #0");                                      // report a singleton or ordinary value
        emitter.instruction("ret");                                             // leave the borrowed wrapper untouched
    } else {
        emitter.instruction("test rdi, rdi");                                   // null cannot own a PHP reference
        emitter.instruction("jz __rt_reference_unshared");                      // skip an absent boxed value
        emitter.instruction("cmp QWORD PTR [rdi], 7");                          // persistent references use nested Mixed storage
        emitter.instruction("jne __rt_reference_unshared");                     // ordinary boxes have no writable reference
        emitter.instruction("cmp QWORD PTR [rdi + 16], 1");                     // inspect the persistent reference flag
        emitter.instruction("jne __rt_reference_unshared");                     // unrelated nested boxes remain ordinary
        emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                   // read the wrapper's physical owner count
        emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                    // inspect an artificial collector pin
        emitter.instruction("shr r10, 18");                                     // position the collector-only owner bit
        emitter.instruction("and r10d, 1");                                     // isolate the collector pin
        emitter.instruction("sub eax, r10d");                                   // PHP sharing excludes collector pins
        super::subtract_validation_pins(
            emitter, "rdi", "rax", "r10", "r11", "__rt_reference_shared",
        );
        emitter.instruction("cmp eax, 1");                                      // one owner separates during array COW
        emitter.instruction("seta al");                                         // another owner keeps the reference shared
        emitter.instruction("movzx eax, al");                                   // normalize the sharing predicate
        emitter.instruction("ret");                                             // leave the borrowed wrapper untouched
        emitter.label("__rt_reference_unshared");
        emitter.instruction("xor eax, eax");                                    // report a singleton or ordinary value
        emitter.instruction("ret");                                             // return the sharing predicate
    }
    emit_cell(emitter);
}

/// Detects both direct wrapper aliases and aliases that share one native cell owner.
fn emit_cell(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let cell = abi::secondary_scratch_reg(emitter);
    let count = if emitter.target.arch == Arch::AArch64 { "x11" } else { "r11" };
    let done = "__rt_reference_cell_is_shared_done";
    emitter.label_global("__rt_reference_cell_is_shared");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_reg_move(emitter, cell, abi::int_arg_reg_name(emitter.target, 0));
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldur w9, [{cell}, #-12]"));           // native aliases retain their managed storage cell
            emitter.instruction(&format!("ldur {count}, [{cell}, #-8]"));       // inspect the collector's artificial owner bit
            emitter.instruction(&format!("ubfx {count}, {count}, #18, #1"));    // isolate the collector lease
            emitter.instruction(&format!("sub x9, x9, {count}"));               // count only language-visible native owners
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov ecx, DWORD PTR [{cell} - 12]"));  // native aliases retain their managed storage cell
            emitter.instruction(&format!("mov {count}, QWORD PTR [{cell} - 8]"));// inspect the collector's artificial owner bit
            emitter.instruction(&format!("shr {count}, 18"));                   // position the collector lease
            emitter.instruction(&format!("and {count}, 1"));                    // isolate that one artificial owner
            emitter.instruction(&format!("sub rcx, {count}"));                  // count only language-visible native owners
        }
    }
    let owners = if emitter.target.arch == Arch::AArch64 { "x9" } else { "rcx" };
    super::subtract_validation_pins(emitter, cell, owners, result, count, "static_cell_sharing");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x9, #1");                                  // a second cell owner is a live PHP alias
            emitter.instruction("cset x0, hi");                                 // preserve the normalized sharing result
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rcx, 1");                                  // a second cell owner is a live PHP alias
            emitter.instruction("seta al");                                     // preserve the normalized sharing result
            emitter.instruction("movzx eax, al");                               // clear the remaining result bits
        }
    }
    abi::emit_branch_if_int_result_nonzero(emitter, done);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_reference_cell_canonical");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), result);
    abi::emit_call_label(emitter, "__rt_reference_is_shared");
    emitter.label(done);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}
