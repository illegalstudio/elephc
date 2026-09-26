//! Purpose:
//! Ownership tests for the filtered `ReflectionX::getAttributes($name)` result.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The synthesized body lands each matched element in a typed local before pushing it, which
//!   is the compiler's ONLY `Op::MixedUnbox` site. That op's lowering ends in
//!   `emit_unbox_mixed_to_owned_refcounted_result`, so the value it produces already owns a
//!   reference; the EIR read it as a borrow and acquired a second one, and the single slot
//!   release in the epilogue never balanced it. Each matched attribute then leaked itself and
//!   the whole subtree it owns — five heap blocks per match, measured.
//! - `new ReflectionClass(...)` leaks two blocks per construction at HEAD, on its own, so these
//!   tests cannot assert `leak summary: clean`, and reusing ONE owner across the loop would not
//!   catch the bug either: the surplus reference lands on the same attribute every time, so the
//!   refcount climbs while the block count does not. Each test therefore compares two programs
//!   that construct the SAME number of owners and differ only in how they call `getAttributes`.
//!   The construction baseline cancels and a per-call imbalance does not.

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

/// A matched element must not leave a reference behind. Against the unfiltered call — which
/// hands back `__attrs` itself and allocates nothing — the filtered call used to end twenty
/// iterations one hundred blocks higher: five per match.
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
