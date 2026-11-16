//! Purpose:
//! Preserves compiler imports and tests for Memoir value storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

use crate::codegen_support::{emit::Emitter, RuntimeFeatures};
/// Emits the existing helper using only its reviewed memory feature gates.
pub fn emit_mixed_free_deep(emitter: &mut Emitter, features: RuntimeFeatures) {
    elephc_memoir::values::mixed_free_deep::emit_mixed_free_deep(emitter, features.into());
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};
    use crate::ir::ResourceCleanupKind;

    use super::*;

    /// One target's ladder shapes: the dispatch test and the branch taken by each gated arm.
    struct LadderShapes {
        platform: Platform,
        arch: Arch,
        compare: &'static str,
        popen_branch: &'static str,
        dir_branch: &'static str,
    }

    const LADDERS: &[LadderShapes] = &[
        LadderShapes {
            platform: Platform::MacOS,
            arch: Arch::AArch64,
            compare: "cmp x9, #",
            popen_branch: "b.eq __rt_mixed_free_deep_resource_popen\n",
            dir_branch: "b.eq __rt_mixed_free_deep_resource_dir\n",
        },
        LadderShapes {
            platform: Platform::Linux,
            arch: Arch::X86_64,
            compare: "cmp r9, ",
            popen_branch: "je __rt_mixed_free_deep_resource_popen\n",
            dir_branch: "je __rt_mixed_free_deep_resource_dir\n",
        },
    ];

    /// Emits the helper for one target and feature set.
    fn emit_for(shapes: &LadderShapes, features: RuntimeFeatures) -> String {
        let mut emitter = Emitter::new(Target::new(shapes.platform, shapes.arch));
        emit_mixed_free_deep(&mut emitter, features);
        emitter.output()
    }

    /// Returns a feature set with only the named resource kinds enabled.
    fn only(popen: bool, directory: bool) -> RuntimeFeatures {
        RuntimeFeatures {
            popen_resource: popen,
            directory_resource: directory,
            ..RuntimeFeatures::none()
        }
    }

    /// The two kind-specific destructor arms appear only for a program whose EIR can produce
    /// that kind, and each one follows its OWN bit.
    ///
    /// Both directions are checked, because an "is it absent" assertion alone passes just as
    /// well against an emitter that has stopped emitting anything at all. The two bits are then
    /// checked independently: they are separate producers, and a gate written as one shared
    /// condition would still satisfy a both-on/both-off test.
    #[test]
    fn the_kind_specific_destructor_arms_follow_their_producers() {
        for shapes in LADDERS {
            let arch = shapes.arch;

            let wide = emit_for(shapes, RuntimeFeatures::all());
            assert!(wide.contains(shapes.popen_branch), "{arch:?}: popen arm must be emitted");
            assert!(wide.contains(shapes.dir_branch), "{arch:?}: directory arm must be emitted");
            assert!(
                wide.contains("__rt_pclose"),
                "{arch:?}: the popen arm is the only runtime reference to the pclose helper"
            );
            assert!(
                wide.contains("__rt_closedir"),
                "{arch:?}: the directory arm is the only runtime reference to the closedir helper"
            );

            let narrow = emit_for(shapes, RuntimeFeatures::none());
            assert!(!narrow.contains(shapes.popen_branch), "{arch:?}: popen arm must be gone");
            assert!(!narrow.contains(shapes.dir_branch), "{arch:?}: directory arm must be gone");
            assert!(
                !narrow.contains("__rt_pclose"),
                "{arch:?}: nothing may still reach the pclose helper, or pclose stays imported"
            );
            assert!(
                !narrow.contains("__rt_closedir"),
                "{arch:?}: nothing may still reach the closedir helper, or closedir, globfree \
                 and close stay imported"
            );

            let popen_only = emit_for(shapes, only(true, false));
            assert!(
                popen_only.contains(shapes.popen_branch) && !popen_only.contains(shapes.dir_branch),
                "{arch:?}: the popen bit must select the popen arm alone"
            );

            let dir_only = emit_for(shapes, only(false, true));
            assert!(
                dir_only.contains(shapes.dir_branch) && !dir_only.contains(shapes.popen_branch),
                "{arch:?}: the directory bit must select the directory arm alone"
            );
        }
    }

    /// The kinds with no producing builtin are NOT gated and must survive the narrowest build.
    ///
    /// Kind 1 closes with a raw syscall on AArch64 and imports nothing, and kind 2 is stamped by
    /// the runtime helper `__rt_hash_init` rather than by a lowering, so no EIR call names it and
    /// no feature bit could honestly carry it. Gating either by mistake would leak a descriptor or
    /// a hash context in exactly the programs that never mention a directory or a pipe.
    #[test]
    fn the_ungated_kinds_survive_the_narrowest_build() {
        for shapes in LADDERS {
            let arch = shapes.arch;
            let narrow = emit_for(shapes, RuntimeFeatures::none());
            assert!(
                narrow.contains("__rt_mixed_free_deep_resource_stream:\n"),
                "{arch:?}: the kind 1 stream arm is not gated"
            );
            assert!(
                narrow.contains("__rt_hash_ctx_free"),
                "{arch:?}: the kind 2 hash-context arm is not gated"
            );
            assert!(
                narrow.contains("__rt_mixed_free_deep_box:\n"),
                "{arch:?}: the generic box-free path must survive the gating"
            );
        }
    }

    /// Stream cleanup preserves fd 0, 1, and 2 aliases on every supported architecture.
    #[test]
    fn stream_cleanup_preserves_standard_stream_aliases() {
        for shapes in LADDERS {
            let arch = shapes.arch;
            let narrow = emit_for(shapes, RuntimeFeatures::none());
            match arch {
                Arch::AArch64 => {
                    assert!(narrow.contains("cmp x0, #2"), "{arch:?}");
                    assert!(
                        narrow.contains("b.ls __rt_mixed_free_deep_box"),
                        "{arch:?}"
                    );
                }
                Arch::X86_64 => {
                    assert!(narrow.contains("cmp rdi, 2"), "{arch:?}");
                    assert!(
                        narrow.contains("jbe __rt_mixed_free_deep_box"),
                        "{arch:?}"
                    );
                }
            }
        }
    }

    /// The ladder tests the SAME number the lowering stamps.
    ///
    /// `RuntimeFnId::resource_cleanup_kind` is the one authority: the lowering stamps `stamp()`
    /// into the Mixed high payload word, and `lowered_runtime_features` turns that same answer
    /// into the bit gating the arm here. Nothing else ties this emitter's literal to it, so
    /// renumbering a kind on one side only is caught here rather than by a resource that quietly
    /// stops being released.
    #[test]
    fn each_arm_matches_the_kind_its_producer_stamps() {
        for shapes in LADDERS {
            let arch = shapes.arch;
            let wide = emit_for(shapes, RuntimeFeatures::all());
            for (kind, branch) in [
                (ResourceCleanupKind::PopenPipe, shapes.popen_branch),
                (ResourceCleanupKind::Directory, shapes.dir_branch),
            ] {
                let dispatch = format!("{}{}\n    {}", shapes.compare, kind.stamp(), branch);
                assert!(
                    wide.contains(&dispatch),
                    "{arch:?}: {kind:?} must dispatch on the kind its producer stamps ({}), \
                     not on some other number:\n{wide}",
                    kind.stamp()
                );
            }
        }
    }
}
