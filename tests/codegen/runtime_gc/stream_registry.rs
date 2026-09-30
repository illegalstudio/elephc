//! Purpose:
//! Regression tests for opaque stream-resource identity, lifecycle, and dynamic registry capacity.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_stream_registry_` through Rust's test harness.
//!
//! Key details:
//! - PHP 8.5.6 oracle outputs pin stale-handle, double-close, descriptor-reuse, stream-capacity, and context-capacity behavior.
//! - Container aliases deliberately ensure close state belongs to the resource entry rather than one local `Mixed` box.

use crate::support::*;

/// Verifies that closing a stream invalidates a resource alias stored in an array.
#[test]
fn test_stream_registry_stale_container_alias_is_closed() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("php://temp", "w+");
$aliases = [$stream];
fclose($stream);

var_dump(is_resource($aliases[0]));
var_dump(get_resource_type($aliases[0]));
try {
    fread($aliases[0], 1);
} catch (Throwable $error) {
    echo get_class($error), ": ", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nstring(7) \"Unknown\"\nTypeError: fread(): Argument #1 ($stream) must be an open stream resource\n"
    );
}

/// Verifies that a second close through a container alias is rejected as a closed resource.
#[test]
fn test_stream_registry_double_close_through_container_alias_is_rejected() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("php://temp", "w+");
$aliases = [$stream];
var_dump(fclose($stream));

try {
    fclose($aliases[0]);
} catch (Throwable $error) {
    echo get_class($error), ": ", $error->getMessage(), "\n";
}
var_dump(is_resource($aliases[0]));
var_dump(get_resource_type($aliases[0]));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nTypeError: fclose(): Argument #1 ($stream) must be an open stream resource\nbool(false)\nstring(7) \"Unknown\"\n"
    );
}

/// Verifies that descriptor reuse cannot revive a stale alias or corrupt the fresh stream.
#[test]
fn test_stream_registry_descriptor_reuse_does_not_revive_stale_alias() {
    let out = compile_and_run(
        r#"<?php
$old = fopen("php://temp", "w+");
$aliases = [$old];
$oldId = get_resource_id($old);
fclose($old);

$fresh = fopen("php://temp", "w+");
$freshId = get_resource_id($fresh);
fwrite($fresh, "fresh");

try {
    fwrite($aliases[0], "STALE");
    echo "accepted\n";
} catch (Throwable $error) {
    echo get_class($error), ": ", $error->getMessage(), "\n";
}

rewind($fresh);
echo stream_get_contents($fresh), "\n";
echo $oldId === $freshId ? "same-id\n" : "distinct-id\n";
"#,
    );
    assert_eq!(
        out,
        "TypeError: fwrite(): Argument #1 ($stream) must be an open stream resource\nfresh\ndistinct-id\n"
    );
}

/// Verifies that more than 256 non-FD memory streams can remain live with unique PHP ids.
#[test]
fn test_stream_registry_supports_more_than_256_live_memory_streams() {
    let out = compile_and_run(
        r#"<?php
$streams = [];
$ids = [];

for ($i = 0; $i < 300; $i++) {
    $stream = fopen("php://memory", "w+");
    if ($stream === false) {
        echo "open-fail:", $i, "\n";
        break;
    }
    fwrite($stream, (string) $i);
    $streams[] = $stream;
    $ids[] = get_resource_id($stream);
}

echo count($streams), " ", count(array_unique($ids)), " ";
if (count($streams) === 300) {
    echo stream_set_chunk_size($streams[299], 4096), " ";
    echo stream_set_chunk_size($streams[299], 2048), " ";
    rewind($streams[299]);
    echo stream_get_contents($streams[299]);
}
echo "\n";

foreach ($streams as $stream) {
    fclose($stream);
}
"#,
    );
    assert_eq!(out, "300 300 8192 4096 299\n");
}

/// Verifies descriptor reuse starts with fresh StreamState-owned metadata.
#[test]
fn test_stream_registry_descriptor_reuse_resets_chunk_size() {
    let out = compile_and_run(
        r#"<?php
$old = fopen("php://temp", "w+");
echo stream_set_chunk_size($old, 1234), " ";
fclose($old);

$fresh = fopen("php://temp", "w+");
echo stream_set_chunk_size($fresh, 4321), " ";
echo stream_set_chunk_size($fresh, 2048), "\n";
"#,
    );
    assert_eq!(out, "8192 8192 4321\n");
}

/// Verifies that more than 16 live contexts retain distinct ids and independent options.
#[test]
fn test_stream_registry_supports_more_than_16_live_contexts() {
    let out = compile_and_run(
        r#"<?php
$contexts = [];
$ids = [];

for ($i = 0; $i < 40; $i++) {
    $context = stream_context_create([
        "http" => ["header" => "N" . $i],
    ]);
    $contexts[] = $context;
    $ids[] = get_resource_id($context);
}

echo count($contexts), " ", count(array_unique($ids)), "\n";
foreach ([0, 15, 16, 31, 39] as $index) {
    $options = stream_context_get_options($contexts[$index]);
    echo $options["http"]["header"], "\n";
}
"#,
    );
    assert_eq!(out, "40 40\nN0\nN15\nN16\nN31\nN39\n");
}

/// Verifies resource-array overwrite and COW keep the old and new stream owners independent.
#[test]
fn test_stream_registry_resource_array_overwrite_preserves_cow_owners() {
    let out = compile_and_run(
        r#"<?php
$old = fopen("php://temp", "w+");
$new = fopen("php://temp", "w+");
$streams = [$old];
$copy = $streams;
$streams[0] = $new;

fwrite($copy[0], "old");
fwrite($streams[0], "new");
rewind($copy[0]);
rewind($streams[0]);
echo stream_get_contents($copy[0]), "|", stream_get_contents($streams[0]), "\n";

$streams[0] = $streams[0];
fwrite($streams[0], "!");
rewind($streams[0]);
echo stream_get_contents($streams[0]), "\n";
"#,
    );
    assert_eq!(out, "old|new\nnew!\n");
}

/// Verifies stream contexts expose PHP's distinct live resource type label.
#[test]
fn test_stream_registry_context_reports_stream_context_type() {
    let out = compile_and_run(
        r#"<?php
$context = stream_context_create();
$default = stream_context_get_default();
echo get_resource_type($context), "|", get_resource_type($default), "\n";
"#,
    );
    assert_eq!(out, "stream-context|stream-context\n");
}

/// Verifies EOF state from a closed descriptor does not contaminate a fresh stream.
#[test]
fn test_stream_registry_eof_state_isolated_after_descriptor_reuse() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("first.txt", "x");
file_put_contents("second.txt", "yz");

$first = fopen("first.txt", "r");
var_dump(fread($first, 1));
var_dump(feof($first));
var_dump(fread($first, 1));
var_dump(feof($first));
fclose($first);

$second = fopen("second.txt", "r");
var_dump(feof($second));
var_dump(fread($second, 2));
var_dump(feof($second));
var_dump(fread($second, 1));
var_dump(feof($second));
fclose($second);

unlink("first.txt");
unlink("second.txt");
"#,
    );
    assert_eq!(
        out,
        "string(1) \"x\"\nbool(false)\nstring(0) \"\"\nbool(true)\nbool(false)\nstring(2) \"yz\"\nbool(false)\nstring(0) \"\"\nbool(true)\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies PHP marks EOF only after a read reaches past an exact stream boundary.
#[test]
fn test_stream_registry_feof_after_complete_read_matches_php() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("php://memory", "w+");
fwrite($stream, "abc");
rewind($stream);

echo fread($stream, 3), "|";
var_dump(feof($stream));
var_dump(fread($stream, 1));
var_dump(feof($stream));
fclose($stream);
"#,
    );
    assert_eq!(
        out,
        "abc|bool(false)\nstring(0) \"\"\nbool(true)\n"
    );
}

/// Verifies a short read that consumes all remaining bytes sets EOF immediately.
#[test]
fn test_stream_registry_feof_after_short_read_matches_php() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("php://memory", "w+");
fwrite($stream, "x");
rewind($stream);

var_dump(fread($stream, 2));
var_dump(feof($stream));
fclose($stream);
"#,
    );
    assert_eq!(out, "string(1) \"x\"\nbool(true)\n");
}

/// Verifies 300 simultaneous memory streams retain independent EOF flags without OS FDs.
#[test]
fn test_stream_registry_eof_isolated_across_300_live_memory_streams() {
    let out = compile_and_run(
        r#"<?php
$streams = [];
$errors = 0;

for ($i = 0; $i < 300; $i++) {
    $stream = fopen("php://memory", "w+");
    fwrite($stream, "x");
    rewind($stream);
    $streams[] = $stream;
}

for ($i = 0; $i < 300; $i++) {
    if ($i % 2 === 0) {
        fread($streams[$i], 1);
        fread($streams[$i], 1);
        if (!feof($streams[$i])) {
            $errors++;
        }
    } elseif (feof($streams[$i])) {
        $errors++;
    }
}

echo count($streams), " ", $errors, "\n";
foreach ($streams as $stream) {
    fclose($stream);
}
"#,
    );
    assert_eq!(out, "300 0\n");
}

/// Verifies reused backend storage cannot leak URI or wrapper metadata to a fresh stream.
#[test]
fn test_stream_registry_metadata_isolated_after_descriptor_reuse() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("first.txt", "x");

$first = fopen("first.txt", "r");
$firstMeta = stream_get_meta_data($first);
echo $firstMeta["wrapper_type"], "|", $firstMeta["uri"], "\n";
fclose($first);

$second = fopen("php://memory", "w+");
$secondMeta = stream_get_meta_data($second);
echo $secondMeta["wrapper_type"], "|", $secondMeta["uri"], "\n";
fclose($second);

unlink("first.txt");
"#,
    );
    assert_eq!(out, "plainfile|first.txt\nPHP|php://memory\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies metadata remains exact for 300 simultaneous non-FD memory streams.
#[test]
fn test_stream_registry_metadata_correct_beyond_256_live_streams() {
    let out = compile_and_run(
        r#"<?php
$streams = [];

for ($i = 0; $i < 300; $i++) {
    $stream = fopen("php://memory", "w+");
    $streams[] = $stream;
}

echo count($streams), "\n";
$meta = stream_get_meta_data($streams[0]);
echo $meta["wrapper_type"], "|", $meta["uri"], "\n";
$meta = stream_get_meta_data($streams[255]);
echo $meta["wrapper_type"], "|", $meta["uri"], "\n";
$meta = stream_get_meta_data($streams[256]);
echo $meta["wrapper_type"], "|", $meta["uri"], "\n";
$meta = stream_get_meta_data($streams[299]);
echo $meta["wrapper_type"], "|", $meta["uri"], "\n";
foreach ($streams as $stream) {
    fclose($stream);
}
"#,
    );
    assert_eq!(
        out,
        "300\nPHP|php://memory\nPHP|php://memory\nPHP|php://memory\nPHP|php://memory\n"
    );
}

/// Verifies `stream_is_local` distinguishes a hermetic URL wrapper from a local stream.
#[test]
fn test_stream_registry_stream_is_local_distinguishes_url_wrapper() {
    let out = compile_and_run(
        r#"<?php
class RemoteProbe {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_read($count): string {
        return "";
    }

    public function stream_eof(): bool {
        return true;
    }
}

stream_wrapper_register("remoteprobe", "RemoteProbe", STREAM_IS_URL);
$remote = fopen("remoteprobe://x", "r");
$local = fopen("php://memory", "r+");
var_dump(stream_is_local($remote));
var_dump(stream_is_local($local));
fclose($remote);
fclose($local);
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\n");
}

/// Verifies a short socket read does not report EOF until the peer closes.
#[test]
fn test_stream_registry_socket_short_read_waits_for_peer_close_before_eof() {
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, STREAM_IPPROTO_IP);
fwrite($pair[0], "x");
var_dump(fread($pair[1], 2));
var_dump(feof($pair[1]));
fclose($pair[0]);
var_dump(fread($pair[1], 2));
var_dump(feof($pair[1]));
fclose($pair[1]);
"#,
    );
    assert_eq!(
        out,
        "string(1) \"x\"\nbool(false)\nstring(0) \"\"\nbool(true)\n"
    );
}

/// Verifies stream metadata owns a dynamic URI after its source strings are released.
#[test]
fn test_stream_registry_metadata_owns_dynamic_uri_after_source_release() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$prefix = $argc > 0 ? "dynamic" : "unused";
$path = $prefix . "-metadata.txt";
file_put_contents($path, "x");

$stream = fopen($path, "r");
$prefix = str_repeat("p", 4096);
$path = str_repeat("q", 4096);
unset($prefix, $path);

$meta = stream_get_meta_data($stream);
echo $meta["wrapper_type"], "|", $meta["uri"], "\n";
fclose($stream);
unlink("dynamic-metadata.txt");
"#,
    );
    assert_eq!(out, "plainfile|dynamic-metadata.txt\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies simultaneous streams keep distinct metadata after one peer is closed.
#[test]
fn test_stream_registry_metadata_isolated_for_distinct_uris_after_peer_close() {
    let out = compile_and_run(
        r#"<?php
$first = fopen("php://memory", "w+");
$second = fopen("php://temp", "w+");

$firstMeta = stream_get_meta_data($first);
$secondBefore = stream_get_meta_data($second);
echo $firstMeta["wrapper_type"], "|", $firstMeta["uri"], "\n";
echo $secondBefore["wrapper_type"], "|", $secondBefore["uri"], "\n";

fclose($first);
$secondAfter = stream_get_meta_data($second);
echo $secondAfter["wrapper_type"], "|", $secondAfter["uri"], "\n";
fclose($second);
"#,
    );
    assert_eq!(
        out,
        "PHP|php://memory\nPHP|php://temp\nPHP|php://temp\n"
    );
}

/// Verifies a user wrapper exposes PHP's canonical metadata values.
#[test]
fn test_stream_registry_user_wrapper_metadata_matches_php() {
    let out = compile_and_run(
        r#"<?php
class MetadataProbe {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_eof(): bool {
        return true;
    }
}

var_dump(stream_wrapper_register("metaprobe", "MetadataProbe"));
$stream = fopen("metaprobe://bucket/item?x=1", "r");
$meta = stream_get_meta_data($stream);
echo $meta["wrapper_type"], "|", $meta["uri"], "\n";
fclose($stream);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nuser-space|metaprobe://bucket/item?x=1\n"
    );
}

/// Verifies closing a user wrapper invalidates every resource alias without crashing.
#[test]
fn test_stream_registry_user_wrapper_close_invalidates_alias() {
    let out = compile_and_run(
        r#"<?php
class ClosedAliasProbe {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_eof(): bool {
        return true;
    }
}

var_dump(stream_wrapper_register("closedaliasprobe", "ClosedAliasProbe"));
$stream = fopen("closedaliasprobe://x", "r");
$alias = $stream;
fclose($stream);
var_dump(is_resource($alias));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\n");
}

/// Verifies persistent standard streams expose their canonical wrapper and URI metadata.
#[test]
fn test_stream_registry_standard_stream_metadata_matches_php() {
    let out = compile_and_run(
        r#"<?php
foreach ([STDIN, STDOUT, STDERR] as $stream) {
    $meta = stream_get_meta_data($stream);
    echo $meta["wrapper_type"], "|", $meta["uri"], "\n";
}
"#,
    );
    assert_eq!(
        out,
        "PHP|php://stdin\nPHP|php://stdout\nPHP|php://stderr\n"
    );
}

/// Verifies a two-argument `stream_socket_accept()` leaves nothing behind.
///
/// The runtime renders the peer address into owned storage on every accept, because it cannot see
/// whether the caller passed `&$peer_name`. Only the three-argument lowering reads that storage, so
/// the common server-loop form leaked one block per connection — invisible to every functional
/// test, and unbounded in a program that accepts for a living.
#[test]
fn test_accept_without_peer_name_releases_the_rendered_peer() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$s = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($s, false);
for ($i = 0; $i < 4; $i++) {
    $c = stream_socket_client("tcp://" . $addr);
    $a = stream_socket_accept($s);
    fclose($a);
    fclose($c);
}
fclose($s);
echo "accepted";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "accepted");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "four accepts must leave no owned peer storage behind: {}",
        out.stderr
    );
}

/// Verifies `stream_socket_pair()` leaves nothing behind, however its result is used.
///
/// The helper creates the result array itself, so the array arrives holding a reference; boxing it
/// as Mixed took a second one, and nobody dropped the first. Releasing the box then freed only the
/// box, and the array plus both element cells stayed live — three blocks per call, growing with
/// every pair, and invisible to every functional test. Discarding the result, closing both ends and
/// unsetting the variable all leaked identically, because the fault is in constructing the result
/// rather than in releasing it.
#[test]
fn test_socket_pair_result_releases_its_creator_reference() {
    // The ends are bound to variables before use on purpose: passing a Mixed-array element
    // straight to a call leaks the element cell, which is a separate defect in the general call
    // path and would mask what this fixture measures.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 3; $i++) {
    $pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, STREAM_IPPROTO_IP);
    $writer = $pair[0];
    $reader = $pair[1];
    fwrite($writer, "x");
    echo fread($reader, 1);
    fclose($writer);
    fclose($reader);
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "xxx");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "three socket pairs must leave no owned storage behind: {}",
        out.stderr
    );
}
