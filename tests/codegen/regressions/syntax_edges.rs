//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions syntax edges, including spread into named params, spread into named params three, and braceless if.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies positional spread args (`...$array`) correctly fill a function's
/// positional parameters. Fixture: 2-element array unpacked into 2-param function.
#[test]
fn test_spread_into_named_params() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b) { return $a + $b; }
$args = [3, 4];
echo add(...$args);
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies positional spread args with 3 elements unpacked into a 3-param function.
#[test]
fn test_spread_into_named_params_three() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) { return $a + $b + $c; }
$args = [10, 20, 30];
echo sum3(...$args);
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies single-statement `if` without braces compiles and executes correctly.
#[test]
fn test_braceless_if() {
    let out = compile_and_run(
        r#"<?php
if (true) echo "yes";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies `echo` with multiple comma-separated arguments outputs each in sequence.
#[test]
fn test_multi_argument_echo() {
    let out = compile_and_run(
        r#"<?php
echo "A", "B", 3, "\n";
"#,
    );
    assert_eq!(out, "AB3\n");
}

/// Verifies braceless `if`/`else` with single-statement branches executes the correct branch.
#[test]
fn test_braceless_if_else() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x > 10) echo "big";
else echo "small";
"#,
    );
    assert_eq!(out, "small");
}

/// Verifies single-statement `for` loop without braces iterates 0..2.
#[test]
fn test_braceless_for() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) echo $i;
"#,
    );
    assert_eq!(out, "012");
}

/// Verifies single-statement `while` loop without braces iterates post-increment 0..2.
#[test]
fn test_braceless_while() {
    let out = compile_and_run(
        r#"<?php
$i = 0;
while ($i < 3) echo $i++;
"#,
    );
    assert_eq!(out, "012");
}

/// Verifies single-statement `foreach` without braces iterates array values 1..3.
#[test]
fn test_braceless_foreach() {
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
foreach ($arr as $v) echo $v;
"#,
    );
    assert_eq!(out, "123");
}

/// Verifies braceless `if` / `else if` / `else` chain with single-statement branches
/// selects the correct branch based on condition $x > 10 > 3.
#[test]
fn test_braceless_else_if() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x > 10) echo "big";
else if ($x > 3) echo "medium";
else echo "small";
"#,
    );
    assert_eq!(out, "medium");
}

// --- Bug regression tests ---

/// Regression: a class whose only instantiation appears inside a multi-value `echo` must still
/// be collected for method emission. PHP's multi-argument `echo a, b;` lowers to a `Synthetic`
/// statement sequence; `collect_required_class_names` previously skipped `Synthetic` bodies, so
/// the class (here `SplObjectStorage`, an injected builtin emitted on demand) had its
/// `__construct` referenced by the `new` call site but never emitted → undefined-symbol link
/// failure. A user-declared class is unaffected because its `ClassDecl` always seeds the set;
/// only on-demand/injected classes exposed the gap.
#[test]
fn test_class_used_only_in_multi_value_echo_is_emitted() {
    let out = compile_and_run(r#"<?php echo "x", (new SplObjectStorage())->count();"#);
    assert_eq!(out, "x0");
}


/// A `catch` sees the variable types as of a point where the exception could be RAISED.
///
/// Checking the catch clauses against the environment the try body LEFT says every assignment in
/// it completed — the one thing that cannot be true of the statement that threw. Here `$r` is
/// still the array when `new ReflectionMethod(...)` raises, and the catch indexes it; the checker
/// had retyped `$r` to `ReflectionMethod` and refused with "Cannot index non-array".
///
/// Symfony's `ClassStub::__construct` is written exactly this way, which is what kept
/// `symfony/var-dumper` out of the compiled set.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_catch_sees_the_types_from_the_point_the_exception_could_be_raised() {
    let out = compile_and_run(
        r#"<?php
class Sample { public function known(): string { return 'k'; } }
function describe(string $identifier): string {
    if (str_contains($identifier, '::')) {
        $i = strpos($identifier, '::');
        $r = [substr($identifier, 0, $i), substr($identifier, 2 + $i)];
    } else {
        $r = new ReflectionClass($identifier);
    }
    if (is_array($r)) {
        try {
            $r = new ReflectionMethod($r[0], $r[1]);
        } catch (ReflectionException) {
            $r = new ReflectionClass($r[0]);
        }
    }
    return get_class($r).':'.$r->getName();
}
echo describe('Sample::known'), "\n";
echo describe('Sample::missing'), "\n";
echo describe('Sample');
"#,
    );
    assert_eq!(
        out,
        "ReflectionMethod:known\nReflectionClass:Sample\nReflectionClass:Sample"
    );
}

/// A name the TRY introduced still reaches the catch, and a catch's own bindings outlive the
/// statement.
///
/// Both are what PHP does: a by-reference argument is bound at the CALL, before the callee can
/// raise, and `catch (\Throwable $e)` leaves `$e` defined afterwards. Scoping the catch to the
/// throw-point environment broke each of them in turn — `Undefined variable: $controllerMetadata`
/// in `HttpKernel::handle`, then `Undefined variable: $e` in `ContainerControllerResolver`.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_catch_keeps_try_introduced_names_and_leaks_its_own_bindings() {
    let out = compile_and_run(
        r#"<?php
class Boom extends RuntimeException {}
function fill(bool $fail, &$out): string {
    $out = 'filled';
    if ($fail) {
        throw new Boom('nope');
    }
    return 'ok';
}
function run(bool $fail): string {
    try {
        $r = fill($fail, $meta);
    } catch (Boom $e) {
        $r = 'caught';
        $tag = $e->getMessage();
    }
    return $r.'/'.($meta ?? 'none');
}
echo run(false), "|", run(true);
"#,
    );
    assert_eq!(out, "ok/filled|caught/filled");
}


/// An assignment in the right operand of `&&` must keep its type fact past the merge.
///
/// `lower_logical_binary` restored the operand's whole local-type snapshot to undo the instanceof
/// narrowing it applies there, which also undid every real assignment. A name that had been an
/// earlier loop's element kept that element's type, and the `foreach` below was lowered as an
/// iteration of that class: `unsupported EIR backend feature: iterator method Item::rewind`.
///
/// The merge now re-reads the frame slot's storage type for every name the operand changed -- the
/// join every store already widened it to, and the same contract an `if` gets.
///
/// `Symfony\Component\VarDumper\Caster\ReflectionCaster::castFunctionAbstract` is the shape.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_assignment_in_a_short_circuit_operand_retypes_the_local_past_the_merge() {
    let out = compile_and_run(
        r#"<?php
class Item { public function __construct(public string $name) {} }
class Src {
    public function items(): array { return [new Item('x'), new Item('y')]; }
    public function statics(): array { return ['a' => 1, 'b' => 2]; }
}
function cast(Src $c, int $filter): string {
    $out = [];
    foreach ($c->items() as $v) {
        $out[] = $v->name;
    }
    if (0 === $filter && $v = $c->statics()) {
        foreach ($v as $k => $x) {
            $out[] = $k.'='.$x;
        }
    }
    return implode(',', $out);
}
echo cast(new Src(), 0), '/', cast(new Src(), 1);
"#,
    );
    assert_eq!(out, "x,y,a=1,b=2/x,y");
}


/// A `goto` label inside an `if` branch may borrow what the branch falls through to.
///
/// elephc desugars a supported `goto` by CLONING the label's tail in its place, which needs the
/// tail to return or throw on every path. `redirect_scheme:` opens an `elseif` branch that can
/// fall off its end -- and what it falls through to is the `throw` after the whole chain, so the
/// tail does terminate once the branch's continuation is counted as part of it.
///
/// Only the shapes where the continuation is statically what runs may borrow it: `if`/`elseif`/
/// `else` and transparent blocks. Falling off a LOOP body starts the next iteration, falling off a
/// `switch` case leaves the switch, and falling off a `try` runs `finally` first -- none of those
/// reach the continuation, so a label there still needs a tail that terminates on its own.
///
/// `Symfony\Component\Routing\Matcher\Dumper\CompiledUrlMatcherTrait::match` is the shape.
///
/// Oracle: `php -n` prints the asserted lines, the `finally` side effect included.
#[test]
fn test_goto_into_an_if_branch_borrows_the_branch_continuation() {
    let out = compile_and_run(
        r#"<?php
class NotFound extends \RuntimeException {}
function match_path(string $path, array $allowSchemes, string $method): string {
    if (!\in_array($method, ['HEAD', 'GET'], true)) {
        // no-op
    } elseif ($allowSchemes) {
        redirect_scheme:
        $scheme = key($allowSchemes);
        try {
            if ('https' === $scheme) {
                return "redirect:$scheme:$path";
            }
        } finally {
            $path .= '|restored';
        }
    } elseif ('' !== $trimmed = rtrim($path, '/')) {
        $path = $trimmed === $path ? $path.'/' : $trimmed;
        if ('/known/' === $path) {
            return "match:$path";
        }
        if ($allowSchemes) {
            goto redirect_scheme;
        }
    }
    throw new NotFound("No routes found for \"$path\".");
}
$cases = [
    ['/known', [], 'GET'],
    ['/x', ['https' => 1], 'GET'],
    ['/x', ['http' => 1], 'GET'],
];
$out = [];
foreach ($cases as [$path, $schemes, $method]) {
    try {
        $out[] = match_path($path, $schemes, $method);
    } catch (NotFound $e) {
        $out[] = 'NotFound:'.$e->getMessage();
    }
}
echo implode('|', $out);
"#,
    );
    assert_eq!(
        out,
        "match:/known/|redirect:https:/x|NotFound:No routes found for \"/x|restored\"."
    );
}


/// `new (expression)(args)` — PHP 8.0's parenthesized class-name reference.
///
/// elephc's `new` parser reached a NAME or a `simple_variable` chain and nothing else, so this
/// form failed with `Expected class name after 'new'` and then desynced the rest of the file into
/// a run of `Unexpected token at statement position: Public`. The parentheses are unambiguous: no
/// other `new` form starts with one.
///
/// `Twig\ExpressionParser\Infix\BinaryOperatorExpressionParser::parse` is
/// `new ($this->nodeClass)($left, $right, $token->getLine())`, so installing Twig was what found
/// it.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_new_accepts_a_parenthesized_class_name_expression() {
    let out = compile_and_run(
        r#"<?php
class Node {
    public function __construct(public string $left, public int $line) {}
    public function show(): string { return $this->left.'@'.$this->line; }
}
class Other {
    public function __construct(public string $left, public int $line) {}
    public function show(): string { return 'other:'.$this->left; }
}
final class Builder {
    private string $nodeClass = Node::class;
    public function build(string $left, int $line): object { return new ($this->nodeClass)($left, $line); }
    public function fromArray(array $pair, string $left): object { return new ($pair[0])($left, 7); }
    public function fromCall(string $left): object { return new ($this->pick())($left, 9); }
    private function pick(): string { return Other::class; }
}
$b = new Builder();
echo $b->build('a', 3)->show(), '|', $b->fromArray([Other::class], 'x')->show(),
     '|', $b->fromCall('y')->show(), '|', (new (Node::class)('p', 11))->show();
"#,
    );
    assert_eq!(out, "a@3|other:x|other:y|p@11");
}

/// Verifies `throw $e` compiles when `$e`'s type comes from a declared return type naming a class
/// the closed world does not have.
///
/// `symfony/framework-bundle`'s `AbstractController::denyAccessUnlessGranted` does exactly this:
/// `$e = $this->createAccessDeniedException(...); ...; throw $e;`, where the method is declared
/// `: AccessDeniedException` — a class that only ships with `symfony/security-core`. The checker
/// read the absent name as a concrete type, concluded it does not implement `Throwable`, and
/// refused the whole file for a branch guarded by `class_exists()` that an app without the package
/// never runs. The existing escape hatch only covered `throw new Absent()` written inline.
/// Value-checked against `php -n`, which prints the same line.
#[test]
fn test_a_throw_of_an_absent_declared_exception_type_defers_to_runtime() {
    let out = compile_and_run(
        r#"<?php
class Guard {
    public function createDenied(string $message): \Absent\Vendor\DeniedException
    {
        if (!class_exists(\Absent\Vendor\DeniedException::class)) {
            throw new \LogicException('the absent/vendor package is not installed');
        }

        return new \Absent\Vendor\DeniedException($message);
    }

    public function deny(string $message): void
    {
        $denied = $this->createDenied($message);

        throw $denied;
    }
}

$guard = new Guard();
try {
    $guard->deny('nope');
} catch (\LogicException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "the absent/vendor package is not installed");
}

/// Verifies a `goto` out of a `catch` leaves a tail whose `if ($e) { throw $e; }` still compiles.
///
/// `goto` is desugared by cloning the label's tail at the jump, so the ORIGINAL tail keeps the
/// variable at its `= null` initialiser. `throw` then saw a null-only local and refused the file,
/// even though the guard proves the branch is dead. Twig's `CoreExtension::getAttribute()` is
/// built this way: two sandbox `catch` blocks `goto methodCheck`, and the tail there rethrows the
/// captured error. Value-checked against `php -n`.
#[test]
fn test_a_null_only_local_under_a_truthy_guard_may_be_thrown() {
    let out = compile_and_run(
        r#"<?php
function probe(bool $sandboxed, bool $bad): string
{
    $notAllowed = null;

    if ($sandboxed) {
        try {
            if ($bad) {
                throw new \RuntimeException('denied');
            }
        } catch (\RuntimeException $notAllowed) {
            goto later;
        }
    }

    return 'straight';

    later:

    if ($notAllowed) {
        throw $notAllowed;
    }

    return 'after-label';
}

echo probe(false, false), ':';
try {
    probe(true, true);
} catch (\Throwable $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "straight:denied");
}

/// A function or method with NO declared return type returns null when control falls off its
/// end, even when its return paths made the checker type it `mixed`; only a declared `: mixed`
/// refuses that.
#[test]
fn test_untyped_mixed_function_falls_off_with_null() {
    let out = compile_and_run(
        r#"<?php
function gc($x) {
    if ($x === 1) { return new stdClass; }
    if ($x === 2) { return; }
    foreach ([5, 6] as $a) {
        if ($x === $a) { return 'found'; }
    }
}
var_dump(gc(1), gc(2), gc(5), gc(9));
class C {
    protected $map = [];
    protected function find($k) {
        if (! is_null($b = $this->map[$k] ?? null)) { return $b; }
        if (empty($this->map)) { return; }
        foreach ($this->map as $v) { if ($v === $k) { return $v; } }
    }
    public function get($k) { return $this->find($k); }
    public function set($k, $v) { $this->map[$k] = $v; }
}
$c = new C();
var_dump($c->get('a'));
$c->set('a', 'x');
var_dump($c->get('a'), $c->get('zz'));
"#,
    );
    assert_eq!(
        out,
        "object(stdClass)#1 (0) {\n}\nNULL\nstring(5) \"found\"\nNULL\nNULL\nstring(1) \"x\"\nNULL\n"
    );
}

/// Verifies the whole IN-PROCESS pipeline survives deep nesting, not only the binary
/// (issue #686).
///
/// Every recursive phase carries its own stack budget through
/// `compiler_stack::with_compiler_stack`, so an embedder calling one of them is safe on its own
/// thread. This harness additionally wraps the whole run, for the AST handling BETWEEN the
/// phases: moving, cloning and dropping a tree this deep recurses through derived code no guard
/// can be placed inside.
///
/// This goes through `compile_and_run` rather than a type-check helper ON PURPOSE. Checking stops
/// at the front end, and the passes after it recurse just as deep: constant propagation, control
/// flow normalization, dead-code elimination and EIR lowering all walk every nesting level. A
/// checker-only fixture would pass while a real compile still aborted.
///
/// A regression aborts the test PROCESS rather than failing an assertion, which is exactly why
/// this is worth pinning: an abort is not something a reviewer reads as a test failure.
#[test]
fn test_deeply_nested_literal_compiles_and_runs_in_process() {
    let depth = 1024;
    let source = format!(
        "<?php\n$a = {}1{};\necho count($a);\n",
        "[".repeat(depth),
        "]".repeat(depth)
    );
    assert_eq!(compile_and_run(&source), "1");
}
