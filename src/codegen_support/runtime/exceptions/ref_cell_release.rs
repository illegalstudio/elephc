//! Purpose:
//! Preserves compiler imports and tests for Memoir boxed-value ownership.
//!
//! Called from:
//! - Compiler runtime emission and memory integration tests.
//!
//! Key details:
//! - Assembly implementation lives in the compiler-only Memoir crate.

pub use elephc_memoir::reference_cells::local_release::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};

#[cfg(test)]
use crate::codegen_support::sentinels::REFERENCE_CELL_HEAP_KIND;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All target variants release the payload inside a boundary and free the cell before rethrowing.
    #[test]
    fn local_ref_cell_cleanup_finishes_before_exception_propagation() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_local_ref_cell_release(&mut emitter);
            let asm = emitter.output();
            let release = asm.find("__rt_cleanup_invoke").unwrap();
            let free = asm.find("__rt_heap_free").unwrap();
            let chain = asm.find("__rt_exception_chain").unwrap();
            let propagate = asm.find("__rt_throw_current").unwrap();
            assert!(release < free && free < chain && chain < propagate, "{name}");
            let retain_guard = if name == "linux-x86_64" {
                "sub DWORD PTR [rax - 12], 1"
            } else {
                "subs w9, w9, #1"
            };
            assert!(asm.find(retain_guard).unwrap() < release, "{name}: shared cells skip payload retirement");
            let managed_kind_guard = if name == "linux-x86_64" {
                format!("cmp BYTE PTR [rax - 8], {REFERENCE_CELL_HEAP_KIND}")
            } else {
                format!("cmp w9, #{REFERENCE_CELL_HEAP_KIND}")
            };
            assert!(
                asm.find(&managed_kind_guard).unwrap() < release,
                "{name}: managed cells use the registered heap kind"
            );
            assert_eq!(asm.matches("__rt_heap_free").count(), 1, "{name}: retire the cell once");
        }
    }
}
