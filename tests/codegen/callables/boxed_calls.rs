//! Purpose:
//! Exercises direct calls through boxed values read from declared PHP arrays.
//!
//! Called from:
//! - The codegen integration harness through the callables module.
//!
//! Key details:
//! - Unknown signatures must use descriptor argument binding and retain callback owners.

use crate::support::*;

/// Opaque eval converts boxed native method callbacks without shifting overflow arguments.
#[test]
fn test_opaque_eval_native_method_descriptors_preserve_stack_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeEvalStack {
    public static function sum(int $a, int $b, int $c, int $d, int $e, int $f, int $g, int $h, int $i): int {
        return $a + $b + $c + $d + $e + $f + $g + $h + $i;
    }
}
function invokeEvalNativeStack(callable $callback): int { return $callback(1, 2, 3, 4, 5, 6, 7, 8, 9); }
function invokeEvalNativeConstructor(callable $callback): void { $callback('fresh', 7, E_WARNING, 'reported.php', 9, null); }
$error = new ErrorException('old');
$source = 'invokeEvalNativeConstructor([$error, "__construct"]); echo invokeEvalNativeStack([NativeEvalStack::class, "sum"]), ";", $error->getMessage(), ":", $error->getLine();';
eval($source . ' // ' . $argc);
unset($error, $source);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "45;fresh:9", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Static and late-bound descriptors preserve arguments that overflow the native registers.
#[test]
fn test_boxed_method_descriptors_preserve_stack_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class StackDescriptorBase {
    public static function sum(int $a, int $b, int $c, int $d, int $e, int $f, int $g, int $h, int $i): int {
        return $a + $b + $c + $d + $e + $f + $g + $h + $i;
    }
    public static function choose(): callable { return static::sum(...); }
}
class StackDescriptorChild extends StackDescriptorBase {
    public static function sum(int $a, int $b, int $c, int $d, int $e, int $f, int $g, int $h, int $i): int {
        return 100 + $a + $b + $c + $d + $e + $f + $g + $h + $i;
    }
}
function stackDescriptorCalls(array $callbacks): void {
    foreach ($callbacks as $callback) { echo $callback(...[1, 2, 3, 4, 5, 6, 7, 8, 9]), ';'; }
}
stackDescriptorCalls([StackDescriptorBase::sum(...), StackDescriptorBase::choose(), StackDescriptorChild::choose()]);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "45;45;145;", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Variable and expression calls dispatch every boxed callable shape after the source is released.
#[test]
fn test_boxed_array_reads_dispatch_direct_callable_shapes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function boxedGreeting(string $name): string { return "function:" . $name; }
class BoxedGreeter {
    public function greet(string $name): string { return "method:" . $name; }
    public static function staticGreet(string $name): string { return "static:" . $name; }
    public function __invoke(string $name): string { return "object:" . $name; }
}
function boxedCallbacks(string $prefix): array {
    $object = new BoxedGreeter();
    return [
        function(string $name) use ($prefix): string { return $prefix . $name; },
        "boxedGreeting", [$object, "greet"], ["BoxedGreeter", "staticGreet"], $object
    ];
}
$callbacks = boxedCallbacks(str_repeat("c", 3));
$closure = $callbacks[0];
$function = $callbacks[1];
$method = $callbacks[2];
$static = $callbacks[3];
$object = $callbacks[4];
echo $callbacks[0]("expr"), "|";
unset($callbacks);
echo $closure("local"), "|", $function("name"), "|", $method("name"), "|";
echo $static("name"), "|", $object("name");
unset($closure, $function, $method, $static, $object);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "cccexpr|ccclocal|function:name|method:name|static:name|object:name", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed direct calls preserve named binding, positional unpacking and by-reference arguments.
#[test]
fn test_boxed_direct_calls_bind_named_spread_and_reference_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function boxedCallTargets(): array {
    return [
        function(int $left, int $right): int { return $left * 10 + $right; },
        function(int &$value): void { $value += 7; },
        function(mixed &$value): void { $value = "mixed"; }
    ];
}
$callbacks = boxedCallTargets();
$callback = $callbacks[0];
echo $callback(right: 2, left: 1), "|", $callbacks[0](...[3, 4]), "|";
$value = 5;
$callbacks[1]($value);
echo $value, "|";
$mixed = 1;
$callbacks[2]($mixed);
echo $mixed;
unset($callback, $callbacks);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "12|34|12|mixed", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Resolves a callable array to a PROTECTED method when it is invoked from inside the hierarchy.
///
/// The runtime callable ladder kept public instance methods only, so Laravel's
/// `Container::getClosure()` closure -- `$container->resolve($c, $p, raiseEvents: false)`,
/// lowered as `call_user_func_array([$container, 'resolve'], ...)` -- found no target and the
/// overridden protected `Application::resolve()` fell to `__call`. Expected output is `php -n`'s.
#[test]
fn test_callable_array_reaches_a_protected_method_from_inside_the_hierarchy() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Container {
    protected function resolve($a, $p = [], $raiseEvents = true) { return "C:$a:" . ($raiseEvents ? 'r' : 'n'); }
    public function pos($c) { return call_user_func_array([$c, 'resolve'], ['x', []]); }
    public function cuf($c) { return call_user_func([$c, 'resolve'], 'z'); }
    public function named($c) { return call_user_func_array([$c, 'resolve'], ['w', [], 'raiseEvents' => false]); }
    public function getClosure($abstract) {
        return function ($container, $parameters = []) use ($abstract) {
            return $container->resolve($abstract, $parameters, raiseEvents: false);
        };
    }
}
class App extends Container {
    public function __call($m, $args) { return "__call($m)"; }
    protected function resolve($a, $p = [], $raiseEvents = true) { return "A:" . parent::resolve($a, $p, $raiseEvents); }
}
$app = $argc > 5 ? new Container() : new App();
echo $app->pos($app), "|", $app->cuf($app), "|", $app->named($app), "|", ($app->getClosure('k'))($app), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "A:C:x:r|A:C:z:r|A:C:w:n|A:C:k:n\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// A callable-array site with more candidates than the inline ladder resolves through a shared
/// table of PUBLIC methods plus a small table of the methods its own class scope reaches.
///
/// One table holding both made every class scope a near-copy of the whole public universe
/// (Laravel `--web`: ~245 tables of 3.67 MB differing by 14 of 23 383 entries, 897 MB of
/// `__data`, an image dyld could no longer map). The asm assertion is what pins the split; the
/// output pins that the scope table still reaches `protected` and `private` methods.
#[test]
fn test_callable_array_table_splits_public_and_scope_methods() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(
        r#"<?php
class P0 { public function m0($a) { return "p0:$a"; } }
class P1 { public function m1($a) { return "p1:$a"; } }
class P2 { public function m2($a) { return "p2:$a"; } }
class P3 { public function m3($a) { return "p3:$a"; } }
class P4 { public function m4($a) { return "p4:$a"; } }
class P5 { public function m5($a) { return "p5:$a"; } }
class P6 { public function m6($a) { return "p6:$a"; } }
class P7 { public function m7($a) { return "p7:$a"; } }
class P8 { public function m8($a) { return "p8:$a"; } }
class Box {
    protected function hidden($a) { return "hidden:$a"; }
    private function secret($a) { return "secret:$a"; }
    public function shown($a) { return "shown:$a"; }
    public function call($target, string $method, $arg) { return call_user_func([$target, $method], $arg); }
}
function outside($target, string $method, $arg) { return call_user_func([$target, $method], $arg); }
$objects = [new P0, new P1, new P2, new P3, new P4, new P5, new P6, new P7, new P8];
$box = new Box;
$n = 3;
for ($i = 0; $i < $n; $i++) {
    $out = [];
    foreach ($objects as $k => $o) {
        $out[] = $box->call($o, "m$k", $i);
        $out[] = outside($o, "m$k", $i);
    }
    $out[] = $box->call($box, 'hidden', $i);
    $out[] = $box->call($box, 'secret', $i);
    $out[] = $box->call($box, 'shown', $i);
    $out[] = outside($box, 'shown', $i);
}
echo implode(',', $out), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "p0:2,p0:2,p1:2,p1:2,p2:2,p2:2,p3:2,p3:2,p4:2,p4:2,p5:2,p5:2,p6:2,p6:2,p7:2,p7:2,p8:2,p8:2,hidden:2,secret:2,shown:2,shown:2\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(asm.contains("callable_array_instance_scope_hit"), "scope table not split out");
}
