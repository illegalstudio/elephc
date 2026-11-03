//! Purpose:
//! Integration coverage for removing the key binding of an array or hash `foreach` whose key
//! variable is never read.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::foreach_key_elision`.
//!
//! Key details:
//! - `--emit-ir` shows whether the `iter_current_key` read survives; the executed programs show
//!   that every observable reader of the key (a later read, a by-value closure capture, a
//!   reference alias, an object iterator's `key()`) keeps the binding intact.

use super::*;

/// Emits textual EIR for a source snippet through the CLI.
fn emit_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_foreach_key_elision_emit_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write PHP fixture");

    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir");
    if !ir_opt {
        command.arg("--no-ir-opt");
    }
    let output = command
        .arg(&php_path)
        .output()
        .expect("failed to run elephc --emit-ir");
    assert!(
        output.status.success(),
        "elephc --emit-ir failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR output should be UTF-8");
    let _ = fs::remove_dir_all(&dir);
    text
}

/// Extracts the textual EIR for one function from a printed module.
fn function_ir<'a>(module: &'a str, signature_prefix: &str) -> &'a str {
    let marker = format!("  function {}", signature_prefix);
    let start = module
        .find(&marker)
        .unwrap_or_else(|| panic!("missing function `{}` in:\n{}", signature_prefix, module));
    let rest = &module[start..];
    let next = rest[marker.len()..]
        .find("\n  function ")
        .map(|offset| marker.len() + offset)
        .unwrap_or(rest.len());
    &rest[..next]
}

/// A key nothing reads is not materialized per element; `--no-ir-opt` keeps it.
#[test]
fn test_foreach_key_elision_drops_an_unread_hash_key() {
    let source = r#"<?php
function total(int $n): int {
    $h = ["a" => 1, "b" => 2];
    $h["c"] = $n + 3;
    $t = 0;
    foreach ($h as $k => $v) { $t += $v; }
    return $t;
}
echo total($argc);
"#;
    let optimized = emit_ir(source, true);
    assert!(
        !function_ir(&optimized, "total(").contains("iter_current_key"),
        "an unread foreach key should not be materialized:\n{}",
        function_ir(&optimized, "total(")
    );
    let unoptimized = emit_ir(source, false);
    assert!(
        function_ir(&unoptimized, "total(").contains("iter_current_key"),
        "--no-ir-opt should keep the key read"
    );
    assert_eq!(compile_and_run(source), "7");
}

/// The same holds for an indexed array, and for a loop whose key name is reused by a second loop.
#[test]
fn test_foreach_key_elision_drops_unread_indexed_and_repeated_keys() {
    let source = r#"<?php
function total(): int {
    $s = 0;
    foreach ([1 => 10, 2 => 20] as $k => $v) { $s += $v; }
    foreach (["x" => 1, "y" => 2] as $k => $v) { $s += $v; }
    foreach ([5, 6, 7] as $k => $v) { $s += $v; }
    return $s;
}
echo total();
"#;
    let optimized = emit_ir(source, true);
    assert!(
        !function_ir(&optimized, "total(").contains("iter_current_key"),
        "{}",
        function_ir(&optimized, "total(")
    );
    assert_eq!(compile_and_run(source), "51");
}

/// A key the program reads, in any way, keeps its binding and its post-loop value.
#[test]
fn test_foreach_key_elision_keeps_every_observable_key() {
    let source = r#"<?php
foreach (["x" => 1] as $k => $v) {}
echo $k, $v, "\n";

function after(): string {
    foreach (["p" => 1, "q" => 2] as $k => $v) {}
    return $k . $v;
}
echo after(), "\n";

function captured(): string {
    foreach (["a" => 1, "b" => 2] as $k => $v) {}
    $f = function () use ($k) { return $k; };
    return $f();
}
echo captured(), "\n";

function aliased(): string {
    foreach (["a" => 1, "b" => 2] as $k => $v) {}
    $r = &$k;
    $r = "z";
    return $k;
}
echo aliased(), "\n";

function seen(): string {
    $o = "";
    foreach (["5" => "a", "07" => "b", "x" => "c"] as $k => $v) { $o .= gettype($k) . $k . $v . ","; }
    return $o;
}
echo seen(), "\n";

function emptied(): string {
    foreach ([] as $k => $v) {}
    return isset($k) ? "set" : "unset";
}
echo emptied(), "\n";
"#;
    assert_eq!(
        compile_and_run(source),
        "x1\nq2\nb\nz\ninteger5a,string07b,stringxc,\nunset\n"
    );
    let optimized = emit_ir(source, true);
    for name in ["after(", "captured(", "aliased(", "seen("] {
        assert!(
            function_ir(&optimized, name).contains("iter_current_key"),
            "{name} reads its key and must keep the binding"
        );
    }
}

/// What the key variable held before the loop is released at the first binding, as in php, even
/// when the key read itself is skipped: an object there is destructed before the body runs.
#[test]
fn test_foreach_key_elision_still_releases_the_previous_key_value() {
    let source = r#"<?php
class D { function __destruct() { echo "destroy\n"; } }
function f() {
    $k = new D;
    foreach (["x" => 1, "y" => 2] as $k => $v) { echo "body\n"; }
    echo "end\n";
}
f();
"#;
    let optimized = emit_ir(source, true);
    assert!(
        !function_ir(&optimized, "f(").contains("iter_current_key"),
        "the key read should still be skipped:\n{}",
        function_ir(&optimized, "f(")
    );
    assert_eq!(compile_and_run(source), "destroy\nbody\nbody\nend\n");
}

/// An object iterator's `key()` is user code: it still runs when the key variable is unused.
#[test]
fn test_foreach_key_elision_still_calls_an_iterator_key_method() {
    let source = r#"<?php
class It implements Iterator {
    private int $i = 0;
    public function current(): mixed { return $this->i * 2; }
    public function key(): mixed { echo "[key]"; return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 3; }
}
function run(): int { $s = 0; foreach (new It as $k => $v) { $s += $v; } return $s; }
echo run(), "\n";
function gen() { yield "a" => 1; yield "b" => 2; }
function runGen(): int { $s = 0; foreach (gen() as $k => $v) { $s += $v; } return $s; }
echo runGen(), "\n";
"#;
    assert_eq!(compile_and_run(source), "[key][key][key]6\n3\n");
}

/// Skipping the key must not disturb mutation during iteration, early exits, exceptions, nested
/// loops over the same array, or by-reference iteration.
#[test]
fn test_foreach_key_elision_keeps_loop_semantics() {
    let source = r#"<?php
function mutate(): string {
    $h = ["a" => 1, "b" => 2];
    $o = "";
    foreach ($h as $k => $v) { $h["z" . $v] = 9; $o .= $v; }
    return $o . count($h);
}
echo mutate(), "\n";
function early(): int {
    $s = 0;
    foreach (["a" => 1, "b" => 2, "c" => 3] as $k => $v) { if ($v == 2) break; $s += $v; }
    return $s;
}
echo early(), "\n";
function thrown(): string {
    try {
        foreach (["a" => 1, "b" => 2] as $k => $v) { if ($v == 2) throw new Exception("boom"); }
    } catch (Exception $e) { return $e->getMessage(); }
    return "none";
}
echo thrown(), "\n";
function nested(): string {
    $h = ["a" => 1, "b" => 2];
    $o = "";
    foreach ($h as $k => $v) { foreach ($h as $k => $w) { $o .= $v . $w; } }
    return $o;
}
echo nested(), "\n";
function byRef(): string {
    $h = ["a" => 1, "b" => 2];
    foreach ($h as $k => &$v) { $v *= 10; }
    unset($v);
    return json_encode($h);
}
echo byRef(), "\n";
$tot = 0;
for ($i = 0; $i < 3; $i++) { foreach (["a" => $i, "b" => 2] as $kk => $vv) { $tot += $vv; } }
echo $tot, "\n";
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "124\n1\nboom\n11122122\n{\"a\":10,\"b\":20}\n9\n");
}
