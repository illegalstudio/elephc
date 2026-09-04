//! Purpose:
//! TDD regressions for user-defined stream-wrapper definitions, live handles, and cleanup.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_user_wrapper_registry_` once this module is wired.
//!
//! Key details:
//! - Expected output is pinned to the local PHP 8.5.6 oracle.
//! - Static wrapper classes deliberately isolate registry capacity from dynamic `eval()` support.
//! - CLI shutdown models one PHP request; persistent web-worker reset needs a separate web harness.

use crate::support::*;

/// Verifies the wrapper-definition table holds more than 64 simultaneous schemes.
#[test]
fn test_user_wrapper_registry_supports_more_than_64_registered_schemes() {
    let out = compile_and_run(
        r#"<?php
class ManySchemeDefinition {
}

$registered = 0;
for ($i = 0; $i < 80; $i++) {
    $scheme = "manyscheme" . $i;
    if (stream_wrapper_register($scheme, "ManySchemeDefinition")) {
        $registered++;
    }
}

$wrappers = stream_get_wrappers();
$missing = 0;
for ($i = 0; $i < 80; $i++) {
    if (!in_array("manyscheme" . $i, $wrappers, true)) {
        $missing++;
    }
}

$unregistered = 0;
for ($i = 0; $i < 80; $i++) {
    if (stream_wrapper_unregister("manyscheme" . $i)) {
        $unregistered++;
    }
}

echo $registered, " ", $missing, " ", $unregistered, "\n";
"#,
    );
    assert_eq!(out, "80 0 80\n");
}

/// Verifies more than 256 user-wrapper streams stay live and close exactly once.
#[test]
fn test_user_wrapper_registry_supports_more_than_256_live_streams() {
    let out = compile_and_run(
        r#"<?php
class ManyLiveStream {
    public static $closed = 0;
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_close(): void {
        self::$closed++;
    }
}

stream_wrapper_register("manylive", "ManyLiveStream");
$streams = [];
for ($i = 0; $i < 300; $i++) {
    $stream = fopen("manylive://" . $i, "r");
    if ($stream === false) {
        break;
    }
    $streams[] = $stream;
}

$live = 0;
foreach ($streams as $stream) {
    if (is_resource($stream)) {
        $live++;
    }
}
foreach ($streams as $stream) {
    fclose($stream);
}

echo count($streams), " ", $live, " ", ManyLiveStream::$closed, "\n";
stream_wrapper_unregister("manylive");
"#,
    );
    assert_eq!(out, "300 300 300\n");
}

/// Verifies more than 256 user-wrapper directories retain independent objects and close once.
#[test]
fn test_user_wrapper_registry_supports_more_than_256_live_directories() {
    let out = compile_and_run(
        r#"<?php
class ManyLiveDirectory {
    public static $closed = 0;
    public $context;

    public function dir_opendir($path, $options): bool {
        return true;
    }

    public function dir_readdir(): string {
        return "entry";
    }

    public function dir_closedir(): bool {
        self::$closed++;
        return true;
    }
}

stream_wrapper_register("manydir", "ManyLiveDirectory");
$directories = [];
for ($i = 0; $i < 300; $i++) {
    $directory = opendir("manydir://" . $i);
    if ($directory === false) {
        break;
    }
    $directories[] = $directory;
}

$readErrors = 0;
foreach ($directories as $directory) {
    if (readdir($directory) !== "entry") {
        $readErrors++;
    }
}
foreach ($directories as $directory) {
    closedir($directory);
}

echo count($directories), " ", $readErrors, " ", ManyLiveDirectory::$closed, "\n";
stream_wrapper_unregister("manydir");
"#,
    );
    assert_eq!(out, "300 0 300\n");
}

/// Verifies unregistering and re-registering a scheme preserves each live handle's class.
#[test]
fn test_user_wrapper_registry_reregister_keeps_live_handle_state_isolated() {
    let out = compile_and_run(
        r#"<?php
class FirstCycleWrapper {
    public $context;
    public $eof = false;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        $this->eof = false;
        return true;
    }

    public function stream_read($count): string {
        $this->eof = true;
        return "first";
    }

    public function stream_eof(): bool {
        return $this->eof;
    }
}

class SecondCycleWrapper {
    public $context;
    public $eof = false;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        $this->eof = false;
        return true;
    }

    public function stream_read($count): string {
        $this->eof = true;
        return "second";
    }

    public function stream_eof(): bool {
        return $this->eof;
    }
}

var_dump(stream_wrapper_register("cycle", "FirstCycleWrapper"));
$old = fopen("cycle://old", "r");
var_dump(stream_wrapper_unregister("cycle"));
var_dump(stream_wrapper_register("cycle", "SecondCycleWrapper"));
$fresh = fopen("cycle://fresh", "r");
echo fread($old, 16), "|", fread($fresh, 16), "\n";
fclose($old);
fclose($fresh);
stream_wrapper_unregister("cycle");
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nfirst|second\n"
    );
}

/// Verifies restoring an unregistered built-in wrapper reinstates native file dispatch.
#[test]
fn test_user_wrapper_registry_restore_reinstates_builtin_wrapper() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("wrapper-restore.txt", "restored");
var_dump(stream_wrapper_unregister("file"));
var_dump(stream_wrapper_restore("file"));
$stream = fopen("wrapper-restore.txt", "r");
echo stream_get_contents($stream), "\n";
fclose($stream);
unlink("wrapper-restore.txt");
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nrestored\n");
}

/// Verifies a COW-container alias keeps a wrapper alive until the final owner is released.
#[test]
fn test_user_wrapper_registry_container_alias_closes_exactly_once() {
    let out = compile_and_run(
        r#"<?php
class ExactCloseWrapper {
    public static $closed = 0;
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_close(): void {
        self::$closed++;
    }
}

stream_wrapper_register("exactclose", "ExactCloseWrapper");
$stream = fopen("exactclose://x", "r");
$aliases = [$stream];
unset($stream);
echo ExactCloseWrapper::$closed, "|";
unset($aliases[0]);
echo ExactCloseWrapper::$closed, "\n";
"#,
    );
    assert_eq!(out, "0|1\n");
}

/// Verifies an abandoned user-wrapper resource closes during CLI request shutdown.
#[test]
fn test_user_wrapper_registry_abandoned_stream_closes_at_request_shutdown() {
    let out = compile_and_run(
        r#"<?php
class ShutdownCloseWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_close(): void {
        echo "closed\n";
    }
}

stream_wrapper_register("shutdownclose", "ShutdownCloseWrapper");
$stream = fopen("shutdownclose://x", "r");
echo "body\n";
"#,
    );
    assert_eq!(out, "body\nclosed\n");
}
