//! Purpose:
//! Pins the branch forms and text-section placement of runtime-selected callable dispatch.
//!
//! Called from:
//! - The codegen integration harness through the callables module.
//!
//! Key details:
//! - Inline callback wrappers open their own ELF text section; the caller's section must be
//!   reopened after them so the caller's conditional branches never become cross-section.
//! - AArch64 `b.cond` reaches only ±1 MiB, so edges that skip runtime-name case tables use an
//!   inverted `b.cond` over an unconditional `b`; x86_64 keeps its rel32 `jcc`.

use crate::support::*;
use std::collections::HashMap;
use std::fs;

/// Runtime-selected callables passed to `array_map()`, `array_filter()`, a variable call, and
/// `pcntl_signal()`, which together emit every widened Mixed callable edge outside eval.
const MIXED_CALLABLE_FIXTURE: &str = r#"<?php
class Inv {
    public function __invoke($x) { return "inv:" . $x; }
    public static function st($x) { return "st:" . $x; }
}
function pick(array $pool, int $index): mixed { return $pool[$index]; }
$callback = pick(["strtoupper", new Inv(), ["Inv", "st"], null], count($argv));
echo json_encode(array_map($callback, ["a"])), "\n";
echo json_encode(array_filter(["a"], $callback)), "\n";
echo $callback("z"), "\n";
pcntl_signal(SIGUSR1, $callback);
"#;

/// Label stems of the Mixed callable edges that jump over runtime-name case tables.
const WIDE_EDGE_STEMS: &[&str] = &[
    "array_map_null_callback",
    "array_filter_null_callback",
    "mixed_callable_array",
    "mixed_callable_object",
    "pcntl_signal_mixed_scalar",
    "pcntl_signal_mixed_bool_error",
];

/// Emits the fixture's assembly for one target through the CLI and returns its text.
fn emit_fixture_assembly(target: &str) -> String {
    let dir = make_cli_test_dir("elephc_callable_branch_reach");
    let php_path = dir.join("main.php");
    fs::write(&php_path, MIXED_CALLABLE_FIXTURE).expect("write callable branch fixture");
    let output = elephc_cli_command(&dir)
        .args(["--emit-asm", "--target", target])
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-asm");
    assert!(
        output.status.success(),
        "{target}: emitting assembly failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = fs::read_to_string(php_path.with_extension("s")).expect("read assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// Returns the mnemonic and target label when `line` is a conditional branch on either ISA.
fn conditional_branch(line: &str) -> Option<(&str, &str)> {
    let (mnemonic, operands) = line.trim().split_once(' ')?;
    let conditional = mnemonic.starts_with("b.")
        || matches!(mnemonic, "cbz" | "cbnz" | "tbz" | "tbnz")
        || (mnemonic.starts_with('j') && mnemonic != "jmp");
    let target = operands.rsplit(',').next().unwrap_or(operands).trim();
    conditional.then_some((mnemonic, target))
}

/// Lists the conditional branches whose target label is defined in another text section.
fn cross_section_conditional_branches(assembly: &str) -> Vec<String> {
    let mut section = ".text";
    let mut label_sections = HashMap::new();
    let mut branches = Vec::new();
    for line in assembly.lines() {
        if let Some(directive) = line.trim().strip_prefix(".section ") {
            section = directive.split(',').next().unwrap_or(directive).trim();
        } else if line.trim() == ".text" {
            section = ".text";
        } else if let Some(label) = line.strip_suffix(':').filter(|label| !label.starts_with(' ')) {
            label_sections.insert(label, section);
        } else if let Some((mnemonic, target)) = conditional_branch(line) {
            branches.push((section, mnemonic, target));
        }
    }
    branches
        .into_iter()
        .filter_map(|(section, mnemonic, target)| {
            let target_section = label_sections.get(target)?;
            (*target_section != section)
                .then(|| format!("{mnemonic} {target} [{section} -> {target_section}]"))
        })
        .collect()
}

/// Returns true when `label` is one minted label of `stem`, such as `.L_eir_main_<stem>_12`.
fn is_stem_label(label: &str, stem: &str) -> bool {
    label
        .rsplit_once('_')
        .is_some_and(|(head, id)| id.bytes().all(|b| b.is_ascii_digit()) && head.ends_with(stem))
}

/// Verifies inline callback wrappers leave the caller's conditional branches in its section.
///
/// The tail of a function that emitted an `array_map()` descriptor wrapper used to continue
/// inside the wrapper's own `.text.<wrapper>` section, turning the caller's `b.eq`/`je` edges
/// into that tail into cross-section relocations. On linux-aarch64 GNU ld placed one of them
/// more than 1 MiB away and failed with `R_AARCH64_CONDBR19`.
#[test]
fn test_inline_callback_wrappers_keep_conditional_branches_in_the_callers_section() {
    for target in ["linux-aarch64", "linux-x86_64"] {
        let assembly = emit_fixture_assembly(target);
        let crossing = cross_section_conditional_branches(&assembly);
        assert!(
            crossing.is_empty(),
            "{target}: cross-section conditional branches:\n{}",
            crossing.join("\n")
        );
    }
}

/// Verifies the Mixed callable edges over runtime-name case tables are widened on AArch64.
///
/// The code those edges skip grows with the program's callable names, so a ±1 MiB `b.eq` there
/// stops linking in large programs (#1444). x86_64 keeps its rel32 `je`.
#[test]
fn test_mixed_callable_edges_over_case_tables_use_wide_aarch64_branches() {
    for target in ["linux-aarch64", "macos-aarch64", "linux-x86_64"] {
        let assembly = emit_fixture_assembly(target);
        let lines = assembly.lines().map(str::trim).collect::<Vec<_>>();
        for stem in WIDE_EDGE_STEMS {
            let reaches = |line: &str, mnemonic: &str| {
                line.strip_prefix(mnemonic).is_some_and(|label| is_stem_label(label, stem))
            };
            if target.ends_with("x86_64") {
                assert!(lines.iter().any(|line| reaches(line, "je ")), "{target}: no je to {stem}");
                continue;
            }
            assert!(
                !lines.iter().any(|line| reaches(line, "b.eq ")),
                "{target}: near b.eq to {stem}"
            );
            assert!(
                lines.windows(2).any(|pair| pair[0] == "b.ne 1f" && reaches(pair[1], "b ")),
                "{target}: no inverted b.ne over an unconditional b to {stem}"
            );
        }
    }
}
