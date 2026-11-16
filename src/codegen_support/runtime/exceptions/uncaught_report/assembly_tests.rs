//! Purpose:
//! Assembles exception reporting entries across supported targets and relocation modes.
//!
//! Called from:
//! - Focused runtime emitter library tests.
//!
//! Key details:
//! - Mach-O conditional branches cannot cross global entry boundaries.
//! - Real assembly covers direct/PIC references and dead-stripping label localization.

use super::*;
use crate::codegen_support::{emit::localize_internal_labels, platform::{Platform, Target}};
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Owns only the generated assembly/object directory for this test invocation.
struct Scratch(PathBuf);

impl Drop for Scratch {
    /// Retires this test's assembly and objects on success and assertion failure.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Every report entry assembles, including Apple's cross-entry branch restrictions.
#[test]
fn debuginfo_report_assembles_on_all_targets_and_relocation_modes() {
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("clang unavailable: cross-assembly check skipped");
        return;
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!("elephc-report-assembly-{}-{nonce}", std::process::id())));
    fs::create_dir(&scratch.0).unwrap();
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13.0"),
        ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"),
        ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        let target = Target::parse(name).unwrap();
        for pic in [false, true] {
            for dead_strip in [false, true] {
                for boundary in [false, true] {
                    let mut emitter = Emitter::new(target);
                    emitter.pic_data_refs = pic;
                    emitter.dead_strip = dead_strip;
                    emitter.cdylib_boundary = boundary;
                    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
                    emitter.raw(".text");
                    emit_report_uncaught_exception(&mut emitter);
                    if target.platform == Platform::MacOS && dead_strip {
                        emitter.raw(".subsections_via_symbols");
                    }
                    let labels = emitter.take_internal_labels();
                    let assembly = localize_internal_labels(&emitter.output(), &labels);
                    let source = scratch.0.join(format!("{name}-{pic}-{dead_strip}-{boundary}.s"));
                    fs::write(&source, assembly).unwrap();
                    let output = Command::new("clang").args(["-target", triple, "-c"])
                        .arg(&source).arg("-o").arg(source.with_extension("o")).output().unwrap();
                    assert!(output.status.success(), "{name}, PIC={pic}, dead_strip={dead_strip}, boundary={boundary}: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
        }
    }
}
