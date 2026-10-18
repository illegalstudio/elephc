//! Purpose:
//! Regressions for Core declaration and runtime behavior identified during PR review.
//!
//! Called from:
//! - The core_classes codegen integration module.
//!
//! Key details:
//! - Opaque source fragments force runtime eval, including receiver side effects and diagnostics.

use crate::support::*;

/// Native allocation keeps default ErrorException lines through inheritance, factories, and rethrows.
#[test]
fn test_core_review_error_exception_creation_line() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class LocatedError extends ErrorException { public int $extra = 1; }
class LocationFactory extends ErrorException {
    public static function make(): static {
        $line = __LINE__ + 1;
        $error = new static('factory');
        echo $error->getLine() === $line ? 'factory:' : 'bad:';
        return $error;
    }
}
$firstLine = __LINE__ + 1;
$error = new ErrorException('default');
echo $error->getLine() === $firstLine ? 'native:' : 'bad:';
$line = __LINE__ + 1;
$child = new LocatedError('inherited');
echo $child->getLine() === $line ? 'inherited:' : 'bad:';
$factory = LocationFactory::make();
$class = $argc > 0 ? ErrorException::class : LocatedError::class;
$line = __LINE__ + 1;
$dynamic = new $class('dynamic');
echo $dynamic->getLine() === $line ? 'dynamic:' : 'bad:';
$line = __LINE__ + 1;
$located = new ErrorException(message: 'nullable', filename: null, line: null);
echo $located->getLine() === $line ? 'nullable:' : 'bad:';
$explicit = new ErrorException(line: 73);
$zero = new ErrorException(line: 0);
echo $explicit->getLine(), ':', $zero->getLine(), ':';
try { throw $error; } catch (Exception $caught) {
    echo $caught->getLine() === $firstLine ? 'thrown:' : 'bad:';
}
unset($error, $child, $factory, $dynamic, $located, $explicit, $zero);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "native:inherited:factory:dynamic:nullable:73:0:thrown:");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval reads the native construction line through the Throwable method bridge.
#[test]
fn test_core_review_error_exception_creation_line_eval_read() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$error = new ErrorException('native');
$line = __LINE__ - 1;
$source = 'echo $error->getLine() === ' . $line . ' ? "eval" : "bad";' . ' // ' . $argc;
eval($source);
unset($error);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "eval");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Protected scalar writes and compound assignments keep independent construction metadata.
#[test]
fn test_core_review_error_exception_code_writes_preserve_line() {
    let out = compile_and_run_with_heap_debug(r#"<?php
#[AllowDynamicProperties]
class CodeError extends ErrorException {
    public function __construct(string $message, int $status) {
        parent::__construct($message);
        $this->code = $status;
    }
    public function revise(int $status): void {
        $this->code += 1;
        $this->code = $status;
    }
}
class CodeException extends Exception {
    public function __construct(int $status) {
        parent::__construct('ordinary');
        $this->code = $status;
    }
}
$line = __LINE__ + 1;
$error = new CodeError('failed', 500);
$error->detail = str_repeat('detail', 5);
echo $error->getCode(), ':', $error->getLine() === $line ? 'kept:' : 'lost:';
$error->revise(600);
echo $error->getCode(), ':', $error->getLine() === $line ? 'kept:' : 'lost:';
$line = __LINE__ + 1;
$ordinary = new CodeException(700);
echo $ordinary->getCode(), ':', $ordinary->getLine() === $line ? 'kept' : 'lost';
unset($error, $ordinary);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "500:kept:600:kept:700:kept");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Uncaught reporting reads the dedicated ordinary location after a protected code write.
#[test]
fn test_core_review_error_exception_uncaught_creation_line() {
    let source = "<?php\nclass ReportError extends ErrorException {\npublic function __construct(string $message) { parent::__construct($message); $this->code = 500; }\n}\n$error = new ReportError('uncaught');\nthrow $error;";
    let line = source.lines().position(|line| line.starts_with("$error = new")).unwrap() + 1;
    let out = compile_and_run_capture(source);
    assert!(!out.success);
    assert!(out.stdout.contains("Fatal error: Uncaught ReportError: uncaught in "), "{}", out.stdout);
    assert!(out.stdout.ends_with(&format!(":{line}\n")), "{}", out.stdout);
}

/// Fatal locations match getters for independent overrides, explicit zero, and an empty filename.
#[test]
fn test_core_review_error_exception_uncaught_location_overrides() {
    for (arguments, oracle_suffix) in [
        ("filename: 'custom.php', line: 73", Some("custom.php:73")),
        ("filename: '', line: 0", Some(":0")),
        ("filename: null, line: 0", None),
        ("filename: 'custom.php', line: null", Some("custom.php:0")),
    ] {
        let source = format!("<?php\nclass LocatedReport extends ErrorException {{}}\n$error = new LocatedReport('located', {arguments});\necho 'expected:', $error->getFile(), ':', $error->getLine(), \"\\n\";\nthrow $error;");
        let out = compile_and_run_capture(&source);
        assert!(!out.success);
        let expected = out.stdout.lines().next().unwrap().strip_prefix("expected:").unwrap();
        if let Some(suffix) = oracle_suffix { assert_eq!(expected, suffix, "{arguments}"); }
        assert!(out.stdout.contains("Fatal error: Uncaught LocatedReport: located in "), "{arguments}: {}", out.stdout);
        assert!(out.stdout.ends_with(&format!(" in {expected}\n")), "{arguments}: {}", out.stdout);
    }
}

/// Null location arguments preserve existing state; a new filename defaults its line to zero.
#[test]
fn test_core_review_error_exception_constructor_location_defaults() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$error = new ErrorException(filename: 'first.php');
echo $error->getFile(), ':', $error->getLine(), ';';
$error->__construct(filename: null, line: null);
echo $error->getFile(), ':', $error->getLine(), ';';
$error->__construct(line: 41);
echo $error->getFile(), ':', $error->getLine(), ';';
$error->__construct(filename: 'second.php');
echo $error->getFile(), ':', $error->getLine();
unset($error);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "first.php:0;first.php:0;first.php:41;second.php:0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval uses the same constructor location defaults and reinitialization rules.
#[test]
fn test_core_review_error_exception_constructor_location_defaults_eval() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = '$error = new ErrorException(filename: "eval.php");
echo $error->getFile(), ":", $error->getLine(), ";";
$error->__construct(line: 17);
$error->__construct(filename: null, line: null);
echo $error->getFile(), ":", $error->getLine();
unset($error);' . ' // ' . $argc;
eval($source);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "eval.php:0;eval.php:17");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime property-name dispatch preserves construction metadata independently of the code slot.
#[test]
fn test_core_review_error_exception_dynamic_code_write_preserves_line() {
    let out = compile_and_run(r#"<?php
class DynamicCodeError extends ErrorException {
    public function revise(string $name, int $status): void { $this->$name = $status; }
}
$line = __LINE__ + 1;
$error = new DynamicCodeError('failed');
$error->revise('code', 503);
echo $error->getCode(), ':', $error->getLine() === $line ? 'kept' : 'lost';
"#);
    assert_eq!(out, "503:kept");
}

/// Opaque eval property setters cannot clear a native ErrorException's dedicated location.
#[test]
fn test_core_review_error_exception_reflection_code_write_preserves_line() {
    let out = compile_and_run(r#"<?php
$error = new ErrorException('native');
$line = __LINE__ - 1;
$source = '$property = new ReflectionProperty(ErrorException::class, "code");
$property->setValue($error, 501);
echo $error->getCode(), ":", $error->getLine() === ' . $line . ' ? "kept" : "lost";
unset($property);' . ' // ' . $argc;
eval($source);
unset($error);
"#);
    assert_eq!(out, "501:kept");
}

/// A throwing warning handler does not strand the already produced instance-method result.
#[test]
fn test_core_review_no_discard_throwing_warning() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = 'class EvalWarningResult {
    #[NoDiscard] public function result(): string { return str_repeat("v", 30); }
}
set_error_handler(function () { throw new Exception("warning"); });
$object = new EvalWarningResult();
try { $object->result(); } catch (Exception $error) { echo $error->getMessage(); }
restore_error_handler();' . ' // ' . $argc;
eval($source);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "warning");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Legacy hydration ignores and releases strings or boxed values returned without a void hint.
#[test]
fn test_core_review_serializable_hydration_return_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class StringPacket implements Serializable {
    public function serialize() { return "x"; }
    public function unserialize($data) { return str_repeat($data, 30); }
}
class MixedPacket implements Serializable {
    public function serialize() { return "x"; }
    public function unserialize($data): mixed { return [str_repeat($data, 30)]; }
}
unserialize(serialize(new StringPacket()));
unserialize(serialize(new MixedPacket()));
echo "clean";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "clean");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Tentative legacy returns are checked at runtime and rejected heap values release their owners.
#[test]
fn test_core_review_serializable_result_validation() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ScalarBad implements Serializable {
    public function serialize(): int { return 7; }
    public function unserialize($data) {}
}
class HeapBad implements Serializable {
    public function serialize(): array { return [str_repeat("x", 30)]; }
    public function unserialize($data) {}
}
class DynamicBad implements Serializable {
    public mixed $value = 7;
    public function serialize(): mixed { return $this->value; }
    public function unserialize($data) {}
}
class EmptyPacket implements Serializable {
    public function serialize() {}
    public function unserialize($data) {}
}
class NullablePacket implements Serializable {
    public function serialize(): ?int { return null; }
    public function unserialize($data): ?int { return null; }
}
foreach ([new ScalarBad(), new HeapBad(), new DynamicBad()] as $packet) {
    try { serialize($packet); }
    catch (Exception $error) { echo $error->getMessage(), "|"; }
}
echo serialize(new EmptyPacket()), serialize(new NullablePacket());
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ScalarBad::serialize() must return a string or NULL|HeapBad::serialize() must return a string or NULL|DynamicBad::serialize() must return a string or NULL|N;N;");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Failed legacy callbacks restore sharing modes before later ordinary wire operations.
#[test]
fn test_core_review_serializable_throw_recovery() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowPacket implements Serializable {
    public function serialize(): ?string { serialize(7); throw new Exception("serialize"); }
    public function unserialize(string $data): void { unserialize("i:7;"); throw new Exception("hydrate"); }
}
try { serialize([new ThrowPacket()]); } catch (Exception $error) { echo $error->getMessage(), ":"; }
try { unserialize('C:11:"ThrowPacket":1:{x}'); } catch (Exception $error) { echo $error->getMessage(), ":"; }
$object = new stdClass();
$wire = serialize([$object, $object]);
echo $wire, ":";
$copy = unserialize($wire);
echo $copy[0] === $copy[1] ? "same" : "bad";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "serialize:hydrate:a:2:{i:0;O:8:\"stdClass\":0:{}i:1;r:2;}:same");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested legacy wire calls share reference indices and retain the outer object identity.
#[test]
fn test_core_review_serializable_nested_references() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Packet implements Serializable {
    public function serialize(): ?string { return serialize(7); }
    public function unserialize(string $data): void { unserialize($data); }
}
$packet = new Packet(); $object = new stdClass();
$wire = serialize([$packet, $object, $object]);
echo $wire, ":";
$copy = unserialize($wire);
echo $copy[1] === $copy[2] && $copy[1] instanceof stdClass ? "same" : "bad";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "a:3:{i:0;C:6:\"Packet\":4:{i:7;}i:1;O:8:\"stdClass\":0:{}i:2;r:4;}:same");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Legacy callbacks share both existing object identities and newly hydrated reference owners.
#[test]
fn test_core_review_serializable_nested_object_identity() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ObjectPacket implements Serializable {
    public mixed $value;
    public function serialize(): ?string { return serialize($this->value); }
    public function unserialize(string $data): void { $this->value = unserialize($data); }
}
$object = new stdClass(); $packet = new ObjectPacket(); $packet->value = $object;
$wire = serialize([$object, $packet, $object, $packet]);
echo $wire, ":";
$copy = unserialize($wire);
echo $copy[0] === $copy[1]->value && $copy[0] === $copy[2] && $copy[1] === $copy[3] ? "same|" : "bad|";
$wire = serialize([$packet, $object, $packet]);
echo $wire, ":";
$copy = unserialize($wire);
echo $copy[0]->value === $copy[1] && $copy[0] === $copy[2] ? "same" : "bad";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "a:4:{i:0;O:8:\"stdClass\":0:{}i:1;C:12:\"ObjectPacket\":4:{r:2;}i:2;r:2;i:3;r:3;}:same|a:3:{i:0;C:12:\"ObjectPacket\":19:{O:8:\"stdClass\":0:{}}i:1;r:3;i:2;r:2;}:same");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Modern hydration hooks isolate nested reference registries and restore the outer identities.
#[test]
fn test_core_review_modern_hydration_nested_references() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function nested_identity(): bool {
    $objects = unserialize('a:2:{i:0;O:8:"stdClass":0:{}i:1;r:2;}');
    return $objects[0] === $objects[1];
}
class ModernHydration {
    public bool $same = false;
    public function __serialize(): array { return []; }
    public function __unserialize(array $data): void { $this->same = nested_identity(); }
}
class WakeHydration {
    public bool $same = false;
    public function __sleep(): array { return []; }
    public function __wakeup(): void { $this->same = nested_identity(); }
}
class HydrationEnvelope implements Serializable {
    public mixed $value;
    public function serialize(): ?string { return serialize($this->value); }
    public function unserialize(string $data): void { $this->value = unserialize($data); }
}
$object = new stdClass();
$modern = unserialize(serialize([new ModernHydration(), $object, $object]));
echo $modern[0]->same ? "same:" : "bad:";
echo $modern[1] === $modern[2] ? "same;" : "bad;";
$wake = unserialize(serialize([new WakeHydration(), $object, $object]));
echo $wake[0]->same ? "same:" : "bad:";
echo $wake[1] === $wake[2] ? "same;" : "bad;";
$packet = new HydrationEnvelope(); $packet->value = new WakeHydration();
$legacy = unserialize(serialize($packet));
echo $legacy->value->same ? "legacy:same" : "legacy:bad";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "same:same;same:same;legacy:same");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Modern hooks isolate nested wire calls from the enclosing reference registry.
#[test]
fn test_core_review_modern_nested_references() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ModernPacket { public function __serialize(): array { serialize(7); return []; } }
class SleepPacket { public function __sleep(): array { serialize(7); return []; } }
$object = new stdClass();
echo serialize([new ModernPacket(), $object, $object]), "\n";
echo serialize([new SleepPacket(), $object, $object]);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "a:3:{i:0;O:12:\"ModernPacket\":0:{}i:1;O:8:\"stdClass\":0:{}i:2;r:3;}\na:3:{i:0;O:11:\"SleepPacket\":0:{}i:1;O:8:\"stdClass\":0:{}i:2;r:3;}");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Instance and dynamic NoDiscard calls warn once without repeating receiver expressions.
#[test]
fn test_core_review_no_discard_instance_methods() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = 'class EvalDiscarded {
    #[NoDiscard] public function result(): string { return str_repeat("v", 30); }
}
function receiver(): EvalDiscarded { echo "receiver:"; return new EvalDiscarded(); }
$object = new EvalDiscarded();
$object->result();
$_ = $object->result();
receiver()->result();
$method = "result";
$object->$method();
$null = null; $null?->result();
echo strlen($_);' . ' // ' . $argc;
eval($source);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "receiver:30");
    assert_eq!(out.stderr.matches("The return value of EvalDiscarded::result()").count(), 3, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Legacy Serializable implementations may omit tentative returns with or without the marker.
#[test]
fn test_core_review_serializable_legacy_signatures() {
    let out = compile_and_run(r#"<?php
class LegacyPacket implements Serializable {
    public $data = "before";
    #[ReturnTypeWillChange] public function serialize() { return $this->data; }
    #[ReturnTypeWillChange] public function unserialize($data) { $this->data = $data; }
}
$packet = unserialize(serialize(new LegacyPacket()));
echo $packet->data, ":";
$source = 'class EvalLegacyPacket implements Serializable {
    public function serialize() { return "payload"; }
    public function unserialize($data) {}
}
echo class_exists("EvalLegacyPacket") ? "legacy" : "bad";' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "before:legacy");
}
