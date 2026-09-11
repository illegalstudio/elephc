//! Purpose:
//! Cross-assembles header, terminal-write, and output-buffer ownership emitters.
//!
//! Called from:
//! - Focused compiler library checks on hosts with the multi-target clang assembler.
//!
//! Key details:
//! - All five supported targets exercise both web modes and both mbstring capability states.
//! - Native execution tests separately cover response ownership and buffered output behavior.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform, Target}};
use std::process::Command;

/// Assembles each response path while proving disabled capabilities do not name their bridge symbols.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn mbstring_response_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-response-targets-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        for web in [false, true] {
            for mbstring in [false, true] {
                let target = Target::parse(name).unwrap();
                let localize = target.platform == Platform::MacOS;
                let mut emitter = Emitter::new_pic(target);
                emitter.dead_strip = localize;
                if emitter.target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
                emitter.raw(".text");
                super::super::http_response::emit_header(&mut emitter, web, mbstring);
                super::super::stdout_write::emit_stdout_write(&mut emitter, web, mbstring);
                super::super::ob_handler::emit_ob_result_to_bytes(&mut emitter);
                super::super::ob_handler::emit_ob_apply_handler(&mut emitter);
                super::super::ob_handler::emit_ob_invoke_descriptor(&mut emitter);
                super::super::ob_handler::emit_ob_eval_trampoline(&mut emitter);
                super::super::ob_buffer::emit_ob_process_and_write(&mut emitter);
                super::super::ob_buffer::emit_ob_start(&mut emitter);
                super::super::ob_buffer::emit_ob_pop_free(&mut emitter);
                super::super::ob_buffer::emit_ob_get_pop_ops(&mut emitter);
                let internal_labels = emitter.take_internal_labels();
                let assembly = emitter.output();
                let assembly = if localize {
                    crate::codegen_support::emit::localize_internal_labels(
                        &assembly,
                        &internal_labels,
                    )
                } else {
                    assembly
                };
                assert_eq!(assembly.contains("elephc_mbstring_response_header_v1"), mbstring);
                assert_eq!(assembly.contains("elephc_mbstring_response_commit_v1"), mbstring);
                assert_eq!(assembly.contains("elephc_web_header"), web);
                assert_eq!(assembly.contains("elephc_web_write"), web);
                let stem = format!("{name}-web{web}-mb{mbstring}");
                let source = directory.join(format!("{stem}.s"));
                std::fs::write(&source, assembly).unwrap();
                let built = Command::new("clang").args(["-target", triple, "-c"]).arg(source)
                    .arg("-o").arg(directory.join(format!("{stem}.o"))).output().unwrap();
                assert!(built.status.success(), "{stem}: {}", String::from_utf8_lossy(&built.stderr));
            }
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
