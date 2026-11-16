//! Purpose:
//! Verifies dynamic stream-filter names and parameter containers retire independently.
//!
//! Called from:
//! - The runtime GC integration suite on every executable target.
//!
//! Key details:
//! - Covers append, prepend, failed lookup, shared directions and throwing lifecycle hooks.

use crate::support::*;

/// Fresh attachment results release dynamic names while callbacks keep their own params.
#[test]
fn test_dynamic_stream_filter_names_and_user_params_owners_bundle() {
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(r#"<?php
class OwnedStreamParamsFilter extends php_user_filter {
    public static bool $throwOnClose = false;
    public static bool $throwOnCreate = false;
    public static bool $rejectOnCreate = false;
    public function onCreate(): bool {
        echo strlen($this->params['prefix']), '|';
        if (self::$throwOnCreate) { throw new RuntimeException('create'); }
        return !self::$rejectOnCreate;
    }
    public function filter(string $data): string { return $data . $this->params['suffix']; }
    public function onClose(): void {
        echo 'close|';
        if (self::$throwOnClose) { throw new RuntimeException('close'); }
    }
    public function __destruct() { echo 'drop|'; }
}
class OwnedStreamDefaultFilter extends OwnedStreamParamsFilter { public mixed $params = 'initial'; }
stream_filter_register('xml.owner.params', 'OwnedStreamParamsFilter');
stream_filter_register('xml.owner.defaults', 'OwnedStreamDefaultFilter');
$stream = fopen('php://memory', 'r+');
$name = strtolower('XML.OWNER.PARAMS');
if ($argc === 1) {
    $params = ['prefix' => str_repeat('p', 40), 'suffix' => '>'];
    stream_filter_append($stream, $name, STREAM_FILTER_WRITE, $params);
    echo strlen($params['prefix']), '|';
    unset($params);
} elseif ($argc === 2) {
    stream_filter_prepend($stream, $name, STREAM_FILTER_WRITE, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    echo 'fresh|';
} elseif ($argc === 3) {
    $name = strtolower('XML.OWNER.MISSING');
    $result = stream_filter_append($stream, $name, STREAM_FILTER_WRITE, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    echo $result === false ? 'missing|' : 'wrong|';
} elseif ($argc === 4) {
    stream_filter_append($stream, $name, STREAM_FILTER_ALL, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    echo 'all|';
} elseif ($argc === 5) {
    OwnedStreamParamsFilter::$throwOnClose = true;
    stream_filter_append($stream, $name, STREAM_FILTER_WRITE, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    echo 'throw|';
} elseif ($argc === 6) {
    stream_filter_append($stream, $name, STREAM_FILTER_WRITE, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    stream_filter_append($stream, $name, STREAM_FILTER_READ, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    echo 'both|';
} elseif ($argc === 7) {
    stream_filter_append($stream, 'xml.owner.defaults', STREAM_FILTER_WRITE, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    echo 'default|';
} elseif ($argc === 8) {
    OwnedStreamParamsFilter::$throwOnCreate = true;
    try { stream_filter_append($stream, $name, STREAM_FILTER_WRITE, ['prefix' => str_repeat('p', 40), 'suffix' => '>']); }
    catch (RuntimeException $error) { echo 'created|'; }
} else {
    OwnedStreamParamsFilter::$rejectOnCreate = true;
    $attached = stream_filter_append($stream, $name, STREAM_FILTER_WRITE, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    echo $attached === false ? 'rejected|' : 'wrong|';
}
fwrite($stream, 'hello'); rewind($stream); echo fread($stream, 64);
try { fclose($stream); } catch (RuntimeException $error) { echo 'caught|'; }
"#, &[&[], &["x"], &["x", "x"], &["x", "x", "x"], &["x", "x", "x", "x"], &["x", "x", "x", "x", "x"], &["x", "x", "x", "x", "x", "x"], &["x", "x", "x", "x", "x", "x", "x"], &["x", "x", "x", "x", "x", "x", "x", "x"]]);
    // PHP keeps a filter whose onCreate throws attached until fclose; refusal retires it immediately.
    let expected = ["40|40|hello>close|drop|", "40|fresh|hello>close|drop|", "missing|hello", "40|all|hello>>close|drop|", "40|throw|hello>close|drop|caught|", "40|40|both|hello>>close|drop|close|drop|", "40|default|hello>close|drop|", "40|created|hello>close|drop|", "40|drop|rejected|hello"];
    assert_eq!(runs.len(), expected.len());
    let mut failures = Vec::new();
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} success={} stdout={:?} expected={expected:?}\n{}", out.success, out.stdout, out.stderr));
        }
    }
    let symbols = ["filter", "onCreate", "onClose", "__destruct"].map(|method|
        elephc::names::method_symbol("OwnedStreamParamsFilter", method));
    let mut symbols = symbols.to_vec();
    symbols.push("_main".into());
    assert!(failures.is_empty(), "{}\nFilter parameter assembly:\n{}", failures.join("\n"), selected_function_assembly(&assembly, &symbols));
}

/// Callback bytes are independent for borrowed, owned and input-alias returns in both directions.
#[test]
fn test_stream_filter_string_result_owners_bundle() {
    let runs = compile_and_run_with_heap_debug_per_argv(r#"<?php
class StreamResultOwnerFilter extends php_user_filter {
    private string $bytes = 'borrowed-bytes';
    public function filter(string $data): string {
        if ($this->params === 'owned') { return str_repeat('f', 17); }
        if ($this->params === 'borrowed') { return $this->bytes; }
        return $data;
    }
}
stream_filter_register('owner.result', 'StreamResultOwnerFilter');
$stream = fopen('php://memory', 'w+');
$kind = ($argc === 1 || $argc === 4) ? 'owned' : (($argc === 2 || $argc === 5) ? 'borrowed' : 'alias');
if ($argc <= 3) {
    stream_filter_append($stream, 'owner.result', STREAM_FILTER_WRITE, $kind);
    fwrite($stream, 'input'); rewind($stream);
} else {
    fwrite($stream, 'input'); rewind($stream);
    stream_filter_append($stream, 'owner.result', STREAM_FILTER_READ, $kind);
}
$bytes = fread($stream, 64); echo $bytes; fclose($stream); unset($bytes);
"#, &[&[], &["x"], &["x", "x"], &["x", "x", "x"], &["x", "x", "x", "x"], &["x", "x", "x", "x", "x"]]);
    for (index, out) in runs.iter().enumerate() {
        assert!(out.success, "mode={index}: {}", out.stderr);
        assert_eq!(out.stdout, ["fffffffffffffffff", "borrowed-bytes", "input"][index % 3], "mode={index}");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={index}: {}", out.stderr);
    }
}

/// Raw and boxed wrapper results detach borrowed bytes and retire every owned source cell.
#[test]
fn test_stream_wrapper_string_result_owners_bundle() {
    let runs = compile_and_run_with_heap_debug_per_argv(r#"<?php
class WrapperResultOwnerBase {
    public mixed $context = null;
    public static bool $fresh = false;
    protected bool $read = false;
    public function stream_open(string $path, string $mode, int $options, mixed &$opened): bool { return true; }
    public function stream_eof(): bool { return $this->read; }
}
class RawWrapperResultOwner extends WrapperResultOwnerBase {
    private string $bytes = 'borrowed-bytes';
    public function stream_read(int $count): string {
        if ($this->read) { return ''; } $this->read = true;
        if (self::$fresh) { return str_repeat('f', 17); }
        return $this->bytes;
    }
}
class BoxedWrapperResultOwner extends WrapperResultOwnerBase {
    private mixed $bytes = 'borrowed-bytes';
    public function stream_read(int $count): mixed {
        if ($this->read) { return false; } $this->read = true;
        if (self::$fresh) { return str_repeat('f', 17); }
        return $this->bytes;
    }
}
WrapperResultOwnerBase::$fresh = $argc === 1 || $argc === 3;
stream_wrapper_register('ownerresult', $argc <= 2 ? 'RawWrapperResultOwner' : 'BoxedWrapperResultOwner');
$stream = fopen('ownerresult://bytes', 'rb');
echo fread($stream, 64), '|', strlen(fread($stream, 64)); fclose($stream);
stream_wrapper_unregister('ownerresult');
"#, &[&[], &["x"], &["x", "x"], &["x", "x", "x"]]);
    for (index, out) in runs.iter().enumerate() {
        assert!(out.success, "mode={index}: {}", out.stderr);
        assert_eq!(out.stdout, if index % 2 == 0 { "fffffffffffffffff|0" } else { "borrowed-bytes|0" }, "mode={index}");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={index}: {}", out.stderr);
    }
}

/// Fresh count results never keep a narrowed union string argument alive after a write.
#[test]
fn test_fwrite_releases_owned_and_narrowed_string_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function preparedWriteInput(int $count): string|false {
    return $count > 0 ? str_repeat('write', $count) : false;
}
function writeUnionInput(mixed $stream, int $count): void {
    $data = preparedWriteInput($count);
    if ($data === false) { return; }
    echo fwrite($stream, $data), ':';
}
$stream = fopen('php://temp', 'w+');
for ($index = 0; $index < 3; $index++) {
    writeUnionInput($stream, $argc);
    echo fwrite($stream, str_repeat('x', $argc)), '|';
}
rewind($stream);
echo stream_get_contents($stream);
fclose($stream);
unset($stream);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "5:1|5:1|5:1|writexwritexwritex", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
