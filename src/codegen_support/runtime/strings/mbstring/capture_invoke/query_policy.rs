//! Purpose:
//! Installs native query configuration and filter callbacks in the shared V5 invocation table.
//!
//! Called from:
//! - The native reference invocation emitter when constructing the V5 host.
//!
//! Key details:
//! - The caller owns immutable policy metadata and a live independent host context.
//! - A missing configuration callback leaves V5 query capability unavailable before coercion.
//! - Tail trampolines preserve every C argument and the caller's stack alignment.

use super::*;

/// Installs only the capabilities actually supplied by the caller's native query state.
pub(super) fn table(emitter: &mut Emitter, state: usize) {
    let arm = emitter.target.arch == Arch::AArch64;
    for offset in [120, 128] {
        let instruction = if arm { format!("str xzr, [sp, #{offset}]") }
            else { format!("mov QWORD PTR [rsp + {offset}], 0") };
        emitter.instruction(&instruction);                                      // initialize optional capabilities before inspecting caller metadata
    }
    table_entry(emitter, 136, "__rt_mbstring_query_register");
    let instruction = if arm { format!("ldr x11, [sp, #{state}]") }
        else { format!("mov r11, QWORD PTR [rsp + {state}]") };
    emitter.instruction(&instruction);                                          // recover the immutable policy record after provider initialization
    if arm {
        emitter.instruction("cbz x11, __rt_mbstring_query_policy_ready");       // leave query capability unavailable without native output state
        emitter.instruction("ldr x12, [x11, #24]");                             // inspect the required live configuration callback
        emitter.instruction("cbz x12, __rt_mbstring_query_policy_ready");       // reject missing configuration before the shared coordinator clones arguments
    } else {
        emitter.instruction("test r11, r11");                                   // distinguish missing query policy from a complete caller record
        emitter.instruction("jz __rt_mbstring_query_policy_ready");             // preserve absent capabilities without dereferencing null
        emitter.instruction("cmp QWORD PTR [r11 + 24], 0");                     // require a configuration provider for query output
        emitter.instruction("je __rt_mbstring_query_policy_ready");             // let shared host validation reject incomplete state before side effects
    }
    table_entry(emitter, 120, "__rt_mbstring_query_configuration_context");
    if arm {
        emitter.instruction("ldr x12, [x11, #32]");                             // inspect the optional SAPI input filter
        emitter.instruction("cbz x12, __rt_mbstring_query_policy_ready");       // a missing filter selects the shared identity behavior
    } else {
        emitter.instruction("cmp QWORD PTR [r11 + 32], 0");                     // inspect the independently optional input filter
        emitter.instruction("je __rt_mbstring_query_policy_ready");             // preserve null filtering as an explicit identity capability
    }
    table_entry(emitter, 128, "__rt_mbstring_query_filter_context");
    emitter.label("__rt_mbstring_query_policy_ready");
}

/// Emits C3 configuration and C6 filter trampolines using the policy's independent host context.
pub(super) fn emit(emitter: &mut Emitter) {
    for (name, offset) in [("__rt_mbstring_query_configuration_context", 24),
        ("__rt_mbstring_query_filter_context", 32)] {
        emitter.label_global(name);
        if emitter.target.arch == Arch::AArch64 {
            emitter.instruction("ldr x9, [x0, #8]");                            // recover the native query state from the wrapped invocation context
            emitter.instruction(&format!("ldr x10, [x9, #{offset}]"));          // select the caller's protected callback without altering later arguments
            emitter.instruction("ldr x0, [x9, #16]");                           // pass the live configuration/filter context instead of the eval context
            emitter.instruction("br x10");                                      // preserve the complete C argument list and original return address
        } else {
            emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                // recover native query policy without changing the remaining C arguments
            emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {offset}]")); // retain the callback target outside argument registers
            emitter.instruction("mov rdi, QWORD PTR [r10 + 16]");               // install the policy context while retaining name/value/output arguments
            emitter.instruction("jmp r11");                                     // tail-call the protected host callback with unchanged stack alignment
        }
    }
}
