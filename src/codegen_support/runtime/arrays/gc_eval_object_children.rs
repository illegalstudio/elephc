//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::gc_eval_object_children::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{platform::Target, RuntimeFeatures};

    /// Every target traverses external edges in both GC phases and releases them after PHP destructors.
    #[test]
    fn eval_object_edges_cover_all_supported_collectors_and_release_paths() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gc_eval_object_children(&mut emitter);
            let helper = emitter.output();
            assert!(helper.contains("_elephc_eval_object_gc_child_fn"), "{name}");
            assert!(helper.contains("_elephc_eval_object_release_fn"), "{name}");
            assert!(helper.contains("__rt_gc_mark_reachable"), "{name}");
            let indirect = if target.arch == Arch::AArch64 { "blr x10" } else { "call r10" };
            assert!(helper.contains(indirect), "{name}");
            let release_body = helper.split_once("__rt_eval_object_release_children:").unwrap().1;
            assert!(release_body.find(indirect).unwrap() < release_body.find("__rt_throw_boxed_destructor_exception").unwrap(), "{name}");
            assert!(release_body.contains("__rt_eval_object_release_children_done:"), "{name}");

            let mut emitter = Emitter::new(target);
            super::super::emit_gc_collect_cycles(&mut emitter);
            assert!(emitter.output().contains("__rt_gc_object_children"), "{name}");
            let mut emitter = Emitter::new(target);
            super::super::emit_gc_mark_reachable(&mut emitter);
            assert!(emitter.output().contains("__rt_gc_object_children"), "{name}");
            let mut emitter = Emitter::new(target);
            super::super::emit_object_free_deep(&mut emitter, RuntimeFeatures::none());
            let free = emitter.output();
            let destructor = free.find("__rt_call_object_destructor").unwrap();
            let release = free.find("__rt_eval_object_release_children").unwrap();
            assert!(destructor < release, "{name}");
        }
    }
}
