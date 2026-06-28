//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions string memory, including string replace in foreach assoc function, concat loop 1000, and concat assignment loop 5000.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies str_replace works correctly inside a foreach associative loop.
/// Fixture: a map of "hello"→"world", "foo"→"bar" applied to "hello foo".
#[test]
fn test_str_replace_in_foreach_assoc_function() {
    let out = compile_and_run(
        r#"<?php
function transform($map, $text) {
    foreach ($map as $key => $value) {
        $text = str_replace($key, $value, $text);
    }
    return $text;
}
$map = ["hello" => "world", "foo" => "bar"];
echo transform($map, "hello foo");
"#,
    );
    assert_eq!(out, "world bar");
}

// --- Bug fix: fmod sign (frintm → frintz) ---

/// Regression test for issue #21: concat buffer overflow after ~362 iterations.
/// Fixture: loop of 1000 single-character `.=` concatenations.
#[test]
fn test_concat_loop_1000() {
    // Regression test for issue #21: concat buffer overflow after ~362 iterations
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 1000; $i++) {
    $s .= "x";
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "1000");
}

/// Regression for x86_64 local-string cleanup: `$s = $s . "x"` must release old heap strings.
/// Fixture: loop of 5000 explicit concat assignments.
#[test]
fn test_concat_assignment_loop_5000() {
    // Regression for x86_64 local-string cleanup: `$s = $s . "x"` must release old heap strings.
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 5000; $i++) {
    $s = $s . "x";
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "5000");
}

/// Verifies strtolower returns correct string when called repeatedly in a loop.
/// Fixture: 500 iterations of strtolower("HELLO WORLD").
#[test]
fn test_string_function_in_loop() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 500; $i++) {
    $x = strtolower("HELLO WORLD");
}
echo $x;
"#,
    );
    assert_eq!(out, "hello world");
}

/// Verifies old string values are freed on reassignment (free-list reuse).
/// Fixture: 2000 iterations reassigning `$s = str_repeat("a", 100)`, then echo final strlen.
#[test]
fn test_string_reassignment_loop() {
    // Tests that old string values are freed on reassignment (free-list reuse)
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 2000; $i++) {
    $s = str_repeat("a", 100);
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "100");
}

/// Verifies string values persist correctly across statement boundaries.
/// Fixture: two concatenated pairs then combined, result should be "foobarbazqux".
#[test]
fn test_string_variables_survive_statements() {
    // Tests that string persist works: values survive across statement boundaries
    let out = compile_and_run(
        r#"<?php
$a = "foo" . "bar";
$b = "baz" . "qux";
echo $a . $b;
"#,
    );
    assert_eq!(out, "foobarbazqux");
}

/// Regression for per-statement concat temporaries: each nested concat only needs
/// a persisted left operand until the next concat has copied it.
/// Fixture: 1200 iterations of 7-way chained concat with heap size 65_536.
#[test]
fn test_echo_concat_chain_releases_intermediate_strings() {
    // Regression for per-statement concat temporaries: each nested concat only needs
    // a persisted left operand until the next concat has copied it.
    let out = compile_and_run_with_heap_size(
        r#"<?php
for ($i = 0; $i < 1200; $i++) {
    echo "a" . $i . "b" . $i . "c" . $i . "\n";
}
echo "done";
"#,
        65_536,
    );
    assert!(out.ends_with("done"));
}

/// Regression for locals first assigned inside control flow: the local slot is
/// zero-initialized, so final cleanup is safe even when the loop does not run.
/// Fixture: 1000 calls to receive_once() whose while(true) loop breaks after one iteration.
#[test]
fn test_string_local_assigned_in_loop_is_released_on_function_exit() {
    // Regression for locals first assigned inside control flow: the local slot is
    // zero-initialized, so final cleanup is safe even when the loop does not run.
    let out = compile_and_run_with_heap_size(
        r#"<?php
function receive_once(): void {
    while (true) {
        $chunk = str_repeat("x", 96);
        break;
    }
}
for ($i = 0; $i < 1000; $i++) {
    receive_once();
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Mirrors the HTTP server's pooled Connection::inbuf slot: each request
/// resets the property and then rebuilds it from an incoming chunk.
/// Fixture: 1000 iterations of reset() then read_once() with 128-byte chunk.
#[test]
fn test_reused_object_string_property_concat_is_released_on_reset() {
    // Mirrors the HTTP server's pooled Connection::inbuf slot: each request
    // resets the property and then rebuilds it from an incoming chunk.
    let out = compile_and_run_with_heap_size(
        r#"<?php
class Conn {
    public string $inbuf = "";

    public function reset(): void {
        $this->inbuf = "";
    }

    public function read_once(): void {
        $chunk = str_repeat("x", 128);
        $this->inbuf = $this->inbuf . $chunk;
    }
}

$conn = new Conn();
for ($i = 0; $i < 1000; $i++) {
    $conn->reset();
    $conn->read_once();
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Mirrors the HTTP reactor's per-iteration poll map: the array grows from
/// its small initial capacity, then the next assignment must release it.
/// Fixture: 1000 iterations building a 64-element indexed poll_map array.
#[test]
fn test_indexed_array_rebuilt_after_growth_does_not_leak() {
    // Mirrors the HTTP reactor's per-iteration poll map: the array grows from
    // its small initial capacity, then the next assignment must release it.
    let out = compile_and_run_with_heap_size(
        r#"<?php
for ($n = 0; $n < 1000; $n++) {
    $poll_map = [];
    $i = 0;
    while ($i < 64) {
        $poll_map[] = $i;
        $i++;
    }
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Mirrors the real HTTP parser, which splits the header block and request
/// line on every request before reading a few array elements.
/// Fixture: 1000 calls to parse_once() splitting "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n".
#[test]
fn test_explode_arrays_and_elements_release_on_function_exit() {
    // Mirrors the real HTTP parser, which splits the header block and request
    // line on every request before reading a few array elements.
    let out = compile_and_run_with_heap_size(
        r#"<?php
function parse_once(string $raw): void {
    $lines = explode("\r\n", $raw);
    $parts = explode(" ", $lines[0]);
    $method = $parts[0];
    $path = $parts[1];
}

for ($i = 0; $i < 1000; $i++) {
    parse_once("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Verifies explode-based HTTP parser leaves no live heap blocks after 3 iterations.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_explode_parser_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function parse_once(string $raw): void {
    $lines = explode("\r\n", $raw);
    $parts = explode(" ", $lines[0]);
    $method = $parts[0];
    $path = $parts[1];
}

for ($i = 0; $i < 3; $i++) {
    parse_once("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Regression: `explode()` on a string with an empty segment (leading/trailing/double
/// delimiter) used to store that segment as a borrowed pointer INTO the subject string,
/// because `str_persist` returned zero-length strings as-is rather than copying them to
/// an owned block. Reading such an empty element into a local and then releasing it
/// (loop reassignment / scope exit) freed into the subject's live heap block, double-
/// freeing it and corrupting the heap under churn — a deterministic SIGSEGV here without
/// the fix. `str_persist` now gives empty strings their own owned block, and the release
/// paths free owned empties through the validating `__rt_heap_free_safe` helper.
/// Fixture: explodes an absolute path (leading empty segment) and reads it 200 times.
#[test]
fn test_explode_empty_segment_not_borrowed_from_subject() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
function run(): string {
    $acc = 0;
    for ($i = 0; $i < 200; $i++) {
        $s = "/seg/" . $i . "/end";
        $parts = explode("/", $s);   // ["", "seg", "$i", "end"] -- leading empty segment
        $first = $parts[0];          // "" -- must be owned, not a pointer into $s
        $acc = $acc + strlen($first) + count($parts);
    }
    return "ok:" . $acc;
}
echo run();
"#,
        65_536,
    );
    assert_eq!(out, "ok:800");
}

/// Verifies the empty owned blocks `str_persist` now allocates for zero-length strings
/// are released — exploding an absolute path (leading empty segment) and reading every
/// element leaves a clean heap (allocs == frees). On v0.24.2 the reassign and scope-exit
/// release paths skipped len-0 strings and leaked one owned empty block per call.
#[test]
fn test_explode_empty_segment_leaves_clean_heap() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function parse(string $s): int {
    $parts = explode("/", $s);       // leading empty segment from the leading "/"
    $a = $parts[0];                  // ""
    $b = $parts[1];
    return strlen($a) + strlen($b);
}
$t = 0;
for ($i = 0; $i < 5; $i++) {
    $t = $t + parse("/alpha/beta/gamma");
}
echo $t;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "25");
}

/// Regression for the v0.24.2 release-on-reassign leak of owned empty strings.
/// Reassigning an empty string local in a loop must free the previous owned block:
/// `__rt_str_persist` now allocates one for `""`, and both the reassign release path
/// and the scope-exit cleanup must free it through `__rt_heap_free_safe` (which skips
/// null/.rodata/out-of-range pointers). The heap must stay clean.
#[test]
fn test_empty_string_reassignment_loop_leaves_clean_heap() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function run(): void {
    $s = "";
    for ($i = 0; $i < 50; $i++) {
        $s = "";
    }
    echo strlen($s);
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "0");
}

/// Verifies echo of property concat log line leaves no live heap blocks after 3 iterations.
/// Fixture: Req/Res objects, echo "  " . method . " " . path . " -> " . status . "\n".
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_echo_property_concat_log_line_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Req {
    public string $method = "GET";
    public string $path = "/";
}

class Res {
    public int $status = 200;
}

function log_once(): void {
    $req = new Req();
    $res = new Res();
    echo "  " . $req->method . " " . $req->path . " -> " . $res->status . "\n";
}

for ($i = 0; $i < 3; $i++) {
    log_once();
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "  GET / -> 200\n  GET / -> 200\n  GET / -> 200\n");
}

/// Verifies full HTTP request parse/route/render cycle leaves no live heap blocks after 3 iterations.
/// Fixture: str_index, Request object, parse_request splitting "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n".
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_http_parse_request_object_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function str_index(string $haystack, string $needle): int {
    $pos = strpos($haystack, $needle);
    if ($pos === false) {
        return -1;
    }
    return intval($pos);
}

class Request {
    public string $method = "";
    public string $path = "";
    public string $query = "";
    public string $version = "";
    public string $body = "";
    public string $head = "";
}

function split_head_body(Request $req, string $raw): void {
    $split = str_index($raw, "\r\n\r\n");
    if ($split >= 0) {
        $body_at = intval($split + 4);
        $req->head = substr($raw, 0, $split);
        $req->body = substr($raw, $body_at);
    } else {
        $req->head = $raw;
    }
}

function parse_target(Request $req, string $target): void {
    $qpos = str_index($target, "?");
    if ($qpos >= 0) {
        $after = intval($qpos + 1);
        $req->path = substr($target, 0, $qpos);
        $req->query = substr($target, $after);
    } else {
        $req->path = $target;
    }
}

function parse_request_line(Request $req, string $line): void {
    $parts = explode(" ", $line);
    if (count($parts) >= 3) {
        $req->method = $parts[0];
        $req->version = $parts[2];
        parse_target($req, $parts[1]);
    }
}

function parse_request(string $raw): Request {
    $req = new Request();
    split_head_body($req, $raw);
    $lines = explode("\r\n", $req->head);
    parse_request_line($req, $lines[0]);
    return $req;
}

function serve_once(): void {
    $req = parse_request("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies strpos mixed-result local (int|false) leaves no live heap blocks after 3 iterations.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_strpos_mixed_result_local_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function str_index(string $haystack, string $needle): int {
    $pos = strpos($haystack, $needle);
    if ($pos === false) {
        return -1;
    }
    return intval($pos);
}

for ($i = 0; $i < 3; $i++) {
    $x = str_index("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n", "\r\n\r\n");
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies strpos strict compare with === false leaves no live heap blocks after 1000 iterations.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_direct_strpos_strict_compare_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
for ($i = 0; $i < 1000; $i++) {
    if (strpos("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n", "\r\n\r\n") === false) {
        echo "bad";
    }
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies split_head_body property substr assignment leaves no live heap blocks after 3 iterations.
/// Fixture: Request object with substr($raw, 0, $split) and substr($raw, $body_at) writes.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_split_head_body_property_substr_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function str_index(string $haystack, string $needle): int {
    $pos = strpos($haystack, $needle);
    if ($pos === false) {
        return -1;
    }
    return intval($pos);
}

class Request {
    public string $body = "";
    public string $head = "";
}

function split_head_body(Request $req, string $raw): void {
    $split = str_index($raw, "\r\n\r\n");
    if ($split >= 0) {
        $body_at = intval($split + 4);
        $req->head = substr($raw, 0, $split);
        $req->body = substr($raw, $body_at);
    } else {
        $req->head = $raw;
    }
}

function serve_once(): void {
    $req = new Request();
    split_head_body($req, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies object argument call leaves no live heap blocks after 3 iterations.
/// Fixture: Request object passed by reference, mark_seen() increments $req->seen.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_object_argument_call_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Request {
    public int $seen = 0;
}

function mark_seen(Request $req): void {
    $req->seen = $req->seen + 1;
}

function serve_once(): void {
    $req = new Request();
    mark_seen($req);
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies substr property assignment leaves no live heap blocks after 3 iterations.
/// Fixture: Request object, serve_once() assigns substr($raw, 0, 31) and substr($raw, 35).
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_substr_property_assignment_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Request {
    public string $body = "";
    public string $head = "";
}

function serve_once(): void {
    $req = new Request();
    $raw = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    $req->head = substr($raw, 0, 31);
    $req->body = substr($raw, 35);
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies string argument call leaves no live heap blocks after 3 iterations.
/// Fixture: use_string() takes string by value, computes strlen().
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_string_argument_call_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function use_string(string $raw): void {
    $n = strlen($raw);
}

for ($i = 0; $i < 3; $i++) {
    use_string("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies substr of string argument property assignment leaves no live heap blocks after 3 iterations.
/// Fixture: Request object, write_head() assigns substr($raw, 0, 31).
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_substr_of_string_argument_property_assignment_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Request {
    public string $head = "";
}

function write_head(Request $req, string $raw): void {
    $req->head = substr($raw, 0, 31);
}

function serve_once(): void {
    $req = new Request();
    write_head($req, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies empty-tail substr property assignment leaves no live heap blocks after 3 iterations.
/// Fixture: Request object, write_body() assigns substr($raw, 35) (empty tail after "GET ").
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_empty_tail_substr_property_assignment_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Request {
    public string $body = "";
}

function write_body(Request $req, string $raw): void {
    $req->body = substr($raw, 35);
}

function serve_once(): void {
    $req = new Request();
    write_body($req, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies two substr property assignments from string argument leave no live heap blocks after 3 iterations.
/// Fixture: Request object, write_parts() assigns substr($raw, 0, 31) and substr($raw, 35).
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_two_substr_property_assignments_from_string_argument_leave_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Request {
    public string $body = "";
    public string $head = "";
}

function write_parts(Request $req, string $raw): void {
    $req->head = substr($raw, 0, 31);
    $req->body = substr($raw, 35);
}

function serve_once(): void {
    $req = new Request();
    write_parts($req, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies substr property assignments with computed offsets leave no live heap blocks after 3 iterations.
/// Fixture: Request object, write_parts() with intval($split + 4) computed body offset.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_substr_property_assignments_with_computed_offsets_leave_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Request {
    public string $body = "";
    public string $head = "";
}

function write_parts(Request $req, string $raw): void {
    $split = 31;
    $body_at = intval($split + 4);
    $req->head = substr($raw, 0, $split);
    $req->body = substr($raw, $body_at);
}

function serve_once(): void {
    $req = new Request();
    write_parts($req, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies substr property assignments with str_index offset leave no live heap blocks after 3 iterations.
/// Fixture: Request object, write_parts() uses str_index($raw, "\r\n\r\n") to compute split point.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_substr_property_assignments_with_str_index_offset_leave_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function str_index(string $haystack, string $needle): int {
    $pos = strpos($haystack, $needle);
    if ($pos === false) {
        return -1;
    }
    return intval($pos);
}

class Request {
    public string $body = "";
    public string $head = "";
}

function write_parts(Request $req, string $raw): void {
    $split = str_index($raw, "\r\n\r\n");
    $body_at = intval($split + 4);
    $req->head = substr($raw, 0, $split);
    $req->body = substr($raw, $body_at);
}

function serve_once(): void {
    $req = new Request();
    write_parts($req, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies user function int return assigned inside function leaves no live heap blocks after 3 iterations.
/// Fixture: outer() calls str_index() and discards the result.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_user_function_int_return_assigned_inside_function_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function str_index(string $haystack, string $needle): int {
    $pos = strpos($haystack, $needle);
    if ($pos === false) {
        return -1;
    }
    return intval($pos);
}

function outer(): void {
    $split = str_index("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n", "\r\n\r\n");
}

for ($i = 0; $i < 3; $i++) {
    outer();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Exercises the showcase's parse/route/render shape without sockets or Fibers so reactor
/// leaks can be separated from response-path leaks.
/// Fixture: 1000 iterations of serve_once() with Request/Response objects, render() and parse_request().
#[test]
fn test_http_response_path_releases_per_request_objects_and_strings() {
    // Exercises the showcase's parse/route/render shape without sockets or
    // Fibers so reactor leaks can be separated from response-path leaks.
    let out = compile_and_run_with_heap_size(
        r#"<?php
class Request {
    public string $method = "";
    public string $path = "";
    public string $body = "";
    public string $head = "";
}

class Response {
    public int $status = 200;
    public string $ctype = "text/plain; charset=utf-8";
    public string $body = "";

    public function html(string $s): void {
        $this->ctype = "text/html; charset=utf-8";
        $this->body = $s;
    }

    public function render(): string {
        $out = "HTTP/1.1 " . $this->status . " OK\r\n";
        $out = $out . "Content-Type: " . $this->ctype . "\r\n";
        $out = $out . "Content-Length: " . strlen($this->body) . "\r\n";
        $out = $out . "Connection: close\r\n";
        $out = $out . "Server: elephc-http\r\n";
        $out = $out . "\r\n";
        return $out . $this->body;
    }
}

function parse_request(string $raw): Request {
    $req = new Request();
    $split = strpos($raw, "\r\n\r\n");
    if ($split === false) {
        $req->head = $raw;
    } else {
        $req->head = substr($raw, 0, intval($split));
        $req->body = substr($raw, intval($split) + 4);
    }
    $req->method = "GET";
    $req->path = "/";
    return $req;
}

function route_index(): Response {
    $res = new Response();
    $res->html(
        "<!doctype html>\n"
        . "<html><head><title>elephc http-server</title></head><body>\n"
        . "<h1>elephc http-server</h1>\n"
        . "<p>A native HTTP/1.1 server written in PHP and compiled to native code.</p>\n"
        . "<ul><li>/hello</li><li>/json</li><li>/stats</li></ul>\n"
        . "</body></html>\n"
    );
    return $res;
}

function handle_request(Request $req): Response {
    static $served = 0;
    $served = $served + 1;
    return route_index();
}

function serve_once(): void {
    $req = parse_request("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    $res = handle_request($req);
    $payload = $res->render();
}

for ($i = 0; $i < 1000; $i++) {
    serve_once();
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Verifies full HTTP response path leaves no live heap blocks after 3 iterations.
/// Fixture: Request/Response objects, parse_request, handle_request, route_index, render().
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_http_response_path_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function str_index(string $haystack, string $needle): int {
    $pos = strpos($haystack, $needle);
    if ($pos === false) {
        return -1;
    }
    return intval($pos);
}

class Request {
    public string $method = "";
    public string $path = "";
    public string $query = "";
    public string $version = "";
    public string $body = "";
    public string $head = "";
}

class Response {
    public int $status = 200;
    public string $ctype = "text/plain; charset=utf-8";
    public string $body = "";

    public function html(string $s): void {
        $this->ctype = "text/html; charset=utf-8";
        $this->body = $s;
    }

    public function render(): string {
        $out = "HTTP/1.1 " . $this->status . " OK\r\n";
        $out = $out . "Content-Type: " . $this->ctype . "\r\n";
        $out = $out . "Content-Length: " . strlen($this->body) . "\r\n";
        $out = $out . "Connection: close\r\n";
        $out = $out . "Server: elephc-http\r\n";
        $out = $out . "\r\n";
        return $out . $this->body;
    }
}

function split_head_body(Request $req, string $raw): void {
    $split = str_index($raw, "\r\n\r\n");
    if ($split >= 0) {
        $body_at = intval($split + 4);
        $req->head = substr($raw, 0, $split);
        $req->body = substr($raw, $body_at);
    } else {
        $req->head = $raw;
    }
}

function parse_target(Request $req, string $target): void {
    $qpos = str_index($target, "?");
    if ($qpos >= 0) {
        $after = intval($qpos + 1);
        $req->path = substr($target, 0, $qpos);
        $req->query = substr($target, $after);
    } else {
        $req->path = $target;
    }
}

function parse_request_line(Request $req, string $line): void {
    $parts = explode(" ", $line);
    if (count($parts) >= 3) {
        $req->method = $parts[0];
        $req->version = $parts[2];
        parse_target($req, $parts[1]);
    }
}

function parse_request(string $raw): Request {
    $req = new Request();
    split_head_body($req, $raw);
    $lines = explode("\r\n", $req->head);
    parse_request_line($req, $lines[0]);
    return $req;
}

function route_index(): Response {
    $res = new Response();
    $res->html("<!doctype html>\n<html><body><h1>elephc</h1></body></html>\n");
    return $res;
}

function handle_request(Request $req): Response {
    static $served = 0;
    $served = $served + 1;
    return route_index();
}

function serve_once(): void {
    $req = parse_request("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    $res = handle_request($req);
    $payload = $res->render();
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies render loop releases per-request Response objects.
/// Fixture: 1000 iterations of route_index() → render() with str_repeat 256-byte body.
#[test]
fn test_response_render_loop_releases_per_request_objects() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
class Response {
    public int $status = 200;
    public string $ctype = "text/plain; charset=utf-8";
    public string $body = "";

    public function html(string $s): void {
        $this->ctype = "text/html; charset=utf-8";
        $this->body = $s;
    }

    public function render(): string {
        $out = "HTTP/1.1 " . $this->status . " OK\r\n";
        $out = $out . "Content-Type: " . $this->ctype . "\r\n";
        $out = $out . "Content-Length: " . strlen($this->body) . "\r\n";
        $out = $out . "Connection: close\r\n";
        $out = $out . "Server: elephc-http\r\n";
        $out = $out . "\r\n";
        return $out . $this->body;
    }
}

function route_index(): Response {
    $res = new Response();
    $res->html(
        "<!doctype html>\n"
        . "<html><head><title>elephc http-server</title></head><body>\n"
        . "<h1>elephc http-server</h1>\n"
        . "<p>A native HTTP/1.1 server written in PHP and compiled to native code.</p>\n"
        . "<ul><li>/hello</li><li>/json</li><li>/stats</li></ul>\n"
        . "</body></html>\n"
    );
    return $res;
}

function serve_once(): void {
    $res = route_index();
    $payload = $res->render();
}

for ($i = 0; $i < 1000; $i++) {
    serve_once();
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Verifies response render loop leaves no live heap blocks after 3 iterations.
/// Fixture: Response object with str_repeat("x", 256) body, render().
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_response_render_loop_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Response {
    public int $status = 200;
    public string $ctype = "text/plain; charset=utf-8";
    public string $body = "";

    public function html(string $s): void {
        $this->ctype = "text/html; charset=utf-8";
        $this->body = $s;
    }

    public function render(): string {
        $out = "HTTP/1.1 " . $this->status . " OK\r\n";
        $out = $out . "Content-Type: " . $this->ctype . "\r\n";
        $out = $out . "Content-Length: " . strlen($this->body) . "\r\n";
        $out = $out . "Connection: close\r\n";
        $out = $out . "Server: elephc-http\r\n";
        $out = $out . "\r\n";
        return $out . $this->body;
    }
}

function route_index(): Response {
    $res = new Response();
    $res->html(str_repeat("x", 256));
    return $res;
}

function serve_once(): void {
    $res = route_index();
    $payload = $res->render();
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies object string properties are released on function exit.
/// Fixture: 1000 iterations of serve_once() creating Response with html(str_repeat("x", 256)).
#[test]
fn test_object_string_properties_release_on_function_exit() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
class Response {
    public string $ctype = "text/plain; charset=utf-8";
    public string $body = "";

    public function html(string $s): void {
        $this->ctype = "text/html; charset=utf-8";
        $this->body = $s;
    }
}

function serve_once(): void {
    $res = new Response();
    $res->html(str_repeat("x", 256));
}

for ($i = 0; $i < 1000; $i++) {
    serve_once();
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Verifies returned object with string property is released on caller exit.
/// Fixture: 1000 iterations of serve_once() → make_response() returning Response with 256-byte body.
#[test]
fn test_returned_object_with_string_property_releases_on_caller_exit() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
class Response {
    public string $body = "";
}

function make_response(): Response {
    $res = new Response();
    $res->body = str_repeat("x", 256);
    return $res;
}

function serve_once(): void {
    $res = make_response();
}

for ($i = 0; $i < 1000; $i++) {
    serve_once();
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Verifies returned object with string property leaves no live heap blocks after 3 iterations.
/// Uses compile_and_run_with_gc_stats to assert allocs == frees.
#[test]
fn test_returned_object_with_string_property_leaves_no_live_heap_blocks() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Response {
    public string $body = "";
}

function make_response(): Response {
    $res = new Response();
    $res->body = str_repeat("x", 256);
    return $res;
}

function serve_once(): void {
    $res = make_response();
}

for ($i = 0; $i < 3; $i++) {
    serve_once();
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
    assert_eq!(out.stdout, "done");
}

/// Verifies response render loop with local object releases strings.
/// Fixture: 1000 iterations of serve_once() creating Response with html(str_repeat("x", 256)) then render().
#[test]
fn test_response_render_loop_with_local_object_releases_strings() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
class Response {
    public int $status = 200;
    public string $ctype = "text/plain; charset=utf-8";
    public string $body = "";

    public function html(string $s): void {
        $this->ctype = "text/html; charset=utf-8";
        $this->body = $s;
    }

    public function render(): string {
        $out = "HTTP/1.1 " . $this->status . " OK\r\n";
        $out = $out . "Content-Type: " . $this->ctype . "\r\n";
        $out = $out . "Content-Length: " . strlen($this->body) . "\r\n";
        $out = $out . "Connection: close\r\n";
        $out = $out . "Server: elephc-http\r\n";
        $out = $out . "\r\n";
        return $out . $this->body;
    }
}

function serve_once(): void {
    $res = new Response();
    $res->html(str_repeat("x", 256));
    $payload = $res->render();
}

for ($i = 0; $i < 1000; $i++) {
    serve_once();
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Verifies repeated string builder return releases intermediate strings.
/// Fixture: 1000 iterations of serve_once() calling render_payload(str_repeat("x", 256)).
#[test]
fn test_repeated_string_builder_return_releases_intermediates() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
function render_payload(string $body): string {
    $out = "HTTP/1.1 200 OK\r\n";
    $out = $out . "Content-Type: text/html\r\n";
    $out = $out . "Content-Length: " . strlen($body) . "\r\n";
    $out = $out . "Connection: close\r\n";
    $out = $out . "Server: elephc-http\r\n";
    $out = $out . "\r\n";
    return $out . $body;
}

function serve_once(): void {
    $body = str_repeat("x", 256);
    $payload = render_payload($body);
}

for ($i = 0; $i < 1000; $i++) {
    serve_once();
}
echo "done";
"#,
        65_536,
    );
    assert_eq!(out, "done");
}

/// Verifies unset frees the string and is_null returns true for unset variable.
/// Fixture: assign concat, strlen, unset, then is_null check.
#[test]
fn test_unset_frees_string() {
    let out = compile_and_run(
        r#"<?php
$x = "hello" . " world";
echo strlen($x);
unset($x);
echo is_null($x) ? "1" : "0";
"#,
    );
    assert_eq!(out, "111");
}

/// Ensure multiple string variables don't interfere after concat_buf reset.
/// Fixture: $a="hello", $b="world", $c=$a." ".$b, $d=strtoupper($a), echo $c."|".$d.
#[test]
fn test_multiple_string_vars_independent() {
    // Ensure multiple string variables don't interfere after concat_buf reset
    let out = compile_and_run(
        r#"<?php
$a = "hello";
$b = "world";
$c = $a . " " . $b;
$d = strtoupper($a);
echo $c . "|" . $d;
"#,
    );
    assert_eq!(out, "hello world|HELLO");
}

/// Verifies str_replace returns correct result when called repeatedly in a loop.
/// Fixture: 100 iterations of str_replace("x", "y", "xox"), expects "yoy".
#[test]
fn test_str_replace_in_loop() {
    let out = compile_and_run(
        r#"<?php
$result = "";
for ($i = 0; $i < 100; $i++) {
    $result = str_replace("x", "y", "xox");
}
echo $result;
"#,
    );
    assert_eq!(out, "yoy");
}
