//! Purpose:
//! Regression tests for PHP code that runs while an exception unwinds: destructors that
//! dispatch methods, interface methods and closures, and generator bodies that return through
//! the coroutine wrapper after a method call.
//!
//! Called from:
//! - `cargo test` through the codegen integration-test harness.
//!
//! Key details:
//! - Method and callable dispatch hold their receiver in the reserved callee-saved nested-call
//!   register (x19 on AArch64, r12 on x86_64). A frame that wrote it without saving it handed
//!   the overwritten value back to whatever called it: the exception unwinder and the frame
//!   cleanup callbacks keep their own state in that register, so the program died with SIGSEGV
//!   after the destructor ran, and the generator wrapper stored the return value through it.
//! - Fixtures run under heap debug and repeat the throwing call, so a destructor that runs
//!   twice, an owner left behind by the unwind, or a per-call imbalance fails the test.
//! - The expected strings are PHP 8.5's output for the same sources.

use crate::support::*;

/// Asserts a heap-debug run exited cleanly with exactly `expected` on stdout and no leak.
fn assert_clean_run(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "stderr={}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A destructor that calls a method runs to completion while an exception leaves its owner.
///
/// This is the reported program verbatim. The destructor ran and so did the method, then the
/// unwinder's cleanup callback restored the frame pointer from the register the method call had
/// just overwritten, and the process died with SIGSEGV before reaching the catch.
#[test]
fn test_destructor_method_call_runs_while_an_exception_unwinds() {
    assert_clean_run(
        r#"<?php
final class Thing
{
    public function __destruct() { echo "destruct\n"; $this->close(); }
    public function close(): void { echo "close\n"; }
}

function run(): void
{
    $thing = new Thing();
    throw new \LogicException('boom');
}

try {
    run();
} catch (\LogicException $e) {
    echo "caught\n";
}
echo "end\n";
"#,
        "destruct\nclose\ncaught\nend\n",
    );
}

/// Arguments, string returns, recursion and a private call chain all survive the unwind.
///
/// Every call made by the destructor goes through the same receiver register, so a deeper or
/// wider call tree is the same defect with more chances to leave the register overwritten.
#[test]
fn test_destructor_call_tree_with_arguments_runs_while_an_exception_unwinds() {
    assert_clean_run(
        r#"<?php
final class Handle
{
    private int $closes = 0;
    public function __construct(private string $name) {}
    public function __destruct()
    {
        echo $this->describe('release', 3), '|';
        $this->close();
    }
    public function describe(string $verb, int $depth): string
    {
        return $depth === 0 ? $verb . ':' . $this->name : $this->describe($verb, $depth - 1);
    }
    public function close(): void
    {
        $this->closes++;
        $this->flush($this->closes);
    }
    private function flush(int $count): void
    {
        echo 'flush', $count, '|';
    }
}
function open(string $name): void
{
    $handle = new Handle($name);
    throw new RuntimeException('open ' . $name);
}
for ($i = 0; $i < 2; $i++) {
    try {
        open('h' . $i);
    } catch (RuntimeException $e) {
        echo 'caught:', $e->getMessage(), '|';
    }
}
echo "end\n";
"#,
        "release:h0|flush1|caught:open h0|release:h1|flush1|caught:open h1|end\n",
    );
}

/// Interface dispatch and a closure call from a destructor both survive the unwind.
///
/// Interface dispatch and callable-descriptor invocation hold their receiver or descriptor in
/// the same register as a direct method call, and neither was saved for a typed receiver.
#[test]
fn test_destructor_interface_and_closure_calls_run_while_an_exception_unwinds() {
    assert_clean_run(
        r#"<?php
interface Releaser
{
    public function release(string $what): void;
}
final class EchoReleaser implements Releaser
{
    public function release(string $what): void
    {
        echo 'release:', $what, '|';
    }
}
final class Texture
{
    private $onClose;
    public function __construct(private Releaser $releaser)
    {
        $this->onClose = static function (string $what): void {
            echo 'closed:', $what, '|';
        };
    }
    public function __destruct()
    {
        $this->releaser->release('texture');
        ($this->onClose)('texture');
    }
}
function load(Releaser $releaser): void
{
    $texture = new Texture($releaser);
    throw new LogicException('load failed');
}
$releaser = new EchoReleaser();
for ($i = 0; $i < 2; $i++) {
    try {
        load($releaser);
    } catch (LogicException $e) {
        echo 'caught|';
    }
}
echo "end\n";
"#,
        "release:texture|closed:texture|caught|release:texture|closed:texture|caught|end\n",
    );
}

/// A throw from the destructor's callee joins the exception that was already unwinding.
///
/// PHP delivers the destructor's exception with the original one as its previous. The callee's
/// throw re-enters the unwinder from inside the destructor's own call tree, the other way back
/// to the same cleanup callback.
#[test]
fn test_destructor_callee_throw_chains_the_exception_being_unwound() {
    assert_clean_run(
        r#"<?php
final class Stream
{
    public function __destruct()
    {
        echo 'destruct|';
        $this->close();
        echo 'unreached|';
    }
    public function close(): void
    {
        echo 'close|';
        throw new RuntimeException('close failed');
    }
}
function write(): void
{
    $stream = new Stream();
    throw new LogicException('write failed');
}
for ($i = 0; $i < 2; $i++) {
    try {
        write();
    } catch (Throwable $e) {
        $previous = $e->getPrevious();
        echo get_class($e), ':', $e->getMessage(), '<-';
        echo $previous === null ? 'none' : get_class($previous) . ':' . $previous->getMessage(), '|';
    }
}
echo "end\n";
"#,
        "destruct|close|RuntimeException:close failed<-LogicException:write failed|\
destruct|close|RuntimeException:close failed<-LogicException:write failed|end\n",
    );
}

/// A RAII object owned by a method frame releases itself when its own method throws.
///
/// This is the engine shape that failed: a window whose destructor delegates to `close()`, held
/// in a local of a test method, while the window's argument check throws through two method
/// frames before the catch.
#[test]
fn test_raii_destructor_runs_when_its_own_method_throws_through_method_frames() {
    assert_clean_run(
        r#"<?php
final class Window
{
    private bool $open = true;
    public function __destruct()
    {
        $this->close();
    }
    public function close(): void
    {
        if (!$this->open) {
            return;
        }
        $this->open = false;
        echo 'closed|';
    }
    public function drawRects(int $count): void
    {
        if (!$this->open) {
            throw new LogicException('window is closed');
        }
        if ($count > 4) {
            throw new InvalidArgumentException('count ' . $count . ' exceeds 4');
        }
        echo 'drew|';
    }
}
final class WindowTest
{
    public function testRejectsACountPastTheRectangles(): void
    {
        $window = new Window();
        $window->drawRects(9);
    }
    public function testRejectsDrawingAfterClose(): void
    {
        $window = new Window();
        $window->close();
        $window->drawRects(1);
    }
}
$test = new WindowTest();
for ($i = 0; $i < 2; $i++) {
    try {
        $test->testRejectsACountPastTheRectangles();
    } catch (InvalidArgumentException $e) {
        echo 'caught:', $e->getMessage(), '|';
    }
    try {
        $test->testRejectsDrawingAfterClose();
    } catch (LogicException $e) {
        echo 'caught:', $e->getMessage(), '|';
    }
}
echo "end\n";
"#,
        "closed|caught:count 9 exceeds 4|closed|caught:window is closed|\
closed|caught:count 9 exceeds 4|closed|caught:window is closed|end\n",
    );
}

/// A generator body that calls a method on a typed object still publishes its return value.
///
/// The coroutine entry wrapper keeps the Generator in the same callee-saved register while the
/// body runs, then stores the return value through it. The body's method call overwrote it, so
/// `getReturn()` read nothing and the value was written into the receiver object instead.
#[test]
fn test_generator_return_survives_a_typed_method_call_in_the_body() {
    assert_clean_run(
        r#"<?php
final class Counter
{
    public int $n = 0;
    public function bump(int $by): void
    {
        $this->n += $by;
    }
}
function steps(Counter $counter)
{
    yield 1;
    $counter->bump(2);
    yield 2;
    $counter->bump(3);
    return 'done';
}
$counter = new Counter();
$gen = steps($counter);
foreach ($gen as $step) {
    echo $step, '|';
}
echo $gen->getReturn(), '|', $counter->n, "\n";
"#,
        "1|2|done|5\n",
    );
}

/// The destructor's prologue saves the nested-call register before the method call writes it.
///
/// Pins the calling convention itself on the test target, independently of whether any caller
/// happens to observe the overwritten value. The overriding subclass keeps `close()` a virtual
/// call, so the destructor really dispatches through the receiver register.
#[test]
fn test_destructor_method_call_saves_the_nested_call_register() {
    let dir = make_cli_test_dir("elephc_destructor_nested_reg");
    let (user_asm, _runtime_asm, _libraries) = compile_source_to_asm_with_options(
        r#"<?php
class Thing
{
    public function __destruct() { $this->close(); }
    public function close(): void { echo "close\n"; }
}
final class LoudThing extends Thing
{
    public function close(): void { echo "CLOSE\n"; }
}
function run(): void { $thing = new Thing(); $loud = new LoudThing(); }
run();
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let body = user_asm
        .split("@fn name=Thing::__destruct")
        .nth(1)
        .expect("the destructor must be emitted")
        .split("@endfn")
        .next()
        .expect("the destructor must be terminated");
    let register = if target().arch == Arch::X86_64 { "r12" } else { "x19" };
    let writes = [format!("mov {register},"), format!("ldr {register},"), format!("ldur {register},")];
    let first_write = body
        .lines()
        .position(|line| writes.iter().any(|write| line.trim().starts_with(write.as_str())))
        .expect("the method call must hold its receiver in the nested-call register");
    let saved_before = body.lines().take(first_write).any(|line| {
        let line = line.trim();
        line.starts_with(&format!("stur {register},"))
            || line.starts_with(&format!("str {register},"))
            || line.starts_with(&format!("push {register}"))
            || (line.starts_with("mov QWORD PTR [rbp -") && line.ends_with(&format!(", {register}")))
    });
    assert!(saved_before, "the nested-call register must be saved before the call writes it:\n{body}");

    let _ = fs::remove_dir_all(&dir);
}
