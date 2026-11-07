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

/// The tables one function's composite callable lookups load, by the function's symbol part.
fn composite_tables_of(asm: &str, function: &str) -> std::collections::BTreeSet<String> {
    let mut tables = std::collections::BTreeSet::new();
    let mut inside = false;
    let mut last_symbol = String::new();
    for line in asm.lines() {
        let label = line.trim_start_matches('_').trim_end_matches(':');
        if label == function {
            inside = true;
        } else if label == format!("{function}_epilogue") {
            inside = false;
        }
        if !inside {
            continue;
        }
        if let Some(start) = line.find("data_") {
            let rest = &line[start..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            last_symbol = rest[..end].to_string();
        }
        if line.contains("__rt_callable_lookup_composite_") {
            tables.insert(last_symbol.clone());
        }
    }
    tables
}

/// `[$object, 'name']` reaching a static method searches one table of PUBLIC statics that every
/// class scope shares, then the few the scope itself reaches. One table holding both made each
/// scope a near-copy of the whole public universe: on the Symfony Demo, 357 tables of 917 KB that
/// differed by 44 rows out of 4 151, 330 MB of `__data`. The container's generated getters are one
/// class each, which is why the copies were so many.
#[test]
fn test_object_selector_static_table_is_shared_between_scopes() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(
        r#"<?php
class P0 { public static function m0($a) { return "p0:$a"; } }
class P1 { public static function m1($a) { return "p1:$a"; } }
class P2 { public static function m2($a) { return "p2:$a"; } }
class P3 { public static function m3($a) { return "p3:$a"; } }
class P4 { public static function m4($a) { return "p4:$a"; } }
class P5 { public static function m5($a) { return "p5:$a"; } }
class P6 { public static function m6($a) { return "p6:$a"; } }
class P7 { public static function m7($a) { return "p7:$a"; } }
class P8 { public static function m8($a) { return "p8:$a"; } }
class First {
    protected static function hidden($a) { return "first:$a"; }
    public function callIt($target, string $method, $arg) { return call_user_func([$target, $method], $arg); }
}
class Second {
    private static function secret($a) { return "second:$a"; }
    public function callIt($target, string $method, $arg) { return call_user_func([$target, $method], $arg); }
}
$objects = [new P0, new P1, new P2, new P3, new P4, new P5, new P6, new P7, new P8];
$first = new First;
$second = new Second;
$out = [];
foreach ($objects as $k => $o) {
    $out[] = $first->callIt($o, "m$k", 1);
    $out[] = $second->callIt($o, "m$k", 2);
}
$out[] = $first->callIt($first, 'hidden', 3);
$out[] = $second->callIt($second, 'secret', 4);
echo implode(',', $out), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "p0:1,p0:2,p1:1,p1:2,p2:1,p2:2,p3:1,p3:2,p4:1,p4:2,p5:1,p5:2,p6:1,p6:2,p7:1,p7:2,p8:1,p8:2,first:3,second:4\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let searches = callable_search_tables(&asm)
        .into_iter()
        .filter(|tables| !tables.is_empty())
        .collect::<Vec<_>>();
    assert!(!searches.is_empty(), "the site no longer uses a table");
    let shared = searches[0]
        .iter()
        .filter(|table| searches[1..].iter().all(|tables| tables.contains(*table)))
        .count();
    assert!(shared > 0, "each class scope built its own table: {searches:?}");
}

/// The composite tables each shared callable-array search reads.
fn callable_search_tables(asm: &str) -> Vec<std::collections::BTreeSet<String>> {
    let mut searches = Vec::new();
    let mut current: Option<std::collections::BTreeSet<String>> = None;
    let mut last_symbol = String::new();
    for line in asm.lines() {
        if line.starts_with('_') && line.ends_with(':') {
            searches.extend(current.take());
            if line.starts_with("_eir_shared_callable_array_lookup_") {
                current = Some(Default::default());
            }
        }
        let Some(tables) = current.as_mut() else {
            continue;
        };
        if let Some(start) = line.find("data_") {
            let rest = &line[start..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            last_symbol = rest[..end].to_string();
        }
        if line.contains("__rt_callable_lookup_composite_") {
            tables.insert(last_symbol.clone());
        }
    }
    searches.extend(current);
    searches
}

/// `new static(...$parts)` builds the lexical class or a descendant, nothing else. A constructor
/// that reads `func_num_args()` took a path with no such limit: every class of the program got a
/// branch, 8 M lines for Symfony's `RememberMeDetails::fromRawCookie()`.
#[test]
fn test_new_static_with_a_counting_constructor_plans_only_descendants() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(
        r#"<?php
class Details {
    public function __construct(public string $a, public int $n, public string $v) {
        if (func_num_args() > 3) { echo "surplus\n"; }
    }
    public static function parse(string $raw): static {
        $parts = explode(':', $raw, 4);
        unset($parts[0]);
        return new static(...$parts);
    }
}
class Child extends Details {}
class Unrelated {
    public function __construct(public string $a, public int $n, public string $v) {
        if (func_num_args() > 3) { echo "unrelated\n"; }
    }
}
echo get_class(Details::parse('x:p:7:q')), Details::parse('x:p:7:q')->n, "\n";
echo get_class(Child::parse('x:r:8:s')), Child::parse('x:r:8:s')->v, "\n";
echo (new Unrelated('u', 1, 'w'))->v, "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Details7\nChilds\nw\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let parse = asm
        .split("name=Details::parse ")
        .nth(1)
        .and_then(|rest| rest.split("@endfn name=Details::parse").next())
        .expect("the method's body is in the assembly");
    assert!(!parse.contains("Unrelated"), "an unrelated class got a branch");
}

/// `$this->{$name}` in a closure whose scope is only known at run time reads through one shared
/// helper, as its writes and removals do. Inline, each read held a ladder over every property of
/// the program: 830 000 lines per site in the Symfony Demo. The helper must keep the closure's
/// scope: private and protected reads answer from the bound class, and nothing else does.
#[test]
fn test_bound_closure_dynamic_read_shares_the_scope_helper() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(
        r#"<?php
class Vault {
    private array $secrets = ['k' => 'v'];
    protected int $level = 3;
    public string $label = 'open';
}
class Other {
    private string $secrets = 'not yours';
}
$read = function (string $name) { return $this->{$name}; };
$probe = function (string $name) { return isset($this->{$name}); };
$vault = Closure::bind($read, new Vault, Vault::class);
echo json_encode($vault('secrets')), $vault('level'), $vault('label'), "\n";
$other = Closure::bind($read, new Other, Other::class);
echo $other('secrets'), "\n";
$check = Closure::bind($probe, new Vault, Vault::class);
var_dump($check('level'), $check('missing'));
$outside = Closure::bind($read, new Vault, Other::class);
try {
    echo $outside('secrets'), "\n";
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
$unset = Closure::bind($read, new Vault, Vault::class);
echo @$unset('nothing') === null ? "null\n" : "value\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "{\"k\":\"v\"}3open\nnot yours\nbool(true)\nbool(false)\nCannot access private property Vault::$secrets\nnull\n",
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(asm.contains("_eir_shared_dyn_prop_get"), "the read kept its inline ladder");
}

/// Classes inheriting one wider implementation take one arm of the class-id ladder. Symfony's
/// dumped container has ~450 classes extending it, so `$container->get($id)` was 456 identical
/// arms, 57 000 lines per site. Overrides of their own keep their arm, so each class still reaches
/// its implementation with its own defaults.
#[test]
fn test_classes_sharing_a_wider_override_share_one_ladder_arm() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(
        r#"<?php
interface Getter { public function get(string $id); }
class Base implements Getter {
    public function get(string $id, int $flags = 1) { return "$id/$flags"; }
}
class S0 extends Base {}
class S1 extends Base {}
class S2 extends Base {}
class S3 extends Base {}
class S4 extends Base {}
class S5 extends Base {}
class S6 extends Base {}
class S7 extends Base {}
class S8 extends Base {}
class S9 extends Base {}
class S10 extends Base {}
class S11 extends Base {}
class S12 extends Base {}
class S13 extends Base {}
class S14 extends Base {}
class S15 extends Base {}
class S16 extends Base {}
class S17 extends Base {}
class S18 extends Base {}
class S19 extends Base {}
class S20 extends Base {}
class S21 extends Base {}
class S22 extends Base {}
class S23 extends Base {}
class S24 extends Base {}
class S25 extends Base {}
class S26 extends Base {}
class S27 extends Base {}
class S28 extends Base {}
class S29 extends Base {}
class S30 extends Base {}
class S31 extends Base {}
class S32 extends Base {}
class S33 extends Base {}
class S34 extends Base {}
class S35 extends Base {}
class S36 extends Base {}
class S37 extends Base {}
class S38 extends Base {}
class S39 extends Base {}
class Wider extends S3 {
    public function get(string $id, int $flags = 2, int $more = 3) { return "$id/$flags/$more"; }
}
class Other extends S7 {
    public function get(string $id, int $flags = 5) { return "other:$id/$flags"; }
}
function useIt(Getter $g) { return $g->get('a'); }
echo useIt(new Base), ' ', useIt(new S0), ' ', useIt(new S39), ' ', useIt(new Wider), ' ', useIt(new Other), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "a/1 a/1 a/1 a/2/3 other:a/5\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    let body = asm
        .split("name=useIt ")
        .nth(1)
        .and_then(|rest| rest.split("@endfn name=useIt").next())
        .expect("the function's body is in the assembly");
    let calls = body.matches("__rt_call_operand_push_owner").count();
    assert!(calls <= 8, "{calls} operand pushes: one ladder arm per class");
}

/// A property program with `count` subclasses of one parent, read through untyped receivers.
fn inherited_property_program(count: usize) -> String {
    let classes = (0..count)
        .map(|index| format!("class C{index} extends Base {{}}"))
        .collect::<Vec<_>>()
        .join("\n");
    // Every subclass is instantiated: an unused one is pruned and proves nothing.
    let objects = (0..count)
        .map(|index| format!("new C{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"<?php
class Base {{ public int $p = 1; public int $u; }}
{classes}
class Redeclared extends C3 {{ public int $p = 9; }}
class Other {{ public int $before = 0; public int $p = 4; public $u = 'set'; }}
function readIt($x) {{ return $x->p; }}
function readUnset($x) {{
    try {{ return $x->u; }} catch (Error $e) {{ return $e->getMessage(); }}
}}
echo readIt(new Base), readIt(new C0), readIt(new C7), readIt(new Redeclared), readIt(new Other), "\n";
echo readUnset(new C5), ' ', readUnset(new Other), "\n";
$sum = 0;
foreach ([{objects}] as $object) {{ $sum += readIt($object); }}
echo $sum === {count} ? "every subclass read\n" : "wrong sum\n";
"#
    )
}

/// Subclasses inheriting a slot read it with one arm and one range test. Symfony's dumped
/// container has ~450 classes deriving from it: every `$container->privates` on an untyped
/// receiver was 470 arms and 12 000 lines. The error still names the declaring class, and a
/// redeclared or unrelated property keeps its own value.
#[test]
fn test_inherited_slot_is_read_by_one_arm_and_one_range() {
    let class_tests = |asm: &str| asm.matches("cmp x9, x10").count() + asm.matches("cmp r11, r10").count();
    let (small_out, small_asm) = compile_and_run_with_heap_debug_and_asm(&inherited_property_program(10));
    let (large_out, large_asm) = compile_and_run_with_heap_debug_and_asm(&inherited_property_program(50));
    let expected = "11194\nTyped property Base::$u must not be accessed before initialization set\nevery subclass read\n";
    assert_eq!(small_out.stdout, expected, "{}", small_out.stderr);
    assert_eq!(large_out.stdout, expected, "{}", large_out.stderr);
    assert!(large_out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", large_out.stderr);
    let (small, large) = (class_tests(&small_asm), class_tests(&large_asm));
    assert!(large <= small + 8, "{small} class tests for 10 subclasses, {large} for 50");
}
