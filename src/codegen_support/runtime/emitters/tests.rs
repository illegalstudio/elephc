//! Purpose:
//! Regression tests for the aggregate runtime-emission surface and its target-specific assembly invariants.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Exercises feature gating, cross-target symbol coverage, and macOS dead-strip label ownership.

use super::*;
use crate::codegen_support::platform::{AppleVariant, Arch, Platform, Target};
use crate::codegen_support::runtime::{arrays, buffers, pointers};

/// Full and native-only eval runtimes define scope APIs once and gate every Rust scope dependency.
#[test]
fn eval_scope_exports_are_unique_and_native_fragments_do_not_require_rust() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for bridge in [false, true] {
            let mut emitter = Emitter::new(target);
            emit_runtime(&mut emitter, RuntimeFeatures {
                eval_scope: true,
                eval_bridge: bridge,
                ..RuntimeFeatures::none()
            });
            let asm = emitter.output();
            for entry in ["__elephc_eval_scope_free", "__elephc_eval_scope_set"] {
                let label = format!("{}:", target.extern_symbol(entry));
                assert_eq!(asm.lines().filter(|line| *line == label).count(), 1,
                    "{name}, bridge={bridge}: duplicate or missing {entry}");
            }
            for entry in ["__elephc_eval_scope_free_v2", "__elephc_eval_scope_set_v2", "__elephc_eval_scope_unset_v2"] {
                assert_eq!(asm.contains(&target.extern_symbol(entry)), bridge,
                    "{name}, bridge={bridge}: wrong dependency on {entry}");
            }
        }
    }
}

/// Independently discovered clone and handler dependencies emit their native helper families.
#[test]
fn generated_wrapper_dependencies_emit_native_runtime_helpers() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit_runtime(&mut emitter, RuntimeFeatures {
            handler_state: true,
            object_clone: true,
            ..RuntimeFeatures::none()
        });
        let asm = emitter.output();
        for symbol in [
            "__rt_object_clone_shallow_boxed",
            "__rt_core_error_handler_pop",
            "__rt_core_exception_handler_pop",
        ] {
            assert!(
                asm.contains(&format!("{symbol}:")),
                "{name}: generated-wrapper runtime is missing {symbol}"
            );
        }
        assert!(
            !asm.contains("__elephc_eval_value_object_clone_shallow"),
            "{name}: native wrapper dependencies must not emit eval bridge exports"
        );
    }
}

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

/// Verifies the shared mbstring helper is omitted when neither AOT nor eval requires it.
#[test]
fn test_runtime_can_gate_mbstring_helpers() {
    let target = Target::new(Platform::MacOS, Arch::AArch64);
    let mut omitted = Emitter::new(target);
    emit_runtime(&mut omitted, RuntimeFeatures::none());
    assert!(!omitted.output().contains("__rt_mbstring_native:"));

    let mut included = Emitter::new(target);
    emit_runtime(
        &mut included,
        RuntimeFeatures {
            mbstring: true,
            ..RuntimeFeatures::none()
        },
    );
    assert!(included.output().contains("__rt_mbstring_native:"));
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


/// `__rt_hash_slice` is emitted for EVERY supported target, and walks the source through the
/// insertion-order iterator rather than addressing it by key.
///
/// The helper exists because `array_slice()`'s `$offset`/`$length` count POSITIONS, which a hash
/// cannot answer without a walk (issue #683). The executable codegen shards run on
/// `macos-aarch64`, `linux-aarch64` and `linux-x86_64`; the two iOS targets are covered by
/// emission tests like this one, so a helper that silently stopped being emitted on one of them
/// would otherwise only surface as a wrong answer on a device.
///
/// The two calls asserted inside the body are the ones that make it a hash slice rather than a
/// clone: `__rt_hash_iter_next` is what gives positions their meaning, and `__rt_hash_new` is
/// what makes the result a table of its own instead of a view into the source.
#[test]
fn test_hash_slice_is_emitted_for_every_supported_target() {
    let targets = [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ];

    for target in targets {
        let mut emitter = Emitter::new(target);
        emit_runtime(&mut emitter, RuntimeFeatures::all());
        let asm = emitter.output();

        assert!(
            asm.contains(".globl __rt_hash_slice\n"),
            "{} did not emit __rt_hash_slice",
            target.as_str()
        );

        // The helper's own body: from its global directive to the next helper's.
        let start = asm
            .find(".globl __rt_hash_slice\n")
            .unwrap_or_else(|| panic!("{}: no __rt_hash_slice body", target.as_str()));
        let rest = &asm[start + ".globl __rt_hash_slice\n".len()..];
        let body = rest.find(".globl ").map_or(rest, |end| &rest[..end]);
        for callee in ["__rt_hash_iter_next", "__rt_hash_new", "__rt_hash_insert_owned"] {
            assert!(
                body.contains(callee),
                "{} must reach {callee} from __rt_hash_slice",
                target.as_str()
            );
        }
    }
}


/// `__rt_array_slice_str` is emitted for EVERY supported target, with the 16-byte slot the
/// string layout needs.
///
/// The helper exists because an indexed `array<string>` stores 16-byte `{pointer, length}` slots
/// while the shared slice helpers copy 8 bytes per element (issue #675). The executable codegen
/// shards run on `macos-aarch64`, `linux-aarch64` and `linux-x86_64`; the two iOS targets are
/// covered by emission tests like this one, so a helper that silently stopped being emitted —
/// or that asked `__rt_array_new` for the wrong slot width on one of them — would otherwise only
/// surface as a wrong answer on a device.
///
/// The slot width is asserted rather than assumed because it is the one number that makes this
/// helper different from its 8-byte siblings: get it wrong and the copy still runs, reading half
/// of each pair.
#[test]
fn test_array_slice_str_is_emitted_for_every_supported_target() {
    let targets = [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ];

    for target in targets {
        let mut emitter = Emitter::new(target);
        emit_runtime(&mut emitter, RuntimeFeatures::all());
        let asm = emitter.output();

        assert!(
            asm.contains(".globl __rt_array_slice_str\n"),
            "{} did not emit __rt_array_slice_str",
            target.as_str()
        );

        // The helper's own body: from its global directive to the next helper's.
        let start = asm
            .find(".globl __rt_array_slice_str\n")
            .unwrap_or_else(|| panic!("{}: no __rt_array_slice_str body", target.as_str()));
        let rest = &asm[start + ".globl __rt_array_slice_str\n".len()..];
        let body = rest.find(".globl ").map_or(rest, |end| &rest[..end]);
        let slot_request = match target.arch {
            Arch::AArch64 => "mov x1, #16",
            Arch::X86_64 => "mov rsi, 16",
        };
        assert!(
            body.contains(slot_request),
            "{} must allocate the slice destination with 16-byte string slots ({slot_request})",
            target.as_str()
        );
        assert!(
            body.contains("__rt_array_push_str"),
            "{} must copy through the persisting string append helper",
            target.as_str()
        );
    }
}
