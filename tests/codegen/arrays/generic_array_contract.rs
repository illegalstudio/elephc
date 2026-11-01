//! Purpose:
//! Integration tests for PHP's bare `array` type contract when the value crossing it is an
//! associative array (a HASH) rather than an indexed list.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - PHP has ONE array type; elephc splits it into an indexed vector (`Array`) and a hash
//!   (`AssocArray`). A bare `array` hint resolves to `Array(Mixed)` -- the INDEXED repr -- so every
//!   boundary that admits both shapes has to pick one. Picking indexed is not an option: the
//!   coercions retype the hash without converting it (converting would discard the string keys),
//!   and every consumer that reads the static type then misreads the container.
//! - The join is therefore the HASH, the same rule
//!   `types::array_storage::join_array_storage_conversion` already states for local storage
//!   transitions: a hash represents any PHP array, a packed vector cannot. It costs nothing
//!   observable -- a hash holding sequential integer keys is indistinguishable from a list to
//!   every consumer, which the `..._still_encodes_as_a_json_list` test pins.
//! - Every expected value is verbatim `php -n` 8.5.10 output.
//! - Three boundaries are covered, because each had its own wrong join: a method/function RETURN
//!   (`Checker::generic_array_return_contract`), a declared `array` PARAMETER
//!   (`respecialized_param_types_for_call`), and an untyped PROPERTY
//!   (`merge_untyped_property_array_storage`).

use crate::support::*;

/// Verifies a method declared `: array` that returns a hash on one path and `[]` on another hands
/// the hash back as a hash.
///
/// This is Symfony's `HeaderBag::all()`. Before the fix `json_encode()` printed `[14,15]` --
/// the hash's raw slot words read as a packed vector -- and the compiled `--web` 404 response
/// emitted five malformed numeric headers (`10000:`, `0:`, `-128:`, …, listener priorities read
/// out of hash buckets) while dropping `Cache-Control` and `Content-Type` entirely.
#[test]
fn test_generic_array_return_keeps_hash_storage_across_an_empty_literal_path() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    protected array $headers = [];
    public bool $pick = false;
    public function __construct(array $headers = []) {
        foreach ($headers as $key => $value) { $this->headers[$key] = (array) $value; }
    }
    public function all(): array {
        if ($this->pick) { return []; }
        return $this->headers;
    }
}
$bag = new Bag(['x-foo' => 'bar', 'x-baz' => 'qux']);
echo json_encode($bag->all()), '|';
$bag->pick = true;
echo json_encode($bag->all());
"#,
    );
    assert_eq!(out, "{\"x-foo\":[\"bar\"],\"x-baz\":[\"qux\"]}|[]");
}

/// Verifies the same when the other path returns a non-empty INDEXED list, which has to be
/// CONVERTED to hash storage rather than left packed.
///
/// `Op::ArrayToHash` carries the element type across unchanged, so a vector of raw slots is boxed
/// through `Op::ArrayToMixed` first; without that the return coercion reported `unsupported EIR
/// backend feature: runtime_call from PHP type Array(Str) to PHP type AssocArray`.
#[test]
fn test_generic_array_return_converts_an_indexed_path_to_hash_storage() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    protected array $headers = [];
    public bool $pick = false;
    public function __construct(array $headers = []) {
        foreach ($headers as $key => $value) { $this->headers[$key] = (array) $value; }
    }
    public function all(): array {
        if ($this->pick) { return ['a', 'b']; }
        return $this->headers;
    }
}
$bag = new Bag(['x-foo' => 'bar']);
echo json_encode($bag->all()), '|';
$bag->pick = true;
echo json_encode($bag->all());
"#,
    );
    assert_eq!(out, "{\"x-foo\":[\"bar\"]}|[\"a\",\"b\"]");
}

/// Verifies a `Mixed` return path -- what `$this->h[$k] ?? []` boxes into -- does not out-vote the
/// hash.
///
/// A boxed value names no storage, so it cannot argue AGAINST the hash: whatever it holds is
/// unboxed into the contract, while a hash handed to an indexed contract is misread outright.
/// This is `HeaderBag::all(?string $key = null)` exactly, including the `get()` that reads `[0]`
/// off the result.
#[test]
fn test_generic_array_return_prefers_hash_over_a_boxed_coalesce_path() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    protected array $headers = [];
    public function __construct(array $headers = []) {
        foreach ($headers as $key => $value) { $this->headers[$key] = (array) $value; }
    }
    public function all(?string $key = null): array {
        if (null !== $key) { return $this->headers[strtolower($key)] ?? []; }
        return $this->headers;
    }
    public function get(string $key, ?string $default = null): ?string {
        $headers = $this->all($key);
        if (!$headers) { return $default; }
        if (null === $headers[0]) { return null; }
        return (string) $headers[0];
    }
}
$bag = new Bag(['x-foo' => 'bar', 'x-baz' => 'qux']);
echo json_encode($bag->all()), '|';
echo json_encode($bag->all('x-foo')), '|';
echo var_export($bag->get('x-foo'), true), '|';
echo var_export($bag->get('nope'), true), '|';
$n = 0;
foreach ($bag->all() as $name => $values) { $n++; }
echo $n;
"#,
    );
    assert_eq!(
        out,
        "{\"x-foo\":[\"bar\"],\"x-baz\":[\"qux\"]}|[\"bar\"]|'bar'|NULL|2",
    );
}

/// Verifies a hash passed through a declared bare `array` PARAMETER stays a hash for every
/// consumer, including the ones that used to fault.
///
/// Measured before the fix on this exact program: `json_encode` printed `[12,5]`, `serialize`
/// wrote `a:2:{i:0;i:12;i:1;i:5;}`, `print_r` showed `Array([0]=>12[1]=>5)`, and `in_array`
/// terminated the process with SIGSEGV.
#[test]
fn test_generic_array_parameter_keeps_hash_storage_for_every_consumer() {
    let out = compile_and_run(
        r#"<?php
function pass(array $h): array { return $h; }
$h = pass(['a' => 1, 'b' => 2]);
echo count($h), '|';
echo implode(',', array_keys($h)), '|';
echo implode('|', $h), '|';
echo json_encode($h), '|';
echo serialize($h), '|';
echo str_replace(["\n", ' '], '', print_r($h, true)), '|';
echo str_replace(["\n", ' '], '', var_export($h, true)), '|';
echo in_array(2, $h) ? 'y' : 'n', '|';
echo array_key_exists('b', $h) ? 'y' : 'n', '|';
echo array_sum($h), '|';
echo json_encode(array_filter($h, fn ($v) => $v > 1)), '|';
echo max($h);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "2|a,b|1|2|{\"a\":1,\"b\":2}|a:2:{s:1:\"a\";i:1;s:1:\"b\";i:2;}|",
            "Array([a]=>1[b]=>2)|array('a'=>1,'b'=>2,)|y|y|3|{\"b\":2}|2"
        ),
    );
}

/// Verifies an INDEXED argument still arrives intact at a parameter the hash rule widened.
///
/// Once one call site passes a hash the parameter's contract IS the hash, so a list argument from
/// another call site must be CONVERTED. Leaving it packed handed the callee a pointer to the wrong
/// layout: this program printed `{"":0,"":992}` for the list.
#[test]
fn test_generic_array_parameter_converts_a_list_argument_to_the_widened_contract() {
    let out = compile_and_run(
        r#"<?php
function dump(array $h): string { return json_encode($h); }
echo dump(['a' => 1, 'b' => 2]), '|';
echo dump([1, 2]), '|';
echo dump([]);
"#,
    );
    assert_eq!(out, "{\"a\":1,\"b\":2}|[1,2]|[]");
}

/// Verifies an untyped property written with both array shapes keeps hash storage.
///
/// `merge_untyped_property_array_storage` answered `array<mixed>` for the cross-shape case, on the
/// belief that it was a runtime-dispatched "either". It is not -- consumers read the static type,
/// so the hash written by the second seed was read back as a packed vector.
///
/// The reader declares `: array` deliberately. An UNTYPED method returning an untyped property
/// that holds a hash is refused by the checker (`count() argument must be array or Countable
/// object`) whether or not the two shapes ever meet, so leaving the hint off would test that
/// separate, pre-existing gap instead of this one.
#[test]
fn test_untyped_property_written_with_both_array_shapes_keeps_hash_storage() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    private $slot = [];
    public function seedList(): void { $this->slot = ['x', 'y']; }
    public function seedMap(): void { $this->slot = ['a' => 1, 'b' => 2]; }
    public function read(): array { return $this->slot; }
}
$holder = new Holder();
$holder->seedList();
echo json_encode($holder->read()), '|';
$holder->seedMap();
$got = $holder->read();
echo count($got), '|', json_encode($got), '|';
echo in_array(2, $got) ? 'y' : 'n';
"#,
    );
    assert_eq!(out, "[\"x\",\"y\"]|2|{\"a\":1,\"b\":2}|y");
}

/// Pins the reason the hash is a safe join: a hash whose keys are sequential integers from zero is
/// indistinguishable from a list to every consumer, exactly as in reference PHP.
///
/// Without this, "widen to the hash" would look like it changes `json_encode`'s array-vs-object
/// choice, which is what makes the indexed repr seem necessary.
#[test]
fn test_a_hash_with_sequential_integer_keys_still_encodes_as_a_json_list() {
    let out = compile_and_run(
        r#"<?php
$h = ['seed' => 1];
unset($h['seed']);
$h[0] = 'a';
$h[1] = 'b';
echo json_encode($h), '|';
echo serialize($h), '|';
$gap = ['seed' => 1];
unset($gap['seed']);
$gap[0] = 'a';
$gap[2] = 'b';
echo json_encode($gap);
"#,
    );
    assert_eq!(
        out,
        "[\"a\",\"b\"]|a:2:{i:0;s:1:\"a\";i:1;s:1:\"b\";}|{\"0\":\"a\",\"2\":\"b\"}",
    );
}

/// A bare `array` parameter stored into one of its own elements (`$context['_parent'] =
/// $context;`, the first statement of every loop in a compiled Twig template). The write detaches
/// the boxed parameter into a clone and releases the box the slot held -- which the right-hand
/// side was still borrowing, so the write stored a freed box: "heap debug detected bad refcount",
/// and Symfony's `/` route died. Plain, generator and local forms; php's output, clean heap.
#[test]
fn test_array_parameter_stored_into_its_own_element_keeps_the_copy() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function plain(array $context): int {
    $context['_parent'] = $context;
    $context['loop'] = ['parent' => $context['_parent'], 'index' => 1];
    return count($context) + count($context['_parent']) + $context['loop']['index'];
}
function gen(array $context): iterable {
    yield 'start';
    $context['_parent'] = $context;
    $context['copy'] = $context['greeting'];
    yield count($context) . ':' . count($context['_parent']) . ':' . $context['copy'];
}
function local(): int {
    $context = ['a' => 1, 'b' => 2];
    $context['_parent'] = $context;
    return count($context) + count($context['_parent']);
}
$ctx = ['greeting' => 'Hello', 'words' => ['alpha', 'beta']];
echo local(), "|", plain($ctx), "|";
foreach (gen($ctx) as $v) { echo $v, "|"; }
echo json_encode($ctx);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "5|7|start|4:2:Hello|{\"greeting\":\"Hello\",\"words\":[\"alpha\",\"beta\"]}"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
