//! Purpose:
//! Regression tests for the aggregate runtime-emission surface and its target-specific assembly invariants.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Exercises feature gating, cross-target symbol coverage, and macOS dead-strip label ownership.

use super::*;
use crate::codegen_support::platform::{Arch, Platform, Target};
use crate::codegen_support::runtime::{arrays, buffers, pointers};

/// Verifies that AArch64 runtime emits fiber routines.
#[test]
fn test_aarch64_runtime_emits_fiber_routines() {
    let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
    emit_runtime(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();

    for sym in [
        "__rt_fiber_alloc_stack",
        "__rt_fiber_free_stack",
        "__rt_fiber_switch",
        "__rt_fiber_entry",
        "__rt_fiber_construct",
        "__rt_fiber_start",
        "__rt_fiber_resume",
        "__rt_fiber_suspend",
        "__rt_fiber_throw",
        "__rt_fiber_get_current",
        "__rt_fiber_get_return",
        "__rt_fiber_state_eq",
    ] {
        assert!(
            asm.contains(&format!(".globl {}\n", sym)),
            "fiber runtime missing global symbol {}",
            sym
        );
    }
}

/// Verifies optional regex helpers are omitted when the program does not reference them.
#[test]
fn test_runtime_can_omit_regex_helpers() {
    let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
    emit_runtime(&mut emitter, RuntimeFeatures::none());
    let asm = emitter.output();

    assert!(!asm.contains("__rt_preg_match:"));
    assert!(!asm.contains("__rt_preg_replace:"));
    assert!(!asm.contains("__rt_preg_split:"));
}

/// Verifies the iconv-backed `mb_strlen()` helper is emitted only for programs that use it.
#[test]
fn test_runtime_can_gate_mb_strlen_helper() {
    let target = Target::new(Platform::MacOS, Arch::AArch64);
    let mut omitted = Emitter::new(target);
    emit_runtime(&mut omitted, RuntimeFeatures::none());
    assert!(!omitted.output().contains("__rt_mb_strlen:"));

    let mut included = Emitter::new(target);
    emit_runtime(
        &mut included,
        RuntimeFeatures {
            mb_strlen: true,
            ..RuntimeFeatures::none()
        },
    );
    assert!(included.output().contains("__rt_mb_strlen:"));
}

/// Verifies that Linux x86_64 uses the shared runtime surface.
#[test]
fn test_linux_x86_64_runtime_uses_shared_surface() {
    let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
    emit_runtime(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();

    for sym in [
        "__rt_hash_count",
        "__rt_gc_note_child_ref",
        "__rt_incref",
        "__rt_decref_array",
        "__rt_json_encode_assoc",
        "__rt_preg_match",
        "__rt_fiber_alloc_stack",
    ] {
        assert!(
            asm.contains(&format!(".globl {}\n", sym)),
            "linux x86_64 shared runtime missing global symbol {}",
            sym
        );
    }
}

/// Verifies the linux x86_64 runtime ASSEMBLES, not merely that it contains the right symbols.
///
/// Its AArch64 twin (`test_macos_dead_strip_runtime_assembles`) has existed for a while; this side
/// had only a symbol-presence check, so a malformed x86_64 instruction — a wrong operand size, an
/// addressing form the assembler rejects — reached CI before anything noticed. Everything in the
/// x86_64 arm of a two-target helper is written blind on an Apple machine, which is exactly the
/// code that needs an assembler to look at it.
///
/// Skipped when no cross-assembler is available, so it never fails for the wrong reason.
#[test]
fn test_linux_x86_64_runtime_assembles() {
    let asm = crate::codegen_support::generate_runtime_with_features_pic(
        8 * 1024 * 1024,
        Target::new(Platform::Linux, Arch::X86_64),
        RuntimeFeatures::all(),
        true,
    );

    let dir = std::env::temp_dir().join(format!("elephc_x86_asm_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let asm_path = dir.join("runtime.s");
    let obj_path = dir.join("runtime.o");
    std::fs::write(&asm_path, &asm).expect("write asm");

    let output = std::process::Command::new("clang")
        .args(["--target=x86_64-unknown-linux-gnu", "-c", "-o"])
        .arg(&obj_path)
        .arg(&asm_path)
        .output();
    let _ = std::fs::remove_dir_all(&dir);

    let Ok(output) = output else {
        eprintln!("no clang to cross-assemble with; skipping");
        return;
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && stderr.contains("unknown target triple") {
        eprintln!("clang cannot target x86_64-linux here; skipping");
        return;
    }
    assert!(
        output.status.success(),
        "linux x86_64 runtime failed to assemble:\n{}",
        stderr
    );
}

/// Every process-fatal buffer, pointer-null, and container-capacity helper named by
/// cdylib safety review must unwind an active boundary on all supported targets.
#[test]
fn test_remaining_runtime_fatals_escape_cdylib_boundaries() {
    let fatal_emitters: [(&str, fn(&mut Emitter)); 7] = [
        ("buffer bounds", buffers::emit_buffer_bounds_fail),
        ("buffer allocation size", buffers::emit_buffer_new),
        ("buffer registry exhaustion", buffers::emit_buffer_registry_fail),
        ("buffer use-after-free", buffers::emit_buffer_use_after_free),
        ("pointer null", pointers::emit_ptr_check_nonnull),
        ("array capacity", arrays::emit_array_new),
        ("hash capacity", arrays::emit_hash_new),
    ];
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        for (name, emit) in fatal_emitters {
            let mut emitter = Emitter::new_cdylib(target);
            emit(&mut emitter);
            let asm = emitter.output();
            assert!(
                asm.contains(crate::codegen_support::cdylib::BOUNDARY_ACTIVE),
                "{name} omits the active-boundary check on {target:?}:\n{asm}"
            );
            assert!(
                asm.contains(crate::codegen_support::cdylib::BOUNDARY_STATUS),
                "{name} omits runtime-failure status on {target:?}:\n{asm}"
            );
            assert!(
                asm.contains("__rt_throw_current"),
                "{name} cannot unwind to the host boundary on {target:?}:\n{asm}"
            );
        }
    }
}

/// Verifies PDO Tier-D callback adapters are emitted under `pdo_udf` on both targets.
#[test]
fn test_runtime_emits_pdo_call_collation_when_pdo_udf() {
    for (platform, arch) in [
        (Platform::MacOS, Arch::AArch64),
        (Platform::Linux, Arch::X86_64),
    ] {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_runtime(&mut emitter, RuntimeFeatures::all());
        let asm = emitter.output();
        for sym in [
            "__rt_pdo_call_collation",
            "__rt_pdo_call_scalar",
            "__rt_pdo_call_agg_step",
            "__rt_pdo_call_agg_final",
        ] {
            assert!(
                asm.contains(&format!(".globl {}\n", sym)),
                "pdo_udf runtime missing {} for {:?}/{:?}",
                sym,
                platform,
                arch
            );
        }
    }
}

/// Verifies PDO Tier-D adapters are omitted when `pdo_udf` is not requested.
#[test]
fn test_runtime_omits_pdo_call_collation_without_pdo_udf() {
    let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
    emit_runtime(&mut emitter, RuntimeFeatures::none());
    let asm = emitter.output();
    assert!(!asm.contains("__rt_pdo_call_collation:"));
    assert!(!asm.contains(".globl __rt_pdo_call_collation\n"));
    assert!(!asm.contains("__rt_pdo_call_scalar:"));
    assert!(!asm.contains(".globl __rt_pdo_call_scalar\n"));
    assert!(!asm.contains(".globl __rt_pdo_call_agg_step\n"));
    assert!(!asm.contains(".globl __rt_pdo_call_agg_final\n"));
}

/// Verifies the full macOS AArch64 runtime still assembles once per-symbol
/// dead stripping is enabled. The real codegen path renames internal labels
/// to `L`-locals and appends a `.subsections_via_symbols` footer; under that
/// mode the Mach-O assembler rejects any conditional branch whose target is
/// another atom (another helper) or a non-local label. Assembling the
/// all-features runtime catches every such cross-helper conditional branch
/// at build time rather than letting it slip into a miscompiled binary.
#[test]
#[cfg(target_os = "macos")]
fn test_macos_dead_strip_runtime_assembles() {
    // Use the real runtime generation path (pic = false → macOS executable),
    // so the assembly is exactly what is linked, including label localization.
    let asm = crate::codegen_support::generate_runtime_with_features_pic(
        8 * 1024 * 1024,
        Target::new(Platform::MacOS, Arch::AArch64),
        RuntimeFeatures::all(),
        false,
    );

    let dir = std::env::temp_dir().join(format!(
        "elephc_deadstrip_asm_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let asm_path = dir.join("runtime.s");
    let obj_path = dir.join("runtime.o");
    std::fs::write(&asm_path, &asm).expect("write asm");

    let output = std::process::Command::new("as")
        .args(["-arch", "arm64", "-o"])
        .arg(&obj_path)
        .arg(&asm_path)
        .output()
        .expect("run as");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "macOS dead-strip runtime failed to assemble:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Guards the atom invariant the assemble-only test cannot see: under macOS
/// `-dead_strip` an internal helper label is renamed to an `L`-local, which
/// is not a symbol, so a reference to it from *another* atom (helper) is not
/// a relocation the linker can follow. The target atom is then stripped even
/// though a live atom still branches into it, miscompiling silently — this is
/// the bug that made `foreach` over an associative array crash in
/// `__rt_mixed_unbox`. A cross-helper helper must instead use `label_shared`
/// (`.alt_entry`) so it stays a real symbol inside its atom.
///
/// This parses the real dead-strip runtime and asserts every `L__rt_*`
/// reference resolves within its defining atom. `.alt_entry` labels stay bare
/// (not `L`-localized) so they are correctly excluded; numeric local labels
/// never start an atom and are ignored.
#[test]
#[cfg(target_os = "macos")]
fn test_macos_dead_strip_no_cross_atom_internal_refs() {
    let asm = crate::codegen_support::generate_runtime_with_features_pic(
        8 * 1024 * 1024,
        Target::new(Platform::MacOS, Arch::AArch64),
        RuntimeFeatures::all(),
        false,
    );

    // A token is an internal helper label iff it is an `L`-localized `__rt_*`
    // name (what `label()` produces under dead stripping). `.alt_entry`
    // helpers stay bare `__rt_*`, so they never match here.
    /// Returns whether an assembly token names a dead-strip-local runtime helper.
    fn is_internal(tok: &str) -> bool {
        tok.starts_with("L__rt_")
    }
    // True when `s` is a bare label definition body (no whitespace, label
    // characters only, not purely numeric → not an assembler-local `N:`).
    /// Returns whether a token can be a non-numeric assembly label definition.
    fn is_label_name(s: &str) -> bool {
        !s.is_empty()
            && !s.bytes().all(|b| b.is_ascii_digit())
            && s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'.'))
    }

    let mut current_atom: &str = "<root>";
    let mut prev_alt_entry: Option<&str> = None;
    let mut owner: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut refs: Vec<(&str, &str)> = Vec::new();

    for raw in asm.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(rest) = line.strip_prefix(".alt_entry ") {
            prev_alt_entry = Some(rest.trim());
            continue;
        }
        // Label definition: a single `name:` token on the line.
        if let Some(name) = line.strip_suffix(':') {
            if is_label_name(name) {
                if is_internal(name) {
                    owner.insert(name, current_atom);
                } else if prev_alt_entry != Some(name) {
                    // A real global symbol starts a new atom; an `.alt_entry`
                    // label stays inside the current atom (not a boundary).
                    current_atom = name;
                }
            }
            prev_alt_entry = None;
            continue;
        }
        prev_alt_entry = None;
        // Reference scan: collect `L__rt_*` tokens used as operands.
        for tok in line
            .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.')))
        {
            if is_internal(tok) {
                refs.push((current_atom, tok));
            }
        }
    }

    let mut violations: Vec<String> = refs
        .iter()
        .filter_map(|(atom, tok)| match owner.get(tok) {
            Some(def_atom) if def_atom != atom => {
                Some(format!("{tok} defined in {def_atom} but referenced from {atom}"))
            }
            _ => None,
        })
        .collect();
    violations.sort();
    violations.dedup();
    assert!(
        violations.is_empty(),
        "cross-atom references to internal `__rt_*` labels would be stripped \
         under -dead_strip (use label_shared/.alt_entry for cross-helper \
         targets):\n{}",
        violations.join("\n")
    );
}
