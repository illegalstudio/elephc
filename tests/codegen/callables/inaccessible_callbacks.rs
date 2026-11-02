//! Purpose:
//! Pins php's behaviour for callable arrays that name a method the caller cannot reach.
//!
//! Called from:
//! - The codegen integration harness through the callables module.
//!
//! Key details:
//! - A callable validated by a builtin (`call_user_func()`, `array_map()`, `usort()` ...) throws a
//!   `TypeError` ending in `cannot access protected method C::m()`; a callable that is called
//!   (`$f()`) throws `Error: Call to protected method C::m() from global scope`.
//! - A class with `__call()` takes the call instead, an inherited private method resolves from the
//!   declaring scope, and every run asserts a clean heap-debug leak summary.

use crate::support::*;

/// A callback builtin rejects `[$object, 'method']` naming a protected or private method the
/// caller cannot reach with php's own `TypeError`, wording the builtin, the argument and the
/// method exactly as php 8.5 does. Expected output is `php -n`'s.
#[test]
fn test_inaccessible_callback_names_the_builtin_and_the_method() {
    let out = compile_and_run_with_heap_debug(r##"<?php
class Box {
    protected function doIt($a, $b = 0) { return $a <=> $b; }
    private function Secret($a) { return "secret:$a"; }
    public function shown($a) { return "shown:$a"; }
}
function attempt(callable|Closure $run): void {
    try { $r = $run(); echo is_array($r) ? implode(',', $r) : var_export($r, true), "\n"; }
    catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
}
$box = new Box;
$xs = [3, 1, 2];
attempt(fn() => call_user_func([$box, 'doIt'], 1));
attempt(fn() => call_user_func_array([$box, 'Secret'], [1]));
attempt(fn() => array_map([$box, 'doIt'], $xs));
attempt(fn() => array_filter($xs, [$box, 'doIt']));
attempt(function () use ($xs, $box) { usort($xs, [$box, 'doIt']); return $xs; });
attempt(function () use ($xs, $box) { uasort($xs, [$box, 'doIt']); return $xs; });
attempt(function () use ($xs, $box) { uksort($xs, [$box, 'doIt']); return $xs; });
attempt(function () use ($xs, $box) { array_walk($xs, [$box, 'doIt']); return $xs; });
attempt(fn() => array_reduce($xs, [$box, 'doIt'], 0));
attempt(fn() => array_udiff($xs, $xs, [$box, 'doIt']));
attempt(fn() => array_uintersect_assoc($xs, $xs, [$box, 'doIt']));
attempt(fn() => array_diff_ukey($xs, $xs, [$box, 'doIt']));
attempt(fn() => array_all($xs, [$box, 'doIt']));
attempt(fn() => array_find($xs, [$box, 'doIt']));
attempt(fn() => array_map([$box, 'shown'], $xs));
"##);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r##"TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: call_user_func_array(): Argument #1 ($callback) must be a valid callback, cannot access private method Box::Secret()
TypeError: array_map(): Argument #1 ($callback) must be a valid callback or null, cannot access protected method Box::doIt()
TypeError: array_filter(): Argument #2 ($callback) must be a valid callback or null, cannot access protected method Box::doIt()
TypeError: usort(): Argument #2 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: uasort(): Argument #2 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: uksort(): Argument #2 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: array_walk(): Argument #2 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: array_reduce(): Argument #2 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: array_udiff(): Argument #3 must be a valid callback, cannot access protected method Box::doIt()
TypeError: array_uintersect_assoc(): Argument #3 must be a valid callback, cannot access protected method Box::doIt()
TypeError: array_diff_ukey(): Argument #3 must be a valid callback, cannot access protected method Box::doIt()
TypeError: array_all(): Argument #2 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: array_find(): Argument #2 ($callback) must be a valid callback, cannot access protected method Box::doIt()
shown:3,shown:1,shown:2
"##, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The same callable array raises the callback-validation `TypeError` through `call_user_func()`
/// and `call_user_func_array()`, the direct-call `Error` through `$f()`, and keeps resolving when
/// the caller is inside the hierarchy. The direct-call `Error` quotes the method as the caller
/// spelled it and names the calling scope. Expected output is `php -n`'s.
#[test]
fn test_inaccessible_callable_array_errors_follow_the_calling_scope() {
    let out = compile_and_run_with_heap_debug(r##"<?php
class Box {
    protected function doIt($a) { return "doIt:$a"; }
    private function Secret($a) { return "secret:$a"; }
    public function shown($a) { return "shown:$a"; }
}
class Other {
    public function probe(Box $b, string $m): string {
        try { $f = [$b, $m]; return $f(1); }
        catch (Throwable $e) { return get_class($e) . ': ' . $e->getMessage(); }
    }
    public function viaCuf(Box $b, string $m): string {
        try { return call_user_func([$b, $m], 2); }
        catch (Throwable $e) { return get_class($e) . ': ' . $e->getMessage(); }
    }
}
class Kid extends Box {
    public function reach(string $m): string {
        try { return call_user_func([$this, $m], 3); }
        catch (Throwable $e) { return get_class($e) . ': ' . $e->getMessage(); }
    }
}
function viaCallUserFunc($cb) { return call_user_func($cb, 1); }
function viaCallUserFuncArray($cb) { return call_user_func_array($cb, [2]); }
function viaVariableCall($cb) { return $cb(3); }
function viaArrayMap($cb) { return array_map($cb, [4]); }
function attempt(Closure $run): void {
    try { $r = $run(); echo is_array($r) ? implode(',', $r) : $r, "\n"; }
    catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
}
$box = new Box;
$protected = $argc > 5 ? 'x' : 'doIt';
$private = $argc > 5 ? 'x' : 'Secret';
$shouted = $argc > 5 ? 'x' : 'DOIT';
foreach ([[$box, $protected], [$box, $private], [$box, 'shown'], [$box, $shouted]] as $callback) {
    attempt(fn() => viaCallUserFunc($callback));
    attempt(fn() => viaCallUserFuncArray($callback));
    attempt(fn() => viaVariableCall($callback));
    attempt(fn() => viaArrayMap($callback));
}
echo (new Other)->probe($box, $protected), "\n";
echo (new Other)->probe($box, $private), "\n";
echo (new Other)->probe($box, 'shown'), "\n";
echo (new Other)->viaCuf($box, $protected), "\n";
echo (new Other)->viaCuf($box, $private), "\n";
echo (new Kid)->reach($protected), "\n";
echo (new Kid)->reach($private), "\n";
echo (new Kid)->reach('shown'), "\n";
"##);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r##"TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: call_user_func_array(): Argument #1 ($callback) must be a valid callback, cannot access protected method Box::doIt()
Error: Call to protected method Box::doIt() from global scope
TypeError: array_map(): Argument #1 ($callback) must be a valid callback or null, cannot access protected method Box::doIt()
TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method Box::Secret()
TypeError: call_user_func_array(): Argument #1 ($callback) must be a valid callback, cannot access private method Box::Secret()
Error: Call to private method Box::Secret() from global scope
TypeError: array_map(): Argument #1 ($callback) must be a valid callback or null, cannot access private method Box::Secret()
shown:1
shown:2
shown:3
shown:4
TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: call_user_func_array(): Argument #1 ($callback) must be a valid callback, cannot access protected method Box::doIt()
Error: Call to protected method Box::DOIT() from global scope
TypeError: array_map(): Argument #1 ($callback) must be a valid callback or null, cannot access protected method Box::doIt()
Error: Call to protected method Box::doIt() from scope Other
Error: Call to private method Box::Secret() from scope Other
shown:1
TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access protected method Box::doIt()
TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method Box::Secret()
doIt:3
TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method Kid::Secret()
shown:3
"##, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A class with `__call()` receives a callable array that names an unreachable or unknown
/// method, a private method of the declaring class resolves on a descendant's instance from inside
/// that class and is refused from outside, and a by-value variadic collects a plain variable
/// handed through `call_user_func()`. Expected output is `php -n`'s.
#[test]
fn test_inaccessible_callable_array_falls_back_to_call_and_parent_private() {
    let out = compile_and_run_with_heap_debug(r##"<?php
class Magic {
    protected function hidden($a) { return "hidden:$a"; }
    private function Secret($a) { return "secret:$a"; }
    public function __call($name, $args) { return "__call:$name:" . implode(',', $args); }
}
class Plain {
    protected function hidden($a) { return "hidden:$a"; }
}
class Parental {
    private function priv($a) { return "Parental::priv:$a"; }
    protected function prot($a) { return "Parental::prot:$a"; }
    public function direct(Parental $o, $a) { return call_user_func([$o, 'priv'], $a); }
    public function viaMap(Parental $o, array $xs) { return implode(',', array_map([$o, 'priv'], $xs)); }
    public function viaVariable(Parental $o, $a) { $f = [$o, 'priv']; return $f($a); }
    public function viaArray(Parental $o, $a) { return call_user_func_array([$o, 'priv'], [$a]); }
    public function viaSort(Parental $o, array $xs) { usort($xs, [$o, 'cmp']); return implode(',', $xs); }
    private function cmp($a, $b) { return $b <=> $a; }
}
class Child extends Parental {
    public function prot($a) { return "Child::prot:$a"; }
}
function viaCallUserFunc($cb, $arg) { return call_user_func($cb, $arg); }
function viaCallUserFuncArray($cb, $arg) { return call_user_func_array($cb, [$arg, 'tail']); }
function viaVariableCall($cb, $arg) { return $cb($arg, 'tail'); }
function viaArrayMap($cb, $xs) { return implode(' ', array_map($cb, $xs)); }
function attempt(Closure $run): void {
    try { $r = $run(); echo is_array($r) ? implode(',', $r) : var_export($r, true), "\n"; }
    catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
}
$magic = new Magic;
$plain = new Plain;
$protected = $argc > 5 ? 'x' : 'hidden';
$unknown = $argc > 5 ? 'x' : 'whatever';
$private = $argc > 5 ? 'x' : 'Secret';
foreach ([[$magic, $protected], [$magic, $private], [$magic, $unknown], [$plain, $protected]] as $callback) {
    attempt(fn() => viaCallUserFunc($callback, 'one'));
    attempt(fn() => viaCallUserFuncArray($callback, 'two'));
    attempt(fn() => viaVariableCall($callback, 'three'));
    attempt(fn() => viaArrayMap($callback, ['x', 'y']));
    attempt(fn() => is_callable($callback));
}
$child = new Child;
$parental = new Parental;
echo $parental->direct($child, 1), "\n";
echo $parental->viaMap($child, [1, 2]), "\n";
echo $parental->viaVariable($child, 3), "\n";
echo $parental->viaArray($child, 4), "\n";
echo $parental->viaSort($child, [1, 3, 2]), "\n";
$private = $argc > 5 ? 'x' : 'priv';
attempt(fn() => viaCallUserFunc([$child, $private], 5));
attempt(fn() => call_user_func([$child, 'priv'], 6));
attempt(fn() => is_callable([$child, $private]));
$f = function (...$parts) { return 'v:' . implode(',', $parts); };
$boxed = [$f];
attempt(fn() => viaCallUserFunc($boxed[0], 'first'));
attempt(fn() => viaCallUserFuncArray($boxed[0], 'first'));
attempt(fn() => viaVariableCall($boxed[0], 'first'));
"##);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r##"'__call:hidden:one'
'__call:hidden:two,tail'
'__call:hidden:three,tail'
'__call:hidden:x __call:hidden:y'
true
'__call:Secret:one'
'__call:Secret:two,tail'
'__call:Secret:three,tail'
'__call:Secret:x __call:Secret:y'
true
'__call:whatever:one'
'__call:whatever:two,tail'
'__call:whatever:three,tail'
'__call:whatever:x __call:whatever:y'
true
TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access protected method Plain::hidden()
TypeError: call_user_func_array(): Argument #1 ($callback) must be a valid callback, cannot access protected method Plain::hidden()
Error: Call to protected method Plain::hidden() from global scope
TypeError: array_map(): Argument #1 ($callback) must be a valid callback or null, cannot access protected method Plain::hidden()
false
Parental::priv:1
Parental::priv:1,Parental::priv:2
Parental::priv:3
Parental::priv:4
3,2,1
TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method Child::priv()
TypeError: call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method Child::priv()
false
'v:first'
'v:first,tail'
'v:first,tail'
"##, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Every supported target emits the unreachable-method diagnostic and the `__call()` forwarder.
///
/// Host execution covers one architecture only, and the two halves of the emitter are written
/// independently, so the fixture is emitted for every target and each one must carry the
/// message fragments and the forwarder helper.
#[test]
fn test_inaccessible_callable_array_paths_are_emitted_for_every_target() {
    const FIXTURE: &str = r##"<?php
class Hidden { protected function secret($a) { return $a; } }
class Magic { public function __call($name, $args) { return $name; } }
function viaCallUserFunc($cb) { return call_user_func($cb, 1); }
function viaCallUserFuncArray($cb) { return call_user_func_array($cb, [2]); }
function viaVariable($cb) { return $cb(3); }
$a = $argc > 1 ? [new Magic, 'anything'] : [new Hidden, 'secret'];
echo viaCallUserFunc($a), viaCallUserFuncArray($a), viaVariable($a);
"##;
    for target in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
        let dir = make_cli_test_dir("elephc_inaccessible_callable_targets");
        let php_path = dir.join("main.php");
        std::fs::write(&php_path, FIXTURE).expect("write inaccessible callable fixture");
        let output = elephc_cli_command(&dir)
            .args(["--emit-asm", "--target", target])
            .arg(&php_path)
            .output()
            .expect("run elephc --emit-asm");
        assert!(
            output.status.success(),
            "{target}: emitting assembly failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let assembly = std::fs::read_to_string(php_path.with_extension("s")).expect("read assembly");
        let _ = std::fs::remove_dir_all(dir);
        for fragment in [
            ", cannot access ",
            "call_user_func(): Argument #1 ($callback) must be a valid callback",
            "call_user_func_array(): Argument #1 ($callback) must be a valid callback",
            "protected method Hidden::",
            "magic_u_callable",
        ] {
            assert!(assembly.contains(fragment), "{target}: assembly lacks {fragment:?}");
        }
    }
}

/// A callable array naming the class's own private method, stored in a property and invoked
/// later from another method of the class, is reachable: php checks visibility in the scope the
/// callable is CALLED from. Symfony's `ErrorHandler` keeps `[$this, 'renderException']` in
/// `$exceptionHandler` and calls it from `handleException()`. The open Mixed dispatcher used to
/// be one module-wide body with no lexical class, so every such call died with `Call to private
/// method ... from global scope`. Expected output is `php -n`'s; the last line pins that the
/// same callable is still refused from outside the class.
#[test]
fn test_private_callable_array_invoked_from_its_own_class_scope() {
    let out = compile_and_run_with_heap_debug(r##"<?php
class Handler {
    private $h;
    private $log = [];
    public static function register(): self {
        $x = new self;
        $x->set([$x, 'render']);
        return $x;
    }
    public function set($h) { $this->h = $h; }
    public function handle(string $m): void {
        $h = $this->h;
        $this->h = [$this, 'render'];
        $h($m);
        call_user_func($this->h, $m . '2');
        call_user_func_array($this->h, [$m . '3']);
        $c = $this->h;
        $c($m . '4');
    }
    public function pass(): array { return [$this, 'render']; }
    private function render(string $m): void { echo "render ", $m, "\n"; }
}
$h = Handler::register();
$h->handle("boom");
try { $f = $h->pass(); $f("outside"); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
"##);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "render boom\nrender boom2\nrender boom3\nrender boom4\nError: Call to private method Handler::render() from global scope\n"
    );
}
