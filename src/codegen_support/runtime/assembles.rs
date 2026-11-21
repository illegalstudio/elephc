//! Purpose:
//! Feeds the fully generated runtime to a real assembler, for every target the compiler
//! emits, and fails on the first diagnostic. It answers the one question no text-level
//! audit can: does what we emit actually assemble?
//!
//! Called from:
//! - `cargo test --bin elephc` only. `#[cfg(test)]`-gated at its `mod` declaration.
//!
//! Key details:
//! - WHY THIS EXISTS. The `r14` → scratch migration renamed `movzx r14d, BYTE PTR [...]`
//!   into `movzx rsid, BYTE PTR [...]`. `r14d` is r14's 32-bit view, but rsi's is `esi` —
//!   `rsid` is not a register name at all. Every emitted-text test stayed green (the text
//!   was exactly what they asserted), the host suite stayed green (macOS assembles the
//!   AArch64 runtime, never the x86_64 one), and a container probe stayed green too,
//!   because the broken helper sits behind a feature its small program never enabled.
//!   The first thing to notice was a linux-x86_64 CI shard: `operand size mismatch for
//!   'movzx'`, 41 failed tests, in a job about PostgreSQL.
//! - A TEXT ASSERTION CANNOT CATCH THIS. Asserting the emitted string is asserting our own
//!   spelling back to ourselves. Only an assembler knows that `rsid` is not a register,
//!   that an immediate cannot be a store's source on AArch64, or that a displacement is out
//!   of range. This module borrows one.
//! - ALL FEATURES, EVERY TARGET. A feature-gated family is exactly where an invalid
//!   instruction hides from the shards that do not enable it, which is what happened here.
//! - WHICH ASSEMBLER. clang's integrated assembler handles every target this compiler emits
//!   from any host, so where clang exists the gate covers all of them. It is NOT always
//!   there: `elephc` reaches for clang only on non-macOS Apple targets
//!   (`linker::assembler_command`), everything else goes through `target.assembler_cmd()`,
//!   and the linux CI images that run the non-codegen shards carry no clang at all. This
//!   module claimed otherwise and turned two shards red on the assumption.
//!   So the driver is resolved: `ELEPHC_TEST_ASSEMBLER`, then `clang`, then `cc`, then
//!   `gcc`. A driver that does not understand `-target` cannot cross-assemble, so with one
//!   of those the gate covers the HOST target only and names the targets it skipped.
//!   COVERAGE IS THEN A PROPERTY OF THE MATRIX, NOT OF ONE RUN: the linux-x86_64 shard
//!   checks the x86_64 runtime, the linux-aarch64 shard checks the AArch64 one, and a
//!   developer machine with clang checks all three at once. The skip note goes to the test's
//!   captured stdout, so it shows with `--nocapture` or when something else in the test
//!   fails — it is a developer aid, not a CI warning, because a passing test has no way to
//!   raise one.

use std::io::Write;
use std::process::Command;

use crate::codegen_support::driver_support::generate_runtime_with_features;
use crate::codegen_support::platform::{Arch, Platform, Target};
use crate::codegen_support::runtime_features::RuntimeFeatures;

/// The clang target triple used to assemble each emitted target.
fn clang_triple(target: Target) -> &'static str {
    match (target.platform, target.arch) {
        (Platform::MacOS, Arch::AArch64) => "arm64-apple-macos",
        (Platform::MacOS, Arch::X86_64) => "x86_64-apple-macos",
        (Platform::Linux, Arch::AArch64) => "aarch64-unknown-linux-gnu",
        (Platform::Linux, Arch::X86_64) => "x86_64-unknown-linux-gnu",
        (Platform::Windows, _) => "x86_64-pc-windows-gnu",
    }
}

/// The assembler driver this host offers, and whether it understands `-target`.
///
/// Resolved once: probing four candidates per assembled file would dominate the runtime of
/// a test that otherwise costs milliseconds.
fn assembler_driver() -> Option<(String, bool)> {
    use std::sync::OnceLock;
    static DRIVER: OnceLock<Option<(String, bool)>> = OnceLock::new();
    DRIVER
        .get_or_init(|| {
            let mut candidates: Vec<String> = Vec::new();
            if let Ok(explicit) = std::env::var("ELEPHC_TEST_ASSEMBLER") {
                candidates.push(explicit);
            }
            candidates.extend(["clang", "cc", "gcc"].map(str::to_string));
            for candidate in candidates {
                let Ok(probe) = Command::new(&candidate).arg("--version").output() else {
                    continue;
                };
                if !probe.status.success() {
                    continue;
                }
                // Only clang's integrated assembler takes `-target`; gcc rejects it.
                let banner = String::from_utf8_lossy(&probe.stdout).to_lowercase();
                let cross_capable = banner.contains("clang");
                return Some((candidate, cross_capable));
            }
            None
        })
        .clone()
}

/// The target this test binary is running on, the only one a non-clang driver can assemble.
fn host_target() -> Target {
    let platform = if cfg!(target_os = "macos") {
        Platform::MacOS
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Linux
    };
    let arch = if cfg!(target_arch = "x86_64") { Arch::X86_64 } else { Arch::AArch64 };
    Target::new(platform, arch)
}

/// Assembles `asm` for `target`, returning the assembler's diagnostics on failure.
fn assemble(target: Target, asm: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "elephc_runtime_assembles_{}_{}",
        std::process::id(),
        clang_triple(target).replace('-', "_")
    ));
    std::fs::create_dir_all(&dir).expect("failed to create the assembly scratch directory");
    let source = dir.join("runtime.s");
    let mut file = std::fs::File::create(&source).expect("failed to write the runtime assembly");
    file.write_all(asm.as_bytes())
        .expect("failed to write the runtime assembly");
    drop(file);

    let (driver, cross_capable) =
        assembler_driver().expect("no assembler driver found (tried clang, cc, gcc)");
    let mut command = Command::new(&driver);
    command.arg("-c");
    if cross_capable {
        command.args(["-target", clang_triple(target)]);
    }
    let output = command
        // No `-masm=intel`: every source fed here carries its own `.intel_syntax noprefix`
        // (`emit_text_prelude` for the runtime, written out by hand in the probe below), so
        // the flag bought nothing — and it is an x86-only option that gcc REJECTS outright,
        // which turned the linux-aarch64 shard red with
        // `cc: error: unrecognized command-line option '-masm=intel'`. The comment that used
        // to sit here said the flag was "ignored elsewhere"; that is true of clang only.
        .arg("-o")
        .arg(dir.join("runtime.o"))
        .arg(&source)
        .output()
        .unwrap_or_else(|error| panic!("failed to run the assembler driver {driver}: {error}"));

    let result = if output.status.success() {
        Ok(())
    } else {
        // Keep the source when it failed: the diagnostics carry line numbers into it.
        return Err(format!(
            "assembling the {:?}/{:?} runtime failed (source kept at {}):\n{}",
            target.platform,
            target.arch,
            source.display(),
            String::from_utf8_lossy(&output.stderr),
        ));
    };
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// The generated runtime must assemble for every target, with every feature on.
///
/// One test per target would report only the first failure; this reports all of them,
/// because a mistake in a shared emitter usually breaks more than one.
#[test]
fn the_generated_runtime_assembles_for_every_target() {
    let targets = [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ];
    let (driver, cross_capable) =
        assembler_driver().expect("no assembler driver found (tried clang, cc, gcc)");
    let host = host_target();
    let mut failures: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for target in targets {
        // Without `-target` the driver only knows its own machine. Assembling an AArch64
        // runtime with an x86_64 `cc` reports hundreds of bogus diagnostics, so the honest
        // move is to skip and say so rather than to fail or to pretend it passed.
        if !cross_capable && (target.platform != host.platform || target.arch != host.arch) {
            skipped.push(format!("{:?}/{:?}", target.platform, target.arch));
            continue;
        }
        for ctx_register in [false, true] {
            let features = RuntimeFeatures {
                ctx_register,
                ..RuntimeFeatures::all()
            };
            let asm = generate_runtime_with_features(8 * 1024 * 1024, target, features);
            if target.arch == Arch::X86_64 {
                // Dropping `-masm=intel` is only safe while the source says it itself.
                assert!(
                    asm.starts_with(".intel_syntax noprefix"),
                    "the x86_64 runtime must carry its own syntax directive: no assembler \
                     flag supplies it any more"
                );
            }
            if let Err(diagnostics) = assemble(target, &asm) {
                failures.push(format!("[ctx_register={ctx_register}] {diagnostics}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the generated runtime must assemble:\n{}",
        failures.join("\n\n")
    );
    if !skipped.is_empty() {
        // Printed, not asserted: a machine with only `cc` still gets its host target checked,
        // and the CI matrix covers the others on their own shards. Captured stdout means this
        // is only read with `--nocapture` or beside another failure — enough for a developer
        // wondering what ran, not a substitute for the matrix.
        println!(
            "assembler driver `{driver}` does not take -target, so these were not assembled \
             on this host: {}",
            skipped.join(", ")
        );
    }
}

/// Negative control: the audit must reject an instruction a text assertion would accept.
///
/// `rsid` is the exact shape that shipped — a register suffix that is valid for r8-r15
/// pasted onto a register whose 32-bit view has a different name entirely.
#[test]
fn the_assembler_gate_rejects_an_invalid_register_name() {
    let (_, cross_capable) =
        assembler_driver().expect("no assembler driver found (tried clang, cc, gcc)");
    let host = host_target();
    if !cross_capable && host.arch != Arch::X86_64 {
        // The probe is x86_64 by nature: `rsid` is only a plausible mistake there. On an
        // AArch64 host with no cross-capable driver there is nothing to prove here.
        println!("no cross-capable assembler on an AArch64 host: the x86_64 probe cannot run");
        return;
    }
    let target = Target::new(Platform::Linux, Arch::X86_64);
    // `.intel_syntax noprefix` is what `emit_text_prelude` puts at the top of every x86_64
    // runtime, and the probe needs it for the same reason: without it GNU `as` reads the
    // body as AT&T and rejects it over syntax. The assertion below would still pass — for
    // the wrong reason, on exactly the driver this gate was just taught to fall back to.
    let prelude = ".intel_syntax noprefix\n.text\n.globl __rt_probe\n__rt_probe:\n";
    let broken = format!("{prelude}    movzx rsid, BYTE PTR [rax + r9]\n    ret\n");
    assert!(
        assemble(target, &broken).is_err(),
        "the gate must reject `movzx rsid, …`; if the assembler accepted it, it proves nothing"
    );

    // The paired positive: the same instruction with the register esi actually has. If this
    // one failed, the rejection above would say nothing about register names — it would only
    // mean the probe never assembled at all.
    let sound = format!("{prelude}    movzx esi, BYTE PTR [rax + r9]\n    ret\n");
    assert_eq!(
        assemble(target, &sound),
        Ok(()),
        "`movzx esi, BYTE PTR [rax + r9]` is valid; the rejection above must be about `rsid`, \
         not about the probe failing to assemble for some unrelated reason"
    );
}
