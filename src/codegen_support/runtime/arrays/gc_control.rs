//! Purpose:
//! Emits runtime entry points for PHP cycle-collector controls and status counters.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime()`.
//!
//! Key details:
//! - Automatic safe points honor `_gc_enabled`, while explicit collection bypasses it.
//! - Status reads use one integer selector shared with the typed EIR `GcControlOp`.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::ir::GcControlOp;

/// Emits target-aware GC enable, disable, query, and automatic-collection wrappers.
pub fn emit_gc_control(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_control ---");
    match emitter.target.arch {
        Arch::AArch64 => emit_gc_control_aarch64(emitter),
        Arch::X86_64 => emit_gc_control_x86_64(emitter),
    }
}

/// Emits the GC control entry points for AArch64 targets.
fn emit_gc_control_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_collect_cycles");
    abi::emit_symbol_address(emitter, "x9", "_gc_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // read whether automatic safe points are enabled
    emitter.instruction("cbz x9, __rt_gc_collect_cycles_disabled");             // skip an automatic collection while disabled
    emitter.instruction("b __rt_gc_collect_cycles_explicit");                   // tail-call the collector without changing its result
    emitter.label("__rt_gc_collect_cycles_disabled");
    emitter.instruction("mov x0, #0");                                          // a skipped automatic pass collects no graph nodes
    emitter.instruction("ret");                                                 // return to the generated safe point

    emitter.label_global("__rt_gc_disable");
    abi::emit_symbol_address(emitter, "x9", "_gc_enabled");
    emitter.instruction("str xzr, [x9]");                                       // disable later automatic collection safe points
    emitter.instruction("mov x0, #0");                                          // materialize the internal void-result placeholder
    emitter.instruction("ret");                                                 // return to the PHP builtin wrapper

    emitter.label_global("__rt_gc_enable");
    abi::emit_symbol_address(emitter, "x9", "_gc_enabled");
    emitter.instruction("mov x10, #1");                                         // prepare the enabled flag
    emitter.instruction("str x10, [x9]");                                       // enable later automatic collection safe points
    emitter.instruction("mov x0, #0");                                          // materialize the internal void-result placeholder
    emitter.instruction("ret");                                                 // return to the PHP builtin wrapper

    emitter.label_global("__rt_gc_enabled");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_enabled", 0);
    emitter.instruction("ret");                                                 // return the current boolean flag

    emitter.label_global("__rt_gc_mem_caches");
    emitter.instruction("mov x0, #0");                                          // the fixed managed heap has no detachable allocator caches
    emitter.instruction("ret");                                                 // report zero reclaimed cache bytes

    emitter.label_global("__rt_gc_status_metric");
    emitter.instruction(&format!(                                               // compare against the collector-running metric selector
        "cmp x0, #{}",
        GcControlOp::Running.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_running");                         // select the collector-active flag
    emitter.instruction(&format!(                                               // compare against the release-protection metric selector
        "cmp x0, #{}",
        GcControlOp::Protected.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_protected");                       // select release-protection state
    emitter.instruction(&format!("cmp x0, #{}", GcControlOp::Runs.as_i64()));   // compare against the productive-run metric selector
    emitter.instruction("b.eq __rt_gc_status_runs");                            // select the productive-run counter
    emitter.instruction(&format!(                                               // compare against the collected-node metric selector
        "cmp x0, #{}",
        GcControlOp::Collected.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_collected");                       // select the cumulative collected-node counter
    emitter.instruction("mov x0, #0");                                          // this collector buffers no root table entries
    emitter.instruction("ret");                                                 // return the roots metric or an unknown-selector fallback
    emitter.label("__rt_gc_status_running");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_collecting", 0);
    emitter.instruction("ret");                                                 // return the collector-active flag
    emitter.label("__rt_gc_status_protected");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_release_suppressed", 0);
    emitter.instruction("ret");                                                 // return release-protection state
    emitter.label("__rt_gc_status_runs");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_runs", 0);
    emitter.instruction("ret");                                                 // return productive collection runs
    emitter.label("__rt_gc_status_collected");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_collected", 0);
    emitter.instruction("ret");                                                 // return cumulative collected graph nodes
}

/// Emits the GC control entry points for Linux x86_64.
fn emit_gc_control_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_collect_cycles");
    abi::emit_symbol_address(emitter, "r8", "_gc_enabled");
    emitter.instruction("cmp QWORD PTR [r8], 0");                               // read whether automatic safe points are enabled
    emitter.instruction("je __rt_gc_collect_cycles_disabled");                  // skip an automatic collection while disabled
    emitter.instruction("jmp __rt_gc_collect_cycles_explicit");                 // tail-call the collector without changing its result
    emitter.label("__rt_gc_collect_cycles_disabled");
    emitter.instruction("xor eax, eax");                                        // a skipped automatic pass collects no graph nodes
    emitter.instruction("ret");                                                 // return to the generated safe point

    emitter.label_global("__rt_gc_disable");
    abi::emit_symbol_address(emitter, "r8", "_gc_enabled");
    emitter.instruction("mov QWORD PTR [r8], 0");                               // disable later automatic collection safe points
    emitter.instruction("xor eax, eax");                                        // materialize the internal void-result placeholder
    emitter.instruction("ret");                                                 // return to the PHP builtin wrapper

    emitter.label_global("__rt_gc_enable");
    abi::emit_symbol_address(emitter, "r8", "_gc_enabled");
    emitter.instruction("mov QWORD PTR [r8], 1");                               // enable later automatic collection safe points
    emitter.instruction("xor eax, eax");                                        // materialize the internal void-result placeholder
    emitter.instruction("ret");                                                 // return to the PHP builtin wrapper

    emitter.label_global("__rt_gc_enabled");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_enabled", 0);
    emitter.instruction("ret");                                                 // return the current boolean flag

    emitter.label_global("__rt_gc_mem_caches");
    emitter.instruction("xor eax, eax");                                        // the fixed managed heap has no detachable allocator caches
    emitter.instruction("ret");                                                 // report zero reclaimed cache bytes

    emitter.label_global("__rt_gc_status_metric");
    emitter.instruction(&format!(                                               // compare against the collector-running metric selector
        "cmp rdi, {}",
        GcControlOp::Running.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_running");                           // select the collector-active flag
    emitter.instruction(&format!(                                               // compare against the release-protection metric selector
        "cmp rdi, {}",
        GcControlOp::Protected.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_protected");                         // select release-protection state
    emitter.instruction(&format!("cmp rdi, {}", GcControlOp::Runs.as_i64()));   // compare against the productive-run metric selector
    emitter.instruction("je __rt_gc_status_runs");                              // select the productive-run counter
    emitter.instruction(&format!(                                               // compare against the collected-node metric selector
        "cmp rdi, {}",
        GcControlOp::Collected.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_collected");                         // select the cumulative collected-node counter
    emitter.instruction("xor eax, eax");                                        // this collector buffers no root table entries
    emitter.instruction("ret");                                                 // return the roots metric or an unknown-selector fallback
    emitter.label("__rt_gc_status_running");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_collecting", 0);
    emitter.instruction("ret");                                                 // return the collector-active flag
    emitter.label("__rt_gc_status_protected");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_release_suppressed", 0);
    emitter.instruction("ret");                                                 // return release-protection state
    emitter.label("__rt_gc_status_runs");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_runs", 0);
    emitter.instruction("ret");                                                 // return productive collection runs
    emitter.label("__rt_gc_status_collected");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_collected", 0);
    emitter.instruction("ret");                                                 // return cumulative collected graph nodes
}
