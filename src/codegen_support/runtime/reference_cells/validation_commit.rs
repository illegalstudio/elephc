//! Purpose:
//! Transfers one validated candidate into a persistent reference before retiring its old value.
//!
//! Called from:
//! - Native reference publishers after the Rust coordinator has returned successfully.
//!
//! Key details:
//! - The caller owns a valid context and keeps the wrapper alive until return.
//! - The candidate is consumed on every path, including pending exceptions and refused wrappers.
//! - Publication runs without PHP; old-value retirement contains Throwable exits in context.
//! - The rooted entry removes the original RHS lease after publication, before old retirement.

use super::*;
use elephc_memoir::owner_layout::NativeValidationContext;

const INVALID_STATUS: i64 = 1;
pub(super) const THROW_STATUS: i64 = 2;

/// Emits the two native commit entries using the validation coordinator's ABI.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    use elephc_memoir::reference_cells::validated_commit::{self, Commit};

    let context = Commit {
        pending: elephc_memoir::RecordOffset::new(NativeValidationContext::PENDING_THROWABLE),
        retire: "__rt_reference_validation_release",
        take_snapshot: None,
        invalid: INVALID_STATUS,
        threw: THROW_STATUS,
    };
    validated_commit::emit_commit(emitter, "__rt_reference_commit_native", context);
    validated_commit::emit_commit(emitter, "__rt_reference_commit_native_rooted", Commit {
        take_snapshot: Some("__rt_reference_validation_pin_take"),
        ..context
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All targets publish before throwable retirement and consume refused candidates separately.
    #[test]
    fn native_reference_commit_transfers_before_protected_retirement_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            let (plain, rooted) = assembly.split_once("__rt_reference_commit_native_rooted:").unwrap();
            let (publish, refused) = plain.split_once("__rt_reference_commit_native_pending:").unwrap();
            let store = if name == "linux-x86_64" { "mov QWORD PTR [r10 + 8], rax" }
                else { "str x0, [x10, #8]" };
            assert!(publish.find(store).unwrap() < publish.find("__rt_reference_validation_release").unwrap(), "{name}: publish before cleanup");
            assert_eq!(refused.matches("__rt_reference_validation_release").count(), 2, "{name}: pending and invalid paths each consume their candidate");
            let success = rooted.split_once("__rt_reference_commit_native_rooted_pending:").unwrap().0;
            let publication = success.find(store).unwrap();
            let take = success.find("__rt_reference_validation_pin_take").unwrap();
            let first_release = success.find("__rt_reference_validation_release").unwrap();
            let last_release = success.rfind("__rt_reference_validation_release").unwrap();
            assert!(publication < take && take < first_release && first_release < last_release,
                "{name}: retire the snapshot after publication but before the displaced value");
            assert!(!assembly.contains("__rt_throw_current"), "{name}: the publisher retires roots before propagating a Throwable");
        }
    }
}
