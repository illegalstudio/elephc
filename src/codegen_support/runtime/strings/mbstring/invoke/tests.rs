//! Purpose:
//! Cross-assembles the value, response, and replacement callback adapters on every supported target.
//!
//! Called from:
//! - The compiler library test harness on hosts with a multi-target clang assembler.
//!
//! Key details:
//! - Regex and eval capability states retain correct dispatch and external symbol spelling.

use super::*;
use crate::codegen_support::platform::Target;
use std::process::Command;

/// Assembles the actual coordinator and response callback addresses for all five supported targets.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn mbstring_output_invoke_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-output-invoke-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        for (mbregex, eval) in [(false, false), (false, true), (true, false), (true, true)] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            if emitter.target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
            emitter.raw(".text");
            if mbregex { super::super::callback::emit(&mut emitter, eval); }
            emit(&mut emitter, mbregex);
            let assembly = emitter.output();
            assert!(assembly.contains(&target.extern_symbol("elephc_mbstring_output_invoke_v1")));
            assert!(assembly.contains(&target.extern_symbol("elephc_mbstring_response_info_v1")));
            assert_eq!(assembly.contains("__rt_mbregex_init"), mbregex);
            assert_eq!(assembly.contains(&target.extern_symbol("elephc_mbstring_callback_invoke_v1")), mbregex);
            assert_eq!(assembly.contains(&target.extern_symbol("__elephc_eval_callable_call_array")), mbregex && eval);
            let source = directory.join(format!("{name}-{mbregex}-{eval}.s"));
            std::fs::write(&source, assembly).unwrap();
            let built = Command::new("clang").args(["-target", triple, "-c"]).arg(source)
                .arg("-o").arg(directory.join(format!("{name}-{mbregex}-{eval}.o"))).output().unwrap();
            assert!(built.status.success(), "{name}/{mbregex}/{eval}: {}", String::from_utf8_lossy(&built.stderr));
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
