//! Purpose:
//! Preserves compiler integration tests and compatibility imports for published graph cleanup.
//!
//! Called from:
//! - Managed runtime emission and focused compiler assembly tests.
//!
//! Key details:
//! - Implementation remains in Memoir while every supported target is checked through the compiler emitter.

pub use elephc_memoir::published_graph::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};

    /// Every target pins the complete marked graph before callbacks and releases it before restoring GC state.
    #[test]
    fn published_graph_cleanup_orders_every_supported_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_published_graph_destructors(&mut emitter);
            let assembly = emitter.output();
            let start = assembly
                .find("__rt_shutdown_destruct_graph_clear_start_loop:")
                .unwrap();
            let mark = assembly.find("__rt_gc_mark_reachable").unwrap();
            let count = assembly
                .find("__rt_shutdown_destruct_graph_count_loop:")
                .unwrap();
            let call = match target.arch {
                Arch::AArch64 => "bl",
                Arch::X86_64 => "call",
            };
            let malloc = assembly
                .find(&format!("{call} {}", target.extern_symbol("malloc")))
                .unwrap();
            let fill = assembly
                .find("__rt_shutdown_destruct_graph_fill_loop:")
                .unwrap();
            let destruct = assembly.find("__rt_call_object_destructor").unwrap();
            let unpin = assembly.find("__rt_decref_any").unwrap();
            let free = assembly
                .find(&format!("{call} {}", target.extern_symbol("free")))
                .unwrap();
            let finish = assembly
                .find("__rt_shutdown_destruct_graph_clear_finish_loop:")
                .unwrap();
            assert!(
                start < mark
                    && mark < count
                    && count < malloc
                    && malloc < fill
                    && fill < destruct
                    && destruct < unpin
                    && unpin < free
                    && free < finish,
                "{name}"
            );
            assert_eq!(
                assembly
                    .matches("__rt_shutdown_destruct_graph_clear_start_loop:")
                    .count(),
                1,
                "{name}"
            );
            assert_eq!(
                assembly
                    .matches("__rt_shutdown_destruct_graph_clear_finish_loop:")
                    .count(),
                1,
                "{name}"
            );
            assert!(assembly.contains("_gc_candidates_enabled"), "{name}");
            assert!(assembly.contains("_gc_release_suppressed"), "{name}");
            assert!(assembly.contains("__rt_cleanup_preserve_exception"), "{name}");
        }
    }
}
