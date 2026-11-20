//! Purpose:
//! Emits bounded storage for retained native exception replacement owners.
//!
//! Called from:
//! - Hosted exception chaining and native export teardown.
//!
//! Key details:
//! - The caller selects a stale owner and materializes the table address.
//! - Removal publishes the compacted prefix and clears vacated entries before any release.

use crate::{abi, Arch, Emitter};

/// Shared count, export base, and capacity for a replacement-owner table.
#[derive(Clone, Copy)]
pub struct ReplacementOwners<'a> {
    /// Symbol holding the number of published owner words.
    pub count: &'a str,
    /// Symbol holding the active export's first owned word.
    pub base: &'a str,
    /// Maximum number of owner words in the table.
    pub limit: usize,
}

impl ReplacementOwners<'_> {
    /// Emits a C entry that counts adapter pins of one borrowed native object.
    pub fn emit_c_pin_count<E: Emitter>(self, emitter: &mut E, emit_address: fn(&mut E, &str)) {
        let entry = emitter.extern_symbol("elephc_phpext_native_replacement_pin_count");
        let scan = "__rt_phpext_replacement_pin_count_scan";
        let done = "__rt_phpext_replacement_pin_count_done";
        emitter.label_global(&entry);
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.emit_load_symbol_to_reg("x10", self.count, 0);
                emit_address(emitter, "x11");
                emitter.instruction(&format!("mov x12, #{}", self.limit));      // bound the published count by the reservation
                emitter.instruction("cmp x10, x12");                            // compare unsigned row counts
                emitter.instruction("csel x10, x10, x12, ls");                  // use only the initialized, in-bounds prefix
                emitter.instruction("mov x9, #0");                              // initialize the cursor
                emitter.instruction("mov x12, #0");                             // initialize the pin total
                emitter.label(scan);
                emitter.instruction("cmp x9, x10");                             // finish after the published prefix
                emitter.instruction(&format!("b.hs {done}"));                   // return the pin total
                emitter.instruction("ldr x13, [x11, x9, lsl #3]");              // borrow one recorded owner
                emitter.instruction("cmp x13, x0");                             // compare against the requested object
                emitter.instruction("cinc x12, x12, eq");                       // count each matching pin independently
                emitter.instruction("add x9, x9, #1");                          // advance to the next owner word
                emitter.instruction(&format!("b {scan}"));                      // inspect the next published row
                emitter.label(done);
                emitter.instruction("mov x0, x12");                             // return the total in the C result register
            }
            Arch::X86_64 => {
                emitter.emit_load_symbol_to_reg("r10", self.count, 0);
                emit_address(emitter, "r11");
                emitter.instruction(&format!("mov rdx, {}", self.limit));       // bound the published count by the reservation
                emitter.instruction("cmp r10, rdx");                            // compare unsigned row counts
                emitter.instruction("cmova r10, rdx");                          // use only the initialized, in-bounds prefix
                emitter.instruction("xor ecx, ecx");                            // initialize the cursor
                emitter.instruction("xor eax, eax");                            // initialize the pin total
                emitter.label(scan);
                emitter.instruction("cmp rcx, r10");                            // finish after the published prefix
                emitter.instruction(&format!("jae {done}"));                    // return the pin total
                emitter.instruction("xor edx, edx");                            // zero-extend the comparison result
                emitter.instruction("cmp QWORD PTR [r11 + rcx*8], rdi");        // compare the borrowed owner against the requested object
                emitter.instruction("sete dl");                                 // count a matching pin as one
                emitter.instruction("add rax, rdx");                            // accumulate matching pins
                emitter.instruction("inc rcx");                                 // advance to the next owner word
                emitter.instruction(&format!("jmp {scan}"));                    // inspect the next published row
                emitter.label(done);
            }
        }
        emitter.emit_return();
    }

    /// Branches to the caller's overflow path when the owner table is full.
    pub fn emit_check_capacity(self, emitter: &mut impl Emitter, overflow: &str) {
        match emitter.arch() {
            Arch::AArch64 => {
                abi::emit_load_symbol_to_reg(emitter, "x9", self.count, 0);
                emitter.instruction(&format!("cmp x9, #{}", self.limit));       // bound simultaneously live replacement pins
                emitter.instruction(&format!("b.hs {overflow}"));               // reject a pin beyond the bounded owner table
            }
            Arch::X86_64 => {
                abi::emit_load_symbol_to_reg(emitter, "r10", self.count, 0);
                emitter.instruction(&format!("cmp r10, {}", self.limit));       // bound simultaneously live replacement pins
                emitter.instruction(&format!("jae {overflow}"));                // reject a pin beyond the bounded owner table
            }
        }
    }

    /// Acquires the current result and publishes its owner before incrementing the count.
    pub fn emit_pin<E: Emitter>(self, emitter: &mut E, emit_address: fn(&mut E, &str)) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction("bl __rt_incref");                          // retain the root across host-side identity tracking
                abi::emit_load_symbol_to_reg(emitter, "x9", self.count, 0);
                emit_address(emitter, "x10");
                emitter.instruction("str x0, [x10, x9, lsl #3]");               // publish the retained owner before exposing its identity
                emitter.instruction("add x9, x9, #1");                          // publish one additional retained replacement owner
                abi::emit_store_reg_to_symbol(emitter, "x9", self.count, 0);
            }
            Arch::X86_64 => {
                emitter.instruction("call __rt_incref");                        // retain the root across host-side identity tracking
                abi::emit_load_symbol_to_reg(emitter, "r10", self.count, 0);
                emit_address(emitter, "r11");
                emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");        // publish the retained owner before the C identity mapping
                emitter.instruction("add r10, 1");                              // publish one additional retained replacement owner
                abi::emit_store_reg_to_symbol(emitter, "r10", self.count, 0);
            }
        }
    }

    /// Removes every pin for the selected object, then returns to the caller's sweep label.
    pub fn emit_remove_selected<E: Emitter>(self, emitter: &mut E, owner_slot: usize, remaining_slot: usize, emit_address: fn(&mut E, &str)) {
        match emitter.arch() {
            Arch::AArch64 => {
            emitter.instruction(&format!("str x0, [sp, #{owner_slot}]"));       // keep the stale root across compaction and releases
            abi::emit_load_symbol_to_reg(emitter, "x10", self.count, 0);
            abi::emit_load_symbol_to_reg(emitter, "x9", self.base, 0);
            emit_address(emitter, "x11");
            emitter.instruction("mov x12, x9");                                 // read cursor starts at this export's first owner
            emitter.instruction("mov x13, x9");                                 // write cursor compacts retained owners in place
            emitter.instruction("mov x14, #0");                                 // number of identical pins to consume
            emitter.label("__rt_exception_chain_replacement_compact");
            emitter.instruction("cmp x12, x10");                                // inspect whether the read cursor reached the published end
            emitter.instruction("b.hs __rt_exception_chain_replacement_compacted"); // publish the compacted owner prefix after the scan
            emitter.instruction("ldr x15, [x11, x12, lsl #3]");                 // borrow the next retained replacement owner
            emitter.instruction("add x12, x12, #1");                            // advance the read cursor before classifying this owner
            emitter.instruction(&format!("ldr x16, [sp, #{owner_slot}]"));      // recover the stale root selected by the adapter
            emitter.instruction("cmp x15, x16");                                // compare the retained owner with the stale root identity
            emitter.instruction("b.eq __rt_exception_chain_replacement_remove"); // count identical pins instead of retaining them
            emitter.instruction("str x15, [x11, x13, lsl #3]");                 // compact one live owner into the published prefix
            emitter.instruction("add x13, x13, #1");                            // advance the compacted write cursor
            emitter.instruction("b __rt_exception_chain_replacement_compact");  // scan the next retained owner
            emitter.label("__rt_exception_chain_replacement_remove");
            emitter.instruction("add x14, x14, #1");                            // record one pin that must release the stale root
            emitter.instruction("b __rt_exception_chain_replacement_compact");  // continue compacting the retained owners
            emitter.label("__rt_exception_chain_replacement_compacted");
            emitter.instruction(&format!("str x14, [sp, #{remaining_slot}]"));  // releases left after publishing compaction
            abi::emit_store_reg_to_symbol(emitter, "x13", self.count, 0);
            emitter.label("__rt_exception_chain_replacement_zero_tail");
            emitter.instruction("cmp x13, x10");                                // inspect whether every vacated owner slot is clear
            emitter.instruction("b.hs __rt_exception_chain_replacement_release"); // release removed pins after clearing stale slots
            emitter.instruction("str xzr, [x11, x13, lsl #3]");                 // clear one vacated owner slot before a destructor can escape
            emitter.instruction("add x13, x13, #1");                            // advance through the vacated owner tail
            emitter.instruction("b __rt_exception_chain_replacement_zero_tail"); // clear every slot removed by compaction
            emitter.label("__rt_exception_chain_replacement_release");
            emitter.instruction(&format!("ldr x14, [sp, #{remaining_slot}]"));  // recover the number of stale pins still owned here
            emitter.instruction("cbz x14, __rt_exception_chain_replacement_sweep"); // ask the adapter for the next stale identity
            emitter.instruction("sub x14, x14, #1");                            // claim one stale pin for the next release
            emitter.instruction(&format!("str x14, [sp, #{remaining_slot}]"));  // make a throwing final decref idempotent
            emitter.instruction(&format!("ldr x0, [sp, #{owner_slot}]"));       // recover the stale root whose retained pin is consumed
            emitter.instruction("bl __rt_decref_any");                          // release one equivalent retained replacement owner
            emitter.instruction("b __rt_exception_chain_replacement_release");  // consume every equivalent pin before the next sweep
            }
            Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rbp - {owner_slot}], rax")); // preserve the stale root across compaction
            abi::emit_load_symbol_to_reg(emitter, "r8", self.base, 0);
            abi::emit_load_symbol_to_reg(emitter, "r10", self.count, 0);
            emit_address(emitter, "r11");
            emitter.instruction("mov r9, r8");                                  // write cursor starts at this export's first owner
            emitter.instruction("xor edx, edx");                                // number of identical pins to consume
            emitter.label("__rt_exception_chain_replacement_compact");
            emitter.instruction("cmp r8, r10");                                 // inspect whether the read cursor reached the published end
            emitter.instruction("jae __rt_exception_chain_replacement_compacted"); // publish the compacted owner prefix after the scan
            emitter.instruction("mov rcx, QWORD PTR [r11 + r8*8]");             // borrow the next retained replacement owner
            emitter.instruction("add r8, 1");                                   // advance the read cursor before classifying this owner
            emitter.instruction(&format!("cmp rcx, QWORD PTR [rbp - {owner_slot}]")); // compare the retained owner with the stale root identity
            emitter.instruction("je __rt_exception_chain_replacement_remove");  // count identical pins instead of retaining them
            emitter.instruction("mov QWORD PTR [r11 + r9*8], rcx");             // compact one live owner into the published prefix
            emitter.instruction("add r9, 1");                                   // advance the compacted write cursor
            emitter.instruction("jmp __rt_exception_chain_replacement_compact"); // scan the next retained owner
            emitter.label("__rt_exception_chain_replacement_remove");
            emitter.instruction("add rdx, 1");                                  // record one pin that must release the stale root
            emitter.instruction("jmp __rt_exception_chain_replacement_compact"); // continue compacting the retained owners
            emitter.label("__rt_exception_chain_replacement_compacted");
            emitter.instruction(&format!("mov QWORD PTR [rbp - {remaining_slot}], rdx")); // releases left after publishing compaction
            abi::emit_store_reg_to_symbol(emitter, "r9", self.count, 0);
            emitter.label("__rt_exception_chain_replacement_zero_tail");
            emitter.instruction("cmp r9, r10");                                 // inspect whether every vacated owner slot is clear
            emitter.instruction("jae __rt_exception_chain_replacement_release"); // release removed pins after clearing stale slots
            emitter.instruction("mov QWORD PTR [r11 + r9*8], 0");               // clear one vacated owner slot before a destructor can escape
            emitter.instruction("add r9, 1");                                   // advance through the vacated owner tail
            emitter.instruction("jmp __rt_exception_chain_replacement_zero_tail"); // clear every slot removed by compaction
            emitter.label("__rt_exception_chain_replacement_release");
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {remaining_slot}]")); // recover the number of stale pins still owned here
            emitter.instruction("test r10, r10");                               // inspect whether another equivalent pin must be released
            emitter.instruction("jz __rt_exception_chain_replacement_sweep");   // ask the adapter for the next stale identity
            emitter.instruction("sub r10, 1");                                  // claim one stale pin for the next release
            emitter.instruction(&format!("mov QWORD PTR [rbp - {remaining_slot}], r10")); // make a throwing final decref idempotent
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {owner_slot}]")); // recover the stale root whose retained pin is consumed
            emitter.instruction("call __rt_decref_any");                        // release one equivalent retained replacement owner
            emitter.instruction("jmp __rt_exception_chain_replacement_release"); // consume every equivalent pin before the next sweep
            }
        }
    }

    /// Retires the owned suffix down to the caller's saved export boundary.
    pub fn emit_release_to<E: Emitter>(
        self, emitter: &mut E, base: usize, suffix: &str, emit_address: fn(&mut E, &str),
    ) {
        let loop_label = format!("L_{suffix}_release_replacement_owner");
        let done = format!("L_{suffix}_released_replacement_owners");
        emitter.label(&loop_label);
        match emitter.arch() {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x10", base);
                abi::emit_load_symbol_to_reg(emitter, "x9", self.count, 0);
                emitter.instruction("cmp x9, x10");                             // retire only pins acquired below this export boundary
                emitter.instruction(&format!("b.ls {done}"));                   // stop at the owner count saved by this export
                emitter.instruction("sub x9, x9, #1");                          // claim the newest replacement pin for release
                abi::emit_store_reg_to_symbol(emitter, "x9", self.count, 0);
                emit_address(emitter, "x10");
                emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // consume the last retained replacement owner
                emitter.instruction("str xzr, [x10, x9, lsl #3]");              // make an escaping destructor path idempotent
                abi::emit_call_label(emitter, "__rt_decref_any");
                emitter.instruction(&format!("b {loop_label}"));                // retire every pin acquired by this export
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r11", base);
                abi::emit_load_symbol_to_reg(emitter, "r10", self.count, 0);
                emitter.instruction("cmp r10, r11");                            // retire only pins acquired below this export boundary
                emitter.instruction(&format!("jbe {done}"));                    // stop at the owner count saved by this export
                emitter.instruction("sub r10, 1");                              // claim the newest replacement pin for release
                abi::emit_store_reg_to_symbol(emitter, "r10", self.count, 0);
                emit_address(emitter, "r11");
                emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");        // consume the last retained replacement owner
                emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");          // make an escaping destructor path idempotent
                abi::emit_call_label(emitter, "__rt_decref_any");
                emitter.instruction(&format!("jmp {loop_label}"));              // retire every pin acquired by this export
            }
        }
        emitter.label(&done);
    }
}
