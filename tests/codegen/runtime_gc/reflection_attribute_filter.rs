//! Purpose:
//! Ownership tests for the filtered `ReflectionX::getAttributes($name)` result.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The synthesized body lands each matched element in a typed local before pushing it.
//!   Both unfiltered and filtered calls build fresh result arrays, so ownership of elements
//!   loaded from the private attribute storage can be compared across the two paths.
//! - Reflection construction has unrelated live blocks, so these tests compare two programs
//!   that construct the same number of owners and differ in the `getAttributes` call. Creating
//!   a fresh owner per iteration exposes reference leaks that reuse of one owner would hide.

use crate::support::compile_and_run_with_heap_debug;

/// Returns the `live_blocks=N` figure heap debug printed on stderr.
fn live_blocks(out: &crate::support::ProgramOutput) -> u64 {
    let marker = "live_blocks=";
    let start = out
        .stderr
        .rfind(marker)
        .unwrap_or_else(|| panic!("no heap debug summary in stderr: {}", out.stderr))
        + marker.len();
    let rest = &out.stderr[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().expect("live_blocks is a number")
}

/// Builds a loop of 20 `ReflectionClass` constructions whose body is `call`.
fn loop_source(call: &str) -> String {
    format!(
        r#"<?php
#[Attribute] class Marker {{ public function __construct(public string $v = "one") {{}} }}
#[Attribute] class Other {{}}

#[Marker("one")]
#[Other]
class Target {{}}

$total = 0;
for ($i = 0; $i < 20; $i++) {{
    $r = new ReflectionClass(Target::class);
    {call}
}}
echo $total, "\n";
"#
    )
}

/// Asserts two programs that construct the same owners leak the same, so only the difference
/// between their `getAttributes` calls is under test.
fn assert_same_live_blocks(
    baseline_call: &str,
    baseline_out: &str,
    probe_call: &str,
    probe_out: &str,
    what: &str,
) {
    let baseline = compile_and_run_with_heap_debug(&loop_source(baseline_call));
    let probe = compile_and_run_with_heap_debug(&loop_source(probe_call));
    assert_eq!(baseline.stdout, baseline_out, "stderr: {}", baseline.stderr);
    assert_eq!(probe.stdout, probe_out, "stderr: {}", probe.stderr);
    assert_eq!(
        live_blocks(&baseline),
        live_blocks(&probe),
        "{what}\nbaseline: {}\nprobe: {}",
        baseline.stderr,
        probe.stderr
    );
}

/// A matched element must not leave a reference behind relative to an unfiltered call.
#[test]
fn test_filtered_get_attributes_does_not_leak_per_call() {
    assert_same_live_blocks(
        r#"$total = $total + count($r->getAttributes());"#,
        "40\n",
        r#"$total = $total + count($r->getAttributes(Marker::class));"#,
        "20\n",
        "filtered getAttributes() leaks per call",
    );
}

/// The same comparison where the filter matches NOTHING, so the loop exercises the empty result
/// rather than the push path. This arm was already balanced and stays pinned that way.
#[test]
fn test_unmatched_get_attributes_does_not_leak_per_call() {
    assert_same_live_blocks(
        r#"$total = $total + count($r->getAttributes());"#,
        "40\n",
        r#"$total = $total + count($r->getAttributes("Nope"));"#,
        "0\n",
        "unmatched getAttributes() leaks per call",
    );
}

/// Reading through the filtered element keeps the attribute reachable while it is in use, and
/// must not accumulate once it is not.
#[test]
fn test_filtered_get_attributes_element_use_does_not_leak_per_call() {
    assert_same_live_blocks(
        r#"$all = $r->getAttributes(); $total = $total + strlen($all[0]->getName());"#,
        "120\n",
        r#"$f = $r->getAttributes(Marker::class); $total = $total + strlen($f[0]->getName());"#,
        "120\n",
        "filtered element use leaks per call",
    );
}

/// The filtered result of a `ReflectionMethod` goes through the same body on a different owner,
/// so it gets the same pin.
#[test]
fn test_filtered_method_attributes_do_not_leak_per_call() {
    let source = |call: &str| {
        format!(
            r#"<?php
#[Attribute] class Marker {{}}
#[Attribute] class Other {{}}

class Target {{ #[Marker] #[Other] public function m(): void {{}} }}

$total = 0;
for ($i = 0; $i < 20; $i++) {{
    $r = new ReflectionMethod(Target::class, "m");
    {call}
}}
echo $total, "\n";
"#
        )
    };
    let baseline =
        compile_and_run_with_heap_debug(&source(r#"$total = $total + count($r->getAttributes());"#));
    let probe = compile_and_run_with_heap_debug(&source(
        r#"$total = $total + count($r->getAttributes(Marker::class));"#,
    ));
    assert_eq!(baseline.stdout, "40\n", "stderr: {}", baseline.stderr);
    assert_eq!(probe.stdout, "20\n", "stderr: {}", probe.stderr);
    assert_eq!(
        live_blocks(&baseline),
        live_blocks(&probe),
        "filtered ReflectionMethod::getAttributes() leaks per call\nbaseline: {}\nprobe: {}",
        baseline.stderr,
        probe.stderr
    );
}
