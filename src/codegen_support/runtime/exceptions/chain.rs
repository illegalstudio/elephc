//! Purpose:
//! Appends an owned previous exception without overwriting existing links or creating cycles.
//!
//! Called from:
//! - Protected cycle-collector destructor callbacks.
//!
//! Key details:
//! - The new exception is borrowed and rooted by the caller; the old exception owner is consumed.
//! - Both chains are traversed through the concrete-layout previous reader.

use crate::codegen_support::{
    abi,
    cdylib::{
        BOUNDARY_STATUS, PHPEXT_REPLACEMENT_OWNER_BASE, PHPEXT_REPLACEMENT_OWNER_COUNT,
        REPLACEMENT_OWNER_STORAGE, STATUS_REPLACEMENT_LIMIT, emit_replacement_owners_address,
    },
    emit::Emitter,
    platform::Arch,
};

#[cfg(test)]
use crate::codegen_support::cdylib::PHPEXT_REPLACEMENT_OWNERS;

/// Emits exception chaining with raw-object and boxed-property ownership on every target.
pub fn emit_exception_chain(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Consumes the ARM64 old exception in x1, appending it to the borrowed exception in x0.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_exception_chain");
    // -- preserve chain cursors and the incoming owner across metadata lookups --
    emitter.instruction("sub sp, sp, #64");                                     // reserve cursors, a property slot, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller across chain traversal
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame
    emitter.instruction("str x0, [sp]");                                        // start at the new exception whose caller roots the chain
    emitter.instruction("str x1, [sp, #8]");                                    // own the old exception until a link adopts it or it is released
    if emitter.phpext_output {
        emitter.instruction("cbnz x1, __rt_exception_chain_has_pending");       // ordinary native pending owners use the normal chain path
        emitter.instruction("cbz x0, __rt_exception_chain_return");             // only a native replacement can own a displaced host escape
        emit_aarch64_replacement_sweep(emitter);
        emitter.instruction("ldr x0, [sp]");                                    // recover the replacement clobbered by the stale-owner callback
        REPLACEMENT_OWNER_STORAGE.emit_check_capacity(emitter, "__rt_exception_chain_replacement_overflow");
        emitter.instruction("str x0, [sp, #40]");                               // seed the replacement's intrinsic previous-chain tail
        emitter.label("__rt_exception_chain_replacement_tail");
        emitter.instruction("bl __rt_throwable_previous");                      // borrow the next constructor/finally predecessor
        emitter.instruction("cbz x0, __rt_exception_chain_replacement_ready");  // stop after the intrinsic previous-chain tail
        emitter.instruction("str x0, [sp, #40]");                               // remember the exact point before later outer chains append
        emitter.instruction("b __rt_exception_chain_replacement_tail");         // continue walking the intrinsic previous chain
        emitter.label("__rt_exception_chain_replacement_ready");
        emitter.instruction("ldr x0, [sp]");                                    // retain the replacement identity through boundary copying
        REPLACEMENT_OWNER_STORAGE.emit_pin(emitter, emit_replacement_owners_address);
        emitter.instruction("ldr x0, [sp]");                                    // C receives the replacement root and its original chain tail
        emitter.instruction("ldr x1, [sp, #40]");                               // pass the original previous-chain tail to the adapter
        emitter.bl_c("elephc_php_finally_replacement");
        emitter.instruction("b __rt_exception_chain_return");                   // the null host marker owns no native reference
        emitter.label("__rt_exception_chain_replacement_overflow");
        emitter.instruction(&format!("mov x9, #{STATUS_REPLACEMENT_LIMIT}"));   // expose the bounded replacement-pin failure precisely
        abi::emit_store_reg_to_symbol(emitter, "x9", BOUNDARY_STATUS, 0);
        emitter.instruction("b __rt_exception_chain_return");                   // preserve the boundary failure for the export handler
        emitter.label("__rt_exception_chain_has_pending");
    } else {
        emitter.instruction("cbz x1, __rt_exception_chain_return");             // there is no previous owner to attach
    }
    emitter.instruction("ldr x9, [x1]");                                        // internal force-close markers are never PHP-visible previous exceptions
    emitter.instruction("cmn x9, #3");                                          // recognize the reserved forced-unwind class identity
    emitter.instruction("b.eq __rt_exception_chain_release");                   // consume the marker instead of attaching it to a real Throwable
    if emitter.phpext_output {
        emitter.instruction("cbnz x0, __rt_exception_chain_scan");              // native exceptions can adopt the older native owner directly
        emitter.instruction("ldr x0, [sp, #8]");                                // expose the native owner before the host escape consumes it
        emitter.bl_c("elephc_php_previous_exception");                           // copy the previous Throwable into malloc-owned transport
        emitter.instruction("ldr x0, [sp, #8]");                                // begin walking every previous link owned by that copied root
        emitter.label("__rt_exception_chain_copy_previous");
        emitter.instruction("bl __rt_throwable_previous");                      // borrow the next native previous link without retaining it
        emitter.instruction("cbz x0, __rt_exception_chain_release");            // finish once the full native chain has been copied
        emitter.instruction("str x0, [sp, #40]");                               // preserve the current node across the C callback
        emitter.bl_c("elephc_php_exception_previous");                           // append this node to the active malloc-owned copied chain
        emitter.instruction("ldr x0, [sp, #40]");                               // continue traversal from the node just copied
        emitter.instruction("b __rt_exception_chain_copy_previous");            // preserve every constructor and finally-chain link
    }
    emitter.label("__rt_exception_chain_scan");
    emitter.instruction("ldr x0, [sp, #8]");                                    // restart the old chain for this new-chain node
    emitter.label("__rt_exception_chain_ancestors");
    emitter.instruction("ldr x9, [sp]");                                        // borrow the current node of the new chain
    emitter.instruction("cmp x0, x9");                                          // does joining the chains repeat this object identity?
    emitter.instruction("b.eq __rt_exception_chain_release");                   // consume a redundant owner without creating a cycle
    emitter.instruction("bl __rt_throwable_previous");                          // follow the old chain without acquiring an extra owner
    emitter.instruction("cbnz x0, __rt_exception_chain_ancestors");             // inspect every old-chain ancestor for an overlap
    emitter.instruction("ldr x0, [sp]");                                        // recover the current new-chain node
    emitter.instruction("bl __rt_throwable_previous");                          // keep any previous link that was supplied by user code
    emitter.instruction("cbz x0, __rt_exception_chain_append");                 // append only at the existing chain's tail
    emitter.instruction("str x0, [sp]");                                        // advance through the borrowed new chain
    emitter.instruction("b __rt_exception_chain_scan");                         // check the next node against every old-chain ancestor

    // -- adopt the old owner using the actual previous-slot representation --
    emitter.label("__rt_exception_chain_append");
    emitter.instruction("ldr x0, [sp]");                                        // recover the tail whose previous link is empty
    emitter.instruction("bl __rt_throwable_previous_slot");                     // resolve raw, boxed, and reference property storage
    emitter.instruction("cbz x0, __rt_exception_chain_release");                // absent storage cannot adopt an owner
    emitter.instruction("cbz x1, __rt_exception_chain_raw");                    // raw slots adopt the existing object owner directly
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the effective slot across Mixed allocation
    emitter.instruction("ldr x9, [x0]");                                        // borrow the existing null box before replacement
    emitter.instruction("str x9, [sp, #32]");                                   // release the old null box only after publishing the new one
    emitter.instruction("ldr x1, [sp, #8]");                                    // supply the owned old exception as the boxed payload
    emitter.instruction("mov x0, #6");                                          // runtime tag six denotes an object payload
    emitter.instruction("mov x2, #0");                                          // object values have no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // create a Mixed box with its own retained object owner
    emitter.instruction("ldr x9, [sp, #24]");                                   // recover the effective previous-property slot
    emitter.instruction("str x0, [x9]");                                        // publish the box before releasing either displaced owner
    emitter.instruction("ldr x0, [sp, #32]");                                   // recover the displaced null box
    emitter.instruction("bl __rt_decref_any");                                  // release the box that no longer belongs to the property
    emitter.instruction("b __rt_exception_chain_release");                      // balance the raw owner now retained by the new box
    emitter.label("__rt_exception_chain_raw");
    emitter.instruction("ldr x9, [sp, #8]");                                    // recover the raw owner to transfer
    emitter.instruction("str x9, [x0]");                                        // the previous link adopts it without a redundant retain
    emitter.label("__rt_exception_chain_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller after successful ownership transfer
    emitter.instruction("add sp, sp, #64");                                     // release traversal storage
    emitter.instruction("ret");                                                 // return with no owner left in this helper
    emitter.label("__rt_exception_chain_release");
    emitter.instruction("ldr x0, [sp, #8]");                                    // recover the redundant or reboxed raw owner
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller before a possible last-owner destructor
    emitter.instruction("add sp, sp, #64");                                     // discard all borrowed traversal cursors
    emitter.instruction("b __rt_decref_any");                                   // consume the old owner under the caller's exception boundary
}

/// Consumes the System V old exception in rdi, appending it to the borrowed exception in rax.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_exception_chain");
    // -- preserve chain cursors and the incoming owner across metadata lookups --
    emitter.instruction("push rbp");                                            // preserve the caller and align outgoing helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish stable traversal slots
    emitter.instruction("sub rsp, 48");                                         // reserve the current node, old owner, and replacement state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // begin at the new exception rooted by the caller
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // keep the old owner until adoption or release
    if emitter.phpext_output {
        emitter.instruction("test rdi, rdi");                                   // distinguish a native pending owner from the null host marker
        emitter.instruction("jnz __rt_exception_chain_has_pending");            // route real pending owners through native chaining
        emitter.instruction("test rax, rax");                                   // only a real native replacement needs retained identity
        emitter.instruction("jz __rt_exception_chain_return");                  // a null host marker owns no replacement pin
        emit_x86_64_replacement_sweep(emitter);
        emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                    // recover the replacement clobbered by the stale-owner callback
        REPLACEMENT_OWNER_STORAGE.emit_check_capacity(emitter, "__rt_exception_chain_replacement_overflow");
        emitter.instruction("mov QWORD PTR [rbp - 48], rax");                   // seed the intrinsic previous-chain tail
        emitter.label("__rt_exception_chain_replacement_tail");
        emitter.instruction("call __rt_throwable_previous");                    // borrow the next constructor/finally predecessor
        emitter.instruction("test rax, rax");                                   // inspect whether the intrinsic chain has another node
        emitter.instruction("jz __rt_exception_chain_replacement_ready");       // stop after the intrinsic previous-chain tail
        emitter.instruction("mov QWORD PTR [rbp - 48], rax");                   // retain the exact insertion point before outer chains append
        emitter.instruction("jmp __rt_exception_chain_replacement_tail");       // continue walking the intrinsic previous chain
        emitter.label("__rt_exception_chain_replacement_ready");
        emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                    // pin the replacement against allocator address reuse
        REPLACEMENT_OWNER_STORAGE.emit_pin(emitter, emit_replacement_owners_address);
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // pass root and original chain tail through SysV
        emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                   // pass the original previous-chain tail to the adapter
        emitter.bl_c("elephc_php_finally_replacement");
        emitter.instruction("jmp __rt_exception_chain_return");                 // the null host marker owns no native reference
        emitter.label("__rt_exception_chain_replacement_overflow");
        emitter.instruction(&format!("mov r10, {STATUS_REPLACEMENT_LIMIT}"));   // expose the bounded replacement-pin failure precisely
        abi::emit_store_reg_to_symbol(emitter, "r10", BOUNDARY_STATUS, 0);
        emitter.instruction("jmp __rt_exception_chain_return");                 // preserve the boundary failure for the export handler
        emitter.label("__rt_exception_chain_has_pending");
    } else {
        emitter.instruction("test rdi, rdi");                                   // inspect the incoming previous owner
        emitter.instruction("jz __rt_exception_chain_return");                  // skip traversal when no previous exception exists
    }
    emitter.instruction("cmp QWORD PTR [rdi], -3");                             // internal forced-unwind markers cannot escape through getPrevious
    emitter.instruction("je __rt_exception_chain_release");                     // release the marker while preserving the real pending Throwable
    if emitter.phpext_output {
        emitter.instruction("test rax, rax");                                   // distinguish a native replacement from a host callback escape
        emitter.instruction("jnz __rt_exception_chain_scan");                   // native replacements use the ordinary in-runtime chain
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // expose the native owner before the host escape consumes it
        emitter.bl_c("elephc_php_previous_exception");                           // copy the previous Throwable into malloc-owned transport
        emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                   // begin walking every previous link owned by that copied root
        emitter.label("__rt_exception_chain_copy_previous");
        emitter.instruction("call __rt_throwable_previous");                    // borrow the next native previous link without retaining it
        emitter.instruction("test rax, rax");                                   // inspect whether another copied node remains
        emitter.instruction("jz __rt_exception_chain_release");                 // release the root after the whole native chain is copied
        emitter.instruction("mov QWORD PTR [rbp - 24], rax");                   // preserve the current node across the SysV callback
        emitter.instruction("mov rdi, rax");                                    // pass the borrowed current node to the adapter
        emitter.bl_c("elephc_php_exception_previous");                           // append it to the active malloc-owned copied chain
        emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                   // continue traversal from the node just copied
        emitter.instruction("jmp __rt_exception_chain_copy_previous");          // preserve every constructor and finally-chain link
    }
    emitter.label("__rt_exception_chain_scan");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restart the old chain at its root
    emitter.label("__rt_exception_chain_ancestors");
    emitter.instruction("cmp rax, QWORD PTR [rbp - 8]");                        // would this link repeat a new-chain object identity?
    emitter.instruction("je __rt_exception_chain_release");                     // consume a redundant owner without introducing a cycle
    emitter.instruction("call __rt_throwable_previous");                        // borrow the next old-chain ancestor
    emitter.instruction("test rax, rax");                                       // has the old chain ended?
    emitter.instruction("jnz __rt_exception_chain_ancestors");                  // compare every old-chain object with the current new-chain node
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the current new-chain node
    emitter.instruction("call __rt_throwable_previous");                        // preserve links supplied before this collector exception
    emitter.instruction("test rax, rax");                                       // is this the tail of the new chain?
    emitter.instruction("jz __rt_exception_chain_append");                      // append only after the existing links
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // advance the borrowed new-chain cursor
    emitter.instruction("jmp __rt_exception_chain_scan");                       // validate the next node against the old chain

    // -- adopt the old owner using the actual previous-slot representation --
    emitter.label("__rt_exception_chain_append");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the new-chain tail
    emitter.instruction("call __rt_throwable_previous_slot");                   // resolve its effective previous-property slot
    emitter.instruction("test rax, rax");                                       // check whether metadata supplied a writable slot
    emitter.instruction("jz __rt_exception_chain_release");                     // absent storage cannot adopt the previous owner
    emitter.instruction("test edx, edx");                                       // inspect the effective slot's boxed flag
    emitter.instruction("jz __rt_exception_chain_raw");                         // raw slots consume the old object owner directly
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the effective slot across allocation
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // inspect the old null box being displaced
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save that box for post-publication release
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the old exception as the boxed object payload
    emitter.instruction("mov eax, 6");                                          // runtime tag six denotes an object
    emitter.instruction("xor esi, esi");                                        // object payloads have no second word
    emitter.instruction("call __rt_mixed_from_value");                          // retain the object under a fresh Mixed box owner
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // recover the effective previous slot
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the new owner before releasing old storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // recover the displaced null box
    emitter.instruction("call __rt_decref_any");                                // release the box no longer owned by the property
    emitter.instruction("jmp __rt_exception_chain_release");                    // balance the old raw owner retained by the new box
    emitter.label("__rt_exception_chain_raw");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // recover the old raw owner
    emitter.instruction("mov QWORD PTR [rax], r10");                            // transfer that owner directly into the raw previous link
    emitter.label("__rt_exception_chain_return");
    emitter.instruction("leave");                                               // restore the caller after ownership transfer
    emitter.instruction("ret");                                                 // leave no owner or cursor in this helper
    emitter.label("__rt_exception_chain_release");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // recover the redundant or reboxed owner
    emitter.instruction("leave");                                               // restore the caller before possible destructor execution
    emitter.instruction("jmp __rt_decref_any");                                 // consume the raw owner under the caller's exception boundary
}

/// Reclaims AArch64 replacement pins whose objects have no owner besides equivalent pins.
fn emit_aarch64_replacement_sweep(emitter: &mut Emitter) {
    emitter.label("__rt_exception_chain_replacement_sweep");
    emit_replacement_owners_address(emitter, "x0");
    abi::emit_load_symbol_to_reg(emitter, "x1", PHPEXT_REPLACEMENT_OWNER_BASE, 0);
    abi::emit_load_symbol_to_reg(emitter, "x2", PHPEXT_REPLACEMENT_OWNER_COUNT, 0);
    emitter.bl_c("elephc_php_stale_finally_replacement");
    emitter.instruction("cbz x0, __rt_exception_chain_replacement_swept");      // finish when every retained root is still live
    REPLACEMENT_OWNER_STORAGE.emit_remove_selected(emitter, 16, 32, emit_replacement_owners_address);
    emitter.label("__rt_exception_chain_replacement_swept");
}

/// Reclaims System V replacement pins whose objects have no owner besides equivalent pins.
fn emit_x86_64_replacement_sweep(emitter: &mut Emitter) {
    emitter.label("__rt_exception_chain_replacement_sweep");
    emit_replacement_owners_address(emitter, "rdi");
    abi::emit_load_symbol_to_reg(emitter, "rsi", PHPEXT_REPLACEMENT_OWNER_BASE, 0);
    abi::emit_load_symbol_to_reg(emitter, "rdx", PHPEXT_REPLACEMENT_OWNER_COUNT, 0);
    emitter.bl_c("elephc_php_stale_finally_replacement");
    emitter.instruction("test rax, rax");                                       // inspect whether the adapter selected a stale root
    emitter.instruction("jz __rt_exception_chain_replacement_swept");           // finish when every retained root is still live
    REPLACEMENT_OWNER_STORAGE.emit_remove_selected(emitter, 24, 32, emit_replacement_owners_address);
    emitter.label("__rt_exception_chain_replacement_swept");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Chaining uses layout-aware reads for both chains and publishes boxed links before releasing.
    #[test]
    fn exception_chain_uses_storage_metadata_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_exception_chain(&mut emitter);
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_throwable_previous\n").count(), 2, "{name}");
            assert!(asm.contains("__rt_throwable_previous_slot"), "{name}");
            assert!(asm.find("__rt_mixed_from_value").unwrap() < asm.find("__rt_decref_any").unwrap(), "{name}");
            assert!(asm.contains("__rt_exception_chain_ancestors:") && asm.contains("__rt_exception_chain_raw:"), "{name}");
            assert!(!asm.contains("elephc_php_previous_exception"), "{name}");
        }
    }

    /// Hosted chains retain real finally replacements and export older native pending owners.
    #[test]
    fn hosted_exception_chain_exports_native_previous_on_every_host_target() {
        for name in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.phpext_output = true;
            emit_exception_chain(&mut emitter);
            let callback = emitter.target.extern_symbol("elephc_php_previous_exception");
            let continuation = emitter.target.extern_symbol("elephc_php_exception_previous");
            let replacement = emitter.target.extern_symbol("elephc_php_finally_replacement");
            let sweep = emitter.target.extern_symbol("elephc_php_stale_finally_replacement");
            let arch = emitter.target.arch;
            let asm = emitter.output();
            assert!(asm.contains(&callback), "{name}: {asm}");
            assert!(asm.contains(&continuation), "{name}: {asm}");
            assert!(asm.contains(&replacement), "{name}: {asm}");
            assert!(asm.contains(&sweep), "{name}: {asm}");
            assert!(asm.contains(PHPEXT_REPLACEMENT_OWNER_BASE), "{name}: {asm}");
            assert!(asm.contains(PHPEXT_REPLACEMENT_OWNER_COUNT), "{name}: {asm}");
            assert!(asm.contains(PHPEXT_REPLACEMENT_OWNERS), "{name}: {asm}");
            assert_eq!(asm.matches("__rt_throwable_previous\n").count(), 4, "{name}: {asm}");
            let swept = asm.find("__rt_exception_chain_replacement_swept:").unwrap();
            let tail_walk = asm[swept..].find("__rt_throwable_previous").unwrap() + swept;
            let recovered_root = &asm[swept..tail_walk];
            match arch {
                Arch::AArch64 => {
                    assert!(recovered_root.contains("ldr x0, [sp]"), "{name}: {asm}");
                    let sweep_callback_return = asm
                        .find("cbz x0, __rt_exception_chain_replacement_swept")
                        .unwrap();
                    let compact = asm
                        .find("__rt_exception_chain_replacement_compact:")
                        .unwrap();
                    let sweep_setup = &asm[sweep_callback_return..compact];
                    let count = sweep_setup
                        .find(PHPEXT_REPLACEMENT_OWNER_COUNT)
                        .unwrap();
                    let base = sweep_setup.find(PHPEXT_REPLACEMENT_OWNER_BASE).unwrap();
                    assert!(count < base, "{name}: {asm}");
                    assert!(asm.contains(&format!("mov x9, #{STATUS_REPLACEMENT_LIMIT}")));
                }
                Arch::X86_64 => {
                    assert!(
                        recovered_root.contains("mov rax, QWORD PTR [rbp - 8]"),
                        "{name}: {asm}"
                    );
                    assert!(
                        asm.contains(&format!("mov r10, {STATUS_REPLACEMENT_LIMIT}")),
                        "{name}: {asm}"
                    );
                }
            }
            assert!(asm.find("__rt_incref").unwrap() < asm.find(&replacement).unwrap(),
                "{name}: {asm}");
            assert!(asm.find(&callback).unwrap() < asm.find("__rt_exception_chain_release:").unwrap(),
                "{name}: {asm}");
        }
    }
}
