//! Purpose:
//! TDD regressions for directory, glob-directory, and process-pipe backend registry state.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_stream_backend_registry_` once this module is wired.
//!
//! Key details:
//! - Expected output is pinned to the local PHP 8.5.6 oracle.
//! - The 300-live-directory case requires a process file-descriptor limit above 300; the oracle host satisfies it.
//! - Exact child reaping is not directly observable, so repeated `fclose()` plus alias invalidation are used as lifecycle evidence.

use crate::support::*;

/// Verifies more than 256 directory backends remain independently usable and closable.
#[test]
fn test_stream_backend_registry_supports_more_than_256_live_directories() {
    let out = compile_and_run(
        r#"<?php
mkdir("many-directories");
file_put_contents("many-directories/marker.txt", "x");
$directories = [];

for ($i = 0; $i < 300; $i++) {
    $directory = opendir("many-directories");
    if ($directory === false) {
        break;
    }
    $directories[] = $directory;
}

$readErrors = 0;
foreach ($directories as $directory) {
    $found = false;
    rewinddir($directory);
    while (($entry = readdir($directory)) !== false) {
        if ($entry === "marker.txt") {
            $found = true;
        }
    }
    if (!$found) {
        $readErrors++;
    }
}

$closed = 0;
foreach ($directories as $directory) {
    closedir($directory);
    $closed++;
}

echo count($directories), " ", $readErrors, " ", $closed, "\n";
unlink("many-directories/marker.txt");
rmdir("many-directories");
"#,
    );
    assert_eq!(out, "300 0 300\n");
}

/// Verifies a fresh directory backend does not inherit EOF state from a closed predecessor.
#[test]
fn test_stream_backend_registry_descriptor_reuse_resets_directory_state() {
    let out = compile_and_run(
        r#"<?php
mkdir("directory-reuse");
mkdir("directory-reuse/first");
mkdir("directory-reuse/second");
file_put_contents("directory-reuse/first/old.txt", "x");
file_put_contents("directory-reuse/second/fresh.txt", "x");

$first = opendir("directory-reuse/first");
$stale = $first;
while (readdir($first) !== false) {
}
closedir($first);

$second = opendir("directory-reuse/second");
$foundFresh = false;
while (($entry = readdir($second)) !== false) {
    if ($entry === "fresh.txt") {
        $foundFresh = true;
    }
}

var_dump(is_resource($stale));
var_dump($foundFresh);
closedir($second);

unlink("directory-reuse/first/old.txt");
unlink("directory-reuse/second/fresh.txt");
rmdir("directory-reuse/first");
rmdir("directory-reuse/second");
rmdir("directory-reuse");
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\n");
}

/// Verifies more than 256 synthetic `glob://` directory streams remain usable and closable.
#[test]
fn test_stream_backend_registry_supports_more_than_256_live_glob_streams() {
    let out = compile_and_run(
        r#"<?php
mkdir("many-globs");
file_put_contents("many-globs/marker.txt", "x");
$directories = [];

for ($i = 0; $i < 300; $i++) {
    $directory = opendir("glob://many-globs/*.txt");
    if ($directory === false) {
        break;
    }
    $directories[] = $directory;
}

$readErrors = 0;
foreach ($directories as $directory) {
    if (readdir($directory) === false) {
        $readErrors++;
    }
}

$closed = 0;
foreach ($directories as $directory) {
    closedir($directory);
    $closed++;
}

echo count($directories), " ", $readErrors, " ", $closed, "\n";
unlink("many-globs/marker.txt");
rmdir("many-globs");
"#,
    );
    assert_eq!(out, "300 0 300\n");
}

/// Verifies request cleanup releases an abandoned glob iterator and its synthetic descriptor.
///
/// `readdir()` over `glob://` answers the entry NAME, not the pattern's whole match — MEASURED on
/// `php -n` 8.5.6, `opendir("glob://gd/*.txt")` reads back `a.txt` where `glob("gd/*.txt")`
/// answers `gd/a.txt`. A directory handle answers names, whichever wrapper opened it.
#[test]
fn test_stream_backend_registry_abandoned_glob_closes_at_scope_exit() {
    let out = compile_and_run(
        r#"<?php
mkdir("abandoned-glob");
file_put_contents("abandoned-glob/marker.txt", "x");
$directory = opendir("glob://abandoned-glob/*.txt");
echo readdir($directory), "\n";
unlink("abandoned-glob/marker.txt");
rmdir("abandoned-glob");
"#,
    );
    assert_eq!(out, "marker.txt\n");
}

/// Verifies `pclose()` returns the child process exit status after draining its output.
#[test]
fn test_stream_backend_registry_pclose_returns_child_status() {
    let out = compile_and_run(
        r#"<?php
$pipe = popen("printf child; exit 7", "r");
echo stream_get_contents($pipe), " ", pclose($pipe), "\n";
"#,
    );
    assert_eq!(out, "child 7\n");
}

/// Verifies PHP's permissive `pclose()` path closes an ordinary stream and returns zero.
#[test]
fn test_stream_backend_registry_pclose_closes_non_process_stream() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("/dev/null", "r");
var_dump(pclose($stream));
"#,
    );
    assert_eq!(out, "int(0)\n");
}

/// Verifies repeated `fclose()` calls on process pipes release their backend slots.
#[test]
fn test_stream_backend_registry_fclose_releases_popen_backend() {
    let out = compile_and_run(
        r#"<?php
$closed = 0;
for ($i = 0; $i < 300; $i++) {
    $pipe = popen(":", "r");
    if ($pipe === false) {
        break;
    }
    if (fclose($pipe)) {
        $closed++;
    }
}
echo $i, " ", $closed, "\n";
"#,
    );
    assert_eq!(out, "300 300\n");
}

/// Verifies closing a process pipe invalidates aliases and rejects a second close.
#[test]
fn test_stream_backend_registry_popen_alias_closes_exactly_once() {
    let out = compile_and_run(
        r#"<?php
$pipe = popen("printf alias", "r");
$alias = $pipe;
echo stream_get_contents($alias), "\n";
var_dump(fclose($pipe));
var_dump(is_resource($alias));
try {
    pclose($alias);
} catch (TypeError $error) {
    echo get_class($error), "\n";
}
"#,
    );
    assert_eq!(out, "alias\nbool(true)\nbool(false)\nTypeError\n");
}

/// Verifies closing a directory invalidates an alias stored in a COW container.
#[test]
fn test_stream_backend_registry_directory_container_alias_is_closed() {
    let out = compile_and_run(
        r#"<?php
mkdir("directory-alias");
$directory = opendir("directory-alias");
$aliases = [$directory];
closedir($directory);

var_dump(is_resource($aliases[0]));
try {
    readdir($aliases[0]);
} catch (TypeError $error) {
    echo get_class($error), "\n";
}
rmdir("directory-alias");
"#,
    );
    assert_eq!(out, "bool(false)\nTypeError\n");
}
