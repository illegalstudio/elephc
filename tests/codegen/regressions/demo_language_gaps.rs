//! Purpose:
//! Regression tests for PHP shapes symfony/demo's vendor tree uses that elephc refused or
//! miscompiled: each one sent its whole class file to the runtime autoloader (a `--closed-world`
//! build then fails) or produced a wrong value.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5's output on the same program.
//! - The vendor file each shape comes from is named in its test's doc comment.

use crate::support::*;

/// `use (...)` with a trailing comma (doctrine/orm's `ProxyFactory`, Symfony's
/// `CachingHttpClient`).
#[test]
fn test_closure_use_list_accepts_a_trailing_comma() {
    let out = compile_and_run(
        "<?php\n$a = 1; $b = 2;\n$f = function (int $x) use (\n    $a,\n    &$b,\n): int { $b++; return $x + $a + $b; };\necho $f(10), \" \", $b, \"\\n\";\n",
    );
    assert_eq!(out, "14 3\n");
}

/// `foreach (... as $k => $list[])` appends every value (Symfony's
/// `IsGrantedAttributeListener`), also into a nested element.
#[test]
fn test_foreach_value_target_can_be_an_append() {
    let out = compile_and_run(
        "<?php\nfunction gen() { yield 'A' => 1; yield 'B' => 2; }\n$attributes = [];\nforeach (gen() as $class => $attributes[]) { echo $class, count($attributes), ' '; }\n$m = ['x' => []];\nforeach ([5, 6] as $m['x'][]) {}\necho json_encode($attributes), json_encode($m), \"\\n\";\n",
    );
    assert_eq!(out, "A1 B2 [1,2]{\"x\":[5,6]}\n");
}

/// `$this->in = &$pipes[0]` (Symfony's `ProcessStream`): the property and the element are one
/// reference afterwards, in both directions.
#[test]
fn test_property_reference_to_an_array_element_is_shared_both_ways() {
    let out = compile_and_run(
        "<?php\nclass S {\n    private $in; private $out;\n    public function open(): void {\n        $pipes = ['i0', 'o1'];\n        $this->in = &$pipes[0];\n        $this->out = &$pipes[1];\n        $pipes[0] = 'changed';\n        echo $this->in, ' ', $this->out, \"\\n\";\n        $this->out = 'w';\n        echo $pipes[1], \"\\n\";\n    }\n}\n(new S())->open();\n",
    );
    assert_eq!(out, "changed o1\nw\n");
}

/// `$a[$k] = &$o->p`, `$list[] = &$o->p` and `[&$this->p, ...]` (Symfony's `NativeResponse`)
/// share the property's cell: before, the element marker wrote boxed values into a property
/// still stored as a raw int, and the property read back a pointer.
#[test]
fn test_array_elements_referencing_untyped_properties_share_their_cell() {
    let out = compile_and_run(
        "<?php\nclass O { public $p = 1; public $q = 'a'; }\nfunction f() {\n    $o = new O(); $a = [];\n    $a['r'] = &$o->p; $a['r'] = 7; echo $o->p, ' ';\n    $o->p = 9; echo $a['r'], ' ';\n    $b = [1]; $b[] = &$o->q; $b[1] .= 'x'; echo $o->q, ' ';\n    return $a;\n}\n$a = f(); echo $a['r'], \"\\n\";\nclass R {\n    private $pause = 0.0; private $remaining = 5;\n    public function go(): void {\n        $h = [&$this->pause, 'H', &$this->remaining];\n        $h[0] = 2.5; $this->remaining = 0;\n        echo $this->pause, ' ', $h[2], \"\\n\";\n    }\n}\n(new R())->go();\n",
    );
    assert_eq!(out, "7 9 ax 9\n2.5 0\n");
}

/// `Ancestor::method()` from a descendant's instance method is a non-virtual call on `$this`,
/// like `parent::` aimed further up (Doctrine DBAL's `MySQL80Platform::createSelectSQLBuilder()`
/// calls `AbstractPlatform::createSelectSQLBuilder()`). Its result was typed `int`, so the
/// method's declared return refused the body.
#[test]
fn test_named_grandparent_instance_call_keeps_its_return_type() {
    let out = compile_and_run(
        "<?php\ninterface Builder { public function sql(): string; }\nfinal class DefaultBuilder implements Builder {\n    public function __construct(private readonly Base $p, private readonly ?string $lock) {}\n    public function sql(): string { return get_class($this->p) . ':' . ($this->lock ?? 'none'); }\n}\nabstract class Base { public function create(): Builder { return new DefaultBuilder($this, 'SKIP'); } }\nabstract class Mid extends Base { public function create(): Builder { return new DefaultBuilder($this, null); } }\nclass Leaf extends Mid { public function create(): Builder { return Base::create(); } }\necho (new Leaf())->create()->sql(), \"\\n\";\n",
    );
    assert_eq!(out, "Leaf:SKIP\n");
}

/// An `iterable` parameter narrowed by `is_array()` or handed to a by-reference array builtin
/// (Symfony DI's `ExpressionLanguage`: `if (!is_array($providers)) { $providers =
/// iterator_to_array($providers); } array_unshift($providers, ...)`). The slot's bare-pointer
/// storage was refused by `implode()`, written through as an array by `array_unshift()`, and
/// left the untyped function's return type at the `int` placeholder (`0`).
#[test]
fn test_iterable_parameter_narrowed_to_an_array_reaches_array_builtins() {
    let out = compile_and_run(
        "<?php\nfunction f(iterable $providers = []) {\n    if (!is_array($providers)) { $providers = iterator_to_array($providers, false); }\n    array_unshift($providers, 'first');\n    return implode(',', $providers);\n}\nfunction g(iterable $p) { if (is_array($p)) { return count($p) . ':' . implode(',', $p); } return 't'; }\nfunction h(iterable $p) { array_push($p, 'z'); return 's'; }\nfunction i(iterable $p) { array_unshift($p, 'x'); return implode(',', $p); }\necho f(['b']), ' ', f(new ArrayIterator(['c'])), ' ', g(['b', 'c']), ' ', h(['a']), ' ', i(['a']), \"\\n\";\ntry { i(new ArrayIterator(['a'])); } catch (TypeError $e) { echo $e->getMessage(), \"\\n\"; }\n",
    );
    assert_eq!(
        out,
        "first,b first,c 2:b,c s x,a\narray_unshift(): Argument #1 ($array) must be of type array, ArrayIterator given\n"
    );
}

/// `private string $algorithm = \PASSWORD_BCRYPT;` (Symfony's `NativePasswordHasher`): a
/// catalogued constant has its catalogued type in a declaration default. It was typed `int` and
/// the class refused, so the demo's password hasher went to the runtime autoloader.
#[test]
fn test_string_constant_as_a_typed_property_default() {
    let out = compile_and_run(
        "<?php\nfinal class H {\n    private string $algorithm = \\PASSWORD_BCRYPT;\n    public string $os = \\PHP_OS_FAMILY;\n    public function hash(string $p): string { return password_hash($p, $this->algorithm, ['cost' => 4]); }\n    public function a(): string { return $this->algorithm; }\n}\n$h = new H(); echo $h->a(), ' ', password_verify('kitten', $h->hash('kitten')) ? 'ok' : 'no', ' ', $h->os !== '' ? 'os' : '', \"\\n\";\n",
    );
    assert_eq!(out, "2y ok os\n");
}

/// php's weak mode binds a float to an `int` or `?int` parameter: an integral value as it is, a
/// fractional one with the "loses precision" deprecation, a non-finite one with a `TypeError`
/// (Symfony's `LoginThrottlingListener` passes `ceil(...)` to a `?int` constructor parameter).
#[test]
fn test_weak_float_argument_binds_to_int_and_nullable_int_parameters() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nclass Ex extends Exception { public function __construct(private ?int $threshold = null) { parent::__construct('t'); } public function t(): ?int { return $this->threshold; } }\nfunction make(int $retry) { return new Ex(ceil($retry / 60)); }\nfunction takes(int $x) { return $x; }\nvar_dump(make(130)->t(), takes(ceil(7 / 2)), takes(2.5));\ntry { takes(NAN); } catch (TypeError $e) { echo get_class($e), \"\\n\"; }\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "int(3)\nint(4)\nint(2)\nTypeError\n");
    // A compiled program writes php's diagnostics to stderr.
    assert!(
        out.stderr.contains("Deprecated: Implicit conversion from float 2.5 to int loses precision"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The float cases the checker used to refuse because it "had no run-time deprecation channel":
/// a lossy literal and a run-time float bind with php's deprecation, an out-of-range literal
/// throws php's TypeError naming the call site.
#[test]
fn test_float_literals_and_values_at_an_int_parameter_follow_php() {
    let out = compile_and_run(
        "<?php\nfunction ti(int $i) { return $i; }\necho ti(5.5), \"\\n\";\n$f = 5.5 * $argc;\necho ti($f), \"\\n\";\ntry { echo ti(1e20); } catch (TypeError $e) { echo explode(', called in', $e->getMessage())[0], \"\\n\"; }\n",
    );
    assert_eq!(out, "5\n5\nti(): Argument #1 ($i) must be of type int, float given\n");
}

/// `?->` on a union of several classes and null (Symfony's `DomVisitor`:
/// `$domNode->attributes?->getIterator()` over `\Dom\NamedNodeMap|\DOMNamedNodeMap|null`) takes
/// the boxed receiver's run-time dispatch; it was refused for not naming a single class.
#[test]
fn test_nullsafe_call_on_a_union_of_several_classes() {
    let out = compile_and_run(
        "<?php\nclass A { public function it(): array { return ['a']; } }\nclass B { public function it(): array { return ['b', 'c']; } }\nfunction pick(int $n): A|B|null { return match ($n) { 0 => new A(), 1 => new B(), default => null }; }\nforeach ([0, 1, 2] as $n) {\n    echo json_encode(pick($n)?->it()), ' ';\n    if (!$x = pick($n)?->it()) { echo 'none '; }\n}\necho \"\\n\";\n",
    );
    assert_eq!(out, "[\"a\"] [\"b\",\"c\"] null none \n");
}

/// A call on a receiver that is never an object raises php's Error when it runs, with `->` and
/// with `?->`, before any argument is evaluated. `$param->getDescription()?->render()` in
/// Symfony's `PhpDocExtractor` -- phpdocumentor is not installed, so `getDescription()` is read
/// from the program's only declaring class, which returns `string` -- refused the whole file for a
/// line that never runs, and the backend had no lowering for the live case.
#[test]
fn test_method_call_on_a_scalar_receiver_raises_phps_error() {
    let out = compile_and_run(
        "<?php\nclass Command { public function getDescription(): string { return 'cmd'; } }\nfunction describe(iterable $tags): ?string {\n    foreach ($tags as $param) {\n        if (!$param instanceof Missing\\Tags\\Param) { continue; }\n        if ($d = $param->getDescription()?->render()) { return $d; }\n    }\n    return null;\n}\nvar_dump(describe([new Command()]));\nfunction d(string $s) { return $s; }\nfunction side() { echo 'side '; return 1; }\ntry { d('x')?->render(side()); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }\ntry { d('x')->render(); } catch (Error $e) { echo $e->getMessage(), \"\\n\"; }\ntry { $a = [1]; $a->m(); } catch (Error $e) { echo $e->getMessage(), \"\\n\"; }\ntry { $i = 1.5 * $argc; $i?->m(); } catch (Error $e) { echo $e->getMessage(), \"\\n\"; }\n",
    );
    assert_eq!(
        out,
        "NULL\nError: Call to a member function render() on string\nCall to a member function render() on string\nCall to a member function m() on array\nCall to a member function m() on float\n"
    );
}

/// `insteadof` naming a trait another `use` statement of the class imports (Symfony's
/// `CachingHttpClient`: `use AsyncDecoratorTrait { AsyncDecoratorTrait::withOptions insteadof
/// HttpClientTrait; } use HttpClientTrait;`). php resolves every `use` of a class together; one
/// statement at a time, the trait was "unrelated" and the class refused.
#[test]
fn test_insteadof_across_trait_use_statements() {
    let out = compile_and_run(
        "<?php\ntrait Decorator { public function withOptions(): string { return 'decorator'; } public function stream(): string { return 'dstream'; } }\ntrait Client { public function withOptions(): string { return 'client'; } public function prepare(): string { return 'prep'; } }\nclass Caching {\n    use Decorator {\n        stream as asyncStream;\n        Decorator::withOptions insteadof Client;\n    }\n    use Client;\n    public function stream(): string { return 'own:' . $this->asyncStream(); }\n}\n$c = new Caching();\necho $c->withOptions(), ' ', $c->stream(), ' ', $c->prepare(), \"\\n\";\nclass Plain { use Client; use Decorator { Decorator::withOptions insteadof Client; } }\necho (new Plain())->withOptions(), \"\\n\";\n",
    );
    assert_eq!(out, "decorator own:dstream prep\ndecorator\n");
}

/// A reference to an element whose key is `string|int` (Symfony's `PropertyAccessor::readIndex()`:
/// `$result[self::REF] = &$zval[self::REF][$index]`): every member is a key, and the element is
/// shared with the array the caller holds by reference.
#[test]
fn test_reference_to_an_element_with_a_string_or_int_key() {
    let out = compile_and_run(
        "<?php\nconst VALUE = 0; const REF = 1;\nfunction readIndex(array $zval, string|int $index): array {\n    $result = [VALUE => null];\n    if (isset($zval[VALUE][$index])) {\n        $result[VALUE] = $zval[VALUE][$index];\n        if (isset($zval[REF]) && is_array($zval[VALUE])) { $result[REF] = &$zval[REF][$index]; }\n    }\n    return $result;\n}\n$data = ['a' => 1, 3 => 'x'];\n$z = [VALUE => $data, REF => &$data];\n$r = readIndex($z, 'a'); $r[REF] = 42;\n$r2 = readIndex($z, 3); $r2[REF] .= 'y';\necho json_encode($data), \"\\n\";\n",
    );
    assert_eq!(out, "{\"a\":42,\"3\":\"xy\"}\n");
}

/// A property element written through a receiver typed by an interface, which declares no
/// property (Doctrine ORM's `XmlDriver`: `$metadata->table['indexes'] = []` over
/// `Persistence\Mapping\ClassMetadata`): php resolves it on the run-time object. It was
/// "Undefined class" in the checker, and the backend had no path for the receiver.
#[test]
fn test_property_element_writes_through_an_interface_typed_receiver() {
    let out = compile_and_run(
        "<?php\ninterface Meta { public function getName(): string; }\nclass Orm implements Meta { public array $table = ['name' => 't']; public $plain; public function getName(): string { return 'orm'; } }\nfunction load(Meta $metadata): void {\n    $metadata->table['indexes'] = [];\n    $metadata->table['indexes']['i1'] = ['columns' => ['a']];\n    $metadata->table['list'][] = 'x';\n    $metadata->plain = 5;\n    echo $metadata->plain, ' ', $metadata->table['name'], \"\\n\";\n}\n$m = new Orm();\nload($m);\necho json_encode($m->table), ' ', $m->plain, \"\\n\";\n",
    );
    assert_eq!(out, "5 t\n{\"name\":\"t\",\"indexes\":{\"i1\":{\"columns\":[\"a\"]}},\"list\":[\"x\"]} 5\n");
}

/// `ArrayObject::__serialize()` / `__unserialize()` in php's `[flags, storage, members, iterator
/// class]` form, `getFlags()`/`setFlags()`, and a `serialize()` round trip (Symfony's var-exporter
/// `Hydrator` rebuilds an ArrayObject through `__unserialize()`). Value-checked against `php -n`.
#[test]
fn test_array_object_serialization_round_trip() {
    let out = compile_and_run(
        "<?php\n$o = new ArrayObject(['a' => 1], 2);\nvar_export($o->__serialize()); echo \"\\n\";\n$p = new ArrayObject();\n$p->__unserialize([1, ['x' => 2], [], null]);\necho $p->getFlags(), json_encode($p->getArrayCopy()), \"\\n\";\n$q = unserialize(serialize($o));\necho get_class($q), ' ', json_encode($q->getArrayCopy()), ' ', $q->getFlags(), \"\\n\";\n",
    );
    assert_eq!(
        out,
        "array (\n  0 => 2,\n  1 => \n  array (\n    'a' => 1,\n  ),\n  2 => \n  array (\n  ),\n  3 => NULL,\n)\n1{\"x\":2}\nArrayObject {\"a\":1} 2\n"
    );
}

/// `ReflectionClass::resetAsLazyGhost()` / `resetAsLazyProxy()` (Symfony's Doctrine
/// `ManagerRegistry::resetService()` re-creates a closed entity manager in place): defaults are
/// restored and the initializer re-run, or the factory's instance is copied onto the object.
/// Value-checked against `php -n`.
#[test]
fn test_reset_as_lazy_ghost_and_proxy_recreate_the_object_in_place() {
    let out = compile_and_run(
        "<?php\nclass Manager {\n    private bool $closed = false;\n    public array $log = [];\n    public function __construct(public string $conn) { $this->log[] = \"ctor:$conn\"; }\n    public function close(): void { $this->closed = true; }\n    public function isOpen(): bool { return !$this->closed; }\n}\n$m = new Manager('db1');\n$m->close();\n$r = new ReflectionClass($m);\n$r->resetAsLazyGhost($m, function (Manager $o) { $o->__construct('db2'); });\necho $m->isOpen() ? 'open' : 'closed', ' ', $m->conn, ' ', json_encode($m->log), \"\\n\";\n$m->close();\n$r->resetAsLazyProxy($m, fn (Manager $o) => new Manager('db3'));\necho $m->isOpen() ? 'open' : 'closed', ' ', $m->conn, ' ', json_encode($m->log), \"\\n\";\n",
    );
    assert_eq!(out, "open db2 [\"ctor:db2\"]\nopen db3 [\"ctor:db3\"]\n");
}

/// Socket-connect error outputs written into properties and elements (Monolog's `SocketHandler`:
/// `@pfsockopen($host, -1, $this->errno, $this->errstr, $timeout)`): the builtin writes local
/// slots only, so each place takes a hidden local and is assigned after the call. They were
/// refused ("must be passed a variable"); accepted as is, a property kept its old value.
#[test]
fn test_socket_error_outputs_reach_properties_and_elements() {
    let out = compile_and_run(
        "<?php\nclass Handler {\n    private $errno = 0;\n    private $errstr = '';\n    public array $errs = [0, ''];\n    public function open(): void {\n        $r = @fsockopen('127.0.0.1', 1, $this->errno, $this->errstr, 0.5);\n        var_dump($r === false, $this->errno !== 0, $this->errstr !== '');\n        $s = @stream_socket_client('tcp://127.0.0.1:1', $this->errs[0], $this->errs[1], 0.5);\n        var_dump($s === false, $this->errs[0] !== 0, is_string($this->errs[1]));\n    }\n}\n(new Handler())->open();\n",
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n");
}

/// An assignment inside the operand of `yield from` defines its variable for the statements after
/// it (Symfony's `AsyncResponse::stream()`: `yield from self::passthruStream($response =
/// $r->response, $r, $asyncMap)`, then `isset($asyncMap[$response])`). The effect pass had no
/// `yield from` case, so `$response` was "undefined".
#[test]
fn test_assignment_in_a_yield_from_operand_defines_the_variable() {
    let out = compile_and_run(
        "<?php\nclass R { public function __construct(public string $response) {} }\nclass A {\n    private static function passthru(string $response, R $r, array &$asyncMap): \\Generator {\n        $asyncMap[$response . '!'] = $r;\n        yield $response;\n    }\n    public static function stream(array $rs): \\Generator {\n        $asyncMap = [];\n        foreach ($rs as $r) {\n            $asyncMap[$r->response] = $r;\n            yield from self::passthru($response = $r->response, $r, $asyncMap);\n            if (!isset($asyncMap[$response])) { echo \"missing $response\\n\"; }\n            echo \"after $response \", count($asyncMap), \"\\n\";\n        }\n    }\n}\nforeach (A::stream([new R('a'), new R('b')]) as $v) { echo \"v=$v\\n\"; }\n",
    );
    assert_eq!(out, "v=a\nafter a 2\nv=b\nafter b 4\n");
}

/// `ArrayObject` over one native array: php's next integer key (`['a' => 1]` then `$o[] = 'n'`
/// is key `0`), string keys through `offsetSet()`, `unset()`, `exchangeArray()` (Symfony's
/// `TraceableHttpClient::reset()`), with no leak. The parallel key/value lists it replaced
/// answered key `2`, read every string key as `0`, and leaked eleven blocks on this program.
#[test]
fn test_array_object_keeps_php_array_semantics() {
    let out = compile_and_run_with_heap_debug(
        "<?php\n$o = new ArrayObject(['a' => 1, 'b' => 2]);\n$o[] = 'n'; $o['k'] = 'v'; $o->append('m');\necho json_encode($o->getArrayCopy()), isset($o['k']) ? 'y' : 'n', $o['a'], count($o), \"\\n\";\nunset($o['a']);\nforeach ($o as $k => $v) { echo \"$k=$v,\"; }\n$old = $o->exchangeArray(['x' => 9]);\necho \"\\n\", json_encode($old), json_encode($o->getArrayCopy()), count($o), \"\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"a\":1,\"b\":2,\"0\":\"n\",\"k\":\"v\",\"1\":\"m\"}y15\nb=2,0=n,k=v,1=m,\n{\"b\":2,\"0\":\"n\",\"k\":\"v\",\"1\":\"m\"}{\"x\":9}1\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$this->types[$param - 1] = 'i'` on a `string` property is php's byte-offset write (Doctrine
/// DBAL's mysqli `Statement`); it was refused as an array write on a non-array property.
#[test]
fn test_offset_write_on_a_string_property_replaces_the_byte() {
    let out = compile_and_run(
        "<?php\ndeclare(strict_types=1);\nclass St {\n    private string $types;\n    public function __construct(int $n) { $this->types = str_repeat('s', $n); }\n    public function bind(int $param, int $type): void { $this->types[$param - 1] = $type === 1 ? 'i' : 'b'; }\n    public function t(): string { return $this->types; }\n}\n$s = new St(3); $s->bind(2, 1); $s->bind(3, 2); echo $s->t(), \"\\n\";\n",
    );
    assert_eq!(out, "sib\n");
}

/// Appending to and counting a `string|iterable|null` property (Symfony's `MockResponse::$body`)
/// goes through the boxed writer and the run-time count check.
#[test]
fn test_append_and_count_on_an_iterable_union_property() {
    let out = compile_and_run(
        "<?php\nclass R { public string|iterable|null $body; public function __construct(string|iterable $b = '') { $this->body = $b; } }\n$r = new R([]); $r->body[] = [1, 2]; $r->body[] = 'x'; echo count($r->body), \"\\n\";\n",
    );
    assert_eq!(out, "2\n");
}

/// `stream_get_contents($fh, -1, $this->readBytes[$type])` takes an element of a declared `array`
/// (Symfony Process's `WindowsPipes`), coerced to the int offset at the call.
#[test]
fn test_stream_offset_from_an_untyped_array_element() {
    let out = compile_and_run(
        "<?php\nclass P {\n    private array $readBytes = [1 => 0, 2 => 0];\n    private array $h = [];\n    public function go(): void {\n        $this->h[1] = fopen('php://memory', 'w+');\n        fwrite($this->h[1], 'hello');\n        foreach ($this->h as $t => $fh) {\n            $d = stream_get_contents($fh, -1, $this->readBytes[$t]);\n            $this->readBytes[$t] += strlen($d);\n            echo $d, $this->readBytes[$t], ' ', stream_get_contents($fh, -1, $this->readBytes[$t] - 3), \"\\n\";\n        }\n    }\n}\n(new P())->go();\n",
    );
    assert_eq!(out, "hello5 llo\n");
}

/// `array_map('array_keys', $mappedClasses)` over a hash of arrays (Symfony's
/// `AttributeMetadataPass`), and closures over hashes of floats and objects: the hash path only
/// mapped scalar values, and `array_keys` had no wrapper for a callback by name.
#[test]
fn test_array_map_over_hashes_of_non_scalar_values() {
    let out = compile_and_run(
        "<?php\n$m = ['A' => ['x' => 1, 'y' => 2], 'B' => ['z' => 3]];\necho json_encode(array_map('array_keys', $m)), json_encode(array_map('array_keys', [[1, 2], [3]])), \"\\n\";\n$name = $argc > 5 ? 'count' : 'array_keys';\necho json_encode(array_map($name, $m)), json_encode(array_map(fn ($v) => count($v), $m)), \"\\n\";\necho json_encode(array_map(fn ($v) => $v * 2, ['a' => 1.5, 'b' => 2.25])), \"\\n\";\nclass P { public function __construct(public string $n) {} }\necho json_encode(array_map(fn (P $p) => strtoupper($p->n), ['k' => new P('one'), 'l' => new P('two')])), json_encode($m), \"\\n\";\n",
    );
    assert_eq!(
        out,
        "{\"A\":[\"x\",\"y\"],\"B\":[\"z\"]}[[0,1],[0]]\n{\"A\":[\"x\",\"y\"],\"B\":[\"z\"]}{\"A\":2,\"B\":1}\n{\"a\":3,\"b\":4.5}\n{\"k\":\"ONE\",\"l\":\"TWO\"}{\"A\":{\"x\":1,\"y\":2},\"B\":{\"z\":3}}\n"
    );
}

/// `$h = [&$v]; $h[0]['y'] = 2` writes through the reference element (Doctrine's
/// `UnitOfWork` identity maps): the write-fetch used to detach the marker and write a copy.
#[test]
fn test_nested_write_through_a_reference_element() {
    let out = compile_and_run(
        "<?php\nfunction lst() { $v = ['x' => 1]; $h = [&$v, 2]; $h[0]['y'] = 2; return $v; }\nfunction hsh() { $v = ['x' => 1]; $h = [0 => &$v, 'k' => 2]; $h[0]['y'] = 2; return $v; }\nfunction pushed() { $v = ['x' => 1]; $h = []; $h[] = &$v; $h[0]['y'] = 2; $v['z'] = 3; return [$v, $h[0]]; }\necho json_encode(lst()), json_encode(hsh()), json_encode(pushed()), \"\\n\";\n",
    );
    assert_eq!(
        out,
        "{\"x\":1,\"y\":2}{\"x\":1,\"y\":2}[{\"x\":1,\"y\":2,\"z\":3},{\"x\":1,\"y\":2,\"z\":3}]\n"
    );
}

/// `$t = &$object->n` with an untyped `$object` (Symfony PropertyAccessor's shape): the
/// receiver is only known at run time, so every declaring class's `$n` must be a boxed cell.
/// A raw `int` cell received the box pointer itself, and `echo $a->n` printed an address.
#[test]
fn test_property_reference_through_an_untyped_receiver() {
    let out = compile_and_run(
        "<?php\nclass A { public int $n = 2; }\nclass B { public $n = 'b'; }\nclass C { public array $n = [1]; }\nfunction lit($object, $v) { $t = &$object->n; $t = $v; }\nfunction ret($object) { $t = &$object->n; return $t; }\nfunction one($object) { $t = &$object->n; $t = 9; }\n$a = new A(); $b = new B(); $c = new C();\nlit($a, 9); lit($b, 'w'); lit($c, [7, 8]);\necho $a->n, ' ', $b->n, ' ', json_encode($c->n), ' ', ret($a) + 1, ret($b), \"\\n\";\n$a2 = new A(); one($a2); echo $a2->n, \"\\n\";\n",
    );
    assert_eq!(out, "9 w [7,8] 10w\n9\n");
}

/// `$result[self::REF] = &$object->$name` and `$t = &$object->$name` with an untyped `$object`
/// (Symfony PropertyAccessor:434/451): the class and the name are both only known at run time,
/// so every visible property is a boxed cell found by a class and name ladder. A `stdClass`
/// property is an entry of the instance's hash, made a reference in place (a missing one is
/// created as null, as php's address-of does).
#[test]
fn test_runtime_named_property_reference_through_an_untyped_receiver() {
    let out = compile_and_run(
        "<?php\nclass A { public $u = 1; public int $n = 2; public array $list = []; }\nclass B { public string $s = 'x'; }\nfunction readRef($object, string $name): array { $result = []; $result[0] = $object->$name; $result[1] = &$object->$name; return $result; }\nfunction local($object, string $name) { $t = &$object->$name; $t = is_int($t) ? $t + 10 : (is_array($t) ? [...$t, 'L'] : $t . '!'); }\n$a = new A(); $b = new B();\n$r = readRef($a, 'n'); $r[1] = 7; echo $a->n, \"\\n\";\n$r = readRef($a, 'u'); $r[1] = 'uu'; echo $a->u, \"\\n\";\n$r = readRef($b, 's'); $r[1] = 'yy'; echo $b->s, \"\\n\";\n$r = readRef($a, 'list'); $r[1][] = 'z'; echo json_encode($a->list), \"\\n\";\nlocal($a, 'n'); local($a, 'u'); local($b, 's'); local($a, 'list');\necho $a->n, ' ', $a->u, ' ', $b->s, ' ', json_encode($a->list), \"\\n\";\n$o = new stdClass(); $o->dyn = 5;\nlocal($o, 'dyn'); local($o, 'fresh'); echo json_encode($o), \"\\n\";\n",
    );
    assert_eq!(out, "7\nuu\nyy\n[\"z\"]\n17 uu! yy! [\"z\",\"L\"]\n{\"dyn\":15,\"fresh\":\"!\"}\n");
}

/// php ignores surplus arguments to userland methods and constructors (Symfony's
/// `EmailValidator` calls `isValid($v, false, true)` on a two-parameter method), and a literal
/// `false` for an object parameter is a TypeError only when that call runs. Value-checked
/// against php. Constructors still refuse surplus arguments (bead elephc-1cx.17).
#[test]
fn test_surplus_arguments_to_userland_methods_and_definite_false_mismatch() {
    let out = compile_and_run(
        "<?php\ninterface EV {}\nclass V { public function isValid(string $email, EV $v): bool { return true; } }\nclass U {\n    public function check(V $s, string $value, bool $legacy): string {\n        if ($legacy && !$s->isValid($value, false, true)) { return 'bad'; }\n        return 'ok';\n    }\n}\necho (new U())->check(new V(), 'a@b.c', false), \"\\n\";\ntry { echo (new U())->check(new V(), 'a@b.c', true), \"\\n\"; } catch (TypeError $e) { echo get_class($e), \"\\n\"; }\nclass Counter { public function add(int $n): int { return $n + 1; } }\necho (new Counter())->add(1, 'surplus', [2]), \"\\n\";\n",
    );
    assert_eq!(out, "ok\nTypeError\n2\n");
}

/// `function &node(array &$t, $id) { ...; return $t[$id]; }` (Symfony's
/// `FormDataCollector::buildPreliminaryFormTree`): the returned element is the caller's own hash
/// entry, so writes through `$n = &node(...)` reach the array. It used to return a copy.
#[test]
fn test_by_ref_return_of_a_by_ref_parameter_element() {
    let out = compile_and_run(
        "<?php\nfunction &f(array &$out, string $k) { $out[$k] = ['v' => 1]; return $out[$k]; }\nfunction &g(array &$out) { if (!isset($out['a'])) { $out['a'] = []; } return $out['a']; }\n$h = [];\n$x = &f($h, 'a');\n$x['k'][] = 1;\n$x['v'] = 2;\necho json_encode($h), \"\\n\";\n$y = &g($h);\n$y['z'] = 3;\necho json_encode($h), \"\\n\";\nclass C {\n    private array $tree = [];\n    private function &node(array &$t, string $id) { if (!isset($t[$id])) { $t[$id] = ['children' => []]; } return $t[$id]; }\n    public function go(): void {\n        $n = &$this->node($this->tree, 'root');\n        $n['children']['c1'] = [];\n        $c = &$this->node($n['children'], 'c1');\n        $c['x'] = 1;\n        echo json_encode($this->tree), \"\\n\";\n    }\n}\n(new C())->go();\n",
    );
    assert_eq!(
        out,
        "{\"a\":{\"v\":2,\"k\":[1]}}\n{\"a\":{\"v\":2,\"k\":[1],\"z\":3}}\n{\"root\":{\"children\":{\"c1\":{\"x\":1}}}}\n"
    );
}


/// The ext/standard functions elephc had no implementation of, built by
/// `standard_gap_prelude`: `stripcslashes`, `htmlspecialchars_decode`, `utf8_encode`/`utf8_decode`,
/// `str_increment`/`str_decrement`, `md5_file`/`sha1_file` and `uniqid`. A call compiled
/// silently and raised "Call to undefined function" when it ran (Doctrine's PgSQL
/// `Connection`, Symfony's `MoFileLoader`). Expected output is php 8.5's on the same program.
#[test]
fn test_standard_gap_functions_match_php() {
    let out = compile_and_run_with_heap_debug(
        "<?php\necho bin2hex(stripcslashes(\"\\\\x41\\\\x4g\\\\101\\\\7\\\\0\\\\777\\\\n\\\\q\\\\\")), \"\\n\";\necho htmlspecialchars_decode(\"&amp;&lt;&#039;&#x27;&apos;&quot;&#65;&amp\"), \"|\", htmlspecialchars_decode(\"&#39;&quot;&apos;\", ENT_COMPAT | ENT_HTML5), \"\\n\";\necho bin2hex(@utf8_encode(\"\\xe9a\")), \" \", bin2hex(@utf8_decode(\"\\xc3\\xa9\\xe2\\x82\\xac\\xff\\xc3\")), \"\\n\";\necho str_increment(\"Az\"), str_increment(\"zz\"), str_increment(\"a9\"), \" \", str_decrement(\"Ba\"), str_decrement(\"10\"), str_decrement(\"aa\"), \"\\n\";\ntry { str_decrement(\"a\"); } catch (ValueError $e) { echo $e->getMessage(), \"\\n\"; }\ntry { str_increment(\"a!\"); } catch (ValueError $e) { echo $e->getMessage(), \"\\n\"; }\n$f = sys_get_temp_dir() . \"/elephc_std_gap_\" . getmypid();\nfile_put_contents($f, \"hello\");\necho md5_file($f), \" \", bin2hex(sha1_file($f, true)), \"\\n\";\nunlink($f);\nvar_dump(@md5_file($f));\n$a = uniqid(); $b = uniqid(\"p\", true);\necho strlen($a), strlen($b), $a !== uniqid() ? \" distinct\" : \" same\", ctype_xdigit_free($a) ? \" hex\" : \" nothex\", \"\\n\";\nfunction ctype_xdigit_free(string $s): bool { return strspn($s, \"0123456789abcdef\") === strlen($s); }\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "410467410700ff0a715c\n&<''&apos;\"&#65;&amp|&#39;\"&apos;\nc3a961 e93f3f3f\nBaaaab0 Az9z\nstr_decrement(): Argument #1 ($string) \"a\" is out of decrement range\nstr_increment(): Argument #1 ($string) must be composed only of alphanumeric ASCII characters\n5d41402abc4b2a76b9719d911017c592 aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\nbool(false)\n1324 distinct hex\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A digest is never a view of its input: `md5($s, false)` inside a function kept the
/// source-order lease on `$s` alive as a presumed alias, one leaked block per call.
#[test]
fn test_two_argument_digests_release_their_operand() {
    let out = compile_and_run_with_heap_debug(
        "<?php\nfunction m(string $s): string { return md5($s, false) . sha1($s, true) . hash('crc32b', $s, false); }\nfor ($i = 0; $i < 3; $i++) { echo strlen(m(str_repeat('x', $i + 1))), \"\\n\"; }\n",
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "60\n60\n60\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A method of a class the build does not declare (`new \Spoofchecker()` without ext/intl, symfony/validator's
/// `NoSuspiciousCharactersValidator`) may bind its arguments by reference, so a bare variable passed to
/// it is defined, as php leaves it. It used to be `Undefined variable: $errorCode`, a compile error
/// that handed the whole class file to the runtime; php only raises `Class not found` when it runs.
#[test]
fn test_method_on_a_class_outside_the_build_defines_its_out_arguments() {
    let out = compile_and_run_capture(r#"<?php
class V {
    public function validate(mixed $value): int {
        if ('' === $value = (string) $value) {
            return -1;
        }
        $checker = new \NoSuchChecker();
        $checker->setChecks(4);
        if (!$checker->isSuspicious($value, $errorCode)) {
            return 0;
        }
        return $errorCode & 4;
    }
}
$v = new V();
try { echo $v->validate('abc'), "\n"; } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Error: Class \"NoSuchChecker\" not found\n");
}

/// A union of `Stringable` objects and null handed to `Type|string|null` binds through the string arm in
/// weak mode: each object converts, null stays null, and an object a class member accepts stays one
/// (symfony/ux-live-component's `LiveComponentDebugCommand::displayType()`, given a
/// `ReflectionParameter::getType()` result). The call was a compile error.
#[test]
fn test_stringable_object_union_binds_through_the_string_arm() {
    let out = compile_and_run_capture(r#"<?php
class TypeBox { public function __toString(): string { return 'box'; } }
final class SubBox extends TypeBox {}
final class Label { public function __toString(): string { return 'label'; } }
final class Note { public function __toString(): string { return 'note'; } }
function pick(int $which): Label|Note|null {
    return match ($which) { 0 => new Label(), 1 => new Note(), default => null };
}
function show(TypeBox|string|null $type): string {
    return gettype($type) . ':' . ($type instanceof TypeBox ? 'box-object' : var_export($type, true));
}
foreach ([0, 1, 2] as $which) {
    echo show(pick($which)), "\n";
}
echo show(new SubBox()), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "string:'label'\nstring:'note'\nNULL:NULL\nobject:box-object\n");
}

/// The Reflection type objects are `Stringable`, so `ReflectionParameter::getType()` passes to a
/// `TypeBox|string|null` parameter as its string, like the vendor command that lists live listeners.
#[test]
fn test_reflection_type_union_binds_to_a_string_union() {
    let out = compile_and_run_capture(r#"<?php
final class TypeBox { public function __toString(): string { return 'box'; } }
class Shower {
    public function show(int $x, ?string $y = null): void {}
    private function display(TypeBox|string|null $type): string { return '[' . (string) $type . ']'; }
    public function run(): string {
        $method = new \ReflectionMethod(self::class, 'show');
        $out = [];
        foreach ($method->getParameters() as $parameter) {
            $out[] = $this->display($parameter->getType()) . '$' . $parameter->getName();
        }
        return implode(', ', $out);
    }
}
echo (new Shower())->run(), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[int]$x, [?string]$y\n");
}

/// `ArrayIterator::__serialize()` / `__unserialize()` / `getFlags()` / `setFlags()` (symfony/var-exporter's
/// `Hydrator` restores an ArrayIterator through `__unserialize([$flags, $storage, []])`). The methods
/// did not exist, so the whole Hydrator class file was handed to the runtime.
#[test]
fn test_array_iterator_serializes_and_restores_like_php() {
    let out = compile_and_run_capture(r#"<?php
$it = new ArrayIterator(['a' => 1, 5 => 'x'], ArrayIterator::ARRAY_AS_PROPS);
var_export($it->__serialize()); echo "\n";
$r = new ArrayIterator([7]);
$r->__unserialize([0, ['k' => 'v', 9 => 'w'], []]);
foreach ($r as $k => $v) { echo $k, '=', $v, ' '; }
echo count($r), ' ', $r->getFlags(), "\n";
var_export($r->getArrayCopy()); echo "\n";
$s = new ArrayIterator(['q' => 1]);
$s->__unserialize([2, ['z' => 26], []]);
echo $s->getFlags(), ' ', $s['z'], "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "array (\n  0 => 2,\n  1 => \n  array (\n    'a' => 1,\n    5 => 'x',\n  ),\n  2 => \n  array (\n  ),\n  3 => NULL,\n)\nk=v 9=w 2 0\narray (\n  'k' => 'v',\n  9 => 'w',\n)\n2 26\n");
}

/// A string offset write into a `string` property (doctrine/dbal's mysqli `Statement::bindValue()`:
/// `$this->types[$param - 1] = 's'`) writes the byte in place, past the end pads with spaces, and
/// works through a variable receiver too. The checker refused it although lowering already
/// replayed such a receiver through a hidden local, so the Statement class went to the runtime.
#[test]
fn test_string_offset_write_on_a_string_property() {
    let out = compile_and_run_capture(r#"<?php
final class Statement {
    private string $types;
    public string $label = 'abc';
    public function __construct(int $count) { $this->types = str_repeat('s', $count); }
    public function bind(int $param, string $type): void { $this->types[$param - 1] = $type; }
    public function types(): string { return $this->types; }
}
$s = new Statement(3);
$s->bind(2, 'i');
$s->bind(3, 'b');
echo $s->types(), "\n";
$s->label[5] = '!';
echo $s->label, "|\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "sib\nabc  !|\n");
}

/// A dynamic property read (`$o->$name`) inside an array literal is a boxed value. The literal typed
/// it `int` through the syntactic fallback, stamped `[0 => $zval[0]->$p]` as `array<int, int>` and
/// read the boxed array back as an integer: symfony/property-access's `readProperty()` returned
/// `[0 => 2]` for `[1, 2]`, a silent miscompile on the pre-rebase tree too.
#[test]
fn test_dynamic_property_read_inside_an_array_literal_keeps_its_value() {
    let out = compile_and_run_capture(r#"<?php
final class Bag { public array $items = [1, 2]; }
function a(array $zval, string $p): array { return [0 => $zval[0]->$p]; }
function b(Bag $o, string $p): array { return [0 => $o->$p]; }
function c(Bag $o, string $p): array { $v = $o->$p; return [0 => $v]; }
function d(array $zval, string $p): mixed { return $zval[0]->$p; }
function e(array $zval, string $p): array { return [$zval[0]->$p]; }
echo json_encode(a([new Bag()], 'items')), json_encode(b(new Bag(), 'items')), json_encode(c(new Bag(), 'items')), json_encode(d([new Bag()], 'items')), json_encode(e([new Bag()], 'items')), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[[1,2]][[1,2]][[1,2]][1,2][[1,2]]\n");
}

/// An `iterable` that `is_object()` rejects is an array, so after
/// `if (\is_object($collection)) { $collection = iterator_to_array($collection); }` the haystack of
/// `in_array($item, $collection, true)` is an array on both edges (symfony/property-access's
/// `PropertyAccessor::writeCollection()`, refused with "second argument must be array").
#[test]
fn test_is_object_false_edge_narrows_an_iterable_to_an_array() {
    let out = compile_and_run_capture(r#"<?php
final class Info { public function __construct(private string $name) {} public function getName(): string { return $this->name; } }
final class Bag {
    public array $items = [];
    public function addItem($i): void { $this->items[] = $i; echo "add $i\n"; }
    public function removeItem($i): void { echo "remove $i\n"; }
}
final class Accessor {
    private const VALUE = 0;
    private function readProperty(array $zval, string $property): array { return [self::VALUE => $zval[self::VALUE]->$property]; }
    public function writeCollection(array $zval, string $property, iterable $collection, Info $addMethod, Info $removeMethod): void
    {
        $previousValue = $this->readProperty($zval, $property);
        $previousValue = $previousValue[self::VALUE];
        $removeMethodName = $removeMethod->getName();
        $addMethodName = $addMethod->getName();
        if ($previousValue instanceof \Traversable) {
            $previousValue = iterator_to_array($previousValue);
        }
        if ($previousValue && \is_array($previousValue)) {
            if (\is_object($collection)) {
                $collection = iterator_to_array($collection);
            }
            foreach ($previousValue as $key => $item) {
                if (!\in_array($item, $collection, true)) {
                    unset($previousValue[$key]);
                    $zval[self::VALUE]->$removeMethodName($item);
                }
            }
        } else {
            $previousValue = false;
        }
        foreach ($collection as $item) {
            if (!$previousValue || !\in_array($item, $previousValue, true)) {
                $zval[self::VALUE]->$addMethodName($item);
            }
        }
    }
}
$bag = new Bag();
$bag->items = [1, 2];
(new Accessor())->writeCollection([$bag], 'items', [2, 3], new Info('addItem'), new Info('removeItem'));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "remove 1\nadd 3\n");
}

/// `in_array()` over an `iterable` parameter narrowed by `is_object()`, and php's TypeError for a
/// Traversable haystack.
#[test]
fn test_in_array_on_iterable_parameters_matches_php() {
    let out = compile_and_run_capture(r#"<?php
function diff(array $previous, iterable $collection): array {
    if (\is_object($collection)) {
        $collection = iterator_to_array($collection);
    }
    $kept = [];
    foreach ($previous as $item) {
        if (\in_array($item, $collection, true)) {
            $kept[] = $item;
        }
    }
    return $kept;
}
function probe(iterable $c): string {
    try { return var_export(in_array(1, $c), true); } catch (\TypeError $e) { return $e->getMessage(); }
}
echo implode(',', diff([1, 2, 3], [3, 1])), "\n";
echo implode(',', diff([1, 2, 3], new ArrayIterator([2]))), "\n";
echo probe([1]), "\n";
echo probe(new ArrayIterator([1])), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1,3\n2\ntrue\nin_array(): Argument #2 ($haystack) must be of type array, ArrayIterator given\n");
}

/// `[&$this->pauseExpiry, ..., &$this->remaining, &$this->info]` (symfony/http-client's
/// `NativeResponse::open()`) shares a `float`, a `?int` and an `array` property with the literal:
/// writes through the slots reach the properties, and a by-value destructure copies them. Only
/// `int|float|bool|string` properties could be shared, so the class went to the runtime.
#[test]
fn test_array_literal_references_to_nullable_and_array_typed_properties() {
    let out = compile_and_run_capture(r#"<?php
final class State { public array $openHandles = []; }
final class Response {
    private float $pauseExpiry = 0.0;
    private ?int $remaining = null;
    private array $info = ['http_code' => 0];
    public function __construct(private State $multi) {}
    public function open(): void
    {
        $this->multi->openHandles[1] = [&$this->pauseExpiry, 'h', &$this->remaining, &$this->info];
    }
    public function report(): string
    {
        return json_encode([$this->pauseExpiry, $this->remaining, $this->info]);
    }
}
$state = new State();
$r = new Response($state);
$r->open();
[$pause, , $remaining, $info] = $state->openHandles[1];
$state->openHandles[1][0] = 1.5;
$state->openHandles[1][2] = 42;
$state->openHandles[1][3]['http_code'] = 200;
echo $r->report(), "\n";
echo json_encode([$pause, $remaining, $info]), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[1.5,42,{\"http_code\":200}]\n[0,null,{\"http_code\":0}]\n");
}

/// `filter_var()` with a literal filter and run-time int flags (symfony/http-client's
/// `NativeHttpClient::dnsResolve()` passes `FILTER_FLAG_IPV4` or `FILTER_FLAG_IPV6` to
/// `FILTER_VALIDATE_IP`): the call takes the dynamic dispatch helper, which applies the literal
/// per-filter lowering under each flag value. It was a compile error.
#[test]
fn test_filter_var_with_run_time_int_flags() {
    let out = compile_and_run_capture(r#"<?php
function is_literal_ip(string $host): bool {
    $flag = '' !== $host && '[' === $host[0] && ']' === $host[-1] && str_contains($host, ':') ? \FILTER_FLAG_IPV6 : \FILTER_FLAG_IPV4;
    $ip = \FILTER_FLAG_IPV6 === $flag ? substr($host, 1, -1) : $host;
    return (bool) filter_var($ip, \FILTER_VALIDATE_IP, $flag);
}
foreach (['127.0.0.1', '[::1]', 'example.com', '::1', '[127.0.0.1]', '256.1.1.1'] as $host) {
    echo $host, ' ', var_export(is_literal_ip($host), true), "\n";
}
$flags = $argc > 5 ? 0 : \FILTER_NULL_ON_FAILURE;
var_dump(filter_var('abc', \FILTER_VALIDATE_INT, $flags), filter_var('12', \FILTER_VALIDATE_INT, $flags));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "127.0.0.1 true\n[::1] true\nexample.com false\n::1 false\n[127.0.0.1] false\n256.1.1.1 false\nNULL\nint(12)\n");
}

/// `foreach ($vars as &$v)` over a by-reference variadic promotes `$v` before its first write.
/// The prologue left that slot holding the caller's stack word, so the cell adopted a share of
/// a live block (here `$this->bound`'s default array) and released it: heap-debug reported a
/// bad refcount, and another caller layout corrupted the heap silently.
#[test]
fn test_by_ref_variadic_foreach_starts_from_a_zeroed_value_slot() {
    let out = compile_and_run_with_heap_debug(r#"<?php
final class Holder {
    public array $bound = [];
    public function bind(mixed &...$vars): void {
        foreach ($vars as $i => &$var) { $this->bound[$i] = &$var; }
    }
}
foreach (['first', 'second'] as $how) {
    $h = new Holder(); $a = 0; $b = 0;
    match ($how) { 'first', 'second' => $h->bind($a, $b) };
    $h->bound[0] = 1; $h->bound[1] = 2;
    echo "$how: $a $b\n";
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "first: 1 2\nsecond: 1 2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An unpack into by-reference parameters binds each one to its element (doctrine/dbal
/// Mysqli\Result: `$refs[$k] = &$value; $stmt->bind_result(...$refs)`). Direct instance and
/// static calls were refused at check time; through a callable they bound copies.
#[test]
fn test_spread_of_reference_elements_binds_by_reference_parameters() {
    let out = compile_and_run_with_heap_debug(r#"<?php
final class Statement {
    private array $bound = [];
    public function bindResult(mixed &...$vars): bool {
        foreach ($vars as $i => &$var) { $this->bound[$i] = &$var; }
        return true;
    }
    public function fetch(int $row): void { $this->bound[0] = $row; $this->bound[1] = "r$row"; }
    public static function setAll(mixed &...$vars): void { foreach ($vars as $i => &$var) { $var = "s$i"; } }
    public function setTwo(&$m, &$n): void { $m = "m"; $n = "n"; }
}
final class Result {
    private array $boundValues = [];
    public function __construct(private Statement $statement) {
        $this->boundValues = [null, null];
        $refs = [];
        foreach ($this->boundValues as $key => &$value) { $refs[$key] = &$value; }
        $this->statement->bindResult(...$refs);
    }
    public function row(int $row): string {
        $this->statement->fetch($row);
        return implode(",", $this->boundValues);
    }
}
$result = new Result(new Statement());
echo $result->row(1), "|", $result->row(2), "\n";
$x = 0; $y = 0;
$refs = [&$x, &$y];
Statement::setAll(...$refs);
echo "$x $y\n";
$c = 0; $d = 0;
$pair = [];
$pair[0] = &$c; $pair[1] = &$d;
(new Statement())->setTwo(...$pair);
echo "$c $d\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1,r1|2,r2\ns0 s1\nm n\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// php makes an unpacked element that lands on a by-reference parameter a reference to its
/// source slot: `f(...$plain)` writes `$plain`, only the by-reference suffix of `f($a, &...$rest)`
/// is bound, and a later copy of the array separates like php.
#[test]
fn test_spread_of_plain_elements_into_by_reference_parameters_makes_them_references() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function setAll(mixed &...$vars): void { foreach ($vars as $i => &$var) { $var = "v$i"; } }
function tail($first, &...$rest): void { $first = "changed"; foreach ($rest as &$r) { $r = "r"; } }
final class Binder {
    public static function setAll(mixed &...$vars): void { foreach ($vars as $i => &$var) { $var = "s$i"; } }
    public function named(&$a, &$b): void { $a = "A"; $b = "B"; }
}
$plain = [0, 0];
setAll(...$plain);
echo implode(",", $plain), "\n";
$m = [1, 2, 3];
tail(...$m);
echo implode(",", $m), "\n";
$copy = [0, 0];
setAll(...$copy);
$other = $copy;
$other[0] = "other";
echo implode(",", $copy), " ", implode(",", $other), "\n";
$s = [0, 0];
Binder::setAll(...$s);
echo implode(",", $s), "\n";
$n = ['b' => 1, 'a' => 2];
(new Binder())->named(...$n);
echo $n['a'], $n['b'], "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "v0,v1\n1,r,r\nv0,v1 other,v1\ns0,s1\nAB\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `f(...[&$x, &$y])` (direct and through a callable string) lowered the bare array-reference
/// node and panicked; a declared `array` holding int-keyed hash storage (an int-keyed literal, or a
/// list a by-reference unpack promoted) was sliced as a packed list and crashed.
#[test]
fn test_spread_of_reference_literals_and_int_keyed_hashes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function setAll(mixed &...$vars): void { foreach ($vars as $i => &$var) { $var = "v$i"; } }
function setTwo(&$m, &$n): void { $m = "m"; $n = "n"; }
function count_args(...$xs) { return count($xs) . ":" . implode(",", $xs); }
function relay(array $refs): void {
    setAll(...$refs);
    echo count_args(...$refs), "\n";
}
$p = 0; $q = 0;
setAll(...[&$p, &$q]);
echo "$p $q\n";
$f = 'setAll';
$r = 0; $s = 0;
$f(...[&$r, &$s]);
echo "$r $s\n";
$g = 'setTwo';
$t = 0; $u = 0;
$refs = [&$t, &$u];
$g(...$refs);
echo "$t $u\n";
$e = 0; $h = 0;
relay([&$e, &$h]);
echo "$e $h\n";
echo count_args(...[5 => 1, 9 => 2]), "\n";
$keyed = [3 => "a"];
$keyed[7] = "b";
(function (array $values) { echo count_args(...$values), "\n"; })($keyed);
function forward(array $values): void { echo count_args(...$values), "\n"; }
forward([5 => 1, 9 => 2]);
forward(['x' => 1, 'y' => 2]);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "v0 v1\nv0 v1\nm n\n2:v0,v1\nv0 v1\n2:1,2\n2:a,b\n2:1,2\n2:1,2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// php 8.2+ still CREATES an undeclared property on an ordinary class, after
/// `Deprecated: Creation of dynamic property C::$p is deprecated` (once per object and name):
/// a constructor's `$this->made = ...`, a `??` read-modify-write, an element push, a compound
/// assignment, `isset`/`unset`, and a reference binding all reach the instance's hash, which the
/// program reserves for the classes it creates names on. elephc refused all of them as
/// `Undefined property`.
#[test]
fn test_undeclared_property_creation_on_an_ordinary_class() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Legacy {
    public $declared = 1;
    public function __construct() { $this->made = 'ctor'; }
    public function bump() { $this->count = ($this->count ?? 0) + 1; return $this->count; }
}
class Child extends Legacy {}
$l = new Legacy();
echo $l->made, "\n";
echo $l->bump(), $l->bump(), "\n";
$c = new Child();
$c->extra = [1];
$c->extra[] = 2;
echo json_encode($c), "\n";
var_dump(isset($c->extra), isset($c->none));
unset($c->extra);
echo json_encode($c), "\n";
$c->counter = 5;
$c->counter += 2;
$c->counter++;
echo $c->counter, "\n";
$q = &$l->aliased;
$q = 'via-ref';
echo $l->aliased, "\n";
var_dump($l);
unset($q);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "ctor\n12\n{\"declared\":1,\"made\":\"ctor\",\"extra\":[1,2]}\nbool(true)\nbool(false)\n\
         {\"declared\":1,\"made\":\"ctor\"}\n8\nvia-ref\nobject(Legacy)#1 (4) {\n  [\"declared\"]=>\n  \
         int(1)\n  [\"made\"]=>\n  string(4) \"ctor\"\n  [\"count\"]=>\n  int(2)\n  [\"aliased\"]=>\n  \
         &string(7) \"via-ref\"\n}\n"
    );
    for created in [
        "Legacy::$made", "Legacy::$count", "Child::$made", "Child::$extra", "Child::$counter",
        "Legacy::$aliased",
    ] {
        let notice = format!("Deprecated: Creation of dynamic property {created} is deprecated");
        assert_eq!(out.stderr.matches(&notice).count(), 1, "{notice}\n{}", out.stderr);
    }
    assert!(!out.stderr.contains("on line 4 in "), "location printed twice: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A by-reference method returning an element of an array PROPERTY (doctrine/orm's
/// `UnitOfWork::&getEntityChangeSet()` returns `$this->entityChangeSets[$oid]`) hands back that
/// element itself: a write through the returned reference lands in the property. The backend
/// refused it ("load_prop_ref_cell on non-reference property").
#[test]
fn test_by_reference_return_of_an_array_property_element() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class U {
    private array $cs = [5 => ['a' => 1]];
    private array $deep = ['x' => ['y' => [1]]];
    public function &get(int $o): array {
        $d = [];
        if (!isset($this->cs[$o])) { return $d; }
        return $this->cs[$o];
    }
    public function &deep(): array { return $this->deep['x']['y']; }
    public function dump(): string { return json_encode([$this->cs, $this->deep]); }
}
$u = new U();
$r = &$u->get(5);
$r['b'] = 2;
$miss = &$u->get(9);
$miss['z'] = 1;
$d = &$u->deep();
$d[] = 2;
echo $u->dump(), "\n";
unset($r, $miss, $d);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"5\":{\"a\":1,\"b\":2}},{\"x\":{\"y\":[1,2]}}]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A generator closure with more than sixteen parameters and captures (symfony/http-client's
/// `CachingHttpClient::request()` returns one with nineteen) compiles and runs: the coroutine has
/// 32 start-argument slots. It was refused at 16 and the class went to the interpreter.
#[test]
fn test_generator_closure_with_many_captures() {
    let out = compile_and_run_with_heap_debug(r#"<?php
final class Client {
    private string $tag = 'c';
    public function request(): Closure {
        $a = 1; $b = 2; $c = 3; $d = 4; $e = 5; $f = 6; $g = 7; $h = 8; $i = 9; $j = 10; $k = 11; $l = 12; $m = 13; $n = 14;
        $count = 0;
        return function (string $chunk, array $context) use ($a, $b, $c, $d, $e, $f, $g, $h, $i, $j, $k, $l, $m, $n, &$count): Generator {
            foreach ([$a, $b, $c, $d, $e, $f, $g, $h, $i, $j, $k, $l, $m, $n] as $v) {
                $count++;
                yield "{$this->tag}:$chunk:$v:" . count($context);
            }
            return $count;
        };
    }
}
$make = (new Client())->request();
$gen = $make('x', [1, 2]);
$out = [];
foreach ($gen as $line) { $out[] = $line; }
echo implode(',', $out), "\n", $gen->getReturn(), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "c:x:1:2,c:x:2:2,c:x:3:2,c:x:4:2,c:x:5:2,c:x:6:2,c:x:7:2,c:x:8:2,c:x:9:2,c:x:10:2,c:x:11:2,c:x:12:2,c:x:13:2,c:x:14:2\n14\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `class_alias()` of an INTERFACE (nette/utils' `compatibility.php` aliases `Translator` as
/// `ITranslator` in an `elseif`) compiles: the alias is synthesized as an interface. It was a
/// class extending the interface, which the checker refuses, and the file went to the
/// interpreter.
#[test]
fn test_class_alias_of_an_interface_compiles() {
    let out = compile_and_run(r#"<?php
namespace Nette\Localization {
    interface Translator { function translate($message, ...$parameters): string; }
}
namespace Nette\Localization {
    if (false) {
        interface ITranslator extends Translator {}
    } elseif (!interface_exists(ITranslator::class)) {
        class_alias(Translator::class, ITranslator::class);
    }
}
namespace App {
    final class En implements \Nette\Localization\Translator {
        function translate($message, ...$parameters): string { return "en:$message"; }
    }
    $t = new En();
    var_dump($t instanceof \Nette\Localization\Translator);
    echo $t->translate('hi'), "\n";
}
"#);
    assert_eq!(out, "bool(true)\nen:hi\n");
}

/// `file_get_contents()` accepts a `$context` that is not statically null (league/uri's
/// `Uri::fromPath()` forwards its `resource|null` parameter), as `fopen()` does; the backend
/// refused it at compile time and sent the class to the interpreter.
#[test]
fn test_file_get_contents_with_a_runtime_stream_context() {
    let out = compile_and_run(r#"<?php
$path = __DIR__ . '/fgc_data.txt';
file_put_contents($path, "hello context");
$read = function (string $path, $context): string|false {
    return file_get_contents(filename: $path, context: $context);
};
var_dump($read($path, null));
var_dump($read($path, stream_context_create(['http' => ['method' => 'GET']])));
var_dump(file_get_contents($path, false, stream_context_create(), 6, 7));
unlink($path);
"#);
    assert_eq!(out, "string(13) \"hello context\"\nstring(13) \"hello context\"\nstring(7) \"context\"\n");
}

/// `array_search()` with named arguments takes the same routing as the positional call (nette's
/// `array_search(self::toKey($key), array_keys($array), strict: true)` over a gradual needle was
/// refused by the native search).
#[test]
fn test_array_search_with_named_arguments() {
    let out = compile_and_run(r#"<?php
function toKey(string|int $k): string|int { return $k; }
function g(array $array, string|int $key): ?int {
    $r = array_search(toKey($key), array_keys($array), strict: true);
    return $r === false ? null : $r;
}
var_dump(g(['a' => 1, 'b' => 2], 'b'), g(['a' => 1], 'z'));
var_dump(array_search(strict: false, haystack: [1, '2', 3], needle: 2));
var_dump(array_search(needle: '2', haystack: [1, 2, 3], strict: true));
"#);
    assert_eq!(out, "int(1)\nNULL\nint(1)\nbool(false)\n");
}

/// `min()` / `max()` over string operands compare with php's rules (numeric strings numerically,
/// a direct `min($a, $b)` takes the later of two equal values, `max()` and the array/spread forms
/// keep the first) and keep the string; symfony/http-client's
/// `min($options['http_version'] ?: '1.1', '1.1')` had no lowering at all.
#[test]
fn test_min_max_over_string_operands() {
    let out = compile_and_run(r#"<?php
function v(array $o) { return min($o['http_version'] ?: '1.1', '1.1'); }
var_dump(v(['http_version' => '2.0']), v(['http_version' => '']), v(['http_version' => '1.0']));
function f(string $a, string $b) { return [min($a, $b), max($a, $b)]; }
echo json_encode([f('10', '9'), f('abc', 'abd'), f('10', 'abc')]), "\n";
$a = 3; $b = "3";
var_dump(min($a, $b), max($a, $b), min($b, $a), max($b, $a), max('apple', 5), max('1e1', '9'));
$v = [3, "3"];
var_dump(min(...$v), max(...$v), min($v), max($v));
"#);
    assert_eq!(
        out,
        "string(3) \"1.1\"\nstring(3) \"1.1\"\nstring(3) \"1.0\"\n\
         [[\"9\",\"10\"],[\"abc\",\"abd\"],[\"10\",\"abc\"]]\n\
         string(1) \"3\"\nint(3)\nint(3)\nstring(1) \"3\"\nstring(5) \"apple\"\nstring(3) \"1e1\"\n\
         int(3)\nint(3)\nint(3)\nint(3)\n"
    );
}

/// `stream_set_chunk_size()` accepts a `?int` size (monolog's `SocketHandler` passes its
/// `?int $chunkSize` after a `null ===` guard-and-throw the property type does not narrow); the
/// backend refused the tagged scalar and sent the handler family to the interpreter.
#[test]
fn test_stream_set_chunk_size_with_a_nullable_int_size() {
    let out = compile_and_run(r#"<?php
class H {
    private $resource;
    private ?int $chunkSize = 8192;
    public function __construct() { $this->resource = fopen('php://memory', 'r+'); }
    public function s(): int|bool {
        if (null === $this->chunkSize) { throw new LogicException('x'); }
        return stream_set_chunk_size($this->resource, $this->chunkSize);
    }
}
var_dump((new H)->s());
"#);
    assert_eq!(out, "int(8192)\n");
}

/// PDO_PGSQL registers its pre-8.4 constant spellings on `\PDO` (doctrine/dbal's
/// `PDO\PgSQL\Driver` reads `PDO::PGSQL_ATTR_DISABLE_PREPARES`); elephc's PDO lacked them, so the
/// driver was refused and the Symfony Demo's closed-world build failed.
#[test]
fn test_pdo_pgsql_constant_aliases_on_pdo() {
    let out = compile_and_run(r#"<?php
var_dump(defined('PDO::PGSQL_ATTR_DISABLE_PREPARES'), class_exists('Pdo\Pgsql'));
$a = PHP_VERSION_ID >= 80400 ? Pdo\Pgsql::ATTR_DISABLE_PREPARES : PDO::PGSQL_ATTR_DISABLE_PREPARES;
var_dump($a, PDO::PGSQL_TRANSACTION_IDLE, PDO::PGSQL_TRANSACTION_UNKNOWN);
"#);
    assert_eq!(out, "bool(true)\nbool(true)\nint(1000)\nint(0)\nint(4)\n");
}

/// An array union in a constant expression (`self::OPTIONS_DEFAULTS + [...]`, symfony/http-client
/// constants and property defaults) reflects as php's union: left entries, then right entries
/// under keys the left lacks. Constants failed the Demo's build; property defaults vanished from
/// `getDefaultProperties()`.
#[test]
fn test_reflected_array_unions_in_constants_and_property_defaults() {
    let out = compile_and_run(r#"<?php
interface HttpClientInterface { const OPTIONS_DEFAULTS = ['a' => 1, 'b' => 2, 'extra' => []]; }
final class Curl implements HttpClientInterface {
    const LIST = [1, 2] + [5, 6, 7];
    const MERGED = self::OPTIONS_DEFAULTS + ['b' => 99, 'c' => 3];
    private array $defaultOptions = self::OPTIONS_DEFAULTS + ['crypto' => 7];
    public array $list = [1] + [5, 6];
}
$r = new ReflectionClass('Curl');
echo json_encode($r->getConstants()), "\n";
echo json_encode($r->getDefaultProperties()), "\n";
"#);
    assert_eq!(
        out,
        "{\"LIST\":[1,2,7],\"MERGED\":{\"a\":1,\"b\":2,\"extra\":[],\"c\":3},\"OPTIONS_DEFAULTS\":{\"a\":1,\"b\":2,\"extra\":[]}}\n\
         {\"defaultOptions\":{\"a\":1,\"b\":2,\"extra\":[],\"crypto\":7},\"list\":[1,6]}\n"
    );
}

/// php 8.2 lets a constant expression read an enum case's `->value` / `->name` (monolog's
/// `Logger::DEBUG = Level::Debug->value`); reflecting such constants failed the backend's
/// metadata evaluator and the Symfony Demo's build.
#[test]
fn test_reflected_class_constants_from_enum_case_properties() {
    let out = compile_and_run(r#"<?php
enum Level: int { case Debug = 100; case Info = 200; }
enum Tag: string { case A = 'alpha'; }
enum Pure { case One; }
final class Logger {
    public const DEBUG = Level::Debug->value;
    public const INFO_NAME = Level::Info->name;
    public const TAG = Tag::A->value;
    public const PURE_NAME = Pure::One->name;
    public const LEVELS = [Level::Debug->value => 'debug', Level::Info->value => 'info'];
}
echo json_encode((new ReflectionClass('Logger'))->getConstants()), "\n";
echo Logger::DEBUG, " ", Logger::LEVELS[200], "\n";
"#);
    assert_eq!(
        out,
        "{\"DEBUG\":100,\"INFO_NAME\":\"Info\",\"TAG\":\"alpha\",\"PURE_NAME\":\"One\",\"LEVELS\":{\"100\":\"debug\",\"200\":\"info\"}}\n100 info\n"
    );
}

/// A class constant defined by an identity comparison of scalars (nette/utils'
/// `IsWindows = PHP_OS_FAMILY === 'Windows'`) reflects with php's type-and-value answer; the
/// backend's reflection metadata refused `===` and failed the Symfony Demo's build.
#[test]
fn test_reflected_class_constants_from_identity_comparisons() {
    let out = compile_and_run(r#"<?php
final class Helpers {
    public const IsWindows = PHP_OS_FAMILY === 'Windows';
    public const NotWindows = PHP_OS_FAMILY !== 'Windows';
    public const IntFloat = 1 === 1.0;
    public const Same = 'a' === 'a';
    public const NullSame = null === null;
}
echo json_encode((new ReflectionClass('Helpers'))->getConstants()), "\n";
"#);
    let windows = cfg!(windows);
    assert_eq!(
        out,
        format!(
            "{{\"IsWindows\":{windows},\"NotWindows\":{},\"IntFloat\":false,\"Same\":true,\"NullSame\":true}}\n",
            !windows
        )
    );
}

/// A class constant initialized from a global constant the program never defines
/// (`private const TYPE_BLOB = SQLITE3_BLOB;` in doctrine/dbal's SQLite3 driver) compiles, the
/// class runs, and reflecting its constants raises php's `Error: Undefined constant`. The backend
/// refused the reflection metadata and failed the Symfony Demo's whole build.
#[test]
fn test_class_constant_from_an_undefined_global_constant_reflects_as_php_error() {
    let out = compile_and_run(r#"<?php
final class St {
    public const OK = 7;
    private const TYPE_BLOB = NO_SUCH_EXTENSION_CONSTANT;
    public static function ok(): int { return self::OK; }
}
echo St::ok(), "\n";
try {
    $r = new ReflectionClass('St');
    var_dump($r->getConstants());
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
echo "end\n";
"#);
    assert_eq!(out, "7\nError: Undefined constant \"NO_SUCH_EXTENSION_CONSTANT\"\nend\n");
}

/// An element write, decrement and `unset()` of a property only a SUBCLASS declares, through a
/// receiver typed with the parent class, reach the run-time object's property, as in php.
/// symfony/http-client's `NativeResponse::perform()` does `--$multi->hosts[$host]` and
/// `unset($multi->hosts[$host])` on a `ClientState` whose `NativeClientState` declares `hosts`;
/// the refusal sent the class to the interpreter and broke the Demo's `--closed-world` build.
#[test]
fn test_element_writes_of_a_property_only_a_subclass_declares() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ClientState { public array $openHandles = []; }
class NativeClientState extends ClientState { public array $hosts = []; }
final class R {
    public static function settle(ClientState $multi, ?string $host): void {
        if (null !== $host && isset($multi->hosts[$host]) && 0 >= --$multi->hosts[$host]) {
            unset($multi->hosts[$host]);
        }
    }
    public static function track(ClientState $multi, string $host): void {
        $multi->hosts[$host] = ($multi->hosts[$host] ?? 0) + 1;
        $multi->hosts[] = 'pushed';
    }
}
$s = new NativeClientState();
R::track($s, 'a');
R::track($s, 'a');
R::track($s, 'b');
R::settle($s, 'a');
R::settle($s, 'b');
R::settle($s, null);
echo json_encode($s->hosts), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"a\":1,\"0\":\"pushed\",\"1\":\"pushed\",\"2\":\"pushed\"}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `#[AllowDynamicProperties]` inside a namespace resolves to `N\AllowDynamicProperties`, a
/// user attribute, not the built-in: the class stays an ordinary one, so creating `$x` reports
/// php's deprecation (and still stores the value) instead of being silently allowed.
#[test]
fn test_namespaced_allow_dynamic_properties_lookalike_keeps_the_deprecation() {
    let out = compile_and_run_capture(
        "<?php namespace N; #[AllowDynamicProperties] class Bag {} $b = new Bag(); $b->x = 1; echo $b->x, \"\\n\";",
    );
    assert_eq!(out.stdout, "1\n");
    assert!(
        out.stderr.contains("Deprecated: Creation of dynamic property N\\Bag::$x is deprecated"),
        "{}",
        out.stderr
    );
}

/// A reference to a runtime-named property of a declared class binds that property: a declared
/// slot (every one this class and its subclasses hold), or, on `#[AllowDynamicProperties]`, the
/// entry of the instance's dynamic-property hash, created on first use -- also for a constant
/// undeclared name (`&$a->extra`), which the backend refused. nette/utils' `Arrays` binds
/// `$x = &$x->{$key}` this way.
#[test]
fn test_reference_to_runtime_named_property_of_a_declared_class() {
    let out = compile_and_run_with_heap_debug(r#"<?php
#[AllowDynamicProperties]
class A { public $p = 0; public $q = 'q'; }
class B extends A { public $own = 'o'; }
class Plain { public $p = 0; public $q = 'q'; }
function fill(A $a, array $names): void {
    foreach ($names as $name) {
        $s = &$a->$name;
        $s = is_array($s) ? $s : "set-$name";
        unset($s);
    }
}
$a = new A();
$a->seen = 'before';
$lit = &$a->extra;
$lit = ['x'];
$lit[] = 'y';
$n = 'dyn';
$r = &$a->$n;
$r = [1];
$r[] = 2;
fill($a, ['p', 'q', 'seen']);
echo json_encode($a), "\n";
$b = new B();
fill($b, ['own', 'p', 'extra']);
echo json_encode($b), "\n";
$c = new Plain();
foreach (['p', 'q'] as $name) {
    $u = &$c->$name;
    $u .= '!';
    unset($u);
}
echo json_encode($c), "\n";
unset($lit, $r);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"p\":\"set-p\",\"q\":\"set-q\",\"seen\":\"set-seen\",\"extra\":[\"x\",\"y\"],\"dyn\":[1,2]}\n\
         {\"p\":\"set-p\",\"q\":\"q\",\"own\":\"set-own\",\"extra\":\"set-extra\"}\n\
         {\"p\":\"0!\",\"q\":\"q!\"}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `class_alias()` of an interface ANOTHER FILE declares compiles: nette/utils' classmap sends
/// `ITranslator` to `compatibility.php`, which aliases `Translator` from `Translator.php`. The
/// per-file pass never saw that interface, so the alias stayed a class extending it.
#[test]
fn test_class_alias_of_an_interface_from_another_file_compiles() {
    let out = compile_and_run_files(&[
        ("composer.json", r#"{"autoload":{"classmap":["src/"]}}"#),
        ("src/Translator.php", r#"<?php
namespace Nette\Localization;
interface Translator { function translate($message, ...$parameters): string; }
"#),
        ("src/compatibility.php", r#"<?php
namespace Nette\Localization;
if (false) {
    interface ITranslator extends Translator {}
} elseif (!interface_exists(ITranslator::class)) {
    class_alias(Translator::class, ITranslator::class);
}
"#),
        ("main.php", r#"<?php
final class En implements \Nette\Localization\ITranslator {
    function translate($message, ...$parameters): string { return "en:$message"; }
}
$t = new En();
var_dump($t instanceof \Nette\Localization\Translator, interface_exists(\Nette\Localization\ITranslator::class));
echo $t->translate('hi'), "\n";
"#),
    ], "main.php");
    assert_eq!(out, "bool(true)\nbool(true)\nen:hi\n");
}

/// The interface alias a guarded `class_alias()` synthesizes answers `interface_exists()` after
/// the guard. Its activation event sat at the end of the file, so it answered false until then.
#[test]
fn test_guarded_interface_alias_exists_after_the_guard() {
    let out = compile_and_run(r#"<?php
namespace Nette\Localization {
    interface Translator {}
    if (!interface_exists(ITranslator::class)) {
        class_alias(Translator::class, ITranslator::class);
    }
    var_dump(interface_exists(ITranslator::class));
}
namespace {
    var_dump(interface_exists('Nette\Localization\ITranslator'));
}
"#);
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// Destructuring into a deeper element (`[$query['params'][$j], $ok] = ...`, Doctrine's
/// `DoctrineDataCollector`) parses and writes like the plain nested assignment; the parser
/// accepted one index only, and the whole file went to the interpreter.
#[test]
fn test_destructuring_into_nested_array_elements() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function pair(int $i): array { return ["v$i", $i % 2 === 0, true]; }
$query = ['params' => [10, 20, 30]];
foreach ($query['params'] as $j => $param) {
    [$query['params'][$j], $explainable, $runnable] = pair($j);
    if (!$explainable) { $query['explainable'] = false; }
}
echo json_encode($query), "\n";
$o = new stdClass();
$o->list = [];
[$o->list['a'], $o->list['b']['c']] = [1, 2];
$m = [];
list('x' => $m['k'][0], 'y' => $m['k'][1]) = ['x' => 'X', 'y' => 'Y'];
[$m['k'][], $m['n'][]] = ['Z', 'N'];
echo json_encode($o), json_encode($m), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"params\":[\"v0\",\"v1\",\"v2\"],\"explainable\":false}\n\
         {\"list\":{\"a\":1,\"b\":{\"c\":2}}}{\"k\":[\"X\",\"Y\",\"Z\"],\"n\":[\"N\"]}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `static` local handed out by reference (by-ref argument, `&` alias, `use (&$x)`) keeps every
/// write across calls: Monolog's `BrowserConsoleHandler::handleStyle` shape.
#[test]
fn test_static_locals_bound_by_reference_persist_their_writes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function add(array &$a, string $k): void { $a[$k] = count($a); }
function f(string $k): string {
    static $labels = [];
    add($labels, $k);
    $r = &$labels;
    $r['last'] = $k;
    return json_encode($labels);
}
echo f('a'), "\n", f('b'), "\n";
function g(string $s): string {
    static $seen = [];
    $cb = function () use ($s, &$seen) { $seen[$s] = ($seen[$s] ?? 0) + 1; return $seen[$s]; };
    return $cb() . ':' . json_encode($seen);
}
echo g('x'), "\n", g('x'), "\n", g('y'), "\n";
class Console {
    public static function style(string $s): string {
        static $colors;
        $colors ??= [];
        $p = &$colors;
        $p[$s] ??= count($p) + 1;
        return $s . '=' . $colors[$s];
    }
}
echo Console::style('a'), ' ', Console::style('b'), ' ', Console::style('a'), "\n";
function h(): int { static $n = 0; $m = &$n; return ++$m; }
echo h(), h(), h(), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"a\":0,\"last\":\"a\"}\n{\"a\":0,\"last\":\"b\",\"b\":2}\n1:{\"x\":1}\n2:{\"x\":2}\n\
         1:{\"x\":2,\"y\":1}\na=1 b=2 a=1\n123\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
