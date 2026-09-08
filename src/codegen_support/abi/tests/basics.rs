//! Purpose:
//! Contains ABI regression tests for basics helper behavior.
//! Checks emitted assembly fragments rather than running linked programs.
//!
//! Called from:
//! - `crate::codegen_support::abi::tests` through Rust test harness
//!
//! Key details:
//! - Assertions pin register, stack, relocation, and platform-specific instruction choices.

use super::*;

/// Verifies AArch64 truthiness helpers use a short inverse branch followed by a
/// wide-range unconditional branch, avoiding `cbz`/`cbnz` fixup overflows in very
/// large generated functions.
#[test]
fn test_emit_branch_helpers_use_long_range_aarch64_sequence() {
    let mut emitter = test_emitter();
    emit_branch_if_int_result_zero(&mut emitter, "zero_label");
    emit_branch_if_int_result_nonzero(&mut emitter, "nonzero_label");

    assert_eq!(
        emitter.output(),
        concat!(
            "    cbnz x0, __elephc_branch_if_zero_skip_0\n",
            "    b zero_label\n",
            "__elephc_branch_if_zero_skip_0:\n",
            "    cbz x0, __elephc_branch_if_nonzero_skip_1\n",
            "    b nonzero_label\n",
            "__elephc_branch_if_nonzero_skip_1:\n",
        )
    );
}

/// Verifies composable branch helpers cannot capture an enclosing numeric label.
#[test]
fn test_emit_branch_helpers_allocate_unique_labels_when_nested() {
    let mut emitter = test_emitter();
    let outer = emitter.unique_local_label("outer_done");
    emitter.instruction(&format!("b {}", outer));
    emit_branch_if_int_result_zero(&mut emitter, "zero_label");
    emit_branch_if_int_result_zero(&mut emitter, "other_zero_label");
    emitter.label(&outer);

    let output = emitter.output();
    assert!(output.contains("__elephc_outer_done_0"));
    assert!(output.contains("__elephc_branch_if_zero_skip_1"));
    assert!(output.contains("__elephc_branch_if_zero_skip_2"));
    assert!(!output.contains("1f"));
    assert!(!output.contains("\n1:"));
}

/// Verifies known inline/composable emitters do not reintroduce bare numeric labels.
#[test]
fn test_composable_emitters_do_not_use_bare_numeric_labels() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rust_sources(&source_root.join("codegen_support/abi"), &mut files);
    collect_rust_sources(&source_root.join("codegen/lower_inst"), &mut files);
    files.push(source_root.join("codegen/eval_static_property_helpers.rs"));
    files.push(source_root.join("codegen/runtime_callable_invoker.rs"));

    for path in files {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            !contains_bare_numeric_label(&source),
            "composable emitter contains a bare numeric label: {}",
            path.display()
        );
    }
}

/// Detects numeric `Nf`/`Nb` references and numeric `label`/`raw` definitions.
fn contains_bare_numeric_label(source: &str) -> bool {
    if ["label(\"", "raw(\""]
        .iter()
        .any(|prefix| source.match_indices(prefix).any(|(index, prefix)| {
            source[index + prefix.len()..]
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_digit())
        }))
    {
        return true;
    }
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_digit()
            && (index == 0 || !bytes[index - 1].is_ascii_alphanumeric())
        {
            let mut end = index + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end < bytes.len()
                && matches!(bytes[end], b'f' | b'b')
                && (end + 1 == bytes.len() || !bytes[end + 1].is_ascii_alphanumeric())
            {
                return true;
            }
            index = end;
        } else {
            index += 1;
        }
    }
    false
}

/// Recursively collects repo-owned Rust sources below one composable-emitter root.
fn collect_rust_sources(root: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
    for entry in entries {
        let path = entry.expect("directory entry must be readable").path();
        if path.is_dir() {
            if path.file_name().is_none_or(|name| name != "tests") {
                collect_rust_sources(&path, files);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_stem().is_none_or(|name| name != "tests")
        {
            files.push(path);
        }
    }
}

/// Tests frame setup and teardown for a small frame (64 bytes).
/// Verifies that the prologue allocates 64 bytes, saves FP/LR at sp+#48,
/// sets up x29 as the frame pointer, and that restore/return undo this correctly
/// via the frame-pointer-anchored restore (immune to mid-body sp drift).
#[test]
fn test_emit_frame_helpers_small_frame() {
    let mut emitter = test_emitter();
    emit_frame_prologue(&mut emitter, 64);
    emit_frame_restore(&mut emitter, 64);
    emit_return(&mut emitter);

    assert_eq!(
        emitter.output(),
        concat!(
            "    ; prologue\n",
            "    sub sp, sp, #64\n",
            "    stp x29, x30, [sp, #48]\n",
            "    add x29, sp, #48\n",
            "    mov x9, x29\n",
            "    add sp, x9, #16\n",
            "    ldp x29, x30, [x9]\n",
            "    ret\n",
        )
    );
}

/// Verifies the frame prologue rejects a frame too small to hold the x29/x30 footer
/// (`frame_size < 16`) with a clear assertion message in debug builds, instead of
/// underflowing the `frame_size - 16` footer-offset subtraction into a corrupt offset.
#[test]
#[should_panic(expected = "frame_size must reserve the 16-byte frame footer")]
fn test_emit_frame_prologue_rejects_undersized_frame() {
    let mut emitter = test_emitter();
    emit_frame_prologue(&mut emitter, 8);
}

/// Routes process-exit helpers through the active cdylib exception boundary on both targets.
#[test]
fn test_emit_exit_recovers_through_cdylib_boundary() {
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        let mut emitter = Emitter::new_cdylib(target);
        emit_exit(&mut emitter, 7);
        let asm = emitter.output();
        assert!(asm.contains("_elephc_boundary_active"));
        assert!(asm.contains("_elephc_boundary_status"));
        assert!(asm.contains("__rt_throw_current"));
    }
}

/// Tests that string return values (pointer in x1, length in x2) are preserved
/// across function boundaries by storing them to the caller's stack frame at negative
/// offsets and restoring them after the call. Uses offset 32 for both stores.
#[test]
fn test_emit_preserve_and_restore_return_value_for_strings() {
    let mut emitter = test_emitter();
    emit_preserve_return_value(&mut emitter, &PhpType::Str, 32);
    emit_restore_return_value(&mut emitter, &PhpType::Str, 32);

    assert_eq!(
        emitter.output(),
        concat!(
            "    stur x1, [x29, #-32]\n",
            "    stur x2, [x29, #-24]\n",
            "    ldur x1, [x29, #-32]\n",
            "    ldur x2, [x29, #-24]\n",
        )
    );
}
