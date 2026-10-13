//! Purpose:
//! Tests PHP by-value copies of boxed values across bindings and user-call boundaries.
//!
//! Called from:
//! - `cargo test --test codegen_tests mixed_value_copies`.
//!
//! Key details:
//! - Array copies must detach zval cells while reference parameters remain aliased.
//! - Objects and resources preserve identity; every normal or exceptional owner is released.
//! - ELEPHC_PHP_CHECK makes this group require a successful, matching PHP execution.

use crate::support::*;

/// Retiring the last foreach alias cannot free the reference cell still owned by its hash entry.
#[test]
fn mixed_value_copies_foreach_alias_unset_preserves_entry_through_allocation() {
    assert_clean_copy(r#"
function readReferenceItems(mixed $items): void {
    foreach ($items as $element) { echo $element; }
}
$source = ["u" => 1];
foreach ($source as &$value) {}
unset($value);
echo json_encode($source), '|';
readReferenceItems($source);
echo '|', json_encode($source);
unset($source);
"#, "{\"u\":1}|1|{\"u\":1}");
}

/// Empty indexed arrays preserve PHP null tags when appending literal or void-call results.
#[test]
fn mixed_value_copies_null_only_append_preserves_runtime_tags() {
    assert_clean_copy(r#"
function voidAppendPayload(): void {}
$values = [];
$values[] = null;
$values[] = voidAppendPayload();
echo json_encode($values), '|', gettype($values[0]), '|', gettype($values[1]);
unset($values);
"#, "[null,null]|NULL|NULL");
}

/// Inline ReflectionClass construction keeps the concrete receiver for authorized private reflection.
#[test]
fn mixed_value_copies_inline_reflection_factory_keeps_private_property_authority() {
    assert_clean_copy(r#"
class InlineReflectedOwner {
    private int $value = 1;
    public function value(): int { return $this->value; }
}
$owner = (new ReflectionClass(InlineReflectedOwner::class))->newInstanceWithoutConstructor();
(new ReflectionProperty(InlineReflectedOwner::class, 'value'))->setValue($owner, 7);
echo $owner->value();
unset($owner);
"#, "7");
}

/// A nested call result remains owned until a throwing capture is retired and unwound.
#[test]
fn mixed_value_copies_nested_result_is_not_discarded_before_callback_cleanup() {
    assert_clean_copy(r#"
class ThrowingNestedCapture { public function __destruct() { echo 'capture|'; throw new RuntimeException('capture'); } }
class NestedResult { public function __destruct() { echo 'result|'; } }
function dropNestedHolder(array &$slot): int { $slot = []; return 1; }
function consumeNestedResult(NestedResult $result): void { echo 'used|'; }
function exerciseNestedResult(): void {
    $held = [new ThrowingNestedCapture()];
    try {
        consumeNestedResult((function (int $ignored) use ($held): NestedResult { return new NestedResult(); })(dropNestedHolder($held)));
    } catch (RuntimeException $error) { echo 'caught'; }
}
exerciseNestedResult();
"#, "capture|result|caught");
}

/// A selected captured callable stays alive while a later argument replaces its source local.
#[test]
fn mixed_value_copies_captured_callable_survives_argument_replacement() {
    assert_clean_copy(r#"
function rewriteCapturedCallable(callable &$callback): int {
    $callback = static function (int $ignored): string { return 'new'; };
    return 1;
}
function exerciseCapturedCallable(): string {
    $value = boxed(['name' => 'old']);
    $callback = static function (int $ignored) use ($value): string { return $value['name']; };
    $result = $callback(rewriteCapturedCallable($callback));
    unset($callback); unset($value);
    return $result;
}
echo exerciseCapturedCallable();
"#, "old");
}

/// A static callable array surviving a branch join crosses a positional parameter as a descriptor.
#[test]
fn mixed_value_copies_joined_static_callable_array_uses_descriptor_parameter_storage() {
    assert_clean_copy(r#"
class JoinedCallableTarget { public static function hit(): int { return 7; } }
function consumeJoinedCallable(callable $callback): int { return $callback(); }
function invokeJoinedCallable(int $choice): int {
    if ($choice > 0) { $callback = [JoinedCallableTarget::class, 'hit']; }
    else { $callback = [JoinedCallableTarget::class, 'hit']; }
    return consumeJoinedCallable($callback);
}
echo invokeJoinedCallable($argc), "|", invokeJoinedCallable($argc - 1);
"#, "7|7");
}

/// A captured instance callable keeps its receiver through a joined positional parameter binding.
#[test]
fn mixed_value_copies_joined_instance_callable_array_keeps_descriptor_receiver() {
    assert_clean_copy(r#"
class PrejoinedCallableTarget { public function hit(): int { return 7; } }
function consumePrejoinedCallable(callable $callback): int { return $callback(); }
function invokePrejoinedCallable(int $choice): int {
    $callback = [new PrejoinedCallableTarget(), 'hit'];
    if ($choice > 0) { $marker = 1; } else { $marker = 2; }
    return consumePrejoinedCallable($callback) + $marker;
}
echo invokePrejoinedCallable($argc), "|", invokePrejoinedCallable($argc - 1);
"#, "8|9");
}

/// Parameter declaring-function and union-type boxes retire all nested creator leases.
#[test]
fn mixed_value_copies_reflection_parameter_metadata_releases_creator_leases() {
    assert_clean_copy(r#"
function reflection_parameter_target(int|string $value = 1): int { return 1; }
$function = new ReflectionFunction("reflection_parameter_target");
$parameters = $function->getParameters();
$parameter = $parameters[0];
$type = $parameter->getType();
$declaring = $parameter->getDeclaringFunction();
echo $type->allowsNull() ? "nullable|" : "nonnull|";
echo $declaring->getName();
unset($declaring); unset($type); unset($parameter); unset($parameters); unset($function);
"#, "nonnull|reflection_parameter_target");
}

/// Nested declaring-class, parent, prototype and constructor boxes consume their fresh owners.
#[test]
fn mixed_value_copies_nested_reflection_metadata_releases_creator_leases() {
    assert_clean_copy(r#"
class ReflectionParent {
    public function __construct() {}
    public function value(): int { return 1; }
}
class ReflectionChild extends ReflectionParent {
    public function value(): int { return 2; }
}
$class = new ReflectionClass(ReflectionChild::class);
$method = $class->getMethod("value");
$prototype = $method->getPrototype();
$declaring = $method->getDeclaringClass();
$parent = $class->getParentClass();
$constructor = $class->getConstructor();
unset($method); unset($class);
echo $declaring->getName(), "|", $parent->getName(), "|", $prototype->getName(), "|";
echo $constructor->getName();
unset($constructor); unset($parent); unset($declaring); unset($prototype);
"#, "ReflectionChild|ReflectionParent|value|__construct");
}

/// Terminal argument cleanup lowering must not erase a surviving ReflectionMethod association.
#[test]
fn mixed_value_copies_reflection_target_survives_unrelated_argument_guard() {
    assert_clean_copy(r#"
class ReflectionTarget { public function value(): int { return 42; } }
class ArgumentHolder { public function __construct(public string $text) {} }
$reflected = new ReflectionMethod(ReflectionTarget::class, "value");
$holder = new ArgumentHolder(str_repeat("x", 2));
$target = new ReflectionTarget();
echo $reflected->invoke($target);
unset($reflected); unset($holder); unset($target);
"#, "42");
}

/// Post-evaluation fact restoration must retain a genuine reference-call target replacement.
#[test]
fn mixed_value_copies_argument_guard_keeps_post_evaluation_callable_target() {
    assert_clean_copy(r#"
function first_target(): int { return 1; }
function second_target(): int { return 2; }
function replace_target(callable &$callback): int { $callback = second_target(...); return 0; }
function consume_ignored(int $ignored): void {}
$callback = first_target(...);
consume_ignored(replace_target($callback));
echo $callback();
unset($callback);
"#, "2");
}

/// Requires complete source evaluation before fixed and variadic Stringable binding.
#[test]
fn mixed_value_copies_argument_sources_precede_fixed_and_variadic_binding() {
    assert_clean_copy(
        r#"
class Text { public function __toString(): string { echo 'bind|'; return 'text'; } }
function later(): mixed { echo 'eval|'; return boxed('tail'); }
function fixed(string $first, mixed $second): int { return 0; }
function variadic(string ...$texts): int { return 0; }
fixed(boxed(new Text()), later());
echo ';';
variadic(boxed(new Text()), later());
"#,
        "eval|bind|;eval|bind|",
    );
}

/// Snapshots a typed array argument before a later argument mutates the original.
#[test]
fn mixed_value_copies_typed_array_argument_is_pinned_before_later_mutation() {
    assert_clean_copy(
        r#"
function mutate(array &$value): int { $value[0] = 2; return 0; }
function observe(array $first, int $ignored): string { return json_encode($first); }
$source = [1];
echo observe($source, mutate($source)), '|', json_encode($source);
unset($source);
"#,
        "[1]|[2]",
    );
}

/// Keeps captured and argument heap results alive after immediate closure descriptors retire.
#[test]
fn mixed_value_copies_immediate_closure_heap_results_outlive_descriptors() {
    assert_clean_copy(
        r#"
$source = boxed(['capture' => 3]);
$captured = (static fn (): mixed => $source)();
$argument = (static function (mixed $value): mixed { return $value; })(boxed(['argument' => 4]));
unset($source);
echo json_encode($captured), '|', json_encode($argument), '|';
(static function (): void { echo 'void'; })();
unset($captured); unset($argument);
"#,
        r#"{"capture":3}|{"argument":4}|void"#,
    );
}

/// Preserves throwing string conversions even when immediate closure results are unused.
#[test]
fn mixed_value_copies_unused_immediate_closures_preserve_string_bindings() {
    assert_clean_copy(
        r#"
class BrokenText {
    public function __toString(): string { throw new RuntimeException('binding'); }
}
$value = boxed(new BrokenText());
try { (static function (string $text): int { return 0; })($value); }
catch (RuntimeException $error) { echo 'fixed|'; unset($error); }
try { (static function (string ...$texts): int { return 0; })($value); }
catch (RuntimeException $error) { echo 'variadic'; unset($error); }
unset($value);
"#,
        "fixed|variadic",
    );
}

/// Releases boxed argument ownership when a static closure body throws.
#[test]
fn mixed_value_copies_static_closure_body_throw_releases_argument_owner() {
    assert_clean_copy(
        r#"
$callback = static function (mixed $value): int { throw new RuntimeException('body'); };
try { $callback(boxed(['payload' => 1])); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
unset($callback);
"#,
        "caught",
    );
}

/// Retires the partially converted named collector when a later Stringable element throws.
#[test]
fn mixed_value_copies_named_variadic_partial_conversion_unwinds_collector() {
    assert_clean_copy(
        r#"
class ValidText { public function __toString(): string { return 'owned'; } }
class BrokenText {
    public function __toString(): string { throw new RuntimeException('binding'); }
}
function consume(string ...$texts): int { return 0; }
try { consume(first: boxed(new ValidText()), second: boxed(new BrokenText())); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
"#,
        "caught",
    );
}

/// Keeps variadic string-binding exceptions observable when the call result is unused.
#[test]
fn mixed_value_copies_unused_variadic_call_preserves_string_binding_exception() {
    assert_clean_copy(
        r#"
class BrokenText {
    public function __toString(): string { throw new RuntimeException('binding'); }
}
function consume(string ...$texts): int { return 0; }
$value = boxed(new BrokenText());
try { consume($value); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
unset($value);
"#,
        "caught",
    );
}

/// Preserves fixed-argument string-binding exceptions and the original source owner.
#[test]
fn mixed_value_copies_unused_call_preserves_string_binding_exception() {
    assert_clean_copy(
        r#"
class ThrowingText {
    public function __toString(): string { throw new RuntimeException('coercion'); }
}
function bind_text(string $first, string $second): int { return 0; }
$source = boxed('payload');
$throwing = boxed(new ThrowingText());
try { bind_text(first: $source, second: $throwing); }
catch (RuntimeException $error) { echo 'caught|'; unset($error); }
echo $source;
unset($source); unset($throwing);
"#,
        "caught|payload",
    );
}

/// Avoids double retirement of an earlier named snapshot when later coercion fails.
#[test]
fn mixed_value_copies_named_coercion_failure_does_not_release_consumed_snapshot() {
    assert_clean_copy(
        r#"
class ThrowingText {
    public function __toString(): string { throw new RuntimeException('coercion'); }
}
function bind_text(string $first, string $second): string { return $first . $second; }
$source = boxed('payload');
$throwing = boxed(new ThrowingText());
try { echo bind_text(first: $source, second: $throwing); }
catch (RuntimeException $error) { echo 'caught|'; unset($error); }
echo $source;
unset($source); unset($throwing);
"#,
        "caught|payload",
    );
}

/// Does not release argument snapshots whose source expressions never ran.
#[test]
fn mixed_value_copies_preparation_unwind_skips_unevaluated_snapshots() {
    assert_clean_copy(
        r#"
function fail_first(): mixed { throw new RuntimeException('first'); }
function take_both(mixed $first, mixed $second): int { return 0; }
try { take_both(fail_first(), boxed(['unused' => 1])); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
"#,
        "caught",
    );
}

/// Unwinds both inner and outer argument-owner ledgers without mutating the source.
#[test]
fn mixed_value_copies_nested_argument_evaluation_unwinds_both_ledgers() {
    assert_clean_copy(
        r#"
function outer(mixed $value, int $ignored): int { return 0; }
function inner(mixed $value, int $ignored): int { return 0; }
function fail_later(): int { throw new RuntimeException('inner'); }
$source = boxed(['x' => 1]);
try { outer($source, inner(boxed(['y' => 3]), fail_later())); }
catch (RuntimeException $error) { echo 'caught|'; unset($error); }
echo json_encode($source);
unset($source);
"#,
        r#"caught|{"x":1}"#,
    );
}

/// Retires an earlier named argument snapshot when later source evaluation throws.
#[test]
fn mixed_value_copies_named_preparation_exception_releases_snapshot() {
    assert_clean_copy(
        r#"
function take_first(mixed $value, int $ignored): mixed { return $value; }
function fail_later(): int { throw new RuntimeException('argument'); }
$source = boxed(['x' => 1]);
try { take_first(value: $source, ignored: fail_later()); }
catch (RuntimeException $error) { echo 'caught|'; unset($error); }
echo json_encode($source);
unset($source);
"#,
        r#"caught|{"x":1}"#,
    );
}

/// Retires a positional argument snapshot when a later argument throws before entry.
#[test]
fn mixed_value_copies_argument_snapshot_is_released_when_later_argument_throws() {
    assert_clean_copy(
        r#"
function take_first(mixed $value, int $ignored): mixed { return $value; }
function fail_later(): int { throw new RuntimeException('argument'); }
$source = boxed(['x' => 1]);
try { take_first($source, fail_later()); }
catch (RuntimeException $error) { echo 'caught|'; unset($error); }
echo json_encode($source);
unset($source);
"#,
        r#"caught|{"x":1}"#,
    );
}

/// Preserves named source-order snapshots across later by-reference mutation.
#[test]
fn mixed_value_copies_named_argument_snapshot_precedes_later_mutation() {
    assert_clean_copy(
        r#"
function take_first(mixed $value, int $ignored): mixed { return $value; }
function change_later(mixed &$value): int { $value['x'] = 2; return 0; }
$source = boxed(['x' => 1]);
$copy = take_first(value: $source, ignored: change_later($source));
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy);
"#,
        r#"{"x":2}|{"x":1}"#,
    );
}

/// Detaches a static assignment's value cell from its source parameter.
#[test]
fn mixed_value_copies_static_assignment_is_not_a_shared_value_cell() {
    assert_clean_copy(
        r#"
function keep_snapshot(mixed $value): mixed {
    static $saved = null;
    $saved = $value;
    $value['x'] = 2;
    return $saved;
}
$source = boxed(['x' => 1]);
$copy = keep_snapshot($source);
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy);
"#,
        r#"{"x":1}|{"x":1}"#,
    );
}

/// Preserves positional source-order snapshots across later by-reference mutation.
#[test]
fn mixed_value_copies_argument_snapshot_precedes_later_argument_mutation() {
    assert_clean_copy(
        r#"
function take_first(mixed $value, int $ignored): mixed { return $value; }
function change_later(mixed &$value): int { $value['x'] = 2; return 0; }
$source = boxed(['x' => 1]);
$copy = take_first($source, change_later($source));
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy);
"#,
        r#"{"x":2}|{"x":1}"#,
    );
}

/// Makes an ordinary copy from a reference independent of its reference set.
#[test]
fn mixed_value_copies_from_reference_are_values_not_aliases() {
    assert_clean_copy(
        r#"
$source = boxed(['x' => 1]);
$reference =& $source;
$copy = $reference;
$copy['x'] = 2;
echo json_encode($source), '|', json_encode($reference), '|', json_encode($copy);
unset($copy); unset($reference); unset($source);
"#,
        r#"{"x":1}|{"x":1}|{"x":2}"#,
    );
}

/// Copies a value into an existing reference set without aliasing the source cell.
#[test]
fn mixed_value_copies_assignment_into_reference_copies_the_value() {
    assert_clean_copy(
        r#"
$source = boxed(['x' => 1]);
$target = boxed([]);
$reference =& $target;
$reference = $source;
$source['x'] = 2;
echo json_encode($target), '|', json_encode($reference), '|', json_encode($source);
unset($reference); unset($target); unset($source);
"#,
        r#"{"x":1}|{"x":1}|{"x":2}"#,
    );
}

/// Keeps a published global snapshot separate from a later local parameter mutation.
#[test]
fn mixed_value_copies_global_assignment_is_not_a_shared_value_cell() {
    assert_clean_copy(
        r#"
function publish(mixed $value): mixed {
    global $published;
    $published = $value;
    $value['x'] = 2;
    return $value;
}
$published = boxed([]);
$source = boxed(['x' => 1]);
$changed = publish($source);
echo json_encode($source), '|', json_encode($published), '|', json_encode($changed);
unset($source); unset($published); unset($changed);
"#,
        r#"{"x":1}|{"x":1}|{"x":2}"#,
    );
}

/// Captures a by-value boxed array before later mutation of its source.
#[test]
fn mixed_value_copies_closure_capture_is_a_value_snapshot() {
    assert_clean_copy(
        r#"
$source = boxed(['x' => 1]);
$callback = function () use ($source): mixed { return $source; };
$source['x'] = 2;
$copy = $callback();
echo json_encode($source), '|', json_encode($copy);
unset($copy); unset($callback); unset($source);
"#,
        r#"{"x":2}|{"x":1}"#,
    );
}

/// Executes the exact tagged source in PHP and fails on missing PHP, fatal errors or differing output.
fn assert_php_copy_oracle(source: &str, native_stdout: &str) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("elephc_copy_oracle_{}_{}.php", std::process::id(), id));
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path)
        .expect("create isolated PHP copy-oracle source");
    std::io::Write::write_all(&mut file, source.as_bytes()).expect("write PHP copy-oracle source");
    drop(file);
    let result = Command::new("php").arg(&path).output();
    fs::remove_file(&path).expect("remove owned PHP copy-oracle source");
    let output = result.expect("ELEPHC_PHP_CHECK requires a runnable PHP executable");
    assert!(output.status.success(), "PHP copy oracle failed: {:?}\n{}", output.status,
        String::from_utf8_lossy(&output.stderr));
    assert_eq!(native_stdout, String::from_utf8_lossy(&output.stdout).as_ref(),
        "PHP copy oracle differs; stderr: {}", String::from_utf8_lossy(&output.stderr));
    eprintln!("PHP copy oracle verified");
}

/// Requires expected native output, clean heap teardown and an opt-in strict PHP oracle.
fn assert_clean_copy(body: &str, expected: &str) {
    let source = format!(
        "<?php\nfunction boxed(mixed $value): mixed {{ return $value; }}\n{body}\n"
    );
    let out = compile_and_run_with_heap_debug(&source);
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        assert_php_copy_oracle(&source, &out.stdout);
    }
}

/// Requires ordinary boxed-array assignment to detach mutable value cells.
#[test]
fn mixed_value_copies_assignment_detaches_array_cell() {
    assert_clean_copy(
        r#"
$source = boxed(['x' => 1]);
$copy = $source;
$copy['x'] = 2;
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy);
"#,
        r#"{"x":1}|{"x":2}"#,
    );
}

/// Keeps a by-value parameter's array mutation isolated from caller storage.
#[test]
fn mixed_value_copies_byvalue_parameter_detaches_array_cell() {
    assert_clean_copy(
        r#"
function mutate(mixed $value): mixed { $value['x'] = 2; return $value; }
$source = boxed(['x' => 1]);
$copy = mutate($source);
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy);
"#,
        r#"{"x":1}|{"x":2}"#,
    );
}

/// Keeps an identity function's returned value cell independent of the source.
#[test]
fn mixed_value_copies_identity_return_remains_an_independent_value() {
    assert_clean_copy(
        r#"
function identity(mixed $value): mixed { return $value; }
$source = boxed(['x' => 1]);
$copy = identity($source);
$copy['x'] = 2;
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy);
"#,
        r#"{"x":1}|{"x":2}"#,
    );
}

/// Preserves value-copy isolation through an additional forwarding function.
#[test]
fn mixed_value_copies_forwarded_parameter_does_not_alias_source() {
    assert_clean_copy(
        r#"
function mutate(mixed $value): mixed { $value['x'] = 2; return $value; }
function forward(mixed $value): mixed { return mutate($value); }
$source = boxed(['x' => 1]);
$copy = forward($source);
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy);
"#,
        r#"{"x":1}|{"x":2}"#,
    );
}

/// Preserves value-copy isolation through an instance-method argument boundary.
#[test]
fn mixed_value_copies_method_parameter_does_not_alias_source() {
    assert_clean_copy(
        r#"
final class Mutator {
    public function apply(mixed $value): mixed { $value['x'] = 2; return $value; }
}
$source = boxed(['x' => 1]);
$mutator = new Mutator();
$copy = $mutator->apply($source);
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy); unset($mutator);
"#,
        r#"{"x":1}|{"x":2}"#,
    );
}

/// Preserves value-copy isolation through a closure argument boundary.
#[test]
fn mixed_value_copies_closure_parameter_does_not_alias_source() {
    assert_clean_copy(
        r#"
$source = boxed(['x' => 1]);
$callback = static function (mixed $value): mixed { $value['x'] = 2; return $value; };
$copy = $callback($source);
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy); unset($callback);
"#,
        r#"{"x":1}|{"x":2}"#,
    );
}

/// Keeps genuine by-reference parameters connected to the caller's mutable value.
#[test]
fn mixed_value_copies_reference_parameter_keeps_caller_alias() {
    assert_clean_copy(
        r#"
function mutate_ref(mixed &$value): void { $value['x'] = 2; }
$source = boxed(['x' => 1]);
mutate_ref($source);
echo json_encode($source);
unset($source);
"#,
        r#"{"x":2}"#,
    );
}

/// Preserves by-value isolation through runtime string callable dispatch.
#[test]
fn mixed_value_copies_runtime_string_descriptor_does_not_alias_source() {
    assert_clean_copy(
        r#"
function mutate(mixed $value): mixed { $value['x'] = 2; return $value; }
function invoke(string $name, mixed $value): mixed { return call_user_func_array($name, [$value]); }
$source = boxed(['x' => 1]);
$copy = invoke('mutate', $source);
echo json_encode($source), '|', json_encode($copy);
unset($source); unset($copy);
"#,
        r#"{"x":1}|{"x":2}"#,
    );
}

/// Detaches value cells without duplicating object identity or shared properties.
#[test]
fn mixed_value_copies_preserve_object_identity_and_shared_properties() {
    assert_clean_copy(
        r#"
function identity(mixed $value): mixed { return $value; }
final class Payload { public int $x = 1; }
function update_payload(Payload $value): void { $value->x = 2; }
function read_payload(Payload $value): int { return $value->x; }
$source = boxed(new Payload());
$copy = identity($source);
echo $source === $copy ? 'same' : 'different';
if ($copy instanceof Payload) { update_payload($copy); }
else { echo '|not-payload'; }
if ($source instanceof Payload) { echo '|', read_payload($source); }
else { echo '|not-source'; }
unset($source); unset($copy);
"#,
        "same|2",
    );
}

/// Keeps resource identity and shared position alive until the final copied owner retires.
#[test]
fn mixed_value_copies_preserve_resource_identity_position_and_lifetime() {
    assert_clean_copy(
        r#"
function identity(mixed $value): mixed { return $value; }
$source = identity(tmpfile());
fwrite($source, 'abcdef');
rewind($source);
$copy = identity($source);
echo $source === $copy ? 'same' : 'different';
echo '|', fread($copy, 3), '|', fread($source, 3);
unset($source);
echo '|', fwrite($copy, 'g') === 1 ? 'live' : 'closed-too-soon';
fclose($copy);
unset($copy);
"#,
        "same|abc|def|live",
    );
}

/// Releases an exceptional callee's private cell without changing the caller's array.
#[test]
fn mixed_value_copies_exception_releases_private_cell_without_mutating_source() {
    assert_clean_copy(
        r#"
function mutate_throw(mixed $value): void {
    $value['x'] = str_repeat('payload', 8);
    throw new RuntimeException('caught');
}
$source = boxed(['x' => 1]);
try { mutate_throw($source); }
catch (RuntimeException $error) { echo $error->getMessage(), '|'; unset($error); }
echo json_encode($source);
unset($source);
"#,
        r#"caught|{"x":1}"#,
    );
}
