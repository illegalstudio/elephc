//! Purpose:
//! Ownership coverage for issue #1049: spreading an INDEXED array into a hash destination must
//! read its source, not consume it.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `Op::ArrayToHash` CONSUMES its operand: the promote path abandons the source indexed array
//!   for a freshly built hash and decrefs it. The spread lowering handed it a borrowed local, so
//!   the promotion released the caller's only reference.
//! - The symptom was quiet: the source read back as `array(0) {}` while its elements still read
//!   correctly through the freed block, so a fixture that checked only the RESULT passed.
//! - These live here rather than beside the array-semantics fixtures because they measure
//!   lifetimes (AGENTS.md:383).

use crate::support::*;

/// Verifies spreading an indexed array leaves the SOURCE untouched.
///
/// `Op::ArrayToHash` consumes its operand: its promote path abandons the source indexed array
/// for a freshly built hash and decrefs it. The spread lowering handed it a borrowed local, so
/// the promotion released the caller's only reference -- `$idx` read back as `array(0) {}` with
/// its elements still intact, and spreading it twice crashed.
///
/// This shape needs NO explicit key, so it reached the same promotion on `main` long before
/// mixed literals existed; it is pinned here because the mixed-literal work is what made it
/// common enough to hit.
#[test]
fn test_spreading_an_indexed_array_leaves_the_source_intact() {
    let out = compile_and_run(
        r#"<?php
$idx = [3, 4];
$assoc = ["x" => 1];
$once = [...$idx, ...$assoc];
$twice = [...$idx, "c" => 8];
echo count($once), count($twice), "|";
foreach ($idx as $k => $v) { echo $k, "=", $v, ","; }
echo "|", count($idx);
"#,
    );
    assert_eq!(out, "33|0=3,1=4,|2");
}

/// Verifies a mixed literal allocates nothing it does not free.
///
/// The promotion's acquire has to be balanced by the release of the promoted hash. Getting that
/// ledger wrong leaks the source array once per iteration, which only a repeated fixture shows.
#[test]
fn test_a_mixed_literal_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 100; $i++) {
    $src = [$i, $i + 1];
    $a = ["head" => "h", ...$src, "tail" => "t"];
    $total += count($a) + count($src);
}
echo $total;
"#,
    );
    assert_eq!(out.stdout, "600", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("leak summary: clean"),
        "a mixed literal must not leak its promoted source: {}",
        out.stderr
    );
}

/// Verifies an OWNING TEMPORARY spread source is released, not leaked.
///
/// The acquire that fixes the borrowed-local case above is unconditional, and the release that
/// balances it targets the PROMOTED hash -- a different value from the array the caller handed
/// over. So a source that already owns its reference (a call result, a nested literal, a
/// property read) ends up with two references and only one consumer: the promotion's decref
/// takes the acquire's, and the temporary's own is left with no owner. One whole array leaked
/// per evaluation.
///
/// It is the exact shape the borrowed-local fixtures cannot see, which is why all three
/// reviewers of this branch landed on it independently. Both destinations are here because the
/// promotion is shared: a mixed literal (an explicit key present) and a spread-only literal with
/// an associative second operand reach it the same way.
///
/// The loop is what separates a leak from a live value at exit -- a fixed residue would not
/// grow with the iteration count.
#[test]
fn test_an_owning_temporary_spread_source_is_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function make(): array { return [3, 4]; }
$assoc = ["x" => 1];
$n = 0;
for ($i = 0; $i < 25; $i++) {
    $keyed = [...make(), "k" => 1];
    $spread = [...make(), ...$assoc];
    $nested = [...[7, 8], "k" => 2];
    $n = $n + count($keyed) + count($spread) + count($nested);
}
echo $n;
"#,
    );
    assert_eq!(out.stdout, "225", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "an owning-temporary spread source leaked: {}",
        out.stderr
    );
}

/// Control: the BORROWED local must stay correct while the temporary case is fixed.
///
/// The two need opposite halves of the same ledger, and the first attempt at this fix -- skipping
/// the acquire when the source is an owning temporary -- made the promotion consume the local's
/// only reference and segfaulted here. Keeping both shapes in one fixture is what makes that
/// trade visible instead of silent.
#[test]
fn test_a_borrowed_local_spread_source_survives_and_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$src = [3, 4];
$assoc = ["x" => 1];
$n = 0;
for ($i = 0; $i < 25; $i++) {
    $keyed = [...$src, "k" => 1];
    $spread = [...$src, ...$assoc];
    $n = $n + count($keyed) + count($spread);
}
echo $n, ",", count($src), ",", $src[1];
"#,
    );
    assert_eq!(out.stdout, "150,2,4", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "a borrowed local spread source leaked: {}",
        out.stderr
    );
}
