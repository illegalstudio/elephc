//! Purpose:
//! Integration tests for php's stat cache as a user-registered wrapper sees it: how many times
//! `url_stat()` is actually called, and what empties the answer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - php holds ONE cached answer per kind, not one per query: `filesize()`, `file_exists()`,
//!   `is_dir()`, `is_file()` and `filemtime()` on one path call `url_stat()` once between them.
//!   elephc called it five times, so a wrapper whose `url_stat` is expensive or has side effects
//!   saw five.
//! - Two slots, because a LINK query keeps its own: `filesize()` then `is_link()` calls twice. An
//!   lstat answer can serve an ordinary stat, but only when what it found is not a symlink.
//! - `clearstatcache()` is the ONLY thing that empties either — measured, `unlink()`, `rename()`,
//!   `touch()`, `chmod()` and a write through `fopen()` all leave the cached answer standing.
//! - Every expectation MEASURED on `php -n` 8.5.6.

use crate::support::*;

/// A wrapper that announces every `url_stat` call, with the flags it was asked with.
const WRAPPER: &str = r#"<?php
class Sc {
    public $context;
    public function url_stat($path, $flags) {
        echo "url_stat($path,$flags)\n";
        if ($path === "sc://link") return ["size" => 3, "mode" => 0120777];
        if ($path === "sc://gone") return false;
        return ["size" => 10, "mode" => 0100644];
    }
    public function unlink($p) { return true; }
    public function rename($a, $b) { return true; }
    public function stream_metadata($p, $o, $v) { return true; }
    public function stream_open($p, $m, $o, &$x) { return true; }
    public function stream_write($d) { return strlen($d); }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
    public function stream_close() {}
}
stream_wrapper_register("sc", "Sc");
"#;

/// Compiles `WRAPPER` followed by `body` and returns the program's captured output.
fn run(body: &str) -> ProgramOutput {
    compile_and_run_capture(&format!("{WRAPPER}{body}\n"))
}

/// Asserts the exact `url_stat` trace, ignoring whatever the queries themselves printed.
fn assert_trace(out: &ProgramOutput, expected: &[&str]) {
    assert!(out.success, "program failed: {}", out.stderr);
    let calls: Vec<&str> = out
        .stdout
        .lines()
        .filter(|l| l.starts_with("url_stat("))
        .collect();
    assert_eq!(calls, expected, "full stdout was:\n{}", out.stdout);
}

/// Verifies five ordinary queries on one path ask the wrapper ONCE.
///
/// This is the whole point: elephc asked five times, with the flags of whichever builtin was
/// running, so the wrapper saw a call per query instead of a call per path.
#[test]
fn test_five_ordinary_queries_ask_the_wrapper_once() {
    let out = run(
        r#"filesize("sc://f"); file_exists("sc://f"); is_dir("sc://f"); is_file("sc://f"); filemtime("sc://f");"#,
    );
    assert_trace(&out, &["url_stat(sc://f,4)"]);
}

/// Verifies a LINK query keeps its own slot: an ordinary answer cannot serve `is_link()`.
#[test]
fn test_a_link_query_does_not_reuse_the_ordinary_answer() {
    let out = run(r#"filesize("sc://f"); is_link("sc://f");"#);
    assert_trace(&out, &["url_stat(sc://f,4)", "url_stat(sc://f,7)"]);
}

/// Verifies an lstat answer DOES serve an ordinary stat — when it found something that is not a
/// symlink.
#[test]
fn test_an_lstat_answer_serves_an_ordinary_stat_for_a_non_link() {
    let out = run(r#"lstat("sc://f"); stat("sc://f");"#);
    assert_trace(&out, &["url_stat(sc://f,5)"]);
}

/// Verifies it does NOT, when what it found is a real symlink.
///
/// The two halves of the same rule: sharing the answer unconditionally would make `filesize()`
/// report the link's own size instead of the target's.
#[test]
fn test_an_lstat_answer_does_not_serve_an_ordinary_stat_for_a_real_link() {
    let out = run(r#"is_link("sc://link"); filesize("sc://link");"#);
    assert_trace(&out, &["url_stat(sc://link,7)", "url_stat(sc://link,4)"]);
}

/// Verifies the cache holds ONE path: a second path evicts the first.
#[test]
fn test_a_second_path_evicts_the_first() {
    let out = run(r#"filesize("sc://a"); filesize("sc://a"); filesize("sc://b"); filesize("sc://a");"#);
    assert_trace(
        &out,
        &["url_stat(sc://a,4)", "url_stat(sc://b,4)", "url_stat(sc://a,4)"],
    );
}

/// Verifies `clearstatcache()` empties it.
#[test]
fn test_clearstatcache_empties_the_slot() {
    let out = run(r#"filesize("sc://f"); clearstatcache(); filesize("sc://f");"#);
    assert_trace(&out, &["url_stat(sc://f,4)", "url_stat(sc://f,4)"]);
}

/// Verifies a TARGETED `clearstatcache()` empties it anyway.
///
/// php's second argument names a path, and one might expect an unrelated entry to survive it.
/// Measured: it does not.
#[test]
fn test_a_targeted_clearstatcache_empties_it_anyway() {
    let out = run(r#"filesize("sc://f"); clearstatcache(true, "sc://other"); filesize("sc://f");"#);
    assert_trace(&out, &["url_stat(sc://f,4)", "url_stat(sc://f,4)"]);
}

/// Verifies a MUTATION does not empty it — the surprising half of the rule.
///
/// `unlink()`, `rename()`, `touch()` and `chmod()` all leave the cached answer standing, so
/// invalidating on them would make elephc ask where php does not.
#[test]
fn test_a_mutation_does_not_empty_the_slot() {
    let out = run(
        r#"filesize("sc://f"); unlink("sc://f"); rename("sc://f", "sc://g"); touch("sc://f"); chmod("sc://f", 0644); filesize("sc://f");"#,
    );
    assert_trace(&out, &["url_stat(sc://f,4)"]);
}

/// Verifies a write through `fopen()` does not empty it either.
#[test]
fn test_a_write_does_not_empty_the_slot() {
    let out = run(
        r#"filesize("sc://f"); $h = fopen("sc://f", "w"); fwrite($h, "x"); fclose($h); filesize("sc://f");"#,
    );
    assert_trace(&out, &["url_stat(sc://f,4)"]);
}

/// Verifies a wrapper that reports the path ABSENT is never cached.
///
/// Caching `false` would turn one missing-file answer into a permanent one, and php asks again
/// every time.
#[test]
fn test_an_absent_path_is_never_cached() {
    let out = run(r#"file_exists("sc://gone"); file_exists("sc://gone"); @filesize("sc://gone");"#);
    assert_trace(
        &out,
        &[
            "url_stat(sc://gone,6)",
            "url_stat(sc://gone,6)",
            "url_stat(sc://gone,4)",
        ],
    );
}

/// Verifies the slot does not leak the answers it evicts.
///
/// The slot holds one reference of its own, so an eviction has to release it. A difference is
/// what this measures, not a clean heap: the cache legitimately still holds the last entry when
/// the program ends, and the baseline carries every allocation the wrapper machinery makes once.
#[test]
fn test_evicting_the_slot_releases_what_it_held() {
    let few = compile_and_run_with_gc_stats(&format!(
        "{WRAPPER}for ($i = 0; $i < 4; $i++) {{ filesize(\"sc://p$i\"); }}\n"
    ));
    let many = compile_and_run_with_gc_stats(&format!(
        "{WRAPPER}for ($i = 0; $i < 40; $i++) {{ filesize(\"sc://p$i\"); }}\n"
    ));
    assert!(few.success, "few failed: {}", few.stderr);
    assert!(many.success, "many failed: {}", many.stderr);
    let (few_allocs, few_frees) = parse_gc_stats(&few.stderr);
    let (many_allocs, many_frees) = parse_gc_stats(&many.stderr);
    assert_eq!(
        many_allocs - few_allocs,
        many_frees - few_frees,
        "36 more evictions must free 36 more times\nfew:\n{}\nmany:\n{}",
        few.stderr,
        many.stderr
    );
}

/// Verifies a stat of a PLAIN filesystem path evicts the wrapper's entry.
///
/// This is the direction that matters: php holds one entry, so `filesize()` on a real file makes
/// the next wrapper query ask again. elephc only ever touched the slot on the wrapper path, so it
/// would have answered from an entry php had already replaced — a stale VALUE, not a saved call.
#[test]
fn test_a_plain_path_stat_evicts_the_wrapper_entry() {
    let out = run(r#"filesize("sc://f"); @filesize(__FILE__); filesize("sc://f");"#);
    assert_trace(&out, &["url_stat(sc://f,4)", "url_stat(sc://f,4)"]);
}

/// Verifies spawning a process evicts it too.
///
/// `shell_exec()`, `exec()`, `system()` and `passthru()` empty php's stat cache — measured, and
/// the only two of the eight invalidation points this file's predecessor claimed that survive
/// measurement (the other is the plain-path stat above). Whatever the command did to the
/// filesystem would otherwise be invisible to the next stat.
#[test]
fn test_spawning_a_process_evicts_the_entry() {
    let out = run(r#"filesize("sc://f"); @shell_exec("true"); filesize("sc://f");"#);
    assert_trace(&out, &["url_stat(sc://f,4)", "url_stat(sc://f,4)"]);
}

/// Verifies `popen()`/`pclose()` does NOT evict — the neighbour that looks the same and is not.
#[test]
fn test_a_popen_pclose_pair_does_not_evict() {
    let out = run(r#"filesize("sc://f"); $h = @popen("true", "r"); @pclose($h); filesize("sc://f");"#);
    assert_trace(&out, &["url_stat(sc://f,4)"]);
}

/// Verifies php's `$context` deprecation is printed for EVERY wrapper instance, not just the one
/// `fopen()` opens.
///
/// php assigns `$context` to each instance it makes, so a class that declares no such property is
/// deprecated once per instantiation — for `url_stat()`, for `unlink()`, for `mkdir()`. elephc
/// printed it for `fopen()` alone, so four of five instantiations were silent.
#[test]
fn test_every_wrapper_instantiation_deprecates_the_invented_context() {
    let out = compile_and_run_capture(
        r#"<?php
class NoCtx {
    public function url_stat($p, $f) { return ["size" => 10, "mode" => 0100644]; }
    public function stream_open($p, $m, $o, &$x) { return true; }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
    public function stream_close() {}
    public function unlink($p) { return true; }
    public function mkdir($p, $m, $o) { return true; }
}
stream_wrapper_register("nc", "NoCtx");
filesize("nc://a");
$h = fopen("nc://c", "r"); fclose($h);
unlink("nc://d");
mkdir("nc://e");
echo "done\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done\n");
    assert_eq!(
        out.diagnostics
            .matches("Creation of dynamic property NoCtx::$context is deprecated")
            .count(),
        4,
        "one per instantiation, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a class that DOES declare `$context` is never deprecated.
#[test]
fn test_a_declared_context_is_not_deprecated() {
    let out = compile_and_run_capture(
        r#"<?php
class WithCtx {
    public $context;
    public function url_stat($p, $f) { return ["size" => 10, "mode" => 0100644]; }
    public function unlink($p) { return true; }
}
stream_wrapper_register("wc", "WithCtx");
filesize("wc://a");
unlink("wc://b");
echo "done\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done\n");
    assert!(
        !out.diagnostics.contains("dynamic property"),
        "a declared property must not be deprecated, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a wrapper stat that answers FALSE does not empty the slot either.
///
/// Emptying was tied to "another path was asked about"; it belongs to "another stat took the
/// slot". A path the wrapper says is absent puts nothing there, so it takes nothing away.
#[test]
fn test_an_absent_wrapper_path_does_not_evict_the_slot() {
    let out = run(r#"filesize("sc://a"); file_exists("sc://gone"); filesize("sc://a");"#);
    assert_trace(&out, &["url_stat(sc://a,4)", "url_stat(sc://gone,6)"]);
}

/// Verifies `file_exists()` on a PLAIN path leaves the wrapper's entry alone.
///
/// It answers from `access(2)` and fills no cache, so it empties none — MEASURED, php asks the
/// wrapper once across the pair where elephc asked twice.
#[test]
fn test_a_query_that_fills_nothing_evicts_nothing() {
    let out = run(r#"filesize("sc://a"); @file_exists(__FILE__); filesize("sc://a");"#);
    assert_trace(&out, &["url_stat(sc://a,4)"]);
}

/// Verifies `is_readable()` on a plain path is the same, and `is_file()` is NOT.
///
/// The two halves of the rule, in one test: `is_file()` fills php's slot and therefore replaces
/// what was in it, where the access predicates never touch it.
#[test]
fn test_is_readable_leaves_it_and_is_file_takes_it() {
    let gentle = run(r#"filesize("sc://a"); @is_readable(__FILE__); filesize("sc://a");"#);
    assert_trace(&gentle, &["url_stat(sc://a,4)"]);

    let takes = run(r#"filesize("sc://b"); @is_file(__FILE__); filesize("sc://b");"#);
    assert_trace(&takes, &["url_stat(sc://b,4)", "url_stat(sc://b,4)"]);
}
