//! Purpose:
//! Verifies case-insensitive user callable lookup and its target assembly.
//!
//! Called from:
//! - Explicit focused runtime assembler validation.
//!
//! Key details:
//! - Comparisons preserve parked table entries and branch on strcmp-style nonzero mismatch.

use super::*;
use crate::codegen_support::platform::Target;

/// Every runtime backend accepts the callable lookup loop with its noncoercive name comparison.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn strict_structural_callable_runtime_assembles_on_every_target() {
    use std::{io::Write, process::{Command, Stdio}};
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit_is_callable_runtime(&mut emitter);
        let output = emitter.output();
        let suffix = if target.arch == Arch::X86_64 { "_x86_64" } else { "" };
        let start = output.find(&format!("__rt_is_callable_string_user_loop{suffix}:" )).unwrap();
        let end = output[start..].find(&format!("__rt_is_callable_string_user_next{suffix}:" )).unwrap()+start;
        let lookup = &output[start..end];
        assert!(lookup.contains("__rt_strcasecmp"), "{name}");
        assert!(!lookup.contains("__rt_str_eq"), "{name}");
        let branch = if target.arch == Arch::X86_64 { "jne __rt_is_callable_string_user_next_x86_64" }
            else { "cbnz x0, __rt_is_callable_string_user_next" };
        assert!(lookup.contains(branch), "{name}");
        let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
        let assembly = format!("{syntax}.text\n{output}");
        let mut child = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
            .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang is required for target validation");
        child.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
