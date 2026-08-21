//! Purpose:
//! End-to-end regressions for user/native iterator dispatch and foreach cleanup.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures verify observable output, heap ownership, and target-specific assembly.

use crate::support::*;

/// Shared iterator classes for foreach lifetime regressions.
const FOREACH_CLEANUP_FIXTURE: &str = r#"<?php
class CleanupRange implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class CleanupAggregate implements IteratorAggregate {
    public function getIterator(): Iterator { return new CleanupRange(0, 3); }
}
"#;

/// Compiles one foreach lifetime scenario and requires both output parity and a clean heap.
fn assert_foreach_cleanup_is_clean(scenario: &str, expected_stdout: &str) {
    let source = format!("{FOREACH_CLEANUP_FIXTURE}\n{scenario}");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, expected_stdout);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Tests a user-defined class implementing Iterator with value-only foreach syntax.
/// The foreach dispatches rewind/valid/current/next against the concrete class,
/// emitting only the value ($v) on each iteration.
#[test]
fn test_foreach_user_iterator_value_only() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->i = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
foreach (new Range(0, 3) as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "012");
}

/// Tests a user-defined class implementing Iterator with key=>value foreach syntax.
/// Verifies both $k and $v are emitted correctly per iteration.
#[test]
fn test_foreach_user_iterator_with_key() {
    let out = compile_and_run(
        r#"<?php
class IntPair implements Iterator {
    private int $i;
    public function __construct() { $this->i = 10; }
    public function rewind(): void {}
    public function valid(): bool { return $this->i < 13; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i - 10; }
    public function next(): void { $this->i = $this->i + 1; }
}
foreach (new IntPair() as $k => $v) { echo $k; echo ":"; echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0:10 1:11 2:12 ");
}

/// Tests that break inside a foreach over a user-defined Iterator terminates the loop
/// at the correct iteration (before the iteration where $v == 4).
#[test]
fn test_foreach_user_iterator_break() {
    let out = compile_and_run(
        r#"<?php
class Counter implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void {}
    public function valid(): bool { return true; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
foreach (new Counter() as $v) {
    if ($v == 4) { break; }
    echo $v;
}
"#,
    );
    assert_eq!(out, "0123");
}

/// Tests a class implementing IteratorAggregate (not Iterator directly). The foreach
/// must call getIterator() once and then dispatch rewind/valid/current/next against
/// the returned object for each iteration.
#[test]
fn test_foreach_iterator_aggregate_class() {
    // A class that implements only IteratorAggregate (not Iterator
    // directly) — foreach calls getIterator() once before the loop and
    // dispatches the per-iteration calls against the returned class.
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): mixed { return $this->current; }
    public function key(): mixed { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Aggregate implements IteratorAggregate {
    public function getIterator(): Range { return new Range(0, 5); }
}
foreach (new Aggregate() as $v) { echo $v; echo " "; }
"#,
    );
    assert_eq!(out, "0 1 2 3 4 ");
}

/// Tests IteratorAggregate where getIterator() is declared to return the Iterator
/// interface type (而非 a concrete class). The foreach must still dispatch correctly
/// against the concrete Range object returned at runtime.
#[test]
fn test_foreach_iterator_aggregate_returning_iterator_interface() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Aggregate implements IteratorAggregate {
    public function getIterator(): Iterator { return new Range(0, 3); }
}
foreach (new Aggregate() as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "012");
}

/// Regression for #385: `IteratorAggregate::getIterator()` may declare the
/// marker `Traversable` return type, but foreach must dispatch through the
/// concrete Iterator methods implemented by the returned object.
#[test]
fn test_foreach_iterator_aggregate_returning_traversable_interface() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Aggregate implements IteratorAggregate {
    public function getIterator(): Traversable { return new Range(0, 3); }
}
foreach (new Aggregate() as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "012");
}

/// Tests foreach with a function parameter typed as Iterator (interface). Verifies
/// dispatch correctly calls rewind/valid/current/key/next on the concrete Range
/// object passed at runtime and that key() result is used for $k.
#[test]
fn test_foreach_iterator_typed_parameter_dispatches_by_interface() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current - 2; }
    public function next(): void { $this->current = $this->current + 1; }
}
function dump_values(Iterator $it): void {
    foreach ($it as $k => $v) {
        echo $k;
        echo "=";
        echo $v;
        echo " ";
    }
}
dump_values(new Range(2, 5));
"#,
    );
    assert_eq!(out, "0=2 1=3 2=4 ");
}

/// Tests that the iterator variable $it can be reused as the foreach value variable
/// ($it again). After the loop, the original iterator reference must remain valid.
#[test]
fn test_foreach_iterator_value_can_reuse_receiver_variable() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
$it = new Range(0, 3);
foreach ($it as $it) {
    echo $it;
}
"#,
    );
    assert_eq!(out, "012");
}

/// Tests that when a function parameter typed as Iterator is consumed by foreach,
/// the parameter variable can be reused as the foreach value variable inside the loop.
#[test]
fn test_foreach_iterator_typed_parameter_can_reuse_receiver_variable() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
function consume(Iterator $it): void {
    foreach ($it as $it) {
        echo $it;
    }
}
consume(new Range(0, 3));
"#,
    );
    assert_eq!(out, "012");
}

/// Tests that pre-existing $k and $v variables outside a foreach are preserved
/// unchanged after iterating over an empty iterator (valid() returns false immediately).
#[test]
fn test_empty_iterator_preserves_existing_key_and_value_variables() {
    let out = compile_and_run(
        r#"<?php
class EmptyIteratorImpl implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 1; }
    public function key(): int { return 2; }
    public function next(): void {}
}
$k = 'key';
$v = 'old';
foreach (new EmptyIteratorImpl() as $k => $v) {
}
echo $k;
echo ':';
echo $v;
"#,
    );
    assert_eq!(out, "key:old");
}

/// Tests that when the iterator variable is reused as the foreach value variable,
/// is_iterable() still returns true on the original iterator object after an empty
/// iteration (the iterator is not consumed by the empty loop).
#[test]
fn test_empty_iterator_preserves_receiver_variable_when_reused_as_value() {
    let out = compile_and_run(
        r#"<?php
class EmptyIteratorImpl implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 1; }
    public function key(): int { return 2; }
    public function next(): void {}
}
$it = new EmptyIteratorImpl();
foreach ($it as $it) {
}
echo is_iterable($it) ? 'iterable' : 'lost';
"#,
    );
    assert_eq!(out, "iterable");
}

/// Tests that fresh $k and $v variables declared in a function scope are initialized
/// to null after a foreach over an empty iterator (not left uninitialized).
#[test]
fn test_empty_iterator_initializes_fresh_function_loop_variables_as_null() {
    let out = compile_and_run(
        r#"<?php
class EmptyIteratorImpl implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 1; }
    public function key(): int { return 2; }
    public function next(): void {}
}
function probe(): void {
    foreach (new EmptyIteratorImpl() as $k => $v) {
    }
    echo is_null($k) ? 'null' : 'key';
    echo ':';
    echo is_null($v) ? 'null' : 'value';
}
probe();
"#,
    );
    assert_eq!(out, "null:null");
}

/// Verifies php-src 8.5.8 `ext/simplexml/tests/009.phpt` dispatches every native
/// `SimpleXMLElement` Iterator method through non-null interface-table entries.
#[test]
fn simplexml_foreach_bodyless_native_iterator_slots_match_php() {
    let out = compile_and_run(
        r#"<?php
$sxe = simplexml_load_string(<<<EOF
<?xml version='1.0'?>
<!DOCTYPE sxe SYSTEM "notfound.dtd">
<sxe id="elem1">
 Plain text.
 <elem1 attr1='first'>
  Bla bla 1.
  <!-- comment -->
  <elem2>
   Here we have some text data.
   <elem3>
    And here some more.
    <elem4>
     Wow once again.
    </elem4>
   </elem3>
  </elem2>
 </elem1>
 <elem11 attr2='second'>
  Bla bla 2.
 </elem11>
</sxe>
EOF
);
foreach ($sxe->children() as $name => $value) {
    var_dump($name);
    var_dump(get_class($value));
    var_dump(trim($value));
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(5) \"elem1\"\n",
            "string(16) \"SimpleXMLElement\"\n",
            "string(10) \"Bla bla 1.\"\n",
            "string(6) \"elem11\"\n",
            "string(16) \"SimpleXMLElement\"\n",
            "string(10) \"Bla bla 2.\"\n",
        )
    );
}

/// Verifies SimpleXML native Iterator and string-conversion entries remain materialized for
/// the base class and a non-overriding descendant on every selected codegen target.
#[test]
fn simplexml_native_iterator_vtables_cover_descendants() {
    let dir = make_cli_test_dir("elephc_simplexml_native_iterator_vtables");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class DerivedSimpleXML extends SimpleXMLElement {}
$xml = simplexml_load_string(
    "<root><first>one</first><second>two</second></root>",
    DerivedSimpleXML::class,
);
foreach ($xml->children() as $value) {
    echo trim($value);
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    for (method_key, method_name) in [
        ("rewind", "rewind"),
        ("valid", "valid"),
        ("current", "current"),
        ("key", "key"),
        ("next", "next"),
        ("haschildren", "hasChildren"),
        ("getchildren", "getChildren"),
    ] {
        let symbol = format!("_method_SimpleXMLElement_{method_key}");
        assert!(
            user_asm.contains(&format!("@fn name=SimpleXMLElement::{method_name} symbol={symbol}")),
            "missing native SimpleXML method body for {method_name} on {}",
            target(),
        );
        let table_entry_count = user_asm
            .lines()
            .filter(|line| {
                let line = line.trim();
                if matches!(method_key, "current" | "key") {
                    line.starts_with(".quad _ifacewrap_")
                        && line.ends_with(&format!("_{method_key}"))
                } else {
                    line == format!(".quad {symbol}")
                }
            })
            .count();
        assert!(
            table_entry_count >= 6,
            "base, SimpleXMLIterator, and user descendant tables expose only {table_entry_count} entries for {method_name} on {}",
            target(),
        );
    }
    assert!(
        user_asm
            .lines()
            .filter(|line| {
                line.trim() == ".quad _method_SimpleXMLElement__u__u_tostring"
            })
            .count()
            >= 6,
        "base, SimpleXMLIterator, and user descendant tables did not all reuse SimpleXMLElement::__toString on {}",
        target(),
    );
}

/// Verifies exhausting a temporary concrete Iterator releases its retained object.
#[test]
fn foreach_iterator_cleanup_exhausts_temporary_without_leaking() {
    assert_foreach_cleanup_is_clean(
        r#"foreach (new CleanupRange(0, 3) as $value) { echo $value; }"#,
        "012",
    );
}

/// Verifies the normal foreach exit block releases a retained Iterator after `break`.
#[test]
fn foreach_iterator_cleanup_break_releases_temporary() {
    assert_foreach_cleanup_is_clean(
        r#"
foreach (new CleanupRange(0, 5) as $value) {
    echo $value;
    if ($value === 2) { break; }
}
"#,
        "012",
    );
}

/// Verifies foreach drops only its retain so a borrowed Iterator local remains usable.
#[test]
fn foreach_iterator_cleanup_preserves_borrowed_source() {
    assert_foreach_cleanup_is_clean(
        r#"
$iterator = new CleanupRange(0, 3);
foreach ($iterator as $value) { echo $value; }
echo "|" . $iterator->current();
unset($iterator);
"#,
        "012|3",
    );
}

/// Verifies IteratorAggregate cleanup releases both its result Iterator and temporary source.
#[test]
fn foreach_iterator_cleanup_releases_aggregate_and_result() {
    assert_foreach_cleanup_is_clean(
        r#"foreach (new CleanupAggregate() as $value) { echo $value; }"#,
        "012",
    );
}

/// Verifies the original DOM regression: foreach releases an inline cloned SimpleXML view.
#[test]
fn foreach_iterator_cleanup_releases_cloned_simplexml_view() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$root = simplexml_load_string('<root><A/><B/></root>');
if ($root === false) { exit(2); }
foreach (clone $root->children() as $child) {
    echo $child->getName();
}
unset($root);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "AB");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a function return emits foreach cleanup before leaving the active loop.
#[test]
fn foreach_iterator_cleanup_runs_before_return() {
    assert_foreach_cleanup_is_clean(
        r#"
function leave_foreach(): string {
    foreach (new CleanupRange(4, 7) as $value) {
        return "R" . $value;
    }
    return "missing";
}
echo leave_foreach();
"#,
        "R4",
    );
}

/// Verifies a thrown exception releases the active foreach retain before unwinding.
#[test]
fn foreach_iterator_cleanup_runs_before_throw() {
    assert_foreach_cleanup_is_clean(
        r#"
try {
    foreach (new CleanupRange(0, 3) as $value) {
        throw new Exception("stop");
    }
} catch (Exception $error) {
    echo $error->getMessage();
}
"#,
        "stop",
    );
}

/// Verifies a multi-level break cleans each skipped inner foreach exactly once.
#[test]
fn foreach_iterator_cleanup_runs_for_multilevel_break() {
    assert_foreach_cleanup_is_clean(
        r#"
foreach (new CleanupRange(0, 2) as $outer) {
    foreach (new CleanupRange(5, 7) as $inner) {
        echo $outer . $inner;
        break 2;
    }
}
"#,
        "05",
    );
}

/// Verifies static foreach cleanup emits the target-specific object decref call.
#[test]
fn foreach_iterator_cleanup_emits_object_decref_for_target() {
    let dir = make_cli_test_dir("elephc_foreach_iterator_cleanup_asm");
    let source = format!(
        "{FOREACH_CLEANUP_FIXTURE}\nforeach (new CleanupRange(0, 1) as $value) {{ echo $value; }}"
    );
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(&source, &dir, 8_388_608, false, false);
    let call = match target().arch {
        Arch::AArch64 => "bl __rt_decref_object",
        Arch::X86_64 => "call __rt_decref_object",
    };
    assert!(
        user_asm.contains(call),
        "foreach cleanup did not emit `{call}` on {}:\n{user_asm}",
        target(),
    );
    let _ = fs::remove_dir_all(&dir);
}
