//! Purpose:
//! Transfers Throwable ownership between boxed eval values and raw native exceptions.
//!
//! Called from:
//! - Eval status lowering and the generated native pending-exception bridge.
//!
//! Key details:
//! - Both directions consume one input owner and return one output owner.
//! - The new owner is acquired before releasing the old representation, so no destructor can run.

pub use elephc_memoir::temporary_owners::emit_throwable_boxed_owners;

#[cfg(test)]
use crate::codegen_support::emit::Emitter;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both conversions acquire the destination owner before releasing the source on all targets.
    #[test]
    fn throwable_box_transfers_balance_both_representations_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_throwable_boxed_owners(&mut emitter);
            let asm = emitter.output();
            let (take, give) = asm.split_once("__rt_throwable_box_owned:").unwrap();
            assert!(take.find("__rt_incref").unwrap() < take.find("__rt_decref_mixed").unwrap(), "{name}");
            assert_eq!(take.matches("__rt_decref_mixed").count(), 1, "{name}");
            assert!(give.find("__rt_mixed_from_value").unwrap() < give.find("__rt_decref_any").unwrap(), "{name}");
            assert_eq!(give.matches("__rt_decref_any").count(), 1, "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
