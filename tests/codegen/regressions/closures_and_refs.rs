//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions closures and refs, including closure default param, closure default param overridden, and for compound subtract.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests closure with a typed int default parameter: `$y = 10` is used when
/// the closure is called with only one argument.
#[test]
fn test_closure_default_param() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5);
"#,
    );
    assert_eq!(out, "15");
}

/// Tests closure with a typed int default parameter overridden by caller:
/// `$y = 10` is ignored because a second argument `20` is passed.
#[test]
fn test_closure_default_param_overridden() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5, 20);
"#,
    );
    assert_eq!(out, "25");
}

/// Tests for-loop with compound subtraction (`$i -= 3`): verifies the loop
/// iterates correctly from 10 down to 1 with step -3.
#[test]
fn test_for_compound_subtract() {
    let out = compile_and_run(
        r#"<?php
for ($i = 10; $i > 0; $i -= 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "10 7 4 1 ");
}

/// Tests for-loop with compound addition (`$i += 3`): verifies the loop
/// iterates correctly from 0 up to 9 with step +3.
#[test]
fn test_for_compound_add() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 10; $i += 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "0 3 6 9 ");
}

/// Tests for-loop with compound multiplication (`$i *= 2`): verifies the loop
/// iterates correctly doubling from 1 to 64.
#[test]
fn test_for_compound_multiply() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 100; $i *= 2) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 32 64 ");
}

/// Tests for-loop with compound left shift (`$i <<= 1`): verifies the loop
/// iterates correctly doubling from 1 to 16.
#[test]
fn test_for_compound_shift_left() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 20; $i <<= 1) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 ");
}

/// Tests closure with `use ($factor)` capturing an integer by value from the
/// enclosing scope. Verifies the captured value is used inside the closure.
#[test]
fn test_closure_use_int() {
    let out = compile_and_run(
        r#"<?php
$factor = 3;
$mul = function($x) use ($factor) { return $x * $factor; };
echo $mul(5);
"#,
    );
    assert_eq!(out, "15");
}

/// Tests closure with `use ($greeting)` capturing a string by value from the
/// enclosing scope. Verifies string concatenation inside the closure.
#[test]
fn test_closure_use_string() {
    let out = compile_and_run(
        r#"<?php
$greeting = "Hello";
$greet = function($name) use ($greeting) { return $greeting . " " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Tests closure with `use ($a, $b)` capturing two integers by value. Verifies
/// both captured variables are accessible inside the closure body.
#[test]
fn test_closure_use_multiple() {
    let out = compile_and_run(
        r#"<?php
$a = 10;
$b = 20;
$sum = function() use ($a, $b) { return $a + $b; };
echo $sum();
"#,
    );
    assert_eq!(out, "30");
}

/// Tests closure with no parameters but with `use ($name)` capturing a string
/// by value. Verifies the closure can be called with no arguments and that
/// captured variables are accessible inside the body.
#[test]
fn test_closure_use_no_params() {
    let out = compile_and_run(
        r#"<?php
$name = "World";
$greet = function() use ($name) {
    echo "Hello " . $name;
};
$greet();
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Tests recursive self-call through a by-reference capture (`use (&$g)`):
/// a factorial closure references itself via the enclosing `$g` variable.
/// Verifies `$g(5)` computes `5! = 120`.
#[test]
fn test_closure_use_by_ref_recursive_self_call() {
    let out = compile_and_run(
        r#"<?php
$g = null;
$g = function ($n) use (&$g) {
    return $n <= 1 ? 1 : $n * $g($n - 1);
};
echo $g(5);
"#,
    );
    assert_eq!(out, "120");
}

/// Tests by-reference capture (`use (&$x)`): the closure mutates the captured
/// outer variable `$x`. Verifies the mutation is visible after the call.
#[test]
fn test_closure_use_by_ref_mutates_outer_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$f = function() use (&$x) { $x = 2; };
$f();
echo $x;
"#,
    );
    assert_eq!(out, "2");
}

/// Regression for #306: by-reference captures must observe later outer mutations.
#[test]
fn test_closure_use_by_ref_observes_later_outer_mutation() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$f = function () use (&$x) {
    echo $x;
};
$x = 2;
$f();
"#,
    );
    assert_eq!(out, "2");
}

/// Regression for #304: by-reference captures stored in arrays must survive the
/// defining function scope and observe loop updates made after the closure is created.
#[test]
fn test_closure_use_by_ref_array_escape_survives_function_scope() {
    let out = compile_and_run(
        r#"<?php
function make() {
    $fns = [];
    for ($i = 0; $i < 1; $i++) {
        $fns[] = function() use (&$i) {
            echo $i;
        };
    }
    return $fns;
}

$fns = make();
$fns[0]();
"#,
    );
    assert_eq!(out, "1");
}

/// Regression for #318: a by-reference captured array accepts post-increment on an element.
#[test]
fn test_closure_use_by_ref_array_element_post_increment() {
    let out = compile_and_run(
        r#"<?php
$a = ["n" => 1];
$f = function () use (&$a) { $a["n"]++; };
$f();
echo $a["n"];
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies a hash write through a *reference-bound* local survives a table reallocation.
///
/// `lower_inst::hashes::source_load_local_slot` recognized only `Op::LoadLocal`, while its
/// indexed-array twin (`lower_inst::arrays::source_load_local_slot`) also recognizes
/// `Op::LoadRefCell`. So `$r = &$a; $r[$k] = $v;` found no destination slot and silently
/// discarded the table pointer `__rt_hash_set` hands back — which is a *new* pointer whenever
/// the table rehashes past its load factor or is COW-split. The stale pointer then kept being
/// read: building a 41-key hash through a ref reported a garbage count (a wild read), and the
/// same shape on an object property lost 37 of the 41 entries.
///
/// Three or four keys are not enough to trigger it — the hash must actually grow — so this
/// test deliberately crosses the rehash threshold.
#[test]
fn test_hash_write_through_ref_bound_local_survives_rehash() {
    let out = compile_and_run_capture(
        r#"<?php
class Holder {
    public array $v = [];
}
$a = ["seed" => 0];
$r = &$a;
for ($i = 0; $i < 40; $i++) {
    $r["k" . $i] = $i;
}
echo count($a), "\n";

$o = new Holder();
$o->v["seed"] = 0;
$p = &$o->v;
for ($i = 0; $i < 40; $i++) {
    $p["k" . $i] = $i;
}
echo count($o->v), "\n";
echo $o->v["k39"], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "41\n41\n39\n");
}


/// Unpacking a `callable` must expand to the two elements PHP's `[$object, 'method']` array has.
///
/// A `callable` slot holds a DESCRIPTOR, so `...$c` had nothing to unpack: the checker answered
/// "Spread operator requires an array" and, where the type was gradual, lowering dropped the
/// spread to a single operand and failed EIR validation with `OperandCountMismatch`.
/// `__rt_mixed_spread_array` materializes the descriptor's bound receiver and bare method name
/// into a real two-element array, so the length guard, the element reads and the argument planner
/// all see an ordinary array.
///
/// Both call shapes Symfony's `ControllerEvent` uses are pinned: the builtin in a `match (true)`
/// arm, and the constructor.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_spreading_a_callable_expands_to_its_receiver_and_method_name() {
    let out = compile_and_run(
        r#"<?php
class Target { public function preview(): string { return 'ok'; } }
function makeCallable(): callable { return [new Target(), 'preview']; }
final class Holder {
    private string|array|object $c;
    public function set(callable $c): void {
        $r = match (true) {
            \is_array($c) && method_exists(...$c) => 'matched',
            default => 'default',
        };
        echo 'arm=', $r, "\n";
        $this->c = $c;
    }
    public function reflect(): void {
        $r = new \ReflectionMethod(...$this->c);
        echo 'reflect=', $r->getName(), "\n";
    }
    public function viaProperty(): void {
        echo 'prop=', var_export(method_exists(...$this->c), true);
    }
}
$h = new Holder();
$h->set(makeCallable());
$h->reflect();
$h->viaProperty();
"#,
    );
    assert_eq!(out, "arm=matched\nreflect=preview\nprop=true");
}


/// A function declared by a `require` that ran inside a closure is visible afterwards.
///
/// elephc gives every compiled function that evals or includes its OWN eval context (a
/// `LocalKind::EvalContext` frame slot), so a `require` inside a closure declared into a context
/// the caller had never seen: `function_exists()` said no and an interpreted call reported
/// `call to undefined function`. Only a TOP-LEVEL require worked, because that context is
/// `main`'s and lives for the process.
///
/// Composer's `files` autoload is exactly this shape —
/// `Closure::bind(static function ($file) { require $file; }, null, null)` in
/// `vendor/composer/autoload_real.php` — so `trigger_deprecation()` came back undefined and killed
/// the Symfony `--web` worker while it rendered an error page.
///
/// Class-likes already crossed that boundary through `sync_global_eval_classes`; the fix is the
/// function half, so the fixture asserts a class and a function declared by the same include.
///
/// Every scope PHP treats alike is covered: top level, a plain closure, a static closure, the
/// bound static closure Composer writes, and a named function.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_require_inside_a_closure_declares_globally() {
    let out = compile_and_run(
        r#"<?php
function makeShim(string $name, string $fn): void {
    file_put_contents($name . '.php', '<?php
if (!function_exists("' . $fn . '")) {
    function ' . $fn . '(): string { return "ok"; }
}
');
}
foreach (['s_top' => 'fn_a', 's_plain' => 'fn_b', 's_static' => 'fn_c', 's_bound' => 'fn_d', 's_named' => 'fn_e'] as $file => $fn) {
    makeShim($file, $fn);
}
file_put_contents('s_class.php', '<?php
class IncludedLate { public function hi(): string { return "hi"; } }
function fn_with_class(): string { return "withclass"; }
');

$p = './s_top.php';
require $p;
$plain = function ($file) { require $file; };
$plain('./s_plain.php');
$static = static function ($file) { require $file; };
$static('./s_static.php');
$bound = \Closure::bind(static function ($file) { require $file; }, null, null);
$bound('./s_bound.php');
function req(string $file): void { require $file; }
req('./s_named.php');
$bound('./s_class.php');

echo function_exists('fn_a') ? 'y' : 'n';
echo function_exists('fn_b') ? 'y' : 'n';
echo function_exists('fn_c') ? 'y' : 'n';
echo function_exists('fn_d') ? 'y' : 'n';
echo function_exists('fn_e') ? 'y' : 'n';
echo '|', class_exists('IncludedLate') ? 'y' : 'n';
echo '|', fn_with_class();
foreach (['s_top', 's_plain', 's_static', 's_bound', 's_named', 's_class'] as $f) { unlink($f . '.php'); }
"#,
    );
    assert_eq!(out, "yyyyy|y|withclass");
}


/// A closure in a STATIC property carries its by-reference parameters to the call site.
///
/// `record_static_property_callable_sig` records the signature of a closure assigned to a static
/// property, keyed `Class::$prop`, for exactly this purpose — and nothing read it back:
/// `resolve_expr_callable_sig` had no `StaticPropertyAccess` arm, so the map was written and never
/// consulted. `(self::$merge)($items, $expiredIds)` therefore never learned that its second
/// parameter binds by reference, and the later `if ($expiredIds)` read an undefined variable.
///
/// Symfony's `AbstractAdapter::commit()` is the site, and it runs on every request:
///
///     $byLifetime = (self::$mergeByLifetime)($this->deferred, $this->namespace, $expiredIds, …);
///     if ($expiredIds) { … $this->doDelete($expiredIds); }
///
/// The `??=` and the `Closure::bind` wrapper are both in the fixture because both are in the
/// original and both are unwrapped on the recording side — `??=` because the parser lowers it to a
/// null-coalesce whose left branch is the property itself, and `Closure::bind` because rebinding
/// `$this` changes neither the parameter list nor which parameters are by reference.
///
/// The assertion reads the WRITTEN value back. Defining `$expiredIds` without wiring the reference
/// would also compile, and would print `kept=a,b` with no expired list — the silent miscompile
/// this is not.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_a_static_property_closure_binds_its_by_ref_argument() {
    let out = compile_and_run(
        r#"<?php
class Item { public function __construct(public string $k) {} }

class Bag {
    private static ?\Closure $merge = null;
    private array $deferred = [];

    public function __construct()
    {
        self::$merge ??= \Closure::bind(
            static function ($deferred, &$expiredIds) {
                $kept = [];
                $expiredIds = [];
                foreach ($deferred as $item) {
                    if ($item->k === 'old') {
                        $expiredIds[] = $item->k;
                        continue;
                    }
                    $kept[] = $item->k;
                }
                return $kept;
            },
            null,
            Item::class
        );
    }

    public function add(Item $i): void { $this->deferred[] = $i; }

    public function commit(): string
    {
        $kept = (self::$merge)($this->deferred, $expiredIds);
        $out = 'kept=' . implode(',', $kept);
        if ($expiredIds) {
            $out .= ' expired=' . implode(',', $expiredIds);
        }
        return $out;
    }
}

$b = new Bag();
$b->add(new Item('a'));
$b->add(new Item('old'));
$b->add(new Item('b'));
echo $b->commit(), "\n";
"#,
    );
    assert_eq!(out, "kept=a,b expired=old\n");
}


/// A local a closure captures BY REFERENCE and writes may still be retyped by its own body.
///
/// `local_binding_is_widenable` refuses every name in `ref_bound_locals`, because re-representing
/// a slot another name reaches would break the alias. That is right for `$a =& $b` and for a
/// by-reference `foreach`, and wrong for the one shape where lowering has already done the
/// widening itself: `expr::closures` reacts to a `use (&$x)` capture whose body WRITES `$x` with
/// `ctx.set_local_type(capture, PhpType::Mixed)`, because a PHP reference has no type and the
/// shared cell must hold whatever the closure stores through it. The cell is boxed before the
/// checker ever refuses, so the refusal cost a diagnostic on code with no representation problem.
///
/// Symfony's `RedisTrait` is the case, and it is the last kind of shape a cache adapter writes:
/// an error handler captures `$error` by reference, then `preg_match`'s by-reference third
/// argument writes the matches array into that same local, the ternary reads `$error[1]`, and the
/// assignment stores a string — `cannot reassign $error from array<mixed> to string`.
///
/// The fixture asserts all three values the one slot carries in sequence, so a widening that
/// dropped the closure's write, or the matches array, would fail rather than merely compile. The
/// two controls below it keep the stricter bindings strict: a `=&` alias and a by-reference
/// `foreach` both REMOVE the exemption, and the last two `echo`s read through the alias to prove
/// it still holds.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_a_written_by_ref_capture_may_be_retyped() {
    let out = compile_and_run(
        r#"<?php
function connect(): string
{
    $error = null;
    $report = static function (string $msg) use (&$error) { $error = $msg; };
    $report('Redis::connect(): boom');
    echo $error, "\n";
    $error = preg_match('/^Redis::p?connect\(\): (.*)/', $error ?? '', $error)
        ? sprintf(' (%s)', $error[1])
        : '';
    return $error;
}
echo connect(), "\n";

$a = 1;
$alias =& $a;
$a = 2;
echo $alias, "\n";

$list = [1, 2, 3];
foreach ($list as &$v) { $v *= 10; }
unset($v);
echo implode(',', $list), "\n";
"#,
    );
    assert_eq!(
        out,
        "Redis::connect(): boom\n (boom)\n2\n10,20,30\n",
    );
}


/// `Closure::bind($c, $newThis)` binds the receiver it was GIVEN, whatever the build knows of it.
///
/// Three defects, one shape, and the gradual one was a wrong answer rather than a refusal:
///
/// - `__rt_closure_bind` takes the new receiver as a RAW object pointer and boxes it into the
///   capture slot itself. A gradual receiver arrives already boxed, so it boxed the box, and
///   `Closure::bind(fn () => $this->n, $gradual)` printed `Warning: Undefined property: ::$n` and
///   `0` where PHP prints `7` — the empty class name is the double box read as an object header.
/// - The CHECKER typed `$this` inside a bound closure as the ENCLOSING class, which is the one
///   thing a bound `$this` is definitely not, unless the body was exactly `return $this->prop;`.
/// - LOWERING had the same narrow restriction, from the other side.
///
/// The last two had to move together: with only one side widened the checker types `$this->p`
/// against one class while the backend emits it against another, which is a silent miscompile
/// rather than a diagnostic. `bound_this_capture` and `closure_bind_property_receiver_type` now
/// answer the same two ways from the same two arguments.
///
/// Symfony's `RedisTrait` is the case that found it:
/// `\Closure::bind(function () { $this->options['exceptions'] = false; }, $options, $options)()`,
/// where `$options` is `clone $redis->getOptions()` over Redis classes the build does not have.
///
/// The fixture covers all three receiver kinds PHP's `?object` allows — a named class, a gradual
/// object, and a gradual null, which UNBINDS rather than raising — and a body that writes rather
/// than returns, because the narrow shape that used to be the only supported one is a read.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_closure_bind_receives_the_object_it_was_given() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    public int $n = 7;
    private array $options = ['exceptions' => true];
    public function shown(): string { return var_export($this->options['exceptions'], true); }
}

function makeHolder(): mixed { return new Holder(); }
function makeNothing(): mixed { return null; }

$known = new Holder();
echo \Closure::bind(function () { return $this->n; }, $known, Holder::class)(), "\n";

$gradual = makeHolder();
echo \Closure::bind(function () { return $this->n; }, $gradual, Holder::class)(), "\n";

\Closure::bind(function () { $this->options['exceptions'] = false; }, $gradual, $gradual)();
echo $gradual->shown(), "\n";

$nothing = makeNothing();
$unbound = \Closure::bind(function () { return 3; }, $nothing, Holder::class);
var_dump($unbound instanceof Closure);
echo $unbound(), "\n";
"#,
    );
    assert_eq!(out, "7\n7\nfalse\nbool(true)\n3\n");
}


/// `array_walk()` writes back through a by-reference callback parameter, captures included.
///
/// `__rt_array_walk` hands the callback each element BY VALUE and discards the return, so every
/// write through `&$v` was dropped — which is most of why the builtin exists. `__rt_array_walk_ref`
/// hands it `&slot[i]` instead, and nothing else was needed on the closure side: elephc already
/// compiles a by-reference parameter as a cell pointer, and the body's `load_ref_cell` /
/// `store_ref_cell` release the old value and retain the new one, boxed `mixed` elements included.
///
/// Two things had to be true at once and the first attempt got the second wrong:
///
/// - the ELEMENT write must land — a widened element gate alone made this compile and print
///   `1,2,3` where PHP prints `2,4,6`;
/// - a `use (&$n)` CAPTURE must be passed as the slot's ADDRESS, the same thing
///   `emit_runtime_closure_descriptor_with_captures` stores into the descriptor. Passing its value
///   compiled and printed an empty `$n`.
///
/// So the fixture asserts both in one line, and the element type is `array<mixed>` — what a
/// declared bare `array` parameter is — because that is the shape Symfony's
/// `Yaml\Command\LintCommand::displayJson` walks.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_array_walk_writes_back_through_a_by_ref_callback() {
    let out = compile_and_run(
        r#"<?php
function scaled(array $rows): string
{
    $n = 0;
    array_walk($rows, static function (&$v) use (&$n) { $n += $v; $v = $v * 10; });
    return $n . '|' . implode(',', $rows);
}
function shown(array $rows): string
{
    $seen = '';
    array_walk($rows, static function ($v) use (&$seen) { $seen .= $v . ';'; });
    return $seen;
}
echo scaled([1, 2, 3]), "\n";
echo shown(['a', 'b']), "\n";
"#,
    );
    assert_eq!(out, "6|10,20,30\na;b;\n");
}


/// `$x = &$arr[]` binds a FRESH reference every time it runs, not one per source position.
///
/// The parser desugars it to a temp named after `span.line, span.col, source_start`, so a loop
/// reuses ONE local — and `$temp = null` on an already-reference-bound name writes THROUGH the
/// existing cell instead of rebinding it. Every element a loop appended then aliased the same
/// cell, so the array ended up holding the LAST iteration's value in every slot:
///
///     foreach ([1, 2, 3] as $i) { $c = &$opt[]; $c = $i; }   // 3,3,3 instead of 1,2,3
///
/// The desugaring now ends with `unset($temp)`. It has to come LAST: lowering walks the block
/// ONCE and `unset_local` only emits `Op::UnsetLocal` for a name that is ref-bound AT LOWERING
/// TIME, so a leading `unset` lowered to nothing and changed nothing at all.
///
/// Symfony's `EventDispatcher::optimizeListeners` writes `$closure = &$this->optimized[$e][]` once
/// per listener, so a shared cell made every event dispatch its LAST listener over and over.
///
/// The fixture reads the elements back through `foreach`, which follows a reference element.
/// `json_encode`/`implode`/`in_array` do NOT follow one — a separate, still-open gap that this
/// test deliberately does not assert.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_an_append_by_reference_binds_a_fresh_cell_each_time() {
    let out = compile_and_run(
        r#"<?php
$opt = [];
foreach ([1, 2, 3] as $i) {
    $c = &$opt[];
    $c = static function () use ($i) { return $i; };
}
unset($c);
$seen = [];
foreach ($opt as $f) { $seen[] = $f(); }
echo implode(',', $seen), "\n";

$vals = [];
foreach (['x', 'y'] as $s) {
    $slot = &$vals[];
    $slot = strtoupper($s);
}
unset($slot);
$seen = [];
foreach ($vals as $v) { $seen[] = $v; }
echo implode(',', $seen), "\n";
echo count($vals), "\n";
"#,
    );
    assert_eq!(out, "1,2,3\nX,Y\n2\n");
}


/// A by-reference `foreach` value passed to a CLOSURE -- by value or by reference, over int,
/// mixed and associative arrays, with and without the key bound. The marker for that argument
/// was retained with `__rt_incref` and released with `__rt_decref_any` on an address INSIDE the
/// array, corrupting a neighbour (for element 0, the capacity word): segfaults, "requested array
/// size exceeds the maximum", and empty values. Identical on the compiler this was found with.
#[test]
fn test_foreach_reference_passed_to_closures() {
    let out = compile_and_run(
        r#"<?php
$p = static function ($v) { echo 'p(', $v, ')'; };
$two = static function ($v, $k) { echo 't(', $v, ',', $k, ')'; };
$double = static function (&$x) { $x = $x * 2; };
$shout = static function (&$x) { $x = $x . '!'; };
$a = [1, 2];
foreach ($a as $k => &$v) { $p($v); $two($v, $k); }
unset($v);
echo '|';
$b = [1, 2, 3];
foreach ($b as &$v) { $double($v); }
unset($v);
echo implode(',', $b), '|';
$c = [1, 'two'];
foreach ($c as &$v) { $shout($v); }
unset($v);
echo implode(',', $c), '|';
$h = ['k' => 1, 'j' => 'two'];
foreach ($h as &$v) { $p($v); }
unset($v);
"#,
    );
    assert_eq!(out, "p(1)t(1,0)p(2)t(2,1)|2,4,6|1!,two!|p(1)p(two)");
}

/// A by-reference `foreach` over a by-reference PARAMETER that holds a SHARED array. The loop
/// separates the array before writing, and the separated copy was never put back into the
/// parameter's cell -- so the writes were lost and the caller read `array(0)`.
#[test]
fn test_foreach_reference_over_a_shared_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
function bump(array &$arr): void { foreach ($arr as &$x) { $x = 7; } unset($x); }
function run(array $extra): array { $copy = $extra; bump($copy); return $copy; }
echo json_encode(run([1, 2])), '|';
function run2(array $extra): array { bump($extra); return $extra; }
echo json_encode(run2([1, 2]));
"#,
    );
    assert_eq!(out, "[7,7]|[7,7]");
}

/// A closure capturing ITSELF by reference inside a function body, read back as a VALUE. The
/// assignment stored the raw descriptor into the boxed cell the capture reads: a direct call
/// worked, but `call_user_func($c, …)` fatalled and `gettype($c)` answered `string`.
#[test]
fn test_self_capturing_closure_in_a_function_is_a_value() {
    let out = compile_and_run(
        r#"<?php
function run(): string {
    $c = static function ($n) use (&$c) { return $n ? 'x' . call_user_func($c, $n - 1) : 'done'; };
    return $c(2) . ':' . gettype($c);
}
echo run();
"#,
    );
    assert_eq!(out, "xxdone:object");
}

/// An untyped by-reference parameter re-assigned inside a type guard that narrowed it. Its cell
/// is `mixed` by declaration, but the merge ran against the narrowed type and refused with
/// `cannot reassign $v from (object) to array<string, mixed>`.
#[test]
fn test_untyped_by_reference_parameter_reassigned_after_narrowing() {
    let out = compile_and_run(
        r#"<?php
class H { public $a = 1; }
$touch = static function (&$v) {
    if (is_object($v)) { $v = get_object_vars($v); }
};
$x = new H();
$touch($x);
echo json_encode($x);
"#,
    );
    assert_eq!(out, r#"{"a":1}"#);
}
