//! Purpose:
//! Verifies candidate discovery and collection assembly on every supported ABI.
//!
//! Called from:
//! - Focused compiler library tests.
//!
//! Key details:
//! - Cross-assembly catches instruction, relocation and frame-layout errors.
//! - Darwin cases include localized labels and dead-stripping alternate entries.

use super::*;
use crate::codegen_support::{emit::localize_internal_labels, platform::{Platform, Target}};
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Removes only the test-owned assembly directory after success or an assertion failure.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Removes assembly and objects owned by this test.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Every supported target can assemble the complete candidate and destructor boundary together.
#[test]
fn candidate_gc_subgraphs_assemble_on_all_supported_targets() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!("candidate-gc-{}-{nonce}", std::process::id())));
    fs::create_dir(&scratch.0).unwrap();
    let clang = Command::new("clang").arg("--version").output().is_ok();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13.0"),
        ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        for pic in [false, true] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emitter.pic_data_refs = pic;
            emitter.dead_strip = target.platform == Platform::MacOS;
            if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
            emitter.raw(".text");
            emit(&mut emitter);
            super::super::emit_heap_alloc(&mut emitter);
            super::super::emit_heap_free(&mut emitter, false, false);
            super::super::emit_heap_debug_validate_free_list(&mut emitter);
            super::super::emit_gc_collect_cycles(&mut emitter);
            super::super::emit_gc_mark_reachable(&mut emitter);
            super::super::emit_gc_note_child_ref(&mut emitter);
            super::super::emit_gc_destructors(&mut emitter);
            super::super::emit_gc_eval_object_children(&mut emitter);
            super::super::emit_gc_callable_children(&mut emitter);
            super::super::emit_gc_object_children(&mut emitter);
            super::super::emit_gc_frame_children(&mut emitter);
            super::super::emit_gc_frame_slots(&mut emitter);
            crate::codegen_support::runtime::warning_dispatch::emit_warning_dispatch(&mut emitter);
            crate::codegen_support::runtime::error_handlers::emit_error_handler_invoke(&mut emitter);
            crate::codegen_support::runtime::fibers::force_close::emit(&mut emitter);
            crate::codegen_support::runtime::fibers::emit_fiber_entry(&mut emitter);
            crate::codegen_support::runtime::fibers::emit_fiber_start(&mut emitter);
            crate::codegen_support::runtime::fibers::emit_fiber_resume(&mut emitter);
            crate::codegen_support::runtime::fibers::emit_fiber_throw(&mut emitter);
            crate::codegen_support::runtime::fibers::emit_fiber_suspend(&mut emitter);
            crate::codegen_support::runtime::reference_cells::emit_reference_cells(&mut emitter);
            crate::codegen_support::runtime::callables::emit_callable_descriptor_release(&mut emitter);
            crate::codegen_support::callable_descriptor::emit_initialize_runtime_descriptor(
                &mut emitter, if target.arch == Arch::AArch64 { "x0" } else { "rax" });
            let labels = emitter.take_internal_labels();
            let assembly = localize_internal_labels(&emitter.output(), &labels);
            let mut definitions = std::collections::HashSet::new();
            for label in assembly.lines().filter_map(|line| line.trim().strip_suffix(':')) {
                if label.chars().all(|ch| ch.is_ascii_digit()) { continue; }
                assert!(definitions.insert(label), "{name}, PIC={pic}: duplicate {label}");
            }
            assert!(assembly.contains("__rt_gc_candidate_live:"));
            assert!(assembly.contains("__rt_gc_candidate_root:"));
            assert!(assembly.contains("__rt_gc_visit_children:"));
            if clang {
                let path = scratch.0.join(format!("{name}-{pic}.s"));
                fs::write(&path, &assembly).unwrap();
                let output = Command::new("clang").args(["-target", triple, "-c"])
                    .arg(&path).arg("-o").arg(path.with_extension("o")).output().unwrap();
                assert!(output.status.success(), "{name}, PIC={pic}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}
