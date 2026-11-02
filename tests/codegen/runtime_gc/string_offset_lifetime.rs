//! Purpose:
//! Regression coverage for string offset views whose receiver is a temporary owner.
//!
//! Called from:
//! - The codegen integration suite through `runtime_gc`.
//!
//! Key details:
//! - Heap poisoning detects reads after an owning receiver has been released (issue #1908).

use crate::support::compile_and_run_with_heap_debug;

/// Final Mixed storage retires detached receiver copies across reads, returns and throws.
#[test]
fn test_string_offset_lifetime_widened_local_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function widenedOffsetBytes(): string { return str_repeat("AB", 32); }
function widenedOffsetReturn(bool $keep): string {
    for ($i = 0; $i < 4; $i++) { $bytes = widenedOffsetBytes(); }
    if ($keep) { return $bytes[2]; }
    unset($bytes);
    return "";
}
function widenedOffsetIndex(): int { throw new Error("index"); }
function widenedOffsetConsumer(string $byte): void { throw new Error("consumer"); }
for ($i = 0; $i < 4; $i++) { $bytes = widenedOffsetBytes(); }
$stored = $bytes[1];
echo ord($bytes[2]), ":", isset($bytes[500]) ? "yes" : "no", ":", widenedOffsetReturn(true), "\n";
try { widenedOffsetConsumer($bytes[2]); } catch (Error $error) { echo $error->getMessage(), "\n"; }
try { echo $bytes[widenedOffsetIndex()]; } catch (Error $error) { echo $error->getMessage(), "\n"; }
set_error_handler(function(int $level, string $message): bool { throw new Error("warning"); });
try { echo $bytes[500]; } catch (Error $error) { echo $error->getMessage(), "\n"; }
restore_error_handler();
unset($bytes);
echo $stored, "\n";
unset($stored, $error);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "65:no:A\nconsumer\nindex\nwarning\nB\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Storage-aware receiver ownership also holds when EIR optimizations are disabled.
#[test]
fn test_string_offset_lifetime_widened_local_owners_without_ir_opt() {
    crate::support::without_ir_opt(test_string_offset_lifetime_widened_local_owners);
}

/// Borrowed local views are pinned before a later argument replaces their source string.
#[test]
fn test_string_offset_lifetime_later_source_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function consumeOffsetByte(string $c, string $unused): void { echo ord($c), "\n"; }
function run(): void {
    $s = str_repeat("ab", 2000);
    consumeOffsetByte($s[0], $s = str_repeat("z", 4000));
    echo ord($s[0]), "\n";
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "97\n122\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Silent property probes retain missing-offset and zero-byte semantics after stabilization.
#[test]
fn test_string_offset_lifetime_silent_probes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class OffsetProbe {
    public string $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function run(): void {
        echo isset($this->bytes[0]) ? "yes|" : "no|";
        echo isset($this->bytes[5000]) ? "yes|" : "no|";
        echo empty($this->bytes[0]) ? "empty|" : "full|";
        echo $this->bytes[5000] ?? "missing";
        echo "|", $this->bytes[-1] ?? "missing", "\n";
    }
}
function run(): void { (new OffsetProbe(str_repeat("0a", 2000)))->run(); }
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "yes|no|empty|missing|a\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Property byte reads preserve the selected byte until its consumer has finished.
#[test]
fn test_string_offset_lifetime_property_reads() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class OffsetUntyped {
    public $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function at(int $i): int { return ord($this->bytes[$i]); }
    public function kept(int $i): string { $c = $this->bytes[$i]; return $c; }
    public function compare(int $i): bool { return $this->bytes[$i] === "a"; }
}
class OffsetTyped {
    public string $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function at(int $i): int { return ord($this->bytes[$i]); }
}
function run(): void {
    $a = new OffsetUntyped(str_repeat("ab", 2000));
    $b = new OffsetTyped(str_repeat("ab", 2000));
    $sum = 0;
    for ($i = 0; $i < 4000; $i++) { $sum += $a->at($i) + $b->at($i); }
    echo $sum, "|", $a->kept(-1), "|", $a->compare(0) ? "yes" : "no", "\n";
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "780000|b|yes\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested array reads and returned large strings keep byte views valid across cleanup.
#[test]
fn test_string_offset_lifetime_temporary_reads() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function offset_identity(string $s): string { return $s; }
function run(): void {
    $a = ["hello"];
    $b = ["value" => "world"];
    $c = [str_repeat("q", 70001)];
    $big = str_repeat("q", 70000);
    echo $a[0][1], $b["value"][1], $c[0][70000], offset_identity($big)[69999], "\n";
    echo ord($a[0][0]), "|", $c[0][-1], "\n";
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "eoqq\n104|q\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Binary payloads cannot make an interior byte view look like a transferable heap allocation.
#[test]
fn test_string_offset_lifetime_binary_header_bytes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class BinaryOffsetReader {
    public string $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function at(): string { return $this->bytes[8]; }
}
class BinaryOffsetStore {
    public string $typed = "";
    public $loose = "";
}
function binaryOffsetIdentity(string $s): string { return $s; }
function binaryOffsetConsume(string $c, string $unused): int { return ord($c); }
function replaceBinaryOffsetBytes(BinaryOffsetReader $reader): int {
    $reader->bytes = "0";
    return 0;
}
function binaryOffsetStore(string $payload, bool $select): void {
    $local = $payload . "!";
    $array = [""];
    $appended = [];
    $mixed = [1, ""];
    $hash = ["value" => ""];
    $keyed = [];
    $object = new BinaryOffsetStore();
    $array[0] = $local[8];
    $appended[] = $local[8];
    $mixed[0] = $local[8];
    $hash["value"] = $local[8];
    $keyed[$local[8]] = 1;
    $object->typed = $local[8];
    $object->loose = $local[8];
    $joined = $select ? $local[8] : 88;
    $joinedItems = [];
    $joinedItems[] = $joined;
    $local = $payload . "!";
    $lateKeyed = [];
    $lateKeyed[$local[8]] = ($local = "replaced");
    $local = $payload . "!";
    $concat = $local[8] . ($local = "replaced");
    $local = $payload . "!";
    $matches = $local[8] === ($local = "Z");
    $literalReader = new BinaryOffsetReader($payload . "!");
    $literal = [
        $literalReader->bytes[8] => replaceBinaryOffsetBytes($literalReader),
    ];
    $mixedLiteralReader = new BinaryOffsetReader($payload . "!");
    $mixedLiteral = [
        0,
        $mixedLiteralReader->bytes[8] => replaceBinaryOffsetBytes($mixedLiteralReader),
    ];
    $local = $payload . "!";
    $matched = match ($local[8]) {
        ($local = "Z") => "yes",
        default => "no",
    };
    $reader = new BinaryOffsetReader($payload . "!");
    $nested = $reader->bytes[8][replaceBinaryOffsetBytes($reader)];
    $local = str_repeat("z", 4000);
    echo "|", ord($array[0]), "|", ord($appended[0]), "|", ord($mixed[0]),
        "|", ord($hash["value"]), "|", $keyed["Z"],
        "|", ord($object->typed), "|", ord($object->loose),
        "|", ord($joinedItems[0]), "|", $lateKeyed["Z"],
        "|", $concat, "|", ($matches ? "yes" : "no"),
        "|", $literal["Z"], "|", $mixedLiteral["Z"],
        "|", $matched, "|", $nested;
}
function binaryOffsetRun(string $payload, bool $select): void {
    echo ord((new BinaryOffsetReader($payload))->at()), "|";
    echo ord(binaryOffsetIdentity($payload)[8]), "|";
    $array = [$payload];
    $hash = ["value" => $payload];
    echo ord($array[0][8]), "|", ord($hash["value"][8]), "|";
    $local = $payload . "!";
    echo binaryOffsetConsume($local[8], $local = "later");
    binaryOffsetStore($payload, $select);
    echo "\n";
}
binaryOffsetRun("\x07aaaaaaaZ" . str_repeat("q", $argc), $argc >= 0);
binaryOffsetRun("\x07\x00\x00\x00HPLEZ" . str_repeat("q", $argc), $argc >= 0);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "90|90|90|90|90|90|90|90|90|1|90|90|90|replaced|Zreplaced|yes|0|0|yes|Z\n".repeat(2),
        "{}",
        out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Alias-returning user calls persist direct byte arguments before handing them back.
#[test]
fn test_string_offset_lifetime_alias_return_boundaries() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function aliasReturn(string $byte): string { return $byte; }
class AliasReturnReader {
    public function same(string $byte): string { return $byte; }
    public static function staticSame(string $byte): string { return $byte; }
}
function runAliasReturns(string $payload): void {
    $reader = new AliasReturnReader();
    $nullable = $reader;
    $closure = function (string $byte): string { return $byte; };

    $source = $payload . "!";
    $kept = aliasReturn($source[8]);
    $source = str_repeat("a", 4000);
    echo ord($kept), ":", $kept, "|";

    $source = $payload . "!";
    $kept = $reader->same($source[8]);
    $source = str_repeat("b", 4000);
    echo ord($kept), ":", $kept, "|";

    $source = $payload . "!";
    $kept = AliasReturnReader::staticSame($source[8]);
    $source = str_repeat("c", 4000);
    echo ord($kept), ":", $kept, "|";

    $source = $payload . "!";
    $kept = $nullable?->same($source[8]);
    $source = str_repeat("d", 4000);
    echo ord($kept), ":", $kept, "|";

    $source = $payload . "!";
    $kept = (["AliasReturnReader", "staticSame"])($source[8]);
    $source = str_repeat("e", 4000);
    echo ord($kept), ":", $kept, "|";

    $source = $payload . "!";
    $kept = call_user_func(["AliasReturnReader", "staticSame"], $source[8]);
    $source = str_repeat("f", 4000);
    echo ord($kept), ":", $kept, "|";

    $source = $payload . "!";
    $kept = strval($source[8]);
    $source = str_repeat("g", 4000);
    echo ord($kept), ":", $kept, "|";

    $source = $payload . "!";
    $kept = $closure($source[8]);
    $source = str_repeat("h", 4000);
    echo ord($kept), ":", $kept, "\n";
}
runAliasReturns("\x07aaaaaaaZ" . str_repeat("q", 4000));
runAliasReturns("\x07\x00\x00\x00HPLEZ" . str_repeat("q", 4000));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "90:Z|90:Z|90:Z|90:Z|90:Z|90:Z|90:Z|90:Z\n".repeat(2),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Undeclared dynamic property values persist selected bytes before boxing them.
#[test]
fn test_string_offset_lifetime_dynamic_property_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
#[AllowDynamicProperties]
class DynamicByteBag {}
function storeDynamicPropertyBytes(string $payload): void {
    $local = $payload . "!";
    $object = new stdClass();
    $object->kept = $local[8];
    $local = "no";
    echo ord($object->kept), ":", $object->kept, "|";

    $local = $payload . "!";
    $bag = new DynamicByteBag();
    $bag->kept = $local[8];
    $local = "no";
    echo ord($bag->kept), ":", $bag->kept, "|";

    $local = $payload . "!";
    $name = "kept";
    $dynamic = new stdClass();
    $dynamic->{$name} = $local[8];
    $local = "no";
    echo ord($dynamic->kept), ":", $dynamic->kept, "\n";
}
storeDynamicPropertyBytes("\x07aaaaaaaZ" . str_repeat("q", 4000));
storeDynamicPropertyBytes("\x07\x00\x00\x00HPLEZ" . str_repeat("q", 4000));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "90:Z|90:Z|90:Z\n".repeat(2), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reflection static-property writes keep selected bytes valid under heap debug.
#[test]
fn test_string_offset_lifetime_reflection_static_property_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReflectedStaticBytes {
    public static mixed $kept;
    public static string $typedClass = "";
    public static string $typedProperty = "";
    public static $inferredClass = "";
    public static $inferredProperty = "";
}
function storeReflectedStaticBytes(string $payload): void {
    $local = $payload . "!";
    $class = new ReflectionClass(ReflectedStaticBytes::class);
    $class->setStaticPropertyValue("kept", $local[8]);
    $local = "no";
    echo ord(ReflectedStaticBytes::$kept), ":", ReflectedStaticBytes::$kept, "|";

    $local = $payload . "!";
    $property = new ReflectionProperty(ReflectedStaticBytes::class, "kept");
    $property->setValue(null, $local[8]);
    $local = "no";
    echo ord(ReflectedStaticBytes::$kept), ":", ReflectedStaticBytes::$kept, "|";

    $local = $payload . "!";
    $class->setStaticPropertyValue("typedClass", $local[8]);
    $local = "no";
    echo ord(ReflectedStaticBytes::$typedClass), ":", ReflectedStaticBytes::$typedClass, "|";

    $local = $payload . "!";
    $typed = new ReflectionProperty(ReflectedStaticBytes::class, "typedProperty");
    $typed->setValue(null, $local[8]);
    $local = "no";
    echo ord(ReflectedStaticBytes::$typedProperty), ":", ReflectedStaticBytes::$typedProperty, "|";

    $local = $payload . "!";
    $class->setStaticPropertyValue("inferredClass", $local[8]);
    $local = "no";
    echo ord(ReflectedStaticBytes::$inferredClass), ":", ReflectedStaticBytes::$inferredClass, "|";

    $local = $payload . "!";
    $inferred = new ReflectionProperty(ReflectedStaticBytes::class, "inferredProperty");
    $inferred->setValue(null, $local[8]);
    $local = "no";
    echo ord(ReflectedStaticBytes::$inferredProperty), ":", ReflectedStaticBytes::$inferredProperty, "\n";
}
storeReflectedStaticBytes("\x07aaaaaaaZ" . str_repeat("q", 4000));
storeReflectedStaticBytes("\x07\x00\x00\x00HPLEZ" . str_repeat("q", 4000));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "90:Z|90:Z|90:Z|90:Z|90:Z|90:Z\n".repeat(2), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: allocs="), "{}", out.stderr);
    assert!(!out.stderr.contains("double free"), "{}", out.stderr);
}

/// Nested parent creation persists direct byte keys before hash insertion.
#[test]
fn test_string_offset_lifetime_nested_parent_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NestedStaticByteKeys {
    public static mixed $hash = ["seed" => ["child" => 0]];
}
function storeNestedByteKey(string $payload): void {
    $hash = ["seed" => ""];
    $local = $payload . "!";
    $hash[$local[8]]["child"] = 1;
    $local = "no";
    foreach ($hash as $key => $unused) {
        if ($key !== "seed") { echo ord($key), ":", $key, "|"; }
    }
    unset($hash);

    NestedStaticByteKeys::$hash = ["seed" => ["child" => 0]];
    $local = $payload . "!";
    NestedStaticByteKeys::$hash[$local[8]]["child"]++;
    $local = "no";
    foreach (NestedStaticByteKeys::$hash as $key => $unused) {
        if ($key !== "seed") { echo ord($key), ":", $key, "\n"; }
    }
}
storeNestedByteKey("\x07aaaaaaaZ" . str_repeat("q", 4000));
storeNestedByteKey("\x07\x00\x00\x00HPLEZ" . str_repeat("q", 4000));
NestedStaticByteKeys::$hash = null;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "90:Z|90:Z\n".repeat(2), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Switch, pipe, generator and dynamic-name boundaries preserve bytes across later effects.
#[test]
fn test_string_offset_lifetime_control_and_dynamic_boundaries() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class OffsetBoundaryStatic {
    public static stdClass $object;
}
class OffsetBoundaryDeclared {
    public string $Z = "";
}
function offsetBoundaryCallableAfterChange(string $unused, callable $callback): callable { return $callback; }
function offsetBoundaryYield(string $payload) {
    $s = $payload . "!";
    yield $s[8];
    $s = $payload . "!";
    yield $s[8] => ($s = "no");
}
function offsetBoundaryRun(string $payload): void {
    $s = $payload . "!";
    switch ($s[8]) {
        case ($s = "no"): echo "bad|"; break;
        case "other": echo "bad|"; break;
        default: echo "default|";
    }
    $s = $payload . "!";
    switch ($s[8]) {
        case ($s = "Z"): echo "matched|"; break;
        case "other": echo "bad|"; break;
        default: echo "bad|";
    }
    $s = $payload . "!";
    echo $s[8] |> (function (string $c): int { echo ord($c), ":"; return 1; }), "|";
    $fn = function (string $c): int { echo ord($c), ":"; return 1; };
    $s = $payload . "!";
    echo $s[8] |> offsetBoundaryCallableAfterChange($s = "no", $fn), "|";
    foreach (offsetBoundaryYield($payload) as $key => $value) {
        echo $key, ":", $value, "|";
    }
    $object = new stdClass();
    $s = $payload . "!";
    $object->{$s[8]} = 1;
    echo $object->Z, "|";
    $s = $payload . "!";
    $object->{$s[8]} = ($s = "no");
    echo $object->Z, "|";
    $declared = new OffsetBoundaryDeclared();
    $s = $payload . "!";
    $declared->{$s[8]} = ($s = "no");
    echo $declared->Z, "|";
    OffsetBoundaryStatic::$object = new stdClass();
    $s = $payload . "!";
    OffsetBoundaryStatic::$object->{$s[8]} = ($s = "no");
    echo OffsetBoundaryStatic::$object->Z, "\n";
}
offsetBoundaryRun("\x07aaaaaaaZ" . str_repeat("q", 4000));
offsetBoundaryRun("\x07\x00\x00\x00HPLEZ" . str_repeat("q", 4000));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout,
        "default|matched|90:1|90:1|0:Z|Z:no|1|no|no|no\n".repeat(2), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Persisted byte copies remain unwind-visible while later calls and writes can throw.
#[test]
fn test_string_offset_lifetime_throwing_consumers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function offsetAliasOrThrow(string $byte): string {
    if (strlen($byte) === 1) { throw new Exception("alias"); }
    return $byte;
}

function offsetThrowString(): string { throw new Exception("string"); }
function offsetThrowInt(): int { throw new Exception("int"); }
function offsetWriteOrThrow(array &$items, string $payload): void {
    $items[$payload[0]] = offsetThrowInt();
}
function runOffsetThrows(): void {
    $payload = str_repeat("Zq", 2000);
    try { offsetAliasOrThrow($payload[0]); } catch (Exception $e) { echo "A"; }
    try { $concat = $payload[0] . offsetThrowString(); } catch (Exception $e) { echo "C"; }
    try { $equal = $payload[0] === offsetThrowString(); } catch (Exception $e) { echo "E"; }
    $items = [];
    try { offsetWriteOrThrow($items, $payload); } catch (Exception $e) { echo "W"; }
    echo "|", count($items), "\n";
}
runOffsetThrows();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ACEW|0\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Byte copies retire large concat receivers, including offsets beyond their first byte.
#[test]
fn test_string_offset_lifetime_large_concat_receivers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function concatOffsetByte(int $offset): string {
    return (str_repeat("a", 70000) . "b")[$offset];
}
function runConcatOffsetBytes(): void {
    for ($i = 0; $i < 4; $i++) { echo concatOffsetByte(0), concatOffsetByte(8); }
    $full = str_repeat("q", 70000) . "Z";
    echo "|", strlen($full), ":", $full[70000], "\n";
}
runConcatOffsetBytes();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "aaaaaaaa|70001:Z\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxing a persisted reference-bound key consumes its source exactly once.
#[test]
fn test_string_offset_lifetime_reference_bound_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function writeOffsetKey(array<mixed> &$items, string $bytes): void {
    $items[$bytes[0]] = 1;
}
function runReferenceOffsetKey(): void {
    $items = [0, "mixed"];
    writeOffsetKey($items, str_repeat("Z", 40));
    echo $items["Z"], "\n";
}
runReferenceOffsetKey();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Discarding a suspended generator releases its boxed byte key without retaining a copy.
#[test]
fn test_string_offset_lifetime_suspended_generator_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function stoppedOffsetKey(string $bytes) { yield $bytes[0] => 1; }
function runStoppedOffsetKeys(): void {
    $bytes = str_repeat("Z", 40);
    for ($i = 0; $i < 4; $i++) {
        $generator = stoppedOffsetKey($bytes);
        $generator->rewind();
        echo $generator->key(), ":", $generator->current(), "|";
        unset($generator);
    }
    echo "\n";
}
runStoppedOffsetKeys();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Z:1|Z:1|Z:1|Z:1|\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Pending whole-string concat operands and keys are visible to caught-exception cleanup.
#[test]
fn test_string_offset_lifetime_non_byte_pending_operands() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function failPendingString(): string { throw new Exception("pending"); }
function failPendingValue(): int { throw new Exception("pending"); }
function ownedPendingKey(int $i): string { return str_repeat("key", 300) . $i; }
function pendingWholeOperands(bool $flag): void {
    $a = str_repeat("a", 70000); $b = "b";
    $error = new Exception("direct");
    $items = ["seed" => 0];
    for ($i = 0; $i < 8; $i++) {
        try { echo ($a . $b) . failPendingString(); } catch (Exception $e) {}
        try { $items[ownedPendingKey($i)] = failPendingValue(); } catch (Exception $e) {}
        try { echo ($a . $b) . (throw $error); } catch (Exception $e) {}
        try { echo ($a . $b) . ($flag ? throw $error : ""); } catch (Exception $e) {}
    }
}
pendingWholeOperands($argc > 0); echo "done\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "done\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Pending whole-string owners stay balanced when optional EIR optimization is disabled.
#[test]
fn test_string_offset_lifetime_non_byte_pending_operands_without_ir_opt() {
    crate::support::without_ir_opt(test_string_offset_lifetime_non_byte_pending_operands);
}


/// Warning handlers can unwind while a byte, whole string or computed key is pending.
#[test]
fn test_string_offset_lifetime_throwing_warning_pending_operands() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function pendingWarningByte(string $s, int $i): void { echo $s[0] . $s[$i]; }
function pendingWarningWhole(string $s, int $i): void { echo ($s . $s) . $s[$i]; }
function pendingWarningKey(string $s, int $i): void {
    $items = ["seed" => 0]; $items[$s . $s] = $s[$i];
}
function rejectPendingWarning(int $level, string $message): bool {
    throw new Exception("warning");
}
$s = str_repeat("s", 70000);
set_error_handler("rejectPendingWarning");
for ($n = 0; $n < 8; $n++) {
    try { pendingWarningByte($s, $argc + 70000); } catch (Exception $e) {}
    try { pendingWarningWhole($s, $argc + 70000); } catch (Exception $e) {}
    try { pendingWarningKey($s, $argc + 70000); } catch (Exception $e) {}
}
restore_error_handler();
echo "done\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "done\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Throwing warning-handler cleanup also works without optional EIR optimization.
#[test]
fn test_string_offset_lifetime_throwing_warning_pending_operands_without_ir_opt() {
    crate::support::without_ir_opt(test_string_offset_lifetime_throwing_warning_pending_operands);
}

/// A persisted byte receiver remains unwind-visible while its nested index is evaluated.
#[test]
fn test_string_offset_lifetime_throwing_nested_index() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function throwingNestedOffsetIndex(Exception $error): int { throw $error; }
function runThrowingNestedOffsetIndex(string $text, Exception $error): void {
    for ($i = 0; $i < 8; $i++) {
        try { echo $text[0][throwingNestedOffsetIndex($error)]; }
        catch (Exception $caught) { unset($caught); }
    }
}
runThrowingNestedOffsetIndex(str_repeat("Z", 4000), new Exception("index"));
echo "done\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "done\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested-index unwind cleanup also works without optional EIR optimization.
#[test]
fn test_string_offset_lifetime_throwing_nested_index_without_ir_opt() {
    crate::support::without_ir_opt(test_string_offset_lifetime_throwing_nested_index);
}
