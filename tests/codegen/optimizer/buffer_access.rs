//! Purpose:
//! End-to-end regression coverage for `buffer<T>` hot-loop optimization: exact packed field
//! effects, concat-reset elimination around packed fields, scalar promotion of the loop
//! index and of locals stored into packed fields, and loop-hoisted buffer handle resolution.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core optimizer::buffer_access`.
//!
//! Key details:
//! - Textual EIR pins the optimized loop shape; every fixture also runs natively with the
//!   optimizer on and off and must print the same result or die with the same fatal.
//! - Hoisting fixtures cover the cases that must NOT be rewritten: a call that frees the
//!   buffer, a reassigned handle, and modules that enable `pcntl_async_signals()`.
//! - Same-buffer length proofs cover EIR flags, runtime parity, fatal preservation, and Linux
//!   AArch64/x86_64 assembly without the proven descriptor-length comparison.
//! - The non-faulting runtime resolver is checked on all five targets' runtime assembly.
//! - Linux assembly keeps immutable float inputs and packed-field temporaries in registers on both
//!   architectures (the preheader resolves the buffer before the hoisted `$dt` load), while a
//!   whole-record copy from `ptr<T>` preserves a float that remains live across its null guard.

use super::*;

mod numeric_accumulators;

/// One packed-record integration step in the shape of Ivory's `bench/integrate` hot loop.
const INTEGRATE_SOURCE: &str = r#"<?php
packed class Body { public float $x; public float $vx; }
function step(buffer<Body> $b, int $n, float $dt): void {
    for ($i = 0; $i < $n; $i++) {
        $x = $b[$i]->x + $b[$i]->vx * $dt;
        if ($x < 0.0) {
            $x = -$x;
            $b[$i]->vx = -$b[$i]->vx;
        }
        $b[$i]->x = $x;
    }
}
$b = buffer_new<Body>(3);
$b[0]->vx = 1.5;
$b[1]->vx = -2.0;
$b[2]->x = 1.0;
$b[2]->vx = 0.25;
step($b, 3, 2.0);
echo $b[0]->x, " ", $b[0]->vx, " ", $b[1]->x, " ", $b[1]->vx, " ", $b[2]->x, "\n";
"#;

/// The output both optimizer modes must print for `INTEGRATE_SOURCE`.
const INTEGRATE_OUTPUT: &str = "3 1.5 4 2 1.5\n";

/// Same-buffer length loops whose exact index is statically in range for every iteration.
const PROVEN_BOUNDS_SOURCE: &str = r#"<?php
function fill_all(buffer<float> $b): void {
    $n = buffer_len($b);
    for ($i = 0; $i < $n; $i++) {
        $b[$i] = $i + 1.0;
    }
}
function sum_all(buffer<float> $b): float {
    $sum = 0.0;
    for ($i = 0; $i < buffer_len($b); $i++) {
        $sum = $sum + $b[$i];
    }
    return $sum;
}
$b = buffer_new<float>(4);
fill_all($b);
echo sum_all($b), " ", $b[3], "\n";
"#;

/// Similar loop shapes whose resolved accesses must retain their bounds checks.
const REFUSED_BOUNDS_SOURCE: &str = r#"<?php
function unrelated_bound(buffer<float> $b, int $n): float {
    $sum = 0.0;
    for ($i = 0; $i < $n; $i++) { $sum = $sum + $b[$i]; }
    return $sum;
}
function inclusive_bound(buffer<float> $b): float {
    $sum = 0.0;
    for ($i = 0; $i <= buffer_len($b); $i++) { $sum = $sum + $b[$i]; }
    return $sum;
}
function negative_start(buffer<float> $b): float {
    $sum = 0.0;
    for ($i = -1; $i < buffer_len($b); $i++) { $sum = $sum + $b[$i]; }
    return $sum;
}
function step_two(buffer<float> $b): float {
    $sum = 0.0;
    for ($i = 0; $i < buffer_len($b); $i = $i + 2) { $sum = $sum + $b[$i]; }
    return $sum;
}
function decreasing(buffer<float> $b): float {
    $sum = 0.0;
    for ($i = buffer_len($b) - 1; $i >= 0; $i--) { $sum = $sum + $b[$i]; }
    return $sum;
}
function offset_index(buffer<float> $b): float {
    $sum = 0.0;
    for ($i = 0; $i < buffer_len($b); $i++) { $sum = $sum + $b[$i + 1]; }
    return $sum;
}
function other_buffer_length(buffer<float> $a, buffer<float> $b): float {
    $sum = 0.0;
    for ($i = 0; $i < buffer_len($b); $i++) { $sum = $sum + $a[$i]; }
    return $sum;
}
$a = buffer_new<float>(4);
$b = buffer_new<float>(4);
echo unrelated_bound($a, 2) + step_two($a) + decreasing($a) + other_buffer_length($a, $b), " ok\n";
if ($argc < 0) {
    echo inclusive_bound($a), negative_start($a), offset_index($a);
}
"#;

/// A do-while whose access precedes its header-local continuation guard.
const DO_WHILE_HEADER_ACCESS_SOURCE: &str = r#"<?php
function past_end(buffer<float> $b): float {
    $i = 0;
    $sum = 0.0;
    do {
        $sum = $sum + $b[$i];
    } while ($i++ < buffer_len($b));
    return $sum;
}
"#;

/// Emits one named function's textual EIR, optimized or not, without other function bodies.
fn function_ir(source: &str, optimized: bool, name: &str) -> String {
    let dir = make_cli_test_dir("elephc_buffer_access_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write PHP fixture");
    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir");
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let output = command.arg(&php_path).output().expect("emit EIR");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let header = format!("  function {name}(");
    let body = text.split_once(&header).expect("named function").1;
    let function = body.split("\n  function ").next().expect("function body").to_string();
    fs::remove_dir_all(dir).expect("remove fixture directory");
    function
}

/// Emits one named function's target assembly, using library mode on iOS, between stable markers.
fn function_assembly(source: &str, target: &str, name: &str) -> String {
    let dir = make_cli_test_dir("elephc_buffer_access_asm");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write PHP fixture");
    let mut command = elephc_cli_command(&dir);
    command.args(["--emit-asm", "--target", target]);
    if target.starts_with("ios-") {
        command.args(["--emit", "staticlib"]);
    }
    let output = command
        .arg(&php_path)
        .output()
        .expect("emit target assembly");
    assert!(
        output.status.success(),
        "{target}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = fs::read_to_string(php_path.with_extension("s")).expect("read target assembly");
    let start = assembly
        .find(&format!("@fn name={name} "))
        .expect("function start marker");
    let end = start
        + assembly[start..]
            .find(&format!("@endfn name={name}"))
            .expect("function end marker");
    let function = assembly[start..end].to_string();
    fs::remove_dir_all(dir).expect("remove fixture directory");
    function
}

/// Returns one named EIR block regardless of the backend's physical block order.
fn named_assembly_block<'a>(assembly: &'a str, name: &str) -> &'a str {
    let marker = format!("@block name={name}");
    let tail = assembly
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing block {name}: {assembly}"))
        .1;
    tail.split_once("@block name=")
        .map_or(tail, |(block, _)| block)
}

/// Returns whether an AArch64 block loads or stores a float through the frame pointer.
///
/// Slots through byte 255 use `ldur` or `stur` directly from `x29`. Larger slots first derive
/// `x9` from `x29`, possibly through several subtraction chunks, then access the float through
/// `[x9]`. The second form is how the pre-change loop reloaded `$dt` on every iteration.
fn aarch64_block_has_float_frame_traffic(block: &str) -> bool {
    let mut x9_is_frame_address = false;
    for line in block.lines().map(str::trim) {
        let float_access = ["ldr d", "ldur d", "str d", "stur d"]
            .iter()
            .any(|opcode| line.starts_with(opcode));
        if float_access
            && (line.contains("[x29") || (x9_is_frame_address && line.contains("[x9]")))
        {
            return true;
        }
        if line.starts_with("sub x9, x29, #") || line == "mov x9, x29" {
            x9_is_frame_address = true;
        } else if !(x9_is_frame_address && line.starts_with("sub x9, x9, #"))
            && !line.is_empty()
            && !line.starts_with("//")
        {
            x9_is_frame_address = false;
        }
    }
    false
}

/// Returns whether an x86_64 block loads or stores a float through `rbp`.
///
/// Every SSE scalar instruction names an `xmm` register, so a line that also addresses `[rbp`
/// moves a float between a register and a frame slot. The loop before the resolver placement fix
/// reloaded `$dt` this way (`movsd xmm0, QWORD PTR [rbp - 96]`) on every iteration.
fn x86_64_block_has_float_frame_traffic(block: &str) -> bool {
    block
        .lines()
        .map(str::trim)
        .any(|line| line.contains("xmm") && line.contains("[rbp"))
}

/// Compiles and runs one PHP program with the EIR optimizer on or off and returns stdout.
fn run_variant(source: &str, optimized: bool) -> String {
    let dir = make_cli_test_dir("elephc_buffer_access_run");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write PHP fixture");
    let mut command = elephc_cli_command(&dir);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compile = command.arg(&php_path).output().expect("compile PHP fixture");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = run_binary(&dir.join("main"), &dir);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8(run.stdout).expect("program stdout is UTF-8");
    fs::remove_dir_all(dir).expect("remove fixture directory");
    stdout
}

/// Returns the effect list printed for one EIR instruction line, or an empty string.
fn printed_effects(line: &str) -> &str {
    line.split(" ; ")
        .find_map(|part| part.strip_prefix("effects: "))
        .unwrap_or("")
}

/// Packed field reads and writes carry their exact effects instead of the object-property
/// contract, so the read no longer advertises warnings (which published the diagnostic source
/// location before every field load) and the write no longer advertises refcounting or throws.
/// A write keeps `reads_heap` because a nested record field store copies its source record.
#[test]
fn test_packed_field_accesses_carry_exact_effects() {
    let ir = function_ir(INTEGRATE_SOURCE, true, "step");
    let reads: Vec<&str> = ir.lines().filter(|line| line.contains("= prop_get ")).collect();
    let writes: Vec<&str> = ir.lines().filter(|line| line.contains(" prop_set ")).collect();
    assert!(!reads.is_empty() && !writes.is_empty(), "{ir}");
    for line in reads {
        assert_eq!(printed_effects(line), "reads_heap", "{line}");
    }
    for line in writes {
        assert_eq!(printed_effects(line), "reads_heap, writes_heap, may_fatal", "{line}");
    }
    assert_eq!(run_variant(INTEGRATE_SOURCE, true), INTEGRATE_OUTPUT);
    assert_eq!(run_variant(INTEGRATE_SOURCE, false), INTEGRATE_OUTPUT);
}

/// A loop body made only of packed field traffic and float arithmetic never touches the concat
/// scratch, so every statement-boundary `concat_reset` inside it is eliminated.
#[test]
fn test_packed_field_loop_drops_statement_concat_resets() {
    let unoptimized = function_ir(INTEGRATE_SOURCE, false, "step");
    let optimized = function_ir(INTEGRATE_SOURCE, true, "step");
    assert!(unoptimized.contains("concat_reset"), "{unoptimized}");
    assert!(!optimized.contains("concat_reset"), "{optimized}");
}

/// The loop index used only as a buffer index and the float local stored into a packed field
/// are plain values, so scalar promotion keeps both in SSA and the loop does no local traffic.
#[test]
fn test_buffer_index_and_packed_store_locals_are_promoted() {
    let unoptimized = function_ir(INTEGRATE_SOURCE, false, "step");
    let optimized = function_ir(INTEGRATE_SOURCE, true, "step");
    assert!(unoptimized.contains("store_local"), "{unoptimized}");
    assert!(!optimized.contains("store_local"), "{optimized}");
    assert!(optimized.contains("for.cond(v"), "{optimized}");
}

/// Compiles and runs one PHP program with the EIR optimizer on or off and returns its output,
/// whether or not the program succeeded.
fn run_variant_output(source: &str, optimized: bool) -> std::process::Output {
    let dir = make_cli_test_dir("elephc_buffer_access_fatal");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write PHP fixture");
    let mut command = elephc_cli_command(&dir);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compile = command.arg(&php_path).output().expect("compile PHP fixture");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = run_binary(&dir.join("main"), &dir);
    fs::remove_dir_all(dir).expect("remove fixture directory");
    run
}

/// Asserts both optimizer modes print `stdout` and then die with `fatal` on stderr.
fn assert_both_modes_fail_after(source: &str, stdout: &str, fatal: &str) {
    for optimized in [true, false] {
        let run = run_variant_output(source, optimized);
        assert!(!run.status.success(), "optimized={optimized} unexpectedly succeeded");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "optimized={optimized}");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(stderr.contains(fatal), "optimized={optimized}: {stderr}");
    }
}

/// A float buffer summed through a parameter alias; `$alias` outlives `buffer_free($b)`.
const STALE_ALIAS_SOURCE: &str = r#"<?php
function sum(buffer<float> $b, int $n): float {
    $s = 0.0;
    for ($i = 0; $i < $n; $i++) {
        $s = $s + $b[$i];
    }
    return $s;
}
$b = buffer_new<float>(4);
$b[1] = 2.5;
$alias = $b;
echo sum($alias, 2), "\n";
buffer_free($b);
echo sum($alias, 0), "\n";
echo sum($alias, 1), "\n";
"#;

/// The packed integration loop resolves its handle once in the preheader, and every access in
/// the loop goes through the hoisted descriptor.
#[test]
fn test_buffer_loop_resolves_handle_once_in_the_preheader() {
    let ir = function_ir(INTEGRATE_SOURCE, true, "step");
    let preheader = ir.split("for.cond(").next().expect("loop preheader");
    assert_eq!(ir.matches("= buffer_resolve ").count(), 1, "{ir}");
    assert!(preheader.contains("= buffer_resolve "), "{ir}");
    assert!(ir.contains("= buffer_get_resolved "), "{ir}");
    assert!(!ir.contains("= buffer_get "), "{ir}");
}

/// Direct and cached same-buffer lengths mark exact induction reads and writes as in range, and
/// both optimizer modes preserve the program result.
#[test]
fn test_same_buffer_length_loops_mark_resolved_accesses_in_range() {
    let read_ir = function_ir(PROVEN_BOUNDS_SOURCE, true, "sum_all");
    let write_ir = function_ir(PROVEN_BOUNDS_SOURCE, true, "fill_all");
    assert!(
        read_ir.lines().any(|line| {
            line.contains("buffer_get_resolved") && line.contains(" true")
        }),
        "{read_ir}"
    );
    assert!(
        write_ir.lines().any(|line| {
            line.contains("buffer_set_resolved") && line.contains(" true")
        }),
        "{write_ir}"
    );
    assert_eq!(run_variant(PROVEN_BOUNDS_SOURCE, true), "10 4\n");
    assert_eq!(run_variant(PROVEN_BOUNDS_SOURCE, false), "10 4\n");
}

/// Every deliberately unsupported loop shape retains a bounds-checked access, resolved or
/// ordinary, and the safe calls remain optimizer-equivalent.
#[test]
fn test_buffer_index_proof_refuses_unsafe_loop_shapes() {
    for name in [
        "unrelated_bound",
        "inclusive_bound",
        "negative_start",
        "step_two",
        "decreasing",
        "offset_index",
        "other_buffer_length",
    ] {
        let ir = function_ir(REFUSED_BOUNDS_SOURCE, true, name);
        let accesses: Vec<_> = ir
            .lines()
            .filter(|line| {
                line.contains("buffer_get") || line.contains("buffer_set")
            })
            .collect();
        assert!(!accesses.is_empty(), "{name}: {ir}");
        assert!(
            accesses.iter().all(|line| {
                !(line.contains("_resolved") && line.contains(" true"))
            }),
            "{name}: {ir}"
        );
    }
    assert_eq!(run_variant(REFUSED_BOUNDS_SOURCE, true), "0 ok\n");
    assert_eq!(run_variant(REFUSED_BOUNDS_SOURCE, false), "0 ok\n");
}

/// A do-while header access executes before its continuation comparison, so it retains the
/// bounds check and reports the same fatal with either optimizer mode for empty and full buffers.
///
/// The post-increment counter may stay a boxed int|float that keeps the loop unhoisted; the
/// header exclusion itself is covered by the hand-built `self_loop_header_access_is_not_proven_in_bounds`.
#[test]
fn test_do_while_header_access_keeps_bounds_fatal() {
    for length in [0, 1] {
        let source = format!(
            "{DO_WHILE_HEADER_ACCESS_SOURCE}\n$b = buffer_new<float>({length});\necho \"before\\n\";\necho past_end($b), \"\\n\";\n"
        );
        if length == 0 {
            let ir = function_ir(&source, true, "past_end");
            assert!(
                !ir.lines()
                    .any(|line| line.contains("_resolved") && line.contains(" true")),
                "{ir}"
            );
        }
        assert_both_modes_fail_after(&source, "before\n", "buffer index out of bounds");
    }
}

/// Proven loop accesses on both Linux architectures retain the stale-descriptor test and emit no
/// descriptor-length compare in the loop function.
#[test]
fn test_proven_buffer_loop_assembly_omits_length_compare() {
    let arm = function_assembly(PROVEN_BOUNDS_SOURCE, "linux-aarch64", "sum_all");
    assert!(arm.contains("cbnz x11"), "{arm}");
    assert!(!arm.contains("ldr x9, [x11, #8]"), "{arm}");
    assert!(!arm.contains("cmp x10, x9"), "{arm}");

    let x86 = function_assembly(PROVEN_BOUNDS_SOURCE, "linux-x86_64", "sum_all");
    assert!(x86.contains("test r11, r11"), "{x86}");
    assert!(!x86.contains("cmp r10, QWORD PTR [r11 + 8]"), "{x86}");
}

/// Every packed access of one iteration, on both sides of the bounce branch, shares a single
/// bounds-checked record address: the loop index reaching the code after the branch through a
/// fresh join parameter still names the same element.
#[test]
fn test_packed_accesses_of_one_iteration_share_one_record_address() {
    let ir = function_ir(INTEGRATE_SOURCE, true, "step");
    assert_eq!(ir.matches("= buffer_get_resolved ").count(), 1, "{ir}");

    let arm = function_assembly(INTEGRATE_SOURCE, "linux-aarch64", "step");
    assert!(arm.contains("fcmp d1, d0"), "{arm}");
    assert!(!arm.contains("cset x0, mi"), "{arm}");

    let x86 = function_assembly(INTEGRATE_SOURCE, "linux-x86_64", "step");
    assert!(x86.contains("ucomisd xmm0, xmm1"), "{x86}");
    assert!(!x86.contains("setb al"), "{x86}");
    assert!(!x86.contains("setnp r10b"), "{x86}");
    assert_eq!(run_variant(INTEGRATE_SOURCE, true), INTEGRATE_OUTPUT);
}

/// Linux AArch64 and x86_64 both keep `$dt` and the integration loop's float temporaries out of
/// frame slots. The preheader resolves the buffer before the `$dt` load LICM hoisted there, so
/// `$dt` never lives across the resolver call and SysV x86_64, which has no callee-saved XMM
/// register, keeps it in a caller-saved one instead of reloading it on every iteration.
#[test]
fn test_packed_float_loop_keeps_live_values_in_registers() {
    let loop_blocks = ["for.cond", "for.body", "for.update", "if.then", "if.else"];
    let arm = function_assembly(INTEGRATE_SOURCE, "linux-aarch64", "step");
    let x86 = function_assembly(INTEGRATE_SOURCE, "linux-x86_64", "step");
    for name in loop_blocks {
        let arm_block = named_assembly_block(&arm, name);
        assert!(!aarch64_block_has_float_frame_traffic(arm_block), "{name}: {arm_block}");
        let x86_block = named_assembly_block(&x86, name);
        assert!(!x86_64_block_has_float_frame_traffic(x86_block), "{name}: {x86_block}");
    }
}

/// The hoisted resolver opens the preheader instead of closing it: it precedes the `$dt` load LICM
/// moved there, so no loop-invariant float is live across the `__rt_buffer_try_resolve` call, and
/// both optimizer modes still print the same result.
#[test]
fn test_buffer_resolver_precedes_hoisted_float_loads() {
    let ir = function_ir(INTEGRATE_SOURCE, true, "step");
    let preheader: Vec<&str> = ir.split("for.cond(").next().expect("loop preheader").lines().collect();
    let resolve = preheader
        .iter()
        .position(|line| line.contains("= buffer_resolve "))
        .unwrap_or_else(|| panic!("preheader resolves the buffer: {ir}"));
    let float_load = preheader
        .iter()
        .position(|line| line.contains("F64 php=float = load_local "))
        .unwrap_or_else(|| panic!("LICM hoists the $dt load into the preheader: {ir}"));
    assert!(resolve < float_load, "{ir}");
    assert_eq!(run_variant(INTEGRATE_SOURCE, true), INTEGRATE_OUTPUT);
    assert_eq!(run_variant(INTEGRATE_SOURCE, false), INTEGRATE_OUTPUT);
}

/// The hoisted resolution never faults: a stale handle still dies at the first access with the
/// use-after-free diagnostic, and a loop that never runs its body reports nothing.
#[test]
fn test_hoisted_buffer_loop_reports_stale_handle_at_the_access() {
    let ir = function_ir(STALE_ALIAS_SOURCE, true, "sum");
    assert!(ir.contains("= buffer_resolve "), "{ir}");
    assert_both_modes_fail_after(STALE_ALIAS_SOURCE, "2.5\n0\n", "use of buffer after buffer_free()");
}

/// The hoisted descriptor keeps the per-access bounds check.
#[test]
fn test_hoisted_buffer_loop_still_traps_out_of_range_index() {
    let source = r#"<?php
function total(buffer<float> $b, int $n): float {
    $s = 0.0;
    for ($i = 0; $i < $n; $i++) {
        $s = $s + $b[$i];
    }
    return $s;
}
$b = buffer_new<float>(2);
$b[1] = 4.0;
echo total($b, 2), "\n";
echo total($b, 3), "\n";
"#;
    assert!(function_ir(source, true, "total").contains("= buffer_resolve "));
    assert_both_modes_fail_after(source, "4\n", "buffer index out of bounds");
}

/// An inclusive same-buffer length loop keeps its bounds check and dies on index `len`.
///
/// Its counter can pass `buffer_len()`, so it may stay a boxed int|float even when numeric
/// preservation permits descriptor hoisting. No access may carry the in-range flag.
#[test]
fn test_inclusive_same_buffer_bound_still_traps() {
    let source = r#"<?php
function total(buffer<float> $b): float {
    $sum = 0.0;
    for ($i = 0; $i <= buffer_len($b); $i++) { $sum = $sum + $b[$i]; }
    return $sum;
}
$b = buffer_new<float>(2);
$b[0] = 3.0;
$b[1] = 4.0;
echo "before\n";
echo total($b), "\n";
"#;
    let ir = function_ir(source, true, "total");
    assert!(
        !ir.lines().any(|line| {
            line.contains("buffer_get_resolved") && line.contains(" true")
        }),
        "{ir}"
    );
    assert_both_modes_fail_after(source, "before\n", "buffer index out of bounds");
}

/// Folding the signed-negative branch into the unsigned length compare preserves its fatal.
#[test]
fn test_negative_buffer_index_keeps_bounds_fatal() {
    let source = r#"<?php
$b = buffer_new<int>(1);
echo "before\n";
echo $b[-1];
"#;
    assert_both_modes_fail_after(source, "before\n", "buffer index out of bounds");
}

/// A cached length cannot prove accesses after the buffer slot is reassigned, and both optimizer
/// modes report the ordinary out-of-range fatal against the replacement buffer.
#[test]
fn test_cached_length_reassigned_buffer_keeps_bounds_fatal() {
    let source = r#"<?php
function overwrite(buffer<int> $b): void {
    $n = buffer_len($b);
    $b = buffer_new<int>(1);
    for ($i = 0; $i < $n; $i++) { $b[$i] = 1; }
}
$b = buffer_new<int>(4);
echo "before\n";
overwrite($b);
"#;
    let ir = function_ir(source, true, "overwrite");
    assert!(ir.contains("buffer_set_resolved"), "{ir}");
    assert!(
        !ir.lines().any(|line| {
            line.contains("buffer_set_resolved") && line.contains(" true")
        }),
        "{ir}"
    );
    assert_both_modes_fail_after(source, "before\n", "buffer index out of bounds");
}

/// A call that frees the buffer through its parameter copy keeps every access resolving the
/// handle again, so the next iteration reports the use-after-free instead of reading freed
/// memory.
#[test]
fn test_buffer_loop_calling_a_free_keeps_per_access_resolution() {
    let source = r#"<?php
function release(buffer<int> $x): void {
    buffer_free($x);
}
function walk(buffer<int> $b, int $n): int {
    $t = 0;
    for ($i = 0; $i < $n; $i++) {
        $t = $t ^ $b[$i];
        if ($i == 0) {
            release($b);
        }
    }
    return $t;
}
$b = buffer_new<int>(4);
$b[0] = 5;
echo walk($b, 1), "\n";
$c = buffer_new<int>(4);
echo walk($c, 2), "\n";
"#;
    let ir = function_ir(source, true, "walk");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert!(ir.contains("= buffer_get "), "{ir}");
    assert_both_modes_fail_after(source, "5\n", "use of buffer after buffer_free()");
}

/// Reassigning the buffer variable inside the loop makes the handle loop-variant.
#[test]
fn test_buffer_loop_reassigning_the_handle_keeps_per_access_resolution() {
    let source = r#"<?php
function pick(buffer<int> $a, buffer<int> $b, int $n): int {
    $t = 0;
    for ($i = 0; $i < $n; $i++) {
        $t = $t ^ $a[$i];
        $a = $b;
    }
    return $t;
}
$a = buffer_new<int>(2);
$b = buffer_new<int>(2);
$a[0] = 1;
$a[1] = 2;
$b[0] = 4;
$b[1] = 8;
echo pick($a, $b, 2), "\n";
"#;
    let ir = function_ir(source, true, "pick");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_eq!(run_variant(source, true), "9\n");
    assert_eq!(run_variant(source, false), "9\n");
}

/// Element writes go through the hoisted descriptor too, and an inner loop shares the outer
/// loop's resolution.
#[test]
fn test_buffer_writes_and_nested_loops_use_the_outer_descriptor() {
    let source = r#"<?php
function fill(buffer<int> $b, int $rows, int $cols): void {
    for ($r = 0; $r < $rows; $r++) {
        for ($c = 0; $c < $cols; $c++) {
            $b[$r * $cols + $c] = $r ^ $c;
        }
    }
}
function checksum(buffer<int> $b, int $n): int {
    $t = 0;
    for ($i = 0; $i < $n; $i++) {
        $t = $t ^ ($b[$i] << ($i % 4));
    }
    return $t;
}
$b = buffer_new<int>(12);
fill($b, 3, 4);
echo checksum($b, 12), "\n";
echo $b[5], " ", $b[11], "\n";
"#;
    let ir = function_ir(source, true, "fill");
    assert_eq!(ir.matches("= buffer_resolve ").count(), 1, "{ir}");
    assert!(ir.contains("buffer_set_resolved "), "{ir}");
    assert!(!ir.contains(" buffer_set "), "{ir}");
    let expected = run_variant(source, false);
    assert_eq!(run_variant(source, true), expected);
    assert!(expected.ends_with("0 1\n"), "{expected}");
}

/// `pcntl_async_signals()` lets a signal handler run after any instruction, so no loop in the
/// module may assume its buffers stay valid between accesses.
#[test]
fn test_pcntl_async_signals_disable_buffer_resolution_hoisting() {
    let source = r#"<?php
pcntl_async_signals(true);
function sum(buffer<float> $b, int $n): float {
    $s = 0.0;
    for ($i = 0; $i < $n; $i++) {
        $s = $s + $b[$i];
    }
    return $s;
}
$b = buffer_new<float>(2);
$b[0] = 1.5;
echo sum($b, 2), "\n";
"#;
    let ir = function_ir(source, true, "sum");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_eq!(run_variant(source, true), "1.5\n");
}

/// The documented example prints the same result with the optimizer on and off and resolves
/// its buffer handle once in front of the `step` loop; its second half steps a list of
/// particle buffers, so the per-layer sums come from buffers handed back by the array.
#[test]
fn test_buffer_loops_example_hoists_resolution() {
    let source = include_str!("../../../examples/buffer-loops/main.php");
    assert!(function_ir(source, true, "step").contains("= buffer_resolve "));
    assert!(function_ir(source, true, "moving_count").contains("= buffer_resolve "));
    let expected =
        "particles=8; sum=380\nmoving=5\nlayer 0: sum=300\nlayer 1: sum=308\nlayer 2: sum=316\nlayers=3; total=924\n";
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let model = source.replace("packed class ", "class ").replace("buffer<Particle>", "array")
            .replace("buffer_new<Particle>($count)", "array_map(fn($unused) => new Particle(), range(1, $count))")
            .replace("buffer_free($p);", "unset($p);")
            .replace("buffer_free($particles);", "unset($particles);");
        let dir = make_cli_test_dir("elephc_buffer_loops_php_model");
        let path = dir.join("model.php"); fs::write(&path, model).unwrap();
        let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
        fs::remove_dir_all(dir).unwrap();
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(php.stdout, expected.as_bytes());
    }
    assert_eq!(run_variant(source, true), expected);
    assert_eq!(run_variant(source, false), expected);
}

/// Every target's runtime carries the non-faulting resolver, and its invalid path answers 0
/// instead of reaching the use-after-free fatal.
#[test]
fn test_every_target_runtime_carries_the_non_faulting_resolver() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).expect("supported target");
        let runtime = elephc::codegen::generate_runtime(8_388_608, target);
        let start = runtime
            .find("__rt_buffer_try_resolve:")
            .unwrap_or_else(|| panic!("{name} runtime lacks __rt_buffer_try_resolve"));
        let rest = &runtime[start..];
        let body = &rest[..rest.find("\n\n").unwrap_or(rest.len())];
        assert!(
            !body.contains("__rt_buffer_use_after_free"),
            "{name}: __rt_buffer_try_resolve reaches the use-after-free fatal"
        );
        assert!(
            body.contains("mov x0, #0") || body.contains("xor eax, eax"),
            "{name}: __rt_buffer_try_resolve has no zero result path"
        );
    }
}

/// A loop that copies one nested packed record into another field of the same element, in the
/// shape of a physics step that keeps the previous position.
const RECORD_COPY_SOURCE: &str = r#"<?php
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; public Vec2 $prev; public int $id; }
function shift(buffer<Body> $b, int $n): void {
    for ($i = 0; $i < $n; $i++) {
        $b[$i]->prev = $b[$i]->pos;
        $b[$i]->pos->x = $b[$i]->pos->x + 1.0;
    }
}
$b = buffer_new<Body>(2);
$b[0]->pos->x = 1.5;
$b[0]->pos->y = 2.0;
$b[1]->pos->x = -3.0;
$b[1]->pos->y = 0.5;
shift($b, 2);
echo $b[0]->prev->x, " ", $b[0]->prev->y, " ", $b[0]->pos->x, " ", $b[1]->prev->x, " ", $b[1]->pos->x, "\n";
"#;

/// A pointer-backed nested record copy with a float SSA value live across its null-check call.
const PTR_RECORD_COPY_LIVE_FLOAT_SOURCE: &str = r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern function free(ptr $p): void;
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; }
function copy_and_sum(buffer<Body> $b, ptr<Vec2> $source, float $live): float {
    $held = $live * 3.0;
    $b[0]->pos = $source;
    return $held + $b[0]->pos->x + $b[0]->pos->y;
}
$raw = calloc(1, ptr_sizeof("Vec2"));
$source = ptr_cast<Vec2>($raw);
$source->x = 1.25;
$source->y = 2.5;
$b = buffer_new<Body>(1);
echo copy_and_sum($b, $source, 4.0), " ", $b[0]->pos->x, " ", $b[0]->pos->y, "\n";
free($raw);
"#;

/// A nested record copy reads its source record, so its `prop_set` advertises `reads_heap`
/// alongside the write, and a loop of record copies still hoists buffer resolution and prints
/// the same result with the optimizer on and off.
#[test]
fn test_nested_record_copy_reads_its_source_in_a_hoisted_loop() {
    let ir = function_ir(RECORD_COPY_SOURCE, true, "shift");
    let writes: Vec<&str> = ir.lines().filter(|line| line.contains(" prop_set ")).collect();
    assert_eq!(writes.len(), 2, "{ir}");
    for line in writes {
        assert_eq!(printed_effects(line), "reads_heap, writes_heap, may_fatal", "{line}");
    }
    assert!(ir.contains("buffer_get_resolved"), "{ir}");
    let expected = "1.5 2 2.5 -3 -2\n";
    assert_eq!(run_variant(RECORD_COPY_SOURCE, true), expected);
    assert_eq!(run_variant(RECORD_COPY_SOURCE, false), expected);
}

/// A float live across a pointer-backed whole-record copy survives the runtime null check in both
/// optimizer modes, and the destination receives an independent byte copy.
#[test]
fn test_float_value_survives_packed_record_copy_from_pointer() {
    let expected = "15.75 1.25 2.5\n";
    assert_eq!(run_variant(PTR_RECORD_COPY_LIVE_FLOAT_SOURCE, true), expected);
    assert_eq!(run_variant(PTR_RECORD_COPY_LIVE_FLOAT_SOURCE, false), expected);
}
