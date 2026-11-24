//! Purpose:
//! Covers boxed-callable conversions that share one module-level selector per policy.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (the codegen core binary).
//!
//! Key details:
//! - Callable names are built at run time so no site sees a finite literal name set.
//! - A class scope keeps its private and protected candidates inline before the shared
//!   public selector, so private shadows and scope-only static methods still bind.
//! - Expected output is what PHP 8.5 prints for the unchanged source.

use crate::support::*;

/// Shared `Closure::fromCallable()` and declared-`callable` conversions keep every scope rule:
/// private, protected and private-shadow instance methods, scope-only static methods by string
/// and by array, public targets from two class scopes, and the `TypeError` for each miss.
#[test]
fn test_shared_callable_selectors_keep_scope_rules_and_errors() {
    let source = r#"<?php
function name(string ...$parts): string { return implode("", $parts); }
function greet(): string { return "greet"; }
class Base {
    protected function prot(): string { return "base-prot"; }
    public function pub(): string { return "base-pub"; }
}
class Holder extends Base {
    private ?Closure $cb = null;
    public function __construct(callable $cb) { $this->cb = Closure::fromCallable($cb); }
    public function run(): string { return ($this->cb)(); }
    private function secret(): string { return "holder-secret"; }
    private static function hidden(): string { return "holder-hidden"; }
    public function bindOwn(string $method): Closure { return Closure::fromCallable([$this, $method]); }
    public function bindOther(object $receiver, string $method): Closure { return Closure::fromCallable([$receiver, $method]); }
    public static function bindStatic(string $method): Closure { return Closure::fromCallable(self::class . "::" . $method); }
    public function bindArrayStatic(string $method): Closure { return Closure::fromCallable([self::class, $method]); }
}
class Child extends Holder { public function secret(): string { return "child-secret"; } }
class Other {
    private ?Closure $cb = null;
    public function __construct(callable $cb) { $this->cb = Closure::fromCallable($cb); }
    public function run(): string { return ($this->cb)(); }
}
function invoke(callable $cb): string { return $cb(); }
function invokeAgain(callable $cb): string { return "again:" . $cb(); }
$h = new Holder(name("gr", "eet"));
echo $h->run(), "\n";
echo $h->bindOwn(name("sec", "ret"))(), "\n";
echo $h->bindOwn(name("pr", "ot"))(), "\n";
echo $h->bindOwn(name("pu", "b"))(), "\n";
echo Holder::bindStatic(name("hid", "den"))(), "\n";
echo $h->bindArrayStatic(name("hid", "den"))(), "\n";
$child = new Child(name("gr", "eet"));
echo $h->bindOther($child, name("sec", "ret"))(), "\n";
echo Closure::fromCallable([$child, name("sec", "ret")])(), "\n";
echo (new Other([new Base(), name("pu", "b")]))->run(), "\n";
echo (new Other(name("gr", "eet")))->run(), "\n";
echo invoke(name("gr", "eet")), "\n";
echo invokeAgain([new Base(), name("pu", "b")]), "\n";
echo invoke(fn(): string => "arrow"), "\n";
foreach ([[$h, name("sec", "ret")], name("Holder::", "hidden"), name("miss", "ing"), [$h, name("miss", "ing")]] as $bad) {
    try { Closure::fromCallable($bad); echo "bad\n"; } catch (TypeError $e) { echo get_class($e), "\n"; }
}
foreach ([name("sec", "ret"), name("miss", "ing")] as $method) {
    try { $h->bindOwn($method); echo "own ok\n"; } catch (TypeError $e) { echo get_class($e), "\n"; }
}
"#;
    for optimize in [true, false] {
        let output = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(output.success, "ir-opt={optimize}: stdout={} stderr={}", output.stdout, output.stderr);
        assert_eq!(output.stdout, "greet\nholder-secret\nbase-prot\nbase-pub\nholder-hidden\nholder-hidden\nholder-secret\nchild-secret\nbase-pub\ngreet\ngreet\nagain:base-pub\narrow\nTypeError\nTypeError\nTypeError\nTypeError\nown ok\nTypeError\n", "ir-opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "ir-opt={optimize}: {}", output.stderr);
    }
}

/// `?Closure` properties and locals invoke their descriptor directly, including after the
/// property is cleared during the call and when the closure throws.
#[test]
fn test_nullable_closure_values_invoke_without_a_selector() {
    let source = r#"<?php
final class Timer {
    private ?Closure $callback;
    public function __construct(callable $callback) { $this->callback = Closure::fromCallable($callback); }
    public function dispatch(): string {
        $callback = $this->callback;
        if ($callback === null) { return "cancelled"; }
        $this->callback = null;
        return $callback("tick");
    }
}
function upper(string $s): string { return strtoupper($s); }
$timer = new Timer(fn(string $s): string => "fired:" . $s);
echo $timer->dispatch(), "|", $timer->dispatch(), "|";
echo (new Timer("upper"))->dispatch(), "|";
$fail = new Timer(function (string $s): string { throw new RuntimeException("boom:" . $s); });
try { $fail->dispatch(); } catch (RuntimeException $e) { echo $e->getMessage(), "|"; }
function maybe(bool $set): ?Closure { return $set ? fn(): string => "set" : null; }
$maybe = maybe(false);
echo $maybe === null ? "null" : $maybe(), "|";
$maybe = maybe(true);
echo $maybe === null ? "null" : $maybe(), "\n";
"#;
    for optimize in [true, false] {
        let output = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(output.success, "ir-opt={optimize}: stdout={} stderr={}", output.stdout, output.stderr);
        assert_eq!(output.stdout, "fired:tick|cancelled|TICK|boom:tick|null|set\n", "ir-opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "ir-opt={optimize}: {}", output.stderr);
    }
}
