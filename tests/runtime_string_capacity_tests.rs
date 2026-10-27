//! Purpose:
//! Regression tests for the shared 64 KiB string-result arena: large mutable copies must move to
//! owned storage, and incremental encoders must fail closed instead of writing past the arena.
//!
//! Called from:
//! - `cargo test --test runtime_string_capacity_tests` through Rust's test harness.
//!
//! Key details:
//! - Every fixture runs in an isolated temporary directory and spawns the compiler binary.
//! - The large-copy fixtures are sized past 64 KiB so a direct `_concat_buf` write would overwrite
//!   `_concat_off` and the adjacent heap, which is exactly the bug these tests pin.
//! - The encoder fixtures assert a controlled fatal (exit 255 + diagnostic), never SIGSEGV, which
//!   is the security property: an oversized result may be rejected but must not corrupt memory.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Resolves the compiler binary built for this integration test.
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Creates an isolated directory for one string-capacity fixture.
fn make_test_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "elephc-string-capacity-{name}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create string-capacity fixture directory");
    dir
}

/// Compiles one PHP source in `dir` and returns the run directory plus the compiler output.
fn compile(dir: &PathBuf, source: &str) -> std::process::Output {
    let php = dir.join("main.php");
    fs::write(&php, source).expect("write string-capacity fixture");
    Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .arg("-q")
        .arg(&php)
        .output()
        .expect("spawn elephc for string-capacity fixture")
}

/// Compiles one PHP source with heap-debug instrumentation enabled.
fn compile_with_heap_debug(dir: &PathBuf, source: &str) -> std::process::Output {
    compile_with_heap_debug_and_size(dir, source, None)
}

/// Compiles one PHP source with heap-debug instrumentation and an optional heap budget.
fn compile_with_heap_debug_and_size(
    dir: &PathBuf,
    source: &str,
    heap_size: Option<usize>,
) -> std::process::Output {
    let php = dir.join("main.php");
    fs::write(&php, source).expect("write string-capacity fixture");
    let mut command = Command::new(elephc_bin());
    command
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .args(["-q", "--heap-debug"]);
    if let Some(size) = heap_size {
        command.arg(format!("--heap-size={size}"));
    }
    command
        .arg(&php)
        .output()
        .expect("spawn heap-debug string-capacity fixture")
}

/// Large mutable copies (`strrev`, `ucfirst`, `lcfirst`, `ucwords`, and the `str_pad` no-op path)
/// must preserve the source and round-trip through owned storage instead of corrupting the heap.
#[test]
fn large_mutable_string_copies_survive_the_concat_arena() {
    let dir = make_test_dir("mutable-copy");
    let compile = compile_with_heap_debug(
        &dir,
        r#"<?php
$seed = "abcdefghijklmnop";
$s = str_repeat($seed, 6250);
$before = md5($s);
$reversed = strrev($s);
$round_trip = strrev($reversed);
$padded = str_pad($s, 10);
$upper_first = ucfirst($s);
$lower_first = lcfirst($s);
$upper_words = ucwords($s);
echo $round_trip === $s ? "rev-ok\n" : "rev-BAD\n";
echo $before === md5($s) ? "src-ok\n" : "src-CORRUPT\n";
echo strlen($reversed), " ", strlen($padded), " ", strlen($upper_first), " ", strlen($lower_first), " ", strlen($upper_words), "\n";
$expected_upper = "A" . substr($s, 1);
echo $padded === $s ? "pad-ok\n" : "pad-BAD\n";
echo $upper_first === $expected_upper ? "ucfirst-ok\n" : "ucfirst-BAD\n";
echo $lower_first === $s ? "lcfirst-ok\n" : "lcfirst-BAD\n";
echo $upper_words === $expected_upper ? "ucwords-ok\n" : "ucwords-BAD\n";
"#,
    );
    assert!(
        compile.status.success(),
        "large mutable-copy fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let output = Command::new(dir.join("main"))
        .output()
        .expect("run large mutable-copy fixture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "large mutable-copy fixture must not fault: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout,
        "rev-ok\nsrc-ok\n100000 100000 100000 100000 100000\npad-ok\nucfirst-ok\nlcfirst-ok\nucwords-ok\n",
        "large copies must match PHP and leave the source intact"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `json_encode()` and `serialize()` must produce results larger than the historical 64 KiB
/// threshold instead of rejecting them or overwriting adjacent runtime state.
#[test]
fn large_json_and_serialize_results_match_php() {
    let dir = make_test_dir("large-encoders");
    let compile = compile(
        &dir,
        r#"<?php
$s = str_repeat("x", 100000);
$json = json_encode($s);
echo strlen($json), " ", $json === '"' . $s . '"' ? "json-ok" : "json-BAD", "\n";
$ser = serialize($s);
echo strlen($ser), " ", $ser === 's:100000:"' . $s . '";' ? "ser-ok" : "ser-BAD", "\n";
$before = md5($s);
echo $before === md5($s) ? "src-ok" : "src-CORRUPT", "\n";
"#,
    );
    assert!(
        compile.status.success(),
        "large encoder fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run large encoder fixture");
    assert!(
        output.status.success(),
        "large encoders must not fault: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "100002 json-ok\n100012 ser-ok\nsrc-ok\n",
        "large JSON and serialize results must stay byte-identical to PHP"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `preg_replace()` must grow its output instead of corrupting the heap when a replacement
/// expands the subject past the historical 64 KiB threshold.
#[test]
fn large_preg_replace_output_matches_php() {
    let dir = make_test_dir("preg-replace-large");
    let compile = compile(
        &dir,
        r#"<?php
$s = str_repeat("a", 100000);
$before = md5($s);
$r = preg_replace("/a/", "Z", $s);
echo strlen($r), " ", $r === str_repeat("Z", 100000) ? "re-ok" : "re-BAD", "\n";
echo $before === md5($s) ? "src-ok" : "src-CORRUPT", "\n";
"#,
    );
    if !compile.status.success() {
        let stderr = String::from_utf8_lossy(&compile.stderr);
        if stderr.contains("pcre2") || stderr.contains("native project error") {
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        panic!("preg_replace fixture must compile: {stderr}");
    }
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run preg_replace fixture");
    assert!(
        output.status.success(),
        "large preg_replace must not fault: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "100000 re-ok\nsrc-ok\n",
        "preg_replace must preserve the subject and return the expanded result"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `preg_replace_callback()` must keep its output in a dedicated growable buffer: the callback
/// may use the shared scratch arena, and the accumulated result used to be copied once per
/// match until the heap was exhausted.
#[test]
fn large_preg_replace_callback_output_matches_php() {
    let dir = make_test_dir("preg-replace-callback-large");
    let compile = compile(
        &dir,
        r#"<?php
$s = str_repeat("a", 1000);
$before = md5($s);
$r = preg_replace_callback("/a/", function ($m) { return str_repeat("b", 1000); }, $s);
echo strlen($r), " ", $r === str_repeat("b", 1000000) ? "cb-ok" : "cb-BAD", "\n";
echo $before === md5($s) ? "src-ok" : "src-CORRUPT", "\n";
"#,
    );
    if !compile.status.success() {
        let stderr = String::from_utf8_lossy(&compile.stderr);
        if stderr.contains("pcre2") || stderr.contains("native project error") {
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        panic!("preg_replace_callback fixture must compile: {stderr}");
    }
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run preg_replace_callback fixture");
    assert!(
        output.status.success(),
        "large preg_replace_callback must not fault: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1000000 cb-ok\nsrc-ok\n",
        "preg_replace_callback must accumulate the callback results without exhausting the heap"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A result larger than the configured heap budget still fails closed through the shared
/// capacity diagnostic: 12 000 control bytes encode to 72 000 JSON bytes, past a 64 KiB
/// program budget, without needing an input that does not fit the heap.
#[test]
fn output_beyond_the_configured_budget_fails_closed() {
    let dir = make_test_dir("configured-budget");
    let source = "<?php\n$s = str_repeat(chr(1), 12000);\necho json_encode($s);\n";
    let php = dir.join("main.php");
    fs::write(&php, source).expect("write configured-budget fixture");
    let compile = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .args(["-q", "--heap-size=65536"])
        .arg(&php)
        .output()
        .expect("compile configured-budget fixture");
    assert!(
        compile.status.success(),
        "configured-budget fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run configured-budget fixture");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(255),
        "an output beyond the configured budget must use PHP's fatal status: {stderr}"
    );
    assert!(
        stderr.contains("string result exceeds the configured string buffer"),
        "expected the controlled capacity diagnostic, got: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Small JSON and serialize results still round-trip after the capacity guards were added.
#[test]
fn small_json_and_serialize_results_are_unchanged() {
    let dir = make_test_dir("small-encoders");
    let compile = compile(
        &dir,
        r#"<?php
echo json_encode(["a" => 1, "b" => [true, null, 1.5]]), "\n";
echo serialize(["a" => 1, "b" => [true, null, 1.5]]), "\n";
"#,
    );
    assert!(
        compile.status.success(),
        "small encoder fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let output = Command::new(dir.join("main"))
        .output()
        .expect("run small encoder fixture");
    assert!(output.status.success(), "small encoders must succeed");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"a\":1,\"b\":[true,null,1.5]}\na:2:{s:1:\"a\";i:1;s:1:\"b\";a:3:{i:0;b:1;i:1;N;i:2;d:1.5;}}\n",
        "small encoder output must stay byte-identical"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `json_decode()` must decode strings larger than the scratch arena into owned storage and must
/// not misreport a valid single-element array as an oversized array allocation.
#[test]
fn large_json_decode_strings_survive_the_concat_arena() {
    let dir = make_test_dir("json-decode-large");
    let compile = compile_with_heap_debug(
        &dir,
        r#"<?php
$s = str_repeat("abcdefghij", 10000);
$before = md5($s);
$string = json_decode('"' . $s . '"');
echo is_string($string) && $string === $s ? "str-ok\n" : "str-BAD\n";
$array = json_decode('["' . $s . '"]', true);
echo is_array($array) && count($array) === 1 && $array[0] === $s ? "arr-ok\n" : "arr-BAD\n";
$object = json_decode('{"k":"' . $s . '"}', true);
echo is_array($object) && $object["k"] === $s ? "obj-ok\n" : "obj-BAD\n";
echo $before === md5($s) ? "src-ok\n" : "src-CORRUPT\n";
"#,
    );
    assert!(
        compile.status.success(),
        "large json_decode fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run large json_decode fixture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "large json_decode fixture must not fault: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout, "str-ok\narr-ok\nobj-ok\nsrc-ok\n",
        "json_decode must preserve large decoded strings and the input heap"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `shell_exec()` must grow its output buffer instead of writing command output past the scratch
/// arena; a 100 KB command output used to corrupt the heap and abort the process.
#[test]
fn large_shell_exec_output_survives_the_concat_arena() {
    let dir = make_test_dir("shell-exec-large");
    let compile = compile_with_heap_debug(
        &dir,
        r#"<?php
$out = shell_exec('head -c 100000 /dev/zero | tr "\0" a');
echo is_string($out) ? strlen($out) : -1, "\n";
echo is_string($out) ? md5($out) : "", "\n";
echo is_string($out) && $out === str_repeat("a", 100000) ? "exec-ok\n" : "exec-BAD\n";
"#,
    );
    assert!(
        compile.status.success(),
        "large shell_exec fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run large shell_exec fixture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "large shell_exec fixture must not fault: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout,
        "100000\n1af6d6f2f682f76f80e606aeaaee1680\nexec-ok\n",
        "shell_exec must return the complete command output"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `grapheme_strrev()` must reserve owned storage for a result larger than the scratch arena.
#[test]
fn large_grapheme_strrev_survives_the_concat_arena() {
    let dir = make_test_dir("grapheme-strrev");
    let compile = compile_with_heap_debug(
        &dir,
        r#"<?php
$s = str_repeat("abcdefghij", 10000);
$before = md5($s);
$reversed = grapheme_strrev($s);
echo strlen($reversed), "\n";
echo $reversed === strrev($s) ? "rev-ok\n" : "rev-BAD\n";
echo $before === md5($s) ? "src-ok\n" : "src-CORRUPT\n";
"#,
    );
    assert!(
        compile.status.success(),
        "large grapheme_strrev fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run large grapheme_strrev fixture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "large grapheme_strrev fixture must not fault: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout, "100000\nrev-ok\nsrc-ok\n",
        "grapheme_strrev must reverse large inputs without corrupting the heap"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A small JSON result must fit beside a large echo instead of reserving a fixed 4 KiB margin.
///
/// The container loop used to demand 4096 free bytes before checking whether any
/// element remained, so `[1]` fataled even though it needs only three bytes. The
/// loop now checks completion first and asks the capacity guard for the bytes the
/// next element actually needs.
#[test]
fn small_json_results_fit_after_a_large_echo() {
    let dir = make_test_dir("json-headroom");
    let php = dir.join("main.php");
    fs::write(
        &php,
        r#"<?php
echo str_repeat("x", 62000), json_encode([1, 2, 3]), "|", json_encode(["k" => 1]);
"#,
    )
    .expect("write json-headroom fixture");
    let compile = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .args(["-q", "--heap-size=65536"])
        .arg(&php)
        .output()
        .expect("compile json-headroom fixture");
    assert!(
        compile.status.success(),
        "json-headroom fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run json-headroom fixture");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "unexpected fatal: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.len(), 62000 + 7 + 1 + 7, "unexpected output length");
    assert!(stdout.ends_with("[1,2,3]|{\"k\":1}"), "tail: {:?}", &stdout[61990..]);
    let _ = fs::remove_dir_all(&dir);
}

/// A small serialized value must fit beside a large echo instead of reserving 64 bytes.
#[test]
fn small_serialized_results_fit_after_a_large_echo() {
    let dir = make_test_dir("serialize-headroom");
    let php = dir.join("main.php");
    fs::write(
        &php,
        r#"<?php
echo str_repeat("x", 65500), "|", serialize(1);
"#,
    )
    .expect("write serialize-headroom fixture");
    let compile = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .args(["-q", "--heap-size=65536"])
        .arg(&php)
        .output()
        .expect("compile serialize-headroom fixture");
    assert!(
        compile.status.success(),
        "serialize-headroom fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run serialize-headroom fixture");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "unexpected fatal: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.len(), 65505, "unexpected output length");
    assert!(stdout.ends_with("|i:1;"), "tail: {:?}", &stdout[65490..]);
    let _ = fs::remove_dir_all(&dir);
}

/// Large decoded/reversed values must exercise the heap fallback after the arena is full.
#[test]
fn heap_fallback_is_clean_after_a_large_arena_prefix() {
    let dir = make_test_dir("heap-fallback");
    let compile = compile_with_heap_debug_and_size(
        &dir,
        r#"<?php
echo str_repeat("x", 65000), "|";
$decoded = json_decode('"' . str_repeat("y", 1000) . '"');
$reversed = grapheme_strrev(str_repeat("z", 1000));
echo strlen($decoded), "|", strlen($reversed), "\n";
"#,
        Some(65536),
    );
    assert!(
        compile.status.success(),
        "heap-fallback fixture must compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("run heap-fallback fixture");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "unexpected fatal: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.len(), 65000 + 1 + 4 + 1 + 4 + 1, "{}", &stdout[..64.min(stdout.len())]);
    assert!(
        stdout.ends_with("|1000|1000\n"),
        "heap fallback must preserve the decoded and reversed values"
    );
    assert!(
        stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}
