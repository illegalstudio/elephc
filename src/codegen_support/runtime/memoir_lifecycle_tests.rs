//! Purpose:
//! Freezes assembly for native ownership and output-buffer lifecycles.
//!
//! Called from:
//! - Compiler assembly regression tests.
//!
//! Key details:
//! - The reference was captured before extraction at 1a946d133959a352e6ba76f2c2c1332723f09bcc.
//! - The fingerprints cover internal labels, emitted instructions and buffer storage declarations.

use super::{arrays, data, eval_bridge, eval_scope, exceptions, io, system};
use crate::codegen_support::{emit::Emitter, platform::Target};
use crate::php_version::PhpVersion;
use sha2::{Digest, Sha256};

/// Scope-only eval exposes the same exact-owner visitor ABI as Magician on every target.
#[test]
fn scope_only_eval_visits_exact_owned_slots_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        eval_scope::emit_eval_scope_runtime(&mut emitter);
        let assembly = emitter.output();
        assert!(
            assembly.contains(&target.extern_symbol("__elephc_eval_scope_visit_owners_v1")),
            "{name}: missing visitor entry"
        );
        if target.arch == crate::codegen_support::platform::Arch::AArch64 {
            assert!(assembly.contains("add x0, x19, #24"), "{name}: exact cell slot");
            assert!(assembly.contains("blr x20"), "{name}: visitor callback");
            assert!(assembly.contains("tst x9, #16"), "{name}: owned entries only");
        } else {
            assert!(assembly.contains("lea rdi, [r12 + 24]"), "{name}: exact cell slot");
            assert!(assembly.contains("call r13"), "{name}: visitor callback");
            assert!(assembly.contains("test rax, 16"), "{name}: owned entries only");
        }
    }
}

/// Verifies ownership assembly through the compiler's existing runtime entry points.
#[test]
fn memoir_lifecycles_preserve_native_assembly() {
    let mut fingerprints = Vec::new();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let fixed = data::emit_runtime_data_fixed(262_144, target, PhpVersion::default());
        let (_, output_storage) = fixed.split_once("_ob_level,").unwrap();
        let (output_storage, _) = output_storage.split_once("_ob_handler_stubs,").unwrap();

        for mode in 0..6 {
            let mut emitter = Emitter::new(target);
            emitter.pic_data_refs = mode == 1 || mode == 2;
            emitter.cdylib_boundary = mode == 2 || mode == 3;
            emitter.phpext_output = mode == 4;
            emitter.dead_strip = mode == 5;

            arrays::emit_array_to_mixed(&mut emitter);
            arrays::emit_hash_to_mixed(&mut emitter);
            arrays::emit_array_to_hash(&mut emitter);
            arrays::emit_array_set_refcounted(&mut emitter);
            arrays::emit_hash_unset(&mut emitter);
            exceptions::emit_exception_cleanup_frames(&mut emitter);
            exceptions::emit_cleanup_invoke(&mut emitter);
            eval_bridge::emit_object_clone_shallow_runtime(&mut emitter, true);
            eval_scope::emit_eval_scope_runtime(&mut emitter);
            io::emit_stream_registry(&mut emitter);
            system::emit_unserialize(&mut emitter);
            io::emit_ob_start(&mut emitter);
            io::emit_ob_append(&mut emitter);
            io::emit_ob_contents(&mut emitter);
            io::emit_ob_pop_free(&mut emitter);

            let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
            labels.sort();
            let assembly = emitter.output();
            let hash = Sha256::digest(format!("{assembly}\n{labels:?}\n{output_storage}"));
            fingerprints.push(format!("{name} {mode} {hash:x}"));
        }
    }

    assert_eq!(fingerprints.join("\n"), include_str!("memoir_lifecycle_baseline.txt").trim());
}
