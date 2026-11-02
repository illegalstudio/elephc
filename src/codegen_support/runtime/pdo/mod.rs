//! Purpose:
//! Groups the PDO Tier-D callback adapters (`__rt_pdo_*`) emitted into the runtime
//! `.text` section. These are the shared, stateless codegen adapters that re-enter
//! compiled-PHP callables on behalf of the `elephc-pdo` bridge: collation
//! comparators, scalar user functions, and aggregate step/finalize callbacks.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`, gated by
//!   `RuntimeFeatures::pdo_udf` so the family is emitted only when a PDO callback
//!   registration is reachable.
//!
//! Key details:
//! - Each adapter is a single `.globl __rt_pdo_*` symbol whose address is taken by
//!   the `__elephc_pdo_adapter_addr` builtin and handed to the bridge; the bridge
//!   stores and calls it but never references a `__rt_*` symbol directly.

mod pdo_call_agg_final;
mod pdo_call_agg_step;
mod pdo_call_collation;
mod pdo_call_scalar;

pub(crate) use pdo_call_agg_final::emit_pdo_call_agg_final;
pub(crate) use pdo_call_agg_step::emit_pdo_call_agg_step;
pub(crate) use pdo_call_collation::emit_pdo_call_collation;
pub(crate) use pdo_call_scalar::emit_pdo_call_scalar;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};
    use crate::codegen_support::try_handlers::{TRY_HANDLER_SLOT_SIZE, TRY_HANDLER_JMP_BUF_OFFSET};

    #[test]
    fn pdo_callback_frames_keep_scratch_outside_jump_buffers() {
        let adapters: [(fn(&mut Emitter), usize, usize, usize); 4] = [
            (emit_pdo_call_scalar, 80, 64, 64),
            (emit_pdo_call_agg_final, 80, 64, 64),
            (emit_pdo_call_agg_step, 96, 80, 80),
            (emit_pdo_call_collation, 96, 72, 80),
        ];
        let area = TRY_HANDLER_SLOT_SIZE + 16;
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for (emit, arm_tail, x86_base_tail, x86_frame_tail) in adapters {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emit(&mut emitter);
                let asm = emitter.output();
                match target.arch {
                    Arch::AArch64 => {
                        assert!(asm.contains(&format!("sub sp, sp, #{}", area + arm_tail)), "{name}");
                        assert!(asm.contains(&format!("str x0, [sp, #{}]", area)), "{name}");
                        assert!(asm.contains(&format!("ldp x29, x30, [sp, #{}]", area + arm_tail - 16)), "{name}");
                    }
                    Arch::X86_64 => {
                        let base = area + x86_base_tail;
                        assert!(asm.contains(&format!("sub rsp, {}", area + x86_frame_tail)), "{name}");
                        assert!(asm.contains(&format!("lea r10, [rbp - {}]", base)), "{name}");
                        assert!(asm.contains(&format!("lea rdi, [rbp - {}]", base - TRY_HANDLER_JMP_BUF_OFFSET)), "{name}");
                        assert!(asm.contains("mov QWORD PTR [rbp - 8], rdi"), "{name}");
                    }
                }
            }
        }
    }
}
