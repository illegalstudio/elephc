//! Purpose:
//! End-to-end tests for the table-driven `ReflectionClass` / `ReflectionObject` / `ReflectionEnum`:
//! their constructors read one module reflection table (`reflection_table`) through a single
//! shared body instead of a generated body per reflected class.
//!
//! Called from:
//! - `cargo test --test reflection_table_tests` through Rust's test harness.
//!
//! Key details:
//! - Every test drives the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir. Behaviour is compared against reference php.
//! - The closed-world programs build their reflectors from RUNTIME names: nothing but the table
//!   can answer them, so a pass proves the table holds the metadata.
//! - Known metadata gaps that are NOT part of this change (and so kept out of the programs): the
//!   end lines of interfaces, traits and enums, and the arguments of `#[\Attribute(...)]`.
//! - Host-target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), id));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

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

/// Compiles `main.php` of `dir` with `flags`.
fn compile(dir: &Path, flags: &[&str]) -> Output {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    // The table-driven reflectors are opt-in in a default build until the per-class path is retired.
    cmd.env("ELEPHC_TABLE_REFLECTION", "1");
    cmd.current_dir(dir);
    // `--closed-world` in a test's flags asks for the strict closed world, which elephc reads
    // from the environment: a default build is closed already and only falls back on need.
    if flags.contains(&"--closed-world") {
        cmd.env("ELEPHC_CLOSED_WORLD", "1");
    }
    cmd.args(flags.iter().filter(|flag| **flag != "--closed-world"));
    cmd.arg("--php-version").arg("8.5");
    cmd.arg(dir.join("main.php"));
    cmd.output().expect("failed to spawn elephc")
}

fn compile_ok(dir: &Path, flags: &[&str]) {
    let output = compile(dir, flags);
    assert!(
        output.status.success(),
        "elephc failed with {flags:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run(dir: &Path) -> Output {
    Command::new(dir.join("main")).current_dir(dir).output().expect("failed to run main")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `main.php` of `dir` under php and returns its stdout.
fn php_stdout(dir: &Path) -> String {
    let output = Command::new("php")
        .args(["-n", "-d", "display_errors=stderr", "-d", "log_errors=0"])
        .arg(dir.join("main.php"))
        .current_dir(dir)
        .output()
        .expect("php must be installed to pin the reference behaviour");
    assert!(
        output.status.success(),
        "reference execution failed:\n{}\n{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    text(&output.stdout)
}

/// Compiles `source` with `flags`, runs it and returns (stdout, stderr), after checking that its
/// stdout is byte-identical to php's.
fn same_as_php(prefix: &str, source: &str, flags: &[&str]) -> (String, String) {
    let dir = make_test_dir(prefix);
    fs::write(dir.join("main.php"), source).unwrap();
    compile_ok(&dir, flags);
    let ours = run(&dir);
    assert!(
        ours.status.success(),
        "native execution failed ({flags:?}, {}):\n{}\n{}",
        ours.status,
        text(&ours.stdout),
        text(&ours.stderr)
    );
    let theirs = php_stdout(&dir);
    assert_eq!(text(&ours.stdout), theirs, "stdout differs from php ({flags:?})");
    (text(&ours.stdout), text(&ours.stderr))
}

/// The live block count a `--heap-debug` run reports, 0 when clean.
fn live_blocks(stderr: &str) -> usize {
    if stderr.contains("leak summary: clean") {
        return 0;
    }
    stderr
        .lines()
        .find_map(|line| line.split("leak summary: live_blocks=").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("no leak summary in:\n{stderr}"))
}

const CLASS_REPORT: &str = r####"<?php
namespace T;

interface Shape
{
    const SIDES = 0;
    public function area(): float;
}

trait Loud
{
    public function shout(): string { return strtoupper($this->name()); }
}


final class Tag
{
    public function __construct(public string $name = "", public int $weight = 0) {}
}

abstract class Base implements Shape
{
    const KIND = "base";
    const LIST = [1, "two", 3.5, null, true];
    const MAP = ['a' => 1, 'b' => ['c' => 2]];
    protected static int $made = 0;
    public int $id = 7;
    public ?string $label = null;
    public array $tags = ['x', 'y'];
    abstract public function name(): string;
    public function area(): float { return 0.0; }
}

#[Tag("sq", weight: 3)]
class Square extends Base implements \Countable
{
    use Loud;
    const SIDES = 4;
    const RATIO = 1.5;
    private float $side = 2.0;
    public function __construct(float $side = 2.0) { $this->side = $side; }
    public function name(): string { return "square"; }
    public function area(): float { return $this->side ** 2; }
    public function count(): int { return 4; }
}

enum Level: int
{
    case Low = 1;
    case High = 2;
    const DEFAULT = self::Low;
}

function report(\ReflectionClass $r): void
{
    echo $r->getName(), "|", $r->getShortName(), "|", $r->getNamespaceName(), "|", basename($r->getFileName()), "|", "\n";
    echo implode(",", $r->getInterfaceNames()), "\n";
    echo json_encode([$r->isAbstract(), $r->isFinal(), $r->isInterface(), $r->isEnum(), $r->isInstantiable(), $r->isTrait(), $r->isInternal(), $r->isUserDefined(), $r->isCloneable(), $r->isIterable(), $r->getModifiers()]), "\n";
    echo json_encode([$r->implementsInterface("Countable"), $r->hasMethod("area"), $r->hasProperty("side"), $r->hasConstant("SIDES"), $r->hasMethod("nope")]), "\n";
    echo json_encode($r->getConstants()), "\n";
    echo json_encode($r->getDefaultProperties()), "\n";
    echo json_encode(array_map(fn($a) => [$a->getName(), $a->getArguments()], $r->getAttributes())), "\n";
}

$names = [Square::class, Base::class, Shape::class, Loud::class, Tag::class];
if ($argc > 5) {
    $names[] = "nothing";
}
foreach ($names as $name) {
    report(new \ReflectionClass($name));
    echo "--\n";
}
$square = new Square(3.0);
$r = new \ReflectionClass($names[0]);
var_dump($r->isInstance($square), $r->isInstance(new \stdClass()));
var_dump($r->newInstance(4.0)->area(), $r->newInstanceArgs([5.0])->area());
echo get_class($r->newInstanceWithoutConstructor()), "\n";
try {
    new \ReflectionClass($argc > 5 ? "x" : "T\\Missing");
} catch (\ReflectionException $e) {
    echo $e->getMessage(), "\n";
}
try {
    new \ReflectionEnum($argc > 5 ? "x" : Square::class);
} catch (\ReflectionException $e) {
    echo $e->getMessage(), "\n";
}
$e = new \ReflectionEnum(Level::class);
var_dump($e->isBacked(), $e->hasCase("Low"), $e->hasCase("Mid"));
$o = new \ReflectionObject($square);
echo $o->getName(), "\n";
"####;

const LITERAL_REPORT: &str = r####"<?php
namespace T;

interface Shape
{
    const SIDES = 0;
    public function area(): float;
}

trait Loud
{
    public function shout(): string { return strtoupper($this->name()); }
}

#[\Attribute(\Attribute::TARGET_CLASS)]
final class Tag
{
    public function __construct(public string $name = "", public int $weight = 0) {}
}

abstract class Base implements Shape
{
    const KIND = "base";
    const LIST = [1, "two", 3.5, null, true];
    const MAP = ['a' => 1, 'b' => ['c' => 2]];
    protected static int $made = 0;
    public int $id = 7;
    public ?string $label = null;
    public array $tags = ['x', 'y'];
    abstract public function name(): string;
    public function area(): float { return 0.0; }
}

#[Tag("sq", weight: 3)]
class Square extends Base implements \Countable
{
    use Loud;
    const SIDES = 4;
    const RATIO = 1.5;
    private float $side = 2.0;
    public function __construct(float $side = 2.0) { $this->side = $side; }
    public function name(): string { return "square"; }
    public function area(): float { return $this->side ** 2; }
    public function count(): int { return 4; }
}

enum Level: int
{
    case Low = 1;
    case High = 2;
    const DEFAULT = self::Low;
}

function report(\ReflectionClass $r): void
{
    echo $r->getName(), "|", $r->getShortName(), "|", $r->getNamespaceName(), "|", basename($r->getFileName()), "|", $r->getStartLine(), "\n";
    echo implode(",", $r->getInterfaceNames()), "\n";
    echo json_encode([$r->isAbstract(), $r->isFinal(), $r->isInterface(), $r->isEnum(), $r->isInstantiable(), $r->isTrait(), $r->isInternal(), $r->isUserDefined(), $r->isCloneable(), $r->isIterable(), $r->getModifiers()]), "\n";
    echo json_encode([$r->implementsInterface("Countable"), $r->hasMethod("area"), $r->hasProperty("side"), $r->hasConstant("SIDES"), $r->hasMethod("nope")]), "\n";
    var_dump($r->getConstants());
    var_dump($r->getDefaultProperties());
    echo implode(",", array_map(fn($m) => $m->getName(), $r->getMethods())), "\n";
    echo implode(",", array_map(fn($p) => $p->getName(), $r->getProperties())), "\n";
    echo implode(",", array_map(fn($c) => $c->getName(), $r->getReflectionConstants())), "\n";
    $parent = $r->getParentClass();
    echo $parent ? $parent->getName() : "none", "\n";
    $ctor = $r->getConstructor();
    echo $ctor ? $ctor->getName() : "noctor", "\n";
    echo $r->hasMethod("area") ? $r->getMethod("area")->class : "-", "\n";
    echo $r->hasProperty("side") ? $r->getProperty("side")->getName() : "-", "\n";
    var_dump($r->isSubclassOf(\T\Base::class));
}
report(new \ReflectionClass(Square::class));
report(new \ReflectionClass(Base::class));
report(new \ReflectionClass(Shape::class));
report(new \ReflectionClass(Loud::class));
report(new \ReflectionClass(Level::class));
$r = new \ReflectionClass(Square::class);
$square = new Square(3.0);
var_dump($r->isInstance($square), $r->isInstance(new \stdClass()));
var_dump($r->newInstance(4.0)->area(), $r->newInstanceArgs([5.0])->area());
echo get_class($r->newInstanceWithoutConstructor()), "\n";
echo count($r->getAttributes()), " ", $r->getAttributes()[0]->newInstance()->weight, "\n";
"####;

const ERRORS: &str = r####"<?php
namespace N;

class A {}
enum E { case X; }

function attempt(callable $make): void
{
    try {
        $r = $make();
        echo get_class($r), " ", $r->getName(), "\n";
    } catch (\ReflectionException $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
$names = [A::class, E::class, "N\\Nope", "\\N\\A", "n\\a"];
foreach ($names as $name) {
    attempt(fn() => new \ReflectionClass($name));
    attempt(fn() => new \ReflectionEnum($name));
}
attempt(fn() => new \ReflectionObject(new A()));
attempt(fn() => new \ReflectionClass(new A()));
"####;

const SUBCLASS_SERIALIZE: &str = r####"<?php
class Mine extends ReflectionClass {
    public function __serialize(): array { return ['x' => 1]; }
}
function attempt(string $label, mixed $value): void {
    try {
        echo $label, ": ", serialize($value), "\n";
    } catch (Exception $e) {
        echo $label, ": ", get_class($e), " ", $e->getMessage(), "\n";
    }
}
attempt("subclass", new Mine('Mine'));
echo "after\n";
"####;

const SUBCLASS_RUNTIME_NAME: &str = r####"<?php
class Mine extends ReflectionClass {
    public function hello(): string { return "hi " . $this->getShortName(); }
}
class Other {}
$name = $argc > 5 ? "x" : "Other";
$m = new Mine($name);
echo $m->hello(), "\n";
$n = new Mine(Mine::class);
echo $n->hello(), " ", $n->getParentClass()->getName(), "\n";
"####;

const CONSTANTS_CLOSED: &str = r####"<?php
namespace S;

interface I { const IC = 'i'; }
abstract class P implements I
{
    const PC = [1, 2, 'k' => ['n' => null]];
    protected const PROT = 3;
    private const PRIV = 4;
    final public const FIN = 5;
}
#[\Attribute]
final class Tag { public function __construct(public string $n = '') {} }
class K extends P
{
    #[Tag('k')]
    const KC = 1.5;
    const TYPED = 'x';
}
enum E: string { case A = 'a'; case B = 'b'; const D = self::A; }
enum U { case X; case Y; }

function dump(\ReflectionClassConstant $c): void
{
    echo $c->getName(), '|', $c->class, '|', (is_object($c->getValue()) ? get_class($c->getValue()) . '::' . $c->getValue()->name : json_encode($c->getValue())), '|', $c->getModifiers(), '|',
        json_encode([$c->isPublic(), $c->isProtected(), $c->isPrivate(), $c->isFinal(), $c->isEnumCase()]), '|',
        $c->getDeclaringClass()->getName(), '|', count($c->getAttributes()), "\n";
}
$names = ['S\K', 'S\E', 'S\U', 'S\P', 'S\I'];
foreach ($names as $cn) {
    $r = new \ReflectionClass($cn);
    foreach ($r->getReflectionConstants() as $c) {
        dump($c);
    }
    echo "--\n";
}
$k = new \ReflectionClassConstant($argc > 5 ? 'x' : 'S\K', 'PC');
dump($k);
$k = new \ReflectionClassConstant(new K(), 'KC');
dump($k);
echo $k->getAttributes()[0]->newInstance()->n, "\n";
foreach ((new \ReflectionEnum('S\E'))->getCases() as $c) {
    echo get_class($c), ' ', $c->getName(), ' ', $c->getValue()->name, ' ', $c->getBackingValue(), ' ', $c->getEnum()->getName(), "\n";
}
foreach ((new \ReflectionEnum('S\U'))->getCases() as $c) {
    echo get_class($c), ' ', $c->getName(), ' ', $c->getValue()->name, ' ', $c->getEnum()->getName(), "\n";
}
$u = new \ReflectionEnumUnitCase('S\U', 'Y');
echo $u->getName(), "\n";
foreach ([['S\K', 'NOPE'], ['S\Missing', 'X']] as [$class, $name]) {
    try {
        new \ReflectionClassConstant($argc > 5 ? 'x' : $class, $name);
    } catch (\ReflectionException $e) {
        echo $e->getMessage(), "\n";
    }
}
try {
    new \ReflectionEnumUnitCase($argc > 5 ? 'x' : 'S\E', 'D');
} catch (\ReflectionException $e) {
    echo $e->getMessage(), "\n";
}
"####;

const CONSTANTS_LITERAL: &str = r####"<?php
namespace S;

interface I { const IC = 'i'; }
abstract class P implements I
{
    const PC = [1, 2, 'k' => ['n' => null]];
    protected const PROT = 3;
    private const PRIV = 4;
    final public const FIN = 5;
}
#[\Attribute]
final class Tag { public function __construct(public string $n = '') {} }
class K extends P
{
    #[Tag('k')]
    const KC = 1.5;
    const TYPED = 'x';
}
enum E: string { case A = 'a'; case B = 'b'; const D = self::A; }
enum U { case X; case Y; }

function dump(\ReflectionClassConstant $c): void
{
    echo $c->getName(), '|', $c->class, '|', (is_object($c->getValue()) ? get_class($c->getValue()) . '::' . $c->getValue()->name : json_encode($c->getValue())), '|', $c->getModifiers(), '|',
        json_encode([$c->isPublic(), $c->isProtected(), $c->isPrivate(), $c->isFinal(), $c->isEnumCase()]), '|',
        $c->getDeclaringClass()->getName(), '|', count($c->getAttributes()), "\n";
}
foreach ([new \ReflectionClass(K::class), new \ReflectionClass(E::class), new \ReflectionClass(U::class), new \ReflectionClass(P::class), new \ReflectionClass(I::class)] as $r) {
    foreach ($r->getReflectionConstants() as $c) {
        dump($c);
    }
    echo "--\n";
}
$k = new \ReflectionClassConstant(P::class, 'PC');
dump($k);
$k = new \ReflectionClassConstant(K::class, 'KC');
dump($k);
echo $k->getAttributes()[0]->newInstance()->n, "\n";
foreach ((new \ReflectionEnum('S\E'))->getCases() as $c) {
    echo get_class($c), ' ', $c->getName(), ' ', $c->getValue()->name, ' ', $c->getBackingValue(), ' ', $c->getEnum()->getName(), "\n";
}
foreach ((new \ReflectionEnum('S\U'))->getCases() as $c) {
    echo get_class($c), ' ', $c->getName(), ' ', $c->getValue()->name, ' ', $c->getEnum()->getName(), "\n";
}
$u = new \ReflectionEnumUnitCase('S\U', 'Y');
echo $u->getName(), "\n";
try { new \ReflectionClassConstant(K::class, 'NOPE'); } catch (\ReflectionException $e) { echo $e->getMessage(), "\n"; }
try { new \ReflectionEnumUnitCase(E::class, 'D'); } catch (\ReflectionException $e) { echo $e->getMessage(), "\n"; }
"####;

const PROPERTIES: &str = r####"<?php
namespace S;

interface Marker {}
interface Other {}
#[\Attribute]
final class Tag { public function __construct(public string $n = '') {} }

class Base
{
    public int $id = 7;
    protected ?string $label = null;
    private array $secret = ['k' => [1, 2]];
    public static int $count = 3;
    public $untyped;
    public $untypedDefault = 'u';
    public float $ratio = 1.5;
    public bool $flag = true;
}
class Child extends Base implements Marker, Other
{
    #[Tag('c')]
    public int|string $mixed = 5;
    public readonly int $locked;
    public ?Base $parent = null;
    public ?Marker $both = null;
    public function __construct(public string $promoted = 'p', int $locked = 9)
    {
        $this->locked = $locked;
    }
}

function typeName(?\ReflectionType $t): string
{
    if ($t === null) {
        return '-';
    }
    if ($t instanceof \ReflectionNamedType) {
        return 'N:' . $t->getName() . ($t->allowsNull() ? '?' : '') . ($t->isBuiltin() ? 'b' : '');
    }
    $parts = [];
    if ($t instanceof \ReflectionUnionType) {
        foreach ($t->getTypes() as $inner) {
            $parts[] = typeName($inner);
        }
    } elseif ($t instanceof \ReflectionIntersectionType) {
        foreach ($t->getTypes() as $inner) {
            $parts[] = typeName($inner);
        }
    }
    return get_class($t) . '[' . implode(',', $parts) . ']' . ($t->allowsNull() ? '?' : '');
}

function dumpProp(\ReflectionProperty $p): void
{
    echo $p->getName(), '|', $p->class, '|', $p->getModifiers(), '|',
        json_encode([$p->isPublic(), $p->isProtected(), $p->isPrivate(), $p->isStatic(), $p->isReadOnly(), $p->isPromoted(), $p->isDefault(), $p->hasType(), $p->hasDefaultValue()]), '|',
        typeName($p->getType()), '|', ($p->hasDefaultValue() ? json_encode($p->getDefaultValue()) : "nodef"), '|',
        $p->getDeclaringClass()->getName(), '|', count($p->getAttributes()), "\n";
}

$child = new Child();
foreach ([new \ReflectionClass(Child::class), new \ReflectionClass(Base::class)] as $r) {
    foreach ($r->getProperties() as $p) {
        dumpProp($p);
    }
    echo "--\n";
}
$p = new \ReflectionProperty(Child::class, 'id');
dumpProp($p);
echo $p->getValue($child), "\n";
$p->setValue($child, 11);
echo $child->id, "\n";
$q = new \ReflectionProperty(Base::class, 'secret');
echo json_encode($q->getValue($child)), "\n";
$q->setValue($child, ['z']);
echo json_encode($q->getValue($child)), "\n";
$s = new \ReflectionProperty(Base::class, 'count');
echo $s->getValue(), "\n";
$s->setValue(null, 8);
echo Base::$count, "\n";
var_dump((new \ReflectionProperty(Child::class, 'locked'))->isInitialized($child));
var_dump((new \ReflectionProperty(Child::class, 'both'))->isInitialized($child));
echo (new \ReflectionProperty(Child::class, 'mixed'))->getAttributes()[0]->newInstance()->n, "\n";
try { new \ReflectionProperty(Child::class, $argc > 5 ? 'x' : 'nope'); } catch (\ReflectionException $e) { echo $e->getMessage(), "\n"; }
"####;

const METHODS: &str = r####"<?php
namespace S;

interface Shape { public function area(): float; }
#[\Attribute]
final class Tag { public function __construct(public string $n = '') {} }
abstract class Base implements Shape
{
    public function __construct(protected int $size = 2) {}
    abstract public function name(): string;
    public function area(): float { return $this->size * 1.0; }
    final protected function hook(?string $a, int|string $b = 3, array $c = [1, 'x' => null], \Countable|null $d = null, &$e = 5, string ...$rest): ?int { return null; }
    public static function make(int $s = 4): static { return new static($s); }
    private function secret(string $x = self::KIND): string { return $x . $this->size; }
    public function callSecret(): string { return $this->secret(); }
    const KIND = 'k';
}
class Square extends Base
{
    #[Tag('m')]
    public function name(): string { return 'square'; }
    public function area(): float { return parent::area() * $this->size; }
}

function typeName(?\ReflectionType $t): string
{
    if ($t === null) {
        return '-';
    }
    if ($t instanceof \ReflectionNamedType) {
        return $t->getName() . ($t->allowsNull() ? '?' : '');
    }
    $parts = [];
    if ($t instanceof \ReflectionUnionType) {
        foreach ($t->getTypes() as $inner) {
            $parts[] = typeName($inner);
        }
    }
    return 'U[' . implode(',', $parts) . ']';
}

function dumpMethod(\ReflectionMethod $m): void
{
    echo $m->getName(), '|', $m->class, '|', $m->getModifiers(), '|',
        json_encode([$m->isPublic(), $m->isProtected(), $m->isPrivate(), $m->isStatic(), $m->isFinal(), $m->isAbstract(), $m->isConstructor()]), '|',
        typeName($m->getReturnType()), '|', $m->hasReturnType() ? 'rt' : 'nort', '|',
        $m->getNumberOfParameters(), '/', $m->getNumberOfRequiredParameters(), '|',
        $m->getDeclaringClass()->getName(), '|', count($m->getAttributes()), "\n";
    foreach ($m->getParameters() as $p) {
        echo '  ', $p->getPosition(), ' ', $p->getName(), ' ', typeName($p->getType()),
            ' ', json_encode([$p->isOptional(), $p->isVariadic(), $p->isPassedByReference(), $p->isPromoted(), $p->allowsNull(), $p->hasType(), $p->isDefaultValueAvailable()]);
        if ($p->isDefaultValueAvailable()) {
            echo ' = ', json_encode($p->getDefaultValue());
            if ($p->isDefaultValueConstant()) {
                echo ' const ', $p->getDefaultValueConstantName();
            }
        }
        echo ' @', $p->getDeclaringFunction()->getName(), ' ', $p->getDeclaringClass()->getName(), "\n";
    }
}

foreach ([new \ReflectionClass(Square::class), new \ReflectionClass(Base::class), new \ReflectionClass(Shape::class)] as $r) {
    foreach ($r->getMethods() as $m) {
        dumpMethod($m);
    }
    $c = $r->getConstructor();
    echo $c ? 'ctor ' . $c->class : 'no ctor', "\n";
    echo "--\n";
}
$m = new \ReflectionMethod(Square::class, 'area');
dumpMethod($m);
echo $m->hasPrototype() ? 'proto ' . $m->getPrototype()->name : 'noproto', "\n";
$sq = new Square(3);
echo $m->invoke($sq), "\n";
echo $m->invokeArgs($sq, []), "\n";
$h = new \ReflectionMethod('S\\Base::hook');
echo $h->getName(), "\n";
echo (new \ReflectionMethod(Square::class, 'name'))->getAttributes()[0]->newInstance()->n, "\n";
echo (new \ReflectionMethod(Square::class, 'make'))->invoke(null, 5)->area(), "\n";
try { new \ReflectionMethod(Square::class, $argc > 5 ? 'x' : 'nope'); } catch (\ReflectionException $e) { echo $e->getMessage(), "\n"; }
"####;

const FUNCTIONS: &str = r####"<?php
namespace S;

#[\Attribute]
final class Mark { public function __construct(public string $n = '') {} }

#[Mark('fn')]
function greet(string $name, int|float $times = 2, ?array $opts = null, &$ref = 1, string ...$rest): string
{
    return str_repeat($name, (int) $times);
}
function gen(int $n): \Generator { yield $n; }
function &byref(array &$a): array { return $a; }

function describe(\ReflectionFunction $f): void
{
    echo $f->getName(), '|', $f->getShortName(), '|', $f->getNumberOfParameters(), '/', $f->getNumberOfRequiredParameters(), '|',
        json_encode([$f->isGenerator(), $f->returnsReference(), $f->isClosure(), $f->isVariadic(), $f->hasReturnType()]), '|',
        $f->hasReturnType() ? (string) $f->getReturnType() : '-', '|', count($f->getAttributes()), "\n";
    foreach ($f->getParameters() as $p) {
        echo '  ', $p->getPosition(), ' ', $p->getName(), ' ', $p->hasType() ? (string) $p->getType() : '-',
            ' ', json_encode([$p->isOptional(), $p->isVariadic(), $p->isPassedByReference(), $p->allowsNull(), $p->isDefaultValueAvailable()]);
        if ($p->isDefaultValueAvailable()) {
            echo ' = ', json_encode($p->getDefaultValue());
        }
        echo ' ', $p->getDeclaringFunction()->getName(), ' ', var_export($p->getDeclaringClass(), true), "\n";
    }
}

describe(new \ReflectionFunction('S\greet'));
describe(new \ReflectionFunction('s\GEN'));
describe(new \ReflectionFunction('S\byref'));
$closure = function (int $a, $b = [1, 2]) use (&$closure): int { return $a; };
describe(new \ReflectionFunction($closure));
$arrow = fn(string $s = 'x'): string => $s . '!';
describe(new \ReflectionFunction($arrow));
$static = static function (): void {};
echo (new \ReflectionFunction($static))->getNumberOfParameters(), "\n";
$rf = new \ReflectionFunction('S\greet');
echo $rf->invoke('ab', 3), "\n";
echo $rf->invokeArgs(['c', 2]), "\n";
echo (new \ReflectionFunction($arrow))->invoke('q'), "\n";
echo (new \ReflectionFunction('strlen'))->getNumberOfParameters(), "\n";
echo (new \ReflectionFunction($closure))->getStartLine() > 0 ? "lines\n" : "nolines\n";
echo (new \ReflectionFunction('S\greet'))->getAttributes()[0]->newInstance()->n, "\n";
try { new \ReflectionFunction($argc > 5 ? 'x' : 'S\nope'); } catch (\ReflectionException $e) { echo $e->getMessage(), "\n"; }
"####;

const METHOD_CLOSURES: &str = r####"<?php
class C {
    public function plain(): string { return 'x'; }
    public function greet(string $name, int $n = 3): string { return $name; }
    public static function make(?int $a): ?C { return null; }
    public function __invoke(int $z) { return $z; }
}
function describe(ReflectionFunctionAbstract $r): void {
    echo $r->getName(), ' ', $r->getNumberOfParameters(), ' ', $r->getNumberOfRequiredParameters(), ' ', $r->isClosure() ? 'closure' : 'plain';
    foreach ($r->getParameters() as $p) {
        echo ' [', $p->getName(), ':', $p->getType(), ($p->isOptional() ? '?' : ''), ']';
    }
    echo ' -> ', $r->getReturnType() ?? 'none', "\n";
}
function mk(mixed $c): ReflectionFunction { return new ReflectionFunction($c(...)); }
describe(mk([new C, 'plain']));
describe(mk([new C, 'greet']));
describe(mk(['C', 'make']));
describe(mk('C::make'));
describe(mk(new C));
describe(mk(fn(int $q, $r = 2) => $q));
echo "done\n";
"####;

/// Every ReflectionClass/ReflectionEnum/ReflectionObject method Symfony and Twig use, for classes
/// named at runtime in a build with no interpreter.
#[test]
fn closed_world_reflection_class_matches_php_for_runtime_names() {
    same_as_php("rt_closed_report", CLASS_REPORT, &["--closed-world"]);
}

/// The same table answers literal names in a default build.
#[test]
fn default_build_reflection_class_matches_php_for_literal_names() {
    same_as_php("rt_default_report", LITERAL_REPORT, &[]);
}

/// Missing classes, non-enums, leading separators, case folding, objects: the exceptions and the
/// names come out as php's.
#[test]
fn closed_world_reflection_errors_and_names_match_php() {
    same_as_php("rt_closed_errors", ERRORS, &["--closed-world"]);
}

/// A user class extending ReflectionClass builds its reflector from the same table: serialization
/// is still refused, and a runtime name is answered.
#[test]
fn reflector_subclasses_read_the_table() {
    same_as_php("rt_subclass_ser", SUBCLASS_SERIALIZE, &[]);
    same_as_php("rt_subclass_name", SUBCLASS_RUNTIME_NAME, &[]);
    same_as_php("rt_subclass_name_cw", SUBCLASS_RUNTIME_NAME, &["--closed-world"]);
    // With the interpreter linked too: a subclass has no interpreter path, so the table builds it.
    same_as_php("rt_subclass_name_eval", SUBCLASS_RUNTIME_NAME, &["--with-eval"]);
}

/// Constructing reflectors must not accumulate memory. Closed world: ReflectionClass of a class
/// with an attribute, a ReflectionObject (which redeclares the class slots and so has its own
/// filler) and a ReflectionEnum. Default build: the literal-name ReflectionClass sites.
#[test]
fn constructing_reflectors_is_heap_clean() {
    let closed_program = |iterations: usize| {
        format!(
            r#"<?php
#[Attribute]
final class Tag {{ public function __construct(public string $name = "") {{}} }}
class Base {{ public $a = 1; }}
#[Tag("sq")]
class Kid extends Base {{ const K = [1, 2]; public array $b = ["x"]; }}
enum Level: int {{ case Low = 1; case High = 2; }}
function once(int $n): string {{
    $names = [Kid::class, Base::class];
    $out = "";
    foreach ($names as $name) {{
        $rc = new ReflectionClass($name);
        $out .= $rc->getShortName() . count($rc->getAttributes()) . count($rc->getConstants());
    }}
    $ro = new ReflectionObject(new Kid());
    $out .= $ro->getName() . count($ro->getAttributes());
    $re = new ReflectionEnum(Level::class);
    $out .= $re->isBacked() ? "b" : "u";
    return $out;
}}
$last = "";
for ($i = 0; $i < {iterations}; $i++) {{
    $last = once($i);
}}
echo $last, "\n";
"#
        )
    };
    let default_program = |iterations: usize| {
        format!(
            r#"<?php
#[Attribute]
final class Tag {{ public function __construct(public string $name = "") {{}} }}
class Base {{ public $a = 1; }}
#[Tag("sq")]
class Kid extends Base {{ const K = [1, 2]; public array $b = ["x"]; }}
function once(int $n): string {{
    $out = "";
    $rc = new ReflectionClass(Kid::class);
    $out .= $rc->getShortName() . count($rc->getAttributes()) . count($rc->getConstants());
    $rc = new ReflectionClass(Base::class);
    $out .= $rc->getShortName() . count($rc->getAttributes()) . count($rc->getConstants());
    return $out;
}}
$last = "";
for ($i = 0; $i < {iterations}; $i++) {{
    $last = once($i);
}}
echo $last, "\n";
"#
        )
    };
    type Program<'a> = &'a dyn Fn(usize) -> String;
    let cases: [(Program, &[&str]); 2] = [
        (&closed_program, &["--closed-world", "--heap-debug"]),
        (&default_program, &["--heap-debug"]),
    ];
    for (program, flags) in cases {
        let mut stderrs = Vec::new();
        for iterations in [5usize, 25] {
            let (_, stderr) = same_as_php("rt_heap", &program(iterations), flags);
            stderrs.push(stderr);
        }
        // Enum cases are statics that stay live at exit; what must not change is the count.
        assert_eq!(
            live_blocks(&stderrs[0]),
            live_blocks(&stderrs[1]),
            "live blocks grew with the iteration count ({flags:?}):\n5: {}\n25: {}",
            stderrs[0],
            stderrs[1]
        );
    }
}

/// `ReflectionObject` fills the class slots it redeclares: its attributes are not lost.
#[test]
fn reflection_object_reads_class_attributes_from_the_table() {
    let source = r#"<?php
#[Attribute]
final class Tag { public function __construct(public string $name = "") {} }
#[Tag("sq")]
class Sq {}
$o = new ReflectionObject(new Sq());
$attributes = $o->getAttributes();
echo count($attributes), " ", $attributes[0]->getName(), " ", $attributes[0]->newInstance()->name, "\n";
"#;
    same_as_php("rt_object_attrs", source, &["--closed-world"]);
}

/// The reflection code a closed-world build links does not grow with the number of reflected
/// classes: the rows are data. (Per-class code that remains in the program, such as property
/// initializers of classes that may be instantiated by name, is not reflection code.)
#[test]
fn closed_world_reflection_code_is_flat_in_the_number_of_classes() {
    let measure = |classes: usize| -> usize {
        let dir = make_test_dir("rt_flat");
        let mut source = String::from("<?php\nnamespace P;\ninterface Marker {}\n");
        let mut names = Vec::new();
        for i in 0..classes {
            source.push_str(&format!(
                "class C{i} implements Marker {{ const A = {i}; const L = [1, 2, {i}]; public int $x = {i}; protected array $t = [{i}]; }}\n"
            ));
            names.push(format!("C{i}::class"));
        }
        source.push_str(&format!(
            "$names = [{}];\n$total = 0;\nforeach ($names as $name) {{\n    $r = new \\ReflectionClass($name);\n    $total += strlen($r->getShortName()) + count($r->getConstants()) + count($r->getInterfaceNames()) + count($r->getDefaultProperties());\n}}\necho $total, \"\\n\";\n",
            names.join(", ")
        ));
        fs::write(dir.join("main.php"), source).unwrap();
        compile_ok(&dir, &["--closed-world", "--emit-asm"]);
        let asm = fs::read_to_string(dir.join("main.s")).expect("--emit-asm writes main.s");
        let mut label = String::new();
        let mut reflection_instructions = 0usize;
        for line in asm.lines() {
            if !line.starts_with(' ') && line.ends_with(':') && !line.starts_with(['L', '.']) {
                label = line.trim_end_matches(':').to_string();
                continue;
            }
            // `ReflectionAttribute::newInstance()` ends in a dynamic `new $name(...)`, whose
            // class ladder is the program's dynamic-instantiation cost, not reflection's.
            let reflection_label = (label.contains("Reflection")
                || label.contains("refl_")
                || label.contains("reflection_class_extras"))
                && !label.contains("ReflectionAttribute_newinstance");
            let instruction = line.starts_with("    ")
                && !line.trim_start().starts_with(['.', ';'])
                && !line.trim().is_empty();
            if reflection_label && instruction {
                reflection_instructions += 1;
            }
        }
        assert!(reflection_instructions > 0, "no reflection code found in {} bytes of asm", asm.len());
        reflection_instructions
    };
    let few = measure(2);
    let many = measure(40);
    assert_eq!(
        few, many,
        "reflection code grew with the number of reflected classes: {few} instructions for 2, {many} for 40"
    );
}

/// ReflectionClassConstant / ReflectionEnumUnitCase / ReflectionEnumBackedCase for runtime names.
#[test]
fn closed_world_constant_and_case_reflectors_match_php() {
    same_as_php("rt_const_closed", CONSTANTS_CLOSED, &["--closed-world"]);
    same_as_php("rt_const_default", CONSTANTS_LITERAL, &[]);
}

/// Nested enum-case arrays retain singleton identity and independent reflected array storage.
#[test]
fn reflected_enum_case_arrays_match_php() {
    assert_reflected_enum_case_arrays("rt_enum_array_default", &["--heap-debug"]);
}

/// Table-decoded enum defaults use the same singleton and retirement rules without the bridge.
#[test]
fn closed_world_reflected_enum_case_arrays_match_php() {
    assert_reflected_enum_case_arrays("rt_enum_array_closed", &["--closed-world", "--heap-debug"]);
}

/// Checks identity, independent containers and terminal cleanup for each reflection pipeline.
fn assert_reflected_enum_case_arrays(name: &str, flags: &[&str]) {
    let source = include_str!("fixtures/reflection_enum_array_defaults/main.php");
    let (stdout, stderr) = same_as_php(name, source, flags);
    assert_eq!(stdout, "same:safe:same\nsame:safe:same\nsame:safe:same\nsafe:safe\n");
    assert_eq!(live_blocks(&stderr), 0, "{name}: {stderr}");
}

/// Runtime-name `constant()` returns an owned box, not a second untracked enum lease.
#[test]
fn dynamic_enum_constant_boxes_release_their_singletons() {
    let source = r#"<?php
enum LookupChoice { case A; case B; }
$name = $argv[1] ?? 'LookupChoice::A';
for ($i = 0; $i < 30; $i++) {
    $value = constant($name);
    $other = constant('LookupChoice::' . ($i % 2 === 0 ? 'A' : 'B'));
}
echo $value === LookupChoice::A ? "same\n" : "different\n";
echo $other === LookupChoice::B ? "same\n" : "different\n";
"#;
    for (name, flags) in [
        ("rt_enum_lookup_default", &["--heap-debug"][..]),
        ("rt_enum_lookup_closed", &["--closed-world", "--heap-debug"][..]),
    ] {
        let (stdout, stderr) = same_as_php(name, source, flags);
        assert_eq!(stdout, "same\nsame\n");
        assert_eq!(live_blocks(&stderr), 0, "{name}: {stderr}");
    }
}

/// A table constructor receives PHP's callable pair and selector, not three legacy operands.
#[test]
fn closed_world_parameter_constructor_preserves_callable_pairs() {
    let source = r#"<?php
class ParameterPairHost {
    public static function choose(int $number = 7): void {}
}
$parameter = new ReflectionParameter([ParameterPairHost::class, 'choose'], 0);
echo $parameter->getName(), ':', $parameter->getDefaultValue(), "\n";
$named = new ReflectionParameter(function: [ParameterPairHost::class, 'choose'], param: 'number');
echo $named->getName(), ':', $named->getPosition(), "\n";
"#;
    let (stdout, _) = same_as_php("rt_parameter_pairs", source, &["--closed-world"]);
    assert_eq!(stdout, "number:7\nnumber:0\n");
}

/// Compiled parameter lookup validates runtime method names through its table constructor.
#[test]
fn closed_world_parameter_constructor_accepts_runtime_method_names() {
    let source = r#"<?php
class ParameterRuntimeHost {
    public function choose(int $number = 7): void {}
}
$method = $argv[1] ?? 'choose';
$parameter = new ReflectionParameter([ParameterRuntimeHost::class, $method], 0);
echo $parameter->getName(), ':', $parameter->getDefaultValue(), "\n";
try {
    new ReflectionParameter([ParameterRuntimeHost::class, 'missing'], 0);
} catch (ReflectionException $error) {
    echo get_class($error), ':', $error->getMessage(), "\n";
}
try {
    new ReflectionParameter([ParameterRuntimeHost::class, 'choose'], 99);
} catch (ReflectionException $error) {
    echo get_class($error), ':', $error->getMessage(), "\n";
}
"#;
    let (stdout, _) = same_as_php("rt_parameter_runtime_method", source, &["--closed-world"]);
    assert!(stdout.starts_with("number:7\n"));
    assert_eq!(stdout.matches("ReflectionException:").count(), 2);
}

/// Enum singleton roots must also retire when a CLI program reaches its shutdown cleanup.
#[test]
fn enum_case_roots_are_released_at_shutdown() {
    let source = r#"<?php
enum ShutdownChoice { case A; case B; }
function keepChoice(ShutdownChoice $choice): ShutdownChoice { return $choice; }
for ($i = 0; $i < 30; $i++) {
    $a = keepChoice(ShutdownChoice::A);
    $b = keepChoice(ShutdownChoice::B);
}
echo $a->name, $b->name, "\n";
register_shutdown_function(static function () { echo (ShutdownChoice::A)->name, "\n"; });
"#;
    let (stdout, stderr) = same_as_php("rt_enum_roots", source, &["--heap-debug"]);
    assert_eq!(stdout, "AB\nA\n");
    assert_eq!(live_blocks(&stderr), 0, "{stderr}");
}

/// Literal table roots keep reflectors for interfaces and nested traits reachable by queries.
#[test]
fn literal_table_reflection_retains_related_classlikes() {
    let source = r#"<?php
interface RelatedRoot {}
interface RelatedLeaf extends RelatedRoot {}
trait RelatedInner {}
trait RelatedOuter { use RelatedInner; }
class RelatedTarget implements RelatedLeaf { use RelatedOuter; }
$reflection = new ReflectionClass(RelatedTarget::class);
$interfaces = $reflection->getInterfaces();
echo $interfaces[RelatedLeaf::class]->getName(), "\n";
echo $interfaces[RelatedRoot::class]->getName(), "\n";
$outer = $reflection->getTraits()[RelatedOuter::class];
echo $outer->getName(), "\n";
echo $outer->getTraits()[RelatedInner::class]->getName(), "\n";
"#;
    for (name, flags) in [
        ("rt_related_default", &[][..]),
        ("rt_related_closed", &["--closed-world"][..]),
    ] {
        let (stdout, _) = same_as_php(name, source, flags);
        assert_eq!(stdout, "RelatedLeaf\nRelatedRoot\nRelatedOuter\nRelatedInner\n");
    }
}

/// ReflectionProperty: values (private, protected, readonly, static), types, defaults, docs,
/// attributes, modifiers, promotion.
#[test]
fn property_reflectors_match_php() {
    same_as_php("rt_prop_closed", PROPERTIES, &["--closed-world"]);
    same_as_php("rt_prop_default", PROPERTIES, &[]);
}

/// ReflectionMethod and ReflectionParameter: invoke, closures, parameters, return types,
/// modifiers, prototypes.
#[test]
fn method_and_parameter_reflectors_match_php() {
    same_as_php("rt_method_closed", METHODS, &["--closed-world"]);
    same_as_php("rt_method_default", METHODS, &[]);
}

/// ReflectionFunction of named functions and closures, with no interpreter to build them.
#[test]
fn closed_world_function_reflectors_match_php() {
    same_as_php("rt_function_closed", FUNCTIONS, &["--closed-world"]);
}

/// A closure over a method (`$callable(...)`) reflects as the method: its parameters, types and
/// name. The descriptor names it `Class::method`; the method row answers.
#[test]
fn closed_world_reflection_function_of_method_closures_matches_php() {
    same_as_php("rt_method_closure", METHOD_CLOSURES, &["--closed-world"]);
}

/// Counts the instructions of every reflection routine in `main.s`.
fn reflection_instructions(asm: &str) -> usize {
    let mut label = String::new();
    let mut count = 0usize;
    for line in asm.lines() {
        if !line.starts_with(' ') && line.ends_with(':') && !line.starts_with(['L', '.']) {
            label = line.trim_end_matches(':').to_string();
            continue;
        }
        let reflection_label = (label.contains("Reflection")
            || label.contains("refl_")
            || label.contains("reflection_class_extras"))
            && !label.contains("ReflectionAttribute_newinstance");
        let instruction = line.starts_with("    ")
            && !line.trim_start().starts_with(['.', ';'])
            && !line.trim().is_empty();
        if reflection_label && instruction {
            count += 1;
        }
    }
    count
}

/// Member reflectors must not accumulate memory, and the code that serves them does not grow with
/// the number of reflected classes.
#[test]
fn member_reflectors_are_heap_clean_and_flat() {
    let program = |iterations: usize| {
        format!(
            r#"<?php
class Box {{
    public const A = 1;
    private int $secret = 7;
    public ?string $label = null;
    public function show(int $n, string ...$rest): string {{ return $this->label . $n; }}
}}
function once(string $class): string {{
    $out = "";
    $p = new ReflectionProperty($class, "secret");
    $out .= $p->getType()->getName() . $p->getValue(new $class());
    $m = new ReflectionMethod($class, "show");
    $out .= $m->getName() . count($m->getParameters()) . $m->getReturnType()->getName();
    $c = new ReflectionClassConstant($class, "A");
    $out .= $c->getValue();
    foreach ((new ReflectionClass($class))->getProperties() as $q) {{ $out .= $q->getName(); }}
    foreach ((new ReflectionClass($class))->getMethods() as $q) {{ $out .= $q->getName(); }}
    return $out;
}}
$last = "";
$name = $argc > 5 ? "Other" : "Box";
for ($i = 0; $i < {iterations}; $i++) {{
    $last = once($name);
}}
echo $last, "\n";
"#
        )
    };
    let mut stderrs = Vec::new();
    for iterations in [5usize, 25] {
        let (_, stderr) =
            same_as_php("rt_member_heap", &program(iterations), &["--closed-world", "--heap-debug"]);
        stderrs.push(stderr);
    }
    assert_eq!(
        live_blocks(&stderrs[0]),
        live_blocks(&stderrs[1]),
        "live blocks grew with the iteration count:\n5: {}\n25: {}",
        stderrs[0],
        stderrs[1]
    );

    let measure = |classes: usize| -> usize {
        let dir = make_test_dir("rt_member_flat");
        let mut source = String::from("<?php\nnamespace P;\n");
        let mut names = Vec::new();
        for i in 0..classes {
            source.push_str(&format!(
                "class C{i} {{ const A = {i}; public int $x = {i}; protected array $t = [{i}]; public function m{i}(int $a, ?string $b = null): int {{ return $a + {i}; }} }}\n"
            ));
            names.push(format!("C{i}::class"));
        }
        source.push_str(&format!(
            "$names = [{}];\n$total = 0;\nforeach ($names as $name) {{\n    foreach ((new \\ReflectionClass($name))->getProperties() as $p) {{ $total += strlen($p->getName()); }}\n    foreach ((new \\ReflectionClass($name))->getMethods() as $m) {{ $total += count($m->getParameters()); }}\n    $total += count((new \\ReflectionClass($name))->getReflectionConstants());\n}}\necho $total, \"\\n\";\n",
            names.join(", ")
        ));
        fs::write(dir.join("main.php"), source).unwrap();
        compile_ok(&dir, &["--closed-world", "--emit-asm"]);
        let asm = fs::read_to_string(dir.join("main.s")).expect("--emit-asm writes main.s");
        let count = reflection_instructions(&asm);
        assert!(count > 0, "no reflection code found in {} bytes of asm", asm.len());
        count
    };
    let few = measure(2);
    let many = measure(40);
    assert_eq!(
        few, many,
        "member reflection code grew with the number of reflected classes: {few} for 2, {many} for 40"
    );
}

/// Reflectors constructed inside a method, a closure and a function, from literal and from runtime
/// operands. The table answers every one of them in a closed-world build; the constructor takes
/// the operands as they are, so no site may need a literal name to be lowered. A builtin owner
/// constructed through the table was once populated from compile-time metadata as if it were a
/// user subclass, which needs a literal operand: `new ReflectionFunction($callback)` was refused
/// with "reflection literal missing data id".
const CONSTRUCTION_SITES: &str = r####"<?php
class Foo {
    public $p = 1;
    function m() { return 1; }
    function inMethod($c, $cb, $n) {
        $a = new ReflectionClass(Foo::class);
        $b = new ReflectionMethod(Foo::class, 'm');
        $d = new ReflectionProperty(Foo::class, 'p');
        $e = new ReflectionClass($c);
        $f = new ReflectionFunction($cb);
        $g = new ReflectionProperty(Exception::class, $n);
        return $a->getName() . $b->getName() . $d->getName() . $e->getName() . $f->isClosure() . $g->getName();
    }
}
function inFunction($c, $cb) {
    $x = new ReflectionClass(Foo::class);
    $y = new ReflectionMethod($c, 'm');
    $z = new ReflectionFunction($cb);
    return $x->getName() . $y->getName() . (new ReflectionFunction('inFunction'))->getName() . $z->isClosure();
}
$clo = function ($c) {
    $q = new ReflectionClass($c);
    return $q->getShortName() . (new ReflectionMethod(Foo::class, 'm'))->class;
};
echo (new Foo)->inMethod("Foo", fn() => 1, "message"), "\n";
echo inFunction("Foo", $clo), "\n";
echo $clo("Foo"), "\n";
"####;

#[test]
fn closed_world_reflectors_construct_in_methods_closures_and_functions() {
    same_as_php("rt_sites_cw", CONSTRUCTION_SITES, &["--closed-world"]);
}
