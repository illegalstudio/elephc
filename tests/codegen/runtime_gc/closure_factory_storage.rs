//! Purpose:
//! Verifies that `Closure::fromCallable()` results stored in typed `?Closure` properties own
//! their target and release it when the property is cleared or replaced.
//!
//! Called from:
//! - The runtime GC codegen tests.
//!
//! Key details:
//! - Every fixture runs with the EIR optimizer on and off, with heap debug enabled.
//! - Destructor output pins the exact point where a cancelled callback releases its captures.

use crate::support::*;

/// Asserts PHP-equivalent stdout and a clean heap with the EIR optimizer on and off.
fn assert_factory_storage(source: &str, expected: &str) {
    for ir_opt in [true, false] {
        let out = if ir_opt {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(out.success, "ir_opt={ir_opt} stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "ir_opt={ir_opt}\n{}", out.stderr);
        assert!(
            out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir_opt={ir_opt}\n{}",
            out.stderr
        );
    }
}

/// A `callable` parameter normalized by the factory is stored, invoked once and cleared.
#[test]
fn test_closure_factory_nullable_property_invoke_cancel_invoke() {
    assert_factory_storage(r#"<?php
final class Handler {
    private ?Closure $callback;
    public function __construct(callable $callback) { $this->callback = Closure::fromCallable($callback); }
    public function fire(): void { $callback = $this->callback; if ($callback !== null) { $callback(); } }
    public function cancel(): void { $this->callback = null; }
}
function run(): void {
    $handler = new Handler(function(): void { echo "called\n"; });
    $handler->fire(); $handler->cancel(); $handler->fire();
}
run();
"#, "called\n");
}

/// Every callable shape goes through the same factory, and clearing the property releases the
/// captured variable or bound receiver at that statement.
#[test]
fn test_closure_factory_property_releases_each_callable_shape_on_cancel() {
    assert_factory_storage(r#"<?php
final class Probe {
    public function __construct(public string $name) {}
    public function __destruct() { echo "drop {$this->name}\n"; }
    public function ping(): void { echo "ping {$this->name}\n"; }
    public static function stat(): void { echo "static\n"; }
    public function __invoke(): void { echo "invoke {$this->name}\n"; }
}
final class Handler {
    private ?Closure $callback;
    public function __construct(callable $callback) { $this->callback = Closure::fromCallable($callback); }
    public function fire(): void { $callback = $this->callback; if ($callback !== null) { $callback(); } }
    public function cancel(): void { $this->callback = null; }
}
function named(): void { echo "named\n"; }
function run(): void {
    $captured = new Probe("captured");
    $h1 = new Handler(function() use ($captured): void { $captured->ping(); });
    unset($captured);
    $h1->fire(); echo "before cancel 1\n"; $h1->cancel(); echo "after cancel 1\n"; $h1->fire();
    $recv = new Probe("receiver");
    $h2 = new Handler([$recv, "ping"]);
    unset($recv);
    $h2->fire(); $h2->fire(); echo "before cancel 2\n"; $h2->cancel(); echo "after cancel 2\n"; $h2->fire();
    $h3 = new Handler("named"); $h3->fire();
    $h4 = new Handler(["Probe", "stat"]); $h4->fire();
    $h5 = new Handler("Probe::stat"); $h5->fire();
    $h6 = new Handler(new Probe("invokable")); $h6->fire(); echo "before cancel 6\n"; $h6->cancel(); echo "after cancel 6\n";
    $kept = new Probe("kept");
    $h7 = new Handler(fn() => $kept->ping());
    unset($kept);
    $h7->fire();
    $existing = function(): void { echo "existing\n"; };
    $h8 = new Handler($existing); $h8->fire(); $existing();
    echo "end run\n";
}
run();
echo "done\n";
"#, concat!(
        "ping captured\nbefore cancel 1\ndrop captured\nafter cancel 1\n",
        "ping receiver\nping receiver\nbefore cancel 2\ndrop receiver\nafter cancel 2\n",
        "named\nstatic\nstatic\n",
        "invoke invokable\nbefore cancel 6\ndrop invokable\nafter cancel 6\n",
        "ping kept\nexisting\nexisting\nend run\ndrop kept\ndone\n",
    ));
}

/// Retained handles survive in a queue; a callback that cancels its own timer keeps running, and
/// a replaced callback releases the previous captures.
#[test]
fn test_closure_factory_timer_queue_self_cancel_and_replace() {
    assert_factory_storage(r#"<?php
final class Probe {
    public function __construct(public string $name) {}
    public function __destruct() { echo "drop {$this->name}\n"; }
}
final class Timer {
    private ?Closure $callback = null;
    public int $fired = 0;
    public function __construct(callable $callback) { $this->callback = Closure::fromCallable($callback); }
    public function active(): bool { return $this->callback !== null; }
    public function fire(): void {
        $callback = $this->callback;
        if ($callback === null) { return; }
        $this->fired++;
        $callback($this);
    }
    public function cancel(): void { $this->callback = null; }
    public function replace(callable $callback): void { $this->callback = Closure::fromCallable($callback); }
}
final class TimerQueue {
    private array $timers = [];
    public function after(callable $callback): Timer {
        $timer = new Timer($callback);
        $this->timers[] = $timer;
        return $timer;
    }
    public function tick(): void {
        foreach ($this->timers as $timer) { $timer->fire(); }
        $alive = [];
        foreach ($this->timers as $timer) { if ($timer->active()) { $alive[] = $timer; } }
        $this->timers = $alive;
        echo "alive ", count($this->timers), "\n";
    }
}
function run(): void {
    $queue = new TimerQueue();
    $a = new Probe("a");
    $queue->after(function (Timer $t) use ($a): void { echo "once {$a->name} {$t->fired}\n"; $t->cancel(); echo "self-cancelled\n"; });
    unset($a);
    $b = new Probe("b");
    $repeat = $queue->after(function (Timer $t) use ($b): void { echo "repeat {$b->name} {$t->fired}\n"; });
    unset($b);
    $queue->tick();
    $queue->tick();
    $c = new Probe("c");
    $repeat->replace(function (Timer $t) use ($c): void { echo "replaced {$c->name} {$t->fired}\n"; });
    unset($c);
    echo "after replace\n";
    $queue->tick();
    $repeat->cancel();
    echo "after cancel\n";
    $queue->tick();
    echo "end run\n";
}
run();
echo "done\n";
"#, concat!(
        "once a 1\nself-cancelled\ndrop a\nrepeat b 1\nalive 1\nrepeat b 2\nalive 1\n",
        "drop b\nafter replace\nreplaced c 3\nalive 1\ndrop c\nafter cancel\nalive 0\nend run\ndone\n",
    ));
}

/// Static and array-held closures from the factory keep named arguments, variadics and
/// by-reference parameters, and a nullable callable clears the stored closure.
#[test]
fn test_closure_factory_static_and_array_storage_keep_signatures() {
    assert_factory_storage(r#"<?php
declare(strict_types=1);
final class Registry {
    private static ?Closure $hook = null;
    private array $named = [];
    public static function install(?callable $c): void { self::$hook = $c === null ? null : Closure::fromCallable($c); }
    public static function run(string $s): string { $h = self::$hook; return $h === null ? "none" : $h($s); }
    public function on(string $event, callable $c): void { $this->named[$event] = Closure::fromCallable($c); }
    public function off(string $event): void { unset($this->named[$event]); }
    public function emit(string $event, int ...$args): int { $c = $this->named[$event] ?? null; return $c === null ? -1 : $c(...$args); }
}
function bump(int &$n): void { $n++; }
function sum(int ...$xs): int { return array_sum($xs); }
function fmt(string $a, string $b = "!"): string { return $a . $b; }
echo Registry::run("x"), "\n";
Registry::install("strtoupper"); echo Registry::run("x"), "\n";
Registry::install(fn(string $s): string => $s . $s); echo Registry::run("x"), "\n";
Registry::install(null); echo Registry::run("x"), "\n";
$r = new Registry();
$r->on("sum", "sum"); echo $r->emit("sum", 1, 2, 3), "\n";
$r->on("sum", fn(int ...$xs): int => count($xs)); echo $r->emit("sum", 1, 2, 3), "\n";
$r->off("sum"); echo $r->emit("sum", 1), "\n";
$b = Closure::fromCallable("bump"); $v = 1; $b($v); $b($v); echo $v, "\n";
$f = Closure::fromCallable("fmt"); echo $f(b: "?", a: "q"), $f("z"), "\n";
function ret(callable $c): ?Closure { return Closure::fromCallable($c); }
$x = ret("fmt"); echo $x === null ? "null" : $x("ok"), "\n";
"#, "none\nX\nxx\nnone\n6\n3\n-1\n3\nq?z!\nok!\n");
}

/// The shipped timers example keeps its documented output, including capture release on cancel.
#[test]
fn test_closure_factory_timers_example_output() {
    assert_factory_storage(include_str!("../../../examples/timers/main.php"), concat!(
        "t=100ms\n  blink 1\nt=200ms\n  autosave\n  blink 2\n",
        "t=300ms\n  vault door closes\n  (vault door released)\n  blink 3\n",
        "t=400ms\n  autosave\n  cancelling the alarm\n  (gate released)\n",
        "t=500ms\npending timers: 1\ndone\n",
    ));
}
