//! Purpose:
//! Assembles checked scalar cast helpers across the complete supported target matrix.
//!
//! Called from:
//! - Focused compiler runtime tests with the multi-target clang assembler.
//!
//! Key details:
//! - PIC and dead-stripped Apple atoms exercise conditional failure edges and message relocation.

use super::*;
use crate::codegen_support::{emit::localize_internal_labels, platform::{Platform, Target}};
use std::{fs, path::PathBuf, process::Command};

/// Owns only this invocation's assembly and object files.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Retires generated inputs and objects even when an assertion fails.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Checked casts assemble with every target's ABI and all supported relocation/atom layouts.
#[test]
fn strict_checked_casts_runtime_assembles_all_targets_and_relocations() {
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("clang unavailable: checked cast cross-assembly skipped"); return;
    }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!("elephc-cast-assembly-{}-{nonce}", std::process::id())));
    fs::create_dir(&scratch.0).unwrap();
    for (name, triple) in [("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("linux-x86_64", "x86_64-linux-gnu")] {
        let target = Target::parse(name).unwrap();
        for pic in [false, true] {
            for dead_strip in [false, true] {
                let mut emitter = Emitter::new(target);
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
                emitter.pic_data_refs = pic; emitter.dead_strip = dead_strip;
                if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
                emitter.raw(&data(emitter.semantic_profile)); emitter.raw(".text"); emit(&mut emitter);
                if target.platform == Platform::MacOS && dead_strip { emitter.raw(".subsections_via_symbols"); }
                let labels = emitter.take_internal_labels();
                let assembly = localize_internal_labels(&emitter.output(), &labels);
                let source = scratch.0.join(format!("{name}-{pic}-{dead_strip}.s"));
                fs::write(&source, assembly).unwrap();
                let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                    .arg("-o").arg(source.with_extension("o")).output().unwrap();
                assert!(output.status.success(), "{name} PIC={pic} dead_strip={dead_strip}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}
