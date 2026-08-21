//! Purpose:
//! End-to-end and both-target tests for php's KEY PRESERVATION in the value set operations
//! `array_diff()`, `array_intersect()` and `array_unique()`.
//!
//! Called from:
//! - `cargo test --test array_set_op_key_preservation_tests` through Rust's test harness.
//!
//! Key details:
//! - THE DEFECT: all three REINDEXED their result 0..n where php keeps the first operand's keys.
//!   Measured against reference PHP 8.5.6 (`php -n`):
//!     `array_diff(["a","b","c"], ["b"])`          php `{"0":"a","2":"c"}`     was `["a","c"]`
//!     `array_intersect(["a","b","c"], ["b","c"])` php `{"1":"b","2":"c"}`     was `["b","c"]`
//!     `array_unique(["a","b","a","c"])`           php `{"0":"a","1":"b","3":"c"}` was `["a","b","c"]`
//!   It is php-VISIBLE, not cosmetic: `array_keys(array_diff(["a","b","c"], ["b"]))` is `[0, 2]`
//!   in php and was `[0, 1]` here. It matters because `array_diff(scandir($d), [".", ".."])` is
//!   the commonest directory idiom in PHP and its keys were wrong.
//! - php-src (PHP-8.4, ext/standard/array.c) confirms the whole family preserves keys, by two
//!   mechanisms and never `zend_hash_next_index_insert`:
//!     * `PHP_FUNCTION(array_diff)` re-adds survivors with `zend_hash_index_add_new(…, idx, …)`;
//!     * `php_array_intersect` COPIES the first array (`zend_array_dup`) and `zend_hash_index_del`s
//!       the entries that are not common;
//!     * `array_unique`'s default `SORT_STRING` path re-adds first occurrences with
//!       `zend_hash_index_add_new(…, num_key, …)`.
//! - THE DESIGN: a dense indexed array cannot represent `{0:"a", 2:"c"}`, so the result type is
//!   now `AssocArray { key: Int, value: T }` and the three lower to `__rt_array_*_to_hash`, which
//!   insert each survivor at its ORIGINAL index. This mirrors `array_slice($a,$o,$l,true)`, which
//!   already returned an int-keyed hash through `__rt_array_slice_to_hash`.
//! - Every expected value in this file was taken from reference PHP 8.5.6 (`php -n`).
//! - The both-target test pins the helpers in the REAL generated runtime for macos-aarch64 AND
//!   linux-x86_64. The end-to-end tests can only run on the host, and a full
//!   `--target linux-x86_64` compile cannot complete on an aarch64 host (the runtime cache is
//!   assembled with the host `as`, which rejects `xor eax, eax`), so generating the runtime text
//!   directly is what gives the hand-written x86_64 bodies real coverage — this repo has shipped
//!   one-arch-only set-operation defects before.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` with extra flags, runs it, and returns its stdout.
fn compile_and_run_with_flags(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> String {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("-q").args(flags).arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(dir.join(stem))
        .current_dir(dir)
        .output()
        .expect("failed to run compiled program");
    let mut text = String::from_utf8_lossy(&run.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&run.stderr));
    text
}

/// Compiles `source`, runs it, and returns its stdout.
fn compile_and_run(dir: &Path, source: &str, stem: &str) -> String {
    compile_and_run_with_flags(dir, source, stem, &[])
}

/// The three set operations keep the FIRST operand's keys, exactly as php does.
///
/// The `json_encode` form is the load-bearing assertion: it is the one that distinguishes
/// `{0:"a", 2:"c"}` from the reindexed `["a","c"]` this used to return. `implode()` and `count()`
/// agreed even while the keys were wrong, which is why the divergence survived so long.
#[test]
fn set_operations_preserve_the_source_keys_like_php() {
    let dir = make_test_dir("setop_keys");
    let out = compile_and_run(
        &dir,
        r#"<?php
echo json_encode(array_diff(["a","b","c"], ["b"])), "\n";
echo json_encode(array_intersect(["a","b","c"], ["b","c"])), "\n";
echo json_encode(array_unique(["a","b","a","c"])), "\n";
echo json_encode(array_keys(array_diff(["a","b","c"], ["b"]))), "\n";
"#,
        "keys",
    );
    assert_eq!(
        out,
        "{\"0\":\"a\",\"2\":\"c\"}\n\
         {\"1\":\"b\",\"2\":\"c\"}\n\
         {\"0\":\"a\",\"1\":\"b\",\"3\":\"c\"}\n\
         [0,2]\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The same preservation over INT elements, and when the FIRST element is the one dropped.
///
/// Dropping element 0 is the case a dense array cannot fake: the result has to start at key 1.
#[test]
fn set_operations_preserve_keys_for_int_elements_and_dropped_heads() {
    let dir = make_test_dir("setop_ints");
    let out = compile_and_run(
        &dir,
        r#"<?php
echo json_encode(array_diff([1,2,3,4], [2,4])), "\n";
echo json_encode(array_intersect([1,2,3,4], [2,4])), "\n";
echo json_encode(array_unique([1,2,1,3,2])), "\n";
echo json_encode(array_diff(["a","b","c"], ["a"])), "\n";
echo json_encode(array_keys(array_diff(["a","b","c"], ["a"]))), "\n";
"#,
        "ints",
    );
    assert_eq!(
        out,
        "{\"0\":1,\"2\":3}\n\
         {\"1\":2,\"3\":4}\n\
         {\"0\":1,\"1\":2,\"3\":3}\n\
         {\"1\":\"b\",\"2\":\"c\"}\n\
         [1,2]\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The directory idiom the defect actually broke, plus the standard `array_values()` reindex.
#[test]
fn the_directory_idiom_keeps_the_scandir_keys() {
    let dir = make_test_dir("setop_dir");
    let out = compile_and_run(
        &dir,
        r#"<?php
mkdir("d");
file_put_contents("d/one.txt", "1");
file_put_contents("d/two.txt", "2");
$files = array_diff(scandir("d"), [".", ".."]);
echo json_encode($files), "\n";
echo implode(",", $files), "\n";
echo count($files), "\n";
echo json_encode(array_values($files)), "\n";
foreach ($files as $k => $v) { echo $k, "=", $v, ";"; }
echo "\n";
unlink("d/one.txt"); unlink("d/two.txt"); rmdir("d");
"#,
        "dirs",
    );
    assert_eq!(
        out,
        "{\"2\":\"one.txt\",\"3\":\"two.txt\"}\n\
         one.txt,two.txt\n\
         2\n\
         [\"one.txt\",\"two.txt\"]\n\
         2=one.txt;3=two.txt;\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `implode()` joins an int-keyed hash by VALUES, and owns nothing afterwards.
///
/// php's `implode()` reads only the values, so the joined text is unchanged by the key fix — but
/// the join now goes through a TEMPORARY indexed array of the hash's values, which holds
/// persisted string copies. `--heap-debug` is the witness that the temporary is deep-freed: a
/// missing free is invisible to a functional assertion, which is why the loop and the leak
/// summary are both part of this test.
#[test]
fn imploding_a_key_preserving_result_joins_values_and_leaks_nothing() {
    let dir = make_test_dir("setop_implode");
    let out = compile_and_run_with_flags(
        &dir,
        r#"<?php
for ($i = 0; $i < 100; $i++) {
    $d = implode(",", array_diff(["alpha","beta","gamma"], ["beta"]));
    $n = implode(",", array_intersect(["alpha","beta","gamma"], ["beta","gamma"]));
    $u = implode(",", array_unique(["alpha","beta","alpha","gamma"]));
    $ints = implode("-", array_diff([10,20,30], [20]));
}
echo $d, "\n", $n, "\n", $u, "\n", $ints, "\n";
"#,
        "implode",
        &["--heap-debug"],
    );
    assert!(
        out.starts_with("alpha,gamma\nbeta,gamma\nalpha,beta,gamma\n10-30\n"),
        "unexpected join output:\n{out}"
    );
    assert!(
        out.contains("leak summary: clean"),
        "the implode values temporary was not released:\n{out}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The key-preserving results stay usable through the hash-native consumers.
#[test]
fn a_key_preserving_result_reads_like_any_other_hash() {
    let dir = make_test_dir("setop_consume");
    let out = compile_and_run(
        &dir,
        r#"<?php
$d = array_diff(["x","y","z"], ["y"]);
echo count($d), "\n";
echo $d[0], $d[2], "\n";
echo (isset($d[2]) ? "set" : "unset"), (isset($d[1]) ? "set" : "unset"), "\n";
echo (in_array("z", $d) ? "in" : "out"), "\n";
var_dump(array_search("z", $d));
echo json_encode(array_flip($d)), "\n";
print_r($d);
"#,
        "consume",
    );
    assert_eq!(
        out,
        "2\n\
         xz\n\
         setunset\n\
         in\n\
         int(2)\n\
         {\"x\":0,\"z\":2}\n\
         Array\n(\n    [0] => x\n    [2] => z\n)\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Pins the three key-preserving helpers inside the REAL generated runtime, on BOTH targets.
///
/// Generating the runtime text directly is the only mechanism that covers the hand-written
/// x86_64 bodies from an aarch64 host, and it proves something an emitter unit test cannot: that
/// each helper is actually REGISTERED in `emit_runtime`. The needles are the load-bearing
/// instructions of the key story — the `-1` key_hi that marks an INTEGER key, and the
/// `__rt_hash_set` that writes the survivor at the source index — so a regression that went back
/// to appending into a dense array cannot keep them.
#[test]
fn the_runtime_defines_the_key_preserving_set_operations_on_both_targets() {
    for (target_name, int_key_marker) in [
        ("macos-aarch64", "mov x2, #-1"),
        ("linux-x86_64", "mov rdx, -1"),
    ] {
        let target = elephc::codegen::platform::Target::parse(target_name).expect("valid target");
        let runtime_asm = elephc::codegen::generate_runtime_with_features(
            8_388_608,
            target,
            elephc::codegen::RuntimeFeatures::none(),
        );
        for helper in [
            "__rt_array_diff_to_hash",
            "__rt_array_intersect_to_hash",
            "__rt_array_unique_to_hash",
        ] {
            let start = runtime_asm
                .find(&format!("--- runtime: {} ---", helper))
                .unwrap_or_else(|| panic!("{helper} is not registered in the {target_name} runtime"));
            // The done LABEL, not the first branch to it: the loop jumps there long before the
            // insertion this test is pinning, so slicing at the branch would cut the body short.
            let end = runtime_asm[start..]
                .find(&format!("{}_done:", helper))
                .map(|offset| start + offset)
                .unwrap_or_else(|| panic!("{helper} has no done label on {target_name}"));
            let body = &runtime_asm[start..end];
            for needle in [int_key_marker, "__rt_hash_set", "__rt_hash_new"] {
                assert!(
                    body.contains(needle),
                    "{helper} on {target_name} is missing {needle:?}; \
                     a key-preserving set operation must insert at an INTEGER key"
                );
            }
        }
    }
}
