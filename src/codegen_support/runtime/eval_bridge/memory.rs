//! Purpose:
//! Adapts boxed eval calls to the same native heap metrics as compiled PHP.
//!
//! Called from:
//! - Both architecture arms of the versioned runtime builtin dispatcher.
//!
//! Key details:
//! - Samples counters before allocating the boxed return value.
//! - NAN warning callbacks cannot unwind across Rust frames.
//! - Non-scalar flags defer to the interpreter's catchable TypeError adapter.

use super::*;
use elephc_builtin_contract::RuntimeBuiltinId;
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;

/// Stable runtime IDs and helper suffixes for the native heap metric family.
pub(super) const OPERATIONS: &[(RuntimeBuiltinId, &str)] = &[
    (RuntimeBuiltinId::MemoryGetUsage, "memory_get_usage"),
    (RuntimeBuiltinId::MemoryGetPeakUsage, "memory_get_peak_usage"),
    (RuntimeBuiltinId::MemoryResetPeakUsage, "memory_reset_peak_usage"),
    (RuntimeBuiltinId::MemoryGetAllocations, "memory_get_allocations"),
    (RuntimeBuiltinId::MemoryGetFrees, "memory_get_frees"),
];

/// Emits arity checks, metric sampling, and owned result transfer.
pub(super) fn emit_cases(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let suffix = if arm { "" } else { "_x86" };
    let unsupported = format!("__elephc_runtime_builtin_v1_unsupported{suffix}");
    for &(id, name) in OPERATIONS {
        let label = format!("__elephc_runtime_builtin_v1_{name}{suffix}");
        emitter.label(&label);
        let flag = matches!(id, RuntimeBuiltinId::MemoryGetUsage | RuntimeBuiltinId::MemoryGetPeakUsage);
        // -- validate arity before borrowing argument cells --
        if arm {
            emitter.instruction(&format!("cmp x21, #{}", usize::from(flag)));   // check the maximum supported argument count
            emitter.instruction(&format!("b.hi {unsupported}"));                // reject extra arguments without sampling counters
        } else {
            emitter.instruction(&format!("cmp r13, {}", usize::from(flag)));    // check the maximum supported argument count
            emitter.instruction(&format!("ja {unsupported}"));                  // leave invalid arity to the signature adapter
        }
        if flag {
            // -- contain warning-handler throws below the Rust caller --
            if arm {
                emitter.instruction("mov x0, #0");                              // omitted flags are represented by a null borrowed pointer
                emitter.instruction(&format!("cbz x21, {label}_ready"));        // skip the argument array for zero-argument calls
                emitter.instruction("ldr x0, [x20]");                           // borrow the original flag without boolean coercion
                emitter.label(&format!("{label}_ready"));
                emitter.instruction("mov x1, x22");                             // transfer results directly into the caller's initialized output slot
            } else {
                emitter.instruction("xor edi, edi");                            // default to an omitted flag without allocating a box
                emitter.instruction("test r13, r13");                           // detect whether the caller supplied a flag cell
                emitter.instruction(&format!("jz {label}_ready"));              // avoid reading a missing argument pointer array
                emitter.instruction("mov rdi, QWORD PTR [r12]");                // borrow the provided flag cell
                emitter.label(&format!("{label}_ready"));
                emitter.instruction("mov rsi, r14");                            // pass the caller-owned result-out pointer
            }
            let symbol = if id == RuntimeBuiltinId::MemoryGetUsage {
                "__elephc_eval_memory_usage_v1"
            } else { "__elephc_eval_memory_peak_v1" };
            emitter.bl_c(symbol);
            emitter.instruction(&format!("{} __elephc_runtime_builtin_v1_done{suffix}", if arm { "b" } else { "jmp" })); // return the protected status without boxing it again
            continue;
        }
        // -- zero-argument counters and reset cannot invoke PHP callbacks --
        abi::emit_call_label(emitter, &format!("__rt_{name}"));
        if id == RuntimeBuiltinId::MemoryResetPeakUsage {
            emitter.bl_c("__elephc_eval_value_null");
        } else {
            if !arm {
                emitter.instruction("mov rdi, rax");                            // preserve the sampled integer before allocating its result cell
            }
            emitter.bl_c("__elephc_eval_value_int");
        }
        emitter.instruction(&format!("{} __elephc_runtime_builtin_v1_result{suffix}", if arm { "b" } else { "jmp" })); // transfer exactly one fresh result cell
    }
}

/// Emits C-ABI query boundaries after the dispatcher's shared return labels.
pub(super) fn emit_boundaries(emitter: &mut Emitter) {
    for (name, body) in [
        ("__elephc_eval_memory_usage_v1", usage_body as fn(&mut Emitter)),
        ("__elephc_eval_memory_peak_v1", peak_body as fn(&mut Emitter)),
    ] {
        let symbol = emitter.target.extern_symbol(name);
        crate::codegen_support::runtime::exceptions::emit_protected_status(emitter, &symbol, body);
    }
}

/// Samples current usage after protected scalar-flag conversion.
fn usage_body(emitter: &mut Emitter) { query_body(emitter, false); }

/// Samples the peak after protected scalar-flag conversion.
fn peak_body(emitter: &mut Emitter) { query_body(emitter, true); }

/// Converts borrowed scalars, samples before boxing, and returns a versioned ABI status.
fn query_body(emitter: &mut Emitter, peak: bool) {
    let arm = emitter.target.arch == Arch::AArch64;
    let name = if peak { "peak" } else { "usage" };
    let prefix = format!("__rt_eval_memory_{name}");
    let argument = abi::int_arg_reg_name(emitter.target, 0);
    let result = abi::int_result_reg(emitter);
    let output = if arm { "x10" } else { "r10" };
    // -- recover inputs retained by the shared protected callback frame --
    abi::emit_load_temporary_stack_slot(emitter, argument, TRY_HANDLER_SLOT_SIZE);
    if arm {
        emitter.instruction(&format!("cbz x0, {prefix}_sample"));               // an omitted flag already materializes raw false
    } else {
        emitter.instruction("test rdi, rdi");                                   // a null flag pointer means default false
        emitter.instruction(&format!("jz {prefix}_sample"));                    // omit conversion when no PHP argument was supplied
    }
    emitter.bl_c("__elephc_eval_value_type_tag");
    // -- admit only weak scalar bool parameters and null --
    if arm {
        emitter.instruction("cmp x0, #3");                                      // scalar tags zero through three admit boolean coercion
        emitter.instruction(&format!("b.ls {prefix}_convert"));                 // preserve the scalar conversion path
        emitter.instruction("cmp x0, #8");                                      // null coerces to false in weak builtin calls
        emitter.instruction(&format!("b.ne {prefix}_unsupported"));             // defer container TypeErrors to the interpreter adapter
    } else {
        emitter.instruction("cmp rax, 3");                                      // scalar tags zero through three are supported natively
        emitter.instruction(&format!("jbe {prefix}_convert"));                  // retain PHP truthiness for valid scalars
        emitter.instruction("cmp rax, 8");                                      // null also satisfies the weak boolean boundary
        emitter.instruction(&format!("jne {prefix}_unsupported"));              // return without crossing Rust with a native throw
    }
    emitter.label(&format!("{prefix}_convert"));
    abi::emit_load_temporary_stack_slot(emitter, argument, TRY_HANDLER_SLOT_SIZE);
    emitter.bl_c("__elephc_eval_value_truthy");
    if !arm {
        emitter.instruction("mov rdi, rax");                                    // pass the converted flag through the SysV argument register
    }
    emitter.label(&format!("{prefix}_sample"));
    let helper = if peak { "__rt_memory_get_peak_usage" } else { "__rt_memory_get_usage" };
    abi::emit_call_label(emitter, helper);
    if !arm {
        emitter.instruction("mov rdi, rax");                                    // preserve the sampled count before allocating the return box
    }
    emitter.bl_c("__elephc_eval_value_int");
    abi::emit_load_temporary_stack_slot(emitter, output, TRY_HANDLER_SLOT_SIZE + 8);
    if arm {
        emitter.instruction("str x0, [x10]");                                   // transfer exactly one owned result into the C caller's output slot
    } else {
        emitter.instruction("mov QWORD PTR [r10], rax");                        // transfer ownership before publishing successful status
    }
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.instruction(&format!("{} {prefix}_done", if arm { "b" } else { "jmp" })); // preserve success through common exception-state restoration
    emitter.label(&format!("{prefix}_unsupported"));
    abi::emit_load_int_immediate(emitter, result, 3);
    emitter.label(&format!("{prefix}_done"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// Assembles the native and boxed metric boundaries for every supported object format.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn memory_metrics_assemble_all_targets() {
        for (name, triple) in [
            ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"),
            ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
            ("linux-aarch64", "aarch64-linux-gnu"),
            ("linux-x86_64", "x86_64-linux-gnu"),
        ] {
            for pic in [false, true] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
                emitter.raw(".text");
                crate::codegen_support::runtime::arrays::emit_memory(&mut emitter);
                emit_cases(&mut emitter);
                // -- complete the dispatcher's shared exits for this assembly-only fixture --
                let suffix = if target.arch == Arch::AArch64 { "" } else { "_x86" };
                for exit in ["unsupported", "fatal", "result", "done"] {
                    emitter.label(&format!("__elephc_runtime_builtin_v1_{exit}{suffix}"));
                    emitter.instruction("ret");                                 // terminate the standalone fixture without a missing branch target
                }
                emit_boundaries(&mut emitter);
                let mut assembler = Command::new("clang")
                    .args(["-target", triple, "-x", "assembler", "-c", "-", "-o", "/dev/null"])
                    .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang is required");
                assembler.stdin.take().unwrap().write_all(emitter.output().as_bytes()).unwrap();
                let result = assembler.wait_with_output().unwrap();
                assert!(result.status.success(), "{name}, pic={pic}: {}", String::from_utf8_lossy(&result.stderr));
            }
        }
    }
}
