//! Purpose:
//! Emits the automatic cycle-collection safe point with a heap-proportional scan budget.
//!
//! Called from:
//! - `gc_control::emit_gc_control()`; generated `GcCollect` safe points call the entry.
//!
//! Key details:
//! - A full collection walks the bump heap up to `_heap_off`, so every safe point earns a
//!   fixed scan credit and the collector runs once the saved credit covers that extent.
//! - Heaps within one credit still collect at every safe point; larger heaps collect once
//!   per `_heap_off / credit` safe points, so the amortized scan per unset stays bounded.
//! - Stranded garbage counts as live bytes, so a safe point also collects once `_gc_live`
//!   reaches `_gc_safe_point_live_limit`: twice the live bytes after the last automatic
//!   collection, capped at half of `_heap_max`. Garbage can therefore never outgrow the
//!   reachable heap or crowd a nearly full arena, whatever each removal strands.
//! - A disabled or nested collector would return without scanning, so those calls defer,
//!   keeping the saved credit and the live limit for a safe point that can collect.
//! - Candidate mode (web applications) already scales with lost owners and always collects.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Heap bytes one automatic safe point may pay for before the collector has to run.
pub(crate) const GC_SAFE_POINT_SCAN_CREDIT: u64 = 64 * 1024;

/// Emits `__rt_gc_collect_cycles_budgeted` for the target architecture.
pub(super) fn emit_gc_collect_budget(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_collect_cycles_budgeted ---");
    emitter.label_global("__rt_gc_collect_cycles_budgeted");
    super::gc_candidates::load(emitter, "x9", "r8", "_gc_candidates_enabled");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x9, __rt_gc_collect_cycles_budget_check"); // budget only the full-heap collector outside candidate mode
            emitter.instruction("b __rt_gc_collect_cycles");                    // candidate collections already scale with the owners that changed
            emitter.label("__rt_gc_collect_cycles_budget_check");
            abi::emit_symbol_address(emitter, "x10", "_gc_safe_point_credit");
            emitter.instruction("ldr x11, [x10]");                              // read the scan credit saved by earlier deferred safe points
            emitter.instruction(&format!("mov x12, #{GC_SAFE_POINT_SCAN_CREDIT}")); // materialize the credit one safe point earns
            emitter.instruction("add x11, x11, x12");                           // add this safe point's credit to the saved budget
        }
        Arch::X86_64 => {
            emitter.instruction("test r8, r8");                                 // test whether candidate collection is active
            emitter.instruction("jz __rt_gc_collect_cycles_budget_check");      // budget only the full-heap collector outside candidate mode
            emitter.instruction("jmp __rt_gc_collect_cycles");                  // candidate collections already scale with the owners that changed
            emitter.label("__rt_gc_collect_cycles_budget_check");
            abi::emit_symbol_address(emitter, "r9", "_gc_safe_point_credit");
            emitter.instruction("mov r10, QWORD PTR [r9]");                     // read the scan credit saved by earlier deferred safe points
            emitter.instruction(&format!("add r10, {GC_SAFE_POINT_SCAN_CREDIT}")); // add this safe point's credit to the saved budget
        }
    }
    super::gc_candidates::load(emitter, "x9", "r8", "_heap_off");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x11, x9");                                 // compare the budget with the heap extent a collection walks
            emitter.instruction("b.hs __rt_gc_collect_cycles_budget_collect");  // collect once the saved credit pays for the full scan
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r10, r8");                                 // compare the budget with the heap extent a collection walks
            emitter.instruction("jae __rt_gc_collect_cycles_budget_collect");   // collect once the saved credit pays for the full scan
        }
    }
    super::gc_candidates::load(emitter, "x9", "r8", "_gc_live");
    super::gc_candidates::load(emitter, "x12", "r11", "_gc_safe_point_live_limit");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x9, x12");                                 // compare live bytes, stranded garbage included, with the growth limit
            emitter.instruction("b.hs __rt_gc_collect_cycles_budget_collect");  // collect before garbage can outgrow the reachable heap
            emitter.label("__rt_gc_collect_cycles_budget_defer");
            emitter.instruction("str x11, [x10]");                              // save the grown budget for the next safe point
            emitter.instruction("mov x0, #0");                                  // a deferred pass collects no graph nodes
            emitter.instruction("ret");                                         // return to the generated safe point
            emitter.label("__rt_gc_collect_cycles_budget_collect");
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r8, r11");                                 // compare live bytes, stranded garbage included, with the growth limit
            emitter.instruction("jae __rt_gc_collect_cycles_budget_collect");   // collect before garbage can outgrow the reachable heap
            emitter.label("__rt_gc_collect_cycles_budget_defer");
            emitter.instruction("mov QWORD PTR [r9], r10");                     // save the grown budget for the next safe point
            emitter.instruction("xor eax, eax");                                // a deferred pass collects no graph nodes
            emitter.instruction("ret");                                         // return to the generated safe point
            emitter.label("__rt_gc_collect_cycles_budget_collect");
        }
    }
    // A disabled or nested collector would return without scanning; keep the credit and the
    // live limit for a later safe point that can actually collect.
    for (symbol, runs_when_zero) in [
        ("_gc_enabled", false),
        ("_gc_collecting", true),
        ("_gc_release_suppressed", true),
    ] {
        super::gc_candidates::load(emitter, "x9", "r8", symbol);
        match (emitter.target.arch, runs_when_zero) {
            (Arch::AArch64, false) => emitter.instruction("cbz x9, __rt_gc_collect_cycles_budget_defer"), // defer while gc_disable() keeps the collector off
            (Arch::AArch64, true) => emitter.instruction("cbnz x9, __rt_gc_collect_cycles_budget_defer"), // defer while a collection or container cleanup is in progress
            (Arch::X86_64, runs_when_zero) => {
                emitter.instruction("test r8, r8");                             // test the collector state flag
                emitter.instruction(if runs_when_zero {                         // defer while a collection or container cleanup is in progress
                    "jnz __rt_gc_collect_cycles_budget_defer"
                } else {
                    "jz __rt_gc_collect_cycles_budget_defer"
                });
            }
        }
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str xzr, [x10]");                              // spend the whole budget on this collection
            emitter.instruction("stp x29, x30, [sp, #-32]!");                   // open a frame for the nested collector call
            emitter.instruction("mov x29, sp");                                 // establish the helper frame pointer
            emitter.instruction("bl __rt_gc_collect_cycles");                   // run the automatic collector, which honors gc_disable()
            emitter.instruction("str x0, [sp, #16]");                           // keep the collected-node count across the limit update
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [r9], 0");                       // spend the whole budget on this collection
            emitter.instruction("push rbp");                                    // open a frame for the nested collector call
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame pointer
            emitter.instruction("sub rsp, 16");                                 // reserve an aligned slot for the collector result
            emitter.instruction("call __rt_gc_collect_cycles");                 // run the automatic collector, which honors gc_disable()
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // keep the collected-node count across the limit update
        }
    }
    super::gc_candidates::load(emitter, "x9", "r8", "_gc_live");
    super::gc_candidates::load(emitter, "x10", "r10", "_heap_max");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("lsl x9, x9, #1");                              // allow live bytes to double before the next forced collection
            emitter.instruction("lsr x10, x10, #1");                            // never let that limit pass half of the fixed arena
            emitter.instruction("cmp x9, x10");                                 // pick the tighter of the two limits
            emitter.instruction("csel x9, x9, x10, lo");                        // keep doubling only while it stays under the arena cap
            abi::emit_symbol_address(emitter, "x10", "_gc_safe_point_live_limit");
            emitter.instruction("str x9, [x10]");                               // publish the live-byte limit for later safe points
            emitter.instruction("ldr x0, [sp, #16]");                           // return the collector's node count
            emitter.instruction("ldp x29, x30, [sp], #32");                     // close the helper frame
            emitter.instruction("ret");                                         // return to the generated safe point
        }
        Arch::X86_64 => {
            emitter.instruction("add r8, r8");                                  // allow live bytes to double before the next forced collection
            emitter.instruction("shr r10, 1");                                  // never let that limit pass half of the fixed arena
            emitter.instruction("cmp r8, r10");                                 // pick the tighter of the two limits
            emitter.instruction("cmovae r8, r10");                              // keep doubling only while it stays under the arena cap
            abi::emit_symbol_address(emitter, "r10", "_gc_safe_point_live_limit");
            emitter.instruction("mov QWORD PTR [r10], r8");                     // publish the live-byte limit for later safe points
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // return the collector's node count
            emitter.instruction("mov rsp, rbp");                                // drop the result slot
            emitter.instruction("pop rbp");                                     // close the helper frame
            emitter.instruction("ret");                                         // return to the generated safe point
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Every target checks candidate mode, the grown credit against `_heap_off`, then live bytes
    /// against their limit, and refreshes that limit after each collection.
    #[test]
    fn budgeted_safe_point_defers_until_credit_or_live_growth_requires_collection() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_gc_collect_budget(&mut emitter);
            let asm = emitter.output();
            let candidates = asm.find("_gc_candidates_enabled").expect("candidate mode check");
            let credit = asm.find("_gc_safe_point_credit").expect("saved scan credit");
            let heap = asm.find("_heap_off").expect("heap extent");
            assert!(candidates < credit && credit < heap, "{target:?}\n{asm}");
            assert!(asm.contains("__rt_gc_collect_cycles_budget_collect:"), "{target:?}\n{asm}");
            assert!(asm.contains(&GC_SAFE_POINT_SCAN_CREDIT.to_string()), "{target:?}\n{asm}");
            let call = match target.arch {
                Arch::AArch64 => "bl __rt_gc_collect_cycles\n",
                Arch::X86_64 => "call __rt_gc_collect_cycles\n",
            };
            let collect = asm.find(call).expect("nested automatic collector call");
            let live = asm.find("_gc_live").expect("live-byte check");
            let limit = asm.rfind("_gc_safe_point_live_limit").expect("limit update");
            assert!(heap < live && live < collect && collect < limit, "{target:?}\n{asm}");
            assert!(asm.contains("_heap_max"), "{target:?}\n{asm}");
            for flag in ["_gc_enabled", "_gc_collecting", "_gc_release_suppressed"] {
                let check = asm.find(flag).unwrap_or_else(|| panic!("{flag} check"));
                assert!(check < collect, "{flag} must be checked before collecting on {target:?}");
            }
            // Apple's assembler rejects conditional branches to external symbols.
            for line in asm.lines().map(str::trim_start) {
                let conditional = line.starts_with("cb") || line.starts_with("b.")
                    || (line.starts_with('j') && !line.starts_with("jmp"));
                assert!(
                    !(conditional && line.contains("__rt_gc_collect_cycles ")
                        || conditional && line.ends_with("__rt_gc_collect_cycles")),
                    "{target:?}: {line}"
                );
            }
        }
    }
}
