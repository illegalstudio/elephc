//! Purpose:
//! Executes checked generic buffer owners across optimized and ordinary call boundaries.
//!
//! Called from:
//! - The method-inlining codegen regression suite.
//!
//! Key details:
//! - Strict heap verification checks exception cleanup and shared-owner destruction.

use super::*;

/// Returns the owner declaration without the example's top-level demonstration.
fn checked_owner_source() -> &'static str {
    include_str!("../../../examples/checked-buffer/main.php")
        .split("function updateChecked").next().unwrap()
}

/// Runtime-bound updates compute the same checksum with or without method splicing.
#[test]
fn test_checked_managed_buffer_update_checksum() {
    assert_both_modes(include_str!("../../../examples/checked-buffer/main.php"), false, "36\n");
}

/// Invalid accesses preserve their exception and aliases share an idempotently closed owner.
#[test]
fn test_checked_managed_buffer_errors_aliases_and_close() {
    let source = format!("{}{}", checked_owner_source(), r#"
function probeChecked(CheckedBuffer<int> $values, int $size): void {
    $alias = $values;
    $alias->set(0, 42);
    echo $values->get(0), '|';
    try { $values->get(-1); } catch (OutOfRangeException $e) { echo $e->getMessage(), '|'; }
    try { $alias->set($size, 99); } catch (OutOfRangeException $e) { echo $e->getMessage(), '|'; }
    echo $values->get(0), '|';
    $values->close();
    $alias->close();
    try { $alias->get(0); } catch (LogicException $e) { echo $e->getMessage(), '|'; }
    try { $values->set(-1, 0); } catch (LogicException $e) { echo $e->getMessage(); }
}
probeChecked(new CheckedBuffer<int>($argc + 1), $argc + 1);
"#);
    assert_both_modes(&source, false, "42|index:-1/2|index:2/2|42|closed|closed");
}

/// Empty updates do not access a closed owner, and overflow keeps ordinary PHP float semantics.
#[test]
fn test_checked_managed_buffer_zero_trip_and_integer_overflow() {
    let source = format!("{}{}", checked_owner_source(), r#"
function emptyChecked(CheckedBuffer<int> $values, int $count): void {
    for ($i = 0; $i < $count; $i++) { $values->set($i, $values->get($i)); }
}
function addChecked(CheckedBuffer<int> $values): void {
    $n = $values->get(0) + 1;
    echo is_float($n) ? 'float|' : 'bad|';
    echo $n > 0 ? 'positive|' : 'bad|';
}
$values = new CheckedBuffer<int>(1);
$values->set(0, PHP_INT_MAX);
addChecked($values);
$values->close();
emptyChecked($values, $argc - 1);
echo 'empty';
"#);
    assert_both_modes(&source, false, "float|positive|empty");
}

/// A later invalid index throws after earlier writes, and a closed nonempty loop throws first.
#[test]
fn test_checked_managed_buffer_loop_failures_preserve_partial_writes() {
    let source = format!("{}{}", checked_owner_source(), r#"
function writeCheckedUntilFailure(CheckedBuffer<int> $values, int $count): void {
    for ($i = 0; $i < $count; $i++) {
        $values->set($i, ($values->get($i) + $i + 1) & 65535);
    }
}
function closeCheckedDuringLoop(CheckedBuffer<int> $values, int $count): void {
    for ($i = 0; $i < $count; $i++) {
        if ($i === 1) { $values->close(); }
        $values->set($i, 7);
    }
}
function writeCheckedNegativeStart(CheckedBuffer<int> $values, int $start): void {
    for ($i = $start; $i < 1; $i++) { $values->set($i, 7); }
}
$values = new CheckedBuffer<int>($argc + 1);
try { writeCheckedUntilFailure($values, $argc + 2); }
catch (OutOfRangeException $e) { echo $e->getMessage(), '|'; }
echo $values->get(0), ':', $values->get(1), '|';
$values->close();
try { writeCheckedUntilFailure($values, $argc); }
catch (LogicException $e) { echo $e->getMessage(), '|'; }
try { closeCheckedDuringLoop(new CheckedBuffer<int>($argc + 1), $argc + 1); }
catch (LogicException $e) { echo $e->getMessage(); }
try { writeCheckedNegativeStart(new CheckedBuffer<int>($argc + 1), -$argc); }
catch (OutOfRangeException $e) { echo '|', $e->getMessage(); }
"#);
    assert_both_modes(&source, false, "index:2/2|1:2|closed|closed|index:-1/2");
}

/// Hot scalar updates allocate and free nothing after their owner has been constructed.
#[test]
fn test_checked_managed_buffer_update_has_no_steady_state_allocations() {
    let workload = include_str!("../../../examples/checked-buffer/main.php")
        .split("function demonstrateChecked").next().unwrap();
    let source = format!("{workload}{}", r#"
function measureCheckedAllocations(CheckedBuffer<int> $values, int $count): void {
$allocations = memory_get_allocations();
$frees = memory_get_frees();
$usage = memory_get_usage();
updateChecked($values, $count, 20);
$afterAllocations = memory_get_allocations();
$afterFrees = memory_get_frees();
$afterUsage = memory_get_usage();
echo $afterAllocations - $allocations, ':', $afterFrees - $frees, ':', $afterUsage - $usage, ':', $values->get(0);
}
measureCheckedAllocations(new CheckedBuffer<int>($argc + 15), $argc + 15);
"#);
    assert_eq!(run(&source, true, false), "0:0:0:190");
}

/// Argument evaluation that replaces a local keeps the original receiver alive through its call.
#[test]
fn test_checked_managed_buffer_local_receiver_rebinding_keeps_its_lease() {
    let source = r#"<?php
final class LocalOwnerProbe {
    public function __construct(private int $id) {}
    public function __destruct() { echo 'drop', $this->id, '|'; }
    public function read(int $unused): int { return $this->id; }
}
function replaceLocalOwner(): void {
    $owner = new LocalOwnerProbe(1);
    echo $owner->read(($owner = new LocalOwnerProbe(2)) instanceof LocalOwnerProbe ? 0 : 1), '|';
    echo $owner->read(0), '|';
}
replaceLocalOwner();
"#;
    assert_both_modes(source, true, "drop1|1|2|drop2|");
}

/// Packed getters observe a field mutation between two inlined reads of the same record.
#[test]
fn test_checked_managed_buffer_packed_reads_observe_writes() {
    let source = r#"<?php
packed class Cell { public int $v; }
final class Grid {
    private buffer<Cell> $cells;
    public function __construct(int $n) {
        $this->cells = buffer_new<Cell>($n);
        $this->cells[0]->v = 1;
    }
    public function __destruct() { $cells = $this->cells; buffer_free($cells); }
    public function get(int $i): int { return $this->cells[$i]->v; }
    public function put(int $i, int $v): void { $this->cells[$i]->v = $v; }
}
function probeField(Grid $g, int $x): int {
    $before = $g->get(0);
    $g->put(0, $x);
    $after = $g->get(0);
    return $before * 100 + $after;
}
$g = new Grid(1);
echo probeField($g, $argc + 6);
"#;
    assert_both_modes(source, false, "107");
}
