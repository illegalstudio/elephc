//! Purpose:
//! Verifies startup configuration metadata and native encoding on every supported target.
//!
//! Called from:
//! - Focused compiler library tests with clang cross-assembly support.
//!
//! Key details:
//! - Executable CLI and reused-worker tests separately validate the actual Rust/PCRE2 state.

use super::*;
use crate::codegen_support::platform::Target;
use std::process::Command;

/// Assembles program-owned descriptors and startup callbacks for all five native targets.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn mbstring_startup_assembles_on_all_supported_targets() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-startup-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        let target = Target::parse(name).unwrap();
        let mut module = Module::new(target);
        module.required_runtime_features.mbstring_mime = true;
        module.mbstring_startup = Some(["UTF-8", "SJIS", "ASCII", "mbstring.language", "Japanese"]
            .map(|value| value.as_bytes().to_vec()).into());
        let mut emitter = Emitter::new(target);
        emitter.pic_data_refs = true;
        if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        let mut data = DataSection::new();
        emit(&module, &mut emitter, &mut data);
        let assembly = emitter.output() + &data.emit(target);
        let source = directory.join(format!("{name}.s"));
        std::fs::write(&source, assembly).unwrap();
        let built = Command::new("clang").args(["-target", triple, "-c"]).arg(source)
            .arg("-o").arg(directory.join(format!("{name}.o"))).output().unwrap();
        assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
    }
    std::fs::remove_dir_all(directory).unwrap();
}

/// Emits PCRE2 MIME imports only when output matching or MIME validation selected the provider.
#[test]
fn mbstring_startup_mime_symbols_follow_runtime_capability() {
    let target = Target::detect_host();
    for (enabled, expected) in [(false, false), (true, true)] {
        let mut module = Module::new(target);
        module.required_runtime_features.mbstring_mime = enabled;
        module.mbstring_startup = Some(vec![b"UTF-8".to_vec(); 3]);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        emit(&module, &mut emitter, &mut data);
        let assembly = emitter.output() + &data.emit(target);
        for symbol in ["elephc_pcre2_v1_mime_compile", "elephc_pcre2_v1_mime_match",
            "elephc_pcre2_v1_mime_free", "elephc_pcre2_v1_error_message"] {
            assert_eq!(assembly.contains(symbol), expected, "{symbol} capability={enabled}");
        }
        assert_eq!(assembly.contains("elephc_mbstring_mime_provider_v1"), expected);
        assert!(assembly.contains("elephc_mbstring_configure_v1"));
    }
}
