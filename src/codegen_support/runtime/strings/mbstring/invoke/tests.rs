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

/// Pins the mail-only V8 extension without changing ordinary V3 dispatch.
#[test]
fn mbstring_mail_uses_timezone_date_host() {
    for name in ["linux-x86_64", "windows-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit(&mut emitter, false, true);
        let assembly = emitter.output();
        assert!(assembly.contains("652835029000"), "{name}: missing V8/152-byte host header");
        assert!(assembly.contains("__rt_mbstring_mail_date"), "{name}: missing mail Date callback");
        assert!(assembly.contains("#144]") || assembly.contains("+ 144]"),
            "{name}: callback must occupy the reviewed V8 offset");
    }
}

/// Ensures generic, callable, output, and INI callbacks all pass through C ABI thunks.
#[test]
fn windows_generic_hosts_publish_c_abi_adapters() {
    let mut emitter = Emitter::new(Target::parse("windows-x86_64").unwrap());
    emit(&mut emitter, true, true);
    let assembly = emitter.output();
    for name in ["__rt_mbstring_cabi_0", "__rt_mbstring_cabi_9",
        "__rt_mbstring_callback_resolve_cabi", "__rt_mbstring_callback_call_cabi",
        "__rt_mbstring_output_header_cabi"] {
        let body = assembly.split(&format!("{name}:\n")).nth(1).expect("C ABI callback adapter");
        let entry = body.split(&format!("{name}__internal:")).next().unwrap();
        assert!(entry.contains("mov rdi, rcx"), "{name}: missing MSx64 adapter");
    }
}

/// Ordinary mbstring code must not introduce references to disabled mbregex callbacks.
#[test]
fn mbstring_without_regex_omits_callable_callback_adapters() {
    for name in ["macos-aarch64", "linux-x86_64", "windows-x86_64"] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        emit(&mut emitter, false, false);
        let assembly = emitter.output();
        assert!(!assembly.contains("__rt_mbstring_callback_resolve"), "{name}");
        assert!(!assembly.contains("__rt_mbstring_callback_call"), "{name}");
        assert!(assembly.contains("__rt_mbstring_output_header_cabi"), "{name}");
        assert!(assembly.contains("__rt_mbstring_ini_input_cabi"), "{name}");
    }
}

#[test]
fn windows_mbstring_coordinators_use_native_bridge_calls() {
    let mut emitter = Emitter::new(Target::parse("windows-x86_64").unwrap());
    emit(&mut emitter, true, true);
    let assembly = emitter.output();
    for symbol in ["elephc_mbstring_invoke_v1", "elephc_mbstring_output_invoke_v1",
        "elephc_mbstring_callback_invoke_v1", "elephc_mbstring_release_v1"] {
        assert!(assembly.contains(&format!("lea r11, [rip + {symbol}]")), "{symbol}");
        assert!(!assembly.contains(&format!("call {symbol}")), "{symbol}");
    }
    assert!(assembly.contains("mov QWORD PTR [rsp + 32], r8"));
    assert!(assembly.contains("mov QWORD PTR [rsp + 40], r9"));
}

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
            emit(&mut emitter, mbregex, true);
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
