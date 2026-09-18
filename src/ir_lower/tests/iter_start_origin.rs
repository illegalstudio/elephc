//! Purpose:
//! Checks the `IterStart` origin metadata that lets `IterNext` reload a relocated
//! by-reference `foreach` source instead of walking the table captured at loop entry.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - A by-reference `foreach` over a simple array variable records the promoted local slot
//!   as the iterator origin; nested and static sources use the synthetic alias that owns their
//!   relocated storage, while direct instance properties keep their fetch-for-write path.
//! - `lower_source_at_for_target` validates the module it returns, so these tests also pin the
//!   validator rule that an origin slot may only appear on a by-reference start.
//! - All five supported targets are checked, and the emitted assembly must carry the table
//!   snapshot pointer compare, its fast-path branch, the heap-kind gate and the resync call on
//!   each of them.

use crate::codegen::platform::Target;
use crate::ir::{Immediate, Op};
use std::path::Path;

/// Every supported target name, so the metadata cannot regress to a single-architecture port.
const SUPPORTED_TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Returns the `IterStart` immediates lowered from one program for one target.
fn iter_start_immediates(source: &str, target: &str) -> Vec<(bool, bool)> {
    let module = super::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::parse(target).unwrap(),
    );
    module
        .functions
        .iter()
        .flat_map(|function| &function.instructions)
        .filter(|inst| inst.op == Op::IterStart)
        .filter_map(|inst| match inst.immediate.as_ref() {
            Some(Immediate::IterStart(metadata)) => {
                Some((metadata.is_by_ref(), metadata.origin().is_some()))
            }
            _ => None,
        })
        .collect()
}

/// A by-reference `foreach` over a simple local records that local as the iterator origin.
#[test]
fn by_ref_foreach_over_a_simple_local_records_its_origin_slot_on_every_target() {
    let source = r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
    $a[] = 4;
    $v = $v + 1;
}
unset($v);
echo count($a);
"#;
    for name in SUPPORTED_TARGETS {
        let starts = iter_start_immediates(source, name);
        assert_eq!(starts.len(), 1, "{name}");
        assert_eq!(starts[0], (true, true), "{name}: missing by-ref origin slot");
    }
}

/// A by-value `foreach` never records an origin, so its iterator keeps the captured source.
#[test]
fn by_value_foreach_records_no_origin_slot_on_every_target() {
    let source = r#"<?php
$a = [1, 2, 3];
foreach ($a as $v) {
    echo $v;
}
"#;
    for name in SUPPORTED_TARGETS {
        let starts = iter_start_immediates(source, name);
        assert_eq!(starts.len(), 1, "{name}");
        assert_eq!(starts[0], (false, false), "{name}: by-value start must not carry an origin");
    }
}

/// A by-reference `foreach` over an array element follows relocation through a synthetic origin.
#[test]
fn by_ref_foreach_over_an_element_source_records_synthetic_origin_on_every_target() {
    let source = r#"<?php
$outer = ["inner" => [1, 2, 3]];
foreach ($outer["inner"] as &$v) {
    $v = $v + 1;
}
unset($v);
echo count($outer["inner"]);
"#;
    for name in SUPPORTED_TARGETS {
        let starts = iter_start_immediates(source, name);
        assert_eq!(starts.len(), 1, "{name}");
        assert_eq!(starts[0], (true, true), "{name}: element sources need a synthetic origin local");
    }
}

/// The recorded origin drives real code: the emitted advance reloads, compares and resyncs.
///
/// The fast path has to be a pointer compare, so the test pins the compare and the branch that
/// skips every recovery step, not just the presence of the resync symbol. It also pins that the
/// resync is gated on the live container still being associative storage, since an indexed
/// cursor stays valid across reallocation and must not be rebuilt from a key.
#[test]
fn by_ref_foreach_emits_the_snapshot_fast_path_and_resync_on_every_target() {
    let source = r#"<?php
$a = [1, 2, 3];
foreach ($a as &$v) {
    $a[] = 4;
    $v = $v + 1;
}
unset($v);
echo count($a);
"#;
    for name in SUPPORTED_TARGETS {
        let target = Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            target,
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        // The hot path: compare the live container against the snapshot and skip everything else.
        // The branch target is matched by mnemonic rather than by label text, because macOS dead
        // stripping rewrites internal label names.
        let (compare, branch, hash_gate, indexed_gate) = match target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                ("cmp x9, x11", "b.eq", "cmp x0, #3", "cmp x0, #2")
            }
            crate::codegen::platform::Arch::X86_64 => {
                ("cmp r11, r10", "je", "cmp rax, 3", "cmp rax, 2")
            }
        };
        let compare_at = asm
            .find(compare)
            .unwrap_or_else(|| panic!("{name}: missing snapshot pointer compare"));
        let after_compare = &asm[compare_at..];
        let window = &after_compare[..after_compare.len().min(240)];
        assert!(
            window.contains(branch),
            "{name}: the snapshot compare is not followed by its fast-path branch"
        );
        assert!(asm.contains("iter_source_stable"), "{name}: missing fast-path label");
        assert!(asm.contains(hash_gate), "{name}: resync must be gated on associative storage");
        assert!(
            asm.contains(indexed_gate),
            "{name}: an indexed replacement must keep its positional cursor"
        );
        assert!(asm.contains("__rt_heap_kind"), "{name}: missing live container classification");
        assert!(asm.contains("__rt_hash_iter_resync"), "{name}: missing cursor resync");
        assert!(
            asm.contains("__rt_hash_entry_make_reference"),
            "{name}: by-reference binding must promote the entry to a managed cell"
        );
        assert!(
            asm.contains("__rt_hash_iter_next_value"),
            "{name}: the loop must read values through the dereferencing iterator"
        );
    }
}
