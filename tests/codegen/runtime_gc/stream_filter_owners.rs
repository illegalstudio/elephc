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
    let runs = compile_and_run_with_heap_debug_per_argv(r#"<?php
class OwnedStreamParamsFilter extends php_user_filter {
    public static bool $throwOnClose = false;
    public function onCreate(): bool { echo strlen($this->params['prefix']), '|'; return true; }
    public function filter(string $data): string { return $data . $this->params['suffix']; }
    public function onClose(): void {
        echo 'close|';
        if (self::$throwOnClose) { throw new RuntimeException('close'); }
    }
    public function __destruct() { echo 'drop|'; }
}
stream_filter_register('xml.owner.params', 'OwnedStreamParamsFilter');
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
} else {
    stream_filter_append($stream, $name, STREAM_FILTER_WRITE, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    stream_filter_append($stream, $name, STREAM_FILTER_READ, ['prefix' => str_repeat('p', 40), 'suffix' => '>']);
    echo 'both|';
}
fwrite($stream, 'hello'); rewind($stream); echo fread($stream, 64);
try { fclose($stream); } catch (RuntimeException $error) { echo 'caught|'; }
"#, &[&[], &["x"], &["x", "x"], &["x", "x", "x"], &["x", "x", "x", "x"], &["x", "x", "x", "x", "x"]]);
    let expected = ["40|40|hello>close|drop|", "40|fresh|hello>close|drop|", "missing|hello", "40|all|hello>>close|drop|", "40|throw|hello>close|drop|caught|", "40|40|both|hello>>close|drop|close|drop|"];
    assert_eq!(runs.len(), expected.len());
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        assert!(out.success, "mode={mode} stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "mode={mode} stderr={}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={mode} {}", out.stderr);
    }
}
