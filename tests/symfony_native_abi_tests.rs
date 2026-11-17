//! Purpose:
//! Pins mixed-value ABI boundaries reached by Symfony's native startup paths.
//!
//! Called from:
//! - The CLI integration test harness, compared against the installed PHP executable.
//!
//! Key details:
//! - Tests exercise real generated binaries and clean their own isolated project directories.
//! - The include fixture preloads a value-returning unit and reads its path from a mixed hash.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "support/managed_pcre2.rs"]
mod managed_pcre2;

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

/// MARK and capture strings use the runtime string-result ABI before persistence.
#[test]
fn symfony_native_abi_regex_capture_strings_and_marks() {
    assert_php_native_regex(r#"<?php
$pattern = '{^(?|/greet/(?<name>[^/]++)(*:22))/?$}sDu';
foreach ([0, PREG_OFFSET_CAPTURE, PREG_UNMATCHED_AS_NULL] as $flags) {
    $matches = [];
    echo preg_match($pattern, '/greet/World', $matches, $flags), ':', json_encode($matches), "\n";
}
$all = [];
echo preg_match_all('~(?<word>[a-z]+)(*:77)~', 'one two', $all, PREG_SET_ORDER), ':', json_encode($all), "\n";
"#);
}

/// Gradual hash merges release their input snapshots without freeing the returned hash.
#[test]
fn symfony_native_abi_gradual_hash_merge_releases_inputs() {
    assert_php_native(r#"<?php
class NativeAttributeBag {
    public function __construct(protected array $parameters = []) {}
    public function add(array $parameters): void {
        $this->parameters = array_replace($this->parameters, $parameters);
    }
    public function set(string $key, mixed $value): void { $this->parameters[$key] = $value; }
    public function all(): array { return $this->parameters; }
}
function routeValues(): array { return ['_route' => 'hello', '_controller' => 'Home::home']; }
$bag = new NativeAttributeBag();
$parameters = routeValues();
$attributes = $parameters;
$bag->add($attributes);
echo json_encode($bag->all()), "\n";
unset($parameters['_route'], $parameters['_controller']);
$bag->set('_route_params', $parameters);
echo json_encode($bag->all()), "\n";
function mergeGradual(mixed $left, mixed $right): mixed { return array_merge($left, $right); }
function replaceGradual(mixed $left, mixed $right): mixed { return array_replace_recursive($left, $right); }
echo json_encode(mergeGradual(['a' => [1]], ['b' => [2]])), "\n";
echo json_encode(replaceGradual(['a' => ['x' => 1]], ['a' => ['y' => 2]])), "\n";
"#, false, false);
}

/// Owns one isolated project until its PHP and native results have been compared.
struct Fixture(PathBuf);

impl Fixture {
    /// Writes a fresh entry file and optional preloaded include unit.
    fn new(source: &str, include_unit: bool) -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("symfony_native_abi_{}_{id}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("main.php"), source).unwrap();
        fs::write(root.join("decl.php"), "<?php class NativeAbiDecl { public static function value(): string { return 'declared'; } }").unwrap();
        if include_unit {
            fs::write(root.join("value.php"), "<?php return 'native unit';").unwrap();
            fs::write(root.join("preload.php"), "<?php opcache_compile_file(__DIR__.'/value.php');").unwrap();
        }
        Self(root)
    }
}

impl Drop for Fixture {
    /// Removes only the directory this test created, including failed build artifacts.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Compiles a fixture and compares its actual exit status and stdout with PHP.
fn assert_php_native(source: &str, include_unit: bool, closed_world: bool) {
    assert_php_native_with_capabilities(source, include_unit, closed_world, false);
}

/// Compares a regex fixture using the repository's isolated managed-PCRE2 provider.
fn assert_php_native_regex(source: &str) {
    assert_php_native_with_capabilities(source, false, false, true);
}

/// Selects only the runtime capabilities needed by one real CLI fixture.
fn assert_php_native_with_capabilities(source: &str, include_unit: bool, closed_world: bool, regex: bool) {
    let fixture = Fixture::new(source, include_unit);
    let php = Command::new("php").args(["-n", "main.php"]).current_dir(&fixture.0)
        .output().expect("run PHP reference");
    assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
    let compiler = std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        if path.ends_with("deps") { path.pop(); }
        path.join("elephc").to_string_lossy().into_owned()
    });
    let mut command = Command::new(compiler);
    command.current_dir(&fixture.0);
    if regex {
        managed_pcre2::configure_host_managed_pcre2(&mut command, &fixture.0);
    }
    if closed_world {
        command.env("ELEPHC_CLOSED_WORLD", "strict");
    }
    if include_unit {
        command.args(["--ini", "opcache.enable_cli=1", "--ini"])
            .arg(format!("opcache.preload={}", fixture.0.join("preload.php").display()));
    }
    let compiled = command.arg("main.php").output().expect("compile ABI fixture");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let native = Command::new(fixture.0.join("main")).current_dir(&fixture.0)
        .output().expect("execute ABI fixture");
    assert!(native.status.success(), "status {:?}, stdout {}, stderr {}", native.status,
        String::from_utf8_lossy(&native.stdout), String::from_utf8_lossy(&native.stderr));
    assert_eq!(native.stdout, php.stdout);
}

/// Array union must unbox the mixed left operand rather than the last materialized right one.
#[test]
fn symfony_native_abi_mixed_array_union_uses_runtime_value_register() {
    assert_php_native(
        "<?php function options(): mixed { return ['base' => 1]; } $options = options(); $options += ['extra' => 2]; echo json_encode($options), PHP_EOL;",
        false,
        false,
    );
}

/// A boxed include path keeps the string length returned by the runtime unbox helper.
#[test]
fn symfony_native_abi_boxed_include_path_keeps_payload_length() {
    assert_php_native(
        "<?php function paths(): array { return ['file' => __DIR__.'/value.php', 'other' => 42]; } $paths = paths(); $path = $paths['file']; echo require $path;",
        true,
        false,
    );
}

/// Closed-world include dispatch uses the same unboxed path contract.
#[test]
fn symfony_native_abi_closed_include_keeps_payload_length() {
    assert_php_native(
        "<?php function paths(): array { return ['file' => __DIR__.'/value.php', 'other' => 42]; } $paths = paths(); $path = $paths['file']; echo require $path;",
        true,
        true,
    );
}

/// A compiled once-include reached through the bridge receives the actual string length.
#[test]
fn symfony_native_abi_boxed_compiled_include_bridge_keeps_payload_length() {
    assert_php_native(
        "<?php require_once __DIR__.'/decl.php'; function paths(): array { return ['file' => __DIR__.'/decl.php', 'other' => 42]; } $paths = paths(); $path = $paths['file']; require_once $path; echo NativeAbiDecl::value();",
        false,
        false,
    );
}

/// A boxed named object survives the nominal predicate before payload unboxing.
#[test]
fn symfony_native_abi_nominal_object_unbox_preserves_source_cell() {
    assert_php_native(
        "<?php class AbiNominalTarget { public function value(): string { return 'target'; } } function abiNominalBox(): ?AbiNominalTarget { return new AbiNominalTarget(); } $value = abiNominalBox() ?? new AbiNominalTarget(); echo $value->value(), PHP_EOL;",
        false,
        false,
    );
}

/// Interface narrowing preserves the source while a null arm still selects the fallback.
#[test]
fn symfony_native_abi_nominal_object_unbox_accepts_interface_and_null_fallback() {
    assert_php_native(
        "<?php interface AbiNominalContract { public function value(): string; } class AbiNominalTarget implements AbiNominalContract { public function value(): string { return 'target'; } } function abiNominalBox(bool $present): ?AbiNominalContract { return $present ? new AbiNominalTarget() : null; } $first = abiNominalBox(true) ?? new AbiNominalTarget(); $second = abiNominalBox(false) ?? new AbiNominalTarget(); echo $first->value(), ':', $second->value(), PHP_EOL;",
        false,
        false,
    );
}

/// A closure-backed aggregate keeps its typed iterator alive after releasing the Mixed result.
#[test]
fn symfony_native_abi_nominal_return_retains_iterator_payload() {
    assert_php_native(
        r#"<?php
class AbiPlainIterator implements Iterator {
    private int $index = 0;
    public function current(): mixed { return $this->index; }
    public function key(): mixed { return $this->index; }
    public function next(): void { $this->index++; }
    public function rewind(): void { $this->index = 0; }
    public function valid(): bool { return $this->index < 2; }
}
class AbiFactoryAggregate implements IteratorAggregate {
    public function __construct(private Closure $factory) {}
    public function getIterator(): Traversable { return ($this->factory)(); }
}
function abiWalk(iterable $values): void {
    foreach ($values as $key => $value) { echo $key, ':', $value, '|'; }
    echo PHP_EOL;
}
$aggregate = new AbiFactoryAggregate(fn() => new AbiPlainIterator());
abiWalk($aggregate);
foreach ($aggregate as $value) { echo $value; }
echo PHP_EOL;
abiWalk(new AbiFactoryAggregate(function () { yield 4; yield 5; }));
"#,
        false,
        false,
    );
}

/// A coalescing assignment receiver probes a missing WeakMap key before reading it.
#[test]
fn symfony_runtime_weakmap_coalescing_assignment_receiver() {
    assert_php_native(
        "<?php class Holder { private static ?WeakMap $values = null; public static function read(object $key): array { return (self::$values ??= new WeakMap())[$key] ?? []; } } echo json_encode(Holder::read(new stdClass())), PHP_EOL;",
        false,
        false,
    );
}

/// A boxed builtin WeakMap requires the ArrayAccess existence path even without user implementations.
#[test]
fn symfony_runtime_mixed_weakmap_missing_key_isset_is_silent() {
    assert_php_native(
        "<?php function inspectMap(mixed $map, object $key): void { var_dump(isset($map[$key])); } inspectMap(new WeakMap(), new stdClass());",
        false,
        false,
    );
}

/// A factory receiver is evaluated once and its denied offset is never fetched.
#[test]
fn symfony_runtime_arrayaccess_factory_coalesce_calls_exists_once() {
    assert_php_native(
        "<?php class Probe implements ArrayAccess { public function offsetExists(mixed $key): bool { echo 'exists|'; return false; } public function offsetGet(mixed $key): mixed { throw new Exception('unexpected get'); } public function offsetSet(mixed $key, mixed $value): void {} public function offsetUnset(mixed $key): void {} } function factory(): Probe { echo 'factory|'; return new Probe(); } echo factory()['missing'] ?? 'fallback', PHP_EOL;",
        false,
        false,
    );
}

/// Empty matches preserve unmatched bytes, include the terminal match, and retry nonempty alternatives.
#[test]
fn symfony_runtime_preg_replace_empty_match_continuation() {
    assert_php_native_regex(
        r#"<?php foreach (['/^/', '/$/', '/x*/', '/(?:|a)/', '/(?=.)/u'] as $pattern) { echo json_encode([preg_replace($pattern, '-', 'ab', -1, $count), $count]), PHP_EOL; } echo json_encode([preg_replace('/^/', '-', '', -1, $count), $count]), PHP_EOL; echo json_encode([preg_replace('/x*/u', '-', 'é', -1, $count), $count]), PHP_EOL; echo json_encode([preg_replace('/x*/', '-', 'ab', 1, $count), $count]), PHP_EOL;"#,
    );
}

/// Inline hash values tagged as Mixed pass their cell pointer into string coercion.
#[test]
fn symfony_native_abi_strtr_unwraps_boxed_replacement() {
    assert_php_native(
        "<?php function boxedReplacement(mixed $value): mixed { return $value; } foreach (['World', 42, 1.5, true, false, null] as $value) { echo '[', strtr('value=%v', ['%v' => boxedReplacement($value)]), ']', PHP_EOL; }",
        false,
        false,
    );
}

/// A nullable reference output keeps its boxed representation across an uninitialized branch.
#[test]
fn symfony_native_abi_nullable_reference_output_preserves_false_and_null() {
    assert_php_native(
        r#"<?php
class AbiReferenceProbe {
    public function cleanup(string $value, ?bool &$marker = null): string {
        $marker = $value === '' ? null : false;
        return $value;
    }
}
function abiReferenceBranch(string|array $value): void {
    if (is_array($value)) { $marker = false; }
    else { $value = (new AbiReferenceProbe())->cleanup($value, $marker); }
    echo json_encode([$marker, (bool) $marker]), PHP_EOL;
}
abiReferenceBranch('yaml');
abiReferenceBranch('');
abiReferenceBranch([]);
"#,
        false,
        false,
    );
}

/// Interface lookup preserves late integer arguments and the hidden backtrace collector.
#[test]
fn symfony_native_abi_interface_dispatch_preserves_late_arguments() {
    assert_php_native(
        r#"<?php
interface AbiBeforeWide {}
interface AbiWideCall { public function join(int $a, int $b, int $c, int $d, int $e): string; }
class AbiWideTarget implements AbiBeforeWide, AbiWideCall {
    public function join(int $a, int $b, int $c, int $d, int $e): string { return implode(':', [$a, $b, $c, $d, $e]); }
}
function abiWideCaller(AbiWideCall $target): void { echo $target->join(1, 2, 3, 4, 5), PHP_EOL; }
interface AbiTraceCall { public function handle(object $request, int $type = 1, bool $catch = true): string; }
class AbiTraceTarget implements AbiTraceCall {
    public function handle(object $request, int $type = 1, bool $catch = true): string {
        try { throw new RuntimeException('caught'); }
        catch (Throwable $error) { return count($error->getTrace()).':'.count(func_get_args()); }
    }
}
function abiTraceCaller(AbiTraceCall $target, object $request): void { echo $target->handle($request), PHP_EOL; }
abiWideCaller(new AbiWideTarget());
abiTraceCaller(new AbiTraceTarget(), new stdClass());
"#,
        false,
        false,
    );
}

/// Callback replacement follows the same empty-match continuation contract.
#[test]
fn symfony_runtime_preg_replace_callback_empty_match_continuation() {
    assert_php_native_regex(
        r#"<?php foreach (['/^/', '/$/', '/x*/', '/(?:|a)/', '/(?=.)/u'] as $pattern) { echo json_encode([preg_replace_callback($pattern, fn($m) => '-', 'ab', -1, $count), $count]), PHP_EOL; } echo json_encode([preg_replace_callback('/^/', fn($m) => '-', '', -1, $count), $count]), PHP_EOL; echo json_encode([preg_replace_callback('/x*/u', fn($m) => '-', 'é', -1, $count), $count]), PHP_EOL;"#,
    );
}
