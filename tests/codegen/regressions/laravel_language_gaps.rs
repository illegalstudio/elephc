//! Purpose:
//! Regression tests for the language gaps the standard `laravel/laravel` skeleton hit when built
//! with `--closed-world`; each expected output is php 8.5's on the same program.
//!
//! Called from:
//! - `cargo test --test codegen_tests laravel_language_gaps`.
//!
//! Key details:
//! - Every fixture also asserts a clean `--heap-debug` leak summary.

use crate::support::*;

/// A property no implementation declares, read off an interface-typed value whose implementation
/// serves it through `__get` (Carbon's language files read `$date->dayOfWeek` off a
/// `CarbonInterface`): refused as undefined before, and the backend had no slot to dispatch to.
#[test]
fn test_magic_property_read_through_an_interface() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ninterface DateI { public function format(string $f): string; }\nclass D implements DateI {\n    public function format(string $f): string { return \"F:$f\"; }\n    public function __get(string $name) { return match ($name) { 'dayOfWeek' => 3, 'hour' => 14, default => null }; }\n}\n$calendar = [\n    'next' => function (DateI $date) { return $date->dayOfWeek === 3 ? 'wed' : 'other'; },\n    'h' => fn (DateI $date) => $date->hour + 1,\n];\n$d = new D();\necho $calendar['next']($d), \" \", $calendar['h']($d), \"\\n\";\nfunction weekday(DateI $date): int { return $date->dayOfWeek; }\necho weekday($d), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "wed 15\n3\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The same read through `mixed` and through `object`: the `object` form answered nothing.
#[test]
fn test_magic_property_read_through_mixed_and_object() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ninterface DateI { public function format(string $f): string; }\nclass D implements DateI {\n    public function format(string $f): string { return \"F:$f\"; }\n    public function __get(string $name) { return match ($name) { 'dayOfWeek' => 3, 'hour' => 14, default => null }; }\n}\nfunction weekday(mixed $date): int { return $date->dayOfWeek; }\nfunction viaObject(object $date) { return $date->hour; }\necho weekday(new D()), viaObject(new D()), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "314\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Several traits each declaring `abstract getBytes()`, used by separate `use` statements of a
/// class that implements it (ramsey/uuid's `Fields`): php never treats an abstract trait method as
/// a conflict.
#[test]
fn test_abstract_trait_methods_from_several_traits_do_not_conflict() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ntrait MaxT { abstract public function getBytes(): string; public function isMax(): bool { return $this->getBytes() === \"\\xff\"; } }\ntrait NilT { abstract public function getBytes(): string; public function isNil(): bool { return $this->getBytes() === \"\\x00\"; } }\ntrait SerT { abstract public function getBytes(): string; public function serialize(): string { return bin2hex($this->getBytes()); } }\ntrait OnlyAbstract { abstract public function tag(): string; }\ntrait AlsoAbstract { abstract public function tag(): string; }\nabstract class Base { use OnlyAbstract; use AlsoAbstract; }\nfinal class Fields extends Base {\n    use MaxT;\n    use NilT;\n    use SerT;\n    public function __construct(private string $bytes) {}\n    public function getBytes(): string { return $this->bytes; }\n    public function tag(): string { return 'f'; }\n}\n$f = new Fields(\"\\xff\");\nvar_dump($f->isMax(), $f->isNil(), $f->serialize(), $f->tag());\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\nstring(2) \"ff\"\nstring(1) \"f\"\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$this->resource::$forceWrapping` and `$builder()::$precision` (Laravel's `ResourceResponse`,
/// `Blueprint`): the class is the run-time object's, so a subclass's redeclared static answers.
#[test]
fn test_static_property_read_through_an_expression_receiver() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Res { public static bool $forceWrapping = true; public static int $precision = 6; }\nclass Holder { public Res $resource; public function __construct() { $this->resource = new Res(); } public function builder(): Res { return $this->resource; } }\n$h = new Holder();\nvar_dump($h->resource::$forceWrapping, $h->builder()::$precision);\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nint(6)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `++self::$rand[$i]` (symfony/uid's `Ulid`) and `static $a = null, $b = 0;`
/// (symfony/polyfill-uuid): both were parse errors.
#[test]
fn test_static_element_increment_and_multi_variable_static() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass U {\n    private static array $rand = [1, 2, 3];\n    public static function bump(int $i): int { ++self::$rand[$i]; self::$rand[$i]++; --self::$rand[$i]; return self::$rand[$i]; }\n}\nfunction seq(): string { static $lastTime = null, $clockSeq = 0; $clockSeq++; $lastTime = $lastTime === null ? 'first' : 'next'; return \"$lastTime:$clockSeq\"; }\necho U::bump(1), U::bump(1), \" \", seq(), \" \", seq(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "34 first:1 next:2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Laravel's `Worker::$currentJob` (untyped, inferred `null|int` from its call sites) calls
/// `->getResolvedJob()`: refused at compile time before; php dispatches on the run-time value
/// and raises Error for a non-object (the message's type word is bead elephc-1cx.13).
#[test]
fn test_method_call_on_an_untyped_property_inferred_without_objects() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Job { public function getResolvedJob(): string { return 'resolved'; } }\nclass Worker {\n    public $currentJob = null;\n    public function run($job) { $this->currentJob = $job; $r = $this->notify(); $this->currentJob = null; return $r; }\n    public function notify(): string { if (! $this->currentJob) { return 'none'; } return $this->currentJob->getResolvedJob(); }\n}\n$w = new Worker();\necho $w->run(new Job()), ' ', $w->run(0), \"\\n\";\ntry { echo $w->run(5), \"\\n\"; } catch (Error $e) { echo get_class($e), \"\\n\"; }\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "resolved none\nError\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `ArrayObject::ARRAY_AS_PROPS` / `STD_PROP_LIST` (Laravel's `AsArrayObject` cast): the injected
/// class declared no constants.
#[test]
fn test_array_object_flag_constants() {
    let out = compile_and_run_with_heap_debug(
        "<?php\n$o = new ArrayObject(['a' => 1], ArrayObject::ARRAY_AS_PROPS);\nvar_dump(ArrayObject::STD_PROP_LIST, ArrayObject::ARRAY_AS_PROPS, ArrayIterator::ARRAY_AS_PROPS, $o->getFlags(), $o['a']);\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "int(1)\nint(2)\nint(2)\nint(2)\nint(1)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$booleanNames = &static::$_booleans;` (psysh's `ConsoleTput`) and `$c = &self::$count`: a
/// local alias of a static was a parse error; writes through it land in the static.
#[test]
fn test_local_reference_to_a_static_property() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Tput {\n    protected static array $_booleans = ['auto_left_margin', 'auto_right_margin'];\n    public static int $count = 1;\n    public static function names(): array {\n        $booleanNames = &static::$_booleans;\n        $out = [];\n        foreach ($booleanNames as $i => $name) {\n            $out[$name] = $i;\n        }\n        $booleanNames[] = 'back_color_erase';\n        return $out;\n    }\n    public static function bump(): int {\n        $c = &self::$count;\n        $c += 10;\n        $c++;\n        return self::$count;\n    }\n}\nclass Sub extends Tput {\n    protected static array $_booleans = ['sub_only'];\n}\nvar_dump(Tput::names(), Sub::names(), count(Tput::names()));\nvar_dump(Tput::bump(), Tput::$count);\n$s = &Tput::$count;\n$s = 99;\nvar_dump(Tput::$count);\n$t = ($u = &Tput::$count);\nvar_dump($t);\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "array(2) {\n  [\"auto_left_margin\"]=>\n  int(0)\n  [\"auto_right_margin\"]=>\n  int(1)\n}\narray(1) {\n  [\"sub_only\"]=>\n  int(0)\n}\nint(3)\nint(12)\nint(12)\nint(99)\nint(99)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `CarbonInterface` declares `__get` itself, and Carbon's class file can be the autoloader's: a
/// read off the interface is the run-time object's to answer even when no compiled class
/// implements it (here the only implementation is eval-defined).
#[test]
fn test_magic_read_through_an_interface_that_declares_get() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ninterface DateLike {\n    public function __get(string $name);\n}\nfunction weekday(DateLike $date): string {\n    return $date->dayOfWeek === 2 ? 'tuesday' : 'other ' . $date->dayOfWeek;\n}\neval('class EvalDate implements DateLike { public function __construct(private int $d) {} public function __get(string $name) { return $name === \"dayOfWeek\" ? $this->d : null; } }');\n$cls = 'EvalDate';\necho weekday(new $cls(2)), \"\\n\";\necho weekday(new $cls(5)), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "tuesday\nother 5\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `__get`/`__set`/`__isset` of an eval-defined class, reached from compiled code: each call
/// leaked the boxed property name (2 blocks per read).
#[test]
fn test_eval_magic_property_hooks_release_the_name() {
    let out = compile_and_run_with_heap_debug(
        "<?php\neval('class EvalBag { private array $d = [\"_\" => 0]; public function __get(string $n) { return $this->d[$n] ?? null; } public function __set(string $n, $v): void { $this->d[$n] = $v; } public function __isset(string $n): bool { return isset($this->d[$n]); } }');\nfunction fill(object $bag, string $p, int $v): void { $bag->$p = $v; }\nfunction probe(object $bag, string $p): string { return isset($bag->$p) ? 'set:' . $bag->$p : 'unset'; }\n$cls = 'EvalBag';\n$o = new $cls();\nfill($o, 'a', 1);\nfill($o, 'b', 2);\nfill($o, 'a', 3);\necho probe($o, 'a'), ' ', probe($o, 'b'), ' ', probe($o, 'c'), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "set:3 set:2 unset\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `echo $object` called `__toString()` inside the echo instruction, so no IR value owned the
/// returned string: one string leaked per echo, statically bound, through an interface, an
/// `object`/base-class receiver and a `mixed` one alike.
#[test]
fn test_echo_of_an_object_releases_the_to_string_result() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ninterface Named { public function __toString(): string; }\nfinal class Prop implements Named {\n    public function __construct(private string $v) {}\n    public function __toString(): string { return $this->v; }\n}\nfinal class Lit { public function __toString(): string { return 'lit'; } }\nfinal class Cat {\n    public function __construct(private int $n) {}\n    public function __toString(): string { return 'n=' . $this->n; }\n}\nfinal class Via {\n    public function __construct(private int|float|string $v) {}\n    public function s(): string { return (string) $this->v; }\n    public function __toString(): string { return $this->s(); }\n}\nfunction show(Named $n): void { echo $n, \"|\"; }\n$p = new Prop(str_repeat('ab', 3));\necho $p, \"|\", $p, \"|\";\necho new Lit(), \"|\";\n$c = new Cat(7);\necho $c, \"|\", $c, \"|\";\necho new Via(1.5), \"|\";\nshow($p);\nshow($p);\nfor ($i = 0; $i < 3; $i++) { echo new Cat($i), \",\"; }\necho \"\\n\", $p, \"\\n\";\nclass Base { public function __construct(protected int $n) {} }\nfinal class Sub extends Base { public function __toString(): string { return 'sub' . $this->n; } }\nclass Parent1 { public function __toString(): string { return 'p' . spl_object_id($this) % 1; } }\nfinal class Child1 extends Parent1 {}\nfunction asObject(object $o): void { echo $o, \"|\"; }\nfunction asBase(Base $b): void { echo $b, \"|\"; }\nfunction asMixed(mixed $m): void { echo $m, \"|\"; }\n$s = new Sub(3);\nasObject($s); asObject($s);\nasBase($s);\nasMixed($s); asMixed(new Child1());\necho new Child1(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ababab|ababab|lit|n=7|n=7|1.5|ababab|ababab|n=0,n=1,n=2,\nababab\nsub3|sub3|sub3|sub3|p0|p0\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The same echo leak for an eval-defined class, whose `__toString()` the run-time coercion reaches
/// through the eval bridge.
#[test]
fn test_echo_of_an_eval_object_releases_the_to_string_result() {
    let out = compile_and_run_with_heap_debug(
        "<?php\neval('class EvalName { public function __construct(private string $v) {} public function __toString(): string { return $this->v; } }');\nfunction asObject(object $o): void { echo $o, \"|\"; }\nfunction asMixed(mixed $m): void { echo $m, \"|\"; }\n$cls = \"EvalName\"; $e = new $cls(str_repeat(\"e\", 3));\nasObject($e); asObject($e); asMixed($e); asMixed(\"plain\"); asMixed(42);\necho $e, \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "eee|eee|eee|plain|42|eee\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// ramsey/uuid's `SystemDceSecurityProvider::getUid()`: `static $uid = null;` caches the system uid
/// string, then `$uid = new IntegerObject($uid)` into the same cell; refused as `cannot reassign
/// $uid from string to IntegerObject` once flow narrowing had re-typed the static.
#[test]
fn test_null_initialized_static_local_takes_any_later_value() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ninterface TypeInterface extends JsonSerializable, Serializable\n{\n    public function toString(): string;\n    public function __toString(): string;\n}\nfinal class IntegerObject implements TypeInterface\n{\n    public function __construct(private int|float|string $value) {}\n    public function toString(): string { return (string) $this->value; }\n    public function __toString(): string { return $this->toString(); }\n    public function jsonSerialize(): string { return $this->toString(); }\n    public function serialize(): string { return $this->toString(); }\n    public function unserialize(string $data): void { $this->value = $data; }\n    public function __serialize(): array { return ['string' => $this->toString()]; }\n    public function __unserialize(array $data): void { $this->value = $data['string']; }\n}\nfinal class Provider\n{\n    public function getUid(): IntegerObject\n    {\n        /** @var IntegerObject | int | float | string | null $uid */\n        static $uid = null;\n        if ($uid instanceof IntegerObject) {\n            return $uid;\n        }\n        if ($uid === null) {\n            $uid = $this->getSystemUid();\n        }\n        if ($uid === '') {\n            throw new RuntimeException('no uid');\n        }\n        $uid = new IntegerObject($uid);\n        return $uid;\n    }\n    private function getSystemUid(): string { return '501'; }\n}\n$p = new Provider();\necho $p->getUid(), ' ', $p->getUid(), ' ', json_encode($p->getUid()), \"\\n\";\nvar_dump($p->getUid() instanceof TypeInterface, $p->getUid() instanceof Serializable);\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "501 501 \"501\"\nbool(true)\nbool(true)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Carbon's `Modifiers::nthOfQuarter()` reads `$date->month` off what `DateTime::setDate()` /
/// `modify()` are declared to return, a `DateTime`; only `Carbon::__get` serves it. Refused as an
/// undefined property before.
#[test]
fn test_magic_read_through_a_base_class_a_descendant_serves() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Moment extends DateTime {\n    public function __get(string $name): mixed {\n        return match ($name) {\n            'month' => (int) $this->format('n'),\n            'year' => (int) $this->format('Y'),\n            default => null,\n        };\n    }\n    public function firstOfQuarter(): static {\n        return $this->setDate((int) $this->format('Y'), intdiv((int) $this->format('n') - 1, 3) * 3 + 1, 1);\n    }\n    public function nthOfQuarter(int $nth): string|false {\n        $date = (clone $this)->setDate(2024, 5, 1);\n        $lastMonth = $date->month;\n        $year = $date->year;\n        $date = $date->firstOfQuarter()->modify('+' . $nth . ' days');\n        return ($lastMonth < $date->month || $year !== $date->year) ? false : $date->format('Y-m-d');\n    }\n}\nvar_dump((new Moment('2024-05-17'))->nthOfQuarter(2));\nvar_dump((new Moment('2024-05-17'))->nthOfQuarter(20));\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "string(10) \"2024-04-03\"\nstring(10) \"2024-04-21\"\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Carbon's `CarbonInterval::doCascade()`: `$previous = []` grows by `array_unshift()` below the
/// `foreach ($previous as [$subUnit, $subFactor])` that reads it; only `array_push` counted as
/// loop growth, so the list unpack saw `array<never>` and was refused.
#[test]
fn test_list_unpack_over_a_loop_grown_by_array_unshift() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfunction cascade(array $factors): array {\n    $newData = ['s' => 90.5, 'i' => 0, 'h' => 0];\n    $previous = [];\n    foreach ($factors as $source => [$target, $factor]) {\n        $value = $newData[$source];\n        $modulo = fmod($value, $factor);\n        $newData[$source] = $modulo;\n        $newData[$target] += ($value - $modulo) / $factor;\n        $decimalPart = fmod($newData[$source], 1);\n        if ($decimalPart !== 0.0) {\n            $unit = $source;\n            foreach ($previous as [$subUnit, $subFactor]) {\n                $newData[$unit] -= $decimalPart;\n                $newData[$subUnit] += $decimalPart * $subFactor;\n                $unit = $subUnit;\n            }\n        }\n        array_unshift($previous, [$source, $factor]);\n    }\n    return $newData;\n}\nvar_dump(cascade(['s' => ['i', 60], 'i' => ['h', 60]]));\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "array(3) {\n  [\"s\"]=>\n  float(30.5)\n  [\"i\"]=>\n  float(1)\n  [\"h\"]=>\n  float(0)\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// php's read-only `DatePeriod` properties (`start`, `end`, `interval`, `recurrences`,
/// `include_start_date`, `include_end_date`, `current`), which Carbon's `CarbonPeriod::instance()`
/// reads; `end`/`getEndDate()` follow the start's class.
#[test]
fn test_date_period_public_properties() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ndate_default_timezone_set('UTC');\n$p = new DatePeriod(new DateTimeImmutable('2024-01-01'), new DateInterval('P1D'), new DateTime('2024-01-04'));\nvar_dump(get_class($p->start), $p->start->format('Y-m-d'), get_class($p->end), $p->end->format('Y-m-d'));\nvar_dump($p->interval->d, $p->recurrences, $p->include_start_date, $p->include_end_date, $p->current);\n$r = new DatePeriod(new DateTime('2024-01-01'), new DateInterval('PT12H'), 3, DatePeriod::EXCLUDE_START_DATE);\nvar_dump($r->end, $r->recurrences, $r->include_start_date, $r->interval->h, get_class($r->start));\nforeach ($r as $d) {}\nvar_dump($r->current === null ? null : $r->current->format('Y-m-d H:i'));\n$e = new DatePeriod(new DateTime('2024-01-01'), new DateInterval('P1D'), new DateTime('2024-01-03'), DatePeriod::INCLUDE_END_DATE);\nvar_dump($e->include_end_date, $e->recurrences);\n$x = $p->start;\nvar_dump($x == $p->getStartDate(), $x === $p->start);\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "string(17) \"DateTimeImmutable\"\nstring(10) \"2024-01-01\"\nstring(17) \"DateTimeImmutable\"\nstring(10) \"2024-01-04\"\nint(1)\nint(1)\nbool(true)\nbool(false)\nNULL\nNULL\nint(3)\nbool(false)\nint(12)\nstring(8) \"DateTime\"\nstring(16) \"2024-01-03 00:00\"\nbool(true)\nint(2)\nbool(true)\nbool(false)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `foreach` over a user `Iterator` boxed each `current()`/`key()` result with the RETAINING
/// boxer, and an interface-typed receiver went through an adapter that did the same: one value
/// leaked per iteration on both paths.
#[test]
fn test_foreach_over_user_iterators_releases_each_value() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfinal class Item { public function __construct(public string $name) {} }\nfinal class Bag implements Iterator {\n    private int $i = 0;\n    /** @param list<Item> $items */\n    public function __construct(private array $items) {}\n    public function current(): Item { return $this->items[$this->i]; }\n    public function key(): string { return 'k' . $this->i; }\n    public function next(): void { $this->i++; }\n    public function rewind(): void { $this->i = 0; }\n    public function valid(): bool { return $this->i < count($this->items); }\n}\nfinal class Words implements Iterator {\n    private int $i = 0;\n    public function current(): string { return str_repeat('w', $this->i + 1); }\n    public function key(): int { return $this->i * 10; }\n    public function next(): void { $this->i++; }\n    public function rewind(): void { $this->i = 0; }\n    public function valid(): bool { return $this->i < 3; }\n}\nfunction walk(Iterator $it): string {\n    $out = '';\n    foreach ($it as $k => $v) { $out .= $k . '=' . ($v instanceof Item ? $v->name : $v) . ';'; }\n    return $out;\n}\n$bag = new Bag([new Item('a'), new Item('b')]);\necho walk($bag), walk($bag), \"\\n\";\nforeach ($bag as $k => $item) { echo $k, ':', $item->name, ' '; }\necho \"\\n\", walk(new Words()), \"\\n\";\n$kept = [];\nforeach ($bag as $item) { $kept[] = $item; }\nforeach (new Words() as $w) { $kept[] = $w; }\necho count($kept), ' ', $kept[0]->name, ' ', $kept[4], \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "k0=a;k1=b;k0=a;k1=b;\nk0:a k1:b \n0=w;10=ww;20=www;\n5 a www\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Laravel's `ValidatesAttributes::validateMacAddress()`: `filter_var($v, FILTER_VALIDATE_MAC)`
/// was refused as an unsupported filter (276); php's `separator` option and its ValueError too.
#[test]
fn test_filter_validate_mac() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfunction validateMacAddress(mixed $value): bool {\n    return filter_var($value, FILTER_VALIDATE_MAC) !== false;\n}\nforeach (['01:23:45:67:89:ab', '01-23-45-67-89-AB', '0123.4567.89ab', '01:23:45:67:89', 'zz:23:45:67:89:ab', '', 12] as $m) {\n    var_dump(validateMacAddress($m));\n}\nvar_dump(filter_var('01:23:45:67:89:ab', FILTER_VALIDATE_MAC, ['options' => ['separator' => '-']]));\nvar_dump(filter_var('01-23-45-67-89-ab', FILTER_VALIDATE_MAC, ['options' => ['separator' => '-']]));\nvar_dump(filter_var('nope', FILTER_VALIDATE_MAC, FILTER_NULL_ON_FAILURE));\ntry {\n    filter_var('01:23:45:67:89:ab', FILTER_VALIDATE_MAC, ['options' => ['separator' => '::']]);\n} catch (ValueError $e) {\n    echo get_class($e), ': ', $e->getMessage(), \"\\n\";\n}\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nstring(17) \"01-23-45-67-89-ab\"\nNULL\nValueError: filter_var(): \"separator\" option must be one character long\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Laravel's `Console\Signals`: `(fn () => $this->signalHandlers = $handlers)->call($this->registry)`
/// reads and writes a private property of the registry. The write was refused as undefined on
/// `Signals`, and the read checked visibility against `Signals` instead of the bound scope.
#[test]
fn test_closure_call_reaches_the_bound_objects_private_property() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfinal class Registry {\n    private array $signalHandlers = [];\n    public function add(int $signal, callable $handler): void { $this->signalHandlers[$signal][] = $handler; }\n    public function count(): int { return count($this->signalHandlers); }\n}\nclass Signals {\n    public function __construct(protected Registry $registry) {}\n    public function getHandlers(): array {\n        return (fn () => $this->signalHandlers)->call($this->registry);\n    }\n    public function setHandlers(array $handlers): void {\n        (fn () => $this->signalHandlers = $handlers)->call($this->registry);\n    }\n}\n$r = new Registry();\n$r->add(15, fn () => null);\n$s = new Signals($r);\n$h = $s->getHandlers();\nvar_dump(array_keys($h), $r->count());\n$s->setHandlers([2 => [], 9 => []]);\nvar_dump(array_keys($s->getHandlers()), $r->count());\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "array(1) {\n  [0]=>\n  int(15)\n}\nint(1)\narray(2) {\n  [0]=>\n  int(2)\n  [1]=>\n  int(9)\n}\nint(2)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Laravel's `FormRequest::getValidatorInstance()` calls `$this->validator(...)` behind
/// `method_exists($this, 'validator')` (a first-class callable only an application request
/// declares), and `PendingRequest` calls `$e->hasResponse()` behind
/// `is_callable([$e, 'hasResponse'])` (a Guzzle 7 method Guzzle 8 dropped). Both were refused.
#[test]
fn test_capability_guarded_subclass_method_calls() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Container {\n    public function call(callable $callback, array $args): mixed { return $callback(...array_values($args)); }\n}\nclass FormRequest {\n    public function __construct(protected Container $container) {}\n    public function validate(): string {\n        if (method_exists($this, 'validator')) {\n            return $this->container->call($this->validator(...), ['factory' => 'F']);\n        }\n        return 'default';\n    }\n}\nclass StoreUser extends FormRequest {\n    public function validator(string $factory): string { return 'custom:' . $factory; }\n}\nclass TransferException extends RuntimeException {}\nclass RequestException extends TransferException {\n    public function getResponse(): string { return 'resp'; }\n}\nclass OldRequestException extends RequestException {\n    public function hasResponse(): bool { return true; }\n}\nfunction responseOf(Throwable $e): ?string {\n    if ($e instanceof RequestException && is_callable([$e, 'hasResponse']) && $e->hasResponse()) {\n        return $e->getResponse();\n    }\n    return null;\n}\n$c = new Container();\necho (new FormRequest($c))->validate(), ' ', (new StoreUser($c))->validate(), \"\\n\";\nvar_dump(responseOf(new RequestException('x')), responseOf(new OldRequestException('y')), responseOf(new LogicException('z')));\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "default custom:F\nNULL\nstring(4) \"resp\"\nNULL\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Laravel's `Cloud\QueueConnector` indexes `$command['QueueUrl']` on the AWS SDK's
/// `CommandInterface` inside a nested closure; the skeleton does not install the SDK, and the
/// captured receiver's undeclared type was refused as "Cannot index non-array".
#[test]
fn test_index_on_a_type_no_package_declares() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nuse Aws\\CommandInterface;\nuse Psr\\Http\\Message\\RequestInterface;\nfinal class Queue { public function normalizeQueue(?string $q): string { return 'n:' . $q; } }\nfinal class Connector {\n    public function register(object $list, Queue $queue): void {\n        $list->appendSign(function (callable $handler) use ($queue) {\n            return function (CommandInterface $command, RequestInterface $request) use ($handler, $queue) {\n                return $handler($command, $request)->otherwise(function ($reason) use ($command, $queue) {\n                    $name = $queue->normalizeQueue($command['QueueUrl'] ?? null);\n                    throw new RuntimeException(\"Managed queue [{$name}] does not exist.\", 0, $reason);\n                });\n            };\n        });\n    }\n}\necho \"ok\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ok\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// voku/portable-ascii's `ASCII::normalize_msword()`: `static $C = ['orig' => [], 'replace' => []];`
/// is filled with `array_keys()`/`array_values()` on first use; refused as `cannot reassign
/// $C from array<string, array<never>> to array<string, array<mixed>>`.
#[test]
fn test_static_local_initialized_with_empty_arrays_widens() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfinal class Ascii {\n    private static array $maps = ['msword' => [\"\\u{2018}\" => \"'\", \"\\u{2019}\" => \"'\", \"\\u{201C}\" => '\"']];\n    public static function normalize(string $str): string {\n        if ($str === '') {\n            return '';\n        }\n        static $CACHE = ['orig' => [], 'replace' => []];\n        if (empty($CACHE['orig'])) {\n            $map = self::$maps['msword'] ?? [];\n            $CACHE = [\n                'orig' => array_keys($map),\n                'replace' => array_values($map),\n            ];\n        }\n        return str_replace($CACHE['orig'], $CACHE['replace'], $str);\n    }\n}\necho Ascii::normalize(\"\\u{2018}hi\\u{2019} \\u{201C}x\"), \"|\", Ascii::normalize(\"\\u{2019}again\"), \"|\", Ascii::normalize(''), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "'hi' \"x|'again|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// league/commonmark's `DelimiterStack::processDelimiters()`: `is_int($b) ? $b : self::getIndex($b)`
/// over callers passing an int and null. The int specialization's dead else branch was checked
/// with `$b: int` against `?DelimiterInterface` and refused.
#[test]
fn test_ternary_branch_its_guard_rules_out_is_not_refused() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ninterface DelimiterInterface { public function getIndex(): ?int; }\nfinal class Delimiter implements DelimiterInterface {\n    public function __construct(private ?int $index) {}\n    public function getIndex(): ?int { return $this->index; }\n}\nfinal class DelimiterStack {\n    public function processDelimiters($stackBottom): int {\n        $stackBottomPosition = \\is_int($stackBottom) ? $stackBottom : self::getIndex($stackBottom);\n        return $stackBottomPosition;\n    }\n    private function getIndex(?DelimiterInterface $delimiter): int {\n        return $delimiter?->getIndex() ?? -1;\n    }\n}\n$s = new DelimiterStack();\nvar_dump($s->processDelimiters(3), $s->processDelimiters(null));\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "int(3)\nint(-1)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// brick/math's `BigInteger::fromBytes()`: `$bytes = ~$bytes;` complements every byte of a
/// string; refused as "Bitwise NOT requires integer operand".
#[test]
fn test_bitwise_not_of_a_string() {
    let out = compile_and_run_with_heap_debug(
        "<?php\n// brick/math BigInteger::fromBytes() two's-complement path, reduced.\nfunction fromBytes(string $value, bool $signed = true): string {\n    $twosComplement = false;\n    if ($signed) {\n        $x = ord($value[0]);\n        if (($twosComplement = ($x >= 0x80))) {\n            $value = ~$value;\n        }\n    }\n    $hex = bin2hex($value);\n    return ($twosComplement ? '-' : '') . $hex;\n}\necho fromBytes(\"\\x7f\\x01\"), ' ', fromBytes(\"\\xff\\xfe\"), ' ', fromBytes(\"\\x80\\x00\\x10\"), \"\\n\";\nvar_dump(bin2hex(~\"abc\"), ~5, ~-1);\n$s = str_repeat(\"\\x0f\", 3);\necho bin2hex(~$s), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7f01 -0001 -7fffef\nstring(6) \"9e9d9c\"\nint(-6)\nint(0)\nf0f0f0\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// guzzle's `FnStream` (`#[\AllowDynamicProperties]`) stores its `_fn_*` callables as dynamic
/// properties behind a throwing `__get(): void`. php reads an existing dynamic property without
/// calling `__get`; elephc typed the read `void` (refused) and then dispatched straight to `__get`.
#[test]
fn test_dynamic_property_read_before_magic_get() {
    let out = compile_and_run_with_heap_debug(
        "<?php\n#[\\AllowDynamicProperties]\nfinal class FnStream {\n    private array $methods;\n    public function __construct(array $methods) {\n        $this->methods = $methods;\n        foreach ($methods as $name => $fn) {\n            $this->{'_fn_' . $name} = $fn;\n        }\n    }\n    public function __get(string $name): void {\n        throw new BadMethodCallException(str_replace('_fn_', '', $name) . '() is not implemented in the FnStream');\n    }\n    public function __toString(): string {\n        return ($this->_fn___toString)();\n    }\n    public function getSize(): ?int {\n        return ($this->_fn_getSize)();\n    }\n}\n$s = new FnStream(['__toString' => fn () => 'body', 'getSize' => fn () => 4]);\necho $s, ' ', $s->getSize(), \"\\n\";\n$t = new FnStream(['__toString' => fn () => 'x']);\ntry {\n    $t->getSize();\n} catch (BadMethodCallException $e) {\n    echo get_class($e), ': ', $e->getMessage(), \"\\n\";\n}\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "body 4\nBadMethodCallException: getSize() is not implemented in the FnStream\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// guzzle's `Create::iterFor(iterable $value)` returns early for an `Iterator`, an array and an
/// `IteratorAggregate`, then calls `new \IteratorIterator($value)`: the `is_array()` complement of
/// `iterable` stayed `iterable` and was refused for the `Traversable` parameter.
#[test]
fn test_iterable_that_is_not_an_array_is_traversable() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfinal class Create {\n    public static function iterFor(iterable $value): \\Iterator {\n        if ($value instanceof \\Iterator) {\n            return $value;\n        }\n        if (is_array($value)) {\n            return new \\ArrayIterator($value);\n        }\n        if ($value instanceof \\IteratorAggregate) {\n            return self::iterFor($value->getIterator());\n        }\n        return new \\IteratorIterator($value);\n    }\n}\nfunction gen() { yield 'g' => 1; }\nfinal class Agg implements IteratorAggregate {\n    public function getIterator(): Iterator { return new ArrayIterator(['a' => 2]); }\n}\nforeach ([[3, 4], gen(), new Agg(), new ArrayIterator(['x' => 5])] as $source) {\n    foreach (Create::iterFor($source) as $k => $v) { echo $k, '=', $v, ' '; }\n}\necho \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0=3 1=4 g=1 a=2 x=5 \n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Carbon's `TranslationOptions::TRANSLATE_ALL = self::TRANSLATE_MONTHS | …`, read as the default
/// of a `Localization` trait method on `CarbonInterval` (which does not implement the interface):
/// interface initializers kept a raw `self::`, resolved against the reading class.
#[test]
fn test_interface_constant_initializer_self_names_the_interface() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ninterface TranslationOptions {\n    public const TRANSLATE_MONTHS = 1;\n    public const TRANSLATE_DAYS = 2;\n    public const TRANSLATE_ALL = self::TRANSLATE_MONTHS | self::TRANSLATE_DAYS;\n}\ninterface CarbonInterface extends TranslationOptions {}\ntrait Localization {\n    public static function translateTimeString(string $s, int $mode = CarbonInterface::TRANSLATE_ALL): string {\n        return $s . ':' . $mode;\n    }\n}\nclass CarbonInterval extends DateInterval {\n    use Localization;\n}\nclass Plain { use Localization; }\necho CarbonInterval::translateTimeString('a'), ' ', Plain::translateTimeString('b'), ' ', Plain::translateTimeString('c', 1), ' ', CarbonInterface::TRANSLATE_ALL, \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "a:3 b:3 c:1 3\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `__call()` reached through a `mixed`/`object` receiver (and so through `__elephc_magic_call()`)
/// got its `$args` array with the caller's element layout while the body was specialized to
/// another one: `$args` printed raw words. Pre-existing since at least c352.
#[test]
fn test_magic_call_args_through_untyped_receivers() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Plain {\n    public function __call(string $method, array $args): mixed { return $method . ':' . json_encode($args); }\n}\nfunction viaMixed(mixed $o) { return $o->anything(4, 'x'); }\nfunction viaObject(object $o) { return $o->other(5); }\nfunction viaTyped(Plain $o) { return $o->typed(6); }\necho viaMixed(new Plain()), ' ', viaObject(new Plain()), ' ', viaTyped(new Plain()), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "anything:[4,\"x\"] other:[5] typed:[6]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Carbon's `Modifiers::nthOfQuarter()`: `$this->avoidMutation()->day(1)->month(...)`, each link a
/// `__call()` on the previous link's `mixed` result, answered the current date (the `$args`
/// layout above), and the chain's `->month` reads go through `__get`.
#[test]
fn test_carbon_nth_of_quarter_through_magic_setters() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ntrait Modifiers {\n    public function firstOfQuarter($dayOfWeek = null) {\n        return $this->setDate($this->year, $this->quarter * static::MONTHS_PER_QUARTER - 2, 1);\n    }\n    #[\\ReturnTypeWillChange]\n    public function modify($modify) {\n        return parent::modify((string) $modify)\n            ?: throw new InvalidArgumentException('Could not modify with: ' . var_export($modify, true));\n    }\n    public function nthOfQuarter($nth, $dayOfWeek) {\n        $date = $this->avoidMutation()->day(1)->month($this->quarter * static::MONTHS_PER_QUARTER);\n        $lastMonth = $date->month;\n        $year = $date->year;\n        $date = $date->firstOfQuarter()->modify('+' . $nth . ' days');\n        return ($lastMonth < $date->month || $year !== $date->year) ? false : $this->modify((string) $date->format('Y-m-d'));\n    }\n}\nclass Moment extends DateTime {\n    use Modifiers;\n    public const MONTHS_PER_QUARTER = 3;\n    public function avoidMutation(): static { return clone $this; }\n    public function __get(string $name): mixed {\n        return match ($name) {\n            'month' => (int) $this->format('n'),\n            'year' => (int) $this->format('Y'),\n            'quarter' => intdiv((int) $this->format('n') - 1, 3) + 1,\n            default => null,\n        };\n    }\n    public function __call(string $method, array $args): mixed {\n        $y = (int) $this->format('Y'); $m = (int) $this->format('n'); $d = (int) $this->format('j');\n        return match ($method) {\n            'day' => $this->setDate($y, $m, $args[0]),\n            'month' => $this->setDate($y, $args[0], $d),\n            default => null,\n        };\n    }\n}\n$m = new Moment('2024-05-17');\nvar_dump($m->nthOfQuarter(2, 1)->format('Y-m-d'), (new Moment('2024-05-17'))->nthOfQuarter(200, 1));\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "string(10) \"2024-04-03\"\nbool(false)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `DatePeriod::__serialize()`/`__unserialize()`, which Carbon's `CarbonPeriod` uses to re-seed
/// itself (`parent::__unserialize($native->__serialize())`): undefined before, so
/// `CarbonPeriod.php` was deferred.
#[test]
fn test_date_period_serialize_and_reseed() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ndate_default_timezone_set('UTC');\nfunction show(array $s): string {\n    $out = [];\n    foreach ($s as $k => $v) {\n        $out[] = $k . '=' . ($v instanceof DateTimeInterface ? get_class($v) . '@' . $v->format('Y-m-d H:i') : ($v instanceof DateInterval ? 'P' . $v->d . 'D' . $v->h . 'H' : var_export($v, true)));\n    }\n    return implode(' ', $out);\n}\n$p = new DatePeriod(new DateTimeImmutable('2024-01-01'), new DateInterval('P1D'), new DateTime('2024-01-04'), DatePeriod::EXCLUDE_START_DATE);\necho show($p->__serialize()), \"\\n\";\n$q = new DatePeriod(new DateTime('2024-01-01'), new DateInterval('PT12H'), 3);\necho show($q->__serialize()), \"\\n\";\nforeach ($q as $d) {}\necho show($q->__serialize()), \"\\n\";\nclass Reseeded extends DatePeriod {\n    public function reseed(DatePeriod $native): void { parent::__unserialize($native->__serialize()); }\n}\n$r = new Reseeded(new DateTime('2020-01-01'), new DateInterval('P1D'), 1);\n$r->reseed($p);\necho $r->start->format('Y-m-d'), ' ', $r->end->format('Y-m-d'), ' ', $r->recurrences, ' ', var_export($r->include_start_date, true), ' ', get_class($r->start), \"\\n\";\nforeach ($r as $d) { echo $d->format('md'), ','; }\necho \"\\n\";\n$f = new Reseeded(new DateTime('2020-01-01'), new DateInterval('P1D'), 1);\n$f->reseed(new DatePeriod(new DateTime('2024-03-01'), new DateInterval('P2D'), 2, DatePeriod::EXCLUDE_START_DATE));\nforeach ($f as $d) { echo $d->format('md'), ','; }\necho ' ', $f->recurrences, \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "start=DateTimeImmutable@2024-01-01 00:00 current=NULL end=DateTimeImmutable@2024-01-04 00:00 interval=P1D0H recurrences=0 include_start_date=false include_end_date=false\nstart=DateTime@2024-01-01 00:00 current=NULL end=NULL interval=P0D12H recurrences=4 include_start_date=true include_end_date=false\nstart=DateTime@2024-01-01 00:00 current=DateTime@2024-01-03 00:00 end=NULL interval=P0D12H recurrences=4 include_start_date=true include_end_date=false\n2024-01-01 2024-01-04 0 false DateTimeImmutable\n0102,0103,\n0303,0305, 2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Carbon's `Units::add()`: `$unit = Closure::fromCallable([$unit, 'convertDate'])`, then
/// `$unit(...)` in a loop. The checker re-read the receiver of the target it recorded by name
/// after the rebinding and refused "First-class method callable requires an object receiver".
/// Output only: the conditional rebinding still leaks one block (a pre-existing gap, tracked).
#[test]
fn test_call_through_a_variable_rebound_to_its_own_from_callable() {
    let out = compile_and_run(
        "<?php\ninterface Converter { public function convertDate(int $d, bool $negated = false): int; }\nfinal class Doubler implements Converter {\n    public function convertDate(int $d, bool $negated = false): int { return $negated ? -$d * 2 : $d * 2; }\n}\nfunction apply($unit, int $value): int {\n    if ($unit instanceof Converter) {\n        $unit = Closure::fromCallable([$unit, 'convertDate']);\n    }\n    if ($unit instanceof Closure) {\n        $result = $value;\n        for ($i = 0; $i < 2; $i++) {\n            $result = $unit($result, false);\n        }\n        return $result;\n    }\n    return -1;\n}\nvar_dump(apply(new Doubler(), 3), apply(fn (int $d, bool $n) => $d + 1, 3), apply('x', 3));\n",
    );
    assert_eq!(out, "int(12)\nint(5)\nint(-1)\n");
}

/// php's `/` keeps an exact quotient of integer operands an `int` (`3600 / 60 / 60` is `int(1)`;
/// Carbon's `offset` setter passes it to `safeCreateDateTimeZone(DateTimeZone|string|int|…)` in
/// strict mode). elephc divided in floating point always, so `var_dump()`, `is_int()` and
/// `json_encode()` saw `float(1)` / `1.0`, and the strict parameter refused the call. Typed ints,
/// boxed and string operands, `/=`, `PHP_INT_MIN / -1` and the zero divisor are all php's.
#[test]
fn test_exact_integer_division_stays_an_int() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfunction q(mixed $a, mixed $b): mixed { return $a / $b; }\n$cases = [[6, 3], [7, 2], [\"6\", \"3\"], [\"6\", 4], [\"1.5\", 3], [6.0, 3], [true, 1], [null, 5], [-9, 3], [-9, 2], [0, 7], [PHP_INT_MAX, 1], [10, -5]];\nforeach ($cases as [$a, $b]) {\n    echo json_encode(q($a, $b)), ' ';\n}\necho \"\\n\";\n$total = 0;\nforeach ([2, 4, 6] as $v) { $total += $v; }\n$avg = $total / 3;\nvar_dump($avg, round($total / 4, 1), intdiv(7, 2), 7 % 3);\n$x = 100;\n$x /= 4;\nvar_dump($x);\n$x /= 3;\nvar_dump($x);\n$seconds = 7200;\nprintf(\"%d hours, %.2f days\\n\", $seconds / 3600, $seconds / 86400);\necho json_encode(['h' => $seconds / 3600, 'r' => 10 / 4]), \"\\n\";\ntry { echo q(1, \"0\"); } catch (DivisionByZeroError $e) { echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }\ntry { $z = 0; echo 5 / $z; } catch (DivisionByZeroError $e) { echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2 3.5 2 1.5 0.5 2 1 0 -3 -4.5 0 9223372036854775807 -2 \nint(4)\nfloat(3)\nint(3)\nint(1)\nint(25)\nfloat(8.333333333333334)\n2 hours, 0.08 days\n{\"h\":2,\"r\":2.5}\nDivisionByZeroError: Division by zero\nDivisionByZeroError: Division by zero\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `passthru($command, $exitCode)` (Laravel's `DevCommand::runViaMultiplex()`): the contract had no
/// `&$result_code`, so `$exitCode` was an undefined variable and the file was deferred. The
/// two-argument form runs through the exec prelude family and stores the exit status.
#[test]
fn test_passthru_stores_the_result_code() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfunction run(string $command): int {\n    passthru($command, $exitCode);\n    return $exitCode;\n}\necho \"[\", run('printf \"out-1\"'), \"]\\n\";\necho \"[\", run('exit 3'), \"]\\n\";\n$r = passthru('printf \"two\"', $code);\nvar_dump($r, $code);\npassthru('printf \"one-arg\\n\"');\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[out-10]\n[3]\ntwoNULL\nint(0)\none-arg\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An offset zone such as `+02:00` lies east of UTC, while a POSIX TZ string counts
/// west of it: the runtime hands libc a sign-flipped copy so the zone keeps its php offset.
#[test]
fn test_offset_timezone_keeps_its_sign() {
    let out = compile_and_run_with_heap_debug(
        "<?php\n$a = new DateTime(\"2024-01-01 12:00:00\", new DateTimeZone(\"UTC\"));\necho $a->setTimezone(new DateTimeZone(\"+02:00\"))->format(\"H:i P\"), \" \", $a->getTimestamp(), \" \", (new DateTime(\"2024-01-01 12:00:00\", new DateTimeZone(\"+02:00\")))->format(\"H:i P U\"), \" \", (new DateTimeZone(\"+02:00\"))->getOffset(new DateTime(\"2024-01-01\", new DateTimeZone(\"UTC\"))), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "14:00 +02:00 1704110400 12:00 +02:00 1704103200 7200\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A gradual receiver may be a class that answers the method through `__call`, so a same-named
/// declared method elsewhere in the world (Faker's `month(): string`) does not type the call:
/// Carbon's `->day(1)->month(...)` chain then read `$date->month` and was refused.
#[test]
fn test_gradual_call_answered_by_magic_call_is_not_typed_by_a_namesake() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfinal class Faker {\n    public function month($until = 'now'): string { return 'May'; }\n}\nfinal class Moment {\n    public int $n = 0;\n    public function __call(string $method, array $parameters): mixed {\n        $this->n = $parameters[0];\n        return $this;\n    }\n    public function pick(mixed $other): int {\n        $date = $other->month(3);\n        return $date->n;\n    }\n}\n$m = new Moment();\necho $m->pick($m), ' ', (new Faker())->month(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3 May\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A narrowed `$this` outlives the calls after it: no call can change the class of the running
/// body's object. Symfony's `CompiledUrlMatcherTrait::match()` returns `$this->redirect(...) + $ret`
/// after `if (!$this instanceof RedirectableUrlMatcherInterface) throw`, and the call purge dropped
/// the fact before the operands were re-inferred.
#[test]
fn test_narrowed_this_survives_the_calls_after_its_guard() {
    let out = compile_and_run_with_heap_debug(
        "<?php\ninterface Redirectable { public function redirect(string $path): array; }\ntrait MatcherTrait {\n    public function match(string $path): array {\n        if ($path === '/') {\n            return ['home'];\n        }\n        if (!$this instanceof Redirectable) {\n            throw new RuntimeException('none');\n        }\n        return $this->redirect($path) + array_merge($this->redirect($path . '/'), ['r' => 1]);\n    }\n}\nclass Matcher { use MatcherTrait; }\ntry { echo json_encode((new Matcher())->match('/a')); } catch (RuntimeException $e) { echo $e->getMessage(); }\necho json_encode((new Matcher())->match('/')), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "none[\"home\"]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A class is a stream filter because it extends `php_user_filter`, not because it has a method
/// named `filter`: in a world that registers a filter (Laravel's does), phpoption's
/// `Some::filter($callable)` kept the filter ABI's raw `int` seed, and `$callable($this->value)` was
/// refused. No heap check: `stream_filter_register()` keeps its registry for the process lifetime.
#[test]
fn test_a_method_named_filter_outside_php_user_filter_keeps_its_callable() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfinal class Upper extends php_user_filter {\n    public function filter($in, $out, &$consumed, $closing): int {\n        while ($bucket = stream_bucket_make_writeable($in)) {\n            $bucket->data = strtoupper($bucket->data);\n            $consumed += $bucket->datalen;\n            stream_bucket_append($out, $bucket);\n        }\n        return PSFS_PASS_ON;\n    }\n}\nfinal class Some {\n    public function __construct(private mixed $value) {}\n    public function filter($callable) {\n        if (true === $callable($this->value)) {\n            return $this;\n        }\n        return null;\n    }\n}\nstream_filter_register('upper', Upper::class);\nfunction keep(mixed $o): string { return $o->filter(fn ($v) => $v === 4) ? 'kept' : 'dropped'; }\necho keep(new Some(4)), ' ', keep(new Some(5)), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "kept dropped\n");
}

/// php 8.4's `BcMath\Number`, declared over the `bc*` functions, with its operator overloads: arithmetic
/// and comparisons on a `Number` operand are the prelude's static helper calls. sebastian/comparator's
/// `NumberComparator` (in the Laravel classmap) computes `$actual < $expected - $deltaNumber`.
#[test]
fn test_bcmath_number_and_its_operator_overloads() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nuse BcMath\\Number;\nfunction within(mixed $expected, mixed $actual, float $delta): string {\n    if (!$expected instanceof Number) {\n        $expected = new Number($expected);\n    }\n    if (!$actual instanceof Number) {\n        $actual = new Number($actual);\n    }\n    $deltaNumber = new Number(number_format($delta, max($expected->scale, $actual->scale)));\n    return ($actual < $expected - $deltaNumber || $actual > $expected + $deltaNumber) ? 'out' : 'in';\n}\necho within('1.50', '1.52', 0.05), ' ', within(new Number('1.50'), '1.60', 0.05), ' ', within(3, new Number('3.0'), 0.0), \"\\n\";\nforeach (['0', '1.50', '-2.250', '007', '.5', '5.', '1e3', '', '+3', '-0.0'] as $raw) {\n    try {\n        $n = new Number($raw);\n        echo json_encode($raw), ' => ', $n->value, ' s', $n->scale, \"\\n\";\n    } catch (ValueError $e) {\n        echo json_encode($raw), ' !! ', $e->getMessage(), \"\\n\";\n    }\n}\n$a = new Number('1.52');\n$b = new Number('0.3');\necho $a->add($b), ' ', $a->sub($b), ' ', $a->mul($b), ' ', $a->div($b), ' ', $a->mod($b), ' ', $a->pow(3), \"\\n\";\necho $a->add($b, 5), ' ', $a->div($b, 3), ' ', $a->mul('2.5'), ' ', $a->add(7), ' ', $a->compare($b), ' ', $b->compare($a), \"\\n\";\necho (new Number('10'))->div('3'), ' ', (new Number('1'))->div('8'), ' ', (new Number('10.0'))->div('2'), ' ', (new Number('2'))->sqrt(), ' ', (new Number('4.00'))->sqrt(), \"\\n\";\necho (new Number('2.5'))->round(), ' ', (new Number('1.5'))->round(3), ' ', (new Number('-2.5'))->floor(), ' ', (new Number('2.1'))->ceil(), ' ', (new Number('2'))->pow(-2), \"\\n\";\necho $a + $b, ' ', $a - 1, ' ', 2 * $a, ' ', '3' / $b, ' ', $a ** 2, ' ', -$a, ' ', $a % '0.5', \"\\n\";\nvar_dump($a > $b, $a == '1.52', $a <=> 2, $a instanceof Stringable, (string) $a, json_encode($a), $a < 'x');\nforeach ([fn () => $a->div('0'), fn () => $a + 'x', fn () => $a + [], fn () => $a->pow('1.5')] as $bad) {\n    try {\n        $r = $bad();\n        echo 'no error ', $r, \"\\n\";\n    } catch (Throwable $e) {\n        echo get_class($e), ': ', $e->getMessage(), \"\\n\";\n    }\n}\n$s = serialize($a);\necho $s, ' ', unserialize($s)->value, ' ', unserialize($s)->scale, \"\\n\";\n[$q, $r] = (new Number('5'))->divmod('3');\necho $q, ',', $r, ' ', (new Number('4'))->powmod('3', '5'), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "in out in\n\"0\" => 0 s0\n\"1.50\" => 1.50 s2\n\"-2.250\" => -2.250 s3\n\"007\" => 7 s0\n\".5\" => 0.5 s1\n\"5.\" => 5 s0\n\"1e3\" !! BcMath\\Number::__construct(): Argument #1 ($num) is not well-formed\n\"\" => 0 s0\n\"+3\" => 3 s0\n\"-0.0\" => 0.0 s1\n1.82 1.22 0.456 5.066666666666 0.02 3.511808\n1.82000 5.066 3.800 8.52 1 -1\n3.3333333333 0.125 5.0 1.4142135623 2.00\n3 1.500 -3 3 0.25\n1.82 0.52 3.04 10 2.3104 -1.52 0.02\nbool(true)\nbool(true)\nint(-1)\nbool(true)\nstring(4) \"1.52\"\nstring(26) \"{\"value\":\"1.52\",\"scale\":2}\"\nbool(false)\nDivisionByZeroError: Division by zero\nValueError: Right string operand cannot be converted to BcMath\\Number\nTypeError: Unsupported operand types: BcMath\\Number + array\nValueError: BcMath\\Number::pow(): Argument #1 ($exponent) exponent cannot have a fractional part\nO:13:\"BcMath\\Number\":1:{s:5:\"value\";s:4:\"1.52\";} 1.52 2\n1,2 4\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `ReflectionProperty::setValue($value)`, php 8.3's deprecated one-argument form, sets a static
/// property: psysh's `Sudo::assignStaticProperty()` calls it that way, and the call was refused
/// ("expects 2 arguments, got 1"). The deprecation notice itself is not raised (suppressed here).
#[test]
fn test_reflection_property_set_value_single_argument_sets_the_static() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass A { public int $i = 0; private static int $y = 2; public static function y(): int { return self::$y; } }\n$a = new A();\n$r = new ReflectionProperty(A::class, \"i\"); $r->setValue($a, 4); echo $a->i, \"\\n\";\n$p = new ReflectionProperty(A::class, \"y\"); $p->setValue(null, 9); echo A::y(), \"\\n\";\n@$p->setValue(12); echo A::y(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "4\n9\n12\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$object->{$name} = &$this->prop` binds the runtime-named property to the property's reference,
/// through the element-marker route array literals use: laravel/serializable-closure's
/// `Native::mapPointers()` does it, and the parser refused any source but a plain variable.
#[test]
fn test_runtime_named_property_reference_to_a_property() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfinal class Holder {\n    public $closure = 'first';\n    public array $list = ['a' => 1];\n    public function map(\\stdClass $data, string $key): void {\n        $data->{$key} = &$this->closure;\n    }\n}\n$h = new Holder();\n$d = new \\stdClass();\n$h->map($d, 'k');\n$h->closure = 'second';\necho $d->k, \"\\n\";\n$d->k = 'third';\necho $h->closure, \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "second\nthird\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A DateTime setter returns `static`, and an immutable one clones its receiver: a subclass keeps its
/// class, its own properties and its `__clone`. Carbon's `setTimestamp()` chains
/// `parent::setTimestamp(...)->setMicroseconds(...)` through its `__call`, which a `DateTime` result
/// refused ("Undefined method: DateTime::setMicroseconds"), and `CarbonImmutable` became a bare
/// `DateTimeImmutable` after every modification.
#[test]
fn test_datetime_setters_keep_the_subclass() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Moment extends DateTimeImmutable {\n    public int $clones = 0;\n    public function __clone() { $this->clones++; }\n    public function __call(string $method, array $args): mixed {\n        if ($method === 'setMicroseconds') {\n            return $this->setMicrosecond($args[0]);\n        }\n        throw new BadMethodCallException($method);\n    }\n    public function setTimestamp(int $timestamp): static {\n        return parent::setTimestamp($timestamp)->setMicroseconds(250);\n    }\n}\nclass Mutable extends DateTime {\n    public function setTimestamp(int $timestamp): static {\n        return parent::setTimestamp($timestamp)->setMicrosecond(7);\n    }\n}\n$m = new Moment('2024-01-01 00:00:00', new DateTimeZone('UTC'));\n$n = $m->setTimestamp(86400);\necho get_class($n), ' ', $n->format('Y-m-d H:i:s.u'), ' ', $n->clones, ' ', $m->format('u'), \"\\n\";\necho get_class($m->modify('+1 day')), ' ', get_class($m->setTime(1, 2)), ' ', get_class($m->setDate(2020, 1, 2)), \"\\n\";\n$u = new Mutable('2024-01-01 00:00:00', new DateTimeZone('UTC'));\n$v = $u->setTimestamp(3600);\necho get_class($v), ' ', $v === $u ? 'same' : 'other', ' ', $u->format('H:i:s.u'), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Moment 1970-01-02 00:00:00.000250 2 000000\nMoment Moment Moment\nMutable same 01:00:00.000007\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The length forms of `fgets()` / `fwrite()` / `fputs()` and the `$context` forms of `file()`,
/// `rename()`, `opendir()`, `unlink()` and `rmdir()` compile (psysh's Hoa `File*` classes and every
/// `fgets($h, 4096)` line reader were refused for their arity), and `file()` frees the payload it
/// split into lines: every call leaked it.
#[test]
fn test_stream_builtin_length_and_context_forms() {
    let out = compile_and_run_with_heap_debug(
        "<?php\n$h = fopen('php://memory', 'w+');\nvar_dump(fwrite($h, 'abcdef', 2), fwrite($h, 'xyz', 0), fwrite($h, 'q', -1), fputs($h, \"line1\\nline2\\n\", 100));\nrewind($h);\nvar_dump(fgets($h, 3), fgets($h, 1), fgets($h, 100), fgets($h, 4), fgets($h, 100), fgets($h, 100));\ntry {\n    fgets($h, 0);\n} catch (ValueError $e) {\n    echo get_class($e), ': ', $e->getMessage(), \"\\n\";\n}\n$dir = sys_get_temp_dir() . '/elephc_stream_args_' . getmypid();\nmkdir($dir);\nfile_put_contents($dir . '/a.txt', \"one\\ntwo\\n\");\n$context = stream_context_create();\nvar_dump(file($dir . '/a.txt', FILE_IGNORE_NEW_LINES, $context));\nvar_dump(rename($dir . '/a.txt', $dir . '/b.txt', $context));\n$d = opendir($dir, $context);\n$names = [];\nwhile (($entry = readdir($d)) !== false) {\n    if ($entry !== '.' && $entry !== '..') {\n        $names[] = $entry;\n    }\n}\nclosedir($d);\nvar_dump($names, unlink($dir . '/b.txt', $context), rmdir($dir, $context), is_dir($dir));\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "int(2)\nint(0)\nint(0)\nint(12)\nstring(2) \"ab\"\nbool(false)\nstring(6) \"line1\n\"\nstring(3) \"lin\"\nstring(3) \"e2\n\"\nbool(false)\nValueError: fgets(): Argument #2 ($length) must be greater than 0\narray(2) {\n  [0]=>\n  string(3) \"one\"\n  [1]=>\n  string(3) \"two\"\n}\nbool(true)\narray(1) {\n  [0]=>\n  string(5) \"b.txt\"\n}\nbool(true)\nbool(true)\nbool(false)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$this[] = $v` and `$this[$k] = $v` inside an `ArrayAccess` class are php's
/// `$this->offsetSet(null, $v)` / `$this->offsetSet($k, $v)`: psysh's Hoa `ProtocolNode` appends its
/// children that way, and the parser refused the target ("Invalid assignment target").
#[test]
fn test_dimension_writes_on_this_call_offset_set() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Node implements ArrayAccess {\n    private array $children = [];\n    public function __construct(array $children = []) { foreach ($children as $child) { $this[] = $child; } $this[\"root\"] = \"r\"; }\n    public function offsetExists(mixed $o): bool { return isset($this->children[$o]); }\n    public function offsetGet(mixed $o): mixed { return $this->children[$o]; }\n    public function offsetSet(mixed $o, mixed $v): void { if ($o === null) { $this->children[] = $v; } else { $this->children[$o] = $v; } }\n    public function offsetUnset(mixed $o): void { unset($this->children[$o]); }\n    public function all(): array { return $this->children; }\n}\n$n = new Node([\"a\", \"b\"]);\nvar_dump($n->all(), $n[1], isset($n[\"root\"]));\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "array(3) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n  [\"root\"]=>\n  string(1) \"r\"\n}\nstring(1) \"b\"\nbool(true)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `\Reflector`-typed value reads the `name` / `class` virtual property its Reflection
/// implementation carries: psysh's `SignatureFormatter` switches on `get_class($reflector)` and reads
/// `$reflector->class`, which was refused ("Undefined property: Reflector::class").
#[test]
fn test_reflector_reads_its_implementations_virtual_properties() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfunction describe(\\Reflector $reflector): string {\n    switch (\\get_class($reflector)) {\n        case \\ReflectionFunction::class: return \"fn \" . $reflector->name;\n        case \\ReflectionMethod::class: return \"m \" . $reflector->class . \".\" . $reflector->name;\n        default: return \"other\";\n    }\n}\nclass K { public function go() {} }\nfunction f() {}\necho describe(new ReflectionMethod(\"K\", \"go\")), \" \", describe(new ReflectionFunction(\"f\")), \" \", describe(new ReflectionClass(\"K\")), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "m K.go fn f other\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A write to a property the receiver's static class lacks but a descendant declares is resolved
/// against the run-time class, as the read already was: psysh's `IteratorRecursiveDirectory` writes
/// `$out->_relativePath` on the `RecursiveDirectoryIterator` its `parent::getChildren()` is declared to
/// return (a `new static`), and the write was refused ("Undefined property").
#[test]
fn test_write_to_a_property_only_a_descendant_declares() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Base { public function child(): Base { return new static(); } }\nclass Derived extends Base {\n    protected $tag = \"root\";\n    public function spawn(): string {\n        $out = $this->child();\n        $out->tag = \"child of \" . $this->tag;\n        return $out->describe();\n    }\n    public function describe(): string { return $this->tag; }\n}\necho (new Derived())->spawn(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "child of root\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// php 8.5 attributes on global constants (`#[Deprecated] const X = 1;`) and property types that
/// open with a keyword member (`private null|string $value;`, phpunit's metadata classes).
#[test]
fn test_constant_attributes_and_keyword_led_property_types() {
    let out = compile_and_run_with_heap_debug(
        "<?php\n#[Deprecated(message: 'use B instead')]\nconst A = 4;\nconst B = 5;\nfinal readonly class W {\n    private null|string $value;\n    public function __construct(null|string $v) { $this->value = $v; }\n    public function yes(): true { return true; }\n    public function v(): null|string { return $this->value; }\n}\n$w = new W('x');\nvar_dump(B, $w->yes(), $w->v());\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "int(5)\nbool(true)\nstring(1) \"x\"\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// FILTER_VALIDATE_URL / EMAIL / DOMAIN and FILTER_CALLBACK through `__elephc_filter_var_ext`
/// (Laravel's `UrlGenerator`, `ValidatesAttributes`, `FilterEmailValidation`, `ComponentSlot`):
/// refused as "filter N is not supported yet" before. The loop also pins the native filters'
/// release of their fresh boxed result inside a function: every call leaked two blocks.
#[test]
fn test_filter_var_url_email_domain_callback_match_php() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nforeach (['http://example.com', 'https://a.b/c?d=1#e', 'http://-bad.com', 'mailto:x@y.z', 'http://[::1]/', 'example.com', 'http://u:p^@h.com'] as $u) {\n    echo json_encode([filter_var($u, FILTER_VALIDATE_URL), filter_var($u, FILTER_VALIDATE_URL, FILTER_FLAG_PATH_REQUIRED)]), \"\\n\";\n}\necho json_encode([filter_var('x', FILTER_VALIDATE_URL, FILTER_NULL_ON_FAILURE), filter_var('x', FILTER_VALIDATE_URL, ['options' => ['default' => 'D']])]), \"\\n\";\nforeach (['a@b.co', '\"q\"@x.org', 'a@b', 'a@[127.0.0.1]'] as $m) {\n    echo json_encode([filter_var($m, FILTER_VALIDATE_EMAIL), filter_var($m, FILTER_VALIDATE_EMAIL, FILTER_FLAG_EMAIL_UNICODE)]), \"\\n\";\n}\nforeach (['example.com.', '-a.com', 'a_b.c', '1.5'] as $d) {\n    echo json_encode([filter_var($d, FILTER_VALIDATE_DOMAIN), filter_var($d, FILTER_VALIDATE_DOMAIN, FILTER_FLAG_HOSTNAME)]), \"\\n\";\n}\necho json_encode(filter_var(\"a<!-- c -->b \", FILTER_CALLBACK, ['options' => fn ($input) => trim(preg_replace(\"/<!--([\\s\\S]*?)-->/\", '', $input))])), \"\\n\";\necho json_encode(filter_var(['x', ['y']], FILTER_CALLBACK, ['options' => 'strtoupper'])), \"\\n\";\ntry { filter_var('x', FILTER_CALLBACK, ['options' => 'nope_fn']); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }\nfunction ip(string $s) { return filter_var($s, FILTER_VALIDATE_IP, FILTER_FLAG_IPV6); }\nfunction int(string $s) { return filter_var($s, FILTER_VALIDATE_INT); }\nfor ($i = 0; $i < 3; $i++) { echo json_encode([ip('::1'), int((string) $i)]), \"\\n\"; }\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[\"http:\\/\\/example.com\",false]\n[\"https:\\/\\/a.b\\/c?d=1#e\",\"https:\\/\\/a.b\\/c?d=1#e\"]\n[false,false]\n[\"mailto:x@y.z\",\"mailto:x@y.z\"]\n[\"http:\\/\\/[::1]\\/\",\"http:\\/\\/[::1]\\/\"]\n[false,false]\n[false,false]\n[null,\"D\"]\n[\"a@b.co\",\"a@b.co\"]\n[\"\\\"q\\\"@x.org\",\"\\\"q\\\"@x.org\"]\n[false,false]\n[\"a@[127.0.0.1]\",\"a@[127.0.0.1]\"]\n[\"example.com.\",\"example.com.\"]\n[\"-a.com\",false]\n[\"a_b.c\",false]\n[\"1.5\",\"1.5\"]\n\"ab\"\n[\"X\",[\"Y\"]]\nTypeError: filter_var(): Option must be a valid callback\n[\"::1\",0]\n[\"::1\",1]\n[\"::1\",2]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `foreach ($object as $key => &$value)` over a `stdClass` (laravel/serializable-closure's
/// `Native::mapPointers()`) walks its properties by reference, and the body may rebind the one it
/// visits (`$data->{$key} = &$this->closure`). The checker refused the walk outside the class's
/// declaring scope; it is checked and lowered as the loop over `get_object_vars()` names binding
/// `$value = &$data->{$key}`.
#[test]
fn test_by_reference_walk_of_a_stdclass() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass SelfReference { public function __construct(public string $hash) {} }\nclass Box {\n    public $closure = \"c\";\n    public function map(&$data): void {\n        if ($data instanceof \\stdClass) {\n            foreach ($data as $key => &$value) {\n                if ($value instanceof SelfReference && $value->hash === \"h\") {\n                    $data->{$key} = &$this->closure;\n                } elseif (is_int($value)) {\n                    $value = $value * 10;\n                } elseif (is_array($value) || is_object($value)) {\n                    $this->map($value);\n                }\n            }\n            unset($value);\n        }\n    }\n}\nfunction plain(stdClass $o): void {\n    foreach ($o as &$v) {\n        if (is_string($v)) { $v = strtoupper($v); }\n    }\n}\n$o = new stdClass();\n$o->a = 1;\n$o->b = new SelfReference(\"h\");\n$o->c = 3;\n$o->d = new stdClass();\n$o->d->x = 5;\n$b = new Box();\n$b->map($o);\nvar_dump($o);\n$b->closure = \"changed\";\nvar_dump($o->b);\n$p = new stdClass();\n$p->s = \"abc\";\n$p->n = 2;\nplain($p);\nvar_dump($p);\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "object(stdClass)#1 (4) {\n  [\"a\"]=>\n  int(10)\n  [\"b\"]=>\n  &string(1) \"c\"\n  [\"c\"]=>\n  int(30)\n  [\"d\"]=>\n  object(stdClass)#3 (1) {\n    [\"x\"]=>\n    int(50)\n  }\n}\nstring(7) \"changed\"\nobject(stdClass)#2 (2) {\n  [\"s\"]=>\n  string(3) \"ABC\"\n  [\"n\"]=>\n  int(2)\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A reference to a `stdClass` property, by runtime or literal name and from a typed or untyped
/// receiver, binds the entry of the instance's hash (it crashed or was refused by the backend),
/// and the alias keeps its own share when the property is rebound. A reference local narrowed by
/// `instanceof` reads its object out of the cell's box (`$v instanceof C && $v->p` was false).
#[test]
fn test_references_to_stdclass_properties_and_narrowed_reference_reads() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass SelfReference { public function __construct(public string $hash) {} }\nclass Box { public $closure = \"c\"; }\nfunction element(array $data) {\n    $value = &$data['b'];\n    var_dump($value instanceof SelfReference && $value->hash === \"h\");\n}\nfunction named($data) {\n    $key = $GLOBALS['argc'] > 5 ? 'x' : 'b';\n    $value = &$data->{$key};\n    var_dump($value instanceof SelfReference && $value->hash === \"h\");\n}\nfunction literal($data) {\n    $value = &$data->b;\n    var_dump($value instanceof SelfReference);\n    $value = \"via literal\";\n}\nfunction typed(stdClass $data, Box $box) {\n    $value = &$data->c;\n    $value = \"typed\";\n    $key = $GLOBALS['argc'] > 5 ? 'x' : 'c';\n    $data->{$key} = &$box->closure;\n    $value = \"old cell\";\n}\nelement(['b' => new SelfReference(\"h\")]);\n$o = new stdClass();\n$o->b = new SelfReference(\"h\");\nnamed($o);\nliteral($o);\n$box = new Box();\ntyped($o, $box);\n$box->closure = \"changed\";\nvar_dump($o);\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\nbool(true)\nobject(stdClass)#1 (2) {\n  [\"b\"]=>\n  string(11) \"via literal\"\n  [\"c\"]=>\n  &string(7) \"changed\"\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `chgrp()`/`chown()`/`lchgrp()`/`lchown()` take a `string|int` principal, and a gradual one
/// (psysh's Hoa `FileGeneric::changeGroup($group)`) is dispatched on its runtime tag: refused at
/// compile time before, and the contract's `string` cast every boxed principal to a name.
#[test]
fn test_gradual_file_principal_is_dispatched_on_its_runtime_type() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfunction change($file, $group) {\n    try {\n        var_dump(chgrp($file, $group));\n    } catch (TypeError $e) {\n        echo get_class($e), \": \", $e->getMessage(), \"\\n\";\n    }\n}\nfunction owner($file, $user) {\n    try {\n        var_dump(lchown($file, $user));\n    } catch (TypeError $e) {\n        echo $e->getMessage(), \"\\n\";\n    }\n}\n$f = sys_get_temp_dir() . '/elephc_principal_' . getmypid();\ntouch($f);\nchange($f, filegroup($f));\nchange($f, (float) filegroup($f));\nchange($f, [1]);\nowner($f, fileowner($f));\nowner($f, new stdClass());\nunlink($f);\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\nTypeError: chgrp(): Argument #2 ($group) must be of type string|int, array given\nbool(true)\nlchown(): Argument #2 ($user) must be of type string|int, stdClass given\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$this->cache = &self::$htmlCache` (Symfony's `CssSelectorConverter`) binds a declared property
/// to a static's storage: every instance reads and writes the one static array, and the parser
/// refused any static source. The static lives for the whole process, so the property holds its
/// cell without a share of it.
#[test]
fn test_property_bound_by_reference_to_a_static_property() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Converter {\n    public static int $maxCachedItems = 2;\n    private array $cache;\n    private static array $xmlCache = [];\n    private static array $htmlCache = [];\n    public function __construct(bool $html = true) {\n        if ($html) {\n            $this->cache = &self::$htmlCache;\n        } else {\n            $this->cache = &self::$xmlCache;\n        }\n    }\n    public function toXPath(string $css): string {\n        if (isset($this->cache[$css])) {\n            $value = $this->cache[$css];\n            unset($this->cache[$css]);\n            return $this->cache[$css] = $value;\n        }\n        if (\\count($this->cache) >= self::$maxCachedItems) {\n            $this->cache = \\array_slice($this->cache, self::$maxCachedItems >> 1);\n        }\n        return $this->cache[$css] = strtoupper($css);\n    }\n    public static function sizes(): string { return count(self::$htmlCache) . \"/\" . count(self::$xmlCache); }\n}\n$a = new Converter();\n$b = new Converter();\n$x = new Converter(false);\necho $a->toXPath(\"div\"), $b->toXPath(\"p\"), $a->toXPath(\"div\"), $x->toXPath(\"a\"), \"\\n\";\necho Converter::sizes(), \"\\n\";\necho $b->toXPath(\"span\"), \" \", Converter::sizes(), \"\\n\";\nunset($a, $b);\necho Converter::sizes(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "DIVPDIVA\n2/1\nSPAN 2/1\n2/1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `new IteratorIterator($iterator, $mode)` with an int (psysh's Hoa `FileFinder` passes its
/// `RecursiveIteratorIterator` mode): php coerces it to the `?string $class` parameter, and the
/// compiled prelude class refused it as a type mismatch.
#[test]
fn test_iterator_iterator_class_argument_is_coerced() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Finder {\n    public function getFirst(): int { return 1; }\n    public function run(array $items): array {\n        $out = [];\n        $it = new \\IteratorIterator(new \\ArrayIterator($items), $this->getFirst());\n        foreach ($it as $k => $v) { $out[] = \"$k=$v\"; }\n        return $out;\n    }\n}\necho implode(\",\", (new Finder())->run(['a' => 1, 'b' => 2])), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "a=1,b=2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `PharData::extractTo()` (psysh's self-updater extracts its phar from a downloaded tarball) copies
/// the requested entries, or all of them, out of the archive and throws `PharException` for an
/// existing target without `$overwrite`: the method was undefined. No leak assertion: the
/// `PharData` constructor's own two-string leak is open (bead).
#[test]
fn test_phar_data_extract_to() {
    let out = compile_and_run(
        "<?php\n$base = sys_get_temp_dir() . '/elephc_pharx_' . getmypid();\n@mkdir($base);\n$tar = $base . '/a.tar';\n$p = new PharData($tar);\n$p->addFromString('psysh', \"#!/usr/bin/env php\\nbinary\\n\");\n$p->addFromString('docs/readme.txt', \"read me\\n\");\nunset($p);\n$out = $base . '/out';\nmkdir($out);\n$archive = new PharData($tar);\nvar_dump($archive->extractTo($out, ['psysh'], true));\necho file_get_contents($out . '/psysh');\nvar_dump(file_exists($out . '/docs/readme.txt'));\ntry {\n    $archive->extractTo($out, 'psysh');\n} catch (PharException $e) {\n    echo get_class($e), \" \", $e instanceof Exception ? \"is Exception\" : \"\", \"\\n\";\n}\nvar_dump($archive->extractTo($out . '/all', null));\necho file_get_contents($out . '/all/docs/readme.txt');\nforeach (['psysh', 'all/psysh', 'all/docs/readme.txt'] as $f) { unlink($out . '/' . $f); }\nrmdir($out . '/all/docs'); rmdir($out . '/all'); rmdir($out);\nunlink($tar); rmdir($base);\necho \"done\\n\";\n",
    );
    assert_eq!(out, "bool(true)\n#!/usr/bin/env php\nbinary\nbool(false)\nPharException is Exception\nbool(true)\nread me\ndone\n");
}

/// php 8's `PhpToken` (nikic/php-parser declares `Internal\TokenPolyfill extends \PhpToken` and its
/// `Token` below it): `tokenize()` builds `static` instances with php's ids, lines and byte offsets,
/// and `getTokenName()`, `is()`, `isIgnorable()` and `__toString()` answer as php does. The class was
/// unknown, so php-parser's `Token`, `Lexer` and every parser class were left to the autoloader.
#[test]
fn test_php_token_class_tokenizes_like_php() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass TokenPolyfill extends \\PhpToken {}\nclass Token extends TokenPolyfill {\n    public function getEndPos(): int { return $this->pos + \\strlen($this->text); }\n    public function getEndLine(): int { return $this->line + \\substr_count($this->text, \"\\n\"); }\n}\n$tokens = Token::tokenize(\"<?php\\n\\$a = 1 + b\\\"x\\\"; // c\\n/** d */ echo \\$a;\\n\");\nforeach ($tokens as $t) {\n    echo get_class($t), \" \", $t->id, \" \", var_export($t->getTokenName(), true), \" \", json_encode($t->text),\n        \" \", $t->line, \":\", $t->pos, \" end \", $t->getEndLine(), \":\", $t->getEndPos(),\n        $t->isIgnorable() ? \" ignorable\" : \"\", $t->is([T_VARIABLE, ';']) ? \" var-or-semi\" : \"\", \"\\n\";\n}\n$p = PhpToken::tokenize('<?php echo 1;');\necho get_class($p[0]), \" \", (string) $p[1], \" \", count($p), \" \", $p[0] instanceof Stringable ? \"stringable\" : \"\", \"\\n\";\n$n = new PhpToken(T_STRING, \"foo\", 3, 9);\nvar_dump($n->is('foo'), $n->is(T_STRING), $n->is(T_VARIABLE), $n->getTokenName());\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Token 394 'T_OPEN_TAG' \"<?php\\n\" 1:0 end 2:6 ignorable\nToken 266 'T_VARIABLE' \"$a\" 2:6 end 2:8 var-or-semi\nToken 397 'T_WHITESPACE' \" \" 2:8 end 2:9 ignorable\nToken 61 '=' \"=\" 2:9 end 2:10\nToken 397 'T_WHITESPACE' \" \" 2:10 end 2:11 ignorable\nToken 260 'T_LNUMBER' \"1\" 2:11 end 2:12\nToken 397 'T_WHITESPACE' \" \" 2:12 end 2:13 ignorable\nToken 43 '+' \"+\" 2:13 end 2:14\nToken 397 'T_WHITESPACE' \" \" 2:14 end 2:15 ignorable\nToken 269 'T_CONSTANT_ENCAPSED_STRING' \"b\\\"x\\\"\" 2:15 end 2:19\nToken 59 ';' \";\" 2:19 end 2:20 var-or-semi\nToken 397 'T_WHITESPACE' \" \" 2:20 end 2:21 ignorable\nToken 392 'T_COMMENT' \"\\/\\/ c\" 2:21 end 2:25 ignorable\nToken 397 'T_WHITESPACE' \"\\n\" 2:25 end 3:26 ignorable\nToken 393 'T_DOC_COMMENT' \"\\/** d *\\/\" 3:26 end 3:34 ignorable\nToken 397 'T_WHITESPACE' \" \" 3:34 end 3:35 ignorable\nToken 291 'T_ECHO' \"echo\" 3:35 end 3:39\nToken 397 'T_WHITESPACE' \" \" 3:39 end 3:40 ignorable\nToken 266 'T_VARIABLE' \"$a\" 3:40 end 3:42 var-or-semi\nToken 59 ';' \";\" 3:42 end 3:43 var-or-semi\nToken 397 'T_WHITESPACE' \"\\n\" 3:43 end 4:44 ignorable\nPhpToken echo 5 stringable\nbool(true)\nbool(true)\nbool(false)\nstring(8) \"T_STRING\"\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `token_get_all()` keeps php's binary string prefix with its string: `b'x'` and `B"c"` are ONE
/// T_CONSTANT_ENCAPSED_STRING and an interpolating `b"…"` opens with the `b"` token; the prefix was
/// tokenized as a separate T_STRING `b`.
#[test]
fn test_tokenizer_keeps_the_binary_string_prefix() {
    let out = compile_and_run_with_heap_debug(
        "<?php\n$src = \"<?php \\$x = b'a\\\\'b' . B\\\"c\\\" . b\\\"d\\$x e\\\" . bin(1) . \\$b . b;\\n\";\nforeach (token_get_all($src) as $t) {\n    echo is_array($t) ? token_name($t[0]) . \" \" . json_encode($t[1]) . \" \" . $t[2] : json_encode($t), \"\\n\";\n}\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "T_OPEN_TAG \"<?php \" 1\nT_VARIABLE \"$x\" 1\nT_WHITESPACE \" \" 1\n\"=\"\nT_WHITESPACE \" \" 1\nT_CONSTANT_ENCAPSED_STRING \"b'a\\\\'b'\" 1\nT_WHITESPACE \" \" 1\n\".\"\nT_WHITESPACE \" \" 1\nT_CONSTANT_ENCAPSED_STRING \"B\\\"c\\\"\" 1\nT_WHITESPACE \" \" 1\n\".\"\nT_WHITESPACE \" \" 1\n\"b\\\"\"\nT_ENCAPSED_AND_WHITESPACE \"d\" 1\nT_VARIABLE \"$x\" 1\nT_ENCAPSED_AND_WHITESPACE \" e\" 1\n\"\\\"\"\nT_WHITESPACE \" \" 1\n\".\"\nT_WHITESPACE \" \" 1\nT_STRING \"bin\" 1\n\"(\"\nT_LNUMBER \"1\" 1\n\")\"\nT_WHITESPACE \" \" 1\n\".\"\nT_WHITESPACE \" \" 1\nT_VARIABLE \"$b\" 1\nT_WHITESPACE \" \" 1\n\".\"\nT_WHITESPACE \" \" 1\nT_STRING \"b\" 1\n\";\"\nT_WHITESPACE \"\\n\" 1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// laravel/serializable-closure's `Native::mapByReference()` keys its `ClosureScope` (an
/// `SplObjectStorage` held by an untyped property that is also nulled) by closures:
/// `$data = $this->scope[$data] = $instance` was refused ("Array index must be integer"), and a
/// closure offset read through a `mixed` container took the hash-key path, so `isset()` answered
/// false and the read null. No leak assertion: a temporary closure stored as a key leaks its
/// descriptor (bead).
#[test]
fn test_closure_keys_of_an_untyped_array_access_property() {
    let out = compile_and_run(
        "<?php\nclass ClosureScope extends SplObjectStorage { public $serializations = 0; }\nclass Ser {\n    protected $scope;\n    public $closure;\n    public function __construct($closure = null) { $this->closure = $closure; }\n    public function start() {\n        if ($this->scope === null) { $this->scope = new ClosureScope(); }\n        $this->scope[$this->closure] = $this;\n    }\n    public function stop() { $this->scope = null; }\n    protected function mapByReference(&$data) {\n        if ($data instanceof Closure) {\n            if ($data === $this->closure) { $data = 'self'; return; }\n            if (isset($this->scope[$data])) { $data = $this->scope[$data]; return; }\n            $instance = new static($data);\n            $instance->scope = $this->scope;\n            $data = $this->scope[$data] = $instance;\n        }\n    }\n    public function run($f) { $this->mapByReference($f); return $f; }\n}\n$s = new Ser(fn () => 1);\n$s->start();\n$g = fn () => 2;\n$r = $s->run($g);\necho get_class($r), \" \", $s->run($g) === $r ? \"same\" : \"other\", \"\\n\";\n$s->stop();\n\nclass H2 {\n    public $scope;\n    public function __construct() { $this->scope = new SplObjectStorage(); }\n    public function has($data) { return var_export(isset($this->scope[$data]), true); }\n    public function get($data) { return $this->scope[$data]; }\n}\n$h = new H2();\n$f = fn () => 1;\n$o = new stdClass();\n$h->scope[$f] = 'closure';\n$h->scope[$o] = 'object';\necho $h->has($f), \" \", $h->has($o), \" \", $h->get($f), \" \", $h->get($o), \"\\n\";\n$m = $h->scope;\n$k = $f;\necho var_export(isset($m[$k]), true), \"\\n\";\n\nclass H3 {\n    public $scope;\n    public function __construct() { $this->scope = new SplObjectStorage(); }\n    public function plain($data) {\n        if (isset($this->scope[$data])) { return 'found'; }\n        $this->scope[$data] = 'v';\n        return 'stored';\n    }\n    public function narrowed($data) {\n        if ($data instanceof Closure) {\n            if (isset($this->scope[$data])) { return 'found'; }\n            $this->scope[$data] = 'v';\n            return 'stored';\n        }\n        return 'no';\n    }\n}\n$h3 = new H3();\n$f = fn () => 1;\necho $h3->plain($f), \" \", $h3->plain($f), \"\\n\";\n$g = fn () => 2;\necho $h3->narrowed($g), \" \", $h3->narrowed($g), \"\\n\";\n$o = new stdClass();\necho $h3->narrowed(fn () => 3), \" \", $h3->plain($o), \" \", $h3->plain($o), \"\\n\";\n",
    );
    assert_eq!(out, "Ser same\ntrue true closure object\ntrue\nstored found\nstored found\nstored stored found\n");
}

/// `array_key_exists('<name>', $GLOBALS)` (psysh's `Shell::isIncluded()` looks for Composer's
/// `_composer_autoload_path`) answers whether the global is set, in a function and at top level;
/// every whole-array use of `$GLOBALS` was refused, which left psysh's `Shell` to the autoloader.
#[test]
fn test_array_key_exists_on_globals_answers_whether_the_global_is_set() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nnamespace Psy;\nclass Shell {\n    public static function isIncluded(array $trace): bool {\n        $isIncluded = isset($trace[0]['function']) && \\in_array($trace[0]['function'], ['require', 'include']);\n        if ($isIncluded && \\array_key_exists('_composer_autoload_path', $GLOBALS)) {\n            return false;\n        }\n        return $isIncluded;\n    }\n    public static function plain(): string {\n        return (array_key_exists('marker', $GLOBALS) ? 'set' : 'unset') . ' ' . (\\array_key_exists('_composer_autoload_path', $GLOBALS) ? 'set' : 'unset');\n    }\n}\nvar_dump(Shell::isIncluded([['function' => 'require']]));\necho Shell::plain(), \"\\n\";\n$marker = 1;\n$_composer_autoload_path = 'x';\nvar_dump(Shell::isIncluded([['function' => 'require']]));\necho Shell::plain(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nunset unset\nbool(false)\nset set\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `constant('NAME')` of a constant a `define()` creates inside a function (php-parser's
/// `defineCompatibilityTokens()`) reads the defined value: the call was lowered as a global load
/// typed `mixed`, which read the stored int as a box and crashed, while the bare `NAME` worked.
#[test]
fn test_constant_of_a_name_defined_in_a_function() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nnamespace PhpParser;\nfunction defineIt(): void {\n    if (!\\defined('T_ELEPHC_COMPAT')) { \\define('T_ELEPHC_COMPAT', -1); }\n}\nclass Lexer {\n    public function token(): string { return 'token ' . \\constant('T_ELEPHC_COMPAT'); }\n}\ndefineIt();\necho (new Lexer())->token(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "token -1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A user class implementing `Reflector` owes only php's `__toString()` (psysh's
/// `Reflection\ReflectionConstant`): elephc's interface also declared `getAttributes()`, which php's
/// does not, so the class was refused as not implementing it. A `Reflector`-typed receiver still
/// answers `getAttributes()` through the reflection object it holds.
#[test]
fn test_a_user_reflector_owes_only_to_string() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass ReflectionConstantish implements \\Reflector {\n    public function __construct(private string $name) {}\n    public function getName(): string { return $this->name; }\n    public function __toString(): string { return \"Constant [ {$this->name} ]\"; }\n}\nfunction describe(\\Reflector $r): string { return (string) $r; }\n$c = new ReflectionConstantish('PHP_EOL');\necho describe($c), \" \", $c->getName(), \" \", $c instanceof Stringable ? \"stringable\" : \"\", \"\\n\";\necho count((new ReflectionClass('ReflectionConstantish'))->getAttributes()), \"\\n\";\n\n#[Attribute]\nclass Marker {}\n#[Marker]\nclass Tagged { #[Marker] public function m() {} }\nfunction attrs(\\Reflector $r): int { return count($r->getAttributes()); }\necho attrs(new ReflectionClass('Tagged')), \" \", attrs(new ReflectionMethod('Tagged', 'm')), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Constant [ PHP_EOL ] PHP_EOL stringable\n0\n1 1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `preserve_keys` flag written as an assignment of a literal (`array_slice($lines, $o, $l,
/// $preserveKeys = true)`, nunomaduro/collision's `Highlighter`) is that literal: the call was refused
/// as non-literal, and the backend now receives the literal after the assignment runs, so the
/// checker and the backend agree on the result shape (a `false` one on a list read garbage).
#[test]
fn test_preserve_keys_flag_assigned_in_the_call_is_its_literal() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Highlighter {\n    private function lines(string $source): array {\n        $out = [];\n        foreach (explode(\"\\n\", $source) as $i => $line) { $out[$i + 1] = strtoupper($line); }\n        return $out;\n    }\n    public function snippet(string $source, int $lineNumber, int $before = 1, int $after = 1): string {\n        $tokenLines = $this->lines($source);\n        $offset = max($lineNumber - $before - 1, 0);\n        $length = $after + $before + 1;\n        $tokenLines = array_slice($tokenLines, $offset, $length, $preserveKeys = true);\n        $s = '';\n        foreach ($tokenLines as $n => $line) { $s .= \"$n:$line \"; }\n        return $s . var_export($preserveKeys, true);\n    }\n    public function listed(array $items): string {\n        $kept = array_slice($items, 1, 2, $flag = true);\n        $chunks = array_chunk([1, 2, 3], 2, $keep = true);\n        return json_encode($kept) . ' ' . json_encode(array_slice(['x', 'y', 'z'], 1, 2, $no = false))\n            . ' ' . json_encode($chunks) . ' ' . var_export($flag && $keep && !$no, true);\n    }\n}\n$h = new Highlighter();\necho $h->snippet(\"a\\nb\\nc\\nd\", 3), \"\\n\";\necho $h->listed(['p', 'q', 'r']), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:B 3:C 4:D true\n{\"1\":\"q\",\"2\":\"r\"} [\"y\",\"z\"] [[1,2],{\"2\":3}] true\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `ob_start([$shell, 'writeStdout'], 1)` (psysh's execution closures) installs the bound method as
/// the output handler, which runs with php's phases for each chunk: array-pair handlers were refused
/// at compile time. The pair is resolved as the closure `$shell->writeStdout(...)`.
#[test]
fn test_ob_start_array_pair_handler_is_the_bound_method() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Shell {\n    public string $captured = '';\n    public function writeStdout($out, $phase = PHP_OUTPUT_HANDLER_END) {\n        $this->captured .= \"[$phase:\" . $out . \"]\";\n        return '';\n    }\n    public function run(callable $body): string {\n        $shell = $this;\n        ob_start([$shell, 'writeStdout'], 1);\n        $body();\n        ob_end_flush();\n        return $this->captured;\n    }\n}\n$s = new Shell();\necho strlen($s->run(function () { echo \"hello\"; echo \" world\"; })) > 0 ? \"captured\\n\" : \"none\\n\";\necho $s->captured, \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "captured\n[1:hello][0: world][8:]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// psysh's `ReflectionLanguageConstruct extends \ReflectionFunctionAbstract` and
/// `ReflectionLanguageConstructParameter extends \ReflectionParameter`: php's `ReflectionParameter`
/// is not final, and `ReflectionFunctionAbstract`'s methods are concrete, raising "Internal error:
/// Failed to retrieve the reflection object" on a subclass that never set its function. elephc
/// declared the first final and the second's methods abstract, which refused both classes.
#[test]
fn test_user_subclasses_of_reflection_function_abstract_and_parameter() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass ConstructParameter extends \\ReflectionParameter {\n    private $function;\n    private $parameter;\n    private array $opts;\n    public function __construct($function, $parameter, array $opts) {\n        $this->function = $function;\n        $this->parameter = $parameter;\n        $this->opts = $opts;\n    }\n    public function getName(): string { return $this->parameter; }\n    public function isOptional(): bool { return !empty($this->opts['isOptional']); }\n    public function isVariadic(): bool { return !empty($this->opts['isVariadic']); }\n}\nclass LanguageConstruct extends \\ReflectionFunctionAbstract {\n    public $keyword;\n    private const CONSTRUCTS = ['isset' => ['var' => [], 'vars' => ['isVariadic' => true]]];\n    public function __construct(string $keyword) {\n        if (!isset(self::CONSTRUCTS[$keyword])) { throw new \\InvalidArgumentException('Unknown language construct: ' . $keyword); }\n        $this->keyword = $keyword;\n    }\n    public function getName(): string { return $this->keyword; }\n    public function returnsReference(): bool { return false; }\n    public function getParameters(): array {\n        $params = [];\n        foreach (self::CONSTRUCTS[$this->keyword] as $parameter => $opts) {\n            $params[] = new ConstructParameter($this->keyword, $parameter, $opts);\n        }\n        return $params;\n    }\n    public function __toString(): string { return $this->getName(); }\n}\n$c = new LanguageConstruct('isset');\necho $c, \" \", $c instanceof ReflectionFunctionAbstract ? \"rfa\" : \"\", \"\\n\";\nforeach ($c->getParameters() as $p) {\n    echo $p->getName(), \":\", var_export($p->isVariadic(), true), \":\", $p instanceof ReflectionParameter ? \"param\" : \"\", \" \";\n}\necho \"\\n\";\ntry { $c->getEndLine(); } catch (Error $e) { echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }\ntry { new LanguageConstruct('nope'); } catch (InvalidArgumentException $e) { echo $e->getMessage(), \"\\n\"; }\n$f = new ReflectionFunction('strlen');\necho $f->getName(), \" \", $f->getNumberOfParameters(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "isset rfa\nvar:false:param vars:true:param \nError: Internal error: Failed to retrieve the reflection object\nUnknown language construct: nope\nstrlen 1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `[&$this, 'callback']` (psysh's `AutoCompleter::activate()`) is the callable pair holding `$this`:
/// `$this` can never be rebound, so the reference is its value. The array-literal reference parser
/// refused it.
#[test]
fn test_reference_to_this_in_an_array_literal_is_this() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass AutoCompleter {\n    public function callback(string $input): array { return [\"$input!\"]; }\n    public function activate(): array { return [&$this, 'callback']; }\n}\n$c = new AutoCompleter();\n$cb = $c->activate();\nvar_dump(is_callable($cb), $cb[0] === $c, call_user_func($cb, 'ls'));\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\narray(1) {\n  [0]=>\n  string(3) \"ls!\"\n}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A whole-array read of `$GLOBALS` (psysh's `GlobalVariableEnumerator`: `global $GLOBALS;`,
/// `array_keys($GLOBALS)`, `$GLOBALS[$name]`) answers the program's set globals, including the null
/// `GLOBALS` global that `global $GLOBALS;` creates: every such use was refused at parse time.
#[test]
fn test_whole_array_read_of_globals_lists_the_set_globals() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Enumerator {\n    protected function getGlobals(): array {\n        global $GLOBALS;\n        $names = \\array_keys($GLOBALS);\n        \\natcasesort($names);\n        $ret = [];\n        foreach ($names as $name) {\n            if ($name[0] === '_' || $name === 'argv' || $name === 'argc') { continue; }\n            $ret[$name] = $GLOBALS[$name];\n        }\n        return $ret;\n    }\n    public function show(): string { return json_encode($this->getGlobals()); }\n}\n$zeta = 3;\n$alpha = 'a';\n$items = [1, 2];\n$e = new Enumerator();\necho $e->show(), \"\\n\";\n$late = true;\necho $e->show(), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"alpha\":\"a\",\"e\":{},\"GLOBALS\":null,\"items\":[1,2],\"zeta\":3}\n{\"alpha\":\"a\",\"e\":{},\"GLOBALS\":null,\"items\":[1,2],\"late\":true,\"zeta\":3}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `self::$settings[] = $setting` on a declared `private static ?array $settings = null` (PHPUnit's
/// `ExtensionSettings`) appends, starting the array from null as php does: the nullable array type
/// was refused as not an array.
#[test]
fn test_append_to_a_nullable_array_static_property() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfinal class ExtensionSettings {\n    private static ?array $settings = null;\n    public static function collect(array $candidates, ?array $loaded): array {\n        if (self::$settings !== null) {\n            return self::$settings;\n        }\n        if ($loaded === null) {\n            return [];\n        }\n        foreach ($candidates as $setting => $name) {\n            if (in_array($name, $loaded, true)) {\n                self::$settings[] = $setting;\n            }\n        }\n        return self::$settings ?? ['none'];\n    }\n}\necho json_encode(ExtensionSettings::collect(['-d ext=a' => 'a', '-d ext=b' => 'b', '-d ext=c' => 'c'], ['a', 'c'])), \"\\n\";\necho json_encode(ExtensionSettings::collect([], [])), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[\"-d ext=a\",\"-d ext=c\"]\n[\"-d ext=a\",\"-d ext=c\"]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `ReflectionClass::getConstants()` of flag constants (`self::A | self::B`, `1 << 2`, `~self::A`,
/// `64 >> 3`), an exact int division and a concatenation with an int: the reflection metadata
/// evaluator refused every one of them, which stopped the closed-world Laravel artisan build in the
/// backend.
#[test]
fn test_reflection_constants_of_flag_expressions() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Flags {\n    public const A = 1;\n    public const B = 1 << 2;\n    public const ALL = self::A | self::B | 16;\n    public const MASK = self::ALL & ~self::A;\n    public const X = self::ALL ^ 4;\n    public const HALF = 8 / 2;\n    public const DOWN = 64 >> 3;\n    public const LABEL = 'v' . self::HALF . '-' . 2;\n}\n$r = new ReflectionClass('Flags');\nforeach ($r->getConstants() as $name => $value) { echo $name, '=', var_export($value, true), ' '; }\necho \"\\n\", var_export($r->getConstant('ALL'), true), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "A=1 B=4 ALL=21 MASK=20 X=17 HALF=4 DOWN=8 LABEL='v4-2' \n21\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A parameter default reading a constant of a class nobody declares (Laravel's `Number::parse()`
/// defaults to `NumberFormatter::TYPE_DOUBLE`, and elephc has no ext/intl): php only fails when the
/// default is evaluated, with `Class "..." not found`. The reflection metadata evaluator stopped
/// the closed-world artisan build in the backend instead.
#[test]
fn test_reflected_default_naming_an_undefined_class_constant() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nnamespace App;\nuse Intl\\Fmt;\nclass Num { public static function parse(string $s, ?int $type = Fmt::TYPE_DOUBLE): int { return 1; } }\n$p = (new \\ReflectionMethod(Num::class, \"parse\"))->getParameters()[1];\nvar_dump($p->isDefaultValueAvailable(), $p->getDefaultValueConstantName());\ntry { var_dump($p->getDefaultValue()); } catch (\\Error $e) { echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }\necho Num::parse(\"1\", 2), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nstring(21) \"Intl\\Fmt::TYPE_DOUBLE\"\nError: Class \"Intl\\Fmt\" not found\n1\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A 24-term flag set (`self::F0 | ... | self::F23`) and a 24-piece concatenation, reflected as
/// constants and as parameter defaults: the metadata evaluator counted every binary node as a
/// recursion level and refused past 16, which stopped the closed-world Laravel artisan build in
/// the backend ("deep recursive ReflectionClass constant metadata").
#[test]
fn test_reflection_constants_of_long_operator_chains() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Deep {\n    public const F0 = 1;\n    public const F1 = 2;\n    public const F2 = 4;\n    public const F3 = 8;\n    public const F4 = 16;\n    public const F5 = 32;\n    public const F6 = 64;\n    public const F7 = 128;\n    public const F8 = 256;\n    public const F9 = 512;\n    public const F10 = 1024;\n    public const F11 = 2048;\n    public const F12 = 4096;\n    public const F13 = 8192;\n    public const F14 = 16384;\n    public const F15 = 32768;\n    public const F16 = 65536;\n    public const F17 = 131072;\n    public const F18 = 262144;\n    public const F19 = 524288;\n    public const F20 = 1048576;\n    public const F21 = 2097152;\n    public const F22 = 4194304;\n    public const F23 = 8388608;\n    public const ALL = self::F0 | self::F1 | self::F2 | self::F3 | self::F4 | self::F5 | self::F6 | self::F7 | self::F8 | self::F9 | self::F10 | self::F11 | self::F12 | self::F13 | self::F14 | self::F15 | self::F16 | self::F17 | self::F18 | self::F19 | self::F20 | self::F21 | self::F22 | self::F23;\n    public const LABEL = 'p0' . 'p1' . 'p2' . 'p3' . 'p4' . 'p5' . 'p6' . 'p7' . 'p8' . 'p9' . 'p10' . 'p11' . 'p12' . 'p13' . 'p14' . 'p15' . 'p16' . 'p17' . 'p18' . 'p19' . 'p20' . 'p21' . 'p22' . 'p23';\n    public function m(int $f = self::ALL, string $s = self::LABEL) {}\n}\n$r = new ReflectionClass(\"Deep\");\nvar_dump($r->getConstant(\"ALL\"), $r->getConstant(\"LABEL\"));\nforeach ((new ReflectionMethod(\"Deep\", \"m\"))->getParameters() as $p) { var_dump($p->getDefaultValue()); }\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "int(16777215)\nstring(62) \"p0p1p2p3p4p5p6p7p8p9p10p11p12p13p14p15p16p17p18p19p20p21p22p23\"\nint(16777215)\nstring(62) \"p0p1p2p3p4p5p6p7p8p9p10p11p12p13p14p15p16p17p18p19p20p21p22p23\"\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
