//! Purpose:
//! Verifies a catchable throwable raised by a codegen guard releases the operands still in flight.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Every fixture repeats its guard in a loop so one stranded operand per catch is visible.
//! - The pinned record must survive a caught guard, a nested expression, and a throwing
//!   `__toString`, without releasing anything twice on the normal path.
//! - The narrowing is covered too: an operand a live variable still owns, and a store that
//!   transfers its temporary, must both stay UNPINNED.
//! - The emitter assertion covers all five supported targets, since only two of them can run here.

use std::path::Path;
use std::process::Command;

use crate::support::*;

/// Resolves the compiler binary the target-aware emitter assertion drives.
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// `count()`'s TypeError must not strand the boxed operand the call it wrapped produced.
///
/// The inner call's result is a pure SSA temporary when the guard jumps to
/// `__rt_throw_current`, so before it was pinned in the unwind chain every caught guard kept
/// one box and the object inside it alive forever.
#[test]
fn test_caught_count_type_error_releases_the_in_flight_boxed_operand() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CountGuardPayload { public int $value = 1; }
function countGuardPick(CountGuardPayload $payload): mixed { return $payload; }
$caught = 0;
for ($i = 0; $i < 12; $i++) {
    try { echo count(countGuardPick(new CountGuardPayload())); }
    catch (TypeError $error) { $caught++; unset($error); }
}
echo $caught;
"#,
    );
    assert_eq!(out.stdout, "12");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A nested expression keeps both its own temporaries and the guarded one balanced.
#[test]
fn test_caught_count_type_error_inside_a_nested_expression_leaves_no_live_blocks() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class NestedGuardPayload { public int $value = 1; }
function nestedGuardPick(NestedGuardPayload $payload): mixed { return $payload; }
function nestedGuardTag(string $text): string { return $text . "!"; }
$caught = 0;
for ($i = 0; $i < 12; $i++) {
    try { echo strlen(nestedGuardTag("a")) + count(nestedGuardPick(new NestedGuardPayload())); }
    catch (TypeError $error) { $caught++; unset($error); }
}
echo $caught;
"#,
    );
    assert_eq!(out.stdout, "12");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// The typed-property write guard is the second emitter, and it strands the same shape.
#[test]
fn test_caught_typed_property_type_error_releases_the_assigned_temporary() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class PropertyGuardHolder { public int $count = 0; }
class PropertyGuardPayload { public int $value = 1; }
function propertyGuardPick(PropertyGuardPayload $payload): mixed { return $payload; }
$holder = new PropertyGuardHolder();
$caught = 0;
for ($i = 0; $i < 12; $i++) {
    try { $holder->count = propertyGuardPick(new PropertyGuardPayload()); }
    catch (TypeError $error) { $caught++; unset($error); }
}
echo $caught;
echo $holder->count;
"#,
    );
    assert_eq!(out.stdout, "120");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A throwing `__toString` unwinds out of the store itself, not out of a codegen guard.
///
/// The operand is pinned before the store rather than before the guard, so the same record
/// covers a user destructor-style throw raised from inside the conversion.
#[test]
fn test_throwing_to_string_during_a_typed_property_store_releases_the_operand() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ToStringGuardBoom {
    public function __toString(): string { throw new TypeError("no string"); }
}
class ToStringGuardHolder { public string $text = ""; }
function toStringGuardMake(): mixed { return new ToStringGuardBoom(); }
$holder = new ToStringGuardHolder();
$caught = 0;
for ($i = 0; $i < 12; $i++) {
    try { $holder->text = toStringGuardMake(); }
    catch (TypeError $error) { $caught++; unset($error); }
}
echo $caught;
"#,
    );
    assert_eq!(out.stdout, "12");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// `clone()`'s reference-property-override refusal is a third emitter reaching the same helper.
///
/// Binding `$value` to the existing override entry promotes that entry into a reference set, so
/// every clone reaches the real catchable reference guard while its freshly cloned receiver is
/// still an unwind-visible owner.
#[test]
fn test_caught_clone_property_override_error_releases_the_in_flight_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CloneGuardPayload { public int $value = 1; }
function cloneGuardPick(CloneGuardPayload $payload): mixed { return $payload; }
$overrides = ['value' => 2];
$value = &$overrides['value'];
$caught = 0;
for ($i = 0; $i < 12; $i++) {
    try { $copy = clone(cloneGuardPick(new CloneGuardPayload()), $overrides); }
    catch (Error $error) { $caught++; unset($error); }
}
echo $caught;
"#,
    );
    assert_eq!(out.stdout, "12");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A guard that never fires must not release the operand twice on the normal path.
#[test]
fn test_untriggered_guards_keep_their_operands_owned_exactly_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class NormalPathPayload implements Countable {
    public function count(): int { return 3; }
}
function normalPathPick(NormalPathPayload $payload): mixed { return $payload; }
$total = 0;
for ($i = 0; $i < 12; $i++) {
    $total += count(normalPathPick(new NormalPathPayload()));
}
echo $total;
"#,
    );
    assert_eq!(out.stdout, "36");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A guard whose operands a LIVE variable still owns must not publish them.
///
/// `explode("", $text)` and `array_fill(0, -1, $text)` raise their `ValueError` while holding a
/// borrowed view of a PHP local. Publishing that view let the unwind record free storage the
/// local frees again, which aborted with "heap debug detected double free".
#[test]
fn test_guards_over_borrowed_variable_operands_release_them_exactly_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$caught = 0;
for ($i = 0; $i < 12; $i++) {
    $text = "value" . $i;
    try { $split = explode("", $text); } catch (ValueError $error) { $caught++; }
    try { $filled = array_fill(0, -1, $text); } catch (ValueError $error) { $caught++; }
    try { $chunks = str_split($text, 0); } catch (ValueError $error) { $caught++; }
    $caught += strlen($text) > 0 ? 0 : 1;
}
echo $caught;
"#,
    );
    assert_eq!(out.stdout, "36");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A property store that TRANSFERS its temporary owes no release, so it must not be pinned.
///
/// An array-typed property adopts the assigned array outright. A pin there left one reference
/// nothing retired and leaked the array, its strings and its box on every iteration.
#[test]
fn test_transferring_property_stores_keep_their_single_reference() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class TransferHolder { public array $items = []; }
$holder = new TransferHolder();
$total = 0;
for ($i = 0; $i < 12; $i++) {
    $text = "value" . $i;
    $holder->items = [$text, "tail"];
    $total += count($holder->items);
}
echo $total;
"#,
    );
    assert_eq!(out.stdout, "24");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// An uncaught codegen guard keeps PHP's own diagnostic and exit status.
#[test]
fn test_uncaught_count_type_error_still_reports_phps_fatal() {
    let out = compile_and_run_expect_failure(
        r#"<?php
class UncaughtGuardPayload { public int $value = 1; }
function uncaughtGuardPick(UncaughtGuardPayload $payload): mixed { return $payload; }
echo count(uncaughtGuardPick(new UncaughtGuardPayload()));
"#,
    );
    assert!(
        out.contains("Fatal error: Uncaught TypeError: count():"),
        "{out}"
    );
}

/// Every supported target publishes an operand-owner record that spans the guard's throw.
///
/// Only the host architecture can RUN the fixtures above, and `ios-arm64`/`ios-sim-arm64`
/// cannot even produce an executable, so the record's presence is pinned in emitted assembly
/// instead: the pin has to be published before the guard branches to `__rt_throw_current` and
/// detached only after every one of the guard's throw sites.
#[test]
fn test_every_supported_target_pins_the_guarded_operand_across_the_throw() {
    let dir = std::env::temp_dir().join(format!(
        "elephc_guard_owner_targets_{}_{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("guard.php"),
        r#"<?php
class TargetGuardPayload { public int $value = 1; }
function targetGuardPick(TargetGuardPayload $payload): mixed { return $payload; }
function targetGuardCount(): int {
    $total = 0;
    try { $total += count(targetGuardPick(new TargetGuardPayload())); }
    catch (TypeError $error) { $total = -1; }
    return $total;
}
echo targetGuardCount();
"#,
    )
    .unwrap();

    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_guard_assembly(&dir, target);
        let body = assembly
            .split_once("_fn_targetGuardCount:")
            .unwrap_or_else(|| panic!("{target}: guarded function missing:\n{assembly}"))
            .1;
        let body = body.split("\n.globl").next().unwrap_or(body);
        // The catch dispatch has its own unmatched-type rethrow after the guarded call has
        // completed and the pin has correctly been detached. Limit this assertion to the
        // protected try body so every `__rt_throw_current` it sees belongs to the guard.
        let protected_body = body
            .find("@block name=try.catch_dispatch")
            .map_or(body, |index| &body[..index]);
        let publish = protected_body
            .rfind("publish temporary call operand owner")
            .unwrap_or_else(|| panic!("{target}: no operand owner published:\n{protected_body}"));
        let detach = protected_body
            .rfind("detach temporary call operand owner")
            .unwrap_or_else(|| panic!("{target}: no operand owner detached:\n{protected_body}"));
        assert!(publish < detach, "{target}: the pin must span the guard");
        let throws = protected_body
            .match_indices("__rt_throw_current")
            .map(|(index, _)| index)
            .filter(|index| *index > publish)
            .collect::<Vec<_>>();
        assert!(
            !throws.is_empty(),
            "{target}: the guard must reach the unwinder inside the pin:\n{protected_body}"
        );
        assert!(
            throws.iter().all(|index| *index < detach),
            "{target}: every guard throw must run while the pin is published:\n{protected_body}"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// Emits one fixture's assembly for a target and returns its text.
///
/// iOS targets refuse a standalone executable, so they are asked for a static library; the
/// user function the assertion reads is emitted either way.
fn emit_guard_assembly(dir: &Path, target: &str) -> String {
    let mut command = Command::new(elephc_bin());
    command.env("XDG_CACHE_HOME", dir.join("cache-root"));
    command.current_dir(dir);
    command.args(["--emit-asm", "--target", target]);
    if target.starts_with("ios") {
        command.args(["--emit", "staticlib"]);
    }
    let output = command
        .arg("guard.php")
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "{target}: emitting assembly failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(dir.join("guard.s")).expect("emitted assembly")
}
